"""Tests of the default blocks that need no screen: runner, focus, actions, dashboard."""

from __future__ import annotations

import threading
import types
import unittest

import multialt
from multialt import Account
from multialt._core.protocol import FocusTimeout, NotForeground, Stopped
from multialt.defaults import focus, runner
from tests.fake_transport import FakeTransport


def make_accounts(*names: str) -> list[Account]:
    return [Account(user_id=100 + i, name=name) for i, name in enumerate(names)]


def make_ctx(mode: str, accounts: list[Account]) -> types.SimpleNamespace:
    return types.SimpleNamespace(mode=mode, accounts=accounts, trigger={"kind": "manual"}, source="app")


class RunnerTests(unittest.TestCase):
    def setUp(self) -> None:
        self.fake = FakeTransport().install()

    def tearDown(self) -> None:
        self.fake.uninstall()

    def test_one_at_a_time_runs_accounts_in_order(self) -> None:
        order: list[str] = []
        module = types.SimpleNamespace(main=lambda account: order.append(account.name))
        runner.run(module, make_ctx("one_at_a_time", make_accounts("A", "B", "C")))
        self.assertEqual(order, ["A", "B", "C"])

    def test_one_at_a_time_finishes_an_account_before_the_next(self) -> None:
        events: list[str] = []

        def main(account: Account) -> None:
            events.append(f"{account.name}:start")
            events.append(f"{account.name}:end")

        runner.run(types.SimpleNamespace(main=main), make_ctx("one_at_a_time", make_accounts("A", "B")))
        self.assertEqual(events, ["A:start", "A:end", "B:start", "B:end"])

    def test_all_together_really_runs_at_the_same_time(self) -> None:
        barrier = threading.Barrier(3, timeout=5)  # only passes if all three are alive together
        done: list[str] = []

        def main(account: Account) -> None:
            barrier.wait()
            done.append(account.name)

        runner.run(types.SimpleNamespace(main=main), make_ctx("all_together", make_accounts("A", "B", "C")))
        self.assertEqual(sorted(done), ["A", "B", "C"])

    def test_a_failure_in_one_account_does_not_stop_the_others(self) -> None:
        done: list[str] = []

        def main(account: Account) -> None:
            if account.name == "B":
                raise ValueError("boom")
            done.append(account.name)

        for mode in ("all_together", "one_at_a_time"):
            done.clear()
            self.fake.notes.clear()
            with self.assertRaises(runner.AccountsFailed) as caught:
                runner.run(types.SimpleNamespace(main=main), make_ctx(mode, make_accounts("A", "B", "C")))
            self.assertEqual(sorted(done), ["A", "C"], mode)
            self.assertIn("B", str(caught.exception))
            # The traceback is logged with the failing account's id.
            errors = [l for l in self.fake.logs() if l["level"] == "error"]
            self.assertEqual(len(errors), 1, mode)
            self.assertEqual(errors[0]["user_id"], 101)
            self.assertIn("ValueError: boom", errors[0]["message"])

    def test_main_all_takes_precedence_and_runs_once(self) -> None:
        seen: list[list[str]] = []
        per_account: list[str] = []
        module = types.SimpleNamespace(
            main_all=lambda accounts: seen.append([a.name for a in accounts]),
            main=lambda account: per_account.append(account.name),
        )
        runner.run(module, make_ctx("all_together", make_accounts("A", "B")))
        self.assertEqual(seen, [["A", "B"]])
        self.assertEqual(per_account, [])

    def test_a_script_without_main_gets_a_clear_error(self) -> None:
        with self.assertRaises(RuntimeError) as caught:
            runner.run(types.SimpleNamespace(), make_ctx("one_at_a_time", make_accounts("A")))
        self.assertIn("main(account)", str(caught.exception))

    def test_stopped_inside_main_ends_quietly(self) -> None:
        def main(account: Account) -> None:
            raise Stopped("Run stopped.")

        runner.run(types.SimpleNamespace(main=main), make_ctx("one_at_a_time", make_accounts("A", "B")))
        runner.run(types.SimpleNamespace(main=main), make_ctx("all_together", make_accounts("A", "B")))

    def test_no_more_accounts_start_after_a_stop(self) -> None:
        order: list[str] = []

        def main(account: Account) -> None:
            order.append(account.name)
            self.fake.request_stop()

        runner.run(types.SimpleNamespace(main=main), make_ctx("one_at_a_time", make_accounts("A", "B")))
        self.assertEqual(order, ["A"])

    def test_report_true_feeds_the_dashboard(self) -> None:
        module = types.SimpleNamespace(main=lambda account: None, REPORT=True)
        runner.run(module, make_ctx("one_at_a_time", make_accounts("A")))
        define = [a for op, a in self.fake.calls if op == "dashboard.define"][0]
        self.assertEqual([w["id"] for w in define["layout"]], ["accounts", "total_loops", "loops_per_hour"])
        rows = [a for op, a in self.fake.calls if op == "dashboard.set" and a["id"] == "accounts"]
        self.assertEqual(rows[-1]["user_id"], 100)
        self.assertEqual(rows[-1]["value"]["Status"], "Done")
        self.assertEqual(rows[-1]["value"]["Loops"], 1)
        totals = [a["value"] for op, a in self.fake.calls if op == "dashboard.set" and a["id"] == "total_loops"]
        self.assertEqual(totals[-1], 1)
        self.assertIn("dashboard.push", self.fake.ops())

    def test_report_records_the_error_of_a_failed_account(self) -> None:
        def main(account: Account) -> None:
            raise RuntimeError("no button")

        module = types.SimpleNamespace(main=main, REPORT=True)
        with self.assertRaises(runner.AccountsFailed):
            runner.run(module, make_ctx("one_at_a_time", make_accounts("A")))
        rows = [a for op, a in self.fake.calls if op == "dashboard.set" and a["id"] == "accounts"]
        self.assertEqual(rows[-1]["value"]["Status"], "Error")
        self.assertEqual(rows[-1]["value"]["Last error"], "RuntimeError: no button")


