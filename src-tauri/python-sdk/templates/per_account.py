"""Per-account script: main(account) runs once for each account you pick.

Run mode (chosen in the app):
  * one at a time - account 1 finishes, then account 2, ...
  * all together  - every account runs main() at the same time; the keyboard
                    is shared fairly through the focus queue.

Rules of thumb:
  * Wrap every key press and click in "with account.focused():".
  * Positions are fractions 0-1 of the window: (0.5, 0.5) is the centre.
  * Use sleep() (not time.sleep) and check stopped() in loops, so Stop is instant.
"""

from multialt import Account, sleep, stopped


def main(account: Account) -> None:
    account.status("Starting")

    while not stopped():
        with account.focused():       # waits for its turn, brings the window to the front
            account.press("Space")    # tap a key
            account.click(0.5, 0.5)   # left click in the centre of the window
        account.status("Waiting")
        sleep(30)                     # seconds; wakes up at once when you press Stop
