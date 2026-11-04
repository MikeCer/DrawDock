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
- Latest validation: 44 ordinary Rust tests and 25 Android tests pass; both
  applications build, Android lint passes, and Rust formatting/strict Clippy pass.
- The two explicit native integration tests pass, including the expanded
  authenticated editor/scene checks described below. Interactive physical pen
  and fullscreen acceptance are not implied by those results.

## Pen radial menu and expanded drawing toolkit

The Android drawing toolbar is replaced by a hierarchical native circular menu
opened with a stationary stylus long-press (about 550 ms). The menu is available
in Draw/Pointer mode and even while input is waiting for readiness; these
unready contacts cannot become drawing/clicks when the connection becomes ready.
Pointer-down is deferred until movement exceeds slop or a short contact ends,
so menu invocation does not click the underlying desktop. Connection/settings
and monitor selection remain accessible at the top.

All requested shape categories, single-object move/rotate/resize/mirror, text,
laser, highlighter, blackboard/grid and explicit **tablet clipboard** image
paste are implemented across Android and the authoritative Windows scene.
The host uses native Rust resvg rendering, escaped plain text and Windows
fonts, bounded/re-encoded PNG assets and physical-coordinate affine transforms.
Canvas/history state is per monitor within the session; Clear resets backgrounds
as requested. Blackboard forces Draw, including when restored by Undo, to avoid
injecting mouse clicks into a concealed desktop. Laser is expiring state, not
stored ink. No new system installation, public listener or trust bypass was added.

Validation evidence:

- 44 ordinary Rust tests pass; native renderer tests cover text, images,
  transformed shapes and backgrounds. Formatting, strict Clippy and host build
  pass.
- The isolated pinned-TLS reconnect test now exercises new-tool/text/PNG commits,
  exact selection/transform responses, correlated errors, canvas synchronization,
  board Pointer rejection and Undo/Redo/Clear. Desktop pixel sampling verifies
  PNG/blackboard presentation, with DPI-aware geometry and heartbeat-backed waits.
  The existing two-monitor x264/NVENC video test also passes. Test hosts and
  fixture trust stores were cleaned up; actual saved trust was untouched.
- 25 Android unit tests, APK build and lint pass. The updated APK is installed
  and its process launched on the authorized tablet.
- After the owner unlocked the tablet, synthetic ADB stylus checks passed
  stationary opening, moving-contact cancellation, finger-hold exclusion,
  action reachability, correct button sizing and corner clamping. The first
  real UI inspection caught oversized radial button layout bounds; explicit
  child measurement/layout fixed that issue and the device smoke test passed.

**Windows remains PARTIAL.** Synthetic stylus injection and desktop framebuffer
sampling are not proof of actual tablet pen input on a physical monitor/projector.
Real-pen long-press/Pointer click safety, all tool visuals over live/fullscreen
applications, actual clipboard-provider interoperability and transform ergonomics
still need owner acceptance. Transform preview is a bounding frame; the PC
object commits on pen-up. The existing background/disconnect lifecycle clears
session annotations, so copy a clipboard image before connecting. Capture
watermarks, latency/soak and Wi-Fi gates remain unchanged. Linux/GNOME and QUIC
remain **DEFERRED**.

## Earlier implementation and runtime evidence

`dd-scene` validates bounded normalized objects and history. `dd-host` compiles a
Windows layered presenter, TLS/TCP pairing service and `SendInput` pointer
adapter. Fixture replay ran on the primary `\\.\DISPLAY2` and printed “Overlay
visible”; the process was stopped afterward. This is not pixel-level proof that
ink appeared over the physical display or a live application.

The Android client compiles and its unit tests pass. The built APK was installed
to the selected tablet after explicit user approval. The earlier W2 manual USB
setup used `\\.\DISPLAY2`, a loopback-only listener at `127.0.0.1:47831` and an
ADB reverse mapping. That stage provided no-video preview and annotation/control.
Later session evidence and the initial W3 video implementation are recorded
below; physical stylus/fullscreen and Wi-Fi acceptance remain outstanding.
No firewall rule was changed.

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

