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
   front before (``window.restore_focus``).

MultiAlt checks all of this on its side too: input without a lease is
refused with ``no_focus_lease``, input to a window that is not in front with
``not_foreground``.
"""

from __future__ import annotations

import threading
from contextlib import contextmanager
from typing import Any, Iterator

from multialt._core import primitives
from multialt._core.protocol import MultiAltError

# A lease expires after 30 s without a renewal (MultiAlt frees the queue so a
# crashed script cannot hold it forever). We renew every 10 s while inside the
# ``with`` so a long action keeps it.
RENEW_EVERY_SECONDS = 10.0

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


@contextmanager
def focused(account: Any, timeout_ms: int = 20000) -> Iterator[None]:
    """Hold the focus queue with ``account``'s window in front for the ``with`` body.

    ``timeout_ms`` is how long to wait for the queue (``FocusTimeout`` after that).
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

    lease = primitives.focus_acquire(uid, timeout_ms)  # waits for the queue (step 1)
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
