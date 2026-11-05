[CmdletBinding()]
param([Parameter(Mandatory)][string]$Installer)
$ErrorActionPreference = 'Stop'
$directory = Join-Path $env:LOCALAPPDATA 'Programs\DrawDock'
$registry = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\DrawDock'
if ((Test-Path $directory) -or (Test-Path $registry)) {
    throw 'Installer smoke test requires no existing per-user DrawDock installation.'
}
$installed = $false
try {
    $process = Start-Process -FilePath $Installer -ArgumentList '/S' -Wait -PassThru
    if ($process.ExitCode -ne 0) { throw "Installer failed: $($process.ExitCode)" }
    $installed = $true
    foreach ($name in 'dd-host.exe', 'drawdock-launcher.exe', 'Uninstall.exe') {
        if ((Get-Item "$directory\$name").VersionInfo.CompanyName -ne 'Mike Cerreto') {
            throw "Incorrect publisher: $name"
        }
    }
    if ((Get-ItemProperty $registry).Publisher -ne 'Mike Cerreto') {
        throw 'Incorrect installed publisher'
    }
    & "$directory\dd-host.exe" outputs
    if ($LASTEXITCODE -ne 0) { throw 'Installed backend could not enumerate Windows monitors' }
    & "$directory\runtime\ffmpeg\ffmpeg.exe" -version
    if ($LASTEXITCODE -ne 0) { throw 'Bundled FFmpeg could not run' }
    & "$directory\runtime\adb\adb.exe" version
    if ($LASTEXITCODE -ne 0) { throw 'Bundled ADB could not run' }
    $shortcut = Join-Path ([Environment]::GetFolderPath('Programs')) 'DrawDock\DrawDock.lnk'
    if (!(Test-Path $shortcut)) { throw 'Start menu shortcut is missing' }
} finally {
    if ($installed) {
        $process = Start-Process -FilePath "$directory\Uninstall.exe" -ArgumentList '/S' -Wait -PassThru
        if ($process.ExitCode -ne 0) { throw "Uninstaller failed: $($process.ExitCode)" }
        $deadline = [DateTime]::UtcNow.AddSeconds(15)
        while ((Test-Path $registry) -and [DateTime]::UtcNow -lt $deadline) {
            Start-Sleep -Milliseconds 100
        }
        if ((Test-Path $registry) -or (Test-Path "$directory\dd-host.exe")) {
            throw 'Uninstall did not remove the owned application files/registration'
        }
    }
}
Write-Output 'Per-user install, publisher metadata, runtime tools, shortcut and uninstall checks passed.'
