[CmdletBinding(SupportsShouldProcess)]
param(
    [string]$Output = '\\.\DISPLAY2',
    [ValidateRange(1, 65535)]
    [int]$Port = 47831,
    [string]$DeviceSerial = 'cfa3e5dce217',
    [string]$AdbPath = 'D:\Tools\android-dev\platform-tools\adb.exe',
    [switch]$Restart,
    [switch]$Pair,
    [ValidateSet('nvenc', 'software')]
    [string]$VideoEncoder = 'nvenc'
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$hostExe = Join-Path $PSScriptRoot 'target\debug\dd-host.exe'

if (-not (Test-Path -LiteralPath $hostExe -PathType Leaf)) {
    throw "Host executable not found. Build it first: cargo build -p dd-host"
}
if (-not (Test-Path -LiteralPath $AdbPath -PathType Leaf)) {
    throw "ADB not found at '$AdbPath'. Specify -AdbPath with its full path."
}

$report = & $hostExe doctor
if ($LASTEXITCODE -ne 0) { throw 'Could not enumerate host displays.' }
$displays = ($report | ConvertFrom-Json).outputs
if (-not @($displays | Where-Object { $_.name -eq $Output }).Count) {
    $available = ($displays | ForEach-Object { $_.name }) -join ', '
    throw "Display '$Output' is unavailable. Use -Output with one of: $available"
}

$deviceState = & $AdbPath -s $DeviceSerial get-state
if ($LASTEXITCODE -ne 0 -or $deviceState -ne 'device') {
    throw "Tablet '$DeviceSerial' is unavailable or not authorized in ADB."
}

$existingHosts = @(Get-CimInstance Win32_Process -Filter "Name = 'dd-host.exe'" |
    Where-Object { $_.ExecutablePath -ieq $hostExe })
if ($existingHosts.Count -gt 0 -and -not $Restart) {
    throw 'This workspace already has a running host. Use -Restart to stop it and start fresh.'
}

$mappings = @(& $AdbPath -s $DeviceSerial reverse --list)
if ($LASTEXITCODE -ne 0) { throw 'Could not inspect ADB reverse mappings.' }
$mappingPattern = '^\S+\s+tcp:' + $Port + '\s+tcp:' + $Port + '\s*$'
$mappingExists = @($mappings | Where-Object { $_ -match $mappingPattern }).Count -gt 0
$conflictingPattern = '^\S+\s+tcp:' + $Port + '\s+'
if (-not $mappingExists -and @($mappings | Where-Object { $_ -match $conflictingPattern }).Count) {
    throw "Tablet port $Port already has a different reverse mapping; refusing to replace it."
}

if (-not $PSCmdlet.ShouldProcess(
    "DrawDock on $Output, USB tablet $DeviceSerial",
    'Start a loopback host and configure USB forwarding; restart workspace hosts if requested'
)) { return }

foreach ($process in $existingHosts) {
    $running = Get-Process -Id $process.ProcessId -ErrorAction Stop
    try {
        $null = $running.Handle
        Stop-Process -Id $process.ProcessId -ErrorAction Stop
        $running.WaitForExit()
    }
    finally {
        $running.Dispose()
    }
}

$createdMapping = $false
try {
    if (-not $mappingExists) {
        & $AdbPath -s $DeviceSerial reverse "tcp:$Port" "tcp:$Port"
        if ($LASTEXITCODE -ne 0) { throw 'Could not configure USB forwarding.' }
        $createdMapping = $true
    }

    Write-Host "Starting DrawDock on $Output, listening only on 127.0.0.1:$Port."
    Write-Host 'Remembered tablet: tap Connect. New tablet: scan QR and approve remembering it.'
    Write-Host 'Use -Restart -Pair to open a new invitation. Ctrl+C stops the host.'
    Write-Host "Video: 1440p30, encoder $VideoEncoder. Use -VideoEncoder software if NVENC is unavailable."
    $hostArguments = @('serve', '--output', $Output, '--listen', '127.0.0.1', '--port', "$Port", '--video-encoder', $VideoEncoder)
    if ($Pair) { $hostArguments += '--pair' }
    & $hostExe @hostArguments
    if ($LASTEXITCODE -ne 0) { throw "DrawDock exited with code $LASTEXITCODE." }
}
finally {
    if ($createdMapping) {
        & $AdbPath -s $DeviceSerial reverse --remove "tcp:$Port"
        if ($LASTEXITCODE -ne 0) { Write-Warning "Could not remove USB forwarding for port $Port." }
    }
}
