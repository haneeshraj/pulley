# Signs Windows binaries with Authenticode (signtool), if a certificate is configured.
# Without one it prints why it skipped and exits 0, so packaging works without certificates.
#
#   powershell -NoProfile -File tools/sign-windows.ps1 -Files dist\installers\Pulley_0.1.0_x64-setup.exe
#
# Configure one of:
#   PULLEY_WIN_SIGN_PFX            path to a .pfx file, with PULLEY_WIN_SIGN_PFX_PASSWORD
#   PULLEY_WIN_SIGN_THUMBPRINT     SHA-1 thumbprint of a certificate in your certificate store
#                                  (also covers hardware tokens / cloud HSM providers)
# Optional:
#   PULLEY_WIN_SIGN_TIMESTAMP      RFC 3161 timestamp server (default http://timestamp.digicert.com)
#   PULLEY_SIGNTOOL                path to signtool.exe (default: newest in the Windows 10/11 SDK)
param([Parameter(Mandatory = $true)][string[]]$Files)
$ErrorActionPreference = "Stop"

$pfx = $env:PULLEY_WIN_SIGN_PFX
$thumb = $env:PULLEY_WIN_SIGN_THUMBPRINT
if (-not $pfx -and -not $thumb) {
    Write-Output "Windows signing skipped: set PULLEY_WIN_SIGN_PFX (+ PULLEY_WIN_SIGN_PFX_PASSWORD) or PULLEY_WIN_SIGN_THUMBPRINT."
    exit 0
}

$signtool = $env:PULLEY_SIGNTOOL
if (-not $signtool) {
    $kits = Join-Path ${env:ProgramFiles(x86)} "Windows Kits\10\bin"
    $signtool = Get-ChildItem -Path $kits -Recurse -Filter signtool.exe -ErrorAction SilentlyContinue |
        Where-Object { $_.FullName -match "\\x64\\" } | Sort-Object FullName -Descending | Select-Object -First 1 -ExpandProperty FullName
}
if (-not $signtool -or -not (Test-Path $signtool)) { throw "signtool.exe not found: install the Windows SDK or set PULLEY_SIGNTOOL." }

$ts = $env:PULLEY_WIN_SIGN_TIMESTAMP
if (-not $ts) { $ts = "http://timestamp.digicert.com" }

foreach ($f in $Files) {
    if (-not (Test-Path $f)) { throw "not found: $f" }
    $signArgs = @("sign", "/fd", "SHA256", "/tr", $ts, "/td", "SHA256", "/d", "Pulley")
    if ($pfx) { $signArgs += @("/f", $pfx, "/p", $env:PULLEY_WIN_SIGN_PFX_PASSWORD) } else { $signArgs += @("/sha1", $thumb) }
    $signArgs += $f
    & $signtool @signArgs
    if ($LASTEXITCODE -ne 0) { throw "signtool failed for $f" }
    & $signtool verify /pa /v $f | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "signature verification failed for $f" }
    Write-Output "signed and verified: $f"
}
