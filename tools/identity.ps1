# Hihu Island's identity package: lets the hihu.exe you built read other
# apps' notifications (the Notifications add-on), which Windows allows only
# for an app with a package identity.
#
#   powershell -ExecutionPolicy Bypass -File tools\identity.ps1 register
#   powershell -ExecutionPolicy Bypass -File tools\identity.ps1 status
#   powershell -ExecutionPolicy Bypass -File tools\identity.ps1 unregister
#
# register: stages installer\identity\AppxManifest.xml with its logos in
# target\identity, then registers it for your Windows account as loose files
# pointing at the exe's folder (-ExeDir, default target\release). That needs
# Developer Mode (Settings > System > For developers), not a certificate or
# admin rights. Nothing is copied: the exe stays where it is, with its data in
# %APPDATA%\Hihu. Close Hihu first, start it again after.
#
# Rebuilding hihu.exe keeps the registration. Moving the exe (another
# -ExeDir) means register again. unregister removes it; the exe then runs
# without identity, as before, and Notifications is off.
param(
    [Parameter(Position = 0)][ValidateSet('register', 'unregister', 'status')][string]$Action = 'status',
    [string]$ExeDir
)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$name = 'HihuIsland.Identity'
if (-not $ExeDir) { $ExeDir = Join-Path $root 'target\release' }

function Get-Registered { Get-AppxPackage -Name $name }

switch ($Action) {
    'status' {
        $p = Get-Registered
        if ($p) { "registered: $($p.PackageFullName)"; "exe folder: $ExeDir" } else { 'not registered' }
        $dev = (Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\AppModelUnlock' -ErrorAction SilentlyContinue).AllowDevelopmentWithoutDevLicense
        "Developer Mode: $(if ($dev -eq 1) { 'on' } else { 'off' })"
    }
    'unregister' {
        $p = Get-Registered
        if (-not $p) { 'not registered'; return }
        Remove-AppxPackage -Package $p.PackageFullName
        # The logos copied beside the exe for it.
        Remove-Item (Join-Path $ExeDir 'Assets') -Recurse -ErrorAction SilentlyContinue
        Remove-Item (Join-Path $ExeDir 'resources.pri') -ErrorAction SilentlyContinue
        'unregistered: restart Hihu to run without identity'
    }
    'register' {
        $exe = Join-Path $ExeDir 'hihu.exe'
        if (-not (Test-Path $exe)) { throw "no hihu.exe in $ExeDir (build it first: cargo build -p poni --release)" }
        if (Get-Process hihu, poni -ErrorAction SilentlyContinue) { throw 'close Hihu first (tray > Quit), then run this again' }
        $stage = Join-Path $root 'target\identity'
        New-Item -ItemType Directory -Force (Join-Path $stage 'Assets') | Out-Null
        Copy-Item (Join-Path $root 'installer\identity\AppxManifest.xml') $stage -Force
        # The logos the manifest names, from the Hihu icon, in the sizes the
        # taskbar, Task Manager and Start ask for (scale-* and targetsize-*,
        # plated and unplated), indexed in resources.pri so Windows finds them.
        # Without them a packaged exe shows a blank icon there.
        Add-Type -AssemblyName System.Drawing
        $src = [System.Drawing.Image]::FromFile((Join-Path $root 'design\hihu\hihu-1024.png'))
        $logos = @(@('StoreLogo.png', 50), @('Square150x150Logo.png', 150), @('Square44x44Logo.png', 44),
            @('StoreLogo.scale-100.png', 50), @('StoreLogo.scale-200.png', 100),
            @('Square150x150Logo.scale-100.png', 150), @('Square150x150Logo.scale-200.png', 300),
            @('Square44x44Logo.scale-100.png', 44), @('Square44x44Logo.scale-200.png', 88))
        foreach ($n in 16, 24, 32, 48, 256) {
            $logos += , @("Square44x44Logo.targetsize-$n.png", $n)
            $logos += , @("Square44x44Logo.targetsize-${n}_altform-unplated.png", $n)
        }
        foreach ($l in $logos) {
            $bmp = New-Object System.Drawing.Bitmap $l[1], $l[1]
            $g = [System.Drawing.Graphics]::FromImage($bmp)
            $g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
            $g.DrawImage($src, 0, 0, $l[1], $l[1])
            $bmp.Save((Join-Path $stage "Assets\$($l[0])"), [System.Drawing.Imaging.ImageFormat]::Png)
            $g.Dispose(); $bmp.Dispose()
        }
        $src.Dispose()
        # resources.pri, with the Windows SDK's makepri (newest one found).
        $makepri = Get-ChildItem "${env:ProgramFiles(x86)}\Windows Kits\10\bin\*\x64\makepri.exe" -ErrorAction SilentlyContinue | Sort-Object FullName | Select-Object -Last 1
        if ($makepri) {
            $cfg = Join-Path $stage 'priconfig.xml'
            & $makepri.FullName createconfig /cf $cfg /dq en-US /o | Out-Null
            & $makepri.FullName new /pr $stage /cf $cfg /of (Join-Path $stage 'resources.pri') /o | Out-Null
            Remove-Item $cfg -ErrorAction SilentlyContinue
            if (-not (Test-Path (Join-Path $stage 'resources.pri'))) { 'makepri made no resources.pri: the icon may stay blank in the taskbar' }
        } else {
            'No Windows SDK makepri.exe: the icon may stay blank in the taskbar and Task Manager'
        }
        # Windows looks for a sparse package's logos in its external location
        # (the exe's folder), not in the package's own: a copy goes there too.
        $exeAssets = Join-Path $ExeDir 'Assets'
        New-Item -ItemType Directory -Force $exeAssets | Out-Null
        Copy-Item (Join-Path $stage 'Assets\*') $exeAssets -Force
        if (Test-Path (Join-Path $stage 'resources.pri')) { Copy-Item (Join-Path $stage 'resources.pri') $ExeDir -Force }
        if (Get-Registered) { Remove-AppxPackage -Package (Get-Registered).PackageFullName }
        Add-AppxPackage -Register (Join-Path $stage 'AppxManifest.xml') -ExternalLocation $ExeDir
        "registered: $((Get-Registered).PackageFullName)"
        "Start Hihu again ($exe), then switch Notifications on in Add-ons."
    }
}
