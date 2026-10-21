"""JSON-lines transport between a script and MultiAlt.

App-owned (rewritten on every start). Do not edit.

One JSON object per line, UTF-8:

* script -> app: ``{"id": 7, "op": "screen.pixel", "args": {...}}``
  (``"id": null`` means fire-and-forget, no response, used by ``run.log``).
* app -> script: ``{"id": 7, "ok": true, "result": ...}`` or
  ``{"id": 7, "ok": false, "error": {"code", "message", "hint"}}``.
* app -> script events: ``{"event": "hello", ...}`` (first line) and
  ``{"event": "stop"}``.
"""

from __future__ import annotations

import json
import os
import sys
import threading
import time
from typing import Any, Callable, Optional

__all__ = [
    "MultiAltError",
    "NotForeground",
    "NoFocusLease",
    "UntrackedWindow",
    "Minimized",
    "Forbidden",
    "FocusTimeout",
    "UnsupportedKey",
    "BadArgs",
    "CaptureFailed",
    "Stopped",
    "Transport",
    "error_from_payload",
]

# ---------------------------------------------------------------------------
# Errors
# ---------------------------------------------------------------------------


class MultiAltError(Exception):
    """Base class of every error MultiAlt reports to a script.

    Attributes:
        code: the stable error code (for example ``"not_foreground"``).
        message: what happened.
        hint: what to change, or an empty string.
    """

    code: str = "error"

    def __init__(self, message: str, hint: str = "", code: Optional[str] = None) -> None:
        super().__init__(message)
        self.message = message
        self.hint = hint or ""
        if code is not None:
            self.code = code

    def __str__(self) -> str:
        if self.hint:
            return f"{self.message} Hint: {self.hint}"
        return self.message


class NotForeground(MultiAltError):
    code = "not_foreground"


class NoFocusLease(MultiAltError):
    code = "no_focus_lease"


class UntrackedWindow(MultiAltError):
    code = "untracked_window"


class Minimized(MultiAltError):
    code = "minimized"


class Forbidden(MultiAltError):
    code = "forbidden"


class FocusTimeout(MultiAltError):
    code = "focus_timeout"


class UnsupportedKey(MultiAltError):
    code = "unsupported_key"


class BadArgs(MultiAltError):
    code = "bad_args"


class CaptureFailed(MultiAltError):
    code = "capture_failed"


class Stopped(MultiAltError):
    """The run was stopped (Stop button, shake-to-stop, app closing).

    The host ends the script quietly when this escapes ``main``. Do not
    swallow it: after a stop every later call raises it again.
    """

    code = "stopped"


_ERROR_CLASSES: dict[str, type[MultiAltError]] = {
    cls.code: cls
    for cls in (
        NotForeground,
        NoFocusLease,
        UntrackedWindow,
        Minimized,
        Forbidden,
        FocusTimeout,
        UnsupportedKey,
        BadArgs,
        CaptureFailed,
        Stopped,
    )
}


def error_from_payload(payload: Any) -> MultiAltError:
    """Turn the ``error`` object of a response into the matching exception."""
    if not isinstance(payload, dict):
        return MultiAltError(str(payload))
    code = str(payload.get("code") or "error")
    message = str(payload.get("message") or code)
    hint = str(payload.get("hint") or "")
    cls = _ERROR_CLASSES.get(code)
    if cls is None:
        return MultiAltError(message, hint, code=code)
    return cls(message, hint)


# ---------------------------------------------------------------------------
# Transport
# ---------------------------------------------------------------------------

# Calls that still go through after a stop, so cleanup code (``finally:``
# blocks, ``with account.focused():`` exits) can release what it holds.
CLEANUP_OPS = frozenset(
    {
        "run.stopped",
        "run.log",
        "run.status",
        "focus.release",
        "input.key_up",
        "window.restore_focus",
    }
)

# After a stop, a call that is still waiting for the app gives up locally
# after this many seconds (the app normally answers within a few ms).
_STOP_GRACE_SECONDS = 5.0


class _Pending:
    __slots__ = ("event", "response")

    def __init__(self) -> None:
        self.event = threading.Event()
        self.response: Optional[dict[str, Any]] = None


