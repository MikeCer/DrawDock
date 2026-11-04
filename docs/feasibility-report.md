# Initial Windows feasibility report

**Current verdict: PARTIAL.**

## Verified environment and builds

- Windows 11 Pro build 26300, interactive console session; Rust 1.98.1 and
  Visual Studio 2026/MSVC x64 are installed.
- Android SDK platform 36 is at `D:\Tools\android-dev`. Xiaomi `25099RP13G`,
  Android 16/API 36, ADB serial `cfa3e5dce217`, is connected and reports
  `NVTCapacitivePen` with pressure/tilt axes.
- Three outputs enumerate at 2560x1440: primary `\\.\DISPLAY2` at `(0,0)`,
  `\\.\DISPLAY3` at `(-2534,1440)` and `\\.\DISPLAY1` at `(1306,1440)`.
- Rust formatting and workspace tests pass: 8 host tests and 4 scene tests.
  Android `testDebugUnitTest assembleDebug` passes and produces the debug APK.
- Rust clippy completes with existing warnings in `dd-scene` (`len_zero`) and
  the rasterizer (`too_many_arguments`); no warning-as-error lint pass is claimed.

## Implementation and runtime evidence

`dd-scene` validates bounded normalized objects and history. `dd-host` compiles a
Windows layered presenter, TLS/TCP pairing service and `SendInput` pointer
adapter. Fixture replay ran on the primary `\\.\DISPLAY2` and printed “Overlay
visible”; the process was stopped afterward. This is not pixel-level proof that
ink appeared over the physical display or a live application.

The Android client compiles and its unit tests pass. The built APK was installed
to the selected tablet after explicit user approval. For the current manual USB
test, the host is running on `\\.\DISPLAY2` with a loopback-only listener at
`127.0.0.1:47831`, and one ADB reverse mapping is active. Pairing, host-visible
pen input and physical stylus gestures are not yet confirmed. The client
supports a no-video tablet preview and annotation/control protocol. Windows
pointer injection is implemented but not exercised. Selected-monitor
capture/H.264, video transport, reconciliation and Wi-Fi acceptance are not
implemented/tested. No firewall rule was changed.

Pairing UX now uses a native Windows QR invitation and a native Yes/No device
approval prompt. The tablet has a bundled camera scanner; manual entry remains
an advanced fallback. TLS certificate pinning, 128-bit invitation possession,
five-minute expiry, attempt limits and local authorization are retained.
QR bitmap/quiet-zone and invalid/expired invitation tests pass as part of 16
Rust tests; Android parsing/shape/input tests total 7 passing tests. Android
APK build and lint pass, and the QR-enabled APK is installed on the tablet.
Camera permission has not been granted automatically; scanning and approval
still need physical manual validation. The native QR window was observed with
its code, endpoint and expiry/cancellation instructions visible. A loopback TCP
probe connected successfully without providing authentication; it consumes one
failed pairing attempt and is not evidence of an authenticated tablet session.

In the subsequent user test, the host logged a locally approved tablet session,
then `peer exceeded the control message rate limit` during pointer-mode use.
The Android writer previously flushed motion as quickly as the socket allowed;
queue-tail coalescing alone did not constrain a fast-draining USB connection.
It now paces every session frame to 80/s below the unchanged 120/s host guard,
and coalesces motion while waiting before dequeue. Four focused regression tests
cover 2,000 samples/s, ordered transitions/mode barriers, the reserved release
slot, and no catch-up burst after a stall. All 11 Android tests, debug build and
lint pass. Sustained physical pen/pointer stability still requires manual retest.

See `capability-matrix.md` for per-gate status. No PASS is claimed for physical
overlay, pen-to-host ink, fullscreen, pointer injection or Wi-Fi.

## Next required acceptance work

1. Verify the fixture visually over an updating application, click-through and
   focus, then over an actual fullscreen presentation; record physical output
   and DPI behavior.
2. With tablet Wi-Fi disabled, approve the pending loopback pairing locally,
   then demonstrate real pen-created host-visible marks plus cancellation/history.
3. After W2 passes, complete pointer and selected-monitor capture/video,
   reconciliation and source-switch recovery, then test private-interface
   Wi-Fi with no automatic firewall changes.

Linux/GNOME and QUIC are **DEFERRED**.
