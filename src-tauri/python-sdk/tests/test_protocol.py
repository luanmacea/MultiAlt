"""Protocol tests: ids, errors, log capture, stop, the real host process, the pipe."""

from __future__ import annotations

import json
import subprocess
import sys
import tempfile
import threading
import time
import unittest
from pathlib import Path

from multialt._core import host, primitives, protocol
from multialt._core.protocol import (
    BadArgs,
    CaptureFailed,
    FocusTimeout,
    Forbidden,
    Minimized,
    MultiAltError,
    NoFocusLease,
    NotForeground,
    Stopped,
    UnsupportedKey,
    UntrackedWindow,
    error_from_payload,
)
from tests.fake_transport import FakeTransport, LoopbackApp

SDK_ROOT = Path(__file__).resolve().parents[1]


def _error(code: str, message: str = "m", hint: str = "h") -> dict:
    return {"code": code, "message": message, "hint": hint}


class RequestResponseTests(unittest.TestCase):
    def test_response_ids_match_their_requests_even_out_of_order(self) -> None:
        held: list[dict] = []
        gate = threading.Event()

        def respond(request: dict):
            if request["op"] == "slow":
                held.append(request)
                gate.wait(2)
                return {"id": request["id"], "ok": True, "result": "slow-result"}
            return {"id": request["id"], "ok": True, "result": "fast-result"}

        app = LoopbackApp(respond)
        app.transport.start()
        results: dict[str, object] = {}
        slow = threading.Thread(target=lambda: results.update(slow=app.transport.call("slow")))
        slow.start()
        time.sleep(0.2)  # the slow request is in flight and unanswered
        results["fast"] = app.transport.call("fast")  # must not wait for the slow one
        gate.set()
        slow.join(3)
        self.assertEqual(results, {"fast": "fast-result", "slow": "slow-result"})
        ids = [r["id"] for r in app.requests]
        self.assertEqual(len(set(ids)), 2)
        app.close()

    def test_error_codes_map_to_their_subclasses(self) -> None:
        cases = {
            "not_foreground": NotForeground,
            "no_focus_lease": NoFocusLease,
            "untracked_window": UntrackedWindow,
            "minimized": Minimized,
            "forbidden": Forbidden,
            "focus_timeout": FocusTimeout,
            "unsupported_key": UnsupportedKey,
            "bad_args": BadArgs,
            "capture_failed": CaptureFailed,
            "stopped": Stopped,
        }
        for code, cls in cases.items():
            error = error_from_payload(_error(code, "Message text.", "Do this."))
            self.assertIs(type(error), cls, code)
            self.assertEqual(error.code, code)
            self.assertEqual(error.message, "Message text.")
            self.assertEqual(error.hint, "Do this.")
            self.assertIsInstance(error, MultiAltError)
        self.assertIn("Do this.", str(error_from_payload(_error("not_foreground", "M.", "Do this."))))

    def test_unknown_code_is_a_plain_multialt_error(self) -> None:
        error = error_from_payload(_error("something_new"))
        self.assertIs(type(error), MultiAltError)
        self.assertEqual(error.code, "something_new")

    def test_error_response_raises_the_subclass(self) -> None:
        app = LoopbackApp(lambda r: {"id": r["id"], "ok": False, "error": _error("not_foreground", "Blocked.", "Wrap it.")})
        app.transport.start()
        with self.assertRaises(NotForeground) as caught:
            app.transport.call("input.key", {"user_id": 1, "key": "W", "hold_ms": 40})
        self.assertEqual(caught.exception.hint, "Wrap it.")
        app.close()

    def test_notify_sends_a_null_id_and_expects_no_response(self) -> None:
        app = LoopbackApp(lambda r: None)
        app.transport.start()
        app.transport.notify("run.log", {"level": "log", "message": "hi"})
        deadline = time.time() + 2
        while not app.requests and time.time() < deadline:
            time.sleep(0.01)
        self.assertEqual(app.requests[0]["id"], None)
        self.assertEqual(app.requests[0]["op"], "run.log")
        app.close()

    def test_malformed_line_is_ignored_not_fatal(self) -> None:
        app = LoopbackApp(lambda r: None)
        app.transport.start()

        def answer_later() -> None:
            time.sleep(0.2)
            app.send_line("this is not json {")
            app.send_line(json.dumps({"id": app.requests[0]["id"], "ok": True, "result": 5}))

        threading.Thread(target=answer_later).start()
        self.assertEqual(app.transport.call("anything"), 5)
        app.close()


