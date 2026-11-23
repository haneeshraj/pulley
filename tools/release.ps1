# Releases Pulley in one command: bumps the version, checks the changelog, runs the checks CI runs,
# commits, tags and pushes. Pushing the tag starts .github/workflows/release.yml on GitHub, which
# builds the signed extensions, the bridge installer and the Windows zip, and publishes the release.
#
#   .\release 0.1.1            (or: powershell -ExecutionPolicy Bypass -File tools\release.ps1 0.1.1)
#   .\release 0.1.1 -DryRun    show what would happen, change nothing
#   .\release 0.1.1 -SkipTests skip the local checks (CI still runs them)
param(
    [Parameter(Mandatory = $true, Position = 0)][string]$Version,
    [switch]$DryRun,
    [switch]$SkipTests,
    [switch]$Yes
)
$ErrorActionPreference = "Stop"
$root = Resolve-Path (Join-Path $PSScriptRoot "..")
Set-Location $root

function Step($msg) { Write-Host "`n== $msg" -ForegroundColor Cyan }
function Fail($msg) { Write-Host "`n$msg" -ForegroundColor Red; exit 1 }
# Runs git and returns its output, or nothing if git failed. Never throws (for checks and cleanup).
function Get-GitOutput {
    $ErrorActionPreference = "Continue"
    $out = & git.exe @args 2>$null
    if ($LASTEXITCODE -ne 0) { return $null }
    return $out
}

function Invoke-GitCommand {
    # Windows PowerShell turns git's progress messages on stderr into errors; judge by exit code.
    $ErrorActionPreference = "Continue"
    $out = & git.exe @args 2>&1 | ForEach-Object { "$_" }
    if ($LASTEXITCODE -ne 0) { throw "git $($args -join ' ') failed: $($out -join ' ')" }
    return $out
}

if ($Version -notmatch '^\d+\.\d+\.\d+(-[0-9A-Za-z.]+)?$') { Fail "Version must look like 1.2.3 (or 1.2.3-beta.1), not '$Version'." }
$tag = "v$Version"

Step "Checking the repository"
$branch = (Invoke-GitCommand branch --show-current).Trim()
if ($branch -ne "main") { Fail "Releases are made from main; you're on '$branch'." }
if (Invoke-GitCommand status --porcelain) { Fail "There are uncommitted changes. Commit or stash them first." }
$remote = (Get-GitOutput remote get-url origin)
if (-not $remote) {
    if (-not $DryRun) { Fail "No 'origin' remote. Add your GitHub repository first: git remote add origin <url>" }
    $remote = "(no origin remote yet)"
}
if (Invoke-GitCommand tag --list $tag) { Fail "Tag $tag already exists locally." }
if (-not $DryRun) {
    Invoke-GitCommand fetch origin --tags --quiet | Out-Null
    if (Get-GitOutput ls-remote --tags origin "refs/tags/$tag") { Fail "Tag $tag already exists on GitHub." }
    $behind = Get-GitOutput rev-list --count "HEAD..origin/main"   # nothing on GitHub yet: no answer
    if ($LASTEXITCODE -eq 0 -and $behind -and [int]$behind -gt 0) { Fail "main is $behind commit(s) behind GitHub. Pull first." }
}
Write-Host "  branch main, clean, remote $remote"

Step "Changelog"
$changelog = Get-Content CHANGELOG.md -Raw
$heading = [regex]::Match($changelog, "(?m)^## " + [regex]::Escape($Version) + "( \(unreleased\))?\s*$")
if (-not $heading.Success) { Fail "CHANGELOG.md has no '## $Version' section. Add one listing what changed, then run this again." }
$date = Get-Date -Format "yyyy-MM-dd"
$newChangelog = $changelog.Remove($heading.Index, $heading.Length).Insert($heading.Index, "## $Version ($date)")
Write-Host "  '## $Version' found; it will be dated $date"

Step "Version"
$cargo = Get-Content Cargo.toml -Raw
$current = [regex]::Match($cargo, '(?m)^version = "([^"]+)"').Groups[1].Value
Write-Host "  $current -> $Version"

if ($DryRun) {
    Write-Host "`nDry run: would update Cargo.toml, Cargo.lock, figma-plugin/package*.json and CHANGELOG.md," -ForegroundColor Yellow
    Write-Host "run the checks$(if ($SkipTests) { ' (skipped)' }), commit 'Release $tag', tag $tag and push main and $tag to $remote." -ForegroundColor Yellow
    exit 0
}

if (-not $Yes) {
    $answer = Read-Host "`nRelease $tag and publish it on GitHub? (y/N)"
    if ($answer -notmatch '^(y|yes)$') { Fail "Cancelled. Nothing was changed." }
}

$changed = @()
try {
    Step "Updating version numbers"
    if ($current -ne $Version) {
        $cargo = [regex]::Replace($cargo, '(?m)^version = "[^"]+"', "version = `"$Version`"", 1)
        [IO.File]::WriteAllText("$root\Cargo.toml", $cargo)
        Push-Location figma-plugin
        npm version $Version --no-git-tag-version --allow-same-version | Out-Null
        Pop-Location
        cargo update --workspace --quiet
        $changed += "Cargo.toml", "Cargo.lock", "figma-plugin/package.json", "figma-plugin/package-lock.json"
    }
    [IO.File]::WriteAllText("$root\CHANGELOG.md", $newChangelog)
    $changed += "CHANGELOG.md"

    if (-not $SkipTests) {
        Step "Running the checks CI runs"
        foreach ($cmd in @(
            "cargo fmt --all --check",
            "cargo clippy --workspace --all-targets -- -D warnings",
            "cargo test --workspace --quiet",
            "cargo xtask icons --check"
        )) {
            Write-Host "  $cmd"
            cmd /c "$cmd >nul 2>&1"
            if ($LASTEXITCODE -ne 0) { throw "'$cmd' failed. Run it to see why." }
        }
        Push-Location figma-plugin
        Write-Host "  figma-plugin: npm test"
        cmd /c "npm test >nul 2>&1"
        $code = $LASTEXITCODE
        Pop-Location
        if ($code -ne 0) { throw "'npm test' in figma-plugin failed. Run it to see why." }
    }

    Step "Commit, tag and push"
    Invoke-GitCommand add -- $changed | Out-Null
    Invoke-GitCommand commit -q -m "Release $tag" | Out-Null
    Invoke-GitCommand tag -a $tag -m "Pulley $Version" | Out-Null
    Invoke-GitCommand push -q origin main | Out-Null
    Invoke-GitCommand push -q origin $tag | Out-Null
} catch {
    Write-Host "`n$($_.Exception.Message)" -ForegroundColor Red
    $pushed = Get-GitOutput ls-remote --tags origin "refs/tags/$tag"
    if (-not $pushed) {
        Get-GitOutput tag -d $tag | Out-Null
        if ((Get-GitOutput log -1 --format=%s) -eq "Release $tag") { Get-GitOutput reset -q --soft HEAD~1 }
        if ($changed) { Get-GitOutput checkout -q HEAD -- $changed }
        Write-Host "Undid the release changes; your repository is as it was." -ForegroundColor Yellow
    }
    exit 1
}

$web = $remote -replace '\.git$', '' -replace '^git@github\.com:', 'https://github.com/'
Write-Host "`nReleased $tag. GitHub is now building it (about 15-20 minutes):" -ForegroundColor Green
Write-Host "  progress: $web/actions"
Write-Host "  release:  $web/releases/tag/$tag"
