[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Destination
)
$ErrorActionPreference = 'Stop'
$expectedHash = 'c7d27f780ddb6cffb4730138cd1591e841f4b7edb155856901cdf5f214394fa1'
$mirrors = @(
    'https://deac-fra.dl.sourceforge.net/project/nsis/NSIS%203/3.11/nsis-3.11.zip'
    'https://netix.dl.sourceforge.net/project/nsis/NSIS%203/3.11/nsis-3.11.zip'
)
New-Item -ItemType Directory -Force $Destination | Out-Null
$archive = Join-Path $Destination 'nsis-3.11.zip'
foreach ($url in $mirrors) {
    Write-Host "Downloading NSIS 3.11 from $url"
    & curl.exe --fail --location --silent --show-error --retry 2 --retry-delay 2 `
        --connect-timeout 20 --max-time 120 --proto '=https' --proto-redir '=https' `
        --output $archive --write-out "NSIS response: HTTP %{http_code}; type %{content_type}; bytes %{size_download}`n" `
        $url
    if ($LASTEXITCODE -ne 0) {
        Write-Warning "NSIS mirror download failed with curl exit code $LASTEXITCODE. Trying the next mirror."
        continue
    }
    $actualHash = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash
    Write-Host "NSIS SHA256: $actualHash; expected: $expectedHash"
    if ($actualHash -ne $expectedHash) {
        Write-Warning 'NSIS checksum mismatch. The response will not be extracted; trying the next mirror.'
        continue
    }
    Expand-Archive -LiteralPath $archive -DestinationPath $Destination
    return
}
throw 'All NSIS mirrors failed download or pinned SHA256 verification. No archive was extracted.'
