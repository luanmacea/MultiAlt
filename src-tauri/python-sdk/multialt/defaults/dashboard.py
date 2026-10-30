"""Block: the ready-made farm report for the Dashboard tab.

Editable in developer mode ("Restore default" brings the original back).

``farm_report()`` defines three widgets:

* ``accounts``       - one row per account: Status, Loops, Last error, Running for
* ``total_loops``    - a big number with all finished loops
* ``loops_per_hour`` - a line chart of loops per hour, one point per finished loop

The default runner (``runner.py``) feeds it when your script has ``REPORT = True``:
it sets the status when an account starts and ends, records errors, and counts
one loop each time ``main(account)`` returns. If your ``main`` loops forever
instead, call ``dashboard.report_loop(account)`` once per lap yourself.

Value shapes sent to the app (the Dashboard tab draws them):

* ``accounts``: per account, an object keyed by column title,
  e.g. ``{"Status": "Working", "Loops": 3, "Last error": "", "Running for": "12m"}``
* ``total_loops``: a number
* ``loops_per_hour``: numbers pushed with ``dashboard.push``
"""

from __future__ import annotations

import threading
import time
from typing import Any

from multialt._core import primitives

COLUMNS = ["Status", "Loops", "Last error", "Running for"]

_lock = threading.Lock()
_rows: dict[int, dict[str, Any]] = {}  # user_id -> {"status", "loops", "error", "started"}
_report_started: float = 0.0
_total_loops: int = 0


def farm_report() -> None:
    """Define the report widgets and reset the counters. Safe to call again."""
    global _report_started, _total_loops
    with _lock:
        _rows.clear()
        _report_started = time.time()
        _total_loops = 0
    primitives.dashboard_define(
        [
            {"id": "accounts", "type": "accounts", "title": "Accounts", "columns": COLUMNS},
            {"id": "total_loops", "type": "stat", "title": "Total loops"},
            {"id": "loops_per_hour", "type": "line", "title": "Loops per hour"},
        ]
    )
    primitives.dashboard_set("total_loops", 0)


def _duration(seconds: float) -> str:
    seconds = int(max(0, seconds))
    hours, rest = divmod(seconds, 3600)
    minutes = rest // 60
    if hours:
        return f"{hours}h {minutes:02d}m"
    if minutes:
        return f"{minutes}m"
    return f"{seconds}s"


def _row(user_id: int) -> dict[str, Any]:
    row = _rows.get(user_id)
    if row is None:
        row = {"status": "", "loops": 0, "error": "", "started": time.time()}
        _rows[user_id] = row
    return row


def _publish(account: Any) -> None:
    with _lock:
        row = _row(account.user_id)
        value = {
            "Status": row["status"],
            "Loops": row["loops"],
            "Last error": row["error"],
            "Running for": _duration(time.time() - row["started"]),
        }
    primitives.dashboard_set("accounts", value, account.user_id)


def report_status(account: Any, text: str) -> None:
    """Set the account's Status cell (and the status line shown for the account)."""
    with _lock:
        _row(account.user_id)["status"] = text
    primitives.run_status(text, account.user_id)
    _publish(account)


def report_loop(account: Any) -> None:
    """Count one finished loop for the account."""
    global _report_started, _total_loops
    now = time.time()
    with _lock:
        _row(account.user_id)["loops"] += 1
        _total_loops += 1
        total = _total_loops
        if not _report_started:
            _report_started = now
        hours = max((now - _report_started) / 3600.0, 1.0 / 60.0)  # count at least one minute
        per_hour = round(total / hours, 1)
    primitives.dashboard_set("total_loops", total)
    primitives.dashboard_push("loops_per_hour", per_hour)
    _publish(account)


def report_error(account: Any, err: object) -> None:
    """Record the account's last error (shown in the Last error cell)."""
    text = str(err).strip().splitlines()[-1] if str(err).strip() else err.__class__.__name__
    with _lock:
        _row(account.user_id)["error"] = text[:200]
    _publish(account)
