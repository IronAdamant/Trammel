# Download a published Trammel release into the user binary directory. No administrator.
# This does not publish anything. A missing release prints the build command and exits.
$ErrorActionPreference = "Stop"
$version = if ($env:TRAMMEL_VERSION) { $env:TRAMMEL_VERSION } else { "4.0.1" }
$arch = if ($env:PROCESSOR_ARCHITECTURE -match "ARM64") { "arm64" } else { "x64" }
$asset = "trammel-windows-$arch"
$url = "https://github.com/IronAdamant/Trammel/releases/download/v$version/$asset.tar.gz"
$sums = "https://github.com/IronAdamant/Trammel/releases/download/v$version/SHA256SUMS"
$dest = Join-Path $env:LOCALAPPDATA "trammel\bin"
New-Item -ItemType Directory -Force -Path $dest | Out-Null
$tmp = Join-Path ([System.IO.Path]::GetTempPath()) ("trammel-" + [guid]::NewGuid().ToString("n"))
New-Item -ItemType Directory -Force -Path $tmp | Out-Null
try {
    Invoke-WebRequest -Uri $url -OutFile (Join-Path $tmp "$asset.tar.gz")
    Invoke-WebRequest -Uri $sums -OutFile (Join-Path $tmp "SHA256SUMS")
} catch {
    Write-Error "No GitHub Release v$version yet. Build with: cargo build --release"
    exit 1
}
$archive = Join-Path $tmp "$asset.tar.gz"
$hash = (Get-FileHash -Algorithm SHA256 $archive).Hash.ToLower()
$expected = Select-String -Path (Join-Path $tmp "SHA256SUMS") -Pattern ([regex]::Escape("$asset.tar.gz")) | Select-Object -First 1
if (-not $expected -or -not $expected.Line.ToLower().StartsWith($hash)) {
    Write-Error "Checksum mismatch for $asset.tar.gz"
    exit 1
}
tar -xzf $archive -C $dest --strip-components=1
Write-Output "installed $dest\trammel.exe"
