[CmdletBinding()]
param(
    [string]$Repository = 'MikeCer/DrawDock',
    [string]$KeyTool = 'keytool'
)
$ErrorActionPreference = 'Stop'
$directory = Join-Path $env:LOCALAPPDATA 'DrawDock\release-signing'
$store = Join-Path $directory 'android-release.p12'
$passwordFile = Join-Path $directory 'android-password.dpapi'
if ((Test-Path $store) -or (Test-Path $passwordFile)) {
    throw "A signing backup already exists in $directory. Do not rotate or overwrite it."
}
$existing = & gh secret list --repo $Repository --json name --jq '.[].name'
if ($LASTEXITCODE -ne 0) { throw 'Read repository Actions secret names failed' }
if ($existing -match '^DRAWDOCK_ANDROID_') { throw 'Android signing secrets already exist; do not overwrite them.' }
New-Item -ItemType Directory -Force $directory | Out-Null
$acl = Get-Acl $directory
$acl.SetAccessRuleProtection($true, $false)
$identity = [Security.Principal.WindowsIdentity]::GetCurrent().User
$rule = [Security.AccessControl.FileSystemAccessRule]::new(
    $identity, 'FullControl', 'ContainerInherit,ObjectInherit', 'None', 'Allow')
$acl.AddAccessRule($rule)
Set-Acl $directory $acl
$bytes = [byte[]]::new(32)
[Security.Cryptography.RandomNumberGenerator]::Fill($bytes)
$password = [Convert]::ToBase64String($bytes)
$password | ConvertTo-SecureString -AsPlainText -Force | ConvertFrom-SecureString |
    Set-Content $passwordFile
$env:DRAWDOCK_KEYTOOL_PASSWORD = $password
try {
    & $KeyTool -genkeypair -noprompt -storetype PKCS12 -keystore $store `
        -storepass:env DRAWDOCK_KEYTOOL_PASSWORD -keypass:env DRAWDOCK_KEYTOOL_PASSWORD `
        -alias drawdock-release -keyalg RSA -keysize 3072 -validity 10000 `
        -dname 'CN=DrawDock Android Release'
    if ($LASTEXITCODE -ne 0) { throw 'Android release key generation failed' }
    $secrets = @{
        DRAWDOCK_ANDROID_KEYSTORE_BASE64 = [Convert]::ToBase64String([IO.File]::ReadAllBytes($store))
        DRAWDOCK_ANDROID_STORE_PASSWORD = $password
        DRAWDOCK_ANDROID_KEY_ALIAS = 'drawdock-release'
        DRAWDOCK_ANDROID_KEY_PASSWORD = $password
    }
    foreach ($secret in $secrets.GetEnumerator()) {
        $start = [Diagnostics.ProcessStartInfo]::new((Get-Command gh -ErrorAction Stop).Source)
        $start.UseShellExecute = $false
        $start.RedirectStandardInput = $true
        foreach ($argument in 'secret', 'set', $secret.Key, '--repo', $Repository) {
            $start.ArgumentList.Add($argument)
        }
        $process = [Diagnostics.Process]::Start($start)
        $process.StandardInput.Write($secret.Value)
        $process.StandardInput.Close()
        $process.WaitForExit()
        if ($process.ExitCode -ne 0) { throw "Upload $($secret.Key) failed; preserve the local backup and finish the upload manually." }
        $process.Dispose()
    }
    Write-Output "Signing key created and uploaded. Back up $directory; the password backup uses this Windows account's DPAPI."
} finally {
    Remove-Item Env:DRAWDOCK_KEYTOOL_PASSWORD
    $password = $null
    $secrets = $null
    [Array]::Clear($bytes, 0, $bytes.Length)
}
