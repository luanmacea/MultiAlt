"""One routine over all the accounts, in your own order (a controller).

main_all(accounts) runs once. Each round it visits every account: takes the
keyboard, does the steps, and moves on. Press Stop to end.
"""

from multialt import Account, log, sleep, stopped

STEPS = ["E", "F", "Space"]   # keys to tap, in order, for every account


def run_steps(account: Account) -> None:
    # One key per focus block, and the wait outside it: other accounts and AFK
    # mode wait for the focus queue while the block is held.
    for key in STEPS:
        with account.focused():
            account.press(key)
        sleep(0.5)


def main_all(accounts: list[Account]) -> None:
    round_number = 0
    while not stopped():
        round_number += 1
        log(f"Round {round_number}")
        for account in accounts:
            if stopped():
                return
            account.status(f"Round {round_number}")
            run_steps(account)
            sleep(1)             # short pause before the next account
        sleep(30)                # pause between rounds
