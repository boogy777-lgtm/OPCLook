param(
    [string]$OutPng = "app_verify.png"
)
Add-Type -AssemblyName System.Drawing

$cs = @"
using System;
using System.Runtime.InteropServices;
using System.Drawing;
using System.Drawing.Imaging;

public static class Cap {
    [DllImport("user32.dll")]
    public static extern bool SetProcessDpiAwarenessContext(IntPtr value);

    [DllImport("user32.dll")]
    public static extern IntPtr FindWindow(string lpClassName, string lpWindowName);

    [DllImport("user32.dll")]
    public static extern bool GetWindowRect(IntPtr hWnd, out RECT lpRect);

    [StructLayout(LayoutKind.Sequential)]
    public struct RECT { public int Left, Top, Right, Bottom; }

    public static void Capture(string cls, string title, string outFile) {
        SetProcessDpiAwarenessContext(new IntPtr(-4));
        IntPtr h = FindWindow(cls, title);
        if (h == IntPtr.Zero) { Console.WriteLine("WINDOW_NOT_FOUND"); return; }
        RECT r;
        GetWindowRect(h, out r);
        int w = r.Right - r.Left;
        int ht = r.Bottom - r.Top;
        using (Bitmap bmp = new Bitmap(w, ht))
        {
            using (Graphics g = Graphics.FromImage(bmp))
            {
                g.CopyFromScreen(r.Left, r.Top, 0, 0, new Size(w, ht));
            }
            bmp.Save(outFile, ImageFormat.Png);
        }
        Console.WriteLine("SAVED " + outFile + " " + w + "x" + ht);
    }
}
"@
Add-Type -TypeDefinition $cs
[Cap]::Capture($args[0], $args[1], $OutPng)