[CmdletBinding(SupportsShouldProcess)]
param(
    [ValidateSet('USB', 'WiFi')]
    [string]$Transport = 'USB',
    [string]$ListenAddress,
    [string]$Output,
    [ValidateRange(1, 65535)]
    [int]$Port = 47831,
    [string]$DeviceSerial,
    [string]$AdbPath,
    [switch]$Restart,
    [switch]$Pair,
    [ValidateSet('nvenc', 'software')]
    [string]$VideoEncoder = 'software'
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$hostExe = Join-Path $PSScriptRoot 'target\debug\dd-host.exe'

if (-not (Test-Path -LiteralPath $hostExe -PathType Leaf)) {
    throw "Host executable not found. Build it first: cargo build -p dd-host"
}
if ([string]::IsNullOrWhiteSpace($Output)) {
    throw 'Specify -Output with an exact device name from: .\target\debug\dd-host.exe outputs'
}
$bindAddress = '127.0.0.1'
if ($Transport -eq 'WiFi') {
    $ip = $null
    if (-not [System.Net.IPAddress]::TryParse($ListenAddress, [ref]$ip) -or
        $ip.AddressFamily -ne [System.Net.Sockets.AddressFamily]::InterNetwork -or
        $ip.ToString() -ne $ListenAddress) {
        throw 'WiFi requires -ListenAddress with an explicit numeric private IPv4 address assigned to this PC.'
    }
    $bytes = $ip.GetAddressBytes()
    $isPrivate = $bytes[0] -eq 10 -or
        ($bytes[0] -eq 172 -and $bytes[1] -ge 16 -and $bytes[1] -le 31) -or
        ($bytes[0] -eq 192 -and $bytes[1] -eq 168)
    if (-not $isPrivate) {
        throw 'WiFi requires an RFC1918 private IPv4 address; loopback, public, link-local and wildcard listeners are not allowed.'
    }
    $assigned = @(Get-NetIPAddress -AddressFamily IPv4 -ErrorAction Stop |
        Where-Object { $_.IPAddress -eq $ListenAddress -and $_.AddressState -eq 'Preferred' })
    if ($assigned.Count -ne 1) {
        throw "ListenAddress '$ListenAddress' is not a preferred IPv4 address assigned to one local interface."
    }
    $profile = @(Get-NetConnectionProfile -ErrorAction Stop |
        Where-Object { $_.InterfaceIndex -eq $assigned[0].InterfaceIndex })
    if ($profile.Count -ne 1 -or $profile[0].NetworkCategory -notin @('Private', 'DomainAuthenticated')) {
        throw "Interface '$($assigned[0].InterfaceAlias)' must have a Private or DomainAuthenticated network profile. No network setting was changed."
    }
    $bindAddress = $ListenAddress
} else {
    if ($PSBoundParameters.ContainsKey('ListenAddress')) {
        throw '-ListenAddress is only supported with -Transport WiFi. USB always binds to 127.0.0.1.'
    }
    if ([string]::IsNullOrWhiteSpace($DeviceSerial)) {
        throw 'USB requires -DeviceSerial with an explicitly selected authorized ADB device.'
    }
    if ([string]::IsNullOrWhiteSpace($AdbPath) -or
        -not (Test-Path -LiteralPath $AdbPath -PathType Leaf)) {
        throw "ADB not found at '$AdbPath'. Specify -AdbPath with its full path."
    }
}

$report = & $hostExe doctor
if ($LASTEXITCODE -ne 0) { throw 'Could not enumerate host displays.' }
$displays = ($report | ConvertFrom-Json).outputs
if (-not @($displays | Where-Object { $_.name -eq $Output }).Count) {
    $available = ($displays | ForEach-Object { $_.name }) -join ', '
    throw "Display '$Output' is unavailable. Use -Output with one of: $available"
}

if ($Transport -eq 'USB') {
    $deviceState = & $AdbPath -s $DeviceSerial get-state
    if ($LASTEXITCODE -ne 0 -or $deviceState -ne 'device') {
        throw "Tablet '$DeviceSerial' is unavailable or not authorized in ADB."
    }
}

$existingHosts = @(Get-CimInstance Win32_Process -Filter "Name = 'dd-host.exe'" |
    Where-Object { $_.ExecutablePath -ieq $hostExe })
if ($existingHosts.Count -gt 0 -and -not $Restart) {
    throw 'This workspace already has a running host. Use -Restart to stop it and start fresh.'
}

$mappingExists = $false
if ($Transport -eq 'USB') {
    $mappings = @(& $AdbPath -s $DeviceSerial reverse --list)
    if ($LASTEXITCODE -ne 0) { throw 'Could not inspect ADB reverse mappings.' }
    $mappingPattern = '^\S+\s+tcp:' + $Port + '\s+tcp:' + $Port + '\s*$'
    $mappingExists = @($mappings | Where-Object { $_ -match $mappingPattern }).Count -gt 0
    $conflictingPattern = '^\S+\s+tcp:' + $Port + '\s+'
    if (-not $mappingExists -and @($mappings | Where-Object { $_ -match $conflictingPattern }).Count) {
        throw "Tablet port $Port already has a different reverse mapping; refusing to replace it."
    }
}

$action = if ($Transport -eq 'USB') {
    'Start a loopback host and configure USB forwarding; restart workspace hosts if requested'
} else {
    'Start a private-interface TLS host without ADB; restart workspace hosts if requested'
}
if (-not $PSCmdlet.ShouldProcess(
    "DrawDock on $Output, $Transport endpoint ${bindAddress}:$Port",
    $action
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
    if ($Transport -eq 'USB' -and -not $mappingExists) {
        & $AdbPath -s $DeviceSerial reverse --no-rebind "tcp:$Port" "tcp:$Port"
        if ($LASTEXITCODE -ne 0) { throw 'Could not configure USB forwarding.' }
        $createdMapping = $true
    }

    Write-Host "Starting DrawDock on $Output, listening only on ${bindAddress}:$Port."
    if ($Transport -eq 'WiFi') {
        Write-Host 'WiFi/TCP uses no ADB or USB cable. Connect the tablet to the same trusted LAN.'
        Write-Host 'Firewall rules are unchanged. If blocked, obtain approval for a scoped inbound TCP rule.'
        Write-Host 'Switching from a saved USB endpoint requires Pair another PC and a new LAN QR invitation.'
    }
    Write-Host 'Remembered tablet: tap Connect. New tablet: scan QR and approve remembering it.'
    Write-Host 'Use -Restart -Pair to open a new invitation. Ctrl+C stops the host.'
    Write-Host "Video: 1440p30, encoder $VideoEncoder. Use -VideoEncoder software if NVENC is unavailable."
    $hostArguments = @('serve', '--output', $Output, '--listen', $bindAddress, '--port', "$Port", '--video-encoder', $VideoEncoder)
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
