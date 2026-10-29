"""A farm loop that reports to the Dashboard tab.

REPORT = True turns on the ready-made farm report (see multialt/defaults/dashboard.py):
a table with one row per account, the total loops and a loops-per-hour chart.
Open the Dashboard tab while this runs. Press Stop to end; the numbers stay
there until you press Clear.
"""

from multialt import Account, dashboard, sleep, stopped

REPORT = True


def main(account: Account) -> None:
    while not stopped():
        dashboard.report_status(account, "Farming")
        with account.focused():
            account.press("E")                   # your farm action
        sleep(1)
        dashboard.report_loop(account)           # one lap done (this main never returns)
        dashboard.report_status(account, "Resting")
        sleep(15)
