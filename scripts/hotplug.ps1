<#
 LiteWall hot-plug soak (M4). Needs LiteWall running with --config (3 monitors).
 Cycles "PC screen only" <-> "Extend" by calling SetDisplayConfig (what Win+P does). After
 each switch it WAITS until the log shows that LiteWall processed it (a new
 "N wallpaper window(s) running" line, up to -TimeoutSec), then records the time it took,
 the window count, handle count and memory. Handles and memory should stay flat; Handled
 must be True on every row and Rc must be 0.
 Expect: 1 window after "internal", 3 after "extend" (your config enables all three).

 Usage:  .\scripts\hotplug.ps1            (20 cycles)
         .\scripts\hotplug.ps1 -Cycles 5
 Save your work first: the screens blank and re-attach repeatedly. If Explorer ever
 misbehaves, LiteWall re-attaches by itself after Explorer restarts.
#>
param([int]$Cycles = 20, [int]$TimeoutSec = 25, [string]$Csv = ".\docs\hotplug-results.csv")
$log = "$env:LOCALAPPDATA\LiteWall\logs\litewall.log"
$p = Get-Process litewall -ErrorAction Stop | Select-Object -First 1

if (-not ('Win32.Disp' -as [type])) {
    Add-Type -Namespace Win32 -Name Disp -MemberDefinition '[DllImport("user32.dll")] public static extern int SetDisplayConfig(uint numPaths, IntPtr paths, uint numModes, IntPtr modes, uint flags);'
}
$SDC_APPLY = 0x80; $SDC_INTERNAL = 0x1; $SDC_EXTEND = 0x4

function ProcessedCount { (Select-String -Path $log -Pattern 'wallpaper window\(s\) running' | Measure-Object).Count }
function LastRunning { ((Select-String -Path $log -Pattern 'wallpaper window\(s\) running' | Select-Object -Last 1).Line) -replace '^.*INFO\s+', '' }

function Snap($tag, $rc, $handled, $seconds) {
    if ($p.HasExited) { throw "litewall exited during the test (at '$tag'); see $log" }
    $p.Refresh()
    [pscustomobject]@{
        Step = $tag; Rc = $rc; Handled = $handled; Seconds = $seconds
        Running = LastRunning
        Handles = $p.HandleCount
        PrivateMB = [math]::Round($p.PrivateMemorySize64 / 1MB)
        WorkingSetMB = [math]::Round($p.WorkingSet64 / 1MB)
    }
}

function InvokeStep($tag, $topology) {
    $before = ProcessedCount
    $sw = [Diagnostics.Stopwatch]::StartNew()
    $rc = [Win32.Disp]::SetDisplayConfig(0, [IntPtr]::Zero, 0, [IntPtr]::Zero, ($SDC_APPLY -bor $topology))
    while ((ProcessedCount) -le $before -and $sw.Elapsed.TotalSeconds -lt $TimeoutSec) { Start-Sleep -Milliseconds 500 }
    $handled = (ProcessedCount) -gt $before
    Start-Sleep 2   # let the windows finish appearing
    Snap $tag $rc $handled ([math]::Round($sw.Elapsed.TotalSeconds, 1))
}

$rows = @(Snap 'start' $null $null 0)
for ($i = 1; $i -le $Cycles; $i++) {
    $rows += InvokeStep "$i internal" $SDC_INTERNAL
    $rows += InvokeStep "$i extend" $SDC_EXTEND
}
$rows | Format-Table -AutoSize
$missed = ($rows | Where-Object { $_.Handled -eq $false }).Count
"Switches LiteWall did not report within ${TimeoutSec}s: $missed"
New-Item -ItemType Directory -Force (Split-Path $Csv) | Out-Null
$rows | Export-Csv $Csv -NoTypeInformation
"Saved $Csv"
