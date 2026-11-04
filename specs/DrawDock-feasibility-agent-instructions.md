# DrawDock — Initial Coding-Agent Instructions and Feasibility Plan

Document ID: DD-AGENT-001 | Version: 0.2 | Date: 2026-11-04  
Purpose: build a runnable base infrastructure and test the project's core feasibility before implementing the complete product.

## 1. How the owner should use this document

Place this document and its companions together in the repository's `specs/` directory. Preserve their exact filenames or update all references together:

- `DrawDock-rust-overlay-architecture.md` — DD-ARCH-001.
- `DrawDock-android-stylus-event-spec.md` — DD-ANDROID-001.
- `DrawDock-feasibility-agent-instructions.md` — this document.
- Optional existing baseline: `DrawDock-prototype-spec-draft.md`.

The two new technical documents supersede older optional/degraded annotation wording. Host-visible live annotations are mandatory. Initial agent execution authorizes repository implementation, not automatic system modifications, firewall changes, extension activation, public listeners or package installation. Ask before side-effecting host setup outside the repository.

## 2. Copy/paste kickoff prompt

```text
You are the implementation agent for DrawDock. Read these documents completely:
1. specs/DrawDock-rust-overlay-architecture.md
2. specs/DrawDock-android-stylus-event-spec.md
3. specs/DrawDock-feasibility-agent-instructions.md
Also read specs/DrawDock-prototype-spec-draft.md if present.

Implement a feasibility prototype, not the full commercial application.
The primary invariant is real tablet pen input producing graphics over live PC
applications, visible on the physical selected PC monitor/projector. Windows 11,
Linux X11, and explicitly tested native Wayland desktops are in scope. Mouse-only,
capture-only, screenshot editing and tablet-only drawing do not meet the goal.

Begin with milestone M0. Inspect the repository, available tools, OS/session,
build environment and connected devices without making privileged/system changes.
Produce project-status/implementation-plan.md, project-status/environment.md and a
requirement-to-test matrix. State blockers, missing hardware and design decisions.
Verify library APIs against pinned versions; do not invent bindings or claim
unsupported capabilities.

Use a Rust backend and Kotlin Android client. A minimal GNOME Shell GJS presenter
is the proposed platform exception; ask the owner to confirm this exception before
implementing that adapter. Other adapters and infrastructure may proceed while
that approval is pending. Keep application logic and authorization in Rust.

Implement runnable vertical slices in milestone order, with real OS adapters and
bounded protocol/queue handling. Keep synthetic test tools separate from actual
platform implementations. Run tests available in this environment and mark all
others NOT RUN. Do not mark a platform supported because cross-compilation passes.

Use authenticated TLS/TCP for the initial Wi-Fi and USB/ADB path. Do not disable
certificate validation, silently expose network listeners or introduce raw UDP.
Defer QUIC until the annotation/capture/input architecture is demonstrated.

Do not remove mandatory requirements to avoid a blocker. If host-visible ink,
fullscreen stacking, monitor mapping or capture reconciliation cannot be made
correct, record the evidence, affected adapter and proposed next experiment.
A failed mandatory gate is BLOCKED, not an excuse to return a lower-feature MVP.

At every milestone update runnable instructions, test evidence, the capability
matrix and known issues. Provide exact commands for me to run on missing hardware.
Do not claim latency, security, hardware compatibility or end-to-end success
without measured evidence. Stop at an owner-review gate when specified below.
```

## 3. Definition of the feasibility prototype

The base system must demonstrate:

1. A Rust host process with visible status and emergency stop.
2. At least one actual native overlay adapter first, then the planned platform matrix.
3. An Android app receiving real stylus input and showing immediate preview.
4. Authenticated tablet-to-host annotation commands over both Wi-Fi and USB/ADB.
5. Freehand, arrow, rectangle, ellipse/circle, colors, widths, undo/redo and clear on the host screen.
6. Pointer mode with absolute motion/click/drag, isolated from drawing mode.
7. Selected-monitor capture streaming back to the tablet, with aspect-preserving mapping.
8. No indefinite duplicate ink, recursive capture or stuck buttons.
9. A reproducible evidence bundle showing what works and what remains blocked.

A no-video drawing pad is permitted as an intermediate milestone to isolate pen-to-host rendering. It is not the final feasibility result. A local synthetic scene generator is permitted to test native overlays early. It is not evidence of Android or network integration.

## 4. Priorities and exclusions

