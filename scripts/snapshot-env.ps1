# Dot-source this file to use a Release bundle's toolchain in this process only.
param([ValidateSet('x64', 'x86')][string]$Arch = 'x64')
$ErrorActionPreference = 'Stop'
$snapshotRoot = Split-Path $PSScriptRoot -Parent
$toolRoot = Join-Path $snapshotRoot 'tools'
foreach ($relative in @('rust/bin/cargo.exe', 'rust/bin/rustc.exe', 'llvm/bin/clang-cl.exe',
    'llvm/bin/lld-link.exe', 'llvm/bin/libclang.dll', 'cmake/bin/cmake.exe', 'ninja/ninja.exe')) {
    if (-not (Test-Path -LiteralPath (Join-Path $toolRoot $relative))) {
        throw "Missing bundled tool: $relative. Download the Windows tools Release asset (not the source ZIP)."
    }
}
$vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
if (-not (Test-Path -LiteralPath $vswhere)) {
    throw 'MSVC Build Tools and Windows SDK are required. See docs/snapshot-quickstart.md. They are not bundled.'
}
$vs = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
if ($LASTEXITCODE -ne 0 -or -not $vs) { throw 'Install the MSVC x86/x64 C++ tools and Windows SDK.' }
$devcmd = Join-Path $vs 'Common7/Tools/VsDevCmd.bat'
$environment = & $env:ComSpec /d /c "call `"$devcmd`" -no_logo -host_arch=x64 -arch=$Arch >nul && set"
if ($LASTEXITCODE -ne 0) { throw 'Failed to activate MSVC Build Tools.' }
foreach ($line in $environment) {
    if ($line -match '^([^=]+)=(.*)$') { [Environment]::SetEnvironmentVariable($matches[1], $matches[2], 'Process') }
}
$nativePath = $env:PATH
$env:PATH = ((@('rust/bin', 'llvm/bin', 'cmake/bin', 'ninja') | ForEach-Object { Join-Path $toolRoot $_ }) -join ';') + ';' + $nativePath
$env:RUSTC = Join-Path $toolRoot 'rust/bin/rustc.exe'
$env:RUSTDOC = Join-Path $toolRoot 'rust/bin/rustdoc.exe'
$env:CARGO_HOME = Join-Path $snapshotRoot '.cargo-home'
$env:CARGO_TARGET_DIR = Join-Path $snapshotRoot 'target'
$env:PE_RS_LLVM_BIN = Join-Path $toolRoot 'llvm/bin'
$env:LIBCLANG_PATH = $env:PE_RS_LLVM_BIN
$env:PE_RS_SOURCE_DIR = Join-Path $snapshotRoot 'vendor/pvz-emulator'
if (-not (Test-Path -LiteralPath (Join-Path $env:PE_RS_SOURCE_DIR 'CMakeLists.txt'))) {
    throw 'The Release bundle is missing vendor/pvz-emulator.'
}
# Do not inherit optimization/PGO flags or a linker from another development shell.
foreach ($name in @('RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS', 'CARGO_PROFILE_RELEASE_LTO',
    'CARGO_PROFILE_RELEASE_CODEGEN_UNITS', 'CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER',
    'CARGO_TARGET_I686_PC_WINDOWS_MSVC_LINKER')) {
    Remove-Item -LiteralPath "Env:$name" -ErrorAction SilentlyContinue
}
Write-Host "RSVZ bundle: $snapshotRoot; MSVC target: $Arch"
