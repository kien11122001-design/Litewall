<#
 LiteWall benchmark (M3). Runs each variant (-Repeat times) on the primary monitor,
 prints a table and writes docs\bench-results.csv.

 Usage (repo root, release build):
   .\scripts\bench.ps1 -Video "$PWD\clip.mp4"
   .\scripts\bench.ps1 -Video "$PWD\clip.mp4" -Repeat 3 -Samples 15
 Leave the PC alone while it runs; mouse movement and other apps skew GPU numbers.

 Columns: Cpu% = process CPU as % of ALL logical cores; Gpu3D / GpuVideo / GpuCopy =
 mean engine utilisation of this process (GPU Engine counters; "video*" = every
 engine whose name starts with "video", e.g. AMD "video codec 0").
#>
param(
    [Parameter(Mandatory)][string]$Video,
    [string]$Exe = ".\target\release\litewall.exe",
    [int]$Warmup = 8,      # seconds before sampling
    [int]$Samples = 10,    # counter samples per run
    [int]$Repeat = 2,      # runs per variant (to see the noise)
    [string]$Csv = ".\docs\bench-results.csv"
)
$ErrorActionPreference = "Stop"
$log = "$env:LOCALAPPDATA\LiteWall\logs\litewall.log"
$cores = [int]$env:NUMBER_OF_PROCESSORS

# name -> extra mpv options (empty = current defaults from player.rs)
$variants = [ordered]@{
    "baseline (gpu-next, auto-safe, display-desync)" = @()
    "vo=gpu"                          = @("vo=gpu")
    "hwdec=d3d11va-copy"              = @("hwdec=d3d11va-copy")
    "hwdec=no (software)"             = @("hwdec=no")
    "video-sync=audio"                = @("video-sync=audio")
    "video-sync=desync"               = @("video-sync=desync")
    "video-sync=display-resample"     = @("video-sync=display-resample")
    "audio + vo=gpu"                  = @("video-sync=audio", "vo=gpu")
    "audio + swapchain-depth=1"       = @("video-sync=audio", "swapchain-depth=1")
}

function Sum-Engines($samples, $filter) {
    $t = 0.0
    foreach ($s in $samples) {
        if ($s.InstanceName -match 'engtype_(.+)$' -and (& $filter $Matches[1])) { $t += $s.CookedValue }
    }
    $t
}

& $Exe --quit | Out-Null; Start-Sleep 2
$seen = @{}
$rows = foreach ($run in 1..$Repeat) { foreach ($name in $variants.Keys) {
    $argList = @("--play", "`"$Video`"")
    foreach ($o in $variants[$name]) { $argList += @("--opt", $o) }
    $mark = (Get-Content $log -ErrorAction SilentlyContinue | Measure-Object).Count
    $p = Start-Process $Exe -ArgumentList $argList -PassThru
    Start-Sleep $Warmup

    $cpu0 = (Get-Process -Id $p.Id).TotalProcessorTime.TotalSeconds
    $sw = [Diagnostics.Stopwatch]::StartNew()
    $g3d = 0.0; $gvid = 0.0; $gcopy = 0.0
    for ($i = 0; $i -lt $Samples; $i++) {
        $c = Get-Counter "\GPU Engine(pid_$($p.Id)_*)\Utilization Percentage" -ErrorAction SilentlyContinue
        foreach ($s in $c.CounterSamples) { if ($s.InstanceName -match 'engtype_(.+)$') { $seen[$Matches[1]] = 1 } }
        $g3d   += Sum-Engines $c.CounterSamples { param($e) $e -eq "3d" }
        $gvid  += Sum-Engines $c.CounterSamples { param($e) $e -like "video*" }
        $gcopy += Sum-Engines $c.CounterSamples { param($e) $e -eq "copy" }
        Start-Sleep 1
    }
    $elapsed = $sw.Elapsed.TotalSeconds
    $proc = Get-Process -Id $p.Id
    $cpu = ($proc.TotalProcessorTime.TotalSeconds - $cpu0) / $elapsed / $cores * 100
    $new = Get-Content $log | Select-Object -Skip $mark

    $applied = ($new | Select-String "mpv override:" | Measure-Object).Count
    if ($applied -ne $variants[$name].Count) {
        & $Exe --quit | Out-Null
        throw "Variant '$name': expected $($variants[$name].Count) 'mpv override' lines, found $applied (stale build?)."
    }
    $hw  = ($new | Select-String "hwdec-current = (\S+)" | Select-Object -Last 1).Matches.Groups[1].Value
    $bad = ($new | Select-String "rejected|ERROR" | Measure-Object).Count

    [pscustomobject]@{
        Variant = $name; Run = $run; Hwdec = $hw
        CpuPct = [math]::Round($cpu, 2)
        Gpu3D = [math]::Round($g3d / $Samples, 1)
        GpuVideo = [math]::Round($gvid / $Samples, 1)
        GpuCopy = [math]::Round($gcopy / $Samples, 1)
        PrivateMB = [math]::Round($proc.PrivateMemorySize64 / 1MB)
        WorkingSetMB = [math]::Round($proc.WorkingSet64 / 1MB)
        LogProblems = $bad
    }
    & $Exe --quit | Out-Null; Start-Sleep 3
} }
$rows | Sort-Object Variant, Run | Format-Table -AutoSize
"GPU engine types seen: " + (($seen.Keys | Sort-Object) -join ", ")
New-Item -ItemType Directory -Force (Split-Path $Csv) | Out-Null
$rows | Export-Csv $Csv -NoTypeInformation
"Saved $Csv"
