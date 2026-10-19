# ncell-cli installer for Windows (PowerShell 5.1+)
#   irm https://raw.githubusercontent.com/rohityadav-sas/ncell-cli/main/install.ps1 | iex
#
# Optional environment variables:
#   NCELL_VERSION      release tag to install (default: latest), e.g. v0.5.3
#   NCELL_INSTALL_DIR  install directory (default: %LOCALAPPDATA%\Programs\ncell-cli)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
[Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12

$Repo = 'rohityadav-sas/ncell-cli'
$Version = if ($env:NCELL_VERSION) { $env:NCELL_VERSION } else { 'latest' }
$InstallDir = if ($env:NCELL_INSTALL_DIR) { $env:NCELL_INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA 'Programs\ncell-cli' }
$InstallDir = [IO.Path]::GetFullPath($InstallDir)

$cpu = if ($env:PROCESSOR_ARCHITEW6432) { $env:PROCESSOR_ARCHITEW6432 } else { $env:PROCESSOR_ARCHITECTURE }
$arch = switch ($cpu) {
    'AMD64' { 'x86_64' }
    'ARM64' { 'arm64' }
    default { throw "Unsupported architecture: $cpu" }
}

$api = if ($Version -eq 'latest') {
    "https://api.github.com/repos/$Repo/releases/latest"
} else {
    "https://api.github.com/repos/$Repo/releases/tags/$Version"
}

Write-Host "Looking up $Version release..."
$release = Invoke-RestMethod -Uri $api -Headers @{ 'User-Agent' = 'ncell-cli-installer' }
$asset = $release.assets | Where-Object { $_.name -like "*windows-$arch.zip" } | Select-Object -First 1
if (-not $asset) { throw "No release asset ending in 'windows-$arch.zip' was found" }

$tmp = Join-Path ([IO.Path]::GetTempPath()) ("ncell-" + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $tmp | Out-Null
try {
    $zip = Join-Path $tmp 'ncell.zip'
    Write-Host "Downloading $($asset.name)..."
    Invoke-WebRequest -Uri $asset.browser_download_url -OutFile $zip -UseBasicParsing
    Expand-Archive -Path $zip -DestinationPath (Join-Path $tmp 'out') -Force

    # The release zip keeps ncell.exe at its root next to LICENSE and docs\.
    $exe = Join-Path $tmp 'out\ncell.exe'
    if (-not (Test-Path -LiteralPath $exe)) { throw "ncell.exe was not found inside the archive" }

    New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
    try {
        Copy-Item -LiteralPath $exe -Destination (Join-Path $InstallDir 'ncell.exe') -Force
    }
    catch {
        throw "Could not write ncell.exe ($($_.Exception.Message)). If ncell is running, close it and run the installer again."
    }
}
finally {
    Remove-Item -Path $tmp -Recurse -Force -ErrorAction SilentlyContinue
}

Write-Host "Installed ncell to $InstallDir\ncell.exe"

$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
$entries = if ($userPath) { $userPath -split ';' | Where-Object { $_ } } else { @() }
if ($entries -notcontains $InstallDir) {
    $newPath = (@($entries) + $InstallDir) -join ';'
    [Environment]::SetEnvironmentVariable('Path', $newPath, 'User')
    $env:Path = "$env:Path;$InstallDir"
    Write-Host "Added $InstallDir to your PATH. Restart other terminals to pick it up."
}
Write-Host "Run: ncell --help"
