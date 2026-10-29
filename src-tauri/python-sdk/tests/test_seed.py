"""The example MultiAlt seeds once (seed/): it runs against a fake app, and the
"from lib.helpers import ..." line works in the workspace layout."""

from __future__ import annotations

import os
import shutil
import subprocess
import sys
import tempfile
import types
import unittest
from pathlib import Path

from multialt import Account
from multialt._core import host
from multialt._core.protocol import Stopped
from multialt.defaults import dashboard as dashboard_block
from multialt.defaults import runner
from tests.fake_transport import FakeTransport

SDK = Path(__file__).resolve().parents[1]
SEED = SDK / "seed"
EXAMPLE = SEED / "scripts" / "example_walk_jump.py"
NO_WINDOW = 0x08000000 if os.name == "nt" else 0  # CREATE_NO_WINDOW


def _forget_lib() -> None:
    for name in [n for n in sys.modules if n == "lib" or n.startswith("lib.")]:
        del sys.modules[name]


class SeedExampleTests(unittest.TestCase):
    def test_the_example_walks_jumps_opens_and_closes_the_menu_until_stopped(self) -> None:
        # One account, so the key order is deterministic (all_together would
        # interleave two threads).
        accounts = [Account(user_id=1, name="A")]
        ctx = types.SimpleNamespace(mode="one_at_a_time", accounts=accounts, trigger={"kind": "manual"}, source="app")
        fake = FakeTransport()
        sleeps = {"n": 0}

        def sleep(args: dict) -> bool:
            sleeps["n"] += 1
            if sleeps["n"] >= 7:
                fake.request_stop()  # the user pressed Stop, in the third loop
            return True

        fake.handlers["run.sleep"] = sleep
        fake.install()
        sys.path.insert(0, str(SEED))  # the workspace root, where lib/ lives
        _forget_lib()
        try:
            module = host.load_user_module(EXAMPLE)
            self.assertTrue(module.REPORT)
            try:
                runner.run(module, ctx)  # raises on any error that is not Stopped
            except Stopped:
                pass
        finally:
            sys.path.remove(str(SEED))
            _forget_lib()
            fake.uninstall()

        keys = [args["key"] for op, args in fake.calls if op == "input.key"]
        # Per loop: walk (W), jump (Space), open the menu (Escape), close it (Escape).
        self.assertEqual(keys, ["W", "Space", "Escape", "Escape"] * 2 + ["W", "Space"])
        walk = next(args for op, args in fake.calls if op == "input.key" and args["key"] == "W")
        self.assertEqual(walk["hold_ms"], 1500)
        # Every key went inside a focus lease (with account.focused():).
        self.assertIn("focus.acquire", fake.ops())
        # The farm report: widgets, status, loop count and loops per hour.
        defined = [args for op, args in fake.calls if op == "dashboard.define"]
        self.assertTrue(defined)
        ids = {w["id"] for w in defined[0]["layout"]}
        self.assertTrue({"accounts", "total_loops", "loops_per_hour"} <= ids)
        pushed = [args for op, args in fake.calls + fake.notes if op == "dashboard.push"]
        self.assertEqual(sum(1 for a in pushed if a["id"] == "loops_per_hour"), 2, pushed)
        self.assertTrue(dashboard_block.COLUMNS)

    def test_lib_imports_work_in_the_workspace_layout(self) -> None:
        # The app's layout: <workspace>/multialt, <workspace>/lib, <workspace>/scripts,
        # and the host puts <workspace> first on sys.path. Checked with and
        # without lib/__init__.py (the user may delete it), in a clean interpreter.
        for keep_init in (True, False):
            with self.subTest(keep_init=keep_init), tempfile.TemporaryDirectory() as tmp:
                ws = Path(tmp) / "python"
                shutil.copytree(SDK / "multialt", ws / "multialt")
                shutil.copytree(SEED / "lib", ws / "lib")
                shutil.copytree(SEED / "scripts", ws / "scripts")
                if not keep_init:
                    (ws / "lib" / "__init__.py").unlink()
                code = (
                    "import sys; sys.path.insert(0, sys.argv[1]);"
                    "from lib.helpers import walk_forward;"
                    "import multialt.defaults.actions as a;"
                    "print(walk_forward.__name__, a.jitter_sleep.__name__)"
                )
                result = subprocess.run(
                    [sys.executable, "-I", "-c", code, str(ws)],
                    cwd=tmp,
                    capture_output=True,
                    text=True,
                    timeout=60,
                    creationflags=NO_WINDOW,
                )
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(result.stdout.strip(), "walk_forward jitter_sleep")


if __name__ == "__main__":
    unittest.main()
