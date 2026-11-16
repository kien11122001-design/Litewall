<#
 Monitor-id reboot check (M4). Runs `litewall --list-monitors` and compares the ids it logs
 with the ids in the example config. Do it right after a reboot (and, if you like, after
 re-plugging a monitor): every config id must still be reported (OK), none MISSING.
 Usage:  .\scripts\check-ids.ps1
#>
param([string]$Exe = ".\target\release\litewall.exe", [string]$Config = ".\docs\config.example.toml")
$log = "$env:LOCALAPPDATA\LiteWall\logs\litewall.log"
$before = (Get-Content $log -ErrorAction SilentlyContinue | Measure-Object).Count
$p = Start-Process $Exe -ArgumentList '--list-monitors' -PassThru
if (-not $p.WaitForExit(8000)) { "list-monitors did not exit (old exe?)"; $p | Stop-Process; return }
$all = @(Get-Content $log | Where-Object { $_ -match 'INFO\s+monitor\[\d+\] id=' })
$lines = Get-Content $log | Select-Object -Skip $before | Where-Object { $_ -match 'INFO\s+monitor\[\d+\] id=' }
if (-not $lines) { $lines = $all | Select-Object -Last 3 }   # the log rotated meanwhile
$current  = @($lines | ForEach-Object { if ($_ -match 'id=(.+?) rect=') { $Matches[1] } })
$expected = @(Select-String -Path $Config -Pattern "^id = '(.+)'" | ForEach-Object { $_.Matches[0].Groups[1].Value })
foreach ($id in $expected) { if ($current -contains $id) { "OK       $id" } else { "MISSING  $id" } }
foreach ($id in $current)  { if ($expected -notcontains $id) { "NEW      $id" } }
