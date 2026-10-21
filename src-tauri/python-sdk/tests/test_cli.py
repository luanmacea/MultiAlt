"""Tests of ``python -m multialt run``."""

from __future__ import annotations

import argparse
import contextlib
import io
import json
import sys
import tempfile
import threading
import time
import unittest
from pathlib import Path
from unittest import mock

from multialt import cli

HELLO = {
    "event": "hello",
    "run": {"id": "t1", "script": "case.py", "mode": "one_at_a_time", "trigger": {"kind": "manual"}, "source": "terminal"},
    "accounts": [{"user_id": 5, "name": "Main_01", "alias": "", "group": "Default", "in_game": True, "place_id": 0, "job_id": ""}],
}


class ParsingTests(unittest.TestCase):
    def test_trigger_forms(self) -> None:
        self.assertEqual(cli.parse_trigger("manual"), {"kind": "manual"})
        self.assertEqual(cli.parse_trigger("loop"), {"kind": "loop"})
        self.assertEqual(cli.parse_trigger("interval:5"), {"kind": "interval", "minutes": 5})
        for bad in ("interval:0", "interval:x", "every5", ""):
            with self.assertRaises(argparse.ArgumentTypeError):
                cli.parse_trigger(bad)

    def test_account_flag_repeats_and_takes_several_names(self) -> None:
        args = cli.build_parser().parse_args(
            ["run", "farm.py", "--account", "A", "B", "--account", "C", "--mode", "all_together", "--trigger", "loop"]
        )
        self.assertEqual([n for g in args.account for n in g], ["A", "B", "C"])
        self.assertEqual((args.mode, args.trigger), ("all_together", {"kind": "loop"}))

    def test_scripts_resolve_by_path_or_by_name_in_scripts(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "scripts").mkdir()
            (root / "scripts" / "farm.py").write_text("", encoding="utf-8")
            with mock.patch.object(cli, "WORKSPACE_DIR", root), mock.patch.object(cli, "SCRIPTS_DIR", root / "scripts"):
                self.assertEqual(cli.resolve_script("farm.py"), (root / "scripts" / "farm.py").resolve())
                self.assertEqual(cli.resolve_script("scripts/farm.py"), (root / "scripts" / "farm.py").resolve())
                self.assertIsNone(cli.resolve_script("missing.py"))


class MessageTests(unittest.TestCase):
    def run_cli(self, pipe_file: Path, script_name: str = "case.py") -> tuple[int, str]:
        err = io.StringIO()
        with tempfile.TemporaryDirectory() as tmp:
            script = Path(tmp) / script_name
            script.write_text("def main(account):\n    pass\n", encoding="utf-8")
            with mock.patch.object(cli, "PIPE_FILE", pipe_file), contextlib.redirect_stderr(err):
                code = cli.main(["run", str(script), "--account", "Main_01"])
        return code, err.getvalue()

    def test_missing_pipe_file_says_developer_mode_is_off(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            code, err = self.run_cli(Path(tmp) / "pipe.txt")
        self.assertEqual(code, 2)
        self.assertIn(
            "Developer mode is off. Turn it on in MultiAlt > Scripts > Roblox Player automation > Developer.", err
        )

    def test_stale_pipe_file_says_the_app_is_not_reachable(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            pipe_file = Path(tmp) / "pipe.txt"
            pipe_file.write_text("multialt-python-does-not-exist", encoding="utf-8")
            code, err = self.run_cli(pipe_file)
        self.assertEqual(code, 2)
        self.assertIn("Could not connect to MultiAlt", err)

    def test_error_lines_from_the_app_are_readable(self) -> None:
        self.assertEqual(
            cli._error_text({"ok": False, "error": {"code": "bad_args", "message": "Unknown account: X", "hint": "Check the name."}}),
            "Unknown account: X Check the name.",
        )
        self.assertEqual(cli._error_text({"error": "Unknown account: X"}), "Unknown account: X")


@unittest.skipUnless(sys.platform == "win32", "named pipes are Windows only")
class EndToEndTests(unittest.TestCase):
    def serve(self, name: str, reply_first: dict, received: list) -> threading.Thread:
        from multiprocessing.connection import Listener

        listener = Listener(r"\\.\pipe" + "\\" + name, family="AF_PIPE")

        def run() -> None:
            conn = listener.accept()
            first = json.loads(conn.recv_bytes())
            received.append(first)
            conn.send_bytes(json.dumps(reply_first).encode() + b"\n")
            try:
                while True:
                    request = json.loads(conn.recv_bytes())
                    received.append(request)
                    if request.get("id") is not None:
                        conn.send_bytes(json.dumps({"id": request["id"], "ok": True, "result": True}).encode() + b"\n")
            except (EOFError, OSError):
                pass
            finally:
                listener.close()

        thread = threading.Thread(target=run, daemon=True)
        thread.start()
        time.sleep(0.2)
        return thread

    def run_cli(self, name: str, source: str) -> tuple[int, str, str]:
        out, err = io.StringIO(), io.StringIO()
        with tempfile.TemporaryDirectory() as tmp:
            script = Path(tmp) / "case.py"
            script.write_text(source, encoding="utf-8")
            pipe_file = Path(tmp) / "pipe.txt"
            pipe_file.write_text(name, encoding="utf-8")
            with mock.patch.object(cli, "PIPE_FILE", pipe_file), contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
                code = cli.main(["run", str(script), "--account", "Main_01", "--mode", "all_together", "--trigger", "interval:5"])
        return code, out.getvalue(), err.getvalue()

    def test_a_terminal_run_logs_to_the_app_and_the_terminal(self) -> None:
        received: list = []
        name = f"multialt-cli-test-{time.time_ns()}"
        self.serve(name, HELLO, received)
        code, out, err = self.run_cli(name, "import multialt\ndef main(account):\n    print('hello from', account.name)\n    account.log('tagged')\n")
        self.assertEqual(code, 0, err)
        self.assertEqual(
            received[0],
            {"op": "cli.start", "args": {"script": "case.py", "accounts": ["Main_01"], "mode": "all_together", "trigger": {"kind": "interval", "minutes": 5}}},
        )
        logs = [r["args"]["message"] for r in received if r.get("op") == "run.log"]
        self.assertIn("hello from Main_01", logs)
        self.assertIn("[Main_01] hello from Main_01", out)
        self.assertIn("[Main_01] tagged", out)

    def test_an_unknown_account_error_line_is_reported_and_exits_2(self) -> None:
        received: list = []
        name = f"multialt-cli-test-{time.time_ns()}"
        self.serve(name, {"ok": False, "error": {"code": "bad_args", "message": "Unknown account: Main_01"}}, received)
        code, _out, err = self.run_cli(name, "def main(account):\n    pass\n")
        self.assertEqual(code, 2)
        self.assertIn("Unknown account: Main_01", err)

    def test_a_failing_script_shows_the_traceback_and_exits_1(self) -> None:
        received: list = []
        name = f"multialt-cli-test-{time.time_ns()}"
        self.serve(name, HELLO, received)
        code, out, err = self.run_cli(name, "def main(account):\n    raise ValueError('kaboom')\n")
        self.assertEqual(code, 1)
        self.assertIn("ValueError: kaboom", err)  # the account's traceback, tagged, in the terminal
        self.assertIn("AccountsFailed", err)


if __name__ == "__main__":
    unittest.main()
