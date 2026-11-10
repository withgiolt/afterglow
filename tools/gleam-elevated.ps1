# Windows without Developer Mode: Gleam needs symlinks for Erlang-target builds, which
# only an elevated process may create. This runs one or more Gleam commands elevated
# (one UAC prompt) and copies their output back.
#   powershell -File tools/gleam-elevated.ps1 "<dir>|<gleam args>" ["<dir>|<gleam args>" ...]
param([Parameter(ValueFromRemainingArguments = $true)][string[]]$Jobs)

$otp = "C:\Program Files\Erlang OTP"
$erts = Get-ChildItem "$otp\erts-*" -Directory | Sort-Object { [version]($_.Name -replace 'erts-', '') } | Select-Object -Last 1
$path = "$($erts.FullName)\bin;$env:USERPROFILE\.local\bin;$env:PATH"
$log = Join-Path $env:TEMP "gleam-elevated.log"
$inner = Join-Path $env:TEMP "gleam-elevated-inner.ps1"

$lines = @("`$env:PATH = '$path'", "`$rc = 0", "Remove-Item '$log' -ErrorAction SilentlyContinue")
foreach ($job in $Jobs) {
  $dir, $gargs = $job -split '\|', 2
  $lines += "Add-Content '$log' '=== $dir : gleam $gargs'"
  $lines += "Set-Location '$dir'"
  $lines += "& gleam $gargs *>&1 | Out-File -Append -Encoding utf8 '$log'"
  $lines += "if (`$LASTEXITCODE -ne 0) { `$rc = `$LASTEXITCODE; Add-Content '$log' `"=== exit `$LASTEXITCODE`" }"
}
$lines += "exit `$rc"
Set-Content -Encoding utf8 $inner $lines

$p = Start-Process powershell -Verb RunAs -Wait -PassThru -WindowStyle Hidden `
  -ArgumentList "-NoProfile -ExecutionPolicy Bypass -File `"$inner`""
Get-Content $log
exit $p.ExitCode