The initial W3 implementation now has a top Android monitor selector, collapsible
overlay tools and an aspect-fit video/ink workspace. A Rust-controlled installed
native FFmpeg gdigrab/libx264 process captures the selected monitor and supplies
baseline H.264 over a separate pinned-TLS connection, authenticated with a
post-approval video credential. The same port/ADB mapping is reused. Source
changes release buttons, switch the native overlay, preserve per-monitor scene
history, restart the encoder and reject old epochs until new video is rendered.
DPI awareness is now established before enumeration; the two non-primary outputs
enumerate at 3840x2160 rather than the earlier DPI-virtualized 2560x1440.

Validation: 20 ordinary Rust tests and 14 Android tests pass; Android build/lint
and Rust clippy/build complete (only pre-existing clippy warnings). The real
60-frame H.264 capture probe passed, and the explicit native TLS test rejected a
wrong video token, streamed actual frames and switched between two real monitors.
No screen recording was saved. Android playback, physical multi-monitor pen
alignment and fullscreen behavior remain unverified. The INCLUDED ink handoff
currently removes provisional ink on lift rather than waiting for a conservative
capture revision watermark: reconciliation remains **BLOCKED**. This may produce
a short ink gap; it is not acceptance of the final visual contract. The external
GPL-enabled FFmpeg dependency is not bundled, and hardware encoding, DXGI capture,
video recovery/latency/soak metrics and Wi-Fi acceptance remain future work.

The video-enabled APK was installed successfully to the authorized tablet and
its process started without a new app crash. The tablet screen was off, so no
visible video/UX acceptance is claimed. Historical tablet crash logs also exposed
TLS `close_notify` being written on the UI thread during Disconnect; socket
shutdown now runs on a worker, including Surface/lifecycle-triggered disconnects.

See `capability-matrix.md` for per-gate status. No PASS is claimed for physical
overlay, pen-to-host ink, fullscreen, pointer injection or Wi-Fi.

## Next required acceptance work

Remembered-device persistence is now implemented at the owner's request,
superseding the original in-memory-only scope. The native host retains its
DPAPI-protected certificate/private key and registry, issues independent
256-bit per-device credentials after approval, stores only their digests, and
continues accepting remembered tablets after a session ends. Android retains
the last PC's pinned endpoint/credential using AES-GCM with an Android Keystore
key. New enrollment still needs QR + local approval; revocation and explicit
re-pairing/forgetting are documented in the run guides.

23 Rust unit tests, 2 explicit native integration tests and 16 Android tests pass.
The actual native host regression used an isolated DPAPI registry and verified
repeated pinned-TLS reconnects, process restart with the same certificate,
fresh video credentials and wrong/revoked credential rejection. It also exposed
a Windows listener-clone shutdown stall; the video listener now explicitly
enables nonblocking mode so disconnect cleanup can finish and accept reconnects.
Session cleanup now discards the old scene's undo/redo history as well as visible
ink, so another session cannot resurrect a previous tablet's annotations.
Android encryption-envelope tamper/format tests, build and lint pass. First
tablet enrollment/Keystore reconnect still needs one owner-approved pairing
with the upgraded app/host. The broader physical mirroring/reconciliation and
fullscreen gates remain unchanged, not passed by these authentication tests.

1. Verify the fixture visually over an updating application, click-through and
   focus, then over an actual fullscreen presentation; record physical output
   and DPI behavior.
2. With tablet Wi-Fi disabled, approve the pending loopback pairing locally,
   then demonstrate real pen-created host-visible marks plus cancellation/history.
3. Validate pointer, selected-monitor video and source alignment on the tablet;
   complete reconciliation watermarks and recovery, then test private-interface
   Wi-Fi with no automatic firewall changes.

## Video quality and rusty_ffmpeg evaluation

