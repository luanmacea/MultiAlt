"""Anti-AFK with a pixel check.

Every few minutes it jumps and nudges the character so Roblox does not kick
the account for being idle. Before each nudge it looks at one pixel: if the
pixel shows the disconnect dialog's colour, the account was kicked, so the
script says so and ends for that account.

Set up (use the Inspector tab to find the numbers):
  1. Disconnect the account on purpose once, open the Inspector, hover the dark
     overlay and note its (x, y) and colour.
  2. Put them in WATCH_POINT and DISCONNECTED_COLOR below.
"""

from multialt import Account, log, sleep, stopped

WATCH_POINT = (0.5, 0.5)                 # where to look (fractions of the window)
DISCONNECTED_COLOR = (57, 59, 61)        # the dialog's colour at that point (r, g, b)
EVERY_SECONDS = 240                      # roblox kicks after 20 minutes idle; 4 is safe


def main(account: Account) -> None:
    while not stopped():
        # Reading the screen does not need the focus queue.
        if account.wait_pixel(*WATCH_POINT, DISCONNECTED_COLOR, tolerance=6, timeout=0):
            account.status("Disconnected")
            log(f"{account.name} looks disconnected, stopping its loop", level="warn")
            return

        with account.focused():
            account.press("Space")       # jump
            account.press("A", 150)      # tiny step left...
            account.press("D", 150)      # ...and back
        account.status("Alive")
        sleep(EVERY_SECONDS)
