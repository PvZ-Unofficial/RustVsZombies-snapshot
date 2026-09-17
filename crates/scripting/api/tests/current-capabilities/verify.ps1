$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $false
$repoPath = (Resolve-Path (Join-Path $PSScriptRoot '../../../../..')).Path
$manifest = Join-Path $PSScriptRoot 'Cargo.toml'
$previousTarget = $env:CARGO_TARGET_DIR
$env:CARGO_TARGET_DIR = Join-Path $repoPath 'target'
$targets = @(
    @('pvz-emulator', 'x86_64-pc-windows-msvc'),
    @('pvz-1-0-0-1051', 'i686-pc-windows-msvc'),
    @('pvz-portable', 'x86_64-pc-windows-msvc'),
    @('pvz-portable', 'x86_64-pc-windows-gnu')
)
Push-Location -LiteralPath $repoPath
try {
    $output = (& cargo check --quiet --manifest-path $manifest --no-default-features `
        --features pvz-emulator,pvz-portable --example unused --target x86_64-pc-windows-msvc 2>&1 | Out-String)
    if ($LASTEXITCODE -eq 0 -or -not $output.Contains('enable only one RSVZ current backend feature')) {
        throw "Wrong multiple-backend selection diagnostic:`n$output"
    }
    Write-Output 'Multiple-backend selection rejection PASS'
    foreach ($case in @('no_backend', 'callable', 'no_backend_call', 'no_backend_value', 'no_backend_entity', 'callable-invoke')) {
        $invoke = $case -eq 'callable-invoke'
        $example = if ($invoke) { 'callable' } else { $case }
        $arguments = @('check', '--quiet', '--manifest-path', $manifest, '--no-default-features',
            '--example', $example, '--target', 'x86_64-pc-windows-msvc')
        if ($invoke) { $arguments += @('--features', 'invoke') }
        $output = (& cargo @arguments 2>&1 | Out-String)
        $positive = $case -in @('no_backend', 'callable')
        if ($positive) {
            if ($LASTEXITCODE -ne 0) { throw "NoBackend positive case failed: $case`n$output" }
        } else {
            $diagnostic = if ($invoke) { 'E0618' } elseif ($case -eq 'no_backend_entity') { 'E0599' } else { 'E0277' }
            if ($LASTEXITCODE -eq 0 -or -not $output.Contains($diagnostic) -or
                $output -match 'E0432|E0433|E0425|E0412') {
                throw "Unexpected NoBackend capability failure: $case`n$output"
            }
            $trait = if ($case -eq 'no_backend_entity') { 'PlantReadBackend' } else { 'PlantFixerBackend' }
            if (-not $invoke -and ($output -notmatch 'NoBackend|CurrentBackend' -or -not $output.Contains($trait))) {
                throw "Wrong NoBackend capability failed: $case`n$output"
            }
        }
        Write-Output "NoBackend $case PASS"
    }
    & cargo run --quiet --manifest-path $manifest --no-default-features --example no_backend --target x86_64-pc-windows-msvc
    if ($LASTEXITCODE -ne 0) { throw 'NoBackend pure operations failed' }
    foreach ($target in $targets) {
        foreach ($case in @('unused', 'cob_surface', 'card_surface', 'dsl_surface', 'frame_surface', 'event_measure', 'key_call', 'key_value', 'callable', 'callable-invoke', 'refresh_trials', 'refresh_for', 'menu_capability', 'death_capability')) {
            $invoke = $case -eq 'callable-invoke'
            $example = if ($invoke) { 'callable' } else { $case }
            $features = $target[0] + $(if ($invoke) { ',invoke' } else { '' })
            $arguments = @('check', '--quiet', '--manifest-path', $manifest, '--no-default-features',
                '--features', $features, '--example', $example, '--target', $target[1])
            $output = (& cargo @arguments 2>&1 | Out-String)
            $exitCode = $LASTEXITCODE
            $refreshFailure = $target[0] -eq 'pvz-1-0-0-1051' -and $case -in @('refresh_trials', 'refresh_for')
            $backendFailure = $target[0] -eq 'pvz-emulator' -and $case -in @('menu_capability', 'death_capability')
            $shouldFail = $refreshFailure -or $backendFailure -or ($target[0] -eq 'pvz-emulator' -and $case -in @('key_call', 'key_value', 'callable-invoke'))
            if ($shouldFail) {
                $diagnostic = if ($invoke) { 'E0618' } else { 'E0277' }
                if ($exitCode -eq 0 -or -not $output.Contains($diagnostic) -or
                    $output -match 'E0432|E0433|E0425|E0412|E0599') {
                    throw "Unexpected capability failure: $case`n$output"
                }
                if ($refreshFailure -and (-not $output.Contains('CommonZombieDanceBackend') -or -not $output.Contains('CobImpactDelayBackend'))) {
                    throw "Wrong Refresh capabilities failed: $case`n$output"
                }
                if ($backendFailure) {
                    $trait = if ($case -eq 'menu_capability') { 'MainMenuBackend' } else { 'ZombieKillBackend' }
                    if (-not $output.Contains($trait)) { throw "Wrong backend capability failed: $case`n$output" }
                }
                if (-not $backendFailure -and -not $refreshFailure -and -not $invoke -and -not $output.Contains('KeyboardStateBackend')) {
                    throw "Wrong capability failed: $case`n$output"
                }
            } elseif ($exitCode -ne 0) {
                throw "Positive case failed: $case`n$output"
            }
            Write-Output "$($target[0]) $($target[1]) $case PASS"
        }
        if ($target[0] -eq 'pvz-portable') {
            foreach ($case in @('portable_capabilities', 'portable_input_shared', 'portable_input_entity', 'portable_input_iterator')) {
                $output = (& cargo check --quiet --manifest-path $manifest --no-default-features `
                    --features pvz-portable --example $case --target $target[1] 2>&1 | Out-String)
                if ($case -eq 'portable_capabilities') {
                    if ($LASTEXITCODE -ne 0) { throw "Portable capability surface failed: $output" }
                } else {
                    $expected = if ($case -eq 'portable_input_shared') { 'E0596' } else { 'E0502' }
                    if ($LASTEXITCODE -eq 0 -or -not $output.Contains($expected) -or $output -match 'E0432|E0433|E0425|E0412|E0405|E0308') {
                        throw "Wrong Portable input borrow diagnostic: $case`n$output"
                    }
                }
                Write-Output "$($target[1]) $case PASS"
            }
        }
    }
    & python (Join-Path $PSScriptRoot '../check_dependency_graphs.py')
    if ($LASTEXITCODE -ne 0) { throw 'Cargo graph verification failed' }
    foreach ($run in @(
        @('smart_remove_pe', 'pe-direct', 'x86_64-pc-windows-msvc'),
        @('key_registry_1051', 'pvz-1-0-0-1051', 'i686-pc-windows-msvc')
    )) {
        & cargo run --quiet --manifest-path $manifest --no-default-features --example $run[0] --features $run[1] --target $run[2]
        if ($LASTEXITCODE -ne 0) { throw "Non-live behavior check failed: $($run[0]) [$($run[1])]" }
    }
    & cargo rustc --quiet --manifest-path $manifest --no-default-features --features pvz-1-0-0-1051 --example no_board_frame_1051 --target i686-pc-windows-msvc -- -C link-arg=/BASE:0x10000000
    if ($LASTEXITCODE -ne 0) { throw 'No-Board Frame fixture build failed' }
    # Windows may reserve the fixed native root address for its loader/heap.
    # Retry only fixture reservation (77); never retry a failed assertion.
    for ($attempt = 0; $attempt -lt 16; $attempt++) {
        & (Join-Path $env:CARGO_TARGET_DIR 'i686-pc-windows-msvc/debug/examples/no_board_frame_1051.exe')
        if ($LASTEXITCODE -ne 77) { break }
    }
    if ($LASTEXITCODE -ne 0) { throw 'No-Board Frame fixture failed' }
    Write-Output 'Valid token / absent Board Frame and callback behavior PASS'
    $env:CARGO_TARGET_DIR = 'C:\rsvz-wt-target'
    & cargo check --quiet --manifest-path (Join-Path $repoPath 'extensions/rsvz-witness/Cargo.toml') --no-default-features --features tooling --bin witness-diff
    if ($LASTEXITCODE -ne 0) { throw 'Witness tooling-only compilation failed' }
    Write-Output 'Witness tooling-only compilation PASS'
} finally {
    $env:CARGO_TARGET_DIR = $previousTarget
    Pop-Location
}
