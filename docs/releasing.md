# Builds, installers and releases

The Windows publisher metadata is **Mike Cerreto** in both EXEs, the installer
and the per-user uninstall registration. These files are currently **unsigned**:
metadata does not establish an Authenticode identity or a verified Windows
publisher. Do not bypass SmartScreen or other operating-system security warnings.
A trusted code-signing certificate and a separate signing integration are needed
before claiming a verified publisher.

## GitHub Actions

`build.yml` tests/builds native Windows executables and the Android debug APK on
branch pushes and pull requests. It does not expose release signing secrets to
pull requests.

`release.yml` runs when a tag matching `v*` is pushed. A validation job accepts
only `vMAJOR.MINOR.PATCH`, with decimal components from 0 to 999, no leading
zeros or prerelease suffixes; `v0.0.0` is rejected. For example:

```powershell
git tag v0.2.0
git push origin v0.2.0
```

Commit and push the workflow/packaging files before tagging the desired commit.
Tagging an older commit without the workflow cannot invoke it. Never reuse or
move a published version tag. Push a higher version for application upgrades.
No release is created until both platform jobs and the Windows installer smoke
test pass. Only the final publish job has `contents: write`.

The release contains:

| Asset | Purpose |
|---|---|
| `DrawDock-<version>-windows-x64-setup.exe` | Per-user Windows installer |
| `DrawDock-<version>-windows-x64-portable.zip` | Same Windows files without registration/shortcuts |
| `DrawDock-<version>-android.apk` | APK signed with the persistent Android release key |
| `DrawDock-<version>-third-party-sources.zip` | Exact FFmpeg/x264/NVENC header source archives, checksums and build instructions |
| `SHA256SUMS.txt` | SHA-256 integrity checks for all four assets |

Versioned file metadata and Android's version name come from the tag, not edits
to development defaults. Android's version code is
`major * 1000000 + minor * 1000 + patch`, preserving upgrade order within the
accepted range. Control/video protocol versions remain independent.

## Windows installation and runtime

Setup requires 64-bit Windows 10 or later. It installs into
`%LOCALAPPDATA%\Programs\DrawDock`, creates a Start menu
shortcut and registers a per-user uninstaller. It requests no elevation and
does not install a service, autostart entry, USB driver or firewall rule.
Quit the launcher through Quit, not its hide-to-tray X, before upgrading.

Both distributions include ADB and a minimal native Windows FFmpeg executable,
including the software x264 and NVIDIA NVENC encoders. Hardware encoding still
requires compatible NVIDIA hardware/drivers. Software encoding does not.
Bundled tools are resolved relative to the installed EXE, without global PATH
changes. `DRAWDOCK_FFMPEG` remains an explicit override; development installations
retain their existing external-tool discovery when no bundled runtime is present.

Trust/pairing data under `%LOCALAPPDATA%\DrawDock` is deliberately separate from
application files and survives uninstall. The uninstaller never terminates a
shared ADB server; if the bundled ADB is still in use, stop it deliberately before
retrying uninstall. Tablet authorization and listener activation remain explicit.

FFmpeg/x264 are separate GPLv2-or-later processes; DrawDock remains MIT-licensed.
License notices are bundled. Do not publish Windows assets without the matching
third-party source asset. Update dependency URLs and SHA-256 hashes together in
`packaging\windows\dependencies.json`, and rerun actual native capture checks
when changing FFmpeg.

## Local Windows packaging

Use PowerShell 7, the repository Rust/MSVC toolchain, NSIS 3.11 and native MSYS2
MINGW64 build tools. MSYS2 compiles native Windows PE files; the desktop app does
not require MSYS2, WSL, Linux or Docker at runtime. CI provisions these tools on
its disposable Windows runner. For an existing MSYS2 installation, install
`mingw-w64-x86_64-gcc`, `mingw-w64-x86_64-pkgconf`, `make`, `nasm` and `diffutils`
through its package manager with the machine owner's approval.

```powershell
.\tests\Test-ReleaseVersion.ps1
.\packaging\windows\Build-Runtime.ps1 -MsysRoot C:\msys64
.\packaging\windows\Package-Windows.ps1 -Version 0.2.0 `
    -NsisExecutable 'C:\Program Files (x86)\NSIS\makensis.exe'
```

Outputs are in `dist`. The Rust EXEs use the static MSVC CRT in release packages
so a separate Visual C++ redistributable installer is not required.
`tests\Test-WindowsInstaller.ps1 -Installer <absolute-setup-path>` performs an
actual per-user install/tool/publisher/shortcut/uninstall smoke test. Run it only
with permission and without an existing per-user installation. It preserves
pairing data and refuses to replace an existing installation.

To rebuild only FFmpeg from the source ZIP, extract it, install the native build
tools above, and run its `Build-Runtime.ps1` with `-MsysRoot C:\msys64`,
`-WorkRoot <new-build-directory>` and `-SkipAdb`. It consumes the supplied source
archives and validates their hashes without downloading ADB. Compiler package
versions from the original build are included in `toolchain-packages.txt`.

## Android signing and backup

Release builds require these repository Actions secrets:

- `DRAWDOCK_ANDROID_KEYSTORE_BASE64`
- `DRAWDOCK_ANDROID_STORE_PASSWORD`
- `DRAWDOCK_ANDROID_KEY_ALIAS`
- `DRAWDOCK_ANDROID_KEY_PASSWORD`

`packaging\Initialize-AndroidSigning.ps1` creates one RSA-3072 PKCS12 key and
uploads those secrets to the explicitly selected repository. It must be run only
with owner approval. It refuses to overwrite an existing local key or signing
secrets; it is not part of CI. Secret values travel over stdin, not command-line
arguments, and are never printed.

The local backup is `%LOCALAPPDATA%\DrawDock\release-signing`: the key store plus
an account-protected DPAPI password backup. **DPAPI is tied to this Windows
account/machine.** Make an independent secure backup of both the key store and
its recovered password in an owner-controlled password manager before relying
on releases. Do not commit keys/passwords or paste them into logs/chat. GitHub
Actions secrets cannot be read back through the normal API.

The release job fails when any signing secret is missing, verifies the APK
signature and deletes its temporary keystore even on failure. Never regenerate
the release key for each build: users could not update existing installations.
A release APK cannot upgrade a debug-signed APK with the same package name.
Uninstalling the debug app first removes its data/pairing; this is an explicit
owner action, never an automatic pipeline/install step.
