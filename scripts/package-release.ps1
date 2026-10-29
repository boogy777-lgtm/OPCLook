param(
    [string]$Target = "i686-pc-windows-msvc",
    [string]$Configuration = "release"
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Push-Location $root
try {
    $profileFlag = "--$Configuration"
    cargo build --package opclook --bin opclook --target $Target $profileFlag
    if ($LASTEXITCODE -ne 0) { throw "opclook build failed" }
    cargo build --package opclook-cli --bin opclook-cli --target $Target $profileFlag
    if ($LASTEXITCODE -ne 0) { throw "opclook-cli build failed" }
    cargo build --package opclook-nethelper --bin opclook-nethelper --target $Target $profileFlag
    if ($LASTEXITCODE -ne 0) { throw "opclook-nethelper build failed" }

    $release = Join-Path $root "target\$Target\$Configuration"
    $app = Join-Path $release "opclook.exe"
    $cli = Join-Path $release "opclook-cli.exe"
    $helper = Join-Path $release "opclook-nethelper.exe"
    foreach ($path in @($app, $cli, $helper)) {
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
    Copy-Item $app, $cli, $helper -Destination $package
    Copy-Item (Join-Path $root "nethelper\nethelper.manifest") -Destination $package
    Copy-Item (Join-Path $root "README.md") -Destination $package
    Copy-Item (Join-Path $root "LICENSE") -Destination $package
    $images = Join-Path $package "docs\images"
    New-Item -ItemType Directory -Path $images -Force | Out-Null
    Copy-Item (Join-Path $root "docs\images\*") -Destination $images
    Write-Host "Package ready: $package"
    Write-Host "  opclook.exe (32-bit)"
    Write-Host "  opclook-cli.exe (32-bit)"
    Write-Host "  opclook-nethelper.exe (32-bit, requireAdministrator)"
} finally {
    Pop-Location
}
