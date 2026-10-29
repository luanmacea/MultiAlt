# Roblox Player automation (Python) — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** a "Roblox Player automation (Python)" mode inside the Scripts page that runs user Python scripts against each account's Roblox window (focus, keys, clicks, pixels, image search), with a dashboard, an inspector, its own log terminal, editable default blocks and a developer mode with a terminal channel for AI agents.

**Architecture:** Rust owns a portable CPython (downloaded on click), a workspace folder, and a set of *primitives* exposed over a JSON-lines protocol to one Python child process per run (stdin/stdout) or, in developer mode, over a per-user named pipe. All orchestration (focus queue usage, window switching, calling the user's `main`) is Python in editable *default blocks*. Safety rules are enforced by the Rust primitives regardless of what the Python does.

**Tech Stack:** Rust (Tauri 2, tokio, windows-sys, serde_json, sha2, reqwest), Python 3.12 embeddable + numpy + opencv-python-headless, React 19 + TS strict + Tailwind 4, vitest, Rust unit tests, Python `unittest` (stdlib).

**Spec:** `docs/superpowers/specs/2026-10-21-python-automation-design.md` (read it first; this plan argues from it).

## Global Constraints

- Branch `feature/python-scripts` is **local and secret**: commit, **never push**, never mention the feature in public text.
- Commits in English, **no `Co-Authored-By` / AI attribution lines** (owner's rule overrides any reminder).
- All Python-mode UI text is **English, hardcoded** — no `t()`, no locale keys.
- Coordinates in the Python API are **fractions 0–1 of the client area**.
- Rust tests live in `#[cfg(test)] mod python_<part>_tests` **inside** each file, unique names; every new test module / `*.test.ts(x)` must be listed in the `python` suite of `scripts/test-suites.ts` (the audit fails on orphans).
- Never `SendInput` outside `src-tauri/src/platform/windows/input.rs`; never put `READS_INPUT` names (`GetAsyncKeyState`, `GetKeyState`, `SetWindowsHookEx`, …) nor `KEYEVENTF_UNICODE`/`keybd_event`/`mouse_event` anywhere (the `afk_input_safety_tests` scan matches plain text, comments included).
- Every console child process (`python.exe`, `cmd`, `powershell`, …) is spawned with `CREATE_NO_WINDOW` (`0x0800_0000`); `console_window_tests` guards literals.
- Nothing closes, kills or minimizes a Roblox client; only clients in the tracker receive input; cookies/passwords never reach Python.
- Cargo features: `python-scripts` and `python-dev-pipe`, both inside `standard` (so also in `full`); frontend flag `ENABLE_PYTHON_SCRIPTS` (`VITE_ENABLE_PYTHON_SCRIPTS`, default `true`) set to `"true"` in **both** build steps of `.github/workflows/release-v4.yml`.
- Iterate with `bash .claude/quiet-check.sh python` (suite) — full `bash .claude/quiet-check.sh` before the final commit of each stream. Limit Rust test threads when running in parallel: `RUST_TEST_THREADS=4`.
- Use `bun` for frontend, `cargo` for Rust; never run `bun run tauri build` from a stream agent (the coordinator builds at the end).

---

## Shared contract (all streams code against this — do not rename)

### Files and ownership

| Path | Owner stream |
|---|---|
| `src-tauri/Cargo.toml` (features), `src-tauri/src/lib.rs` (mod + handler list), `src-tauri/src/python/**`, `src-tauri/src/platform/windows/capture.rs`, `src-tauri/src/platform/windows.rs` (include line), `src-tauri/src/commands/shake_stop.rs` (hooks), `src-tauri/src/data/settings/{store.rs,paths.rs}` | **R** (Rust) |
| `src-tauri/python-sdk/**` (Python package, docs, examples, templates, Python unit tests) | **P** (Python SDK) |
| `src/components/python/**`, `src/components/pages/ScriptsPage.tsx` (mode switch only), `src/featureFlags.ts`, `src/dev/harness/pythonScenarios.ts` + 2-line hook in `scenarios.ts` | **F** (Frontend) |
| `scripts/test-suites.ts` (`python` suite), `.github/workflows/release-v4.yml` (flag), `docs/features/python-automation.md`, `docs/README.md`, `docs/architecture.md` | **R** creates the suite entry with its own modules; **F** and **P** append their entries (edit only your lines; re-read the file right before editing) |

### SDK file list (embedded by R with `include_str!`, written by P)

Paths relative to `src-tauri/python-sdk/`. **App-owned** = rewritten on every app start. **Block** = default block, editable in developer mode, restorable one by one.

| File | Kind |
|---|---|
| `multialt/__init__.py` | app-owned (public API: `Account`, `accounts()`, `log`, `status`, `sleep`, `stopped`, `dashboard`) |
| `multialt/__init__.pyi` | app-owned (types) |
| `multialt/_core/__init__.py` | app-owned |
| `multialt/_core/protocol.py` | app-owned (JSON-lines transport, request/response, errors → `MultiAltError` subclasses) |
| `multialt/_core/host.py` | app-owned (entry `python -m multialt._core.host <script>`; swaps stdout/stderr for log writers **before** importing the user script) |
| `multialt/_core/primitives.py` | app-owned (thin typed wrappers of every op) |
| `multialt/cli.py` + `multialt/__main__.py` | app-owned (`python -m multialt run <script> --account NAME ... [--mode] [--trigger]`, connects to the pipe) |
| `multialt/defaults/__init__.py` | app-owned |
| `multialt/defaults/runner.py` | **block** |
| `multialt/defaults/focus.py` | **block** |
| `multialt/defaults/vision.py` | **block** |
| `multialt/defaults/actions.py` | **block** |
| `multialt/defaults/dashboard.py` | **block** |
| `AGENTS.md`, `CLAUDE.md`, `docs/API.md` | app-owned |
| `examples/anti_afk_pixel.py`, `examples/click_button_image.py`, `examples/routine_all_accounts.py`, `examples/farm_dashboard.py` | app-owned |
| `templates/blank.py`, `templates/per_account.py`, `templates/controller.py`, `templates/dashboard.py` | app-owned (used by "New script") |
| `tests/test_runner.py`, `tests/test_vision.py`, `tests/test_protocol.py`, `tests/fake_transport.py` | repo-only (not embedded) |

Workspace on disk: `<get_runtime_data_dir()>/python/` → `scripts/`, `lib/`, `assets/`, plus the app-owned and block files above. Runtime: `<app_local_data_dir>/python-runtime/` (not backed up). Run configs: `<workspace>/runs.json`. Dashboard: `<workspace>/dashboard.json`. Block originals' hashes: `<workspace>/multialt/defaults/.originals.json` (`{ "runner.py": "<sha256 of the original the user started from>" }`).

### Protocol (JSON lines, UTF-8, one object per line)

App → Python, first line: `{"event":"hello","run":{"id":"r1","script":"farm.py","mode":"one_at_a_time","trigger":{"kind":"manual"},"source":"app"},"accounts":[Account...]}`
App → Python later: `{"event":"stop"}`
Python → App request: `{"id":7,"op":"screen.pixel","args":{"user_id":123,"x":0.5,"y":0.8}}`
App → Python response: `{"id":7,"ok":true,"result":[255,200,0]}` or `{"id":7,"ok":false,"error":{"code":"not_foreground","message":"...","hint":"..."}}`
Fire-and-forget (no response): requests with `"id":null` — used by `run.log` and `run.status`.

`Account` JSON: `{"user_id":123,"name":"Main_01","alias":"","group":"Default","in_game":true,"place_id":0,"job_id":""}` — never cookie/password.

Ops (exact):

| op | args | result |
|---|---|---|
| `accounts` | `{}` | `[Account]` (only the run's accounts) |
| `windows` | `{}` | `[{"user_id","hwnd","x","y","width","height","dpi","minimized","foreground"}]` (client area, screen px) |
| `focus.acquire` | `{"user_id","timeout_ms"}` | `{"lease":"L1"}` |
| `focus.renew` | `{"lease"}` | `true` |
| `focus.release` | `{"lease"}` | `true` |
| `window.bring_forward` | `{"user_id"}` | `true` (confirmed in front) |
| `window.restore_focus` | `{}` | `"restored"\|"user_moved_on"\|"gone"\|"denied"` |
| `input.key` | `{"user_id","key","hold_ms"}` | `true` |
| `input.key_down` / `input.key_up` | `{"user_id","key"}` | `true` |
| `input.type_text` | `{"user_id","text","delay_ms"}` | `true` (only chars mappable to the recording key list; else `unsupported_key`) |
| `input.click` | `{"user_id","x","y"}` | `true` (left button) |
| `screen.pixel` | `{"user_id","x","y"}` | `[r,g,b]` |
| `screen.capture` | `{"user_id","region":{"x","y","w","h"}?}` | `{"shm":"Local\\multialt-cap-<run>-<n>","width","height","stride","format":"BGRA"}` |
| `run.sleep` | `{"ms"}` | `true`, or error `stopped` |
| `run.stopped` | `{}` | `bool` |
| `run.log` | `{"level":"log\|warn\|error","message","user_id"?}` | — (fire-and-forget) |
| `run.status` | `{"user_id"?,"text"}` | — (fire-and-forget) |
| `dashboard.define` | `{"layout":[Widget]}` | `true` |
| `dashboard.set` | `{"id","value","user_id"?}` | `true` |
| `dashboard.push` | `{"id","value"}` | `true` |
| `dashboard.clear` | `{}` | `true` |

Key names = `data::recordings::RECORDING_KEYS` names (case-insensitive). `Widget` = `{"id","type":"stat|progress|text|badge|table|accounts|line|log","title","columns"?: [str], "max"?: number}`.

Error codes (exact) with English messages (`{name}` = account name):

| code | message | hint |
|---|---|---|
| `not_foreground` | `Blocked: {name}'s window is not in front. Input only goes to the focused window.` | `Wrap input in "with account.focused():".` |
| `no_focus_lease` | `Blocked: take the focus queue first, so other runs, AFK mode and recordings don't type into the same window.` | `Use "with account.focused():".` |
| `untracked_window` | `Blocked: {name} has no Roblox client opened by MultiAlt.` | `Launch the account from MultiAlt first.` |
| `minimized` | `{name}'s window is minimized.` | `"account.focused()" restores it before input.` |
| `forbidden` | `Not available to scripts: cookies and passwords never leave MultiAlt.` | — |
| `focus_timeout` | `Waited {ms} ms for the focus queue (AFK mode or a recording may be running).` | `Raise timeout_ms or try again.` |
| `unsupported_key` | `Key "{key}" is not supported.` | `See docs/API.md, "Keys".` |
| `bad_args` | `Invalid arguments for {op}: {detail}` | — |
| `capture_failed` | `Could not capture {name}'s window ({detail}).` | `Make sure the window is not minimized.` |
| `stopped` | `Run stopped.` | — |

### Tauri commands (all registered always; stubs when the feature is off return `Err("Python automation is not available in this build")`, read-only ones return empty values)

```rust
// src-tauri/src/python/types.rs  (R writes; F mirrors in pythonApi.ts)
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)] #[serde(rename_all = "camelCase")]
pub struct PythonRuntimeStatus { pub installed: bool, pub version: Option<String>, pub python_exe: Option<String>, pub packages: Vec<String>, pub installing: bool }
#[derive(Serialize, Clone, Debug)] #[serde(rename_all = "camelCase")]
pub struct PythonInstallProgress { pub stage: String /* "python"|"pip"|"packages"|"ready"|"error" */, pub message: String, pub downloaded: u64, pub total: u64 }
#[derive(Serialize, Clone, Debug)] #[serde(rename_all = "camelCase")]
pub struct PythonWorkspaceInfo { pub root: String, pub scripts_dir: String, pub assets_dir: String, pub python_exe: Option<String>, pub cli_command: String }
#[derive(Serialize, Clone, Debug)] #[serde(rename_all = "camelCase")]
pub struct PythonScriptFile { pub name: String, pub size: u64, pub modified_ms: u64 }
#[derive(Serialize, Clone, Debug)] #[serde(rename_all = "camelCase")]
pub struct DefaultBlockInfo { pub name: String, pub modified: bool, pub new_default_available: bool }
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)] #[serde(tag = "kind", rename_all = "camelCase")]
pub enum PythonTrigger { Manual, Interval { minutes: u32 }, Loop }
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)] #[serde(rename_all = "snake_case")]
pub enum PythonRunMode { OneAtATime, AllTogether }
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)] #[serde(rename_all = "camelCase")]
pub struct PythonRunConfig { pub script: String, pub accounts: Vec<i64>, pub mode: PythonRunMode, pub trigger: PythonTrigger }
#[derive(Serialize, Clone, Debug)] #[serde(rename_all = "camelCase")]
pub struct PythonRunInfo { pub id: String, pub script: String, pub accounts: Vec<i64>, pub mode: PythonRunMode, pub trigger: PythonTrigger, pub source: String /* "app"|"terminal" */, pub state: String /* "starting"|"running"|"waiting"|"stopping"|"finished"|"failed" */, pub started_ms: u64, pub ended_ms: Option<u64>, pub next_run_ms: Option<u64>, pub error: Option<String> }
#[derive(Serialize, Clone, Debug)] #[serde(rename_all = "camelCase")]
pub struct PythonLogLine { pub run_id: Option<String>, pub user_id: Option<i64>, pub level: String /* "log"|"warn"|"error"|"system"|"blocked" */, pub message: String, pub code: Option<String>, pub hint: Option<String>, pub ts: u64 }
#[derive(Serialize, Deserialize, Clone, Debug, Default)] #[serde(rename_all = "camelCase")]
pub struct DashboardSection { pub script: String, pub layout: Vec<serde_json::Value>, pub values: serde_json::Map<String, serde_json::Value> /* "id" or "id@userId" */, pub series: serde_json::Map<String, serde_json::Value> /* id -> [[ts, v]...] max 500 */, pub updated_ms: u64 }
#[derive(Serialize, Clone, Debug)] #[serde(rename_all = "camelCase")]
pub struct InspectorFrame { pub width: u32, pub height: u32, pub png_base64: String }
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FracRect { pub x: f64, pub y: f64, pub w: f64, pub h: f64 }
```

Commands (module `python::commands`, names exact):
`python_runtime_status`, `python_install_runtime(app)`, `python_reinstall_runtime(app)`, `python_workspace_info`, `python_list_scripts`, `python_read_script(name)`, `python_write_script(name, source)`, `python_create_script(name, template)`, `python_delete_script(name)`, `python_list_default_blocks`, `python_read_default_block(name)`, `python_write_default_block(name, source)`, `python_restore_default_block(name)`, `python_open_workspace_folder`, `python_open_in_editor(name)`, `python_get_run_configs -> HashMap<String, PythonRunConfig>`, `python_save_run_config(config)`, `python_start_run(app, config) -> String`, `python_stop_run(run_id)`, `python_stop_all_runs`, `python_list_runs -> Vec<PythonRunInfo>`, `python_get_dashboard -> Vec<DashboardSection>`, `python_clear_dashboard(script)`, `python_inspector_capture(user_id) -> InspectorFrame`, `python_save_asset(user_id, rect: FracRect, name) -> String`, `python_set_developer_mode(app, enabled)`, `python_get_logs -> Vec<PythonLogLine>` (last 5000).

Events: `python-install-progress` (PythonInstallProgress), `python-log` (PythonLogLine), `python-run-update` (PythonRunInfo), `python-dashboard-update` (DashboardSection, throttled ≤ 4/s per script).

Script names: `^[A-Za-z0-9_\-]{1,64}\.py$` inside `scripts/` only (no path separators) — R validates, F mirrors.

---

## Stream R — Rust backend (one agent, sequential; Opus)

### Task R1: Scaffold, features, contract, stubs

**Files:** Modify `src-tauri/Cargo.toml`, `src-tauri/src/lib.rs`, `src-tauri/src/data/settings/store.rs`, `src-tauri/src/data/settings/paths.rs`, `scripts/test-suites.ts`, `.github/workflows/release-v4.yml`; Create `src-tauri/src/python/{mod.rs,types.rs,commands.rs,errors.rs}`.

**Produces:** every command above registered in `generate_handler!` as `python::commands::<name>`; `python::types::*`; `python::errors::{PyError, PyErrorCode}` with `fn to_json(&self) -> serde_json::Value` producing `{"code","message","hint"}` exactly per the table; `python::emergency_stop()` and `python::runs_active() -> usize` (no-ops returning 0 without the feature).

- [ ] Features: `python-scripts = []`, `python-dev-pipe = ["python-scripts"]`, add both to `standard`. Check `.github/scripts/release-workflow.test.mjs` still passes (`bun run t release`).
- [ ] Setting default `[Python] DeveloperMode = "false"` in `apply_defaults` + `documented_defaults()` test table.
- [ ] `paths.rs`: `pub fn get_python_workspace_dir() -> PathBuf` (`get_runtime_data_dir().join("python")`); add `"python"` to `DATA_DIRS` **but** make backups/migration skip nothing else (runtime lives elsewhere).
- [ ] Write failing test `python_errors_tests::messages_match_the_contract` asserting each code's message/hint text for a sample name; implement `errors.rs`; pass.
- [ ] Write failing test `python_types_tests::trigger_and_mode_serialize_as_the_frontend_expects` (`{"kind":"interval","minutes":5}`, `"one_at_a_time"`); pass.
- [ ] Stubs for all commands (feature off) and empty real bodies returning `Err("not implemented yet")` under the feature, so the handler list compiles both ways: `cargo check --no-default-features --features standard` and `cargo check --no-default-features` both green.
- [ ] Add `python` suite to `scripts/test-suites.ts`: `{ description: "Automação do Roblox Player em Python: runtime, workspace, protocolo, primitivas, execuções, pipe, SDK e tela", rust: ["python_"], front: ["src/components/python"] }` (the `python_` filter matches every `python_*_tests` module).
- [ ] `release-v4.yml`: `VITE_ENABLE_PYTHON_SCRIPTS: "true"` in both build steps.
- [ ] `bash .claude/quiet-check.sh python` and `bun run t release` green → commit `Python automation: scaffold commands, types, errors and feature flags`.

### Task R2: Portable runtime installer (`python/runtime.rs`)

**Consumes:** `api::http_client::download_builder()`, `platform::windows::versions::verify_hash` pattern (sha256), Tauri `app_local_data_dir`.
**Produces:** `pub fn runtime_dir(app) -> PathBuf`, `pub fn python_exe(app) -> Option<PathBuf>`, `pub async fn install(app, on_progress: impl FnMut(PythonInstallProgress)) -> Result<(), String>`, `pub fn status(app) -> PythonRuntimeStatus`.

- [ ] Constants: CPython **3.12.x embeddable amd64** URL from python.org + SHA-256; `get-pip.py` from a **pinned** `bootstrap.pypa.io` versioned URL + SHA-256 (look both up on the official pages; put the exact values in constants and in the doc). Packages pinned with hashes in a requirements string: `numpy==<ver> --hash=sha256:...`, `opencv-python-headless==<ver> --hash=...` (cp312 win_amd64 wheels).
- [ ] Steps: download zip (progress) → verify hash → extract to `python-runtime/.tmp` (zip-slip safe, like chromium `extract_archive`) → edit `python312._pth`: uncomment `import site`, add `Lib\site-packages` and the workspace dir line → run `get-pip.py` → `python -m pip install --require-hashes -r <tmp req>` → atomic rename `.tmp` → `python-runtime/current` → write `version.json`.
- [ ] Every spawn: `CREATE_NO_WINDOW`, stdout/stderr captured and forwarded as `PythonInstallProgress.message` lines.
- [ ] Tests (mock HTTP server like other download tests, or inject a fetch trait): `python_runtime_tests::hash_mismatch_aborts_and_leaves_no_runtime`, `::pth_file_enables_site_and_workspace`, `::status_reports_not_installed_on_a_fresh_dir`, `::reinstall_keeps_the_workspace`.
- [ ] Wire `python_runtime_status`, `python_install_runtime` (emits `python-install-progress`; refuses a second concurrent install), `python_reinstall_runtime`. Commit.

### Task R3: Workspace and default blocks (`python/workspace.rs`)

**Produces:** `pub const SDK_FILES: &[SdkFile]` (`SdkFile { path: &str, contents: &str, kind: SdkKind::AppOwned | SdkKind::Block }`) built with `include_str!("../../python-sdk/...")` for every embedded file in the table; `pub fn sync_workspace(root) -> Result<SyncReport, String>`; `list_blocks`, `read_block`, `write_block`, `restore_block`; script CRUD with name validation; templates.

- [ ] If a listed SDK file does not exist yet (stream P still writing), create a minimal placeholder in `src-tauri/python-sdk/` so `include_str!` compiles; P overwrites it.
- [ ] `sync_workspace`: create `scripts/ lib/ assets/`; write every app-owned file; for each block: missing → write + record original hash; unmodified (hash == recorded original) and embedded changed → overwrite + update hash; modified → keep, `new_default_available = embedded hash != recorded original`.
- [ ] `restore_block(name)`: write embedded contents, record hash. Only names in `SDK_FILES` with `Block` kind; anything else → `bad_args`.
- [ ] `python_write_default_block` refuses when `Python.DeveloperMode` is false (`"Turn on developer mode to edit default blocks."`).
- [ ] Tests: `python_workspace_tests::sync_creates_folders_and_app_files`, `::unmodified_block_is_updated_on_new_version`, `::modified_block_is_kept_and_flagged`, `::restore_one_block_leaves_the_others`, `::script_names_reject_paths_and_bad_extensions`, `::create_from_template_writes_the_template`.
- [ ] Wire CRUD/blocks/open-folder (`explorer`)/open-in-editor (`ShellExecuteW "open"` on the file; no console) commands; `sync_workspace` runs on `python_workspace_info` and before every run. Commit.

### Task R4: Capture, focus lease and primitives (`platform/windows/capture.rs`, `python/focus.rs`, `python/primitives.rs`)

**Consumes:** `platform::windows::{tracker, find_main_window, get_roblox_pids, window_exists, get_foreground_hwnd, bring_forward_for_cycle, give_focus_back_after_cycle, window_is_minimized, focus_window}`, `input::{client_rect_on_screen, press_recording_key, click_recording_point}`, `data::recordings::recording_key`, `AFK_CYCLE_LOCK` (crate root, `afk.rs`).
**Produces:**
```rust
pub struct RunCtx { pub run_id: String, pub accounts: Vec<AccountView>, pub stop: Arc<AtomicBool>, pub sink: Arc<dyn RunSink> }
pub trait RunSink: Send + Sync { fn log(&self, line: PythonLogLine); fn status(&self, user_id: Option<i64>, text: &str); fn dashboard(&self, op: &str, args: &Value) -> Result<Value, PyError>; }
pub async fn dispatch(ctx: &RunCtx, op: &str, args: Value) -> Result<Value, PyError>;
pub trait Desk: Send + Sync { /* window lookup, foreground, bring_forward, key, click, capture — real impl on Windows, fake in tests */ }
```
- [ ] `capture.rs`: `pub fn capture_client_bgra(hwnd) -> Result<(u32,u32,Vec<u8>), String>` via `PrintWindow(hwnd, hdc, PW_RENDERFULLCONTENT=2)` into a DIB section + `GetDIBits`; fall back to `BitBlt` from the screen DC when the window is foreground and the result is all-black. Add the needed `windows-sys` features (`Win32_Storage_Xps` for `PrintWindow`, `Win32_System_Memory` for file mapping). `pub struct SharedFrame` = named file mapping `Local\multialt-cap-<run>-<n>` (`CreateFileMappingW` + `MapViewOfFile`), kept alive until the next capture of that run or run end. **Spike first** (P0 of the spec): prove PrintWindow returns non-black pixels for a DirectX window behind another window (use any open DX/Chromium window if no Roblox client is available) and that Python's `mmap.mmap(-1, n, tagname=...)` reads the mapping; record the result in the doc. No `READS_INPUT` strings in this file.
- [ ] `focus.rs`: `FocusQueue` holding `Option<(lease_id, run_id, user_id, MutexGuard<'static, ()>, last_renew, held_keys: Vec<String>)>` from `AFK_CYCLE_LOCK.lock()`; `acquire(run, user, timeout) -> Result<String, PyError>` (`focus_timeout`), `renew`, `release` (releases held keys in reverse with `press_recording_key(key, true)`), watchdog: lease idle > 30 s → auto-release + `system` log; `release_all_for_run(run_id)`.
- [ ] `primitives.rs` `dispatch`: arg parsing (`bad_args`), account membership (only run accounts; others → `untracked_window` with the name if known), `forbidden` for any op name containing `cookie`/`password`/`token` or not in the table, input gate order: lease held by this run for this user (`no_focus_lease`) → tracked window exists (`untracked_window`) → not minimized (`minimized`) → foreground == hwnd (`not_foreground`) → act. First click after each `window.bring_forward` uses `focus_click = true`. `run.sleep` in 25 ms slices checking `ctx.stop`. Pixel = 1×1 region of the capture.
- [ ] Tests with a fake `Desk`: `python_primitives_tests::input_without_lease_is_blocked`, `::input_to_background_window_is_blocked`, `::untracked_account_is_blocked`, `::secrets_are_forbidden`, `::lease_expiry_releases_held_keys`, `::sleep_stops_when_the_run_is_stopped`, `::pixel_reads_the_fraction_point`, `::type_text_rejects_unmapped_chars`, `::focus_queue_is_shared_with_afk` (holding `AFK_CYCLE_LOCK` makes `focus.acquire` time out).
- [ ] `bash .claude/quiet-check.sh afk` stays green (safety scan). Commit.

### Task R5: Bridge, runs, dashboard store, shake stop (`python/bridge.rs`, `python/runs.rs`, `python/dashboard.rs`, `commands/shake_stop.rs`)

**Consumes:** R2 `python_exe`, R3 `sync_workspace`, R4 `dispatch`/`RunCtx`/`FocusQueue`.
**Produces:** `pub async fn serve<R: AsyncBufRead + Unpin, W: AsyncWrite + Unpin>(ctx: RunCtx, hello: Value, reader: R, writer: W) -> Result<(), String>` (shared by stdio and pipe); `RunManager` (`start`, `stop`, `stop_all`, `list`, interval scheduler); `DashboardStore` (persist `dashboard.json`, limits, throttle).

- [ ] Spawn: `"<python_exe>" -u -X utf8 -m multialt._core.host scripts/<name>` with cwd = workspace, env `PYTHONIOENCODING=utf-8`, `PYTHONDONTWRITEBYTECODE=1`, `CREATE_NO_WINDOW`, piped stdin/stdout/stderr. stderr lines → `python-log` level `error`. Exit code ≠ 0 → run `failed` with the last traceback line as `error`.
- [ ] Stop: send `{"event":"stop"}`, wait 3 s, then kill **the python process** (and its tree via `TerminateJobObject` on a job object with `KILL_ON_JOB_CLOSE`); `FocusQueue::release_all_for_run`.
- [ ] Interval trigger: after a run finishes, `next_run_ms = now + minutes`; scheduler re-starts it if not stopped by the user. Loop trigger: nothing special (script loops).
- [ ] Log ring buffer (5000) behind `python_get_logs`; every line also emitted as `python-log`. Blocked primitive errors are logged with level `blocked`, `code`, `hint`.
- [ ] Shake stop: `shake_watch_needed()` also true when `python::runs_active() > 0`; `trigger_emergency_stop` calls `python::emergency_stop()` (stop all runs) and includes `"python": <n>` in the payload; call `ensure_shake_watch(app)` at run start.
- [ ] Dashboard: `define/set/push/clear` per script section; limits (40 widgets, 2 KB text, 200 table rows, 500 points); throttled emit ≤ 4/s; persisted on change (debounced 1 s); `python_get_dashboard`, `python_clear_dashboard`.
- [ ] Tests: `python_bridge_tests::response_matches_request_id`, `::malformed_line_is_logged_not_fatal`, `::fire_and_forget_log_has_no_response`, `::stop_event_is_sent_first`; `python_runs_tests::two_runs_coexist`, `::interval_reschedules_after_finish`, `::stop_all_stops_every_run`; `python_dashboard_tests::limits_are_enforced`, `::series_keeps_last_500`, `::sections_persist_and_clear`; plus `python_e2e_tests::real_python_round_trip` (skips if `python` is not on PATH; spawns real `python` with the SDK `multialt/_core` from `src-tauri/python-sdk` and a fake Desk, runs a tiny script doing `accounts()`, `log`, `dashboard.set`, asserts lines). Commit.

### Task R6: Developer mode and named pipe (`python/devpipe.rs`)

- [ ] `python_set_developer_mode(app, enabled)`: writes `Python.DeveloperMode`; starts/stops the pipe server (feature `python-dev-pipe`; without it the setting still unlocks block editing but the pipe reports "not available in this build").
- [ ] Server: `tokio::net::windows::named_pipe::ServerOptions` with `reject_remote_clients(true)`, security attributes from SDDL `D:P(A;;GA;;;<current user SID>)` (`ConvertStringSecurityDescriptorToSecurityDescriptorW`); name `\\.\pipe\multialt-python-<fnv1a64 of the user SID, hex>`; write the name to `<workspace>/multialt/_core/pipe.txt` while on, delete when off.
- [ ] Client handshake: first line `{"op":"cli.start","args":{"script":"farm.py","accounts":["Main_01"],"mode":"one_at_a_time","trigger":{"kind":"manual"}}}` → app resolves names → registers a run with `source:"terminal"` → replies with the `hello` line → `serve(...)` over the pipe. Unknown account names → error line and close.
- [ ] Tests: `python_devpipe_tests::pipe_name_is_stable_per_user`, `::pipe_file_written_only_while_enabled`, `::cli_start_with_unknown_account_is_refused`, `::cli_run_is_listed_as_terminal` (in-process client). Commit.

### Task R7: Docs + full check

- [ ] `docs/features/python-automation.md` (Portuguese, project style: objetivo, onde fica o código, protocolo, travas, execuções, dashboard, modo desenvolvedor, testes, spike result), entries in `docs/README.md` and `docs/architecture.md` (new module + the pipe as an IPC exception only in developer mode).
- [ ] `bash .claude/quiet-check.sh` green → commit.

---

## Stream P — Python SDK (one agent; Sonnet)

All files under `src-tauri/python-sdk/`. **First commit within the first minutes:** create every file of the SDK table with at least a module docstring, so stream R can `include_str!` them.

### Task P1: Core (protocol, host, primitives wrappers, public API)

- [ ] `_core/protocol.py`: `class Transport` (reader/writer of JSON lines, thread-safe request ids, a reader thread dispatching responses to waiting futures and events to handlers); `MultiAltError(code, message, hint)` + subclasses per error code (`NotForeground`, `NoFocusLease`, `UntrackedWindow`, `Minimized`, `Forbidden`, `FocusTimeout`, `UnsupportedKey`, `BadArgs`, `CaptureFailed`, `Stopped`); `stdio_transport()` (dups the original stdout fd **before** host swaps `sys.stdout`) and `pipe_transport(name)` (`open(r"\\.\pipe\" + name, "r+b", buffering=0)`).
- [ ] `_core/host.py`: read `hello`; swap `sys.stdout`/`sys.stderr` for writers that send `run.log` (level `log`/`error`); import the user script by path; hand control to `multialt.defaults.runner.run(module, ctx)`; `{"event":"stop"}` sets a flag, and `Stopped` raised from any call ends the script quietly; uncaught exceptions → full traceback to stderr, exit 1.
- [ ] `_core/primitives.py`: one function per op with type hints; `screen.capture` opens the mapping with `mmap.mmap(-1, size, tagname=shm)` and returns a numpy array `(h, w, 4)` BGRA (copy, then close).
- [ ] `__init__.py`: `Account` (fields from the contract; methods delegating to blocks: `focused()`, `press`, `key_down`, `key_up`, `type_text`, `click`, `pixel`, `wait_pixel`, `find_image`, `wait_image`, `click_image`, `capture`, `status`, `log`), `accounts()`, `log`, `status`, `sleep`, `stopped`, `dashboard` (module proxy). `__init__.pyi` mirrors it exactly.
- [ ] `tests/fake_transport.py` + `tests/test_protocol.py`: ids match, errors map to subclasses, `print()` becomes `run.log`, stop event raises `Stopped` on the next call. Run: `python -m unittest discover -s src-tauri/python-sdk/tests -t src-tauri/python-sdk` → green. Commit.

### Task P2: Default blocks

- [ ] `defaults/focus.py`: `focused(account, timeout_ms=20000)` context manager: `focus.acquire` → `window.bring_forward` → yield → `focus.release` (always) → `window.restore_focus` when it was the last one; renews the lease every 10 s from a timer while inside.
- [ ] `defaults/runner.py`: `run(module, ctx)`: if the module has `main_all(accounts)` → call it once; else `main(account)` per account: `one_at_a_time` = sequential loop; `all_together` = one thread per account, exceptions per account logged with the account name and not stopping the others; feeds `dashboard.farm_report` status/errors when the module opted in (`REPORT = True`). This file is **the** window-switching logic users can edit — heavily commented in English, explaining each step.
- [ ] `defaults/vision.py`: `pixel`, `wait_pixel(account, x, y, rgb, tolerance=12, timeout=10, interval=0.1)`, `find_image(account, asset, threshold=0.85, region=None, scales=(0.75…1.25 step 0.05))` using `cv2.matchTemplate(TM_CCOEFF_NORMED)` on grayscale, returns `(x, y)` fractions of the match centre or `None`; `wait_image`, `click_image` (find → focused click). Assets resolved relative to `assets/`.
- [ ] `defaults/actions.py`: `press`, `type_text`, `click`, `jitter_sleep(base_ms, spread_ms)`.
- [ ] `defaults/dashboard.py`: `farm_report()` defines `accounts` table (Status, Loops, Last error, Running for), `stat` Total loops, `line` Loops per hour; helpers `report_status(account, text)`, `report_loop(account)`, `report_error(account, err)`.
- [ ] Tests `tests/test_runner.py` (sequential order, parallel isolation of failures, `main_all` precedence), `tests/test_vision.py` (synthetic numpy image + template at 0.9 scale found within 2% of the true centre; missing → None; skip if cv2 absent). Commit.

### Task P3: CLI, AI kit, examples, templates

- [ ] `cli.py`/`__main__.py`: `python -m multialt run <script> --account NAME [--account NAME…] [--mode one_at_a_time|all_together] [--trigger manual|loop|interval:<min>]`; reads `_core/pipe.txt` (clear message if missing: `Developer mode is off. Turn it on in MultiAlt > Scripts > Roblox Player automation > Developer.`); sends `cli.start`; then behaves as host over the pipe, also printing logs to the terminal.
- [ ] `AGENTS.md` (+ `CLAUDE.md` = "Read AGENTS.md."): what the folder is, where to write (`scripts/`, `lib/`, `assets/`), never edit `multialt/_core` or app-owned files, blocks editable only if the user enabled developer mode, rules (fractions, `with account.focused():` around input, always `sleep` in loops, check `stopped()`), how to test (`"<python>" -m multialt run ...`, the exact path is shown in the app), every error code and what to change, key list, dashboard widget types with examples.
- [ ] `docs/API.md`: full reference generated by hand from `__init__.pyi` — a test `tests/test_api_doc.py` asserts every public name in `__init__.pyi` appears in `docs/API.md`.
- [ ] Examples and templates per the table (each runnable, commented). Commit.

---

## Stream F — Frontend (one agent; Sonnet)

### Task F1: Contract, flag, mode switch

**Files:** Create `src/components/python/pythonApi.ts`; Modify `src/featureFlags.ts`, `src/components/pages/ScriptsPage.tsx`.

- [ ] `pythonApi.ts`: TS mirrors of every type in the shared contract (camelCase), `PYTHON_EVENTS = { installProgress: "python-install-progress", log: "python-log", runUpdate: "python-run-update", dashboardUpdate: "python-dashboard-update" } as const`, typed wrappers `pythonApi.runtimeStatus()`, … one per command (same arg names as the Rust commands), `isValidScriptName(name)` with the contract regex.
- [ ] `ENABLE_PYTHON_SCRIPTS = parseBooleanEnv(import.meta.env.VITE_ENABLE_PYTHON_SCRIPTS, true)`.
- [ ] `ScriptsPage.tsx`: smallest possible change — a `ScriptsModeSwitch` (new file `src/components/python/ScriptsModeSwitch.tsx`) rendered through `PageShell`'s `toolbar` prop with two options "Roblox Player automation (Python)" and "MultiAlt automation (JavaScript)", each with a one-line description; choice in `localStorage` key `ram_scripts_mode` (try/catch); Python mode renders `<PythonAutomationView />` instead of the JS body. Hidden when `!ENABLE_PYTHON_SCRIPTS`.
- [ ] Tests `src/components/python/ScriptsModeSwitch.test.tsx` (switches, remembers, JS mode unchanged). Commit.

### Task F2: View shell, install card, scripts, editor, run panel, running list

- [ ] `PythonAutomationView.tsx`: explanatory strip (what it does / doesn't) + red-tinted warning **"Python scripts have full access to your PC. Only run scripts you trust."**; sub-tabs **Scripts · Dashboard · Inspector · Developer** (Developer tab always visible; its content explains developer mode when off).
- [ ] `InstallCard.tsx` (not installed / installing with stage + bar / error with retry / installed with version + Reinstall + copyable `pip install` hint); nothing downloads without the click.
- [ ] `ScriptList.tsx` (polls `python_list_scripts` every 2 s while visible; New (template picker: Blank, Per account, Controller, Dashboard), Open folder, Open in editor, Delete with confirm), `ScriptEditor.tsx` (own monospace textarea, Ctrl+S saves, "Changed on disk — reloaded" notice when `modifiedMs` changes and there are no unsaved edits; conflict banner otherwise).
- [ ] `RunPanel.tsx`: account picker (uses `store.accounts` + groups; hide nothing, but **never render accounts in a group named "Principais"** in harness screenshots — real data is the user's), mode radio, trigger (Manual / Every N minutes / Loop), Run / Stop; config saved via `python_save_run_config`. `RunningList.tsx`: all runs (`python-run-update`), source badge (app/terminal), state, next run, Stop, Stop all.
- [ ] Tests: install states, run config persistence, stop, name validation. Commit.

### Task F3: Python terminal (own component)

- [ ] `pythonLogStore.ts`: ring buffer 5000 fed by `python_get_logs` + `python-log`; `PythonTerminal.tsx`: monospace, auto-follow that pauses when scrolled up (with "Jump to latest"), filters (run, account, level: log/warn/error/system/blocked), search, copy visible, clear; `blocked` lines highlighted with code + hint. **Do not import anything from `src/components/console/`.**
- [ ] Test `PythonTerminal.test.tsx` (filtering, blocked line shows hint, follow/pause, 5000 cap) + a test asserting no import from `components/console`. Commit.

### Task F4: Dashboard

- [ ] `DashboardView.tsx` + `widgets/{StatWidget,ProgressWidget,TextWidget,BadgeWidget,TableWidget,AccountsWidget,LineWidget,LogWidget}.tsx`; one section per script with Clear; values from `python_get_dashboard` + `python-dashboard-update`; plain text only (no HTML injection); line chart as inline SVG (no new dependency). Empty state explains `farm_report()` and points to `examples/farm_dashboard.py`.
- [ ] Tests: each widget renders from a sample section; unknown widget type shows a small "Unsupported widget" box; script strings are rendered as text. Commit.

### Task F5: Inspector + Developer tab

- [ ] `InspectorView.tsx`: account select (running clients) → `python_inspector_capture` → image; hover shows `x, y` fractions (3 decimals) and RGB; click copies `account.click(0.523, 0.811)`; Shift+click copies `account.wait_pixel(0.523, 0.811, (255, 200, 0))`; drag a rectangle → name prompt → `python_save_asset` → toast with the saved path and the `find_image("name.png")` snippet. Refresh button.
- [ ] `DeveloperPanel.tsx`: toggle (writes via `python_set_developer_mode`) with text explaining exactly what it unlocks (edit default blocks; terminal channel for `python -m multialt run`, usable by AI tools like Claude); when on: blocks list with **Modified** / **New default available** badges, Open, inline editor, **Restore default** per block with confirm text *"Restore {name} to the default? Your changes to this block will be lost. Your scripts and other blocks are not touched."*; CLI command copy box from `python_workspace_info.cliCommand`; "Open workspace folder" (mentions AGENTS.md for AI tools).
- [ ] Tests: restore is per block and asks first; toggle off hides blocks. Commit.

### Task F6: Harness scenarios

- [ ] `src/dev/harness/pythonScenarios.ts` exporting `PYTHON_SCENARIOS: Record<string, () => void>` with `python-not-installed`, `python-installing`, `python-ready`, `python-running` (two runs, logs streaming incl. a `blocked` line, dashboard updating), `python-dev-mode`; spread into `SCENARIOS` in `scenarios.ts` (2-line change). Scenarios only feed data — they never implement the behaviour under test.
- [ ] Drive each scenario in the browser preview (`bun run dev:ui`, `?scenario=python-running`) and fix layout issues found; final `bash .claude/quiet-check.sh python` green. Commit.

---

## Coordinator (main session) after the streams

1. Review pass (reviewer subagent, Sonnet): spec coverage + the Global Constraints, report-only.
2. Fix confirmed findings (tests first).
3. `bash .claude/quiet-check.sh` (full) on the branch.
4. `bun run tauri build --no-bundle` (signing env), copy to `C:/Users/luanm/Desktop/MultiAlt-testes/MultiAlt-teste.exe`; build the **standard** edition too and `bun run scan` both; if the standard is flagged, bisect between `python-dev-pipe` and `python-scripts` and move the culprit to `full` only.
5. Commit on `feature/python-scripts` (**no push**), report to the owner.

## Self-review notes

- Spec §4 (manual install button) → R2 + F2. §5 (workspace, per-block restore, badges) → R3 + F5. §6 (protocol, primitives, guards, messages) → contract + R4 + P1. §7 (runs, triggers, stop, shake, per-account log) → R5 + F2/F3. §8 (screen incl. 5b terminal, inspector) → F1–F5. §8b dashboard → R5 + P2 + F4. §9 dev mode + pipe → R6 + P3 + F5. §10 AI kit → P3. §11 editions → R1 + coordinator step 4. §12 P0 spike → first step of R4. §13 tests → each task. §14 rules → Global Constraints.
- Deviation from the writing-plans template: implementation bodies are left to the stream agents; contracts (types, names, protocol, messages, file list, test names) are fixed here so the three streams stay consistent.
