# DrawDock — Prototype Specification and Research Baseline

Version: 0.1 — review draft, not approved for implementation  
Research date: 2026-11-04  
Owner/reviewer: Mike Cerreto  
Status: proposed architecture; platform feasibility gates remain open

## 1. Purpose and authority

DrawDock mirrors one existing PC monitor onto an Android pen-enabled tablet. The tablet stylus controls the PC mouse or creates presentation annotations visible on the PC and tablet. A Rust PC application supports Windows 11 and Linux X11/Wayland; an Android application connects over local Wi-Fi or USB through ADB.

This document is a proposed spec-driven implementation baseline, not a claim that the design has been validated. MUST denotes an acceptance requirement, SHOULD a default that needs a documented reason to change, and MAY an optional capability. Proposed defaults become binding only after owner approval. Source IDs R01–R16 identify the research register in section 17. Engineering targets and architectural choices are proposals, not findings from the cited publications.

Coding agents MUST implement only approved scope, run the feasibility gates first, and report unsupported requirements rather than substitute mocks, tablet-only ink, XWayland-only control, plaintext networking, or privileged input injection.

## 2. Requirements traceability

| ID | Requirement | Verification |
|---|---|---|
| FR-01 | Two application components: Android tablet client and Rust PC backend/control application | Build/install both; end-to-end demonstration |
| FR-02 | Windows 11, Linux X11, and native Wayland operation | Separate real-session platform runs; section 14 |
| FR-03 | USB via ADB and local Wi-Fi | Cable-only test with Wi-Fi disabled; Wi-Fi test with cable removed |
| FR-04 | Low-latency communication; evaluate UDP | TCP/QUIC benchmark and recorded decision |
| FR-05 | Tablet selects an existing PC monitor | At least two monitors; permission-aware switching |
| FR-06 | Higher-resolution desktop fits tablet while preserving usable image quality | 4K source to negotiated lower resolution; geometry and quality tests |
| FR-07 | Stylus emulates mouse | Absolute pointing, click, drag, right click |
| FR-08 | Explicit draw mode: freehand, arrow, ellipse/circle, rectangle, colors | Host-visible and tablet-visible annotations |
| SEC-01 | Authenticated encrypted communication and explicit host authorization | Negative pairing/security tests |
| SAFE-01 | No stuck buttons or stale cross-monitor input | Loss, disconnect, topology, mode-change tests |

Interpretation requiring approval: “stretched” means aspect-preserving fit, not geometric distortion. The prototype mirrors existing displays; it does not create an extended/virtual monitor. “Draw mode” means a DrawDock overlay, not editing the underlying PowerPoint document. Pressure-sensitive native tablet/HID emulation is outside this prototype.

## 3. Scope and platform policy

### 3.1 Proposed initial targets

- Android 11/API 30 minimum, Kotlin UI and application logic, stylus-aware rendering, hardware H.264 decode through MediaCodec. Compile/target SDK versions MUST be selected and pinned at implementation kickoff; do not confuse target SDK with minimum SDK.
- Windows 11 x86-64, standard interactive user session, no administrator requirement.
- Linux x86-64: one GNOME Wayland environment, one KDE Plasma Wayland environment, and one X11 environment. Exact distribution, compositor, portal, PipeWire, GPU and driver versions MUST be frozen in the gate report. Supporting these does not imply every Wayland compositor is supported.
- One connected tablet and one active mirrored monitor per session. Multiple authorized sources may be retained for quick switching.
- SDR video; 60 fps target, 30 fps fallback. HDR, audio, clipboard, file transfer, WAN relay, browser clients, iOS, virtual displays and multi-client collaboration are excluded.

### 3.2 Compatibility must be capability-based

At startup probe capture, pointer injection, monitor mapping, video encoding and annotation presentation separately. Display a capability report. Capture-only operation MAY be offered as an explicitly degraded mode but MUST NOT count as full platform acceptance.

Windows SendInput is limited by UIPI to equal/lower-integrity targets. Elevated applications, UAC/secure desktop, protected content and lock-screen interaction are not supported and must not be worked around by automatic elevation [R02]. Android low-latency decoding depends on decoder/driver support; API-level availability alone is insufficient [R07].

## 4. Architecture

