# Builds the Windows download: dist\share\Pulley-<version>-Windows.zip, holding the signed .zxp
# packages, the bridge installer, the files in this folder and the license.
# Run after `node tools/package-zxp.mjs ...` and `node tools/package-bridge.mjs`:
#   powershell -ExecutionPolicy Bypass -File tools\share\build-bundle.ps1
$ErrorActionPreference = "Stop"
$root = Resolve-Path (Join-Path $PSScriptRoot "..\..")
$version = (Select-String -Path (Join-Path $root "Cargo.toml") -Pattern '^version\s*=\s*"([^"]+)"' | Select-Object -First 1).Matches[0].Groups[1].Value
$name = "Pulley-$version-Windows"
$out = Join-Path $root "dist\share\$name"

$needed = @(
    "dist\zxp\Pulley-Illustrator-$version.zxp",
    "dist\zxp\Pulley-AfterEffects-$version.zxp",
    "dist\installers\pulley-bridge_${version}_x64-setup.exe"
)
foreach ($n in $needed) {
    if (-not (Test-Path (Join-Path $root $n))) { throw "Missing $n. Build it first (see docs/PACKAGING.md)." }
}

if (Test-Path $out) { Remove-Item $out -Recurse -Force }
New-Item -ItemType Directory -Force $out | Out-Null
foreach ($n in $needed) { Copy-Item (Join-Path $root $n) $out }
foreach ($f in "install.ps1", "Install Pulley.bat", "READ ME FIRST.txt") { Copy-Item (Join-Path $PSScriptRoot $f) $out }
Copy-Item (Join-Path $root "LICENSE") (Join-Path $out "LICENSE.txt")

# Windows line endings for the files people open in Notepad or run with cmd.
foreach ($f in "Install Pulley.bat", "READ ME FIRST.txt", "LICENSE.txt") {
    $p = Join-Path $out $f
    [IO.File]::WriteAllText($p, ([IO.File]::ReadAllText($p) -replace "`r?`n", "`r`n"))
}

$zip = Join-Path $root "dist\share\$name.zip"
if (Test-Path $zip) { Remove-Item $zip -Force }
Compress-Archive -Path $out -DestinationPath $zip
Write-Host "Built $zip ($([math]::Round((Get-Item $zip).Length / 1MB, 1)) MB)"