class FocusTests(unittest.TestCase):
    def setUp(self) -> None:
        self.fake = FakeTransport().install()
        self.account = Account(user_id=7, name="Main_01")

    def tearDown(self) -> None:
        self.fake.uninstall()

    def test_focused_runs_acquire_bring_body_release_restore(self) -> None:
        with self.account.focused():
            self.fake.calls.append(("<body>", {}))
        self.assertEqual(
            self.fake.ops(),
            ["focus.acquire", "window.bring_forward", "<body>", "focus.release", "window.restore_focus"],
        )
        self.assertEqual(self.fake.calls[0][1], {"user_id": 7, "timeout_ms": 20000})
        self.assertEqual(self.fake.calls[3][1], {"lease": "L1"})

    def test_the_lease_is_released_even_when_the_body_fails(self) -> None:
        with self.assertRaises(NotForeground):
            with self.account.focused():
                raise NotForeground("Blocked.", "")
        self.assertIn("focus.release", self.fake.ops())
        self.assertEqual(self.fake.ops()[-1], "window.restore_focus")

    def test_the_lease_is_released_after_a_stop(self) -> None:
        with self.assertRaises(Stopped):
            with self.account.focused():
                self.fake.request_stop()
                self.fake.call("run.sleep", {"ms": 1})  # raises Stopped
        self.assertIn("focus.release", self.fake.ops())

    def test_a_busy_queue_raises_focus_timeout_and_holds_nothing(self) -> None:
        def busy(args: dict) -> None:
            raise FocusTimeout("Waited 20000 ms for the focus queue.", "Raise timeout_ms.")

        self.fake.handlers["focus.acquire"] = busy
        with self.assertRaises(FocusTimeout):
            with self.account.focused():
                self.fail("must not enter the body")
        self.assertNotIn("focus.release", self.fake.ops())
        # and the thread's stack is clean: the next one can take it
        self.fake.handlers["focus.acquire"] = lambda a: {"lease": "L2"}
        with self.account.focused():
            pass

    def test_nested_focused_for_the_same_account_reuses_the_lease(self) -> None:
        with self.account.focused():
            with self.account.focused():
                pass
        self.assertEqual(self.fake.ops().count("focus.acquire"), 1)
        self.assertEqual(self.fake.ops().count("focus.release"), 1)

    def test_nested_focused_for_another_account_is_refused(self) -> None:
        other = Account(user_id=8, name="Alt_02")
        with self.account.focused():
            with self.assertRaises(RuntimeError):
                with other.focused():
                    pass

    def test_timeout_is_passed_to_the_queue(self) -> None:
        with self.account.focused(timeout_ms=500):
            pass
        self.assertEqual(self.fake.calls[0][1]["timeout_ms"], 500)