```text
ANDROID TABLET                              PC — Rust application
Kotlin UI / monitor selector                Local consent and session UI
MediaCodec -> video Surface                 Capture adapter -> scaler -> encoder
Stylus input -> coordinate mapper     <-->  Authenticated transport / scheduler
Local speculative ink                      Input router -> OS pointer adapter
Canonical annotation replica               Canonical annotation scene
                                            Host annotation presenter
```

The PC is the listening endpoint; the tablet initiates connections. No cloud service or account is required. Keep tablet rendering responsive independently of video reception. The annotation scene is a separate vector layer, not destructive edits to captured pixels.

PC workspace proposal:

```text
crates/protocol/          # validation, wire framing, golden vectors
crates/session/           # authorization, lifecycle, epochs
crates/transport/         # TLS/TCP and optional QUIC adapters
crates/capture/           # OS-specific capture implementations
crates/video/             # scale, encode, recovery, bounded queues
crates/input/             # Windows/X11/Wayland adapters
crates/annotations/       # scene, commands, replication
crates/presentation/      # host overlays/presentation fallback
apps/desktop/            # Rust control UI, CLI diagnostics
android/                 # Kotlin app, transport bridge if required
specs/ tests/ docs/
```

All PC application logic MUST be Rust. OS APIs, graphics drivers, codecs and audited native dependencies remain permitted: Rust does not eliminate their vulnerabilities. Unsafe code MUST be isolated behind small reviewed adapters with ownership/lifetime contracts. A Rust-native desktop UI is preferred; exact toolkit selection follows the presentation gate. Avoid claiming one generic capture/input crate guarantees platform parity.

For Linux codecs, evaluate a Rust-controlled GStreamer pipeline with installed hardware encoders and a software fallback. For Windows, evaluate Media Foundation hardware encoding. These are candidate implementations, not researched guarantees of availability. Record dependency licensing, binary distribution requirements and selected versions before implementation.

## 5. Platform adapters and permission flow

### 5.1 Windows

Use DXGI Desktop Duplication per selected output as the initial capture baseline [R01]. Evaluate Windows.Graphics.Capture if duplication compatibility or permissions require an alternative [R03]. Handle device loss, capture recreation, output changes, GPU copies and cursor composition explicitly.

Use SendInput for mouse events, absolute virtual-desktop coordinates, and standard buttons [R02]. Declare per-monitor DPI awareness. Map tablet coordinates through source pixels to the desktop's virtual bounds; negative monitor origins are valid. Do not assume the selected display begins at (0,0).

### 5.2 Linux X11

Use RandR for monitor topology, X11 capture with an evaluated shared-memory path, and XTEST for pointer emulation. R14 and R15 document candidate Rust interfaces; their existence is not evidence of complete capture implementation. Gate tests MUST confirm required server extensions and actual capture throughput. Fail clearly when extensions are absent.

### 5.3 Linux Wayland

Use a combined XDG RemoteDesktop/ScreenCast session. Create the RemoteDesktop session, request pointer access, select monitor sources through ScreenCast, start through RemoteDesktop, then open the PipeWire remote. Request multiple sources when available and explain the host selection dialog [R04, R05].

Prefer ConnectToEIS/libei for input; use granted device regions and mapping IDs. If EIS is unavailable, evaluate the portal NotifyPointer methods. Once EIS is established, do not mix those methods in that session. If fallback is needed after EIS failure, create a fresh session [R04, R06].

Do not use XTEST against XWayland as the native Wayland implementation. Do not use /dev/uinput, root, compositor security bypasses or unrestricted evdev access by default.

ScreenCast source positions/sizes are compositor coordinates and need not equal buffer pixels. Match streams to libei regions through mapping_id when available. Prefer PipeWire serial targeting where exposed; capability-negotiate older portal interfaces rather than require the latest documentation version [R05]. If a correct absolute mapping cannot be established, disable remote control with an actionable explanation rather than guess.

### 5.4 Multi-monitor selection and consent

The tablet MUST list selectable authorized monitors with an opaque session ID, useful label if available, pixel dimensions, logical dimensions when available, rotation, and current selection. Do not expose a promise of global monitor enumeration that the Wayland portal cannot fulfill.

