"""Block: keys, typing, clicks and waits with a little randomness.

Editable in developer mode ("Restore default" brings the original back).

These are the functions behind ``account.press()``, ``account.click()`` and
friends. They must run inside ``with account.focused():``: MultiAlt refuses
input without the focus queue (``no_focus_lease``) or while the window is not
in front (``not_foreground``).
"""

from __future__ import annotations

import random
from typing import Any

from multialt._core import primitives


def press(account: Any, key: str, hold_ms: int = 40) -> None:
    """Tap ``key`` and hold it ``hold_ms`` milliseconds (10-10000).

    Key names: A-Z, 0-9, Space, Shift, Up, Down, Left, Right (case-insensitive).
    """
    primitives.input_key(account.user_id, key, hold_ms)


def key_down(account: Any, key: str) -> None:
    """Press a key and leave it down. Always pair it with ``key_up``; if you forget,
    MultiAlt releases it when the focus lease ends."""
    primitives.input_key_down(account.user_id, key)


def key_up(account: Any, key: str) -> None:
    """Release a key pressed with ``key_down``."""
    primitives.input_key_up(account.user_id, key)


def type_text(account: Any, text: str, delay_ms: int = 30) -> None:
    """Type ``text`` one character at a time, ``delay_ms`` between characters.

    Only characters that map to the supported keys work (letters, digits and
    space); anything else raises ``UnsupportedKey``.
    """
    primitives.input_type_text(account.user_id, text, delay_ms)


def click(account: Any, x: float, y: float) -> None:
    """Left click at ``(x, y)``, fractions 0-1 of the window's client area."""
    primitives.input_click(account.user_id, x, y)


def jitter_sleep(base_ms: int, spread_ms: int = 0) -> None:
    """Sleep ``base_ms`` plus or minus up to ``spread_ms`` (random), never below 0.

    Handy to keep repeated actions from being perfectly regular. Wakes up
    immediately when the run is stopped.
    """
    ms = base_ms + random.uniform(-spread_ms, spread_ms)
    primitives.run_sleep(max(0, int(ms)))
