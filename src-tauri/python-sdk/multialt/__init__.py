"""MultiAlt: automate the Roblox windows of your accounts from Python.

App-owned (rewritten on every start). Do not edit. The public API is
``Account``, ``accounts()``, ``log``, ``status``, ``sleep``, ``stopped`` and
``dashboard``; the reference is in ``docs/API.md`` and the types in
``__init__.pyi``.

Typical script::

    from multialt import Account, sleep

    def main(account: Account) -> None:
        with account.focused():          # take the focus queue, bring the window up
            account.press("Space")
        sleep(1.0)

Every position is a fraction 0-1 of the window's client area, so scripts do
not depend on the window size.
"""

from __future__ import annotations

from contextlib import AbstractContextManager
from dataclasses import dataclass, field
from typing import TYPE_CHECKING, Any, Optional, Sequence, Union

from ._core import primitives as _p
from ._core import protocol as _proto
from ._core.protocol import (
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
)

if TYPE_CHECKING:
    import numpy

__all__ = [
    "Account",
    "RunContext",
    "accounts",
    "run_context",
    "log",
    "status",
    "sleep",
    "stopped",
    "dashboard",
    "Dashboard",
    "MultiAltError",
    "NotForeground",
    "NoFocusLease",
    "UntrackedWindow",
    "Minimized",
    "Forbidden",
    "FocusTimeout",
    "UnsupportedKey",
    "BadArgs",
    "CaptureFailed",
    "Stopped",
]

RGB = tuple[int, int, int]
Point = tuple[float, float]
Region = tuple[float, float, float, float]


@dataclass(frozen=True)
class Account:
    """One of the accounts a run was started with.

    Never carries a cookie or a password. Every position is a fraction 0-1 of
    the account's window (client area): ``(0.5, 0.5)`` is the centre.
    """

    user_id: int
    name: str
    alias: str = ""
    group: str = ""
    in_game: bool = False
    place_id: int = 0
    job_id: str = ""

    @classmethod
    def from_json(cls, data: dict[str, Any]) -> "Account":
        return cls(
            user_id=int(data["user_id"]),
            name=str(data.get("name", "")),
            alias=str(data.get("alias") or ""),
            group=str(data.get("group") or ""),
            in_game=bool(data.get("in_game", False)),
            place_id=int(data.get("place_id") or 0),
            job_id=str(data.get("job_id") or ""),
        )

    def __str__(self) -> str:
        return self.name

    # -- focus ----------------------------------------------------------

    def focused(self, timeout_ms: Optional[int] = None) -> AbstractContextManager[None]:
        """Take the focus queue and bring this window to the front.

        Wrap every key press and click in ``with account.focused():``, and keep
        the block short (one action): everybody else waits while you are in it.
        On exit the queue is released and the window that was in front is
        restored. ``timeout_ms`` caps the wait for the queue (``FocusTimeout``);
        without it the wait is 20 s, or until your turn in "all_together".
        """
        from .defaults import focus

        return focus.focused(self, timeout_ms)

    # -- input (needs ``with account.focused():``) ----------------------

    def press(self, key: str, hold_ms: int = 40) -> None:
        """Tap a key (hold it ``hold_ms`` milliseconds). See "Keys" in docs/API.md."""
        from .defaults import actions

        actions.press(self, key, hold_ms)

    def key_down(self, key: str) -> None:
        """Press a key and keep it down until ``key_up`` (it is released for you on exit)."""
        from .defaults import actions

        actions.key_down(self, key)

    def key_up(self, key: str) -> None:
        """Release a key pressed with ``key_down``."""
        from .defaults import actions

        actions.key_up(self, key)

    def type_text(self, text: str, delay_ms: int = 30) -> None:
        """Type text with the supported keys (letters, digits, space)."""
        from .defaults import actions

        actions.type_text(self, text, delay_ms)

    def click(self, x: float, y: float) -> None:
        """Left click at a point (fractions 0-1 of the window)."""
        from .defaults import actions

        actions.click(self, x, y)

    # -- vision (does not need focus) -----------------------------------

    def pixel(self, x: float, y: float) -> RGB:
        """The ``(r, g, b)`` colour at a point."""
        from .defaults import vision

        return vision.pixel(self, x, y)

    def wait_pixel(
        self,
        x: float,
        y: float,
        rgb: RGB,
        tolerance: int = 12,
        timeout: float = 10.0,
        interval: float = 0.1,
    ) -> bool:
        """Wait until the pixel matches ``rgb`` (each channel within ``tolerance``).

        Returns True when it matched, False on timeout.
        """
        from .defaults import vision

        return vision.wait_pixel(self, x, y, rgb, tolerance, timeout, interval)

    def find_image(
        self,
        asset: str,
        threshold: float = 0.85,
        region: Optional[Region] = None,
        scales: Optional[Sequence[float]] = None,
    ) -> Optional[Point]:
        """Look for ``assets/<asset>`` in the window. Returns the centre of the
        best match as ``(x, y)`` fractions, or None."""
        from .defaults import vision

        if scales is None:
            return vision.find_image(self, asset, threshold, region)
        return vision.find_image(self, asset, threshold, region, scales)

    def wait_image(
        self,
        asset: str,
        timeout: float = 10.0,
        threshold: float = 0.85,
        region: Optional[Region] = None,
        interval: float = 0.5,
    ) -> Optional[Point]:
        """Wait until ``find_image`` finds the image. Returns its centre or None on timeout."""
        from .defaults import vision

        return vision.wait_image(self, asset, timeout, threshold, region, interval)

    def click_image(
        self,
        asset: str,
        timeout: float = 10.0,
        threshold: float = 0.85,
        region: Optional[Region] = None,
    ) -> bool:
        """Find the image and click its centre. Returns False if it never appeared."""
        from .defaults import vision

        return vision.click_image(self, asset, timeout, threshold, region)

    def capture(self, region: Optional[Region] = None) -> "numpy.ndarray":
        """Capture the window (or a region) as a numpy array ``(height, width, 4)``, BGRA."""
        from .defaults import vision

        return vision.capture(self, region)

    # -- reporting ------------------------------------------------------

    def status(self, text: str) -> None:
        """Set the status line shown for this account in the app."""
        _p.run_status(text, self.user_id)

    def log(self, *args: object, level: str = "log") -> None:
        """Log a line tagged with this account (filterable in the log terminal)."""
        _p.run_log(level, _join(args), self.user_id)