On Wayland, the host user authorizes sources in system UI. The tablet chooses among those authorized sources; selecting an ungranted monitor triggers a host reauthorization flow. Because SelectSources is once per session, adding sources can require session recreation [R05]. Monitor IDs are session-scoped; no assumption of stable hardware identity is allowed.

## 6. Transport design and UDP evaluation

### 6.1 Baseline transport: TLS over TCP

Implement authenticated TLS/TCP first for both Wi-Fi and USB. Use separate control/input, annotation and video connections tied cryptographically to the same approved session. Set TCP_NODELAY on latency-sensitive sockets. Separate video prevents a large video write from occupying the input byte stream; it does not eliminate shared bottleneck congestion or TCP head-of-line blocking.

USB MUST use ADB reverse, not an invented UDP forward. ADB documents TCP and local socket redirection; it does not expose native UDP forwarding in these commands [R09]. Illustrative host commands:

```sh
adb devices -l
adb -s SERIAL reverse tcp:47831 tcp:47831
adb -s SERIAL reverse tcp:47832 tcp:47832
adb -s SERIAL reverse tcp:47833 tcp:47833
```

Port assignments are proposed defaults: 47831 control/input, 47832 video, 47833 annotation. USB-only listeners bind host loopback. The Android app connects to 127.0.0.1 on the mapped ports. Select a device explicitly if more than one is connected. Require authorized USB debugging, provide installation diagnostics, remove only DrawDock-owned mappings on clean shutdown, and never enable network ADB automatically. ADB authorization is not a replacement for DrawDock session authentication.

scrcpy is a useful reference for separating channels, raw encoded frames, ADB connection direction and minimal buffering. Its video direction is Android-to-PC, the opposite of DrawDock, and its internal protocol is not a stable DrawDock dependency [R10]. Do not adopt scrcpy's hidden Android APIs or shell-privileged server architecture.

### 6.2 Required Wi-Fi experiment: QUIC over UDP

QUIC is the preferred UDP experiment, not raw unauthenticated UDP. RFC 9221 provides unreliable, non-retransmitted datagrams; Quinn exposes streams and datagrams with maximum-size negotiation [R11, R12]. Reliable streams carry session control, button transitions, geometry, annotation commits and codec configuration. Candidate datagrams carry replaceable pointer motion, transient stroke previews and video fragments.

If video datagrams are implemented, include session/stream epoch, frame ID, fragment index/count, keyframe flag and timestamps. Fit each message to the negotiated datagram size minus headers. Bound reassembly bytes, frame count and lifetime. Expired incomplete frames are discarded; never depend on IP fragmentation. Loss of referenced H.264 frames requires keyframe recovery; dropping frames is not by itself a complete recovery strategy.

Use library congestion control and an application scheduler that prioritizes control/input over video. Disable input/control in 0-RTT to avoid replay-sensitive operations. The Android QUIC implementation is an open dependency decision: prototype a supported native library or shared Rust JNI transport and include its build/size/security costs in the comparison. Do not assume Kotlin has an interchangeable Quinn client.

Acceptance of the evaluation requires recorded TCP-versus-QUIC p50/p95/p99 latency, sustained fps, bitrate, CPU, battery/thermal observations, packet loss recovery and complexity. Use identical codec, resolution and workload on the same network. Test clean LAN, 1% random packet loss, added jitter and a constrained bandwidth link. QUIC MAY become the Wi-Fi default only if the report supports it. ADB stays TCP; do not tunnel QUIC over TCP and claim UDP latency benefits.

## 7. Video pipeline and quality

Proposed initial profile: H.264, 1920x1080 maximum bounding box subject to tablet decoder support, 60 fps target, 30 fps fallback, adaptive 4–20 Mbit/s starting range. The exact negotiated dimensions preserve source aspect ratio and obey encoder alignment. Profile/level, resolution, frame rate and codec configuration MUST be negotiated; no universal hardware decode support is assumed.

Encoder SHOULD disable B-frames and lookahead, minimize internal buffering, use a bounded recovery interval, and expose keyframe requests. Android MUST decode into a Surface, query codec capabilities and enable low-latency mode only where supported [R07]. Use measured fallback settings when it is not.

Capture/scaling SHOULD remain GPU-side where feasible. Evaluate bilinear versus higher-quality scaling using desktop text, diagrams and thin colored lines. The target is readable output, not impossible recovery of detail lost during downscaling. HDR-to-SDR treatment must be explicit; unsupported HDR sources must receive a warning or rejection.