Priority: visible host ink correctness > safe event handling > monitor geometry > streaming/reconciliation > measured optimization > packaging polish.

Exclude account systems, cloud relay, audio, clipboard, file transfer, virtual monitors, native pressure-aware HID emulation, multi-client collaboration, document editing, durable annotation storage, installers and production update infrastructure. Do not build a web administration dashboard as a substitute for platform work.

Baseline target: Android API 30+, Windows 11 x86-64, one declared X11 setup, one declared layer-shell Wayland setup and one declared GNOME Wayland setup. Record exact versions rather than fabricate a generic Linux guarantee. Initial software renderer is acceptable if it meets measured feasibility targets; a speculative GPU abstraction is not required first.

## 5. M0 — repository, authority and environment gate

Tasks:

- Check existing files and uncommitted changes; preserve unrelated work.
- Read the specs, list any contradictions and apply the stated precedence. User requirement overrides a weaker fallback.
- Discover Rust/Android/JDK/SDK/build tools, display session, compositor, GPU, PipeWire/portal availability, ADB and actual devices. Never assume all are accessible from CI or a container.
- Create workspace skeleton, protocol directory and build instructions. Select/pin toolchains and candidate dependencies after verifying actual APIs.
- Write ADRs: raster-first renderer; native adapters; GNOME exception pending approval; TLS pairing; wire format and language interop; initial video pipeline.
- Populate project-status/capability-matrix.md with UNKNOWN/NOT RUN, not optimistic PASS.

Exit evidence: environment inventory, architecture sketch, dependency/license inventory and runnable unit-test skeleton. Ask one consolidated set of consequential questions only: approval of GNOME GJS exception, missing reference devices/sessions and any genuinely blocking system setup. Do not ask the owner to choose low-level details already specified.

## 6. M1 — deterministic scene and actual host overlays

Implement dd-geometry, dd-scene, dd-render, bounded presenter commands, doctor CLI and scene replay tool. Replay creates visible ink on a live desktop independently of network/Android.

Suggested command contract (agent must implement or document exact replacements):

```sh
cargo run -p dd-diag -- doctor --output reports/doctor.json
cargo run -p dd-diag -- outputs
cargo run -p dd-host -- overlay-demo --output OUTPUT_ID
cargo run -p dd-diag -- replay tests/fixtures/basic-scene.json
cargo test --workspace
```

Implement the native adapter for the current real environment first. Other modules may be cfg-gated, but stubs must return Unsupported, never synthetic success. For Windows use layered click-through window; X11 alpha/input-shape overlay; native Wayland layer-shell; GNOME GJS adapter only after owner approval. See DD-ARCH-001 for lifecycle, protocol and focus rules.

Mandatory visual tests: original live application remains visible; ink above it; all four tools; underlying keyboard and pointer unaffected; fullscreen slideshow; selected output only; clear restores unchanged application content; stop removes overlay. Verify opaque painted pixels also pass clicks through.

Owner review gate: obtain evidence of at least one actual presenter and identify blockers for each other target. Do not spend the rest of the project on Android/video while all desktop overlay approaches remain unproven.

## 7. M2 — secure session and real tablet drawing

Build TLS/TCP host and Android input app with current source geometry, tool controls and local preview. Keep video optional in this milestone. App can display a neutral calibrated source-sized drawing surface, explicitly labeled NO VIDEO TEST MODE.

Feasibility pairing minimum:

- Host creates TLS identity and displays its SHA-256 public-key fingerprint plus a cryptographically random 128-bit single-use invitation token, expiring in 5 minutes.
- Owner transfers/enters fingerprint and token into tablet, or scans an equivalent QR invitation. Android verifies the pinned fingerprint during handshake; token proves invitation possession after TLS identity validation. Never accept all certificates as a shortcut.
- Host shows pending device and requires local approval before any input or capture capability is granted.
- Use in-memory session credentials for this feasibility milestone; re-pair after process restart. Persistent trusted-device storage is deferred, not faked with plaintext files.
- Each control/video/annotation socket authenticates with a separate derived/issued session-binding credential over TLS. Never authorize by session ID alone.
- Rate-limit pairing attempts, expire tokens, revoke on stop and mask secrets in logs. Clipboard/terminal history leakage must be documented.

The host binds only loopback by default. Wi-Fi requires explicit --listen ADDRESS and visible warning. No public or wildcard exposure by default. Host status identifies USB/local versus Wi-Fi endpoint and approved peer.

USB setup, performed explicitly by the owner using a selected device:

