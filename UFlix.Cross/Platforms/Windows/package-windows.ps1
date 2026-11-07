# Builds the Windows installer with Velopack into out/windows/: UFlix-win-Setup.exe (per-user install, no admin, Start menu +
# desktop shortcuts, uninstall entry) and the release feed files (UFlix-<version>-full.nupkg,
# releases.win.json) that release.yml uploads for the in-app updater.
# Unpackaged WinAppSDK head, self-contained (.NET + Windows App SDK), bundled libmpv (Native/mpv/fetch-windows.sh) + tsdl.dll.
# Version: -Version, else the VERSION file at the repo root. Velopack needs SemVer.
# $env:UFlixUpdateRepo (a GitHub repo URL, set by CI) turns on the in-app update check against its releases.
# Usage: UFlix.Cross\Platforms\Windows\package-windows.ps1 [-Version 1.2.3]
param(
    [string]$Version
)
$ErrorActionPreference = 'Stop'

$Root = (Resolve-Path (Join-Path $PSScriptRoot '..\..\..')).Path
$Out = Join-Path $Root 'out\windows'
$Publish = Join-Path $Out 'publish'
$Icon = Join-Path $Root 'UFlix.Cross\Assets\uflix.ico'

if (-not $Version) { $Version = (Get-Content (Join-Path $Root 'VERSION') -Raw).Trim() }
if ($Version -notmatch '^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$') { throw "'$Version' is not a SemVer version (x.y.z[-pre])" }

if (Test-Path $Out) { Remove-Item -LiteralPath $Out -Recurse -Force }
New-Item -ItemType Directory -Force $Publish | Out-Null

dotnet publish (Join-Path $Root 'UFlix.Cross\UFlix.Cross.csproj') -f net10.0-windows10.0.26100 -c Release -r win-x64 --self-contained -o $Publish -p:Version=$Version
if ($LASTEXITCODE -ne 0) { throw "dotnet publish failed ($LASTEXITCODE)" }

$native = Join-Path $Publish 'runtimes\win-x64\native'
foreach ($dll in 'libmpv-2.dll', 'tsdl.dll') {
    if (-not (Test-Path (Join-Path $native $dll))) { throw "$dll not bundled in $native" }
}
if (-not (Test-Path (Join-Path $Publish 'resources.pri'))) { throw 'resources.pri missing: the app would crash at startup (EnableMsixTooling must stay on)' }
$foreign = Get-ChildItem (Join-Path $Publish 'runtimes') -Directory | Where-Object Name -ne 'win-x64'
if ($foreign) { throw "non-Windows runtimes in the publish output: $($foreign.Name -join ', ')" }

# vpk is pinned in dotnet-tools.json
Push-Location $Root
try {
    dotnet tool restore
    if ($LASTEXITCODE -ne 0) { throw "dotnet tool restore failed ($LASTEXITCODE)" }
    dotnet vpk pack --runtime win-x64 --packId UFlix --packTitle UFlix --packAuthors UFlix --packVersion $Version --packDir $Publish `
        --mainExe UFlix.Cross.exe --icon $Icon --noPortable --outputDir $Out
    if ($LASTEXITCODE -ne 0) { throw "vpk pack failed ($LASTEXITCODE)" }
}
finally { Pop-Location }

Remove-Item -LiteralPath $Publish -Recurse -Force

Write-Host ''
Write-Host "UFlix $Version"
Get-ChildItem $Out -File | ForEach-Object { '{0,8:N0} MB  {1}' -f ($_.Length / 1MB), $_.Name }
