param(
  [string]$Out = "J:/01_projects/pomodoro/shot.png",
  [int]$ClickX = -1,
  [int]$ClickY = -1,
  [switch]$NoTop
)

Add-Type -AssemblyName System.Drawing

$sig = @'
using System;
using System.Text;
using System.Runtime.InteropServices;
public class Win {
  public delegate bool D(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] public static extern bool EnumWindows(D d, IntPtr l);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int c);
  [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h, IntPtr after, int x, int y, int cx, int cy, uint flags);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, uint dx, uint dy, uint d, IntPtr e);
  public struct RECT { public int Left, Top, Right, Bottom; }
  public static IntPtr Find(uint target) {
    IntPtr found = IntPtr.Zero;
    EnumWindows((h, l) => {
      uint pid; GetWindowThreadProcessId(h, out pid);
      if (pid == target && IsWindowVisible(h)) { found = h; return false; }
      return true;
    }, IntPtr.Zero);
    return found;
  }
}
'@

Add-Type -TypeDefinition $sig

$TOPMOST = [IntPtr](-1)
$NOTOPMOST = [IntPtr](-2)
$SWP_NOMOVE = 0x0002
$SWP_NOSIZE = 0x0001

$proc = Get-Process pomodoro -ErrorAction SilentlyContinue | Select-Object -First 1
if (-not $proc) { Write-Output "process-not-running"; exit 1 }

$h = [Win]::Find([uint32]$proc.Id)
if ($h -eq [IntPtr]::Zero) { Write-Output "window-not-found"; exit 1 }

[Win]::ShowWindow($h, 9) | Out-Null
[Win]::SetForegroundWindow($h) | Out-Null
if (-not $NoTop) {
  # Bring the window above everything else so the screen capture shows it.
  [Win]::SetWindowPos($h, $TOPMOST, 0, 0, 0, 0, ($SWP_NOMOVE -bor $SWP_NOSIZE)) | Out-Null
}
Start-Sleep -Milliseconds 700

$r = New-Object Win+RECT
[Win]::GetWindowRect($h, [ref]$r) | Out-Null
$w = $r.Right - $r.Left
$ht = $r.Bottom - $r.Top
Write-Output "window: $($r.Left),$($r.Top) ${w}x${ht}"

if ($ClickX -ge 0) {
  $sx = $r.Left + $ClickX
  $sy = $r.Top + $ClickY
  [Win]::SetCursorPos($sx, $sy) | Out-Null
  Start-Sleep -Milliseconds 250
  [Win]::mouse_event(0x0002, 0, 0, 0, [IntPtr]::Zero)
  [Win]::mouse_event(0x0004, 0, 0, 0, [IntPtr]::Zero)
  Write-Output "clicked $sx,$sy"
  Start-Sleep -Milliseconds 500
}

$bmp = New-Object System.Drawing.Bitmap $w, $ht
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.CopyFromScreen($r.Left, $r.Top, 0, 0, $bmp.Size)
$bmp.Save($Out)
Write-Output "saved $Out"

if (-not $NoTop) {
  [Win]::SetWindowPos($h, $NOTOPMOST, 0, 0, 0, 0, ($SWP_NOMOVE -bor $SWP_NOSIZE)) | Out-Null
}
