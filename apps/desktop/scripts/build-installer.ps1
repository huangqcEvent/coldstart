$ErrorActionPreference = 'Stop'

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\..\..')).Path
$buildCache = Join-Path $repoRoot '.tauri-local'
New-Item -ItemType Directory -Force -Path $buildCache | Out-Null
$env:LOCALAPPDATA = $buildCache

Push-Location $repoRoot
try {
    & cargo build -p coldstart-mcp --release --locked
    if ($LASTEXITCODE -ne 0) {
        throw "coldstart-mcp release build failed with exit code $LASTEXITCODE"
    }

    & pnpm --dir apps/desktop tauri build
    if ($LASTEXITCODE -ne 0) {
        throw "Tauri NSIS build failed with exit code $LASTEXITCODE"
    }

    $installer = Get-ChildItem `
        -Path (Join-Path $repoRoot 'target\release\bundle\nsis') `
        -Filter '*-setup.exe' `
        -File |
        Sort-Object LastWriteTime -Descending |
        Select-Object -First 1
    if (-not $installer) {
        throw 'Tauri did not produce an NSIS setup executable'
    }
    $stream = [System.IO.File]::OpenRead($installer.FullName)
    try {
        $sha256 = [System.Security.Cryptography.SHA256]::Create()
        try {
            $hash = [System.BitConverter]::ToString($sha256.ComputeHash($stream)).Replace('-', '')
        }
        finally {
            $sha256.Dispose()
        }
    }
    finally {
        $stream.Dispose()
    }
    Write-Output "Installer: $($installer.FullName)"
    Write-Output "SHA256: $hash"
}
finally {
    Pop-Location
}
