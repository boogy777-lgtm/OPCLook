param(
    [string]$Target = "i686-pc-windows-msvc",
    [string]$Configuration = "release"
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Push-Location $root
try {
    $profileFlag = "--$Configuration"
    cargo build --package opcda-egui --bin opcda-egui --target $Target $profileFlag
    if ($LASTEXITCODE -ne 0) { throw "opcda-egui build failed" }
    cargo build --package opcda-nethelper --bin opcda-nethelper --target $Target $profileFlag
    if ($LASTEXITCODE -ne 0) { throw "opcda-nethelper build failed" }

    $release = Join-Path $root "target\$Target\$Configuration"
    $app = Join-Path $release "opcda-egui.exe"
    $helper = Join-Path $release "opcda-nethelper.exe"
    foreach ($path in @($app, $helper)) {
        if (-not (Test-Path $path)) { throw "missing artifact: $path" }
        $stream = [System.IO.File]::OpenRead($path)
        try {
            $reader = New-Object System.IO.BinaryReader($stream)
            $stream.Seek(0x3C, [System.IO.SeekOrigin]::Begin) | Out-Null
            $peOffset = $reader.ReadInt32()
            $stream.Seek($peOffset + 4, [System.IO.SeekOrigin]::Begin) | Out-Null
            $machine = $reader.ReadUInt16()
            if ($machine -ne 0x014C) {
                throw ("{0} is not 32-bit x86 (PE machine 0x{1:X4})" -f $path, $machine)
            }
        } finally {
            $stream.Dispose()
        }
    }

    $package = Join-Path $release "package"
    if (Test-Path $package) { Remove-Item -Recurse -Force $package }
    New-Item -ItemType Directory -Path $package | Out-Null
    Copy-Item $app, $helper -Destination $package
    Copy-Item (Join-Path $root "nethelper\nethelper.manifest") -Destination $package
    Copy-Item (Join-Path $root "README.md") -Destination $package
    Write-Host "Package ready: $package"
    Write-Host "  opcda-egui.exe (32-bit)"
    Write-Host "  opcda-nethelper.exe (32-bit, requireAdministrator)"
} finally {
    Pop-Location
}
