"""Block: ``account.focused()``, the "take the window" step.

Editable in developer mode ("Restore default" brings the original back).

Roblox only reads keys and clicks from the window that is in front, and only
one script (or AFK mode, or a recording) may drive the keyboard at a time. So
before any input a script must:

1. take the single *focus queue* (``focus.acquire``) - it waits its turn if
   another run, AFK mode or a recording has it;
2. bring the account's window to the front and wait until it is confirmed
   there (``window.bring_forward``);
3. act (your keys and clicks);
4. give the queue back (``focus.release``) and put back the window that was in
   front before (``window.restore_focus``). MultiAlt only does that last step
   when the queue is really free: if another account (even another thread of
   this run), AFK mode or a recording already has its turn, or is waiting for
   it, the answer is ``"denied"`` and whoever releases last gives the screen
   back.

MultiAlt checks all of this on its side too: input without a lease is
refused with ``no_focus_lease``, input to a window that is not in front with
``not_foreground``.

Keep each ``with account.focused():`` short, one action (or a few keys) at a
time: while you are inside it every other account, AFK mode and recordings
wait. Wait for pixels and images *outside* the block (``click_image`` already
does: it waits first and takes the queue only for the click).

How long it waits for the queue: with ``timeout_ms`` you choose
(``FocusTimeout`` after that). Without it, in the "all_together" mode a thread
waits for its turn as long as it takes (until Stop): the other accounts of the
same run are expected to hold the queue now and then. In the other modes the
default is 20 s.
"""

from __future__ import annotations

import threading
from contextlib import contextmanager
from typing import Any, Iterator, Optional

from multialt._core import primitives
from multialt._core.protocol import FocusTimeout, MultiAltError

# A lease expires after 30 s without a renewal (MultiAlt frees the queue so a
# crashed script cannot hold it forever). We renew every 10 s while inside the
# ``with`` so a long action keeps it.
RENEW_EVERY_SECONDS = 10.0

# Wait for the queue without ``timeout_ms`` outside "all_together".
DEFAULT_TIMEOUT_MS = 20000
# In "all_together" the wait has no end; it is asked in chunks this long.
WAIT_CHUNK_MS = 60000

# What this thread holds right now: a list of (user_id, lease) entries, so a
# nested ``with account.focused()`` for the same account is a no-op.
_held = threading.local()


def _stack() -> list[tuple[int, str]]:
    stack = getattr(_held, "stack", None)
    if stack is None:
        stack = []
        _held.stack = stack
    return stack


class _Renewer:
    """Background timer that renews the lease while the ``with`` block runs."""

    def __init__(self, lease: str) -> None:
        self._lease = lease
        self._done = threading.Event()
        self._thread = threading.Thread(target=self._run, name="multialt-lease", daemon=True)
        self._thread.start()

    def _run(self) -> None:
        while not self._done.wait(RENEW_EVERY_SECONDS):
            try:
                primitives.focus_renew(self._lease)
            except MultiAltError:
                return  # lease gone or run stopped: nothing more to renew

    def stop(self) -> None:
        self._done.set()


def _run_mode() -> str:
    import multialt

    return multialt.run_context().mode


def _acquire(uid: int, timeout_ms: Optional[int]) -> str:
    """Step 1: wait for the queue. See the module doc for how long."""
    if timeout_ms is not None:
        return primitives.focus_acquire(uid, timeout_ms)
    if _run_mode() != "all_together":
        return primitives.focus_acquire(uid, DEFAULT_TIMEOUT_MS)
    while True:
        try:
            return primitives.focus_acquire(uid, WAIT_CHUNK_MS)
        except FocusTimeout:
            continue  # still another account's turn; Stop raises Stopped here


@contextmanager
def focused(account: Any, timeout_ms: Optional[int] = None) -> Iterator[None]:
    """Hold the focus queue with ``account``'s window in front for the ``with`` body.

    ``timeout_ms`` is how long to wait for the queue (``FocusTimeout`` after
    that). Without it: until it is this thread's turn in "all_together", 20 s
    otherwise.
    """
    stack = _stack()
    uid = account.user_id

    # Already inside ``account.focused()`` for this account (for example
    # ``click_image`` called from a ``with`` block): reuse the lease.
    if stack and stack[-1][0] == uid:
        yield
        return
    if stack:
        raise RuntimeError(
            "Nested 'with other_account.focused()' inside another account's block would wait "
            "for the queue you already hold. Finish with one account before starting the next."
        )

    lease = _acquire(uid, timeout_ms)  # waits for the queue (step 1)
    stack.append((uid, lease))
    renewer = _Renewer(lease)
    try:
        primitives.window_bring_forward(uid)  # step 2: window in front, confirmed
        yield  # step 3: the body of the "with" runs here
    finally:
        # Step 4. Always runs, even on errors and on Stop, so the queue never stays taken.
        renewer.stop()
        stack.pop()
        try:
            primitives.focus_release(lease)
        except MultiAltError:
            pass  # the lease already expired or the run was stopped
        try:
            # We are the outermost ``focused`` of this thread: hand the screen back
            # to whatever the user had in front ("restored", "user_moved_on", ...).
            primitives.window_restore_focus()
        except MultiAltError:
            pass
