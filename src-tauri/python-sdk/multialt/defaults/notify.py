"""Block: ready-made Discord messages for your scripts.

Editable in developer mode ("Restore default" brings the original back).

Everything goes through ``multialt.discord``: the app posts on the webhook set
in Discord alerts (the URL never reaches the script), in the same queue as the
other alerts. Turn on "A message from a Python script" there first.

* ``send(text, title=None, account=None)``          - one message
* ``send_report(values, title="Report", account=None)`` - a ``{name: value}`` dict, one per line
* ``send_at_most_every(minutes, text, key=None, ...)``  - skips it if the same key sent less than
  ``minutes`` ago (for loops: no flood in the channel)
* ``notify_errors(title="Script error", account=None)`` - ``with`` block that posts the error and
  raises it again (a Stop is not an error and is not posted)

Example::

    from multialt.defaults import notify

    def main(account):
        with notify.notify_errors(account=account):
            ...
            notify.send_at_most_every(30, f"{account.name}: {loops} loops", key=account.name)
"""

from __future__ import annotations

import threading
import time
from contextlib import contextmanager
from typing import Any, Iterator, Mapping, Optional, Union

import multialt
from multialt._core.protocol import Stopped

_lock = threading.Lock()
_last_sent: dict[str, float] = {}  # key -> time.monotonic() of the last message


def send(text: str, title: Optional[str] = None, account: Union["multialt.Account", int, None] = None) -> None:
    """Post one message on your Discord."""
    multialt.discord(text, title=title, account=account)


def send_report(
    values: Mapping[str, Any],
    title: str = "Report",
    account: Union["multialt.Account", int, None] = None,
) -> None:
    """Post ``{"Loops": 12, "Coins": "3.4k"}`` as one line per value."""
    text = "\n".join(f"**{name}:** {value}" for name, value in values.items())
    send(text, title=title, account=account)


def send_at_most_every(
    minutes: float,
    text: str,
    key: Optional[str] = None,
    title: Optional[str] = None,
    account: Union["multialt.Account", int, None] = None,
) -> bool:
    """Post ``text`` unless the same ``key`` posted less than ``minutes`` ago.

    Returns True when it was sent. ``key`` defaults to the text itself.
    """
    key = key if key is not None else text
    now = time.monotonic()
    with _lock:
        last = _last_sent.get(key)
        if last is not None and now - last < minutes * 60:
            return False
        _last_sent[key] = now
    send(text, title=title, account=account)
    return True


@contextmanager
def notify_errors(
    title: str = "Script error",
    account: Union["multialt.Account", int, None] = None,
) -> Iterator[None]:
    """Post the error of the ``with`` block on Discord, then raise it again."""
    try:
        yield
    except Stopped:
        raise
    except Exception as error:  # noqa: BLE001 - reported and raised again
        try:
            send(f"{type(error).__name__}: {error}", title=title, account=account)
        except Exception:  # noqa: BLE001 - a Discord problem must not hide the real error
            pass
        raise
