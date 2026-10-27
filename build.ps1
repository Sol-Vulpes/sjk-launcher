# Builds the launcher in release mode and packs it into dist/.
#   ./build.ps1          build and pack
#   ./build.ps1 -Test    run the tests first
param([switch]$Test)
$ErrorActionPreference = 'Stop'
Set-Location $PSScriptRoot

if ($Test) {
    cargo test --locked --workspace
    if ($LASTEXITCODE) { exit $LASTEXITCODE }
}
cargo build --locked --release -p sjk-launcher
if ($LASTEXITCODE) { exit $LASTEXITCODE }

$arch = if ($env:PROCESSOR_ARCHITECTURE -eq 'ARM64') { 'arm64' } else { 'x64' }
$name = "sjk-launcher-windows-$arch"
$stage = "dist/$name"
if (Get-Process sjk-launcher -ErrorAction SilentlyContinue | Where-Object Path -like "$PSScriptRoot\dist\*") {
    Write-Error "Close the launcher running from dist\ first; Windows locks its exe."
}
Remove-Item -Recurse -Force $stage, "dist/$name.zip" -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $stage | Out-Null
# The fonts are built into the exe; their licence asks to travel with them.
Copy-Item target/release/sjk-launcher.exe, assets/fonts/*-OFL.txt $stage
Compress-Archive "$stage/*" "dist/$name.zip"
Write-Host "Built dist/$name.zip"
