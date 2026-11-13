param([ValidateSet('start', 'dev', 'check', 'test')][string]$Action = 'start')
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
Set-Location -LiteralPath $projectRoot

$runtimeRoot = Join-Path $projectRoot '.tools\runtime'
$runtimeProgram = Join-Path $runtimeRoot 'shike.exe'
$manifestPath = Join-Path $runtimeRoot 'build.json'

function Get-SourceHash {
    $hash = & node (Join-Path $PSScriptRoot 'source-hash.mjs')
    if ($LASTEXITCODE -ne 0) { throw 'Unable to read project source files.' }
    return $hash.Trim()
}

function Test-CachedBuild([string]$Hash) {
    if (-not (Test-Path -LiteralPath $runtimeProgram) -or -not (Test-Path -LiteralPath $manifestPath)) { return $false }
    try {
        $manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
        return $manifest.sourceHash -eq $Hash
    } catch { return $false }
}

function Stop-ProjectApp {
    $processes = @(Get-Process -Name shike -ErrorAction SilentlyContinue | Where-Object {
        $_.Path -and $_.Path.StartsWith($projectRoot + '\', [StringComparison]::OrdinalIgnoreCase)
    })
    foreach ($process in $processes) {
        Start-Process -FilePath $process.Path -ArgumentList '--quit' -WorkingDirectory $projectRoot -WindowStyle Hidden -Wait
        if (-not $process.WaitForExit(10000)) { throw 'Please resolve the note-save message in the application before restarting.' }
    }
}

function Initialize-Toolchain {
    if (-not $env:CARGO_HTTP_PROXY) {
        $registryAddress = [Uri]'https://index.crates.io'
        $systemProxy = [System.Net.WebRequest]::DefaultWebProxy.GetProxy($registryAddress)
        if ($systemProxy.AbsoluteUri -ne $registryAddress.AbsoluteUri) {
            $env:CARGO_HTTP_PROXY = $systemProxy.AbsoluteUri
        }
    }

    $toolRoots = @((Join-Path $projectRoot '.tools'), (Join-Path (Split-Path -Parent $projectRoot) '.tools'))
    foreach ($toolRoot in $toolRoots) {
        $localCargo = Join-Path $toolRoot 'cargo'
        if (Test-Path -LiteralPath (Join-Path $localCargo 'bin\cargo.exe')) {
            $env:CARGO_HOME = $localCargo
            $env:RUSTUP_HOME = Join-Path $toolRoot 'rustup'
            $env:PATH = (Join-Path $localCargo 'bin') + ';' + $env:PATH
            break
        }
    }

    $developerShell = $null
    foreach ($toolRoot in $toolRoots) {
        $candidate = Join-Path $toolRoot 'buildtools\Common7\Tools\VsDevCmd.bat'
        if (Test-Path -LiteralPath $candidate) { $developerShell = $candidate; break }
    }
    if (-not $developerShell) {
        $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
        if (Test-Path -LiteralPath $vswhere) {
            $installation = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
            if ($installation) { $developerShell = Join-Path $installation 'Common7\Tools\VsDevCmd.bat' }
        }
    }
    if ($developerShell -and (Test-Path -LiteralPath $developerShell)) {
        $compilerEnvironment = & $env:ComSpec /d /s /c "`"`"$developerShell`" -no_logo -arch=amd64 >nul && set`""
        if ($LASTEXITCODE -ne 0) { throw 'Unable to initialize the Windows C++ build environment.' }
        foreach ($entry in $compilerEnvironment) {
            $parts = $entry -split '=', 2
            if ($parts.Length -eq 2 -and $parts[0]) { [Environment]::SetEnvironmentVariable($parts[0], $parts[1], 'Process') }
        }
    }
    if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) { throw 'Rust is required. Install the stable MSVC toolchain and Windows C++ Build Tools.' }
}

if ($Action -eq 'start') {
    $sourceHash = Get-SourceHash
    if (Test-CachedBuild $sourceHash) {
        Start-Process -FilePath $runtimeProgram -WorkingDirectory $projectRoot
        Write-Output 'Shike started from the local application cache.'
        exit 0
    }
    New-Item -ItemType Directory -Force -Path $runtimeRoot | Out-Null
    $buildLock = $null
    try {
        while (-not $buildLock) {
            try { $buildLock = [IO.File]::Open((Join-Path $runtimeRoot 'build.lock'), 'OpenOrCreate', 'ReadWrite', 'None') }
            catch [IO.IOException] { Start-Sleep -Milliseconds 250 }
        }
        $sourceHash = Get-SourceHash
        if (-not (Test-CachedBuild $sourceHash)) {
            Write-Output 'Preparing the application after source changes. Future launches use the cached application.'
            Stop-ProjectApp
            Initialize-Toolchain
            do {
                $sourceHash = Get-SourceHash
                & npm.cmd run build
                if ($LASTEXITCODE -ne 0) { throw 'Frontend build failed.' }
                & cargo build --manifest-path src-tauri/Cargo.toml --features custom-protocol --bin shike
                if ($LASTEXITCODE -ne 0) { throw 'Desktop build failed.' }
                $finishedHash = Get-SourceHash
            } while ($sourceHash -ne $finishedHash)
            $nextProgram = Join-Path $runtimeRoot 'shike.next.exe'
            Copy-Item -LiteralPath (Join-Path $projectRoot 'src-tauri\target\debug\shike.exe') -Destination $nextProgram -Force
            Move-Item -LiteralPath $nextProgram -Destination $runtimeProgram -Force
            $manifest = @{ sourceHash = $sourceHash } | ConvertTo-Json
            [IO.File]::WriteAllText($manifestPath, $manifest)
        }
    } finally {
        if ($buildLock) { $buildLock.Dispose() }
    }
    Start-Process -FilePath $runtimeProgram -WorkingDirectory $projectRoot
    Write-Output 'Shike started.'
    exit 0
}

Initialize-Toolchain
switch ($Action) {
    'check' { & cargo check --manifest-path src-tauri/Cargo.toml }
    'test' { & cargo test --manifest-path src-tauri/Cargo.toml }
    'dev' { Stop-ProjectApp; & (Join-Path $projectRoot 'node_modules\.bin\tauri.cmd') dev }
}
exit $LASTEXITCODE
