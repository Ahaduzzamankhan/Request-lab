; RequestLab Windows installer (Inno Setup 6).
;
; Built in CI with:
;   ISCC.exe /DAppVersion=0.1.2 /DSourceDir=..\dist installer\requestlab.iss
;
; The installer only places the two shipped binaries and their metadata. User
; data lives in %APPDATA%\RequestLab and is never touched by installing,
; upgrading or uninstalling, so collections, environments, history and settings
; survive all three.

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif

#ifndef SourceDir
  #define SourceDir "..\dist"
#endif

#define AppName "RequestLab"
#define AppPublisher "RequestLab"
#define AppExeName "RequestLab.exe"
#define CliExeName "requestlab-cli.exe"
#define AppId "{{7C1F3E52-9B4A-4C57-9F1B-2F8A6D0C4E11}"

[Setup]
AppId={#AppId}
AppName={#AppName}
AppVersion={#AppVersion}
AppVerName={#AppName} {#AppVersion}
AppPublisher={#AppPublisher}
DefaultDirName={autopf}\{#AppName}
DefaultGroupName={#AppName}
UninstallDisplayName={#AppName} {#AppVersion}
UninstallDisplayIcon={app}\{#AppExeName}
OutputDir=Output
OutputBaseFilename=RequestLab-Setup-{#AppVersion}-x64
SetupIconFile=..\assets\branding\requestlab.ico
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
; Per-user by default (no elevation); the user can choose a machine-wide
; installation from the privileges dialog.
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
DisableProgramGroupPage=yes
CloseApplications=yes
RestartApplications=no
VersionInfoVersion={#AppVersion}
VersionInfoCompany={#AppPublisher}
VersionInfoDescription={#AppName} installer
VersionInfoProductName={#AppName}
VersionInfoProductVersion={#AppVersion}

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: checkedonce

[Files]
Source: "{#SourceDir}\RequestLab.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\requestlab-cli.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\LICENSE"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\version.json"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\{#AppName}"; Filename: "{app}\{#AppExeName}"
Name: "{group}\{#AppName} command line"; Filename: "{app}\{#CliExeName}"
Name: "{group}\{cm:UninstallProgram,{#AppName}}"; Filename: "{uninstallexe}"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#AppExeName}"; Tasks: desktopicon

[Run]
Filename: "{app}\{#AppExeName}"; Description: "{cm:LaunchProgram,{#StringChange(AppName, '&', '&&')}}"; Flags: nowait postinstall skipifsilent

[UninstallDelete]
; Only files this installer created inside {app}. The application's data
; directory is deliberately absent: uninstalling must not delete user data.
Type: filesandordirs; Name: "{app}\work"
Type: dirifempty; Name: "{app}"