```sh
adb devices -l
adb -s SERIAL reverse tcp:47831 tcp:47831
adb -s SERIAL reverse tcp:47832 tcp:47832
adb -s SERIAL reverse tcp:47833 tcp:47833
```

Tablet connects to 127.0.0.1 for USB and the selected host IP for Wi-Fi. Disable tablet Wi-Fi in the USB test to prove the path. Do not enable network ADB. Report missing USB authorization/driver rather than fall back silently.

Exit evidence: real pen draws all tools on the PC over each transport; palm/finger does not draw; actual cancellation works; source epochs reject stale events; disconnect cancels transient strokes; undo/redo and chunked long-stroke commit are correct. No screenshot or synthetic MotionEvent test can stand in for real pen evidence.

## 8. M3 — pointer input and safety

Implement per-platform OS mouse adapter separately from presenter. Windows SendInput, X11 XTEST and native Wayland RemoteDesktop/EIS or approved portal fallback. Observe OS permissions, identity mapping and privilege boundaries; no /dev/uinput, root or accessibility hacks.

Implement mode-ack barriers, interaction generations, reliable down/up, heartbeat, lease expiry and release-all. Draw mode sends no underlying mouse clicks. Mode change during contact cancels it before acknowledgment; source change releases all buttons.

Exit evidence: click, drag, right click, hover where hardware supports it; cancellation and cable pull mid-drag cause no stuck button; elevated Windows target restriction is reported; Wayland works in native applications, not just XWayland. OS permission denial is clear and non-destructive.

## 9. M4 — monitor capture and minimal streaming

Implement actual selected-monitor capture on Windows and Linux (portal/PipeWire on Wayland). Query tablet decoder support, negotiate SDR H.264, and stream using a separate bounded TLS/TCP video connection.

Begin with 1280x720 at 30 fps to debug pipeline correctness, then measure 1920x1080 at 60 fps where supported. These settings are initial experiments, not permission to change final performance requirements. Log capture/encode/decode queue ages; never grow queues to maintain nominal fps.

Implement source list, host authorization where required, tablet selection, source/geometry epoch, letterbox transform, rotation and stream reset/config/keyframe. On Wayland the tablet chooses among authorized sources; reauthorization may be required to add another monitor. Do not identify an output by an ambiguous list index alone.

Prove the video annotation capture mode and reconciliation protocol from DD-ARCH-001. A presenter submission acknowledgment is not sufficient to remove local ink in INCLUDED mode. Do not conceal double ink by disabling mandatory host drawing. Do not capture an opaque self-presentation window recursively.

Exit evidence: tablet shows the live selected source and matching ink; output switch aligns pen; 4K desktop fits tablet without distortion; fullscreen annotation works; no indefinite duplicate ink, flicker or feedback loop.

## 10. M5 — cross-platform and performance evidence

Run actual target sessions or prepare owner-run scripts where unavailable. Missing machines/sessions remain NOT RUN. A successful build is COMPILED ONLY. Generic mocked adapters are UNIT TEST ONLY.

Required matrix rows:

| Platform | Native overlay | Fullscreen ink | Pointer | Video/geometry | Wi-Fi | USB | Pen-to-host latency |
|---|---|---|---|---|---|---|---|
| Windows 11, exact build/GPU | NOT RUN | NOT RUN | NOT RUN | NOT RUN | NOT RUN | NOT RUN | NOT MEASURED |
| X11, exact WM/compositor | NOT RUN | NOT RUN | NOT RUN | NOT RUN | NOT RUN | NOT RUN | NOT MEASURED |
| Wayland, layer-shell desktop/version | NOT RUN | NOT RUN | NOT RUN | NOT RUN | NOT RUN | NOT RUN | NOT MEASURED |
| GNOME Wayland, exact Shell version | NOT RUN | NOT RUN | NOT RUN | NOT RUN | NOT RUN | NOT RUN | NOT MEASURED |

Measure local ink, pen-to-PC visible response, PC-to-tablet video and full round trip separately. Targets from the specs are proposals, never substitute fictional numbers. Record refresh rates, codec, bitrate, reference hardware, camera method and uncertainty. High-speed camera is preferred for physical input-to-photon; raw clocks across machines cannot be subtracted without offset estimation.

QUIC evaluation comes only after mandatory functional gates. It is not necessary to implement QUIC to decide whether core overlays/stylus/streaming are feasible. Record it as deferred rather than claim UDP was tested. Before the complete prototype is approved, execute the earlier baseline's required transport comparison or obtain an explicit deferral decision.