class StopTests(unittest.TestCase):
    def test_stop_event_raises_stopped_on_the_next_call(self) -> None:
        app = LoopbackApp(lambda r: {"id": r["id"], "ok": True, "result": True})
        app.transport.start()
        self.assertTrue(app.transport.call("accounts"))
        app.send_line({"event": "stop"})
        deadline = time.time() + 2
        while not app.transport.stop_requested and time.time() < deadline:
            time.sleep(0.01)
        with self.assertRaises(Stopped):
            app.transport.call("accounts")
        app.close()

    def test_cleanup_ops_still_go_through_after_a_stop(self) -> None:
        app = LoopbackApp(lambda r: {"id": r["id"], "ok": True, "result": True})
        app.transport.start()
        app.transport.request_stop()
        self.assertTrue(app.transport.call("focus.release", {"lease": "L1"}))
        with self.assertRaises(Stopped):
            app.transport.call("input.key", {})
        app.close()

    def test_losing_the_connection_unblocks_a_waiting_call(self) -> None:
        app = LoopbackApp(lambda r: None)  # never answers
        app.transport.start()
        threading.Timer(0.2, app._app_out.close).start()
        with self.assertRaises(Stopped):
            app.transport.call("run.sleep", {"ms": 10000})


class LogCaptureTests(unittest.TestCase):
    def setUp(self) -> None:
        self.fake = FakeTransport().install()

    def tearDown(self) -> None:
        self.fake.uninstall()

    def test_print_becomes_run_log_lines(self) -> None:
        writer = host.LogWriter("log")
        print("hello", "world", file=writer)
        writer.write("partial")
        writer.flush()
        self.assertEqual([l["message"] for l in self.fake.logs()], ["hello world", "partial"])
        self.assertTrue(all(l["level"] == "log" for l in self.fake.logs()))

    def test_stderr_writer_uses_the_error_level(self) -> None:
        writer = host.LogWriter("error")
        writer.write("boom\n")
        self.assertEqual(self.fake.logs()[0]["level"], "error")

    def test_log_lines_carry_the_current_account(self) -> None:
        protocol.set_current_user(42)
        try:
            host.LogWriter("log").write("from account\n")
        finally:
            protocol.set_current_user(None)
        self.assertEqual(self.fake.logs()[0]["user_id"], 42)


class PrimitiveWrapperTests(unittest.TestCase):
    def setUp(self) -> None:
        self.fake = FakeTransport(
            {"screen.pixel": lambda a: [255, 200, 0], "focus.acquire": lambda a: {"lease": "L9"}}
        ).install()

    def tearDown(self) -> None:
        self.fake.uninstall()

    def test_wrappers_send_the_contract_names_and_args(self) -> None:
        self.assertEqual(primitives.screen_pixel(7, 0.5, 0.8), (255, 200, 0))
        self.assertEqual(primitives.focus_acquire(7, 1500), "L9")
        primitives.input_key(7, "W", 100)
        primitives.input_click(7, 0.1, 0.2)
        primitives.run_sleep(250)
        self.assertEqual(
            self.fake.calls,
            [
                ("screen.pixel", {"user_id": 7, "x": 0.5, "y": 0.8}),
                ("focus.acquire", {"user_id": 7, "timeout_ms": 1500}),
                ("input.key", {"user_id": 7, "key": "W", "hold_ms": 100}),
                ("input.click", {"user_id": 7, "x": 0.1, "y": 0.2}),
                ("run.sleep", {"ms": 250}),
            ],
        )

    def test_status_and_log_are_fire_and_forget(self) -> None:
        primitives.run_status("working", 7)
        primitives.run_log("warn", "careful")
        self.assertEqual(self.fake.calls, [])
        self.assertEqual(
            self.fake.notes,
            [("run.status", {"text": "working", "user_id": 7}), ("run.log", {"level": "warn", "message": "careful"})],
        )