class Transport:
    """Request/response over a pair of line-oriented binary streams.

    ``reader`` needs ``readline() -> bytes``; ``writer`` needs ``write(bytes)``
    (and optionally ``flush()``). Safe to use from several threads.
    """

    def __init__(self, reader: Any, writer: Any) -> None:
        self._reader = reader
        self._writer = writer
        self._write_lock = threading.Lock()
        self._id_lock = threading.Lock()
        self._next_id = 1
        self._pending: dict[int, _Pending] = {}
        self._stop = threading.Event()
        self._stop_at = 0.0
        self._closed = threading.Event()
        self._thread: Optional[threading.Thread] = None
        self._handlers: list[Callable[[dict[str, Any]], None]] = []

    # -- low level -----------------------------------------------------

    def send(self, message: dict[str, Any]) -> None:
        """Write one JSON line. Raises ``Stopped`` if the connection is gone."""
        data = (json.dumps(message, ensure_ascii=False, separators=(",", ":")) + "\n").encode("utf-8")
        try:
            with self._write_lock:
                self._writer.write(data)
                flush = getattr(self._writer, "flush", None)
                if flush is not None:
                    flush()
        except (OSError, ValueError) as exc:
            self._mark_closed()
            raise Stopped("Run stopped.") from exc

    def read_message(self) -> Optional[dict[str, Any]]:
        """Read one line synchronously (used for the hello line, before ``start``).

        Returns ``None`` at end of stream. Lines that are not JSON objects are skipped.
        """
        while True:
            line = self._reader.readline()
            if not line:
                return None
            line = line.strip()
            if not line:
                continue
            try:
                message = json.loads(line)
            except ValueError:
                continue
            if isinstance(message, dict):
                return message

    def on_event(self, handler: Callable[[dict[str, Any]], None]) -> None:
        """Register a callback for app events (``stop`` and friends)."""
        self._handlers.append(handler)

    def start(self) -> None:
        """Start the reader thread. Call once, after the hello line was read."""
        if self._thread is not None:
            return
        self._thread = threading.Thread(target=self._read_loop, name="multialt-reader", daemon=True)
        self._thread.start()

    def close(self) -> None:
        """Mark the transport closed and close the write side (best effort).

        The read side is left alone on purpose: closing a buffered stdin while the
        reader thread is blocked in ``readline`` would deadlock. The process exits
        right after the run, which drops it.
        """
        self._mark_closed()
        close = getattr(self._writer, "close", None)
        if close is not None:
            try:
                close()
            except (OSError, ValueError):
                pass

    # -- state ---------------------------------------------------------

    @property
    def stop_requested(self) -> bool:
        """True once the app asked the run to stop (or the connection dropped)."""
        return self._stop.is_set()

    def request_stop(self) -> None:
        """Mark the run as stopped (what the ``stop`` event does)."""
        if not self._stop.is_set():
            self._stop_at = time.monotonic()
            self._stop.set()

    # -- calls ---------------------------------------------------------

    def call(self, op: str, args: Optional[dict[str, Any]] = None) -> Any:
        """Send a request and wait for its response.

        Raises the ``MultiAltError`` subclass matching the error code. Raises
        ``Stopped`` when the run was stopped, except for the cleanup ops in
        ``CLEANUP_OPS``.
        """
        cleanup = op in CLEANUP_OPS
        if not cleanup and (self._stop.is_set() or self._closed.is_set()):
            raise Stopped("Run stopped.")
        if self._closed.is_set():
            raise Stopped("Run stopped.")
        with self._id_lock:
            request_id = self._next_id
            self._next_id += 1
        pending = _Pending()
        self._pending[request_id] = pending
        try:
            self.send({"id": request_id, "op": op, "args": args or {}})
            while not pending.event.wait(0.1):
                if (
                    not cleanup
                    and self._stop.is_set()
                    and time.monotonic() - self._stop_at > _STOP_GRACE_SECONDS
                ):
                    raise Stopped("Run stopped.")
        finally:
            self._pending.pop(request_id, None)
        response = pending.response
        if response is None:
            raise Stopped("Run stopped.")
        if response.get("ok"):
            return response.get("result")
        raise error_from_payload(response.get("error"))

    def notify(self, op: str, args: Optional[dict[str, Any]] = None) -> None:
        """Fire-and-forget request (``"id": null``): the app sends no response.

        Never raises: a lost connection just drops the message.
        """
        try:
            self.send({"id": None, "op": op, "args": args or {}})
        except MultiAltError:
            pass

    # -- internals -----------------------------------------------------

    def _mark_closed(self) -> None:
        self._closed.set()
        self.request_stop()
        for pending in list(self._pending.values()):
            pending.event.set()

    def _read_loop(self) -> None:
        try:
            while True:
                try:
                    line = self._reader.readline()
                except (OSError, ValueError):
                    break
                if not line:
                    break
                line = line.strip()
                if not line:
                    continue
                try:
                    message = json.loads(line)
                except ValueError:
                    # A broken line must never kill the run. Say so on the real stderr.
                    _warn("multialt: ignored a malformed line from the app")
                    continue
                if not isinstance(message, dict):
                    continue
                if "event" in message:
                    self._handle_event(message)
                elif isinstance(message.get("id"), int):
                    pending = self._pending.get(message["id"])
                    if pending is not None:
                        pending.response = message
                        pending.event.set()
        finally:
            self._mark_closed()

    def _handle_event(self, message: dict[str, Any]) -> None:
        if message.get("event") == "stop":
            self.request_stop()
        for handler in list(self._handlers):
            try:
                handler(message)
            except Exception:  # a broken handler must not stop the reader
                pass


def _warn(text: str) -> None:
    stream = getattr(sys, "__stderr__", None)
    if stream is None:
        return
    try:
        stream.write(text + "\n")
        stream.flush()
    except (OSError, ValueError):
        pass


