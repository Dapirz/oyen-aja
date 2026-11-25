# Put the hihu.exe you built where the copy in daily use runs from, so
# building and testing never touch it.
#
#   powershell -ExecutionPolicy Bypass -File tools\install.ps1            # update it
#   powershell -ExecutionPolicy Bypass -File tools\install.ps1 -Identity  # first time
#
# Closes Hihu the way its Quit does (WM_CLOSE to its island window, so Windows'
# notification banners and its volume/brightness bar come back), copies
# target\release\hihu.exe to %LOCALAPPDATA%\Programs\Hihu, points Start with
# Windows there when it's on, and starts that copy. Settings and data stay in
# %APPDATA%\Hihu and %LOCALAPPDATA%\Hihu, shared by both copies.
#
# -Identity also registers the identity package (tools\identity.ps1) for that
# folder: Notifications then work in the installed copy, not in test builds
# run from target\release. Needed once, or after the folder changed.
param(
    [switch]$Identity,
    [string]$Dest = (Join-Path $env:LOCALAPPDATA 'Programs\Hihu')
)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$built = Join-Path $root 'target\release\hihu.exe'
if (-not (Test-Path $built)) { throw "no $built (build it first: cargo build -p poni --release)" }

Add-Type @"
using System; using System.Text; using System.Runtime.InteropServices;
public static class HihuQuit {
  public delegate bool EnumProc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc f, IntPtr l);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassName(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
  public static int Close() {
    int n = 0;
    EnumWindows((h, p) => { var c = new StringBuilder(64); GetClassName(h, c, 64); if (c.ToString() == "Hihu.Island") { PostMessage(h, 0x0010, IntPtr.Zero, IntPtr.Zero); n++; } return true; }, IntPtr.Zero);
    return n;
  }
}
"@
if ([HihuQuit]::Close() -gt 0) {
    $t = [Diagnostics.Stopwatch]::StartNew()
    while ((Get-Process hihu -ErrorAction SilentlyContinue) -and $t.Elapsed.TotalSeconds -lt 10) { Start-Sleep -Milliseconds 200 }
    if (Get-Process hihu -ErrorAction SilentlyContinue) { throw 'Hihu is still running after 10 s; quit it from the tray and run this again' }
}

New-Item -ItemType Directory -Force $Dest | Out-Null
$exe = Join-Path $Dest 'hihu.exe'
Copy-Item $built $exe -Force
"copied to $exe"

if ($Identity) {
    & (Join-Path $PSScriptRoot 'identity.ps1') register -ExeDir $Dest
}

# Start with Windows on: start this copy (Hihu itself only repoints a Run
# value whose exe is gone).
$run = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
if ((Get-ItemProperty $run -ErrorAction SilentlyContinue).Hihu) {
    Set-ItemProperty $run -Name Hihu -Value "`"$exe`""
    "Start with Windows: $exe"
}

Start-Process $exe
'started'