class HostProcessTests(unittest.TestCase):
    """Runs the real ``python -m multialt._core.host`` and plays the app over its pipes."""

    def _run(self, source: str, script_name: str = "case.py", stop_after_first_request: bool = False):
        with tempfile.TemporaryDirectory() as tmp:
            script = Path(tmp) / script_name
            script.write_text(source, encoding="utf-8")
            process = subprocess.Popen(
                [sys.executable, "-u", "-X", "utf8", "-m", "multialt._core.host", str(script)],
                cwd=str(SDK_ROOT),
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
            )
            hello = {
                "event": "hello",
                "run": {"id": "r1", "script": script_name, "mode": "one_at_a_time", "trigger": {"kind": "manual"}, "source": "app"},
                "accounts": [
                    {"user_id": 1, "name": "Main_01", "alias": "", "group": "Default", "in_game": True, "place_id": 0, "job_id": ""}
                ],
            }
            process.stdin.write((json.dumps(hello) + "\n").encode())
            process.stdin.flush()
            lines: list[dict] = []
            for raw in iter(process.stdout.readline, b""):
                message = json.loads(raw)  # a stray print would break this
                lines.append(message)
                if message.get("id") is not None:
                    response = {"id": message["id"], "ok": True, "result": True}
                    if message["op"] == "accounts":
                        response["result"] = hello["accounts"]
                    process.stdin.write((json.dumps(response) + "\n").encode())
                    process.stdin.flush()
                if stop_after_first_request and message.get("id") is not None:
                    process.stdin.write(b'{"event":"stop"}\n')
                    process.stdin.flush()
            process.stdin.close()
            stderr = process.stderr.read().decode("utf-8", "replace")
            process.wait(10)
            process.stdout.close()
            process.stderr.close()
            return process.returncode, lines, stderr

    def test_print_never_corrupts_the_protocol(self) -> None:
        code, lines, stderr = self._run(
            "import multialt\n"
            "def main(account):\n"
            "    print('plain print', account.name)\n"
            "    multialt.log('via log')\n"
            "    print('to stderr', file=__import__('sys').stderr)\n"
        )
        self.assertEqual(code, 0, stderr)
        logs = [(l["args"]["level"], l["args"]["message"]) for l in lines if l.get("op") == "run.log"]
        self.assertIn(("log", "plain print Main_01"), logs)
        self.assertIn(("log", "via log"), logs)
        self.assertIn(("error", "to stderr"), logs)

    def test_an_uncaught_error_prints_a_traceback_and_exits_1(self) -> None:
        code, _lines, stderr = self._run("def main(account):\n    raise ValueError('kaboom')\n")
        # The runner keeps going for other accounts and then reports AccountsFailed.
        self.assertEqual(code, 1)
        self.assertIn("AccountsFailed", stderr)

    def test_a_syntax_error_in_the_script_exits_1_with_the_traceback(self) -> None:
        code, _lines, stderr = self._run("def main(:\n")
        self.assertEqual(code, 1)
        self.assertIn("SyntaxError", stderr)

    def test_stop_ends_the_script_quietly(self) -> None:
        code, _lines, stderr = self._run(
            "import multialt\n"
            "def main(account):\n"
            "    while True:\n"
            "        multialt.sleep(0.01)\n",
            stop_after_first_request=True,
        )
        self.assertEqual(code, 0, stderr)
        self.assertNotIn("Traceback", stderr)


@unittest.skipUnless(sys.platform == "win32", "named pipes are Windows only")
class NamedPipeTests(unittest.TestCase):
    def test_a_waiting_read_does_not_block_writes(self) -> None:
        """Regression: a plain open(..., 'r+b') pipe deadlocks here; the overlapped client must not."""
        from multiprocessing.connection import Listener

        name = r"\\.\pipe\multialt-test-" + str(time.time_ns())
        listener = Listener(name, family="AF_PIPE")
        received: list[dict] = []

        def server() -> None:
            conn = listener.accept()
            request = json.loads(conn.recv_bytes())
            received.append(request)
            conn.send_bytes(json.dumps({"id": request["id"], "ok": True, "result": "pong"}).encode() + b"\n")
            conn.send_bytes(b'{"event":"stop"}\n')

        thread = threading.Thread(target=server, daemon=True)
        thread.start()
        transport = protocol.pipe_transport(name)
        transport.start()  # the reader thread is now blocked waiting for the app
        time.sleep(0.3)
        self.assertEqual(transport.call("ping"), "pong")  # the write must go through
        deadline = time.time() + 2
        while not transport.stop_requested and time.time() < deadline:
            time.sleep(0.01)
        self.assertTrue(transport.stop_requested)
        self.assertEqual(received[0]["op"], "ping")
        transport.close()
        listener.close()

    def test_pipe_path_accepts_a_bare_name_or_a_full_path(self) -> None:
        self.assertEqual(protocol.pipe_path("multialt-python-ab12"), "\\\\.\\pipe\\multialt-python-ab12")
        self.assertEqual(protocol.pipe_path("\\\\.\\pipe\\x"), "\\\\.\\pipe\\x")


if __name__ == "__main__":
    unittest.main()