# ---------------------------------------------------------------------------
# The transport of the current run
# ---------------------------------------------------------------------------

_active: Optional[Transport] = None
_local = threading.local()


def set_transport(transport: Optional[Transport]) -> None:
    """Set the transport every primitive uses (the host calls this once)."""
    global _active
    _active = transport


def get_transport() -> Transport:
    if _active is None:
        raise RuntimeError(
            "This script is not connected to MultiAlt. Start it with the Run button in "
            "MultiAlt > Scripts > Roblox Player automation, or with "
            "'python -m multialt run <script> --account NAME' (developer mode)."
        )
    return _active


def call(op: str, args: Optional[dict[str, Any]] = None) -> Any:
    return get_transport().call(op, args)


def notify(op: str, args: Optional[dict[str, Any]] = None) -> None:
    get_transport().notify(op, args)


def stop_requested() -> bool:
    return get_transport().stop_requested


def set_current_user(user_id: Optional[int]) -> None:
    """Tag this thread's log lines with an account (the runner does this)."""
    _local.user_id = user_id


def current_user() -> Optional[int]:
    return getattr(_local, "user_id", None)


# ---------------------------------------------------------------------------
# Streams
# ---------------------------------------------------------------------------


def stdio_transport() -> Transport:
    """The transport used when MultiAlt starts the script itself.

    Must run before the host swaps ``sys.stdout``: it duplicates the original
    stdout descriptor for the protocol, then points descriptor 1 at stderr so
    any native code that writes to stdout cannot corrupt the protocol.
    """
    stdin = sys.stdin.buffer
    out_fd = os.dup(sys.stdout.fileno())
    writer = os.fdopen(out_fd, "wb", buffering=0)
    try:
        sys.stdout.flush()
        os.dup2(sys.stderr.fileno(), sys.stdout.fileno())
    except (OSError, ValueError):
        pass
    return Transport(stdin, writer)


class _PipeFile:
    """Duplex client end of a Windows named pipe, with overlapped I/O.

    Why not ``open(r"\\\\.\\pipe\\name", "r+b", buffering=0)``: a synchronous
    handle serialises its operations, so a read waiting in the reader thread
    blocks every write from the script (verified: it deadlocks). Overlapped
    I/O lets one thread read while another writes.
    """

    def __init__(self, path: str) -> None:
        import _winapi  # Windows only

        self._w = _winapi
        self._buffer = b""
        deadline = time.monotonic() + 5.0
        while True:
            try:
                self._handle = _winapi.CreateFile(
                    path,
                    _winapi.GENERIC_READ | _winapi.GENERIC_WRITE,
                    0,
                    _winapi.NULL,
                    _winapi.OPEN_EXISTING,
                    _winapi.FILE_FLAG_OVERLAPPED,
                    _winapi.NULL,
                )
                break
            except OSError as exc:
                busy = getattr(exc, "winerror", None) == _winapi.ERROR_PIPE_BUSY
                if not busy or time.monotonic() > deadline:
                    raise
                time.sleep(0.05)
        self._closed = False

    def readline(self) -> bytes:
        while True:
            index = self._buffer.find(b"\n")
            if index >= 0:
                line, self._buffer = self._buffer[: index + 1], self._buffer[index + 1 :]
                return line
            chunk = self._read_chunk()
            if not chunk:
                rest, self._buffer = self._buffer, b""
                return rest
            self._buffer += chunk

    def _read_chunk(self) -> bytes:
        w = self._w
        if self._closed:
            return b""
        try:
            ov, err = w.ReadFile(self._handle, 65536, overlapped=True)
        except OSError:
            return b""
        try:
            _count, err = ov.GetOverlappedResult(True)
        except OSError:
            ov.cancel()
            return b""
        if err in (0, w.ERROR_MORE_DATA):
            return bytes(ov.getbuffer())
        return b""  # broken pipe: the app is gone

    def write(self, data: bytes) -> int:
        w = self._w
        if self._closed:
            raise OSError("pipe closed")
        ov, err = w.WriteFile(self._handle, data, overlapped=True)
        try:
            _count, err = ov.GetOverlappedResult(True)
        except OSError:
            ov.cancel()
            raise
        if err:
            raise OSError(f"pipe write failed ({err})")
        return len(data)

    def flush(self) -> None:
        return None

    def close(self) -> None:
        if not self._closed:
            self._closed = True
            try:
                self._w.CloseHandle(self._handle)
            except OSError:
                pass


def pipe_path(name: str) -> str:
    """Full path of a pipe from the name stored in ``_core/pipe.txt``."""
    name = name.strip()
    if name.startswith("\\\\.\\pipe\\"):
        return name
    return "\\\\.\\pipe\\" + name


def pipe_transport(name: str) -> Transport:
    """Connect to MultiAlt's developer-mode named pipe."""
    pipe = _PipeFile(pipe_path(name))
    return Transport(pipe, pipe)
