# Run with Invoke-Pester .\tests\Start-DrawDockHost.Tests.ps1 after cargo build -p dd-host.
$repo = Split-Path -Parent $PSScriptRoot
$launcher = Join-Path $repo 'Start-DrawDockHost.ps1'
$adbFixture = Join-Path $PSScriptRoot 'fixtures\launcher-adb.ps1'

Describe 'DrawDock transport preflight' {
    BeforeEach {
        Mock Get-CimInstance { @() } -ParameterFilter { $Filter -eq "Name = 'dd-host.exe'" }
        Mock Get-NetIPAddress {
            foreach ($ip in @('192.168.1.16', '10.0.0.5')) {
                [pscustomobject]@{ IPAddress = $ip; AddressState = 'Preferred'; InterfaceIndex = 17; InterfaceAlias = 'Test LAN' }
            }
        }
        Mock Get-NetConnectionProfile {
            [pscustomobject]@{ InterfaceIndex = 17; NetworkCategory = 'Private' }
        }
        $env:DRAWDOCK_TEST_ADB_LOG = Join-Path $TestDrive 'adb.log'
        $env:DRAWDOCK_TEST_ADB_MAPPING = ''
        if (Test-Path -LiteralPath $env:DRAWDOCK_TEST_ADB_LOG) {
            Remove-Item -LiteralPath $env:DRAWDOCK_TEST_ADB_LOG
        }
    }

    AfterEach {
        Remove-Item Env:\DRAWDOCK_TEST_ADB_LOG
        Remove-Item Env:\DRAWDOCK_TEST_ADB_MAPPING
    }

    It 'accepts assigned private WiFi without an ADB executable' {
        { & $launcher -Transport WiFi -ListenAddress 192.168.1.16 -AdbPath 'Z:\missing-adb.exe' -WhatIf } |
            Should Not Throw
        Assert-MockCalled Get-NetIPAddress -Times 1 -Exactly -Scope It
        Assert-MockCalled Get-NetConnectionProfile -Times 1 -Exactly -Scope It
        (Test-Path -LiteralPath $env:DRAWDOCK_TEST_ADB_LOG) | Should Be $false
    }

    It 'accepts a domain-authenticated LAN profile' {
        Mock Get-NetConnectionProfile { [pscustomobject]@{ InterfaceIndex = 17; NetworkCategory = 'DomainAuthenticated' } }
        { & $launcher -Transport WiFi -ListenAddress 10.0.0.5 -WhatIf } | Should Not Throw
    }

    It 'requires an explicit canonical private IPv4 address' {
        foreach ($address in @('', '0.0.0.0', '127.0.0.1', '8.8.8.8', '169.254.1.2', '::1',
            '192.168.1.999', '192.168.001.16', 'example.com', '172.32.0.1')) {
            $rejected = $false
            try { & $launcher -Transport WiFi -ListenAddress $address -WhatIf }
            catch { $rejected = $true }
            $rejected | Should Be $true
        }
        Assert-MockCalled Get-NetIPAddress -Times 0 -Exactly -Scope It
    }

    It 'rejects a private address not assigned to this PC' {
        Mock Get-NetIPAddress { @() }
        { & $launcher -Transport WiFi -ListenAddress 192.168.1.16 -WhatIf } | Should Throw 'not a preferred'
    }

    It 'rejects public or missing network profiles without changing settings' {
        Mock Get-NetConnectionProfile { [pscustomobject]@{ InterfaceIndex = 17; NetworkCategory = 'Public' } }
        { & $launcher -Transport WiFi -ListenAddress 192.168.1.16 -WhatIf } | Should Throw 'network profile'
        Mock Get-NetConnectionProfile { @() }
        { & $launcher -Transport WiFi -ListenAddress 192.168.1.16 -WhatIf } | Should Throw 'network profile'
    }

    It 'keeps USB as the default and only reads the device and mapping under WhatIf' {
        { & $launcher -AdbPath $adbFixture -DeviceSerial fixture -WhatIf } | Should Not Throw
        $calls = @(Get-Content -LiteralPath $env:DRAWDOCK_TEST_ADB_LOG)
        $calls.Count | Should Be 2
        $calls[0] | Should Be '-s fixture get-state'
        $calls[1] | Should Be '-s fixture reverse --list'
        Assert-MockCalled Get-NetIPAddress -Times 0 -Exactly -Scope It
    }

    It 'leaves an existing matching USB reverse mapping untouched' {
        $env:DRAWDOCK_TEST_ADB_MAPPING = 'UsbFfs tcp:47831 tcp:47831'
        { & $launcher -AdbPath $adbFixture -DeviceSerial fixture -WhatIf } | Should Not Throw
        @(Get-Content -LiteralPath $env:DRAWDOCK_TEST_ADB_LOG).Count | Should Be 2
    }

    It 'refuses a conflicting USB reverse mapping' {
        $env:DRAWDOCK_TEST_ADB_MAPPING = 'UsbFfs tcp:47831 tcp:12345'
        { & $launcher -AdbPath $adbFixture -DeviceSerial fixture -WhatIf } | Should Throw 'different reverse mapping'
    }

    It 'does not permit USB to expose a LAN listener' {
        { & $launcher -Transport USB -ListenAddress 192.168.1.16 -WhatIf } | Should Throw 'only supported'
    }

    It 'requires explicit restart of an existing workspace host' {
        $exe = Join-Path $repo 'target\debug\dd-host.exe'
        Mock Get-CimInstance {
            [pscustomobject]@{ ExecutablePath = $exe; ProcessId = 1234 }
        } -ParameterFilter { $Filter -eq "Name = 'dd-host.exe'" }
        { & $launcher -Transport WiFi -ListenAddress 192.168.1.16 -WhatIf } | Should Throw 'Use -Restart'
        { & $launcher -Transport WiFi -ListenAddress 192.168.1.16 -Restart -Pair -WhatIf } | Should Not Throw
    }
}