class ActionTests(unittest.TestCase):
    def setUp(self) -> None:
        self.fake = FakeTransport({"screen.pixel": lambda a: [250, 198, 3]}).install()
        self.account = Account(user_id=7, name="Main_01")

    def tearDown(self) -> None:
        self.fake.uninstall()

    def test_account_methods_reach_the_contract_ops(self) -> None:
        self.account.press("space", 100)
        self.account.key_down("W")
        self.account.key_up("W")
        self.account.type_text("hi", 10)
        self.account.click(0.5, 0.25)
        self.assertEqual(
            self.fake.calls,
            [
                ("input.key", {"user_id": 7, "key": "space", "hold_ms": 100}),
                ("input.key_down", {"user_id": 7, "key": "W"}),
                ("input.key_up", {"user_id": 7, "key": "W"}),
                ("input.type_text", {"user_id": 7, "text": "hi", "delay_ms": 10}),
                ("input.click", {"user_id": 7, "x": 0.5, "y": 0.25}),
            ],
        )

    def test_wait_pixel_matches_within_the_tolerance(self) -> None:
        self.assertTrue(self.account.wait_pixel(0.5, 0.8, (255, 200, 0), tolerance=12, timeout=1))
        self.assertEqual(self.account.pixel(0.5, 0.8), (250, 198, 3))

    def test_wait_pixel_times_out_with_false(self) -> None:
        self.assertFalse(self.account.wait_pixel(0.5, 0.8, (0, 0, 0), tolerance=5, timeout=0))

    def test_jitter_sleep_stays_inside_the_spread(self) -> None:
        from multialt.defaults import actions

        for _ in range(20):
            actions.jitter_sleep(1000, 200)
        slept = [a["ms"] for op, a in self.fake.calls if op == "run.sleep"]
        self.assertTrue(all(800 <= ms <= 1200 for ms in slept))
        actions.jitter_sleep(10, 500)
        self.assertGreaterEqual(self.fake.calls[-1][1]["ms"], 0)

    def test_sleep_is_milliseconds_in_the_app(self) -> None:
        multialt.sleep(1.5)
        self.assertEqual(self.fake.calls[-1], ("run.sleep", {"ms": 1500}))

    def test_status_and_log_are_tagged_with_the_account(self) -> None:
        self.account.status("farming")
        self.account.log("a", 1)
        self.assertEqual(
            self.fake.notes,
            [
                ("run.status", {"text": "farming", "user_id": 7}),
                ("run.log", {"level": "log", "message": "a 1", "user_id": 7}),
            ],
        )

    def test_stopped_reflects_the_stop_flag(self) -> None:
        self.assertFalse(multialt.stopped())
        self.fake.request_stop()
        self.assertTrue(multialt.stopped())


class AccountTests(unittest.TestCase):
    def test_account_is_built_from_the_contract_json_without_secrets(self) -> None:
        account = Account.from_json(
            {"user_id": 123, "name": "Main_01", "alias": "", "group": "Default", "in_game": True, "place_id": 9, "job_id": "abc"}
        )
        self.assertEqual((account.user_id, account.name, account.group, account.in_game), (123, "Main_01", "Default", True))
        self.assertFalse(any("cookie" in f.lower() or "password" in f.lower() for f in account.__dataclass_fields__))


if __name__ == "__main__":
    unittest.main()
