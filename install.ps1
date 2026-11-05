# Download a published Trammel release into the user binary directory. No administrator.
# This does not publish anything. A missing release prints the build command and exits.
$ErrorActionPreference = "Stop"
# TRAMMEL_VERSION=X.Y.Z pins a release; the default is the latest one.
$base = if ($env:TRAMMEL_VERSION) { "https://github.com/IronAdamant/Trammel/releases/download/v$($env:TRAMMEL_VERSION)" } else { "https://github.com/IronAdamant/Trammel/releases/latest/download" }
$label = if ($env:TRAMMEL_VERSION) { "v$($env:TRAMMEL_VERSION)" } else { "(latest)" }
$arch = if ($env:PROCESSOR_ARCHITECTURE -match "ARM64") { "arm64" } else { "x64" }
$asset = "trammel-windows-$arch"
$url = "$base/$asset.tar.gz"
$sums = "$base/SHA256SUMS"
$dest = Join-Path $env:LOCALAPPDATA "trammel\bin"
New-Item -ItemType Directory -Force -Path $dest | Out-Null
$tmp = Join-Path ([System.IO.Path]::GetTempPath()) ("trammel-" + [guid]::NewGuid().ToString("n"))
New-Item -ItemType Directory -Force -Path $tmp | Out-Null
try {
    Invoke-WebRequest -Uri $url -OutFile (Join-Path $tmp "$asset.tar.gz")
    Invoke-WebRequest -Uri $sums -OutFile (Join-Path $tmp "SHA256SUMS")
} catch {
    Write-Error "No GitHub Release $label with $asset. Build with: cargo build --release"
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