The owner's video-quality request was evaluated against the installed static
FFmpeg 7.0 executable, Intel i9-11900K, RTX 3080 Ti, and tablet vendor AVC limits
(up to 4096x2176 advertised). `rusty_ffmpeg` is raw FFI/linking to the same FFmpeg
libraries, not a higher-quality encoder. Equivalent codecs/settings do not gain
image quality merely by replacing the subprocess. The current pipe carries
already-compressed H.264, not full raw desktop frames. In-process APIs could
support finer timestamps, buffer control or future GPU-frame integration, but
would require implementing those paths, not just substituting the crate.

Upstream Windows instructions require vcpkg or compatible FFmpeg libraries,
import libraries/headers and Clang or pregenerated bindings. This installation
contains executables only, not a development SDK; no new SDK, library, driver or
system-wide tool was installed. Context7 could not run because Node was absent
from PATH, so the crate's canonical published README was consulted directly.
Retaining the executable also preserves encoder-process fault isolation and
avoids adding unsafe media FFI to the presenter/session process.

Instead, the actual bottlenecks were improved: 1280x720/fast-bilinear/ultrafast
became up to 2560x1440/Lanczos with BT.709 signaling and H.264 High level 5.0.
The launcher explicitly selects tested NVENC P5/ultra-low-latency, CQ 18,
12 Mbit/s VBR target, 24 Mbit/s maximum and 2-Mbit VBV; software veryfast/CRF 18
is explicitly selectable and remains the raw CLI default. Both have no
B-frames/lookahead. There is no silent encoder fallback. GDI capture and scaling
still use CPU memory; this is not a zero-copy GPU pipeline or HDR validation.

A reproducible synthetic 2560x1440 desktop chart (Consolas text at 12-36 pixels
and one-pixel grid lines) was encoded for 90 frames/profile and decoded/upscaled
back to the same reference for SSIM. Results compare entire profiles, not codecs
at equal bitrate or the Rust/CLI integration boundary:

| Profile | SSIM (higher is better) | Static chart average Mbit/s | Synthetic motion encode fps including startup |
|---|---:|---:|---:|
| Previous 720p x264 ultrafast | 0.798078 | 2.897 | 385.1 |
| New 1440p x264 veryfast | 0.994318 | 7.568 | 81.0 |
| New 1440p NVENC P5 | 0.998846 | 5.825 | 50.6 |

Motion throughput used 90 `testsrc2` frames without real-time pacing; it is not
capture latency, glass-to-glass latency, a sustained FPS guarantee or proof that
NVENC is always faster than software. Native live primary-monitor probes each
captured 60 AUs at 2560x1440 in 2.12 seconds including startup, with both encoders.
No actual desktop recording was saved. The synthetic chart/encoded files were
removed; the session contains `Measure-DrawDockVideoQuality.ps1` and its JSON
results for repeatability.

Initial hardware capture exposed a brittle one-slot `try_send`: multiple
complete AUs from a single pipe read could fail before an idle consumer got CPU.
The queue remains one slot, but handoff now waits at most 100 ms and is
cancellable. It never silently drops reference frames; sustained stalls still
require reconnecting. Regression covers burst order, stall deadline and stop.
Actual pinned-TLS tests passed both encoders on two monitors, including
SPS/PPS/IDR startup and invalid video-token rejection. 26 Rust unit tests and
2 explicit native integration tests pass; Android has 18 passing tests with
build/lint passing. Physical tablet visual quality, color accuracy, video/ink
reconciliation, fullscreen, Wi-Fi and sustained thermal/latency tests remain
**PARTIAL/BLOCKED**, not promoted to PASS by these results.

References:
- https://docs.rs/crate/rusty_ffmpeg/latest
- https://docs.nvidia.com/video-technologies/video-codec-sdk/13.0/ffmpeg-with-nvidia-gpu/index.html

Linux/GNOME and QUIC are **DEFERRED**.
