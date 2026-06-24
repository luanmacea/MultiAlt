param(
    [string]$ConfigPath = "",
    [string]$V4Exe = "",
    [string]$V38Exe = "",
    [string]$V372Exe = "",
    [int]$Iterations = 10,
    [int]$Warmup = 2,
    [int]$StartupTimeoutSeconds = 45,
    [int]$IdleSampleSeconds = 5,
    [string]$OutputDir = "scripts/benchmarks/results"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Resolve-FilePath([string]$Path) {
    if (-not $Path) {
        return ""
    }

    try {
        return (Resolve-Path -Path $Path -ErrorAction Stop).Path
    } catch {
        throw "File not found: $Path"
    }
}

function Get-Percentile([double[]]$Values, [double]$Percentile) {
    if (-not $Values -or $Values.Count -eq 0) {
        return $null
    }

    $sorted = $Values | Sort-Object
    $index = [Math]::Ceiling($Percentile * $sorted.Count) - 1
    if ($index -lt 0) {
        $index = 0
    }
    if ($index -ge $sorted.Count) {
        $index = $sorted.Count - 1
    }

    return [Math]::Round([double]$sorted[$index], 2)
}

function Get-Average([double[]]$Values) {
    if (-not $Values -or $Values.Count -eq 0) {
        return $null
    }

    return [Math]::Round(([double]($Values | Measure-Object -Average).Average), 2)
}

function Get-Median([double[]]$Values) {
    return Get-Percentile -Values $Values -Percentile 0.5
}

function Wait-ForMainWindow([System.Diagnostics.Process]$Process, [int]$TimeoutMs) {
    $sw = [System.Diagnostics.Stopwatch]::StartNew()
    while ($sw.ElapsedMilliseconds -lt $TimeoutMs) {
        if ($Process.HasExited) {
            return $null
        }

        try {
            $Process.Refresh()
        } catch {
        }

        if ($Process.MainWindowHandle -ne 0) {
            try {
                $null = $Process.WaitForInputIdle(5000)
            } catch {
            }
            return [int]$sw.ElapsedMilliseconds
        }

        Start-Sleep -Milliseconds 100
    }

    return $null
}

function Stop-ProcessSafely([System.Diagnostics.Process]$Process) {
    if (-not $Process) {
        return $false
    }

    $graceful = $false
    try {
        if (-not $Process.HasExited -and $Process.MainWindowHandle -ne 0) {
            $graceful = [bool]$Process.CloseMainWindow()
            $null = $Process.WaitForExit(5000)
        }
    } catch {
    }

    try {
        if (-not $Process.HasExited) {
            Stop-Process -Id $Process.Id -Force -ErrorAction SilentlyContinue
        }
    } catch {
    }

    return $graceful
}

function Invoke-AppRun(
    [string]$AppName,
    [string]$ExecutablePath,
    [int]$RunNumber,
    [bool]$IsWarmup,
    [int]$StartupTimeoutMs,
    [int]$IdleSeconds
) {
    $startedAt = (Get-Date).ToString("o")
    $proc = $null

    try {
        $proc = Start-Process -FilePath $ExecutablePath -PassThru
        $startupMs = Wait-ForMainWindow -Process $proc -TimeoutMs $StartupTimeoutMs

        if ($null -eq $startupMs) {
            $closedGracefully = Stop-ProcessSafely -Process $proc
            return [pscustomobject]@{
                app_name                = $AppName
                executable_path         = $ExecutablePath
                run_number              = $RunNumber
                is_warmup               = $IsWarmup
                status                  = "timeout"
                error                   = "Main window was not detected before timeout."
                started_at              = $startedAt
                process_id              = if ($proc) { $proc.Id } else { $null }
                startup_ms              = $null
                working_set_mb          = $null
                private_memory_mb       = $null
                paged_memory_mb         = $null
                virtual_memory_mb       = $null
                cpu_time_ms             = $null
                closed_gracefully       = $closedGracefully
            }
        }

        Start-Sleep -Seconds $IdleSeconds
        $proc.Refresh()

        $workingSetMb = [Math]::Round(($proc.WorkingSet64 / 1MB), 2)
        $privateMemoryMb = [Math]::Round(($proc.PrivateMemorySize64 / 1MB), 2)
        $pagedMemoryMb = [Math]::Round(($proc.PagedMemorySize64 / 1MB), 2)
        $virtualMemoryMb = [Math]::Round(($proc.VirtualMemorySize64 / 1MB), 2)
        $cpuTimeMs = [Math]::Round($proc.TotalProcessorTime.TotalMilliseconds, 2)

        $closedGracefully = Stop-ProcessSafely -Process $proc

        return [pscustomobject]@{
            app_name                = $AppName
            executable_path         = $ExecutablePath
            run_number              = $RunNumber
            is_warmup               = $IsWarmup
            status                  = "ok"
            error                   = $null
            started_at              = $startedAt
            process_id              = $proc.Id
            startup_ms              = [double]$startupMs
            working_set_mb          = [double]$workingSetMb
            private_memory_mb       = [double]$privateMemoryMb
            paged_memory_mb         = [double]$pagedMemoryMb
            virtual_memory_mb       = [double]$virtualMemoryMb
            cpu_time_ms             = [double]$cpuTimeMs
            closed_gracefully       = $closedGracefully
        }
    } catch {
        if ($proc) {
            $null = Stop-ProcessSafely -Process $proc
        }
        return [pscustomobject]@{
            app_name                = $AppName
            executable_path         = $ExecutablePath
            run_number              = $RunNumber
            is_warmup               = $IsWarmup
            status                  = "error"
            error                   = $_.Exception.Message
            started_at              = $startedAt
            process_id              = if ($proc) { $proc.Id } else { $null }
            startup_ms              = $null
            working_set_mb          = $null
            private_memory_mb       = $null
            paged_memory_mb         = $null
            virtual_memory_mb       = $null
            cpu_time_ms             = $null
            closed_gracefully       = $false
        }
    }
}

function Summarize-AppRuns([string]$AppName, [object[]]$Runs) {
    $measured = @($Runs | Where-Object { $_.is_warmup -eq $false })
    $successful = @($measured | Where-Object { $_.status -eq "ok" })
    $failures = @($measured | Where-Object { $_.status -ne "ok" })

    $startupValues = @($successful | ForEach-Object { [double]$_.startup_ms })
    $workingSetValues = @($successful | ForEach-Object { [double]$_.working_set_mb })
    $privateValues = @($successful | ForEach-Object { [double]$_.private_memory_mb })
    $cpuValues = @($successful | ForEach-Object { [double]$_.cpu_time_ms })

    [pscustomobject]@{
        app_name                         = $AppName
        total_runs                       = $measured.Count
        successful_runs                  = $successful.Count
        failed_runs                      = $failures.Count
        startup_mean_ms                  = Get-Average -Values $startupValues
        startup_median_ms                = Get-Median -Values $startupValues
        startup_p95_ms                   = Get-Percentile -Values $startupValues -Percentile 0.95
        working_set_mean_mb              = Get-Average -Values $workingSetValues
        private_memory_mean_mb           = Get-Average -Values $privateValues
        cpu_time_mean_ms                 = Get-Average -Values $cpuValues
    }
}

function Format-Delta([Nullable[double]]$Current, [Nullable[double]]$Baseline) {
    if ($null -eq $Current -or $null -eq $Baseline -or $Baseline -eq 0) {
        return "n/a"
    }

    $delta = (($Current - $Baseline) / $Baseline) * 100
    $sign = if ($delta -ge 0) { "+" } else { "" }
    return "$sign$([Math]::Round($delta, 2))%"
}

if ($ConfigPath) {
    $config = Get-Content -Path $ConfigPath -Raw | ConvertFrom-Json
    if (-not $V4Exe -and ($null -ne $config.PSObject.Properties["v4_exe"])) { $V4Exe = [string]$config.v4_exe }
    if (-not $V38Exe -and ($null -ne $config.PSObject.Properties["v3_8_exe"])) { $V38Exe = [string]$config.v3_8_exe }
    if (-not $V372Exe -and ($null -ne $config.PSObject.Properties["v3_7_2_exe"])) { $V372Exe = [string]$config.v3_7_2_exe }
    if ($null -ne $config.PSObject.Properties["iterations"]) { $Iterations = [int]$config.iterations }
    if ($null -ne $config.PSObject.Properties["warmup"]) { $Warmup = [int]$config.warmup }
    if ($null -ne $config.PSObject.Properties["startup_timeout_seconds"]) { $StartupTimeoutSeconds = [int]$config.startup_timeout_seconds }
    if ($null -ne $config.PSObject.Properties["idle_sample_seconds"]) { $IdleSampleSeconds = [int]$config.idle_sample_seconds }
    if ($null -ne $config.PSObject.Properties["output_dir"]) { $OutputDir = [string]$config.output_dir }
}

if (-not $V4Exe -or -not $V38Exe -or -not $V372Exe) {
    throw "You must provide executable paths for v4, v3.8, and v3.7.2 using parameters or -ConfigPath."
}

if ($Iterations -lt 1) { throw "Iterations must be at least 1." }
if ($Warmup -lt 0) { throw "Warmup cannot be negative." }
if ($StartupTimeoutSeconds -lt 1) { throw "StartupTimeoutSeconds must be at least 1." }
if ($IdleSampleSeconds -lt 1) { throw "IdleSampleSeconds must be at least 1." }

$v4Path = Resolve-FilePath -Path $V4Exe
$v38Path = Resolve-FilePath -Path $V38Exe
$v372Path = Resolve-FilePath -Path $V372Exe

$apps = @(
    @{ name = "v4"; executable_path = $v4Path },
    @{ name = "v3.8"; executable_path = $v38Path },
    @{ name = "v3.7.2"; executable_path = $v372Path }
)

$runId = Get-Date -Format "yyyyMMdd-HHmmss"
$resultsDir = Join-Path $OutputDir $runId
New-Item -ItemType Directory -Path $resultsDir -Force | Out-Null

$allRuns = New-Object System.Collections.Generic.List[object]
$startupTimeoutMs = $StartupTimeoutSeconds * 1000
$totalPerApp = $Warmup + $Iterations

foreach ($app in $apps) {
    for ($runIndex = 1; $runIndex -le $totalPerApp; $runIndex++) {
        $isWarmup = $runIndex -le $Warmup
        $phase = if ($isWarmup) { "warmup" } else { "measured" }
        Write-Host "[$($app.name)] Run $runIndex/$totalPerApp ($phase)"

        $run = Invoke-AppRun `
            -AppName $app.name `
            -ExecutablePath $app.executable_path `
            -RunNumber $runIndex `
            -IsWarmup $isWarmup `
            -StartupTimeoutMs $startupTimeoutMs `
            -IdleSeconds $IdleSampleSeconds

        $allRuns.Add($run)
    }
}

$summary = foreach ($app in $apps) {
    $appRuns = @($allRuns | Where-Object { $_.app_name -eq $app.name })
    Summarize-AppRuns -AppName $app.name -Runs $appRuns
}

$summaryByName = @{}
foreach ($item in $summary) {
    $summaryByName[$item.app_name] = $item
}

$resultsJson = [pscustomobject]@{
    metadata = [pscustomobject]@{
        generated_at_utc         = (Get-Date).ToUniversalTime().ToString("o")
        run_id                   = $runId
        iterations               = $Iterations
        warmup                   = $Warmup
        startup_timeout_seconds  = $StartupTimeoutSeconds
        idle_sample_seconds      = $IdleSampleSeconds
    }
    applications = $apps
    summary      = $summary
    runs         = $allRuns
}

$runsCsvPath = Join-Path $resultsDir "runs.csv"
$summaryCsvPath = Join-Path $resultsDir "summary.csv"
$jsonPath = Join-Path $resultsDir "results.json"
$reportPath = Join-Path $resultsDir "report.md"

$allRuns | Export-Csv -Path $runsCsvPath -NoTypeInformation -Encoding utf8
$summary | Export-Csv -Path $summaryCsvPath -NoTypeInformation -Encoding utf8
$resultsJson | ConvertTo-Json -Depth 8 | Set-Content -Path $jsonPath -Encoding utf8

$v4Summary = $summaryByName["v4"]
$v38Summary = $summaryByName["v3.8"]
$v372Summary = $summaryByName["v3.7.2"]

$report = @()
$report += "# Version Performance Benchmark"
$report += ""
$report += "- Run ID: $runId"
$report += "- Generated (UTC): $((Get-Date).ToUniversalTime().ToString("yyyy-MM-dd HH:mm:ss"))"
$report += "- Iterations: $Iterations measured + $Warmup warmup"
$report += "- Startup timeout: $StartupTimeoutSeconds s"
$report += "- Idle sampling window: $IdleSampleSeconds s"
$report += ""
$report += "## Executables"
$report += ""
$report += "| Version | Path |"
$report += "| --- | --- |"
$report += "| v4 | $v4Path |"
$report += "| v3.8 | $v38Path |"
$report += "| v3.7.2 | $v372Path |"
$report += ""
$report += "## Summary"
$report += ""
$report += "| Version | Success / Total | Startup Mean (ms) | Startup P95 (ms) | Working Set Mean (MB) | Private Memory Mean (MB) | CPU Time Mean (ms) |"
$report += "| --- | --- | ---: | ---: | ---: | ---: | ---: |"
foreach ($item in $summary) {
    $report += "| $($item.app_name) | $($item.successful_runs) / $($item.total_runs) | $($item.startup_mean_ms) | $($item.startup_p95_ms) | $($item.working_set_mean_mb) | $($item.private_memory_mean_mb) | $($item.cpu_time_mean_ms) |"
}
$report += ""
$report += "## v4 Delta vs v3.8"
$report += ""
$report += "| Metric | v4 | v3.8 | Delta |"
$report += "| --- | ---: | ---: | ---: |"
$report += "| Startup Mean (ms) | $($v4Summary.startup_mean_ms) | $($v38Summary.startup_mean_ms) | $(Format-Delta -Current $v4Summary.startup_mean_ms -Baseline $v38Summary.startup_mean_ms) |"
$report += "| Startup P95 (ms) | $($v4Summary.startup_p95_ms) | $($v38Summary.startup_p95_ms) | $(Format-Delta -Current $v4Summary.startup_p95_ms -Baseline $v38Summary.startup_p95_ms) |"
$report += "| Working Set Mean (MB) | $($v4Summary.working_set_mean_mb) | $($v38Summary.working_set_mean_mb) | $(Format-Delta -Current $v4Summary.working_set_mean_mb -Baseline $v38Summary.working_set_mean_mb) |"
$report += "| Private Memory Mean (MB) | $($v4Summary.private_memory_mean_mb) | $($v38Summary.private_memory_mean_mb) | $(Format-Delta -Current $v4Summary.private_memory_mean_mb -Baseline $v38Summary.private_memory_mean_mb) |"
$report += "| CPU Time Mean (ms) | $($v4Summary.cpu_time_mean_ms) | $($v38Summary.cpu_time_mean_ms) | $(Format-Delta -Current $v4Summary.cpu_time_mean_ms -Baseline $v38Summary.cpu_time_mean_ms) |"
$report += ""
$report += "## v4 Delta vs v3.7.2"
$report += ""
$report += "| Metric | v4 | v3.7.2 | Delta |"
$report += "| --- | ---: | ---: | ---: |"
$report += "| Startup Mean (ms) | $($v4Summary.startup_mean_ms) | $($v372Summary.startup_mean_ms) | $(Format-Delta -Current $v4Summary.startup_mean_ms -Baseline $v372Summary.startup_mean_ms) |"
$report += "| Startup P95 (ms) | $($v4Summary.startup_p95_ms) | $($v372Summary.startup_p95_ms) | $(Format-Delta -Current $v4Summary.startup_p95_ms -Baseline $v372Summary.startup_p95_ms) |"
$report += "| Working Set Mean (MB) | $($v4Summary.working_set_mean_mb) | $($v372Summary.working_set_mean_mb) | $(Format-Delta -Current $v4Summary.working_set_mean_mb -Baseline $v372Summary.working_set_mean_mb) |"
$report += "| Private Memory Mean (MB) | $($v4Summary.private_memory_mean_mb) | $($v372Summary.private_memory_mean_mb) | $(Format-Delta -Current $v4Summary.private_memory_mean_mb -Baseline $v372Summary.private_memory_mean_mb) |"
$report += "| CPU Time Mean (ms) | $($v4Summary.cpu_time_mean_ms) | $($v372Summary.cpu_time_mean_ms) | $(Format-Delta -Current $v4Summary.cpu_time_mean_ms -Baseline $v372Summary.cpu_time_mean_ms) |"
$report += ""
$report += "## Output Files"
$report += ""
$report += "- runs.csv"
$report += "- summary.csv"
$report += "- results.json"
$report += "- report.md"

$report -join [Environment]::NewLine | Set-Content -Path $reportPath -Encoding utf8

Write-Host ""
Write-Host "Benchmark completed."
Write-Host "Results: $resultsDir"
Write-Host "Summary report: $reportPath"

$appsWithNoSuccessfulRuns = @(
    $summary | Where-Object { $_.successful_runs -eq 0 }
)

if ($appsWithNoSuccessfulRuns.Count -gt 0) {
    Write-Warning "One or more versions had zero successful measured runs. Check $reportPath and runs.csv."
}
