[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Version,
    [Parameter(Mandatory)][string]$NsisExecutable,
    [string]$WorkRoot = "$PSScriptRoot\..\..\target\packaging"
)
$ErrorActionPreference = 'Stop'
$root = [IO.Path]::GetFullPath("$PSScriptRoot\..\..")
$WorkRoot = [IO.Path]::GetFullPath($WorkRoot)
$release = & "$PSScriptRoot\..\Get-ReleaseVersion.ps1" -Tag "v$Version"
$runtime = Join-Path $WorkRoot 'runtime'
$payload = Join-Path $WorkRoot "payload-$Version"
$dist = Join-Path $root 'dist'
foreach ($file in 'ffmpeg\ffmpeg.exe', 'adb\adb.exe', 'adb\NOTICE.txt') {
    if (!(Test-Path "$runtime\$file")) { throw "Missing runtime\$file; run Build-Runtime.ps1 first." }
}
$env:DRAWDOCK_VERSION = $release.Version
Push-Location $root
try {
    & cargo build --locked --release -p dd-host --target x86_64-pc-windows-msvc `
        --config 'target.x86_64-pc-windows-msvc.rustflags=["-C","target-feature=+crt-static"]'
    if ($LASTEXITCODE -ne 0) { throw 'Release Rust build failed' }
} finally {
    Pop-Location
}
New-Item -ItemType Directory -Force $payload, "$payload\runtime", $dist | Out-Null
foreach ($name in 'dd-host.exe', 'drawdock-launcher.exe') {
    $exe = "$root\target\x86_64-pc-windows-msvc\release\$name"
    if ((Get-Item $exe).VersionInfo.CompanyName -ne 'Mike Cerreto' -or
        (Get-Item $exe).VersionInfo.ProductVersion -ne $Version) {
        throw "Incorrect publisher/version in $exe"
    }
    Copy-Item $exe $payload -Force
}
Copy-Item "$root\LICENSE", "$PSScriptRoot\THIRD-PARTY-NOTICES.txt" $payload -Force
Copy-Item "$runtime\*" "$payload\runtime" -Recurse -Force
$installer = "$dist\DrawDock-$Version-windows-x64-setup.exe"
& $NsisExecutable "/DVERSION=$Version" "/DPAYLOAD=$payload" "/DOUTPUT=$installer" "$PSScriptRoot\DrawDock.nsi"
if ($LASTEXITCODE -ne 0) { throw 'NSIS installer compilation failed' }
Compress-Archive -Path "$payload\*" -DestinationPath "$dist\DrawDock-$Version-windows-x64-portable.zip" -Force
$sourcePayload = Join-Path $WorkRoot "source-payload-$Version"
New-Item -ItemType Directory -Force $sourcePayload | Out-Null
$dependencies = Get-Content "$PSScriptRoot\dependencies.json" -Raw | ConvertFrom-Json
foreach ($name in 'ffmpeg', 'x264', 'nv-codec-headers') {
    Copy-Item "$WorkRoot\downloads\$($dependencies.$name.file)" $sourcePayload -Force
}
Copy-Item "$PSScriptRoot\Build-Runtime.ps1", "$PSScriptRoot\dependencies.json",
    "$PSScriptRoot\THIRD-PARTY-NOTICES.txt", "$PSScriptRoot\SOURCE-BUILD.txt" $sourcePayload -Force
Copy-Item "$WorkRoot\sources\toolchain-packages.txt" $sourcePayload -Force
Compress-Archive -Path "$sourcePayload\*" -DestinationPath "$dist\DrawDock-$Version-third-party-sources.zip" -Force
Write-Output "Windows release assets: $dist"
