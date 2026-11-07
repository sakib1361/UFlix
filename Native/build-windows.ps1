# Builds the self-contained tsdl.dll (libtorrent, Boost and OpenSSL linked statically, dynamic CRT),
# checks it and installs it with BUILDINFO.txt into Engines/NativeRunner/runtimes/win-x64/native.
# Run from a "x64 Native Tools / Developer PowerShell for VS 2022" so cl.exe, dumpbin and ninja are on PATH:
#   $env:VCPKG_ROOT = "D:\vcpkg"; Native\build-windows.ps1
$ErrorActionPreference = 'Stop'

$here = $PSScriptRoot
$repo = Split-Path $here -Parent
$out = Join-Path $repo 'Engines\NativeRunner\runtimes\win-x64\native'
$preset = 'windows-x64-release'

if (-not $env:VCPKG_ROOT) { throw 'Set VCPKG_ROOT to a vcpkg checkout.' }
if (-not (Get-Command cl.exe -ErrorAction SilentlyContinue)) { throw 'cl.exe not found: run from a VS x64 developer shell.' }

Push-Location $here
try {
    cmake --preset $preset
    if ($LASTEXITCODE) { throw 'configure failed' }
    cmake --build --preset $preset
    if ($LASTEXITCODE) { throw 'build failed' }

    if (Test-Path $out) { Remove-Item $out -Recurse -Force }
    New-Item -ItemType Directory $out | Out-Null
    cmake --install "build\$preset" --prefix $out
    if ($LASTEXITCODE) { throw 'install failed' }
    $dll = Join-Path $out 'tsdl.dll'

    # Definition of done (doc/CrossPlatformDev.md, Windows form)
    $deps = dumpbin /nologo /dependents $dll | Where-Object { $_ -match '^\s+\S+\.dll\s*$' } | ForEach-Object { $_.Trim() }
    $foreign = $deps | Where-Object { $_ -notmatch '^(KERNEL32|ADVAPI32|USER32|WS2_32|IPHLPAPI|CRYPT32|BCRYPT|MSWSOCK|DBGHELP|SHELL32|OLE32|SECUR32|NTDLL|MSVCP140.*|VCRUNTIME140.*|api-ms-win-.*)\.dll$' }
    if ($foreign) { throw "unexpected dependencies: $($foreign -join ', ')" }

    $exports = dumpbin /nologo /exports $dll | Where-Object { $_ -match '^\s+\d+\s+[0-9A-F]+\s+[0-9A-F]+\s+(\S+)' } | ForEach-Object { $Matches[1] }
    $bad = $exports | Where-Object { $_ -notlike 'tsdl_*' }
    if ($bad) { throw "exports outside tsdl_*: $($bad -join ', ')" }

    $commit = git -C $repo rev-parse HEAD
    git -C $repo diff --quiet HEAD -- Native
    if ($LASTEXITCODE) { $commit += '+dirty' }
    $version = (Select-String -Path CMakeLists.txt -Pattern 'set\(PRODUCT_VERSION_(MAJOR|MINOR|PATCH) (\d+)\)' | ForEach-Object { $_.Matches[0].Groups[2].Value }) -join '.'
    $baseline = (Get-Content vcpkg.json -Raw | ConvertFrom-Json).'builtin-baseline'
    $cl = (cmd /c 'cl 2>&1' | Select-Object -First 1)

    @"
tsdl.dll $version
built:          $((Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ssZ'))
source commit:  $commit
vcpkg commit:   $(git -C $env:VCPKG_ROOT rev-parse HEAD)
vcpkg baseline: $baseline
triplet:        x64-windows-tsdl (static deps, dynamic CRT, release)
compiler:       $cl
os:             $([System.Environment]::OSVersion.VersionString)
dependencies:   $($deps -join ' ')
size:           $((Get-Item $dll).Length) bytes
sha256:         $((Get-FileHash $dll -Algorithm SHA256).Hash.ToLower())
"@ | Set-Content -Encoding utf8 (Join-Path $out 'BUILDINFO.txt')

    Get-Content (Join-Path $out 'BUILDINFO.txt')
}
finally {
    Pop-Location
}