@dataclass
class RunContext:
    """What the app told the script about this run."""

    id: str = ""
    script: str = ""
    mode: str = "one_at_a_time"
    trigger: dict[str, Any] = field(default_factory=lambda: {"kind": "manual"})
    source: str = "app"
    accounts: list[Account] = field(default_factory=list)


_context = RunContext()


def _begin_run(hello: dict[str, Any]) -> RunContext:
    """Called by the host with the hello line. Not part of the public API."""
    global _context
    run = hello.get("run") or {}
    _context = RunContext(
        id=str(run.get("id", "")),
        script=str(run.get("script", "")),
        mode=str(run.get("mode", "one_at_a_time")),
        trigger=dict(run.get("trigger") or {"kind": "manual"}),
        source=str(run.get("source", "app")),
        accounts=[Account.from_json(a) for a in hello.get("accounts", [])],
    )
    return _context


def _join(args: Sequence[object]) -> str:
    return " ".join(str(a) for a in args)


def _uid(account: Union[Account, int, None]) -> Optional[int]:
    if account is None:
        return None
    return account.user_id if isinstance(account, Account) else int(account)


def accounts() -> list[Account]:
    """The accounts of this run, fresh from the app.

    ``in_game``, ``place_id`` and ``job_id`` are read now: a client opened by
    MultiAlt that is in a game, and the place and server the Roblox log
    reported for it (``0`` / ``""`` until the log says so, or with no client).
    """
    return [Account.from_json(a) for a in _p.accounts()]


def run_context() -> RunContext:
    """Mode, trigger, script name and source (``"app"`` or ``"terminal"``) of this run."""
    return _context


def log(*args: object, level: str = "log") -> None:
    """Log a line (same as ``print``). ``level`` is ``"log"``, ``"warn"`` or ``"error"``."""
    _p.run_log(level, _join(args))


def status(text: str) -> None:
    """Set the status line of the run shown in the app."""
    _p.run_status(text)


def sleep(seconds: float) -> None:
    """Sleep ``seconds``. Wakes up and raises ``Stopped`` as soon as the run is stopped.

    Use this instead of ``time.sleep`` so the Stop button is instant.
    """
    _p.run_sleep(int(round(seconds * 1000)))


def stopped() -> bool:
    """True once the run was asked to stop. Check it in long loops."""
    return _proto.stop_requested()


class Dashboard:
    """Draw your own report in the Dashboard tab. See "Dashboard" in docs/API.md."""

    def define(self, layout: list[dict[str, Any]]) -> None:
        """Set the widgets of this script's section. Each widget is
        ``{"id", "type", "title", "columns"?, "max"?}``; ``type`` is one of
        stat, progress, text, badge, table, accounts, line, log."""
        _p.dashboard_define(layout)

    def set(self, widget_id: str, value: Any, account: Union[Account, int, None] = None) -> None:
        """Set a widget value, for the whole script or for one account."""
        _p.dashboard_set(widget_id, value, _uid(account))

    def push(self, widget_id: str, value: Any) -> None:
        """Add a point to a ``line`` widget (or a line to a ``log`` widget)."""
        _p.dashboard_push(widget_id, value)

    def clear(self) -> None:
        """Remove this script's widgets and values."""
        _p.dashboard_clear()

    def farm_report(self) -> None:
        """Define the ready-made farm report: per-account table, total loops, loops per hour."""
        from .defaults import dashboard as block

        block.farm_report()

    def report_status(self, account: Account, text: str) -> None:
        """Update the account's row of the farm report (and its status line)."""
        from .defaults import dashboard as block

        block.report_status(account, text)

    def report_loop(self, account: Account) -> None:
        """Count one finished loop for the account in the farm report."""
        from .defaults import dashboard as block

        block.report_loop(account)

    def report_error(self, account: Account, err: object) -> None:
        """Record an error for the account in the farm report."""
        from .defaults import dashboard as block

        block.report_error(account, err)


dashboard = Dashboard()
