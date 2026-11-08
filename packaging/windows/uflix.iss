; Windows installer (Inno Setup 6): per-user, no admin, Start menu + desktop shortcuts, uninstall entry.
; The in-app updater runs it with /VERYSILENT; [Run] restarts UFlix afterwards.
#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif

[Setup]
AppId={{8B4E8C1E-6E1F-4C2B-9C3A-5F7D2A1E9B10}
AppName=UFlix
AppVersion={#AppVersion}
AppPublisher=UFlix
DefaultDirName={localappdata}\Programs\UFlix
DefaultGroupName=UFlix
PrivilegesRequired=lowest
OutputDir=..\..\out\windows
OutputBaseFilename=UFlix-win-Setup
SetupIconFile=uflix.ico
UninstallDisplayIcon={app}\uflix.exe
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
CloseApplications=force
LicenseFile=..\..\LICENSE

[Files]
Source: "..\..\target\release\uflix.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\runtimes\win-x64\native\*"; DestDir: "{app}\runtimes\win-x64\native"; Flags: ignoreversion recursesubdirs
Source: "..\..\LICENSE"; DestDir: "{app}"

[Icons]
Name: "{group}\UFlix"; Filename: "{app}\uflix.exe"
Name: "{userdesktop}\UFlix"; Filename: "{app}\uflix.exe"

[Run]
Filename: "{app}\uflix.exe"; Flags: nowait postinstall skipifsilent
Filename: "{app}\uflix.exe"; Flags: nowait; Check: WizardSilent
