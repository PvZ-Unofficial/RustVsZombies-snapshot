[CmdletBinding()]
param(
    [string]$Golden,
    [string]$Actual
)

$ErrorActionPreference = 'Stop'
$workspace = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$targetRoot = [System.IO.Path]::GetFullPath((Join-Path $workspace 'target'))
if (-not $Golden) {
    $Golden = Join-Path $workspace 'tests\witness\golden\pe24-three-rounds-1051.json'
}
if (-not $Actual) {
    $Actual = Join-Path $workspace 'target\ci-witness\pe24-three-rounds-pe.json'
}
$emulator = Join-Path (Split-Path $workspace -Parent) 'PvZ-Emulator\CMakeLists.txt'
if (-not (Test-Path -LiteralPath $emulator)) {
    throw "PvZ-Emulator must be checked out beside RustVsZombies: $emulator"
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
$oldTargetDirectory = $env:CARGO_TARGET_DIR

Push-Location $workspace
try {
    & cargo run -p rsvz-cli -- run-pe `
        --dev-script witness_acceptance `
        --release `
        --threads 1 `
        --seed 1592594513 `
        --max-wall-secs 120 `
        --max-sim-frames 60000 `
        --output $Actual
    if ($LASTEXITCODE -ne 0) {
        throw "PE Witness run failed with exit code $LASTEXITCODE"
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
    Pop-Location
}
