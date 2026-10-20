; Inno Setup 打包脚本：生成带卸载项与开始菜单快捷方式的安装包。
;
; 用法（需先安装 Inno Setup 6，并把 ISCC.exe 加入 PATH）：
;   ISCC.exe packaging\pomodoro.iss
;
; 前置条件：先跑 `cargo build --release -p pomodoro-desktop`，
; 或直接用 `scripts\package-windows.ps1`，它会自动完成两步。
;
; 发布正式版本前，请按 website/docs/packaging.md 的要求：
;   1. 先给 pomodoro.exe 签名，再放进安装包；
;   2. 配置 signed uninstaller，否则卸载程序没有签名。

#define AppName "番茄钟"
#define AppVersion "0.1.0"
#define AppPublisher "Example"
#define AppExe "pomodoro.exe"

[Setup]
; AppId 必须跨版本稳定，否则新版本会变成“另一个应用”，而不是升级。
AppId=Pomodoro.GpuiKit.Example
AppName={#AppName}
AppVersion={#AppVersion}
AppPublisher={#AppPublisher}
DefaultDirName={autopf}\Pomodoro
DefaultGroupName={#AppName}
; 安装包输出到项目根的 dist 目录。
OutputDir=..\dist
OutputBaseFilename=Pomodoro-{#AppVersion}-Setup
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
UninstallDisplayName={#AppName}
; 有图标时取消下一行注释，并把 .ico 放到 packaging 目录。
; SetupIconFile=pomodoro.ico

[Languages]
Name: "chinesesimplified"; MessagesFile: "compiler:Languages\ChineseSimplified.isl"
Name: "english"; MessagesFile: "compiler:Default.isl"

[Files]
Source: "..\target\release\{#AppExe}"; DestDir: "{app}"; Flags: ignoreversion
; 应用需要随包分发的外部文件（主题、字体、许可）在此追加，例如：
; Source: "..\assets\*"; DestDir: "{app}\assets"; Flags: ignoreversion recursesubdirs

[Icons]
Name: "{group}\{#AppName}"; Filename: "{app}\{#AppExe}"
Name: "{group}\卸载 {#AppName}"; Filename: "{uninstallexe}"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#AppExe}"; Tasks: desktopicon

[Tasks]
Name: "desktopicon"; Description: "创建桌面快捷方式"; GroupDescription: "附加任务:"

[Run]
Filename: "{app}\{#AppExe}"; Description: "启动 {#AppName}"; Flags: nowait postinstall skipifsilent
