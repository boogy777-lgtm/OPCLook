param([string]$ExePath)
Add-Type -AssemblyName System.Drawing
$icon = [System.Drawing.Icon]::ExtractAssociatedIcon($ExePath)
$i = $icon.ToBitmap()
Write-Output ("ExtractAssociatedIcon: " + ($icon -ne $null))
Write-Output ("Size: " + $i.Width + "x" + $i.Height)
Write-Output ("PixelFormat: " + $i.PixelFormat)
$i.Save("$PSScriptRoot\_icon_extracted.png", [System.Drawing.Imaging.ImageFormat]::Png)
Write-Output ("Saved: $PSScriptRoot\_icon_extracted.png")