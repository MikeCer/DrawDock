param([Parameter(ValueFromRemainingArguments)][string[]]$AdbArguments)

$global:LASTEXITCODE = 0
Add-Content -LiteralPath $env:DRAWDOCK_TEST_ADB_LOG -Value ($AdbArguments -join ' ') -WhatIf:$false
switch ($AdbArguments[-1]) {
    'get-state' { 'device' }
    '--list' { $env:DRAWDOCK_TEST_ADB_MAPPING }
    default { throw 'Launcher performed a state-changing ADB call during WhatIf.' }
}