Bound raw-frame queues to one pending frame and video queue age to a proposed 50 ms. Drop replaceable raw frames before encoding when overloaded. Never arbitrarily discard already encoded reference frames and continue feeding dependent frames as valid. On an intentional encoded discontinuity, reset the decode dependency chain and obtain a fresh keyframe/configuration as necessary. Avoid unbounded buffers and catch-up playback.

Deliver codec configuration before dependent media. A stream reset includes a new epoch, dimensions, configuration and keyframe. Rate-limit keyframe requests. Cursor handling MUST avoid duplicates: negotiate embedded versus separately rendered cursor behavior; a local predicted pointer must be visually distinct from authoritative host state.

## 8. Geometry and scaling contract

Default mode is fit-with-letterboxing, no crop and no aspect-ratio distortion. Optional fill/crop or zoom is deferred. Tablet toolbar and system insets are excluded from the content viewport.

Given source dimensions Ws,Hs and usable viewport Wv,Hv:

```text
s = min(Wv/Ws, Hv/Hs)
Wd = s*Ws; Hd = s*Hs
offX = (Wv-Wd)/2; offY = (Hv-Hd)/2
u = (tabletX-offX)/Wd
v = (tabletY-offY)/Hd
```

New contacts outside the displayed image MUST NOT click the PC. While dragging, clamp to the selected image edge and always deliver termination. Convert normalized coordinates through the negotiated source rotation and OS-specific logical/physical transform. Send normalized coordinates, source ID and geometry epoch rather than global desktop guesses from Android.

The PC maintains one authoritative transform used by capture, input and ink. Test all corners, center, negative desktop origins, portrait rotation, Android rotation and mixed DPI/fractional scaling. Reject stale epochs. During a monitor or geometry change, release held buttons, suspend input, publish new geometry, require client acknowledgment, then resume after a frame from the new stream epoch.

## 9. Stylus and interaction modes

### 9.1 Pointer mode

- Hover moves the pointer when the tablet reports hover; absence of hover is not an error.
- Tip contact sends left-button down at its authoritative coordinate; movement drags; lift sends button up.
- Barrel button maps to right-click where supported; provide an explicit UI alternative.
- Touch operates UI controls; drawing-surface finger navigation is disabled initially to reduce accidental input.
- Process historical/coalesced MotionEvent samples for ink, preserving original order/timestamps. Expose pressure, tilt and tool type if available, but do not claim these become native PC pen pressure.
- Handle cancel, palm rejection, app backgrounding and device disconnect by ending interactions safely [R08].

### 9.2 Draw mode

Switching modes is explicit, acknowledged and visible. Draw mode MUST NOT inject pointer clicks or drags into the underlying application. Initial tools: freehand, line/arrow, rectangle and ellipse; a constraint option makes squares/circles. Provide a color palette, at least three widths, eraser/delete, undo, redo and clear with confirmation.

Android renders ink immediately in a separate transparent layer. Motion prediction MAY improve local previews, but predicted points MUST be replaced by real samples and MUST NOT be committed or used as authoritative mouse clicks [R08]. The stylus latency literature supports separating immediate feedback from final rendering, but does not justify a zero-lag guarantee on this architecture [R13, R16].

### 9.3 Canonical annotation model

Store annotations in normalized source coordinates, scoped to a monitor identity for the current session. Each object has UUID, revision, tool, RGBA color, normalized width, ordered points/geometry and author/session ID. Commands include Begin, Append, Commit, Cancel, Delete, Undo, Redo and Clear. Commit includes the complete final geometry so missing preview points cannot corrupt final output. Bound object/point counts and serialized sizes.

The PC owns the canonical scene and acknowledges revisions. Android reconciles previews by object ID, avoiding duplicate ink. On reconnect, send a canonical scene snapshot; do not replay uncertain clicks or unacknowledged destructive commands. A mode/source change cancels incomplete shapes. Annotations remain until cleared within that session; durable storage/export is deferred.

## 10. Host-visible annotations: mandatory feasibility gate

A tablet-only overlay does not satisfy the presentation use case. The selected PC output MUST show annotations to people looking at that display/projector.

