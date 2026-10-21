"""Thin typed wrappers: one function per operation MultiAlt exposes.

App-owned (rewritten on every start). Do not edit.

Every function here is a direct call into the app. The safety rules (focus
lease, window in front, tracked clients only, no cookies) are enforced by
MultiAlt itself, so nothing in this file or in the default blocks can bypass
them. Coordinates are fractions 0-1 of the window's client area.
"""

from __future__ import annotations

import mmap
from typing import Any, Optional

from . import protocol

Region = dict[str, float]  # {"x", "y", "w", "h"}, fractions 0-1


# -- accounts and windows ---------------------------------------------------


def accounts() -> list[dict[str, Any]]:
    """The accounts of this run (never cookies or passwords)."""
    return protocol.call("accounts")


def windows() -> list[dict[str, Any]]:
    """The Roblox window of each account: client area in screen pixels, DPI,
    ``minimized`` and ``foreground`` flags."""
    return protocol.call("windows")


# -- focus queue ------------------------------------------------------------


def focus_acquire(user_id: int, timeout_ms: int = 20000) -> str:
    """Take the single focus queue (shared with AFK mode and recordings).

    Returns the lease id. Raises ``FocusTimeout`` when the queue did not free up.
    """
    result = protocol.call("focus.acquire", {"user_id": user_id, "timeout_ms": timeout_ms})
    return str(result["lease"])


def focus_renew(lease: str) -> bool:
    """Keep a lease alive. A lease expires after 30 s without renewal."""
    return bool(protocol.call("focus.renew", {"lease": lease}))


def focus_release(lease: str) -> bool:
    """Give the focus queue back (also releases any key still held)."""
    return bool(protocol.call("focus.release", {"lease": lease}))


def window_bring_forward(user_id: int) -> bool:
    """Bring the account's window to the front. True once it is confirmed in front."""
    return bool(protocol.call("window.bring_forward", {"user_id": user_id}))


def window_restore_focus() -> str:
    """Give the foreground back to whoever had it.

    Returns ``"restored"``, ``"user_moved_on"``, ``"gone"`` or ``"denied"``.
    """
    return str(protocol.call("window.restore_focus"))


# -- input ------------------------------------------------------------------


def input_key(user_id: int, key: str, hold_ms: int = 40) -> bool:
    return bool(protocol.call("input.key", {"user_id": user_id, "key": key, "hold_ms": hold_ms}))


def input_key_down(user_id: int, key: str) -> bool:
    return bool(protocol.call("input.key_down", {"user_id": user_id, "key": key}))


def input_key_up(user_id: int, key: str) -> bool:
    return bool(protocol.call("input.key_up", {"user_id": user_id, "key": key}))


def input_type_text(user_id: int, text: str, delay_ms: int = 30) -> bool:
    return bool(protocol.call("input.type_text", {"user_id": user_id, "text": text, "delay_ms": delay_ms}))


def input_click(user_id: int, x: float, y: float) -> bool:
    """Left click at a point given as fractions of the client area."""
    return bool(protocol.call("input.click", {"user_id": user_id, "x": x, "y": y}))


# -- screen -----------------------------------------------------------------


def screen_pixel(user_id: int, x: float, y: float) -> tuple[int, int, int]:
    """The ``(r, g, b)`` colour at a point (fractions of the client area)."""
    r, g, b = protocol.call("screen.pixel", {"user_id": user_id, "x": x, "y": y})
    return int(r), int(g), int(b)


def screen_capture(user_id: int, region: Optional[Region] = None) -> Any:
    """Capture the window (or a region of it) as a numpy array ``(h, w, 4)`` BGRA.

    The app puts the pixels in a named shared-memory block; this copies them
    out and closes the mapping. Needs numpy (installed with the Python runtime).
    """
    try:
        import numpy as np
    except ImportError as exc:
        raise RuntimeError(
            "numpy is not installed. Install the Python runtime from "
            "MultiAlt > Scripts > Roblox Player automation, or run 'pip install numpy'."
        ) from exc
    args: dict[str, Any] = {"user_id": user_id}
    if region is not None:
        args["region"] = region
    info = protocol.call("screen.capture", args)
    width, height, stride = int(info["width"]), int(info["height"]), int(info["stride"])
    mapping = mmap.mmap(-1, stride * height, tagname=str(info["shm"]))
    raw = None
    try:
        raw = np.frombuffer(mapping, dtype=np.uint8, count=stride * height)
        frame = raw.reshape(height, stride)[:, : width * 4].reshape(height, width, 4).copy()
    finally:
        raw = None  # release the buffer export so the mapping can close
        mapping.close()
    return frame


# -- run --------------------------------------------------------------------


def run_sleep(ms: int) -> bool:
    """Sleep in the app (it wakes early and raises ``Stopped`` on Stop)."""
    return bool(protocol.call("run.sleep", {"ms": max(0, int(ms))}))


def run_stopped() -> bool:
    return bool(protocol.call("run.stopped"))


def run_log(level: str, message: str, user_id: Optional[int] = None) -> None:
    """Send a log line (fire-and-forget). ``level``: ``log``, ``warn`` or ``error``."""
    args: dict[str, Any] = {"level": level, "message": message}
    uid = user_id if user_id is not None else protocol.current_user()
    if uid is not None:
        args["user_id"] = uid
    protocol.notify("run.log", args)


def run_status(text: str, user_id: Optional[int] = None) -> None:
    """Set the status line shown for the run, or for one account (fire-and-forget)."""
    args: dict[str, Any] = {"text": text}
    uid = user_id if user_id is not None else protocol.current_user()
    if uid is not None:
        args["user_id"] = uid
    protocol.notify("run.status", args)


# -- dashboard --------------------------------------------------------------


def dashboard_define(layout: list[dict[str, Any]]) -> bool:
    return bool(protocol.call("dashboard.define", {"layout": layout}))


def dashboard_set(widget_id: str, value: Any, user_id: Optional[int] = None) -> bool:
    args: dict[str, Any] = {"id": widget_id, "value": value}
    if user_id is not None:
        args["user_id"] = user_id
    return bool(protocol.call("dashboard.set", args))


def dashboard_push(widget_id: str, value: Any) -> bool:
    return bool(protocol.call("dashboard.push", {"id": widget_id, "value": value}))


def dashboard_clear() -> bool:
    return bool(protocol.call("dashboard.clear"))
