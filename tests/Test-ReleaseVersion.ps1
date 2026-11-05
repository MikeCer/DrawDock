$ErrorActionPreference = 'Stop'
$getVersion = Join-Path $PSScriptRoot '..\packaging\Get-ReleaseVersion.ps1'
$version = & $getVersion -Tag 'v1.23.456'
if ($version.Version -ne '1.23.456' -or $version.VersionCode -ne 1023456) {
    throw 'Release version/code derivation failed'
}
$previous = & $getVersion -Tag 'v0.999.999'
$next = & $getVersion -Tag 'v1.0.0'
if ($next.VersionCode -le $previous.VersionCode) { throw 'Release upgrade ordering failed' }
foreach ($tag in 'v0.0.0', '1.2.3', 'v01.2.3', 'v1.2', 'v1.2.3-rc.1',
    'v1000.0.0', 'v1.2.3;exit', "v1.2.3`n", 'V1.2.3') {
    $rejected = $false
    try { & $getVersion -Tag $tag | Out-Null } catch { $rejected = $true }
    if (!$rejected) { throw "Invalid release tag was accepted: $tag" }
}
Write-Output 'Release version mapping, upgrade ordering and invalid-tag checks passed.'