Windows and X11: evaluate a transparent, non-activating, click-through host overlay. Test fullscreen slideshow behavior, z-order, scaling, capture interaction and physical mouse pass-through. Wayland: capture/input portals do not supply a generic desktop annotation overlay API. Compositor-specific presentation support needs separate implementation and validation; do not infer that Wayland capture success proves overlay feasibility [R04, R05].

Gate G3 must choose one reviewed strategy per supported desktop:

1. Native compositor-compatible overlay with verified positioning, visibility and pass-through.
2. Explicit presentation-window mode: show a full-screen DrawDock view containing captured content plus canonical ink on the selected presentation output, with a separately defined non-recursive capture source.
3. Owner-approved compositor extension/integration with documented installation and security implications.

Option 2 is not automatically equivalent to annotating the original selected output: capturing the same output that displays the composed view can create feedback. It needs a different source/window/output topology and explicit owner approval. An ordinary fullscreen window MUST NOT silently replace the requested overlay behavior. If no strategy meets the approved requirement on a target compositor, mark that platform feature blocked and return to the owner.

The overlay must be composited exactly once in the tablet view. Choose and test one path: capture contains the host overlay, or capture excludes it and canonical ink is rendered locally. Prevent both duplicate alpha and temporarily disappearing ink when local previews reconcile. Screen-sharing a whole display versus sharing an individual PowerPoint window may expose different overlay results; document the tested conferencing/share mode. Do not promise third-party window-capture inclusion.

## 11. Protocol and state machine

Protocol name: drawdock/1. Major-version mismatch fails with an actionable message. Use fixed-width network-byte-order framing and a language-neutral protobuf payload schema; generate Rust/Kotlin bindings and golden test vectors. This is a proposed encoding choice, to be frozen before implementation.

Envelope fields: version, message type, session ID, source ID when applicable, stream/geometry epoch, monotonically increasing per-channel sequence, monotonic sender timestamp and payload length. Maximum ordinary control message: 64 KiB; maximum encoded video access unit: 8 MiB; initial reassembly budget: 16 MiB/session. Reject invalid lengths before allocation. Validate enum values, counts and finite coordinates. Timestamp origins differ between devices; never subtract raw cross-device clocks.

Core messages:

- Hello/Capabilities, PairRequest/PairResult, SessionBind.
- SourceList, SelectSource, SourceSelected, Geometry, GeometryAck.
- SetMode/ModeAck, PointerMotion, ButtonTransition, ButtonStateSnapshot.
- AnnotationCommand/Ack, SceneSnapshot.
- VideoConfig, VideoAccessUnit or VideoFragment, RequestKeyframe.
- Ping/Pong, Metrics, Error, Goodbye.

Every button transition includes coordinates and an input generation. Button state snapshots and a proposed 500 ms control heartbeat recover from missing datagrams. On connection/control lease expiry (proposed 1.5 seconds), release all held buttons immediately. Reliable transitions and unordered motion require an explicit interaction-generation barrier: never apply motion from an earlier drag after release or from an earlier source after switching.

States: Idle -> Connecting -> AwaitingHostApproval -> Negotiating -> AwaitingCaptureConsent -> Streaming -> Suspended/Reconnecting -> Closed. Pairing trust and OS capture consent are independent. Every failure path MUST release OS buttons, cancel unfinished ink, stop capture and clear outstanding allocations as appropriate. Suspend on host lock or Android backgrounding; require an explicit resume/renewed consent where the OS requires it.

## 12. Security and privacy baseline

- Wi-Fi listeners are disabled until the host enables sharing; bind to explicitly selected local interfaces. No automatic router port forwarding, public discovery or cloud relay.
- TLS identity MUST be verified. Proposed pairing: host-generated high-entropy QR invitation containing endpoint, host public-key fingerprint, expiring single-use token; tablet pins identity and host explicitly approves the new device. No certificate-validation bypass, six-digit-only bearer secret or silent trust-on-first-use.
- Trusted devices use stored cryptographic credentials; store host keys in the OS credential facility and tablet credentials in Android Keystore-compatible storage. Document the selected library and exact credential flow before coding authentication.
- Additional channel connections must authenticate the same device and bind to an approved session. A session ID by itself is not an authorization token.
- USB uses the same encrypted/authenticated application protocol. Protect host loopback listeners from unauthorized local clients with session credentials.
- Bound authentication attempts, token lifetime, message sizes, concurrent connections, fragment queues and annotation memory. Validate all untrusted inputs before OS calls.
- Default telemetry is local and opt-in. Logs must exclude screen pixels, tokens, pen content and private keys. No persistence of video or annotations by default.
- Provide a visible sharing indicator, authorized-device list, revoke button and local emergency stop. Stop/revoke must take precedence over stream work.
- Pin dependencies and lockfiles, generate an SBOM, run Rust dependency advisories/license checks, fuzz protocol boundaries and review native/unsafe code. Security is defense in depth, not an automatic consequence of using Rust.

