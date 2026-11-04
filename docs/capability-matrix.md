# Capability matrix

Status reflects implementation and evidence available on 2026-11-04.

| Windows requirement | Status | Evidence / gap |
|---|---|---|
| W0 Windows/Rust/MSVC environment | PASS | Windows 11 Pro build 26300 and MSVC 14.51.36231 recorded in `environment.md`; native Rust tests pass. |
| W0 Android target and pen source | PASS | ADB serial `cfa3e5dce217`, Xiaomi `25099RP13G`, Android 16/API 36; `NVTCapacitivePen` with pressure/tilt axes enumerated. |
| W1 native layered overlay implementation | PARTIAL | `dd-host` compiled and fixture replay ran on `\\.\DISPLAY2`; process reported “Overlay visible.” Raster test passes, but pixel-level physical display confirmation is unavailable. |
| W1 live application, click-through and focus | BLOCKED | No observed screen evidence of the overlay over an independently updating app; click-through/focus interaction is unverified. |
| W1 fullscreen presentation | BLOCKED | No actual fullscreen presentation test was performed. |
| W1 monitor/DPI mapping | PARTIAL | Outputs enumerate as 2560x1440 at `(0,0)`, `(-2534,1440)` and `(1306,1440)`; physical identity, DPI scaling and projector mapping are unverified. |
| W1 scene tools/history | PARTIAL | Freehand/arrow/rectangle/ellipse rasterization and scene history tests pass; interactive host controls and visuals are not verified. |
| W2 Android unit tests and APK build | PASS | `testDebugUnitTest assembleDebug` completed successfully; debug APK exists. |
| W2 app installed on tablet | PASS | Built debug APK installed to ADB serial `cfa3e5dce217` after explicit user approval. |
| W2 TLS, pinning and pairing implementation | PARTIAL | Native QR window was inspected and loopback TCP acceptance was verified; Yes/No approval and bundled Android scanner are implemented. 16 Rust tests and 7 Android tests pass; Android build/lint pass and updated APK is installed. Physical QR scan, approval and authenticated session still require manual acceptance. |
| W2 unified tablet/PC clear and shape commits | PARTIAL | Fixed retained host framebuffer pixels and shape commits containing too many points. Rust redraw/clear/undo regression and 5 Android tests pass; Android build and lint pass. Physical display clearing and all shape tools require manual retest. |
| W2 real USB/ADB pen-to-host annotation | BLOCKED | Manual test is ready, but no approved pairing or actual pen stroke has been reported; host-visible ink is not yet verified. |
| W2 palm/finger exclusion and cancellation | BLOCKED | Reducer tests/build pass; physical pen/finger/cancellation gestures were not exercised. |
| W3 pointer mode and release state | PARTIAL | User exercised pointer mode; host log confirmed disconnect at its 120-message/s guard. Client now paces all session frames to 80/s and coalesces pending motion without dropping transitions. Regression simulates 2,000 hover samples/s; queue/release/order tests and Android build/lint pass. Physical stability retest remains pending. |
| W3 selected-monitor video and reconciliation | BLOCKED | Capture, H.264, video client and annotation reconciliation are not implemented; W3 waits for actual W2 acceptance. |
| W4 private-interface Wi-Fi | BLOCKED | Private-address filtering exists, but no Wi-Fi listener/session was started or validated; no firewall rule was changed. |
| Latency, soak, memory and dropped-frame metrics | BLOCKED | No end-to-end pen/video path; no performance measurements exist. |
| Linux adapters / GNOME | DEFERRED | Explicitly out of this Windows-first implementation. |
| QUIC / UDP | DEFERRED | Explicitly deferred until core Windows gates are demonstrated. |

The overall Windows-only result is **PARTIAL**. Mandatory host-visible pen ink,
live-app/fullscreen behavior and Wi-Fi remain unverified or blocked. Linux and
QUIC are **DEFERRED**, not passed.