## 11. Automated checks and owner-run tests

Rust checks: formatting, workspace tests, clippy with documented exceptions, parser/property tests, invalid lengths/nonfinite coordinates, stale epoch rejection, idempotent commits, chunk assembly budget, annotation undo/redo and geometry transforms. Dependency audit tools may require installation/network; do not claim they ran if unavailable.

Android: Gradle unit tests and APK build; reducer fixtures cover pointer indices, historical samples, palm cancellation SDK differences, barrel latching, geometry/mode changes and lifecycle stop. Instrumented tests supplement, not replace, actual stylus tests. Golden wire fixtures are consumed by both languages.

Owner procedure:

1. Run doctor and save capability report.
2. Run overlay demo over the desktop and an actual fullscreen slide show.
3. Install/launch Android APK, pair after checking host fingerprint.
4. Test real pen drawing on Wi-Fi; repeat with ADB and Wi-Fi disabled.
5. Switch tools/colors, undo/redo, clear, pointer drag and right-click.
6. Switch between two monitors; test mixed scaling and fullscreen presentation.
7. Pull cable mid-drag/stroke, reconnect, background tablet and stop host.
8. Enable live video and inspect alignment, persistent ink and duplication.
9. Run 30-minute mixed workload and export non-sensitive metrics.

Provide OS-specific commands and extension recovery steps; avoid generic 'run the app' instructions. Installing/enabling the GNOME extension and opening firewall ports are explicit owner actions, never silent agent operations.

## 12. Stop conditions and reporting discipline

Stop the affected milestone and report:

- Overlay cannot appear over fullscreen content on a required target.
- GNOME exception is rejected or selected API version cannot be supported.
- Capture-to-output identity is ambiguous.
- Annotation reconciliation cannot be made correct without projector flicker.
- Rust/Android TLS identity validation cannot be implemented with verified APIs.
- Required tool/hardware/session is missing.
- Native encoder/decoder/capture dependencies or licensing block distribution.

Keep independent work progressing where safe, but do not label a missing MUST as optional. A report can conclude 'feasible on Windows, blocked on GNOME pending X' rather than manufacture a universal result.

Each milestone report includes: changes, commands actually run and exit status, versions, requirement IDs, evidence locations, test results, unresolved risks and next experiment. Never fabricate screenshots, camera results, benchmark numbers or owner approvals.

## 13. Required repository deliverables

```text
project-status/implementation-plan.md
project-status/environment.md
project-status/capability-matrix.md
docs/protocol.md
docs/pairing-and-threat-model.md
docs/run-windows.md
docs/run-linux-x11.md
docs/run-linux-wayland-layer.md
docs/run-linux-gnome.md
docs/run-android-and-adb.md
docs/adr/                   # versioned architecture decisions
project-status/feasibility-report.md
reports/                    # actual non-sensitive test/metric outputs
scripts/                    # explicit setup/test commands, no silent system changes
```

Code deliverables: buildable Rust workspace, Android project/APK where build environment allows, real adapters, protocol schema/fixtures, native extension when approved, diagnostics/replay tools and test harness. Include exact build/run instructions, lockfiles and known limitations.

Final feasibility verdict MUST be one of: PASS on specified tested configurations, PARTIAL with named blocked configurations/features, or BLOCKED with evidence and next experiment. PASS requires all mandatory features for every configuration claimed as supported, not every possible desktop in existence.

## 14. Source grounding

Platform implementation requirements derive from the source registers in DD-ARCH-001 and DD-ANDROID-001. Particularly consequential references:

- Microsoft layered-window transparency and input pass-through: https://learn.microsoft.com/en-us/windows/win32/winmsg/window-features
- Wayland layer-shell lifecycle, overlay layer and empty input region: https://wayland.app/protocols/wlr-layer-shell-unstable-v1
- GNOME extension architecture and shell privileges: https://gjs.guide/extensions/overview/architecture.html
- Android input cancellation/historical samples: https://developer.android.com/develop/ui/views/touch-and-input/input-compatibility-on-large-screens
- Android advanced stylus behavior: https://developer.android.com/develop/ui/views/touch-and-input/stylus-input/advanced-stylus-features
- ADB TCP reverse forwarding: https://android.googlesource.com/platform/packages/modules/adb/+/refs/heads/master/docs/user/adb.1.md

The milestones, port assignments, pairing proposal, performance targets and repository layout are project design instructions, not externally validated performance or security results.