## 13. Performance budgets and measurements

“No lag” is a goal, not a physically achievable guarantee. Proposed prototype acceptance budgets on declared reference hardware:

| Metric | Target |
|---|---|
| Tablet local stylus-to-ink visible latency | p95 <= 25 ms |
| Tablet input to visible PC pointer response | p95 <= 50 ms USB; <= 70 ms Wi-Fi |
| PC visual change to tablet display | p95 <= 80 ms USB; <= 100 ms Wi-Fi |
| Tablet input -> PC visual change -> tablet feedback | p95 <= 120 ms USB; <= 150 ms Wi-Fi |
| 1080p negotiated profile | 60 fps target; >= 55 presented fps in a 60-second moving workload |
| Loss recovery after a short impairment | usable clean video within 1 second after network recovery |
| Repeated monitor switching | 20 switches without stale input or growing queues |

These are proposed engineering targets, not measured results or literature thresholds. If unavailable hardware prevents them, return measurements and a scope/performance decision; do not relabel a failed target as passed.

Use high-frame-rate camera measurements for genuine input-to-photon/display latency. Separately instrument capture, encode, enqueue, send, receive, decode and present times. Estimate clock offset/uncertainty with repeated exchanges if calculating cross-device stages; report uncertainty. Report p50/p95/p99, actual display refresh rates, codec, bitrate, frame drops, queue ages, CPU/GPU utilization, thermal state and test duration. Local predicted ink MUST NOT be used as proof of low remote interaction latency.

## 14. Verification matrix and acceptance tests

Freeze a reference tablet, desktop CPU/GPU, encoder, USB cable/link, Wi-Fi topology and OS/driver versions. Physical hardware end-to-end tests are mandatory; CI mocks are supplementary.

1. Windows 11, X11, GNOME Wayland and KDE Wayland: capture, mouse injection and host-visible ink tested independently.
2. Each platform: both Wi-Fi and USB; USB tested with tablet Wi-Fi disabled. Android app needs no root.
3. Two monitors: tablet changes selection, including a monitor with negative origin. Source content, pointer and ink must agree.
4. Windows mixed DPI and Wayland fractional scaling: nine-point calibration grid, rotation and source-resolution changes. Proposed mapping error <= 2 source pixels excluding quantization; record logical-coordinate rounding separately.
5. Higher-resolution source: 3840x2160 desktop fitted into lower-resolution viewport, no aspect distortion, no clipped toolbar, thin-line/text visual review against saved source captures. Record quality results rather than invent an unsupported objective score.
6. Pointer: hover where available, click, right-click, long drag and release. Disconnect mid-drag; no button may remain down beyond the control lease.
7. Draw: all tools/colors/widths, undo/redo/clear; host/projector sees marks, underlying application receives no drawing clicks.
8. Annotation replication: delayed/dropped previews, commit repair, reconnect snapshot and no duplicate overlay.
9. Permission rejection/revocation, lock, portal closure, monitor unplug and GPU capture loss: safe stop and clear diagnostics.
10. Malformed frames, oversized lengths, stale epochs, reordered datagrams, unauthorized channel joins, wrong certificate and replayed invitation: reject without crash or input execution.
11. Network impairment and TCP/QUIC comparison from section 6; verify memory and queue bounds.
12. Thirty-minute mixed pointer/drawing run: no crash, stuck input, sustained thermal collapse or unbounded memory growth. Record stable RSS range and battery observations.

Automated tests MUST cover geometry transforms, protocol golden vectors shared across languages, annotation revision semantics, source switching, replay/stale input rejection and video reassembly bounds. Use property tests/fuzzing for parsers. Native Wayland tests must run in a native Wayland session, not merely XWayland.

