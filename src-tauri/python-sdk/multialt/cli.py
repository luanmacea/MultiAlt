"""Run a script from a terminal: ``python -m multialt run <script> --account NAME``.

App-owned (rewritten on every start). Do not edit.

Needs developer mode (MultiAlt > Scripts > Roblox Player automation >
Developer): the app then opens a private channel (a named pipe only your
Windows user can use) and writes its name to ``multialt/_core/pipe.txt``. The
run shows up in the app's Running list as "terminal", and its log appears both
there and in your terminal. This is how an AI tool such as Claude can write a
script, run it, read the error and fix it.

Examples::

    python -m multialt run farm.py --account Main_01
    python -m multialt run scripts/farm.py --account Main_01 --account Alt_02 --mode all_together
    python -m multialt run farm.py --account Main_01 --trigger interval:5

Exit code: the script's (0 = finished or stopped, 1 = failed), 2 = could not start.
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path
from typing import Any, Optional

from multialt._core import PIPE_FILE, SCRIPTS_DIR, WORKSPACE_DIR, host, protocol

DEVELOPER_MODE_OFF = (
    "Developer mode is off. Turn it on in MultiAlt > Scripts > Roblox Player automation > Developer."
)


def parse_trigger(text: str) -> dict[str, Any]:
    """``manual``, ``loop`` or ``interval:<minutes>`` -> the trigger object of the protocol."""
    value = text.strip().lower()
    if value in ("manual", "loop"):
        return {"kind": value}
    if value.startswith("interval:"):
        try:
            minutes = int(value.split(":", 1)[1])
        except ValueError:
            minutes = 0
        if minutes >= 1:
            return {"kind": "interval", "minutes": minutes}
    raise argparse.ArgumentTypeError("trigger must be manual, loop or interval:<minutes> (for example interval:5)")


def resolve_script(text: str) -> Optional[Path]:
    """Find the script: as given, inside the workspace, or by name in scripts/."""
    given = Path(text)
    for candidate in (given, WORKSPACE_DIR / text, SCRIPTS_DIR / given.name):
        if candidate.is_file():
            return candidate.resolve()
    return None


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(prog="python -m multialt", description="Run MultiAlt Python scripts.")
    commands = parser.add_subparsers(dest="command", required=True)
    run = commands.add_parser("run", help="run a script against the open Roblox windows of some accounts")
    run.add_argument("script", help="script name or path, for example farm.py or scripts/farm.py")
    run.add_argument(
        "--account",
        action="append",
        nargs="+",
        required=True,
        metavar="NAME",
        help="account name as shown in MultiAlt (repeat the flag or list several names)",
    )
    run.add_argument("--mode", choices=("one_at_a_time", "all_together"), default="one_at_a_time")
    run.add_argument("--trigger", type=parse_trigger, default={"kind": "manual"}, metavar="manual|loop|interval:<min>")
    return parser


def _fail(message: str) -> int:
    print(f"multialt: {message}", file=sys.stderr)
    return 2


def _error_text(reply: dict[str, Any]) -> str:
    """The reason in an error line from the app, whatever shape it has."""
    error = reply.get("error")
    if isinstance(error, dict):
        text = str(error.get("message") or error.get("code") or "error")
        hint = error.get("hint")
        return f"{text} {hint}" if hint else text
    for key in ("error", "message"):
        if reply.get(key):
            return str(reply[key])
    return "the app refused to start the run"


def main(argv: Optional[list[str]] = None) -> int:
    args = build_parser().parse_args(argv)
    accounts = [name for group in args.account for name in group]

    script = resolve_script(args.script)
    if script is None:
        return _fail(f"script not found: {args.script} (looked here and in {SCRIPTS_DIR})")

    try:
        pipe_name = PIPE_FILE.read_text(encoding="utf-8").strip()
    except OSError:
        pipe_name = ""
    if not pipe_name:
        return _fail(DEVELOPER_MODE_OFF)

    try:
        transport = protocol.pipe_transport(pipe_name)
    except (OSError, ImportError):
        return _fail(
            "Could not connect to MultiAlt. Is the app open? "
            "(Developer mode must be on: MultiAlt > Scripts > Roblox Player automation > Developer.)"
        )

    transport.send(
        {
            "op": "cli.start",
            "args": {"script": script.name, "accounts": accounts, "mode": args.mode, "trigger": args.trigger},
        }
    )
    reply = transport.read_message()
    if reply is None:
        return _fail("MultiAlt closed the connection before starting the run.")
    if reply.get("event") != "hello":
        return _fail(_error_text(reply))

    return host.run_script(script, transport, reply, echo=True, swap_stdin=False)
