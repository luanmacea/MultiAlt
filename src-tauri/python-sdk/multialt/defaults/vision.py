"""Block: reading the screen - pixels and image search.

Editable in developer mode ("Restore default" brings the original back).

Reading does not need the focus queue: the window is captured even when
another window is on top of it (not when it is minimized). Positions are
fractions 0-1 of the window's client area.

``find_image`` uses OpenCV template matching on grayscale at several scales,
so a template cut at one window size still works when the window is a bit
bigger or smaller. numpy and OpenCV are imported only when needed.
"""

from __future__ import annotations

import time
from pathlib import Path
from typing import Any, Optional, Sequence

from multialt._core import ASSETS_DIR, primitives

# Template scales tried by ``find_image``: 0.75, 0.80, ... 1.25.
DEFAULT_SCALES: tuple[float, ...] = tuple(round(0.75 + 0.05 * i, 2) for i in range(11))

# (path, modified time) -> decoded grayscale template, so a polling loop does not re-read the file.
_template_cache: dict[tuple[str, float], Any] = {}


def _need_cv() -> tuple[Any, Any]:
    try:
        import cv2
        import numpy as np
    except ImportError as exc:
        raise RuntimeError(
            "Image search needs numpy and opencv-python-headless. Install the Python runtime from "
            "MultiAlt > Scripts > Roblox Player automation (they come with it), or run "
            "'pip install numpy opencv-python-headless'."
        ) from exc
    return cv2, np


def _region_arg(region: Optional[Sequence[float]]) -> Optional[dict[str, float]]:
    if region is None:
        return None
    x, y, w, h = region
    return {"x": float(x), "y": float(y), "w": float(w), "h": float(h)}


def _sleep(seconds: float) -> None:
    # The app's sleep wakes up early and raises Stopped when the run is stopped.
    primitives.run_sleep(int(round(seconds * 1000)))


# -- pixels -----------------------------------------------------------------


def pixel(account: Any, x: float, y: float) -> tuple[int, int, int]:
    """The ``(r, g, b)`` colour at ``(x, y)``."""
    return primitives.screen_pixel(account.user_id, x, y)


def wait_pixel(
    account: Any,
    x: float,
    y: float,
    rgb: Sequence[int],
    tolerance: int = 12,
    timeout: float = 10.0,
    interval: float = 0.1,
) -> bool:
    """Poll the pixel until every channel is within ``tolerance`` of ``rgb``.

    Returns True when it matched, False after ``timeout`` seconds.
    """
    deadline = time.monotonic() + timeout
    while True:
        r, g, b = pixel(account, x, y)
        if abs(r - rgb[0]) <= tolerance and abs(g - rgb[1]) <= tolerance and abs(b - rgb[2]) <= tolerance:
            return True
        if time.monotonic() >= deadline:
            return False
        _sleep(interval)


# -- images -----------------------------------------------------------------


def capture(account: Any, region: Optional[Sequence[float]] = None) -> Any:
    """The window (or ``region`` = ``(x, y, w, h)`` fractions) as a BGRA numpy array."""
    return primitives.screen_capture(account.user_id, _region_arg(region))


def _load_template(asset: str) -> Any:
    cv2, np = _need_cv()
    path = Path(asset)
    if not path.is_absolute():
        path = ASSETS_DIR / asset
    if not path.is_file():
        raise FileNotFoundError(f"Image not found: {path}. Save it in the assets/ folder (the Inspector can do it).")
    key = (str(path), path.stat().st_mtime)
    cached = _template_cache.get(key)
    if cached is None:
        # imdecode(fromfile) instead of imread: imread cannot open non-ASCII paths on Windows.
        data = np.fromfile(str(path), dtype=np.uint8)
        cached = cv2.imdecode(data, cv2.IMREAD_GRAYSCALE)
        if cached is None:
            raise ValueError(f"Could not read {path} as an image.")
        _template_cache[key] = cached
    return cached


def find_image(
    account: Any,
    asset: str,
    threshold: float = 0.85,
    region: Optional[Sequence[float]] = None,
    scales: Sequence[float] = DEFAULT_SCALES,
) -> Optional[tuple[float, float]]:
    """Look for ``assets/<asset>`` in the window.

    Returns the centre of the best match as ``(x, y)`` fractions of the window,
    or None when nothing scores at least ``threshold`` (0-1, higher = stricter).
    ``region`` = ``(x, y, w, h)`` fractions limits the search (and is faster).
    """
    cv2, np = _need_cv()
    template = _load_template(asset)
    frame = capture(account, region)  # BGRA
    height, width = frame.shape[:2]
    gray = cv2.cvtColor(frame, cv2.COLOR_BGRA2GRAY)

    best_score = -1.0
    best_center = (0.0, 0.0)
    for scale in scales:
        tw = max(1, int(round(template.shape[1] * scale)))
        th = max(1, int(round(template.shape[0] * scale)))
        if tw > width or th > height:
            continue  # the template would not fit at this scale
        interpolation = cv2.INTER_AREA if scale < 1 else cv2.INTER_LINEAR
        scaled = template if scale == 1 else cv2.resize(template, (tw, th), interpolation=interpolation)
        if scaled.std() < 1e-6:
            continue  # a flat template matches everything; skip it
        result = cv2.matchTemplate(gray, scaled, cv2.TM_CCOEFF_NORMED)
        _min, score, _min_loc, max_loc = cv2.minMaxLoc(result)
        if np.isfinite(score) and score > best_score:
            best_score = float(score)
            best_center = (max_loc[0] + tw / 2.0, max_loc[1] + th / 2.0)

    if best_score < threshold:
        return None
    # Pixels of the (possibly cropped) capture -> fractions of the whole window.
    rx, ry, rw, rh = (0.0, 0.0, 1.0, 1.0) if region is None else tuple(float(v) for v in region)
    return (rx + best_center[0] / width * rw, ry + best_center[1] / height * rh)


def wait_image(
    account: Any,
    asset: str,
    timeout: float = 10.0,
    threshold: float = 0.85,
    region: Optional[Sequence[float]] = None,
    interval: float = 0.5,
) -> Optional[tuple[float, float]]:
    """Keep calling ``find_image`` until it finds the image or ``timeout`` seconds pass."""
    deadline = time.monotonic() + timeout
    while True:
        found = find_image(account, asset, threshold, region)
        if found is not None:
            return found
        if time.monotonic() >= deadline:
            return None
        _sleep(interval)


def click_image(
    account: Any,
    asset: str,
    timeout: float = 10.0,
    threshold: float = 0.85,
    region: Optional[Sequence[float]] = None,
) -> bool:
    """Find the image, then click its centre. False if it did not show up in time."""
    found = wait_image(account, asset, timeout, threshold, region)
    if found is None:
        return False
    # The click needs the focus queue and the window in front; if the caller is
    # already inside ``account.focused()`` this reuses it.
    with account.focused():
        account.click(found[0], found[1])
    return True
