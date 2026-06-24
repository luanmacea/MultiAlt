# Version Benchmarks

This benchmark runner compares desktop process performance between:

- v4 rewrite
- v3.8
- v3.7.2

It captures per-run startup latency and post-start idle resource usage.

## Metrics

- `startup_ms`: time until a visible main window is detected
- `working_set_mb`: resident memory after idle sampling window
- `private_memory_mb`: private bytes after idle sampling window
- `cpu_time_ms`: total process CPU time after idle sampling window

## Usage

### Option 1: direct parameters

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\benchmarks\run-version-benchmarks.ps1 `
  -V4Exe "C:\bench\v4\roblox-account-manager.exe" `
  -V38Exe "C:\bench\v3.8\RBX Alt Manager.exe" `
  -V372Exe "C:\bench\v3.7.2\RBX Alt Manager.exe" `
  -Iterations 10 `
  -Warmup 2 `
  -StartupTimeoutSeconds 45 `
  -IdleSampleSeconds 5
```

### Option 2: config file

1. Copy `scripts/benchmarks/config.example.json` to `scripts/benchmarks/config.local.json`.
2. Update executable paths.
3. Run:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\benchmarks\run-version-benchmarks.ps1 `
  -ConfigPath .\scripts\benchmarks\config.local.json
```

## Outputs

Each run creates a timestamped directory under `scripts/benchmarks/results/<run-id>/`:

- `runs.csv`: every run, including warmup/failure rows
- `summary.csv`: aggregated stats per version
- `results.json`: full machine-readable output
- `report.md`: comparison report with v4 deltas vs v3.8 and v3.7.2

## Benchmark Hygiene

- Close other heavy apps before running.
- Use the same machine and power profile for all versions.
- Keep account data/environment as similar as possible.
- Prefer at least 10 measured runs for stable averages.