## 15. Staged coding-agent work plan

### Phase 0 — approval and feasibility, no broad implementation

- G1: capture one monitor and inject absolute pointer input under each target OS/session. Record permissions and actual runtime capabilities.
- G2: decode one low-latency PC-encoded H.264 stream on the selected Android tablet over TLS/TCP and ADB; measure queue age and video latency.
- G3: demonstrate host-visible, non-recursive annotations on all three desktop families, explicitly separating GNOME and KDE Wayland results.
- G4: select/pin codec dependencies, desktop UI, pairing library and Android QUIC option; review licenses/native boundaries.

Deliver a feasibility report with evidence, capability matrix, version inventory and recommended spec amendments. Stop for owner review if a MUST is blocked. Spike code may be disposable, but must be labeled as such and never used as a fake production adapter.

### Phase 1 — secure vertical slice

Implement pairing, session lifecycle, Windows capture/pointer, Android video decode, Wi-Fi TLS/TCP and USB ADB. Include metrics and emergency stop from the outset. Demonstrate one monitor end-to-end.

### Phase 2 — geometry and Linux parity

Implement shared transforms, monitor selection, topology epochs, X11 and native Wayland adapters. Exercise consent revocation and mixed scaling. No platform acceptance based solely on compilation.

### Phase 3 — annotation mode

Implement canonical scene, immediate tablet preview, final commit repair and the G3-approved host presenter. Validate fullscreen presentation and capture-loop behavior.

### Phase 4 — latency optimization

Implement the required QUIC experiment, benchmark against TCP, tune codec and bounded queues. Retain TCP USB support regardless of the Wi-Fi outcome.

### Phase 5 — hardening and reproducibility

Run the full acceptance matrix, parser fuzzing and a soak test. Deliver build/install/run documentation, dependency inventory and remaining limitations. Owner approves the prototype results separately from approving this spec.

Each phase MUST include runnable build commands, tests, artifacts and a short report listing passed/failed requirements. Do not proceed past a failed gate without an approved spec change.

## 16. Approval decisions and deliverables

Owner approval is requested for:

1. Android API 30 minimum; Kotlin tablet client and Rust PC application with permitted audited native dependencies.
2. Existing-monitor mirroring, aspect-preserving fit, one active monitor/tablet, SDR H.264 initial profile.
3. Mouse emulation rather than pressure-aware native digitizer emulation.
4. Persistent-until-cleared DrawDock presentation annotations rather than modifying PowerPoint content.
5. TLS/TCP baseline for Wi-Fi/ADB, measured QUIC evaluation rather than mandatory unvalidated UDP shipping path.
6. Host OS consent for Wayland source authorization; tablet selection only among authorized sources.
7. Mandatory host-visible annotation gate and an explicit follow-up decision if Wayland needs compositor-specific integration or changed presentation topology.
8. Proposed performance budgets and exact reference hardware/test environments to be selected before implementation.

Prototype deliverables: Rust workspace, Android project/APK, protocol schema and golden vectors, dependency lockfiles/SBOM, setup instructions for Windows/X11/GNOME/KDE, ADB diagnostics, security/pairing design, capability report, TCP-versus-QUIC benchmark, acceptance results, known limitations and annotated research/decision log. No telemetry service or remote infrastructure is required.

## 17. Research register and rationale

This is the retained baseline literature/documentation, not every search hit. URLs point to primary technical documentation where possible; sources consulted on the research date can evolve. Before coding, pin library versions and preserve relevant documentation revisions. The papers motivate feedback/measurement strategy; they do not benchmark DrawDock.

