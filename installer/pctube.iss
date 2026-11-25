; Inno Setup script. Built by tools\package.ps1 and by the release workflow:
;   iscc /DAppVersion=0.2.0 /DExePath=..\target\release\pc_tube.exe /DDmExePath=..\target\release\pctube_dm.exe installer\pctube.iss
#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
#ifndef ExePath
  #define ExePath "..\target\release\pc_tube.exe"
#endif
#ifndef DmExePath
  #define DmExePath "..\target\release\pctube_dm.exe"
#endif

[Setup]
AppId={{6B0E2C1A-5D7E-4F58-9B6C-2E3F4A1D9C70}
AppName=PC Tube
AppVersion={#AppVersion}
AppPublisher=PC Tube
DefaultDirName={autopf}\PC Tube
DefaultGroupName=PC Tube
UninstallDisplayIcon={app}\PC Tube.exe
; per-user install: no administrator rights needed
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
LicenseFile=..\LICENSE
OutputDir=..\dist
OutputBaseFilename=PC-Tube-{#AppVersion}-setup
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
SetupIconFile=..\assets\icon.ico
; the apps hold these mutexes while running: setup asks to close them before updating
AppMutex=Local\PcTube.SingleInstance,Local\PcTubeDownloads.SingleInstance
CloseApplications=yes

[Languages]
Name: "spanish"; MessagesFile: "compiler:Languages\Spanish.isl"
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; Flags: unchecked

[Files]
Source: "{#ExePath}"; DestDir: "{app}"; DestName: "PC Tube.exe"; Flags: ignoreversion
Source: "{#DmExePath}"; DestDir: "{app}"; DestName: "PC Tube Downloads.exe"; Flags: ignoreversion
Source: "..\LICENSE"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\README.md"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\PC Tube"; Filename: "{app}\PC Tube.exe"
Name: "{group}\PC Tube Descargas"; Filename: "{app}\PC Tube Downloads.exe"
Name: "{autodesktop}\PC Tube"; Filename: "{app}\PC Tube.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\PC Tube.exe"; Description: "{cm:LaunchProgram,PC Tube}"; Flags: nowait postinstall skipifsilent
; in-app updates run the installer silently: start the app again when it is done
Filename: "{app}\PC Tube.exe"; Flags: nowait skipifnotsilent

[Registry]
; the "start with Windows" entry goes away with the app
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueName: "PC Tube"; Flags: dontcreatekey uninsdeletevalue

[Code]
procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usPostUninstall then
    if MsgBox('Borrar tambien tus ajustes, la sesion de YouTube y los yt-dlp/ffmpeg descargados?' + #13#10 +
              '(%LOCALAPPDATA%\PcTube. Los videos descargados no se tocan.)', mbConfirmation, MB_YESNO or MB_DEFBUTTON2) = IDYES then
      DelTree(ExpandConstant('{localappdata}\PcTube'), True, True, True);
end;
