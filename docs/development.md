# Development guide

## Prerequisites

Build the desktop host in an interactive Windows environment with:

- Rust's MSVC target and the toolchain selected by `rust-toolchain.toml`
  (including rustfmt and Clippy).
- Visual Studio or Build Tools with the C++ toolchain and a Windows SDK.
- A native FFmpeg executable with `gdigrab` and `libx264`; `h264_nvenc` additionally
  requires compatible NVIDIA hardware, driver and FFmpeg encoder support.
- Installed Windows fonts for text rendering.

The Android project uses JDK 17, Gradle 9.4.1, Android Gradle Plugin 9.2.1 and
Android SDK platform 36. It targets API 36 with minimum API 30. There is no
checked-in Gradle wrapper; use a local Gradle installation or cached distribution.
ADB and a USB-debugging-authorized tablet are needed for device work.

Build and USB operation do not require WSL, Linux, Docker, elevation or firewall changes.
Do not commit `android\local.properties`, credentials or machine-specific paths.

## Repository layout

| Path | Contents |
|---|---|
| `apps\dd-host` | Native Windows application and platform adapters |
| `crates\dd-scene` | Shared scene/geometry model and validation |
| `android\app` | Kotlin client and JVM unit tests |
| `tests\fixtures` | JSON scenes for deterministic replay |
| `docs` | Contributor-facing technical documentation |
| `docs\assets\brand` | Self-contained documentation images |
| `specs` | Original design and requirement inputs |
| `project-status` | Milestones, environment snapshots and validation reports |

The [architecture guide](architecture.md) maps individual source files.
The technical guides describe implementation behavior; design inputs can contain
broader proposals that are not part of the executable contract.
The optional root `brand` design workspace is ignored; contributor builds and
documentation must not depend on it. See [asset ownership](branding.md).

## Rust build and checks

From the repository root in a Visual Studio Developer PowerShell:

```powershell
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo build -p dd-host
```

This builds both `target\debug\dd-host.exe` and the native Windows-subsystem
`target\debug\drawdock-launcher.exe`. The default Cargo run target remains the
CLI host. See [launcher operation](run-windows.md#native-windows-launcher) for
listener, process and forwarding ownership.

If using a regular PowerShell, initialize the installed Visual Studio environment
and run Cargo inside the **same** child process. Substitute your installation:

```powershell
$repo = (Get-Location).Path
$vcvars = 'C:\Path\To\Visual Studio\VC\Auxiliary\Build\vcvars64.bat'
$cmd = "call `"$vcvars`" >nul && cd /d `"$repo`" && cargo build -p dd-host"
& $env:ComSpec /c $cmd
if ($LASTEXITCODE -ne 0) { throw 'Rust build failed' }
```

Running the batch setup in one process and Cargo in another loses the compiler,
linker and SDK environment.

Launcher preflight regressions use the installed Pester module and built host
diagnostics. Network/profile queries and ADB are mocked; no listener or firewall
change is performed:

```powershell
Invoke-Pester .\tests\Start-DrawDockHost.Tests.ps1
```

## Android build and checks

Set process-local paths to your installations:

```powershell
$env:JAVA_HOME = 'C:\Path\To\jdk-17'
$env:ANDROID_HOME = 'C:\Path\To\Android\Sdk'
gradle -p .\android testDebugUnitTest assembleDebug lintDebug
if ($LASTEXITCODE -ne 0) { throw 'Android checks failed' }
```

Alternatively, resolve an already-cached Gradle distribution:

```powershell
$gradle = Get-ChildItem `
    (Join-Path $env:USERPROFILE '.gradle\wrapper\dists\gradle-9.4.1-bin') `
    -Filter gradle.bat -Recurse -File |
    Select-Object -First 1 -ExpandProperty FullName
if (-not $gradle) { throw 'Configure a Gradle 9.4.1 installation first' }
& $gradle -p .\android testDebugUnitTest assembleDebug lintDebug
if ($LASTEXITCODE -ne 0) { throw 'Android checks failed' }
```

The APK is `android\app\build\outputs\apk\debug\app-debug.apk`.
JUnit results and lint reports are under `android\app\build`.
See [Android and ADB](run-android-and-adb.md) for installation and pairing.

## Testing changes

Keep pure scene/geometry/protocol behavior in ordinary Rust or JVM unit tests.
Add focused regressions for changed behavior, including invalid input, bounds,
history, cancellation, stale epochs and ordering where relevant.
Android JVM tests use framework default return values; they do not execute the
real view lifecycle, device input stack, Keystore or MediaCodec.

### Native integration checks

These ignored tests run actual Windows services/capture and use isolated trust
stores. Build the host binary before running them:

```powershell
cargo build -p dd-host
cargo test -p dd-host -- --ignored --nocapture
```

The video test uses two real outputs and both software/NVENC encoders.
The reconnect/editor test uses native overlay presentation and desktop pixel
sampling, with DPI-aware geometry and heartbeat-backed waits. Run in an
interactive desktop session and avoid unrelated input during these checks.
Do not substitute production credentials or automatically approve pairing.

### Device and visual checks

Choose coordinates inside the app's actual canvas, accounting for orientation,
toolbars, system bars and letterboxing. For synthetic gesture routing:

```powershell
& $adb -s $serial shell input stylus swipe 800 600 800 600 750
& $adb -s $serial shell input touchscreen swipe 800 600 800 600 750
```

The stationary stylus should open the radial menu; the finger should not.
Also check movement beyond slop, corner placement and submenu actions.
Synthetic input does not establish physical pen behavior.

For drawing/input changes, exercise a real pen over live desktop applications
and a fullscreen presentation. Check the physical PC/projector, tool geometry,
pointer release, palm/finger exclusion, mode/source changes and video alignment.
For clipboard changes, test actual image providers and denied access. For video
changes, distinguish encoder throughput from end-to-end input/display latency.

Record configurations, commands and observations in
[project records](../project-status/README.md), not in technical guide prose.
Do not save secrets or sensitive desktop recordings as evidence.

## Contribution checklist

- Follow the existing component boundaries; keep Windows APIs out of `dd-scene`.
- Preserve authentication, source-readiness barriers, traffic limits and explicit
  error reporting.
- Update both serializers/handlers for protocol or scene-model changes.
- Keep scene/history/payload budgets and transient-state cleanup intact.
- Run the smallest relevant tests, then the platform build/lint checks above.
- Update the relevant technical guides and architecture decision when changing
  a contract; keep progress/results out of `docs`.
