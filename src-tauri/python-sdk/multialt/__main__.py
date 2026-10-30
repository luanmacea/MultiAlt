"""``python -m multialt ...`` entry point (app-owned, do not edit)."""

import os
import sys

from multialt.cli import main

if __name__ == "__main__":
    code = main()
    try:
        sys.stdout.flush()
        sys.stderr.flush()
    except (OSError, ValueError):
        pass
    # A reader thread may still be waiting on the pipe; skip the normal shutdown.
    os._exit(code)
