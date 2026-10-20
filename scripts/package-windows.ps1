# Package the desktop app for Windows.
#
#   powershell -ExecutionPolicy Bypass -File scripts\package-windows.ps1
#
# Output:
#   dist\pomodoro-windows-x64.zip        portable, unzip and run
#   dist\Pomodoro-<version>-Setup.exe    installer (needs Inno Setup 6)
#
# Messages stay in ASCII on purpose: Windows PowerShell 5.1 reads .ps1 files
# without a BOM as ANSI, which mangles non-ASCII text and can break parsing.

param(
  [switch]$SkipBuild
)

$ErrorActionPreference = "Stop"

$root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
Push-Location $root
try {
  if (-not $SkipBuild) {
    Write-Host "==> cargo build --release -p pomodoro-desktop"
    cargo build --release -p pomodoro-desktop --locked
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
  }

  $exe = Join-Path $root "target\release\pomodoro.exe"
  if (-not (Test-Path $exe)) { throw "executable not found: $exe" }

  $stage = Join-Path $root "dist\windows"
  if (Test-Path $stage) { Remove-Item -Recurse -Force $stage }
  New-Item -ItemType Directory -Force $stage | Out-Null
  Copy-Item $exe (Join-Path $stage "pomodoro.exe")

  # Add external runtime files here. This example's icons and fonts are
  # already compiled into the binary, so the folder holds only the exe.

  $zip = Join-Path $root "dist\pomodoro-windows-x64.zip"
  if (Test-Path $zip) { Remove-Item -Force $zip }
  Compress-Archive -Path (Join-Path $stage "*") -DestinationPath $zip
  Write-Host "==> $zip"

  $iscc = $null
  $cmd = Get-Command ISCC.exe -ErrorAction SilentlyContinue
  if ($null -ne $cmd) {
    $iscc = $cmd.Source
  } else {
    $candidates = @(
      (Join-Path ${env:ProgramFiles(x86)} "Inno Setup 6\ISCC.exe"),
      (Join-Path $env:ProgramFiles "Inno Setup 6\ISCC.exe")
    )
    foreach ($candidate in $candidates) {
      if ($candidate -and (Test-Path $candidate)) { $iscc = $candidate; break }
    }
  }

  if (-not $iscc) {
    Write-Host "==> Inno Setup not found; skipping the installer."
    Write-Host "    Install Inno Setup 6 (https://jrsoftware.org/isinfo.php) and rerun."
  } else {
    Write-Host "==> Inno Setup: $iscc"
    & $iscc "packaging\pomodoro.iss"
    if ($LASTEXITCODE -ne 0) { throw "ISCC failed" }
    Write-Host "==> dist\Pomodoro-*-Setup.exe"
  }
} finally {
  Pop-Location
}
