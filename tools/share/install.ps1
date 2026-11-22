# Installs Pulley for Illustrator and After Effects on Windows (per user, no administrator rights).
# Run it through "Install Pulley.bat" next to it.
param(
    [string]$ExtensionsDir = "$env:APPDATA\Adobe\CEP\extensions",
    [switch]$SkipBridge,
    [switch]$SkipAppCheck
)
$ErrorActionPreference = "Stop"
$here = Split-Path -Parent $MyInvocation.MyCommand.Path
Write-Host "Installing Pulley (created by Haneesh Raj Banisetti, MIT License)" -ForegroundColor Cyan
Write-Host ""

try {
    if (-not $SkipAppCheck) {
        $open = Get-Process -Name Illustrator, AfterFX -ErrorAction SilentlyContinue
        if ($open) {
            Write-Host "Please close Illustrator and After Effects first, then run the installer again." -ForegroundColor Yellow
            exit 1
        }
    }

    # 1. The extensions. A .zxp is a signed zip; it is unpacked into your per-user Adobe extensions folder.
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $packages = @(
        @{ Pattern = "Pulley-Illustrator-*.zxp"; Folder = "dev.pulley.illustrator"; Label = "Illustrator" },
        @{ Pattern = "Pulley-AfterEffects-*.zxp"; Folder = "dev.pulley.ae"; Label = "After Effects" }
    )
    New-Item -ItemType Directory -Force $ExtensionsDir | Out-Null
    foreach ($p in $packages) {
        $zxp = Get-ChildItem -Path $here -Filter $p.Pattern | Sort-Object Name -Descending | Select-Object -First 1
        if (-not $zxp) { throw "Couldn't find $($p.Pattern) next to this script. Unzip the whole download first." }
        $dst = Join-Path $ExtensionsDir $p.Folder
        if (Test-Path $dst) { Remove-Item $dst -Recurse -Force }
        [IO.Compression.ZipFile]::ExtractToDirectory($zxp.FullName, $dst)
        if (-not (Test-Path (Join-Path $dst "CSXS\manifest.xml"))) { throw "The $($p.Label) package looks damaged. Download it again." }
        Write-Host "  Installed the $($p.Label) extension"
    }

    # 2. Let Illustrator and After Effects load extensions that aren't from Adobe Exchange.
    foreach ($v in 11, 12) {
        $key = "HKCU:\Software\Adobe\CSXS.$v"
        New-Item -Path $key -Force | Out-Null
        Set-ItemProperty -Path $key -Name PlayerDebugMode -Value "1" -Type String
    }
    Write-Host "  Allowed Adobe apps to load the extensions"

    # 3. The Pulley bridge: a small tray app that carries artwork between the apps.
    if (-not $SkipBridge) {
        $setup = Get-ChildItem -Path $here -Filter "pulley-bridge_*_x64-setup.exe" | Select-Object -First 1
        if (-not $setup) { throw "Couldn't find the Pulley bridge installer next to this script." }
        Write-Host "  Opening the Pulley bridge installer. Click through it (if Windows says it protected your PC, click 'More info', then 'Run anyway')."
        Start-Process -FilePath $setup.FullName -Wait
    }

    Write-Host ""
    Write-Host "Done." -ForegroundColor Green
    Write-Host "Open After Effects, then Window > Extensions > Pulley."
    Write-Host "Open Illustrator, then Window > Extensions > Pulley. Select artwork and press Push to After Effects."
} catch {
    Write-Host ""
    Write-Host "Installation failed: $($_.Exception.Message)" -ForegroundColor Red
    exit 1
}
