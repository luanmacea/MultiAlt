"""Test doubles for the SDK tests (repo-only, not embedded in the app)."""

from __future__ import annotations

import json
import os
import threading
from typing import Any, Callable, Optional

from multialt._core import protocol
from multialt._core.protocol import CLEANUP_OPS, MultiAltError, Stopped, Transport

Handler = Callable[[dict[str, Any]], Any]


def default_handlers() -> dict[str, Handler]:
    """Plausible answers for every op, so a test only overrides what it cares about."""
    return {
        "accounts": lambda a: [],
        "windows": lambda a: [],
        "focus.acquire": lambda a: {"lease": "L1"},
        "focus.renew": lambda a: True,
        "focus.release": lambda a: True,
        "window.bring_forward": lambda a: True,
        "window.restore_focus": lambda a: "restored",
        "input.key": lambda a: True,
        "input.key_down": lambda a: True,
        "input.key_up": lambda a: True,
        "input.type_text": lambda a: True,
        "input.click": lambda a: True,
        "screen.pixel": lambda a: [0, 0, 0],
        "run.sleep": lambda a: True,
        "run.stopped": lambda a: False,
        "discord.send": lambda a: True,
        "dashboard.define": lambda a: True,
        "dashboard.set": lambda a: True,
        "dashboard.push": lambda a: True,
        "dashboard.clear": lambda a: True,
    }


class FakeTransport:
    """Same surface as ``Transport`` for the module-level helpers, no I/O.

    ``calls`` records ``(op, args)`` for requests, ``notes`` the fire-and-forget
    ones. A handler may return a value or raise a ``MultiAltError``.
    """

    def __init__(self, handlers: Optional[dict[str, Handler]] = None) -> None:
        self.handlers = default_handlers()
        if handlers:
            self.handlers.update(handlers)
        self.calls: list[tuple[str, dict[str, Any]]] = []
        self.notes: list[tuple[str, dict[str, Any]]] = []
        self._lock = threading.Lock()
        self._stop = threading.Event()

    # -- Transport surface ---------------------------------------------

    @property
    def stop_requested(self) -> bool:
        return self._stop.is_set()

    def request_stop(self) -> None:
        self._stop.set()

    def call(self, op: str, args: Optional[dict[str, Any]] = None) -> Any:
        if op not in CLEANUP_OPS and self._stop.is_set():
            raise Stopped("Run stopped.")
        args = args or {}
        with self._lock:
            self.calls.append((op, args))
        handler = self.handlers.get(op)
        if handler is None:
            raise MultiAltError(f"no handler for {op}")
        return handler(args)

    def notify(self, op: str, args: Optional[dict[str, Any]] = None) -> None:
        with self._lock:
            self.notes.append((op, args or {}))

    # -- helpers for assertions ----------------------------------------

    def ops(self) -> list[str]:
        return [op for op, _ in self.calls]

    def logs(self) -> list[dict[str, Any]]:
        return [args for op, args in self.notes if op == "run.log"]

    def install(self) -> "FakeTransport":
        protocol.set_transport(self)  # type: ignore[arg-type]
        return self

    def uninstall(self) -> None:
        protocol.set_transport(None)


class LoopbackApp:
    """A real ``Transport`` wired through OS pipes to a thread that plays the app.

    ``respond(request) -> response dict | None`` is called for each request
    (``None`` = no answer). ``requests`` keeps every request line received.
    """

    def __init__(self, respond: Callable[[dict[str, Any]], Optional[dict[str, Any]]]) -> None:
        to_app_r, to_app_w = os.pipe()
        to_py_r, to_py_w = os.pipe()
        self._app_in = os.fdopen(to_app_r, "rb")
        self._py_out = os.fdopen(to_app_w, "wb", buffering=0)
        self._py_in = os.fdopen(to_py_r, "rb")
        self._app_out = os.fdopen(to_py_w, "wb", buffering=0)
        self.transport = Transport(self._py_in, self._py_out)
        self.requests: list[dict[str, Any]] = []
        self._respond = respond
        self._thread = threading.Thread(target=self._serve, daemon=True)
        self._thread.start()

    def _serve(self) -> None:
        while True:
            line = self._app_in.readline()
            if not line:
                return
            request = json.loads(line)
            self.requests.append(request)
            response = self._respond(request)
            if response is not None:
                self.send_line(response)

    def send_line(self, message: dict[str, Any] | str) -> None:
        data = message if isinstance(message, str) else json.dumps(message)
        self._app_out.write(data.encode("utf-8") + b"\n")

    def ok(self, request: dict[str, Any], result: Any = True) -> dict[str, Any]:
        return {"id": request["id"], "ok": True, "result": result}

    def close(self) -> None:
        self._app_out.close()  # the script side sees end of stream
        self._py_out.close()
        self._thread.join(2)  # both readers see end of stream, then it is safe to close them
        if self.transport._thread is not None:
            self.transport._thread.join(2)
        self._app_in.close()
        self._py_in.close()
