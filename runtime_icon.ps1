Add-Type @"
using System;
using System.Runtime.InteropServices;
public class WinIcon {
    [DllImport("user32.dll", SetLastError = true)]
    public static extern IntPtr SendMessage(IntPtr hWnd, int Msg, IntPtr wParam, IntPtr lParam);
    [DllImport("user32.dll")]
    public static extern IntPtr GetClassLongPtr(IntPtr hWnd, int nIndex);
    public const int WM_GETICON = 0x7F;
    public const int ICON_BIG = 1;
    public const int ICON_SMALL = 0;
    public const int ICON_SMALL2 = 2;
    public const int GCLP_HICON = -14;
}
"@
$p = Start-Process -FilePath $args[0] -PassThru
Start-Sleep -Seconds 5
$p.Refresh()
$hwnd = $p.MainWindowHandle
if ($hwnd -eq [IntPtr]::Zero) { Write-Output "NO HWND"; exit 1 }
$big  = [WinIcon]::SendMessage($hwnd, [WinIcon]::WM_GETICON, [IntPtr][WinIcon]::ICON_BIG, [IntPtr]::Zero)
$small = [WinIcon]::SendMessage($hwnd, [WinIcon]::WM_GETICON, [IntPtr][WinIcon]::ICON_SMALL, [IntPtr]::Zero)
$cls  = [WinIcon]::GetClassLongPtr($hwnd, [WinIcon]::GCLP_HICON)
Write-Output ("hWnd: " + $hwnd)
Write-Output ("WM_GETICON BIG   = 0x" + $big.ToString("X"))
Write-Output ("WM_GETICON SMALL = 0x" + $small.ToString("X"))
Write-Output ("GCLP_HICON       = 0x" + $cls.ToString("X"))
$has = ($big -ne [IntPtr]::Zero) -or ($small -ne [IntPtr]::Zero) -or ($cls -ne [IntPtr]::Zero)
Write-Output ("WindowHasIcon: " + $has)
Stop-Process -Id $p.Id -Force
Write-Output "closed"