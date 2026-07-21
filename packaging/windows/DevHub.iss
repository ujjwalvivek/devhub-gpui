#ifndef AppVersion
  #error AppVersion must be provided by the release workflow
#endif
#ifndef SourceDir
  #error SourceDir must be provided by the release workflow
#endif
#ifndef OutputDir
  #error OutputDir must be provided by the release workflow
#endif
#ifndef SetupIcon
  #error SetupIcon must be provided by the release workflow
#endif

[Setup]
AppId={{F16A94EF-D69C-49B8-8BF4-0AF08685816C}
AppName=DevHub
AppVersion={#AppVersion}
AppPublisher=DevHub
AppPublisherURL=https://github.com/ujjwalvivek/devhub-gpui
AppSupportURL=https://github.com/ujjwalvivek/devhub-gpui/issues
AppUpdatesURL=https://github.com/ujjwalvivek/devhub-gpui/releases
DefaultDirName={localappdata}\Programs\DevHub
DefaultGroupName=DevHub
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir={#OutputDir}
OutputBaseFilename=DevHub-Setup-{#AppVersion}-x64
SetupIconFile={#SetupIcon}
UninstallDisplayIcon={app}\devhub-gpui.exe
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
CloseApplications=yes
RestartApplications=no

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; GroupDescription: "Shortcuts:"; Flags: unchecked

[Files]
Source: "{#SourceDir}\devhub-gpui.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\devhub-mcp.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\README.md"; DestDir: "{app}\docs"; Flags: ignoreversion
Source: "{#SourceDir}\RELEASE.md"; DestDir: "{app}\docs"; Flags: ignoreversion
Source: "{#SourceDir}\LICENSE"; DestDir: "{app}\docs"; Flags: ignoreversion

[Icons]
Name: "{group}\DevHub"; Filename: "{app}\devhub-gpui.exe"; WorkingDir: "{app}"
Name: "{autodesktop}\DevHub"; Filename: "{app}\devhub-gpui.exe"; WorkingDir: "{app}"; Tasks: desktopicon

[Run]
Filename: "{app}\devhub-gpui.exe"; Description: "Launch DevHub"; Flags: nowait postinstall skipifsilent
