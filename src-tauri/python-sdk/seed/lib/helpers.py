"""Your global functions: an example module in lib/.

Anything you put in a .py file inside lib/ can be imported by every script in
scripts/. This file is lib/helpers.py, so a script imports it with:

    from lib.helpers import walk_forward

Add your own functions here, or create more files next to this one (for
example lib/menus.py -> "from lib.menus import open_shop"). The Global
functions tab lists every function it finds in lib/ with the import line
ready to copy.

MultiAlt created this file once, as an example. It is yours: edit it, rename it
or delete it. MultiAlt never writes it again.
"""

from multialt import Account


def walk_forward(account: Account, seconds: float = 1.0) -> None:
    """Hold W for ``seconds`` so the character walks forward.

    It takes the window itself (``with account.focused():``), so call it
    OUTSIDE your own ``with account.focused():`` block. Up to 10 seconds per
    call (the longest a key can be held).
    """
    hold_ms = int(min(max(seconds, 0.01), 10.0) * 1000)
    with account.focused():
        account.press("W", hold_ms=hold_ms)
