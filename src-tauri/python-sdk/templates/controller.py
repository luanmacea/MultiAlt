"""Controller script: main_all(accounts) runs ONCE and you decide the order.

Use it when accounts depend on each other (one waits for another, a shared
timer, a custom rotation). When this function exists, the run mode chosen in
the app is ignored - this script is in charge.
"""

from multialt import Account, log, sleep, stopped


def main_all(accounts: list[Account]) -> None:
    while not stopped():
        for account in accounts:
            if stopped():
                return
            log(f"Turn of {account.name}")
            with account.focused():          # one account at a time takes the keyboard
                account.press("Space")
            sleep(2)                         # pause between accounts
        sleep(60)                            # pause between rounds
