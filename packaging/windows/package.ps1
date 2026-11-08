# Windows installer: out\windows\UFlix-win-Setup.exe (release build + the bundled libmpv-2.dll), with Inno Setup 6.
# -Version x.y.z overrides the VERSION file. Needs the natives (bash Native/natives.sh fetch win-x64).
param([string]$Version = "")
$ErrorActionPreference = 'Stop'
$root = Resolve-Path (Join-Path $PSScriptRoot '..\..')
Set-Location $root
if (-not $Version) { $Version = (Get-Content VERSION -Raw).Trim() }
if (-not (Test-Path 'runtimes\win-x64\native\libmpv-2.dll')) { throw 'runtimes\win-x64\native\libmpv-2.dll missing (run Native/natives.sh fetch win-x64)' }

$env:UFLIX_VERSION = $Version
cargo build --release --locked -p uflix
if ($LASTEXITCODE) { throw 'cargo build failed' }

$iscc = @("${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe", "$env:ProgramFiles\Inno Setup 6\ISCC.exe") | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $iscc) { throw 'Inno Setup 6 (ISCC.exe) not found: choco install innosetup' }
& $iscc "/DAppVersion=$Version" 'packaging\windows\uflix.iss'
if ($LASTEXITCODE) { throw 'ISCC failed' }
Write-Host "UFlix $Version -> out\windows\UFlix-win-Setup.exe"
