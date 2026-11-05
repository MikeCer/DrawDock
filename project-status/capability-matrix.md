# Capability matrix

Historical validation record. Contributor documentation starts at
[docs](../docs/README.md).

Status reflects implementation and evidence available on 2026-11-05.

| Windows requirement | Status | Evidence / gap |
|---|---|---|
| W0 Windows/Rust/MSVC environment | PASS | Windows 11 Pro build 26300 and MSVC 14.51.36231 recorded in `environment.md`; native Rust tests pass. |
| W0 Android target and pen source | PASS | ADB serial `cfa3e5dce217`, Xiaomi `25099RP13G`, Android 16/API 36; `NVTCapacitivePen` with pressure/tilt axes enumerated. |
| W1 native layered overlay implementation | PARTIAL | `dd-host` compiled and fixture replay ran on `\\.\DISPLAY2`; process reported “Overlay visible.” Raster test passes, but pixel-level physical display confirmation is unavailable. |
| W1 live application, click-through and focus | BLOCKED | No observed screen evidence of the overlay over an independently updating app; click-through/focus interaction is unverified. |
| W1 fullscreen presentation | BLOCKED | No actual fullscreen presentation test was performed. |
| W1 monitor/DPI mapping | PARTIAL | Outputs enumerate as 2560x1440 at `(0,0)`, `(-2534,1440)` and `(1306,1440)`; physical identity, DPI scaling and projector mapping are unverified. |
| W1 scene tools/history | PARTIAL | Native resvg rendering adds line/circle/polygon/polyline/text/image/highlighter, bounded affine move/rotate/resize/mirror, blackboard/grid, undoable Clear and expiring laser. Scene/protocol/raster tests pass; physical tool visuals still require real-pen acceptance. |
| W2 Android unit tests and APK build | PASS | Latest 25 Android tests, `assembleDebug` and `lintDebug` pass; updated debug APK installed and launched. |
| W2 app installed on tablet | PASS | Built debug APK installed to ADB serial `cfa3e5dce217` after explicit user approval. |
| W2 TLS, pinning and pairing implementation | PARTIAL | Native QR window inspected; host logs confirm a locally approved authenticated tablet session. Camera scan UX still requires physical acceptance. Current host has 20 passing Rust unit tests; Android has 14 passing tests with build/lint passing and updated APK installed. |
| W2 unified tablet/PC clear and shape commits | PARTIAL | Fixed retained host framebuffer pixels and shape commits containing too many points. Rust redraw/clear/undo regression and 5 Android tests pass; Android build and lint pass. Physical display clearing and all shape tools require manual retest. |
| Remembered-device reconnects | PARTIAL | DPAPI-protected stable host identity and device registry, credential revocation and Android Keystore AES-GCM storage implemented. Actual native host test passed repeated pinned-TLS reconnects, process restart, wrong-token and revoked-token rejection. 23 Rust unit tests, 2 explicit native integration tests and 16 Android tests pass; Android build/lint pass. Tablet Keystore enrollment/reconnect still needs first owner-approved pairing after upgrade. |
| W2 real USB/ADB pen-to-host annotation | BLOCKED | Approved tablet session is logged and user-reported tool/clear issues were fixed. Formal physical verification of real pen strokes and every required host-visible tool remains outstanding. |
| W2 palm/finger exclusion and cancellation | BLOCKED | Reducer tests/build pass; physical pen/finger/cancellation gestures were not exercised. |
| W3 pointer mode and release state | PARTIAL | User exercised pointer mode; host log confirmed disconnect at its 120-message/s guard. Client now paces all session frames to 80/s and coalesces pending motion without dropping transitions. Regression simulates 2,000 hover samples/s; queue/release/order tests and Android build/lint pass. Physical stability retest remains pending. |
| W3 selected-monitor capture and H.264 | PARTIAL | Upgraded from 720p to 1440p30, Lanczos/BT.709 and quality-controlled H.264. Actual 60-frame captures passed with x264 and NVENC; native TLS test verified both encoders' SPS/PPS/IDR startup, wrong-token rejection and two-monitor switching. Synthetic desktop SSIM improved 0.798078 to 0.998846 with the NVENC profile. 26 Rust unit tests, 2 native tests and 18 Android tests/build/lint pass. Physical tablet playback/alignment and latency remain unverified. |
| W3 tablet screen selector / overlay menu | PARTIAL | Top monitor dropdown, Settings overlay and maximal aspect-fit workspace retained. Pen-only long-press opens hierarchical circular tools. Synthetic stylus tablet checks passed finger exclusion, motion cancellation, action reachability, button sizing and corner clamping; real-pen UX/screen-switch alignment remain manual. |
| Expanded tools, editing and tablet clipboard paste | PARTIAL | End-to-end protocol/host rendering and Android menu/input implemented for all requested categories. Clear resets backgrounds too. Paste uses explicit tablet clipboard image URI import with bounded lossless PNG; actual provider compatibility and physical image/text/transform/laser visuals remain unverified. |
| W3 annotation/video reconciliation | BLOCKED | INCLUDED capture avoids independently drawing committed objects on Android; provisional ink hands off on lift. Conservative capture revision watermark is missing, so a short ink gap and physical capture-contract validation remain open. |
| W4 private-interface Wi-Fi | PASS | No-ADB WiFi launcher bound 192.168.1.16 on a Private LAN. Logs confirm tablet 192.168.1.32 QR enrollment/resume. Owner confirmed video, real pen/shape ink on PC, Pointer, Clear and reconnect with USB unplugged. Final APK passed nine automated LAN display switches and owner unplugged-USB retest. No firewall changes. Initial handoff timeout and remaining soak/latency gaps are recorded in feasibility-report.md. |
| Latency, soak, memory and dropped-frame metrics | BLOCKED | No end-to-end pen/video path; no performance measurements exist. |
| Linux adapters / GNOME | DEFERRED | Explicitly out of this Windows-first implementation. |
| QUIC / UDP | DEFERRED | Explicitly deferred until core Windows gates are demonstrated. |

The overall Windows-only result is **PARTIAL**. Mandatory host-visible pen ink,
formal live-app/fullscreen behavior and long-run metrics remain unverified or blocked. Linux and
QUIC are **DEFERRED**, not passed.
