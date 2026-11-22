# Measures how much CPU Hihu Island (poni.exe) uses over a window of time,
# in CPU cycles (QueryProcessCycleTime), as AGENTS.md asks: Task Manager and
# TotalProcessorTime count in 15.6 ms ticks and miss short frames.
#
#   powershell -ExecutionPolicy Bypass -File tools\measure_cpu.ps1 [-Seconds 30]
#
# Prints the cycles per second and that as a share of one core at the
# processor's base clock, plus the working set (memory) at the end.
param([int]$Seconds = 30)

Add-Type @"
using System;
using System.Runtime.InteropServices;
public static class Cycles {
    [DllImport("kernel32.dll", SetLastError = true)]
    public static extern bool QueryProcessCycleTime(IntPtr process, out ulong cycles);
}
"@

$p = Get-Process poni -ErrorAction Stop | Select-Object -First 1
$mhz = (Get-CimInstance Win32_Processor | Select-Object -First 1).MaxClockSpeed
$c0 = [uint64]0
[void][Cycles]::QueryProcessCycleTime($p.Handle, [ref]$c0)
$t0 = [Diagnostics.Stopwatch]::StartNew()
Start-Sleep -Seconds $Seconds
$c1 = [uint64]0
[void][Cycles]::QueryProcessCycleTime($p.Handle, [ref]$c1)
$secs = $t0.Elapsed.TotalSeconds
$perSec = ($c1 - $c0) / $secs
$core = 100.0 * $perSec / ($mhz * 1e6)
$p.Refresh()
"{0:N0} Mcycles/s = {1:N2}% of one core at {2} MHz, over {3:N1} s; working set {4:N1} MB, private {5:N1} MB" -f ($perSec / 1e6), $core, $mhz, $secs, ($p.WorkingSet64 / 1MB), ($p.PrivateMemorySize64 / 1MB)
