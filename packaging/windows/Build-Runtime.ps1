[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$MsysRoot,
    [string]$WorkRoot = "$PSScriptRoot\..\..\target\packaging",
    [switch]$SkipAdb
)
$ErrorActionPreference = 'Stop'
$WorkRoot = [IO.Path]::GetFullPath($WorkRoot)
if ($WorkRoot -match '\s') { throw 'Use an FFmpeg build directory without spaces.' }
$downloads = Join-Path $WorkRoot 'downloads'
$sources = Join-Path $WorkRoot 'sources'
$runtime = Join-Path $WorkRoot 'runtime'
New-Item -ItemType Directory -Force $downloads, $sources, "$runtime\ffmpeg", "$runtime\adb" | Out-Null
$dependencies = Get-Content "$PSScriptRoot\dependencies.json" -Raw | ConvertFrom-Json
foreach ($dependency in $dependencies.PSObject.Properties) {
    if ($SkipAdb -and $dependency.Name -eq 'adb') { continue }
    $item = $dependency.Value
    $archive = Join-Path $downloads $item.file
    if (!(Test-Path $archive)) {
        $supplied = Join-Path $PSScriptRoot $item.file
        if (Test-Path $supplied) {
            Copy-Item $supplied $archive
        } else {
            Invoke-WebRequest $item.url -OutFile $archive
        }
    }
    if ((Get-FileHash $archive -Algorithm SHA256).Hash -ne $item.sha256) {
        throw "Checksum mismatch: $archive. Remove this specific download and retry."
    }
    if ($dependency.Name -ne 'adb') {
        $destination = Join-Path $sources $dependency.Name
        New-Item -ItemType Directory -Force $destination | Out-Null
        & tar -xf $archive --strip-components=1 -C $destination
        if ($LASTEXITCODE -ne 0) { throw "Extract $archive failed" }
    }
}
if (!$SkipAdb) {
    Expand-Archive "$downloads\platform-tools.zip" "$WorkRoot\adb" -Force
    foreach ($name in 'adb.exe', 'AdbWinApi.dll', 'AdbWinUsbApi.dll', 'NOTICE.txt', 'source.properties') {
        Copy-Item "$WorkRoot\adb\platform-tools\$name" "$runtime\adb\$name" -Force
    }
}
$env:MSYSTEM = 'MINGW64'
$env:CHERE_INVOKING = '1'
$build = @'
set -euo pipefail
root="$(cygpath -u "$1")"
prefix="$root/prefix"
export PKG_CONFIG_PATH="$prefix/lib/pkgconfig"
cd "$root/sources/nv-codec-headers"
make PREFIX="$prefix" install
cd "$root/sources/x264"
./configure --prefix="$prefix" --enable-static --disable-cli --disable-opencl --extra-ldflags=-static
make -j4
make install
cd "$root/sources/ffmpeg"
./configure --prefix="$prefix" --target-os=mingw32 --arch=x86_64 \
  --disable-autodetect --disable-shared --enable-static --enable-gpl \
  --disable-everything --disable-doc --disable-debug --disable-network \
  --enable-ffmpeg --disable-ffprobe --disable-ffplay --enable-avdevice \
  --enable-indev=gdigrab --enable-decoder=bmp,rawvideo \
  --enable-libx264 --enable-encoder=libx264,h264_nvenc --enable-nvenc --enable-ffnvcodec \
  --enable-filter=scale,format --enable-swscale \
  --enable-muxer=tee,h264,framecrc --enable-protocol=pipe \
  --pkg-config-flags=--static --extra-ldflags=-static
make -j4
cp ffmpeg.exe "$root/runtime/ffmpeg/ffmpeg.exe"
cp COPYING.GPLv2 "$root/runtime/ffmpeg/LICENSE-FFmpeg.txt"
cp ../x264/COPYING "$root/runtime/ffmpeg/LICENSE-x264.txt"
cp ../nv-codec-headers/include/ffnvcodec/nvEncodeAPI.h "$root/runtime/ffmpeg/nvEncodeAPI-license.h"
mkdir -p "$root/runtime/ffmpeg/toolchain-licenses"
cp -r /mingw64/share/licenses/gcc* /mingw64/share/licenses/headers* \
  /mingw64/share/licenses/crt* /mingw64/share/licenses/winpthreads* \
  /mingw64/share/licenses/libgcc* /mingw64/share/licenses/libwinpthread* \
  "$root/runtime/ffmpeg/toolchain-licenses/"
pacman -Q > "$root/sources/toolchain-packages.txt"
'@
& "$MsysRoot\usr\bin\bash.exe" --login -c $build drawdock $WorkRoot
if ($LASTEXITCODE -ne 0) { throw 'Native FFmpeg build failed' }
$features = & "$runtime\ffmpeg\ffmpeg.exe" -hide_banner -encoders 2>&1 | Out-String
if ($LASTEXITCODE -ne 0 -or $features -notmatch 'libx264' -or $features -notmatch 'h264_nvenc') {
    throw 'Bundled FFmpeg is missing required H.264 encoders'
}
Copy-Item "$PSScriptRoot\Build-Runtime.ps1", "$PSScriptRoot\dependencies.json" $sources -Force
Copy-Item "$PSScriptRoot\THIRD-PARTY-NOTICES.txt" "$runtime\THIRD-PARTY-NOTICES.txt" -Force
Write-Output "Runtime ready: $runtime"
