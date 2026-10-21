"""Click a button found by its picture.

Set up:
  1. Open the Inspector tab, pick the account, drag a rectangle around the
     button, name it play_button.png. It is saved in the assets/ folder.
  2. Run this script.

find_image tries the picture at several sizes (75%-125%), so it keeps working
if the window is a bit larger or smaller than when you cut it.
"""

from multialt import Account, log

BUTTON = "play_button.png"


def main(account: Account) -> None:
    account.status(f"Looking for {BUTTON}")
    # Waits up to 15 s for the picture, then clicks its centre.
    if account.click_image(BUTTON, timeout=15, threshold=0.85):
        account.status("Clicked")
        log(f"{account.name}: clicked {BUTTON}")
    else:
        account.status("Button not found")
        log(f"{account.name}: {BUTTON} never appeared (lower threshold= or re-cut the picture)", level="warn")
