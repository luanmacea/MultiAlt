# AGENTS.md - writing MultiAlt Python scripts

Read this first. It is for AI tools (Claude and others) writing scripts for a
MultiAlt user. Full reference: `docs/API.md`. Types: `multialt/__init__.pyi`.

## What this folder is

The workspace of MultiAlt's **Roblox Player automation (Python)**. Scripts
control the Roblox windows of the user's accounts: read pixels, find images,
send keys and clicks, one window at a time. They do not inject into Roblox.

```
scripts/     <- WRITE YOUR SCRIPTS HERE (one .py per script)
lib/         <- shared helpers: "from lib.tools import wait_for_menu"
assets/      <- images for find_image (the app's Inspector saves cut-outs here)
examples/    <- working examples, copy from them
templates/   <- what "New script" starts from
docs/API.md  <- full reference
multialt/    <- the library. DO NOT EDIT multialt/_core, multialt/__init__.py(i),
                AGENTS.md, docs/, examples/, templates/ (rewritten on every app start)
multialt/defaults/  <- "blocks": editable ONLY if the user turned on developer mode
```

Script file names: letters, digits, `_` and `-`, ending in `.py`, in `scripts/` only.

## A script

```python
from multialt import Account, sleep, stopped

def main(account: Account) -> None:        # runs once per account
    while not stopped():
        with account.focused():             # REQUIRED around every key / click
            account.press("Space")
        sleep(30)
```

Alternative: `def main_all(accounts: list[Account])` runs once and you loop over
the accounts yourself (it wins over `main`). `REPORT = True` at module level
turns on the farm report in the Dashboard tab.

## Rules (the app enforces them; breaking one raises an error)

1. **Positions are fractions 0-1** of the window's client area: `(0.5, 0.5)` is the centre. Never pixels.
2. **Wrap input in `with account.focused():`** (keys, clicks, typing). Reading (`pixel`, `find_image`, `capture`) does not need it.
3. **Never `time.sleep`.** Use `sleep(seconds)` from multialt: Stop wakes it up at once.
4. **Every loop checks `stopped()`** (`while not stopped():`) and contains a `sleep`.
5. **Never catch `Stopped`** and never write a bare `except:`. Catch specific `MultiAltError` subclasses and re-raise `Stopped`.
6. Do not hold `with account.focused():` while sleeping for a long time: other accounts and AFK mode wait for the queue. Do the input, leave the block, then sleep.
7. Cookies and passwords are not available and never will be. Do not try to read them.
8. Keys: A-Z, 0-9, Space, Shift, Up, Down, Left, Right only (no Enter, Esc, Tab, F-keys, Ctrl, Alt).

## How to test

Ask the user to turn on **developer mode** (MultiAlt > Scripts > Roblox Player
automation > Developer) and to launch the accounts from MultiAlt first. Then run
from the terminal, using the Python path shown in the Developer tab:

```
"<python>" -m multialt run farm.py --account Main_01
"<python>" -m multialt run farm.py --account Main_01 --account Alt_02 --mode all_together --trigger interval:5
```

- `--mode one_at_a_time` (default) or `all_together`; `--trigger manual` (default), `loop` or `interval:<minutes>`.
- The log prints in the terminal as `[AccountName] message`; errors have the full traceback.
- Exit code: 0 finished or stopped, 1 script failed, 2 could not start (the message says why).
- Stop a run with Ctrl+C in the terminal or the Stop button in the app.
- Developer mode off prints: `Developer mode is off. Turn it on in MultiAlt > Scripts > Roblox Player automation > Developer.`

Loop: write the script, run it, read the error, fix, run again. Keep test runs
short (a few seconds of real input), the user's accounts are real.

## Errors and the fix

