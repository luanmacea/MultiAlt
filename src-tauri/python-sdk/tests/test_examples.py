"""Every example and template must compile and run against a fake app."""

from __future__ import annotations

import types
import unittest
from pathlib import Path

from multialt import Account
from multialt._core import host
from multialt._core.protocol import Stopped
from multialt.defaults import runner
from tests.fake_transport import FakeTransport

ROOT = Path(__file__).resolve().parents[1]
FILES = sorted((ROOT / "examples").glob("*.py")) + sorted((ROOT / "templates").glob("*.py"))
# Needs a real picture and OpenCV; compiled only.
COMPILE_ONLY = {"click_button_image.py"}


class ExampleTests(unittest.TestCase):
    def test_there_are_the_four_examples_and_four_templates(self) -> None:
        names = {p.name for p in FILES}
        self.assertTrue(
            {
                "anti_afk_pixel.py",
                "click_button_image.py",
                "routine_all_accounts.py",
                "farm_dashboard.py",
                "blank.py",
                "per_account.py",
                "controller.py",
                "dashboard.py",
            }
            <= names
        )

    def test_each_file_runs_until_stopped_without_errors(self) -> None:
        accounts = [Account(user_id=1, name="A"), Account(user_id=2, name="B")]
        ctx = types.SimpleNamespace(mode="all_together", accounts=accounts, trigger={"kind": "manual"}, source="app")
        for path in FILES:
            with self.subTest(path.name):
                source = path.read_text(encoding="utf-8")
                compile(source, str(path), "exec")
                if path.name in COMPILE_ONLY:
                    continue
                sleeps = {"n": 0}
                fake = FakeTransport()

                def sleep(args: dict) -> bool:
                    sleeps["n"] += 1
                    if sleeps["n"] >= 6:
                        fake.request_stop()  # the user pressed Stop
                    return True

                fake.handlers["run.sleep"] = sleep
                fake.install()
                try:
                    module = host.load_user_module(path)
                    try:
                        runner.run(module, ctx)  # raises on any error that is not Stopped
                    except Stopped:
                        pass  # main_all scripts let Stop escape; the host ends them quietly
                finally:
                    fake.uninstall()
                self.assertTrue(
                    any(op.startswith("input.") for op in fake.ops()) or path.name == "blank.py",
                    f"{path.name} never sent input",
                )


if __name__ == "__main__":
    unittest.main()
