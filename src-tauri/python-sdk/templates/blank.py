"""Blank script. Write your steps in main().

Run it from MultiAlt > Scripts > Roblox Player automation: pick this script,
pick the accounts, press Run. main(account) is called once per account.
"""

from multialt import Account, log


def main(account: Account) -> None:
    log(f"Hello from {account.name}")
