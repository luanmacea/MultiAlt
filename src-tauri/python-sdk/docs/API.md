# MultiAlt Python API

Reference for scripts in `scripts/`. The types are in `multialt/__init__.pyi`;
this page covers the same names. Everything is imported from `multialt`.

## Notes

- Windows only. Roblox windows are driven one at a time: input goes to the
  window that is in front, through a single focus queue shared with AFK mode
  and recordings.
- **Python has full access to your PC.** A script is a normal Python program
  running as you: it can read and write your files, start programs and use
  the network, like any program you run. The rules in this page (focus queue,
  window in front, accounts of the run) apply to what goes **through
  MultiAlt's API**; they are not a sandbox. Only run scripts you trust.
- MultiAlt's API never sends cookies or passwords to scripts: `Account` has no
  such field and the app refuses any request for them (`forbidden`, "Not
  available through MultiAlt's API: cookies and passwords are never sent to
  scripts.").
- The terminal channel (`python -m multialt run`) talks to MultiAlt through a
  Windows named pipe opened with overlapped I/O (a plain `open(...)` handle
  deadlocks when one thread reads while another writes).
- Shapes of dashboard values (`accounts`, `stat`, `badge`, ...) are listed
  under "Dashboard" below.

## Quick start

```python
from multialt import Account, sleep, stopped

def main(account: Account) -> None:
    while not stopped():
        with account.focused():
            account.press("Space")
        sleep(30)
```

`main(account)` is called once per account you picked. Or define
`main_all(accounts)`, called once, and you loop over the accounts yourself.
If both exist, `main_all` wins.

## Coordinates

Every position is a **fraction 0-1 of the window's client area**:
`(0.0, 0.0)` is the top-left corner, `(1.0, 1.0)` the bottom-right,
`(0.5, 0.5)` the centre. Scripts keep working when the window is resized.
Use the Inspector tab to read the fractions and colours of what you see.

## Account

`Account` is what `main` receives. Fields: `user_id: int`, `name: str`,
`alias: str`, `group: str`, `in_game: bool`, `place_id: int`, `job_id: str`.
The `Account` your `main` receives is a snapshot from the start of the run;
`multialt.accounts()` asks the app again: `in_game` (a client opened by
MultiAlt, in a game), and the `place_id` / `job_id` the Roblox log reported
for it (`0` / `""` until the log says so, or without a client).
`str(account)` is the name. `Account.from_json(data)` builds one from the
protocol object (you rarely need it).

### Focus

#### `account.focused(timeout_ms=None)`

Context manager. Takes the focus queue, brings the account's window to the
front and confirms it, runs your block, then releases the queue and puts back
the window that was in front before (only when nobody else is using or waiting
for the queue; otherwise the last one to leave does it). **All input must be
inside it.** Keep each block short, one action at a time: while you are inside
it every other account, AFK mode and recordings wait. Wait for pixels and
images outside it (`click_image` already does).

How long it waits for the queue: with `timeout_ms`, that long, then
`FocusTimeout`. Without it, 20 s — except in the `all_together` mode, where
each account waits for its turn as long as it takes (until Stop). Nesting it
for the same account is allowed (it reuses the lease); nesting it for another
account raises `RuntimeError`.

```python
with account.focused():
    account.click(0.5, 0.8)
```

### Input (inside `with account.focused():`)

- `account.press(key, hold_ms=40)` - tap a key, holding it `hold_ms`
  (10-10000) milliseconds. See "Keys".
- `account.key_down(key)` / `account.key_up(key)` - hold a key across several
  steps. Always pair them; MultiAlt releases a forgotten key when the focus
  lease ends.
- `account.type_text(text, delay_ms=30)` - type letters, digits and spaces.
  Other characters raise `UnsupportedKey`.
- `account.click(x, y)` - left click at a point (fractions).

### Reading the screen (no focus needed)

The window is captured even when other windows cover it. A minimized window
cannot be read (`minimized` / `capture_failed`).

- `account.pixel(x, y)` -> `(r, g, b)`.
- `account.wait_pixel(x, y, rgb, tolerance=12, timeout=10.0, interval=0.1)` ->
  `bool`. Polls until every channel is within `tolerance` of `rgb`.
- `account.find_image(asset, threshold=0.85, region=None, scales=None)` ->
  `(x, y)` or `None`. Looks for `assets/<asset>` (OpenCV template matching on
  grayscale, scales 0.75 to 1.25 in steps of 0.05 unless `scales` is given).
  The result is the **centre** of the best match, as fractions of the window.
  `region=(x, y, w, h)` (fractions) limits the search and makes it faster.
  `threshold` is 0-1; higher is stricter.
- `account.wait_image(asset, timeout=10.0, threshold=0.85, region=None, interval=0.5)`
  -> `(x, y)` or `None` after the timeout.
- `account.click_image(asset, timeout=10.0, threshold=0.85, region=None)` ->
  `bool`. `wait_image` then a click in `with account.focused()`.
- `account.capture(region=None)` -> numpy array `(height, width, 4)`, BGRA.

Images live in the workspace `assets/` folder; the Inspector tab saves
cut-outs there. Needs numpy and opencv-python-headless (installed with the
Python runtime).

### Reporting

- `account.status(text)` - the status line shown for this account.
- `account.log(*args, level="log")` - a log line tagged with the account
  (filterable in the log terminal). `level` is `"log"`, `"warn"` or `"error"`.

## Module functions

- `accounts()` -> `list[Account]` - the accounts of this run, fresh from the app.
- `run_context()` -> `RunContext` - `id`, `script`, `mode` (`"one_at_a_time"`
  or `"all_together"`), `trigger` (`{"kind": "manual"|"loop"|"interval", "minutes"?}`),
  `source` (`"app"` or `"terminal"`) and `accounts`.
- `log(*args, level="log")` - same as `print`, with a level. `print()` also
  works and is logged.
- `status(text)` - the status line of the whole run.
- `sleep(seconds)` - sleeps; wakes immediately and raises `Stopped` on Stop.
  **Use it instead of `time.sleep`.**
- `stopped()` -> `bool` - True once Stop was pressed. Check it in loops:
  `while not stopped():`.

## Errors

All are subclasses of `MultiAltError` (fields `code`, `message`, `hint`):
`NotForeground`, `NoFocusLease`, `UntrackedWindow`, `Minimized`, `Forbidden`,
`FocusTimeout`, `UnsupportedKey`, `BadArgs`, `CaptureFailed`, `Stopped`.

| Class | Code | Meaning and fix |
|---|---|---|
| `NotForeground` | `not_foreground` | Input to a window that is not in front. Wrap input in `with account.focused():`. |
| `NoFocusLease` | `no_focus_lease` | Input without taking the focus queue. Use `with account.focused():`. |
| `UntrackedWindow` | `untracked_window` | The account has no Roblox client opened by MultiAlt. Launch it from MultiAlt first. |
| `Minimized` | `minimized` | The window is minimized. `account.focused()` restores it before input. |
| `Forbidden` | `forbidden` | Asked MultiAlt's API for something it never sends to scripts (cookies, passwords), or for an op that does not exist. Not fixable. |
| `FocusTimeout` | `focus_timeout` | The focus queue stayed busy (AFK mode or a recording running). Raise `timeout_ms` or try again. |
| `UnsupportedKey` | `unsupported_key` | The key is not in the "Keys" list, or `type_text` got a character with no key. |
| `BadArgs` | `bad_args` | A value is out of range or the wrong type (a fraction outside 0-1, a negative time). |
| `CaptureFailed` | `capture_failed` | The window could not be captured. Make sure it is not minimized. |
| `Stopped` | `stopped` | The run was stopped. Do not catch it; let it end the script. |
| `MultiAltError` | `internal` | MultiAlt itself failed while handling the call (a bug in the app, not in your script). The run goes on; report it. |

`Stopped` is an ordinary exception, so never write a bare `except:` or
`except Exception: pass` around a loop: after a stop every later call raises
`Stopped` again, but swallowing it hides the stop for a moment. If you must
catch errors, catch `MultiAltError` subclasses by name and re-raise `Stopped`.

## Keys

Names are case-insensitive: `A`-`Z`, `0`-`9`, `Space`, `Shift`, `Up`, `Down`,
`Left`, `Right`. Enter, Tab, Escape, function keys, Alt, Ctrl, Windows,
Backspace, Delete, `/` and the backtick are **not** available (they open
menus, chat or can close the client).

## Dashboard

`dashboard` draws your own report in the Dashboard tab, one section per
script, kept after the run ends until you press Clear.

- `dashboard.define(layout)` - the widgets, as a list of
  `{"id", "type", "title", "columns"?, "max"?}`. Limit 40 widgets.
- `dashboard.set(widget_id, value, account=None)` - a value for the whole
  script, or for one account (`account` is an `Account` or a user id).
- `dashboard.push(widget_id, value)` - add a point to a `line` widget (keeps the
  last 500) or a line to a `log` widget.
- `dashboard.clear()` - remove this script's widgets and values.
- `dashboard.farm_report()` - the ready-made report (see below).
- `dashboard.report_status(account, text)`, `dashboard.report_loop(account)`,
  `dashboard.report_error(account, err)` - feed the farm report.

Widget types and the value each takes (text is plain text, up to 2 KB):

| `type` | Value for `set` / `push` |
|---|---|
| `stat` | a number, or `{"value": 12, "delta": 3}` for a change arrow |
| `progress` | a number from 0 to `max` (default 100) |
| `text` | a string |
| `badge` | `{"level": "ok"\|"warn"\|"error", "text": "Running"}` |
| `table` | `set` a list of rows, each a list of cells matching `columns` (max 200 rows) |
| `accounts` | one `set(..., account=acc)` per account: an object keyed by column title, for example `{"Status": "Working", "Loops": 3}`; `columns` lists the titles |
| `line` | `push` a number per point (last 500 kept) |
| `log` | `push` a string per line |

```python
dashboard.define([
    {"id": "coins", "type": "stat", "title": "Coins"},
    {"id": "progress", "type": "progress", "title": "Quest", "max": 50},
])
dashboard.set("coins", 120)
dashboard.set("progress", 12)
```

`dashboard.farm_report()` defines an `accounts` table (Status, Loops, Last
error, Running for), a `stat` called "Total loops" and a `line` called
"Loops per hour". With `REPORT = True` at the top of your script the runner
calls it for you, sets each account's status when it starts and ends, records
errors, and counts one loop each time `main(account)` returns. If your `main`
loops forever, call `dashboard.report_loop(account)` once per lap.

## Running a script

- In the app: Scripts > Roblox Player automation > pick the script, the
  accounts, a mode and a trigger > Run.
  - Mode: *one at a time* (account 1 finishes, then 2, ...) or *all together*
    (one thread per account; input still takes turns through the focus queue).
  - Trigger: *manual*, *every N minutes* (the app starts the script again N
    minutes after it finished) or *loop* (your script loops).
- From a terminal (developer mode on):

  ```
  "<python>" -m multialt run farm.py --account Main_01 --account Alt_02 --mode all_together --trigger loop
  ```

  `--trigger` is `manual` (default) or `loop`. A terminal run happens once, so
  `interval:<minutes>` is refused: for "every N minutes" use the Run button in
  the app. The exact `<python>` path is shown in the Developer tab. The run appears in
  the app as "terminal" and its log shows in both places. Exit code: 0 done or
  stopped, 1 the script failed, 2 it could not start.

## Default blocks (developer mode)

`multialt/defaults/` holds the parts you can rewrite, one by one, each with a
"Restore default" button: `runner.py` (calls your `main`, one at a time or all
together), `focus.py` (`account.focused()`), `vision.py` (pixels and images),
`actions.py` (keys and clicks, `jitter_sleep(base_ms, spread_ms)`), and
`dashboard.py` (`farm_report`). MultiAlt keeps enforcing its safety rules
whatever the blocks do.

## Low-level primitives

`multialt/_core/primitives.py` has one function per app operation (for people
rewriting blocks): `accounts`, `windows`, `focus_acquire`, `focus_renew`,
`focus_release`, `window_bring_forward`, `window_restore_focus`, `input_key`,
`input_key_down`, `input_key_up`, `input_type_text`, `input_click`,
`screen_pixel`, `screen_capture`, `run_sleep`, `run_stopped`, `run_log`,
`run_status`, `dashboard_define`, `dashboard_set`, `dashboard_push`,
`dashboard_clear`. Do not edit `multialt/_core`: it is rewritten on every start.
