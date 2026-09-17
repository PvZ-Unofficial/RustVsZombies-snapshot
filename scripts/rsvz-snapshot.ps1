# Forward the existing CLI unchanged; this script only chooses its tool environment.
$ErrorActionPreference = 'Stop'
$arch = if ($args.Count -gt 0 -and $args[0] -in @('inject-1051', 'run-1051')) { 'x86' } else { 'x64' }
. (Join-Path $PSScriptRoot 'snapshot-env.ps1') -Arch $arch
& (Join-Path (Split-Path $PSScriptRoot -Parent) 'tools/rsvz.exe') @args
exit $LASTEXITCODE
