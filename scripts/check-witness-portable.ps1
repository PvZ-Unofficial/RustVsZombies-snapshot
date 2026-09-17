[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$ResourceDirectory,
    [string]$Golden,
    [string]$Actual,
    [string]$Report
)

$ErrorActionPreference = 'Stop'
$workspace = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$targetRoot = [System.IO.Path]::GetFullPath((Join-Path $workspace 'target'))
$portableRoot = Join-Path (Split-Path $workspace -Parent) 'PvZ-Portable'
$resourceDirectory = (Resolve-Path -LiteralPath $ResourceDirectory).Path
$game = Join-Path $targetRoot 'rsvz\pvz-portable\game-sdk-v4\msvc\release\native\Release\pvz-portable.exe'

if (-not (Test-Path -LiteralPath (Join-Path $portableRoot 'CMakeLists.txt'))) {
    throw "PvZ-Portable must be checked out beside RustVsZombies: $portableRoot"
}
foreach ($resource in @('main.pak', 'properties')) {
    if (-not (Test-Path -LiteralPath (Join-Path $resourceDirectory $resource))) {
        throw "Portable resource is missing: $(Join-Path $resourceDirectory $resource)"
    }
}
if (-not $Golden) {
    $Golden = Join-Path $workspace 'tests\witness\golden\pe24-three-rounds-1051.json'
}
if (-not $Actual) {
    $Actual = Join-Path $targetRoot 'ci-witness\pe24-three-rounds-portable.json'
}
if (-not $Report) {
    $Report = Join-Path $targetRoot 'ci-witness\pe24-three-rounds-portable.report.json'
}
if (-not (Test-Path -LiteralPath $Golden)) {
    throw "1051 Witness golden is missing: $Golden"
}

function Reset-TargetFile([string]$Path) {
    $full = [System.IO.Path]::GetFullPath($Path)
    if (-not $full.StartsWith($targetRoot + [System.IO.Path]::DirectorySeparatorChar, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "CI output must stay below $targetRoot`: $full"
    }
    if (Test-Path -LiteralPath $full) {
        Remove-Item -LiteralPath $full -Force
    }
    New-Item -ItemType Directory -Force -Path ([System.IO.Path]::GetDirectoryName($full)) | Out-Null
    $full
}

$Actual = Reset-TargetFile $Actual
$Report = Reset-TargetFile $Report
$oldTargetDirectory = $env:CARGO_TARGET_DIR

Push-Location $workspace
try {
    if (Test-Path -LiteralPath $game) {
        Get-Process pvz-portable -ErrorAction SilentlyContinue |
            Where-Object { $_.Path -eq $game } |
            Stop-Process -Force
    }

    & cargo run -p rsvz-cli -- run-portable `
        --dev-script witness_acceptance `
        --portable-toolchain msvc `
        --portable-root $portableRoot `
        --release `
        --output $Actual `
        --report $Report `
        --timeout-secs 180 `
        -- -resdir $resourceDirectory
    if ($LASTEXITCODE -ne 0) {
        throw "Portable Witness run failed with exit code $LASTEXITCODE"
    }

    $env:CARGO_TARGET_DIR = Join-Path $env:TEMP 'rsvz-witness-ci-target'
    & cargo run `
        --manifest-path extensions\rsvz-witness\Cargo.toml `
        --features tooling `
        --bin witness-diff `
        -- $Golden $Actual
    if ($LASTEXITCODE -ne 0) {
        throw "Witness comparison failed with exit code $LASTEXITCODE"
    }
}
finally {
    $env:CARGO_TARGET_DIR = $oldTargetDirectory
    if (Test-Path -LiteralPath $game) {
        Get-Process pvz-portable -ErrorAction SilentlyContinue |
            Where-Object { $_.Path -eq $game } |
            Stop-Process -Force
    }
    Pop-Location
}
