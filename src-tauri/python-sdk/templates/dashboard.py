"""Script with the ready-made farm report in the Dashboard tab.

REPORT = True makes the runner show, per account, its status, loops, last error
and running time, plus the total number of loops and loops per hour.

This main() loops until you press Stop, so it counts its own laps with
dashboard.report_loop(account). (A main() that returns after one lap, started
again by the "Every N minutes" trigger, gets one loop counted for free.)
"""

from multialt import Account, dashboard, sleep, stopped

REPORT = True


def main(account: Account) -> None:
    while not stopped():
        dashboard.report_status(account, "Working")
        with account.focused():
            account.press("E")               # replace with your farm step
        dashboard.report_loop(account)
        dashboard.report_status(account, "Resting")
        sleep(20)
