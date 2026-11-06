Describe 'Pinned NSIS download' {
    BeforeEach {
        $script:downloader = Join-Path $PSScriptRoot '..\packaging\windows\Get-Nsis.ps1'
        $script:destination = Join-Path $TestDrive ([guid]::NewGuid().ToString('N'))
        $env:DRAWDOCK_TEST_NSIS_ATTEMPT = '0'
        $env:DRAWDOCK_TEST_NSIS_MODE = 'valid'
        Mock curl.exe {
            $env:DRAWDOCK_TEST_NSIS_ATTEMPT = [string]([int]$env:DRAWDOCK_TEST_NSIS_ATTEMPT + 1)
            $global:LASTEXITCODE = 0
            if ($env:DRAWDOCK_TEST_NSIS_MODE -eq 'transport-failure' -and
                $env:DRAWDOCK_TEST_NSIS_ATTEMPT -eq '1') {
                $global:LASTEXITCODE = 22
            }
        }
        Mock Get-FileHash {
            if ($env:DRAWDOCK_TEST_NSIS_MODE -eq 'reject-all' -or
                ($env:DRAWDOCK_TEST_NSIS_MODE -eq 'reject-first' -and
                 $env:DRAWDOCK_TEST_NSIS_ATTEMPT -eq '1')) {
                [pscustomobject]@{ Hash = '0' * 64 }
            } else {
                [pscustomobject]@{ Hash = 'c7d27f780ddb6cffb4730138cd1591e841f4b7edb155856901cdf5f214394fa1' }
            }
        }
        Mock Expand-Archive {}
    }

    AfterEach {
        Remove-Item Env:\DRAWDOCK_TEST_NSIS_ATTEMPT
        Remove-Item Env:\DRAWDOCK_TEST_NSIS_MODE
    }

    It 'extracts a verified download without contacting another mirror' {
        & $script:downloader -Destination $script:destination
        Assert-MockCalled curl.exe -Times 1 -Exactly -Scope It
        Assert-MockCalled Expand-Archive -Times 1 -Exactly -Scope It
    }

    It 'falls back after an unexpected response and extracts only the verified download' {
        $env:DRAWDOCK_TEST_NSIS_MODE = 'reject-first'
        & $script:downloader -Destination $script:destination
        Assert-MockCalled curl.exe -Times 2 -Exactly -Scope It
        Assert-MockCalled Expand-Archive -Times 1 -Exactly -Scope It
    }

    It 'fails closed when every mirror returns an invalid checksum' {
        $env:DRAWDOCK_TEST_NSIS_MODE = 'reject-all'
        $message = ''
        try { & $script:downloader -Destination $script:destination }
        catch { $message = $_.Exception.Message }
        if ($message -notmatch '^All NSIS mirrors failed') {
            throw "Expected a pinned-download failure; got: $message"
        }
        Assert-MockCalled curl.exe -Times 2 -Exactly -Scope It
        Assert-MockCalled Expand-Archive -Times 0 -Exactly -Scope It
    }

    It 'falls back after a transport failure without hashing the failed download' {
        $env:DRAWDOCK_TEST_NSIS_MODE = 'transport-failure'
        & $script:downloader -Destination $script:destination
        Assert-MockCalled curl.exe -Times 2 -Exactly -Scope It
        Assert-MockCalled Get-FileHash -Times 1 -Exactly -Scope It
        Assert-MockCalled Expand-Archive -Times 1 -Exactly -Scope It
    }
}
