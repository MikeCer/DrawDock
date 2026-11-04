# DrawDock environment inventory

Machine-specific discovery record. General setup instructions are in the
[development guide](../docs/development.md).

Discovery date: 2026-11-04. This inventory records the system as observed during
the initial W0 inspection; device and display state may change.

## Windows host

| Item | Observed |
|---|---|
| OS | Windows 11 Pro, 64-bit, build 26300 |
| Session | Interactive console session; `explorer.exe` is running |
| System | ASUS, x64, 64 GiB RAM |
| GPUs | Intel UHD Graphics 750 driver 31.0.101.5334; NVIDIA GeForce RTX 3080 Ti driver 32.0.16.1742 |
| Windows SDK | 10.0.22621.0 and 10.0.26100.0 |
| Visual Studio | Visual Studio Enterprise 2026, 18.11.3, at `D:\Program Files\Microsoft Visual Studio\18\Enterprise` |
| MSVC | x64 compiler/linker 14.51.36231 is installed; load `VC\Auxiliary\Build\vcvars64.bat` in the build process |
| Rust | rustc/cargo 1.98.1, `x86_64-pc-windows-msvc`; rustfmt and clippy installed |

Three active display outputs were enumerated in the interactive session, including
non-zero and negative virtual-desktop positions. WMI reported physical 3840x2160
and 2560x1440 monitors and virtual display adapters; the WinForms enumeration
reported 2560x1440 bounds for each output. Treat the output API's current physical
pixel dimensions as authoritative and re-enumerate at run time. Exact physical
projector identity and mixed-DPI behavior are not yet verified.

## Android tablet and build tools

| Item | Observed |
|---|---|
| Device | Xiaomi model `25099RP13G`, ADB serial `cfa3e5dce217`, state `device` |
| Android | Android 16, API 36 |
| Display | 1600x2560, density 320 dpi; logical display reported as 2560x1600 |
| Pen | Android input device `NVTCapacitivePen`, source includes `STYLUS`; pressure range 0..4095 and tilt X/Y -60..60 are exposed |
| ADB | `D:\Tools\android-dev\platform-tools\adb.exe`, version 36.0.2; system ADB 36.0.0 also sees the same device |
| Android SDK | `D:\Tools\android-dev`, Android platform 36 is configured in Android Studio and `android.jar` exists |
| JDK | `JAVA_HOME` points to Temurin 17; Android Studio includes JBR 25.0.3. The `java` found on `PATH` is a Java 8 shim, so builds must set `JAVA_HOME` explicitly |
| Gradle | No global `gradle` command; Gradle 9.4.1 distribution is cached under the user's `.gradle\wrapper\dists` directory |

ADB reported the tablet as connected over USB. No app installation or port
forwarding had been performed at discovery. Android reports a pen-capable input
device, but real pen dispatch, pressure, palm rejection, and latency require an
instrumented DrawDock run with the physical pen.

## Build constraints

The desktop can be built natively on Windows; WSL, Linux, and Docker are not
required. VS developer environment variables are process-local. Do not persist
environment changes, install drivers, change firewall rules, elevate the host,
or expose a listener as part of these instructions.