| Code | Class | What to change |
|---|---|---|
| `not_foreground` | `NotForeground` | The window was not in front. Put the input inside `with account.focused():`. |
| `no_focus_lease` | `NoFocusLease` | Input without the focus queue. Use `with account.focused():`. |
| `untracked_window` | `UntrackedWindow` | The account has no client opened by MultiAlt. The user must launch it from MultiAlt. |
| `minimized` | `Minimized` | The window is minimized. `account.focused()` restores it before input; for reading, ask the user to un-minimize. |
| `forbidden` | `Forbidden` | You asked for cookies/passwords or an op that does not exist. Remove it. |
| `focus_timeout` | `FocusTimeout` | The queue stayed busy (AFK mode, a recording, another run). Raise `focused(timeout_ms=...)` or shorten how long other code holds the block. |
| `unsupported_key` | `UnsupportedKey` | Use a key from rule 8. For `type_text`, only letters, digits and space. |
| `bad_args` | `BadArgs` | A fraction outside 0-1, a negative time, an empty name. Fix the value. |
| `capture_failed` | `CaptureFailed` | Could not capture the window. It must not be minimized; retry after a `sleep`. |
| `stopped` | `Stopped` | The user stopped the run. Not a bug: let it propagate. |

Python-side errors: `FileNotFoundError: Image not found: ...` means the file is
missing from `assets/`. `RuntimeError: ... numpy and opencv` means the Python
runtime is not installed (the user installs it in the Scripts page).

## Finding positions and colours

You cannot see the screen. Ask the user to open the **Inspector** tab: hovering
shows the `x, y` fraction and RGB; clicking copies `account.click(0.523, 0.811)`;
Shift+click copies `account.wait_pixel(...)`; dragging a rectangle saves an
image to `assets/` for `find_image`. Prefer images/pixels over blind clicks.

## Vision cheat sheet

```python
account.pixel(0.5, 0.8)                                  # (r, g, b)
account.wait_pixel(0.5, 0.8, (255, 200, 0), timeout=10)  # True / False
pos = account.find_image("play_button.png")              # (x, y) fractions or None
pos = account.wait_image("play_button.png", timeout=15)
account.click_image("play_button.png")                   # find + focused click -> bool
frame = account.capture()                                # numpy (h, w, 4) BGRA
```

## Dashboard cheat sheet

```python
from multialt import dashboard
dashboard.define([
    {"id": "coins", "type": "stat", "title": "Coins"},
    {"id": "quest", "type": "progress", "title": "Quest", "max": 50},
    {"id": "state", "type": "badge", "title": "State"},
    {"id": "who", "type": "accounts", "title": "Accounts", "columns": ["Status", "Coins"]},
    {"id": "rate", "type": "line", "title": "Coins per lap"},
])
dashboard.set("coins", 120)
dashboard.set("state", {"level": "ok", "text": "Farming"})
dashboard.set("who", {"Status": "Working", "Coins": 40}, account=account)
dashboard.push("rate", 12.5)
```

Types: `stat`, `progress`, `text`, `badge`, `table`, `accounts`, `line`, `log`
(values in `docs/API.md`). Limits: 40 widgets, 2 KB of text each, 200 table
rows, 500 line points. Quick start: `REPORT = True` plus
`dashboard.report_loop(account)` per lap - see `examples/farm_dashboard.py`.

## Editing blocks (developer mode only)

`multialt/defaults/*.py` (`runner`, `focus`, `vision`, `actions`, `dashboard`)
change how runs behave. Edit one only when the user asks. They can restore any
block to the original with "Restore default"; your scripts are never touched.
The app still enforces every safety rule whatever a block does.

## Do / Don't

- Do put new scripts in `scripts/`, keep one clear purpose each, log what happens with `account.log(...)` and `account.status(...)`.
- Do start from `examples/` or `templates/`.
- Don't edit anything under `multialt/_core`, `docs/`, `examples/`, `templates/`.
- Don't use `subprocess`, other automation libraries or raw `SendInput`/`pyautogui`: they bypass the focus queue and fight the user's other windows.
