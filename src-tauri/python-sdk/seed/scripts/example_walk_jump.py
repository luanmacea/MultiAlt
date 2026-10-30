"""Example: walk, jump, open and close the Roblox menu, in a loop, per account.

What it does, for every account you pick, until you press Stop:

1. walks forward for a moment (W), with a global function from lib/helpers.py;
2. jumps (Space);
3. opens the Roblox menu (Escape), waits a little, and closes it (Escape);
4. counts one loop in the Dashboard tab, then rests.

How to try it: launch an account from MultiAlt, pick this script, tick the
account, press Run, and open the Dashboard tab to see the status, the loop
count and the loops per hour.

MultiAlt created this file once, as an example. It is yours: edit it, rename
it or delete it. MultiAlt never writes it again.

Careful with Escape: in the Roblox menu, L leaves the game and R resets the
character. Never press L or R while the menu is open unless that is the goal.
"""

from multialt import Account, dashboard, sleep, stopped

# A default block function (multialt/defaults/actions.py): sleep a base time
# plus or minus a random spread, so the loop is not perfectly regular.
from multialt.defaults.actions import jitter_sleep

# A global function of your own, from lib/helpers.py (see the Global functions tab).
from lib.helpers import walk_forward

# The ready-made farm report in the Dashboard tab: one row per account
# (Status, Loops, Last error, Running for), the total loops and a
# "Loops per hour" chart.
REPORT = True

WALK_SECONDS = 1.5   # how long to hold W each loop
REST_SECONDS = 5     # pause at the end of each loop


def main(account: Account) -> None:
    # Runs once per account. "One at a time" runs the accounts one after the
    # other (this loop never ends, so pick "all together" for several accounts).
    while not stopped():
        dashboard.report_status(account, "Walking")
        walk_forward(account, WALK_SECONDS)   # takes the window by itself

        # Every key and click goes inside "with account.focused():", and the
        # block stays short: other accounts wait while you are inside it.
        with account.focused():
            account.press("Space")            # jump
        sleep(0.5)

        dashboard.report_status(account, "Menu")
        with account.focused():
            account.press("Escape")           # open the Roblox menu
        jitter_sleep(1500, 300)               # wait 1.2-1.8 s, outside the block
        with account.focused():
            account.press("Escape")           # close the menu

        # Waiting for something on screen instead of a fixed time:
        # open the Inspector tab, pick the account and hover over the spot.
        # It shows the x, y position (fractions 0-1 of the window) and the
        # colour. Clicking copies "account.click(x, y)"; Shift+click copies
        # "account.wait_pixel(x, y, (r, g, b))". Then, for example:
        #
        #     if account.wait_pixel(0.52, 0.81, (255, 200, 0), timeout=10):
        #         with account.focused():
        #             account.click(0.52, 0.81)
        #
        # (0.52, 0.81) and (255, 200, 0) are placeholders: use your own.

        dashboard.report_loop(account)        # one loop done (main never returns)
        dashboard.report_status(account, "Resting")
        sleep(REST_SECONDS)
