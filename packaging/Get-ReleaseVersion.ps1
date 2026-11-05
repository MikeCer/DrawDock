[CmdletBinding()]
param([Parameter(Mandatory)][string]$Tag)
$ErrorActionPreference = 'Stop'
if ($Tag -cnotmatch '\Av(0|[1-9][0-9]{0,2})\.(0|[1-9][0-9]{0,2})\.(0|[1-9][0-9]{0,2})\z') {
    throw 'Release tags must be vMAJOR.MINOR.PATCH with each component between 0 and 999.'
}
$version = $Tag.Substring(1)
$parts = $version.Split('.') | ForEach-Object { [int]$_ }
$code = $parts[0] * 1000000 + $parts[1] * 1000 + $parts[2]
if ($code -eq 0) { throw 'v0.0.0 is not a release version.' }
[pscustomobject]@{ Version = $version; VersionCode = $code }
