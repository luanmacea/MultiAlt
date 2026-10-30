"""Image search tests on synthetic frames. Skipped when numpy or OpenCV are missing."""

from __future__ import annotations

import tempfile
import unittest
from pathlib import Path
from unittest import mock

try:
    import cv2
    import numpy as np

    HAVE_CV = True
except ImportError:  # the app installs them with the Python runtime; a bare Python may not have them
    HAVE_CV = False

from multialt import Account
from multialt.defaults import vision
from tests.fake_transport import FakeTransport


def noise_pattern(width: int, height: int, seed: int) -> "np.ndarray":
    """A recognisable grayscale texture (smoothed noise, so resizing keeps its shape)."""
    rng = np.random.default_rng(seed)
    base = rng.integers(0, 256, (height // 4 + 1, width // 4 + 1), dtype=np.uint8)
    return cv2.resize(base, (width, height), interpolation=cv2.INTER_CUBIC)


def bgra(gray: "np.ndarray") -> "np.ndarray":
    return cv2.cvtColor(gray, cv2.COLOR_GRAY2BGRA)


@unittest.skipUnless(HAVE_CV, "numpy and opencv-python-headless are not installed")
class FindImageTests(unittest.TestCase):
    FRAME_W, FRAME_H = 640, 400

    def setUp(self) -> None:
        self.fake = FakeTransport().install()
        self.account = Account(user_id=7, name="Main_01")
        self.assets = tempfile.TemporaryDirectory()
        patcher = mock.patch.object(vision, "ASSETS_DIR", Path(self.assets.name))
        patcher.start()
        self.addCleanup(patcher.stop)
        vision._template_cache.clear()
        self.template = noise_pattern(80, 50, seed=3)
        cv2.imwrite(str(Path(self.assets.name) / "button.png"), self.template)

    def tearDown(self) -> None:
        self.fake.uninstall()
        self.assets.cleanup()

    def frame_with(self, scale: float, left: int, top: int) -> "np.ndarray":
        """A noisy frame with the template pasted at ``scale`` with its top-left at (left, top)."""
        frame = noise_pattern(self.FRAME_W, self.FRAME_H, seed=99)
        size = (round(self.template.shape[1] * scale), round(self.template.shape[0] * scale))
        pasted = cv2.resize(self.template, size, interpolation=cv2.INTER_AREA)
        frame[top : top + size[1], left : left + size[0]] = pasted
        return bgra(frame)

    def serve(self, frame: "np.ndarray") -> mock._patch:
        return mock.patch.object(vision.primitives, "screen_capture", return_value=frame)

    def test_finds_a_template_shown_at_90_percent_within_2_percent_of_the_centre(self) -> None:
        left, top = 300, 120
        frame = self.frame_with(0.9, left, top)
        true_cx = (left + 72 / 2) / self.FRAME_W  # 80 * 0.9 = 72 px wide
        true_cy = (top + 45 / 2) / self.FRAME_H  # 50 * 0.9 = 45 px high
        with self.serve(frame):
            found = self.account.find_image("button.png")
        self.assertIsNotNone(found)
        self.assertAlmostEqual(found[0], true_cx, delta=0.02)
        self.assertAlmostEqual(found[1], true_cy, delta=0.02)

    def test_finds_it_at_the_original_size_too(self) -> None:
        frame = self.frame_with(1.0, 100, 250)
        with self.serve(frame):
            found = self.account.find_image("button.png")
        self.assertIsNotNone(found)
        self.assertAlmostEqual(found[0], (100 + 40) / self.FRAME_W, delta=0.01)
        self.assertAlmostEqual(found[1], (250 + 25) / self.FRAME_H, delta=0.01)

    def test_returns_none_when_the_image_is_not_there(self) -> None:
        frame = bgra(noise_pattern(self.FRAME_W, self.FRAME_H, seed=99))
        with self.serve(frame):
            self.assertIsNone(self.account.find_image("button.png"))

    def test_region_results_are_fractions_of_the_whole_window(self) -> None:
        # The app returns only the region; the match must be mapped back to the full window.
        region = (0.5, 0.25, 0.5, 0.5)  # right half, middle half of the window
        crop = noise_pattern(320, 200, seed=5)
        crop[60:110, 120:200] = self.template
        with self.serve(bgra(crop)) as capture:
            found = self.account.find_image("button.png", region=region)
        self.assertIsNotNone(found)
        self.assertAlmostEqual(found[0], 0.5 + (160 / 320) * 0.5, delta=0.01)
        self.assertAlmostEqual(found[1], 0.25 + (85 / 200) * 0.5, delta=0.01)
        capture.assert_called_once_with(7, {"x": 0.5, "y": 0.25, "w": 0.5, "h": 0.5})

    def test_a_missing_asset_is_a_clear_error(self) -> None:
        with self.assertRaises(FileNotFoundError) as caught:
            self.account.find_image("nope.png")
        self.assertIn("assets", str(caught.exception))

    def test_wait_image_returns_none_after_the_timeout(self) -> None:
        frame = bgra(noise_pattern(self.FRAME_W, self.FRAME_H, seed=99))
        with self.serve(frame):
            self.assertIsNone(self.account.wait_image("button.png", timeout=0))

    def test_wait_image_finds_it_on_a_later_try(self) -> None:
        empty = bgra(noise_pattern(self.FRAME_W, self.FRAME_H, seed=99))
        shown = self.frame_with(1.0, 10, 10)
        with mock.patch.object(vision.primitives, "screen_capture", side_effect=[empty, empty, shown]):
            found = self.account.wait_image("button.png", timeout=5, interval=0)
        self.assertIsNotNone(found)

    def test_click_image_clicks_inside_a_focus_block(self) -> None:
        frame = self.frame_with(1.0, 200, 100)
        with self.serve(frame):
            self.assertTrue(self.account.click_image("button.png", timeout=1))
        ops = self.fake.ops()
        self.assertLess(ops.index("focus.acquire"), ops.index("input.click"))
        self.assertLess(ops.index("input.click"), ops.index("focus.release"))
        click = [a for op, a in self.fake.calls if op == "input.click"][0]
        self.assertAlmostEqual(click["x"], (200 + 40) / self.FRAME_W, delta=0.01)

    def test_click_image_returns_false_when_it_never_shows_up(self) -> None:
        frame = bgra(noise_pattern(self.FRAME_W, self.FRAME_H, seed=99))
        with self.serve(frame):
            self.assertFalse(self.account.click_image("button.png", timeout=0))
        self.assertNotIn("input.click", self.fake.ops())

    def test_template_bigger_than_the_frame_is_skipped_not_an_error(self) -> None:
        tiny = bgra(noise_pattern(40, 30, seed=1))
        with self.serve(tiny):
            self.assertIsNone(self.account.find_image("button.png"))


class MissingLibraryTests(unittest.TestCase):
    def test_find_image_explains_what_to_install_when_cv2_is_missing(self) -> None:
        import builtins

        real_import = builtins.__import__

        def fake_import(name, *args, **kwargs):
            if name in ("cv2", "numpy"):
                raise ImportError(name)
            return real_import(name, *args, **kwargs)

        with mock.patch.object(builtins, "__import__", fake_import):
            with self.assertRaises(RuntimeError) as caught:
                vision._need_cv()
        self.assertIn("opencv-python-headless", str(caught.exception))


if __name__ == "__main__":
    unittest.main()
