"""The host: what MultiAlt starts for each run.

App-owned (rewritten on every start). Do not edit.

    python -u -X utf8 -m multialt._core.host scripts/farm.py

Order of events (it matters):

1. duplicate the original stdout for the protocol, then swap ``sys.stdout`` and
   ``sys.stderr`` for log writers - **before** any user code is imported, so a
   ``print()`` can never corrupt the protocol;
2. read the ``hello`` line (run settings and accounts);
3. import the user's script;
4. hand control to ``multialt.defaults.runner.run(module, ctx)``.

``{"event": "stop"}`` sets a flag; the next call into MultiAlt raises
``Stopped``, which ends the script quietly. An uncaught exception prints its
full traceback to the real stderr and exits with code 1.
"""

from __future__ import annotations

import importlib
import importlib.util
import io
import os
import sys
import traceback
from pathlib import Path
from types import ModuleType
from typing import Any, Optional, TextIO

from multialt._core import WORKSPACE_DIR, primitives, protocol
from multialt._core.protocol import Stopped, Transport


class LogWriter(io.TextIOBase):
    """A file-like object that turns written text into ``run.log`` lines.

    ``print()`` goes through it, so output shows up in the app's log terminal.
    With ``echo`` (the terminal CLI) the text is also written to the real terminal.
    """

    def __init__(self, level: str, echo: Optional[TextIO] = None) -> None:
        super().__init__()
        self._level = level
        self._echo = echo
        self._buffer = ""

    def writable(self) -> bool:
        return True

    def isatty(self) -> bool:
        return False

    @property
    def encoding(self) -> str:  # type: ignore[override]
        return "utf-8"

    def write(self, text: str) -> int:
        if not isinstance(text, str):
            raise TypeError(f"write() argument must be str, not {type(text).__name__}")
        if self._echo is not None:
            try:
                self._echo.write(text)
                self._echo.flush()
            except (OSError, ValueError):
                pass
        self._buffer += text
        while "\n" in self._buffer:
            line, self._buffer = self._buffer.split("\n", 1)
            self._emit(line)
        return len(text)

    def flush(self) -> None:
        if self._buffer:
            line, self._buffer = self._buffer, ""
            self._emit(line)

    def _emit(self, line: str) -> None:
        line = line.rstrip("\r")
        if not line.strip():
            return  # blank lines would only add noise to the log
        try:
            primitives.run_log(self._level, line)
        except RuntimeError:
            pass  # no transport (tests, interpreter shutdown): nothing to log to


def load_user_module(path: Path) -> ModuleType:
    """Import the user's script by path, as the module ``__user_script__``."""
    spec = importlib.util.spec_from_file_location("__user_script__", str(path))
    if spec is None or spec.loader is None:
        raise ImportError(f"Cannot load {path}")
    module = importlib.util.module_from_spec(spec)
    sys.modules["__user_script__"] = module
    spec.loader.exec_module(module)
    return module


def _ensure_workspace_on_path() -> None:
    root = str(WORKSPACE_DIR)
    if root not in sys.path:
        sys.path.insert(0, root)  # lets scripts do "from lib.helpers import x"


def run_script(
    script: Path,
    transport: Transport,
    hello: dict[str, Any],
    *,
    echo: bool = False,
    swap_stdin: bool = True,
) -> int:
    """Run ``script`` against ``transport``. Returns the process exit code.

    ``hello`` is the first line from the app. With ``echo`` the log is also
    printed to the terminal (the ``python -m multialt run`` command).
    """
    import multialt

    real_stdout, real_stderr, real_stdin = sys.stdout, sys.stderr, sys.stdin
    protocol.set_transport(transport)
    _ensure_workspace_on_path()
    ctx = multialt._begin_run(hello)
    transport.start()

    # Step 1 of the module doc: from here on, print() is a log line.
    sys.stdout = LogWriter("log", real_stdout if echo else None)
    sys.stderr = LogWriter("error", real_stderr if echo else None)
    if swap_stdin:
        sys.stdin = io.StringIO("")  # input() must never read protocol lines

    code = 0
    try:
        module = load_user_module(script)
        runner = importlib.import_module("multialt.defaults.runner")
        runner.run(module, ctx)
    except Stopped:
        code = 0  # the user pressed Stop: not an error
    except SystemExit as exit_request:
        code = exit_request.code if isinstance(exit_request.code, int) else (0 if exit_request.code is None else 1)
    except KeyboardInterrupt:
        code = 130
    except BaseException:  # noqa: BLE001 - the host reports everything
        # The real stderr, not the log writer: the app reads the last
        # traceback line from it as the reason the run failed.
        traceback.print_exc(file=real_stderr)
        code = 1
    finally:
        for stream in (sys.stdout, sys.stderr):
            try:
                stream.flush()
            except (OSError, ValueError):
                pass
        sys.stdout, sys.stderr, sys.stdin = real_stdout, real_stderr, real_stdin
        try:
            real_stderr.flush()
        except (OSError, ValueError):
            pass
        transport.close()
        protocol.set_transport(None)
    return code


def main(argv: Optional[list[str]] = None) -> int:
    """Entry point for ``python -m multialt._core.host <script>``."""
    args = list(sys.argv[1:] if argv is None else argv)
    if len(args) != 1:
        print("usage: python -m multialt._core.host <script.py>", file=sys.stderr)
        return 2
    script = Path(args[0])
    if not script.is_absolute():
        script = Path.cwd() / script

    # Step 1: take the protocol channel before anything else can print.
    transport = protocol.stdio_transport()

    # Step 2: the hello line.
    hello = transport.read_message()
    if hello is None or hello.get("event") != "hello":
        print("multialt: the app did not send the hello line", file=sys.stderr)
        return 2
    return run_script(script, transport, hello)


if __name__ == "__main__":
    exit_code = main()
    try:
        sys.stdout.flush()
        sys.stderr.flush()
    except (OSError, ValueError):
        pass
    # The reader thread may still be blocked reading stdin; skip the normal
    # interpreter shutdown, which would wait for it.
    os._exit(exit_code)