| ID | Source | Why it matters / limitation |
|---|---|---|
| R01 | Microsoft, [Desktop Duplication API](https://learn.microsoft.com/en-us/windows/win32/direct3ddxgi/desktop-dup-api) and [desktop duplication overview](https://learn.microsoft.com/en-us/windows-hardware/drivers/display/desktop-duplication-api) | Per-output desktop frames, GPU buffers, dirty/move regions and cursor metadata; Windows capture baseline. |
| R02 | Microsoft, [SendInput](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-sendinput) | Mouse injection and UIPI integrity-level restriction; no promise of elevated/secure-desktop control. |
| R03 | Microsoft, [Screen capture](https://learn.microsoft.com/en-us/windows/apps/develop/media-authoring-processing/screen-capture) | Windows.Graphics.Capture alternative, system source selection and visible capture indication. |
| R04 | XDG Desktop Portal, [RemoteDesktop](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.RemoteDesktop.html) | Combined capture/input lifecycle, permission model, EIS and mutually exclusive Notify input paths. |
| R05 | XDG Desktop Portal, [ScreenCast](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.ScreenCast.html) | Multi-source authorization, PipeWire access, logical versus pixel geometry, mapping_id, serial targeting and restore-token semantics. |
| R06 | Freedesktop, [libei API](https://libinput.pages.freedesktop.org/libei/api/index.html) and [EI protocol](https://libinput.pages.freedesktop.org/libei/) | Compositor-mediated emulated input and client/server model; not hardware tablet virtualization. |
| R07 | AOSP, [Low-latency decoding in MediaCodec](https://source.android.com/docs/core/media/low-latency-media) | Android 11 low-latency decoding support and vendor-driver dependency. |
| R08 | Android Developers, [Advanced stylus features](https://developer.android.com/develop/ui/views/touch-and-input/stylus-input/advanced-stylus-features) | MotionEvent information, low-latency rendering, palm rejection and replaceable predicted ink. |
| R09 | AOSP, [ADB manual](https://android.googlesource.com/platform/packages/modules/adb/+/refs/heads/master/docs/user/adb.1.md) and [reverse implementation explanation](https://android.googlesource.com/platform/system/core/+/252586941934d23073a8d167ec240b221062505f) | TCP/local-socket reverse forwarding, Android-to-host connection direction and USB debugging setup. |
| R10 | Genymobile, [scrcpy developer documentation](https://github.com/Genymobile/scrcpy/blob/master/doc/develop.md) | Reference implementation for ADB tunnels, channel separation, H.264 framing and immediate display. Reverse video direction; internal protocol not reusable as a stable standard. |
| R11 | IETF, [RFC 9221: An Unreliable Datagram Extension to QUIC](https://www.rfc-editor.org/rfc/rfc9221.html), March 2022 | Negotiated datagrams without retransmission; baseline for UDP evaluation and application loss handling. |
| R12 | Quinn, [crate documentation](https://docs.rs/quinn/latest/quinn/) and [Connection datagram API](https://docs.rs/quinn/latest/quinn/struct.Connection.html) | Candidate Rust QUIC implementation, stream/datagram semantics and changing maximum datagram size. |
| R13 | Albert Ng, Michelle Annett, Paul Dietz, Anoop Gupta and Walter F. Bischof, “In the Blink of an Eye: Investigating Latency Perception During Stylus Interaction,” CHI 2014, pp. 1103–1112. [Paper](https://iihm.imag.fr/coutrix/ens/M2-MoSIG-HiL/1stAssignment/JustNoticeableDifference/2556288.2557037_JND.pdf) | Controlled stylus latency experiments; supports careful latency measurement and immediate feedback, not a universal perceptual threshold. |
| R14 | [Rust xrandr API](https://docs.rs/xrandr) | Candidate X11 topology binding; inspect maintenance and features before adoption. |
| R15 | [Rust x11::xtest API](https://docs.rs/x11/latest/x11/xtest/) | X11 input binding reference; not a native Wayland solution. |
| R16 | Albert Ng, Julian Lepinski, Daniel Wigdor, Steven Sanders and Paul Dietz, “Designing for Low-Latency Direct-Touch Input,” UIST 2012, pp. 453–464, DOI 10.1145/2380116.2380174. [Research record](https://www.semanticscholar.org/paper/Designing-for-low-latency-direct-touch-input-Ng-Lepinski/020f76e207dc023fea856987add3b7fee7f9c44a) | Immediate low-fidelity feedback followed by final rendering; supporting background, distinct from remote-desktop performance. |

### Evidence boundaries

The portal APIs document permissioned capture/input, not universal compositor implementation or host-overlay portability. Native overlay strategy, encoder choice, TLS credential implementation, GUI toolkit, Android QUIC dependency and exact supported platform versions remain feasibility/design work. They MUST NOT be presented as already verified by these sources. Performance numbers above are proposed acceptance targets, not external benchmark claims.
