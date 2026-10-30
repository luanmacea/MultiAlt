"""Block: the runner. This is the code that calls YOUR ``main``.

Editable in developer mode ("Restore default" brings the original back).

What happens when you press Run
-------------------------------
MultiAlt starts one Python process for the run. The host
(``multialt/_core/host.py``) loads your script as a module and then calls
``run(module, ctx)`` below. ``ctx.accounts`` are the accounts you picked and
``ctx.mode`` is the mode you picked in the app:

* ``"one_at_a_time"`` - account 1 runs ``main`` to the end, then account 2, ...
* ``"all_together"``  - every account runs ``main`` at the same time, each in
  its own thread. The threads do NOT all type at once: input goes through the
  single focus queue (``with account.focused():`` in ``focus.py``), so each
  account takes its turn bringing its window to the front, and waits for that
  turn as long as it takes. Keep each ``with account.focused():`` short.

Which function of your script is called
---------------------------------------
1. ``main_all(accounts)`` if your script defines it. It is called once with
   every account and the mode is ignored: YOU decide the order (a controller).
2. otherwise ``main(account)``, once per account, as the mode says.

Switching windows
-----------------
This file does not move windows itself. Switching happens inside your ``main``,
every time you write ``with account.focused():`` (see ``focus.py``): it waits
for its turn, brings that account's window to the front, runs your keys and
clicks, then gives the turn back. So to change *how* accounts take turns, edit
``focus.py`` (the turn) or this file (who runs when).

Errors
------
* If ``main`` raises for one account, the error is logged with the account's
  name and the OTHER accounts keep going. When everybody finished, the run
  ends as failed (``AccountsFailed``) so you notice.
* ``Stopped`` (you pressed Stop) is not an error: everything ends quietly.

Dashboard report
----------------
Put ``REPORT = True`` at the top of your script to get the farm report in the
Dashboard tab for free: the status of each account, its errors, and one loop
counted every time ``main(account)`` returns.
"""

from __future__ import annotations

import threading
import traceback
from typing import Any, Callable

from multialt import dashboard, stopped
from multialt._core import protocol
from multialt._core.protocol import Stopped


class AccountsFailed(Exception):
    """Raised at the end of a run when ``main`` failed for one or more accounts."""


def run(module: Any, ctx: Any) -> None:
    """Entry point called by the host. ``module`` is your script, ``ctx`` the run."""
    report = bool(getattr(module, "REPORT", False))

    # Step 1: if the script opted in to the farm report, define its widgets once.
    if report:
        dashboard.farm_report()

    # Step 2: a controller script takes over completely.
    main_all = getattr(module, "main_all", None)
    if callable(main_all):
        main_all(list(ctx.accounts))
        return

    # Step 3: otherwise we need a per-account main.
    main = getattr(module, "main", None)
    if not callable(main):
        raise RuntimeError(
            "Your script needs a function main(account) (runs once per account) "
            "or main_all(accounts) (runs once, you loop)."
        )

    # Step 4: run it for every account, in the order the mode asks for.
    if ctx.mode == "all_together":
        failed = _run_all_together(main, list(ctx.accounts), report)
    else:
        failed = _run_one_at_a_time(main, list(ctx.accounts), report)

    # Step 5: tell the app if somebody failed (it shows the run as failed).
    if failed:
        names = ", ".join(failed)
        raise AccountsFailed(
            f"main() failed for {len(failed)} of {len(ctx.accounts)} accounts: {names}. "
            "The traceback of each one is in the log above."
        )


def _run_one_at_a_time(main: Callable[[Any], Any], accounts: list[Any], report: bool) -> list[str]:
    """Account 1 to the end, then account 2, and so on. Returns the names that failed."""
    failed: list[str] = []
    for account in accounts:
        if stopped():
            break  # Stop was pressed between two accounts
        if not _run_account(main, account, report):
            failed.append(account.name)
    return failed


def _run_all_together(main: Callable[[Any], Any], accounts: list[Any], report: bool) -> list[str]:
    """One thread per account. A failure in one thread never stops the others."""
    failed: list[str] = []
    failed_lock = threading.Lock()

    def worker(account: Any) -> None:
        if not _run_account(main, account, report):
            with failed_lock:
                failed.append(account.name)

    threads = [threading.Thread(target=worker, args=(a,), name=f"account-{a.name}") for a in accounts]
    for thread in threads:
        thread.start()
    for thread in threads:
        thread.join()
    return failed


def _run_account(main: Callable[[Any], Any], account: Any, report: bool) -> bool:
    """Run ``main(account)``. Returns True on success, False if it raised.

    ``Stopped`` is swallowed here: it just means "end quietly", and every later
    call raises it again, so the rest of the run unwinds on its own.
    """
    # From now on, everything this thread prints is tagged with this account in the log.
    protocol.set_current_user(account.user_id)
    try:
        if report:
            dashboard.report_status(account, "Running")
        main(account)
        if report:
            dashboard.report_loop(account)
            dashboard.report_status(account, "Done")
        return True
    except Stopped:
        return True
    except Exception as exc:  # noqa: BLE001 - one account must not take the others down
        account.log(f"{account.name}: main() failed\n{traceback.format_exc()}", level="error")
        if report:
            try:
                dashboard.report_error(account, f"{type(exc).__name__}: {exc}")
                dashboard.report_status(account, "Error")
            except Stopped:
                pass
        return False
    finally:
        protocol.set_current_user(None)
