# DrawDock — Rust Cross-Platform Overlay Architecture

Document ID: DD-ARCH-001 | Version: 0.2 | Date: 2026-11-04  
Status: implementation proposal for feasibility testing, not proof of compatibility.

## 1. Authority and companion documents

Companions: `DrawDock-android-stylus-event-spec.md` (DD-ANDROID-001) and `DrawDock-prototype-spec-draft.md`.

This technical proposal includes future cross-platform work. For the current Windows implementation and contributor commands, use [the documentation](../docs/README.md). Linux adapters and QUIC remain deferred.

This document supersedes any earlier suggestion that host-visible annotations may be omitted, replaced with tablet-only ink, or accepted through a capture-only fallback. The user's mandatory requirement is authoritative: draw on the live PC screen using a tablet pen during presentations, on Windows 11 and explicitly supported Linux X11/Wayland configurations. Fullscreen presentation annotations are a release gate, not optional polish.

MUST is required for feasibility acceptance. SHOULD is a default with documented exceptions. Proposed dependency/toolkit choices require validation, not blind adoption. Do not claim compatibility from compilation alone.

The PC backend and shared logic MUST be Rust. A minimal GNOME Shell JavaScript/GJS extension is the proposed necessary platform integration exception. Its use must be explicitly documented and approved; an all-Rust requirement cannot silently be treated as satisfied by an extension. Android application logic is Kotlin. No root or automatic elevation.

## 2. Product invariant

The selected monitor shows the original live desktop/application plus DrawDock ink. Freehand, arrow, rectangle and ellipse/circle are mandatory. Color and width selection, undo/redo and clear are mandatory. Ink persists across Draw/Pointer mode changes, does not modify underlying files, and does not consume underlying application clicks. People looking at the display/projector MUST see it. Sharing a display in conferencing software is a separate tested configuration; application-window sharing is not assumed to contain external overlays.

Not equivalent: a screenshot editor, frozen desktop, opaque substitute presentation, tablet-only drawing, annotation confined inside the DrawDock control window, or a fullscreen view recursively capturing itself.

## 3. Topology and separation of concerns

```text
Tablet stylus -> authenticated session -> ordered input/annotation commands
                                             |
                                     Session actor (Rust)
                                  /          |           \
                          Input router   Scene reducer   Geometry registry
                              |              |                 |
                          OS injection   Scene patches --------+
                                             |
                                  Presenter command bridge
                     /              /                 /             \
                Windows HWND    X11 overlay      Wayland layer    GNOME extension
                                             |
                             original live display + transparent ink

Capture -> scaler -> encoder -> separate video transport -> Android decoder
```

Capture, input injection and annotation presentation are separate capabilities. A working presenter does not prove native Wayland input support. A working portal does not prove overlay support. Backend capability reports must expose these separately.

## 4. Workspace and dependencies

```text
Cargo.toml / Cargo.lock / rust-toolchain.toml
crates/dd-protocol/        # prost-generated wire messages and validation
crates/dd-geometry/        # strongly typed transforms, no OS dependencies
crates/dd-scene/           # deterministic commands, undo/redo, object limits
crates/dd-session/         # actor, authorization, epochs, input safety
crates/dd-transport/       # TLS/TCP baseline; QUIC deferred experiment
crates/dd-render/          # canonical geometry -> premultiplied raster
crates/dd-platform/        # capability types and dispatch
crates/dd-win/             # windows-rs, HWND and SendInput adapters
crates/dd-x11/             # x11rb candidate: RandR, Render, Shape, XFixes, XTEST
crates/dd-wayland/         # wayland-client + layer-shell bindings candidate
crates/dd-gnome/           # zbus candidate: local presenter bridge
crates/dd-capture/         # DXGI and portal/PipeWire adapters
crates/dd-video/           # encoder and keyframe recovery
apps/dd-host/              # local control UI, emergency stop, CLI
apps/dd-diag/              # doctor, list outputs, replay, capture probe
extensions/gnome/          # minimal version-pinned GJS presenter
proto/drawdock/v1/         # shared schemas, fixtures
android/                   # Kotlin client
specs/ docs/ tests/fixtures/
```

Candidate dependencies are architectural suggestions, not verified version selections: Tokio, rustls, prost, tracing, windows-rs, x11rb, wayland-client, a compatible layer-shell binding, zbus, and tiny-skia for initial software rasterization. The agent must check actual APIs, licensing and maintenance, pin versions and record ADRs. No fabricated wrapper API or latest-version assumption. Keep desktop UI toolkit choice independent of native overlay surfaces.

Initial raster rendering is chosen to reduce GPU interoperability risk. Performance must be measured; replacing it with GPU rendering is an optimization, not permission to miss the annotation gate.

## 5. Shared scene and coordinate model

### 5.1 Data contracts

```rust
// Illustrative domain API, not a promised third-party library API.
struct SourceKey { session: SessionId, source: SourceId }
struct GeometryEpoch(u64);
struct SceneRevision(u64);
struct NormalizedPoint { u: f32, v: f32 }
struct InkStyle { rgba_srgb: [u8; 4], width_short_edge: f32 }
struct InkSample { p: NormalizedPoint, pressure: Option<f32>, t_us: u64 }
enum Tool { Freehand, Arrow, Rectangle, Ellipse }
struct InkObject { id: ObjectId, source: SourceKey, tool: Tool,
                   style: InkStyle, geometry: CanonicalGeometry }
```

Use UUIDs for object/command identity. Coordinates are finite normalized values relative to the unrotated agreed source image. A single versioned transform converts capture orientation, physical pixels, compositor logical coordinates and presenter pixels. Do not apply rotation once in Android and again on the host. Width is a fraction of the source's shorter image edge, independent of tablet DPI. Default ink is constant-width; pressure may be preserved but does not imply native PC pen emulation.

Committed scene commands: CommitObject(full final geometry), DeleteObject, Undo, Redo, Clear. Transient commands: BeginPreview, AppendPreview, CancelPreview. The Rust reducer is canonical. Retries with the same command ID are idempotent. Acknowledgments include applied scene revision. Undo/redo is scoped to the active source/session; a new commit after undo clears redo. Clear is an undoable operation, confirmed in UI, not deletion of underlying application content.

Bounds: 2,000 committed objects, 20,000 points/object, 200,000 total committed points/source, 8 MiB decoded scene/source. Reject overflow before mutation. Stream preview batches of <=64 points. At release, a final object exceeding the 64 KiB ordinary message limit is sent as a bounded commit transaction: BeginCommit(metadata, total bytes, digest), chunks <=48 KiB, then FinishCommit. Maximum serialized object 512 KiB; transaction expires after 3 seconds and is not applied partially. Snapshot transfer uses the same bounded chunk principle, max 8 MiB/source. These limits supersede an assumption that every complete stroke fits one ordinary message.

### 5.2 Presenter API and thread ownership

A presenter endpoint is addressed through bounded channels, not called directly from network threads:

```rust
enum PresenterCommand {
    Attach { source: SourceKey, geometry: OutputGeometry },
    ReplaceScene { revision: SceneRevision, scene: SceneSnapshot },
    ApplyPatch { base: SceneRevision, patch: ScenePatch },
    SetVisible(bool), Detach, Shutdown,
}
enum PresenterEvent {
    Ready { capabilities: PresenterCaps },
    FrameSubmitted { revision: SceneRevision, monotonic_us: u64 },
    OutputLost, NeedsSnapshot, Failed { code: ErrorCode, detail: String },
}
```

A frame submission is not proof of photons on the display. Every native window/event loop lives on its owning OS thread; no blanket Send/Sync on handles. Tokio handles networking and actors. Raster work runs on a bounded worker pool. The GNOME renderer runs inside the shell main loop and must never block it on Rust RPC.

Coalesce pending scene snapshots to the newest revision; do not drop ordered semantic commands. If presentation lags, reduce preview frequency rather than grow queues. Separate emergency stop and button-release paths from video work. Presenter failure marks draw capability unavailable and visible in UI; it cannot silently switch to tablet-only ink.

## 6. Windows adapter

Reference basis: Microsoft layered-window behavior and window styles [W1,W2].

- Create a borderless top-level WS_POPUP window per active selected output with WS_EX_LAYERED, WS_EX_TRANSPARENT, WS_EX_NOACTIVATE and WS_EX_TOOLWINDOW.
- Use SetWindowPos(HWND_TOPMOST, ..., SWP_NOACTIVATE) and show without activation. Do not steal focus from PowerPoint or a terminal. Being topmost does not mean above every other topmost window or secure desktop.
- For the first presenter, rasterize to a premultiplied-alpha BGRA DIB and submit through UpdateLayeredWindow with per-pixel alpha. Do not mix SetLayeredWindowAttributes and UpdateLayeredWindow without following Microsoft's reset rules [W1].
- Keep hit testing click-through for opaque ink as well as transparent areas. Do not rely solely on transparent pixels or cross-thread WM_NCHITTEST tricks.
- Declare per-monitor DPI awareness before creating windows. Handle WM_DPICHANGED, display topology changes and negative virtual-desktop coordinates.
- Raster surface size is physical output pixels; logical coordinate conversion occurs only at documented boundaries. Check allocations for overflow at large monitor sizes.
- On lock, output removal or stop: hide/destroy overlays as appropriate, cancel previews and release injected buttons. Test device/session recreation.

Fullscreen slide shows in the normal composited desktop are in scope. UAC, lock screens, DRM/protected content and exclusive rendering paths are not bypassed. For a failing fullscreen target, investigate the supported application presentation mode and native stacking, report the exact limitation, and require owner scope approval before declaring support.

## 7. X11 adapter

Use an ARGB visual with a compositing manager. Probe Render, Shape/XFixes, RandR and XTEST independently. Detect compositing ownership for the actual screen. If no alpha compositor exists, provide a concrete diagnostic and mark the configuration unsupported for the initial presenter; do not silently use an opaque window.

Create an undecorated transparent window with an empty input shape. Initially use a WM-managed window with explicit above/taskbar/pager/focus hints and verify them against the target WM. If fullscreen stacking requires an override-redirect overlay, implement it behind a distinct tested path and document restacking responsibilities; never assume WM hints apply to override-redirect windows.

Use a server-native alpha-capable upload/composition path. Validate pixel format/channel ordering; CPU BGRA memory is not automatically every X server's wire format. Handle expose events, resize and compositor restarts. Do not run an unconditional busy-loop to restack over other applications. Test pass-through on painted and empty pixels, slide navigation with keyboard, Alt+Tab and two-monitor selection.

## 8. Wayland layer-shell adapter

Layer-shell provides explicit overlay-layer and input behavior [L1]. Probe the advertised global; do not infer availability from XDG_SESSION_TYPE alone. GNOME/Mutter cannot be assumed to provide this protocol; route to the GNOME bridge when appropriate [L1].

Required initial sequence:

1. Connect to the native Wayland display and bind supported protocol versions.
2. Create wl_surface without prior role or buffer.
3. Assign layer_surface on an explicit wl_output, layer=overlay, namespace=drawdock.
4. Anchor all four edges, request compositor-sized dimensions, zero margins, exclusive_zone=-1 to span the output without moving for reserved panels, keyboard_interactivity=none.
5. Set an empty wl_region as input region, not NULL (NULL means default/unrestricted input region).
6. Commit with no buffer; receive configure; acknowledge its serial before attaching the first rendered buffer.
7. Submit premultiplied ARGB shared-memory buffers with correct stride; retain each buffer until compositor release. Use frame callbacks to pace redraw.

Support buffer_scale and output transform first; implement fractional-scale/viewporter only after capability negotiation and dedicated tests. A fractional desktop must either render correctly or produce an explicit unsupported result; never accept misaligned ink. Surface configure sizes are logical surface dimensions, not necessarily physical pixel sizes. Layer ordering within the same layer is not guaranteed [L1].

On closed/output removal, destroy resources and suspend input for that source. Map portal capture source to actual wl_output through validated metadata or an explicit host calibration/selection step. A portal node ID is not a wl_output ID. If identity is ambiguous, stop selection rather than draw on the wrong display.

## 9. GNOME Wayland presenter bridge

A GNOME Shell extension is part of the desktop shell process; failures affect shell stability. GJS can access Clutter/St and display/window APIs, but internal API compatibility must be pinned [G1].

Rust owns scene semantics, geometry validation and authorization. The extension owns only transparent actor creation, bounded render primitives, per-monitor placement, lifecycle and rendering acknowledgments. It must not implement network listeners, authentication, mouse injection, file access or shell command execution.

Use a session D-Bus interface owned by the Rust backend, with the extension initiating registration/subscription. Define bounded chunked scene updates, epoch and revision validation, capability handshake, reset and heartbeat. Follow external-backend IPC guidance [G2]. Verify the unique bus sender/name owner on both ends. Session D-Bus is not a security boundary against arbitrary fully compromised same-user processes; document this threat assumption rather than claim caller UID provides strong isolation.

The Rust side only publishes to the registered extension instance for the current shell owner. Do not expose unrestricted mutating methods on the shell. Decode into newly validated structures, cap primitives/points and rendering time per frame, avoid eval and arbitrary strings interpreted as code. Recover revision gaps with snapshot reset.

Create a non-reactive, non-focusable monitor-sized actor through the selected GNOME version's supported/tested shell integration points. Do not assume adding an actor to an arbitrary container places it above fullscreen windows. Gate evidence must show actual fullscreen visibility, keyboard focus preservation, workspace behavior and exclusion from lock/login UI. On disable(), remove every actor, signal handler, timer and bus subscription. On backend heartbeat expiry (1.5 s), hide stale actors and drop previews. On shell owner change, re-handshake and require a fresh snapshot.

Pin tested GNOME Shell versions in metadata; record installation/enable/disable/recovery instructions. Do not install or enable an extension automatically without user action. The user must approve this non-Rust integration exception before it is considered an approved design.

## 10. Capture and duplicate-ink contract

Physical host overlay is mandatory regardless of capture strategy. The first feasibility implementation may use INCLUDED mode: the selected monitor capture already contains authoritative host ink, and Android shows that video plus only local provisional ink. This introduces a reconciliation challenge that must be measured, not hidden.

Negotiate annotation_capture_mode per stream:

- INCLUDED: Android MUST NOT independently render committed canonical objects over video. Retain transient previews until receiving a conservative presentation/capture watermark indicating the same scene revision is represented in a transmitted frame. A presenter FrameSubmitted event alone is insufficient. Correlate capture timestamps after presentation and include a measured safety margin; if reliable correlation is unavailable, explicitly report possible short-lived double ink and do not mark the visual gate passed until corrected.
- EXCLUDED: capture is proven to omit the host overlay; Android renders canonical scene plus local previews. Capture exclusion must be tested per adapter and API; never assume a window flag guarantees the desired result.

A permanent INDETERMINATE mode is not acceptable. Do not hide the overlay around every capture frame: flicker on the projector would violate the primary use case. Do not add composed ink to an already annotated video frame. Do not introduce fullscreen recursive capture as a shortcut.

## 11. Safety, lifecycle and diagnostics

States: Detached -> Initializing -> Ready -> Visible -> Suspended/Failed -> Detached. Source changes increment geometry epoch, release buttons, cancel previews, hide old presenter, obtain current geometry and await tablet acknowledgment before accepting contact. Committed source scenes are retained only for the current authorized session. Stop revokes input immediately, removes presentations and stops capture. Disconnect cancels transient ink and hides host ink after the session lease; a local host action may explicitly preserve committed ink if implemented, but default is no stale remote overlay.

Diagnostics include OS/session/compositor versions, adapter, output identity/mapping confidence, render scale, required extensions, fullscreen test result and failure code. Never log screen content, invitation secrets or full point trails by default.

## 12. Feasibility acceptance

All four adapter rows must have a real test result: Windows 11, X11, a selected layer-shell Wayland desktop, GNOME Wayland. Missing hardware/session = NOT RUN, not PASS.

Mandatory: show all tools/colors on a live application; fullscreen slideshow ink; click/keyboard pass-through; no focus theft; undo/redo/clear; selected monitor only; mixed scaling/rotation alignment; no ghost input on mode switch; clean stop/extension disable; 30-minute bounded-memory run. Native Wayland tests cannot be substituted by XWayland.

Software first-stage performance target: source command to submitted presenter frame p95 <=16.7 ms on declared hardware. This is an engineering target, not input-to-photon proof. Measure actual pen-to-host visible latency with high-speed camera later. The feasibility report must distinguish visual correctness, measured latency and unresolved risks.

## 13. Reference register

[W1] Microsoft, Window Features — layered windows, alpha, click-through and z-order: https://learn.microsoft.com/en-us/windows/win32/winmsg/window-features

[W2] Microsoft, Extended Window Styles: https://learn.microsoft.com/en-us/windows/win32/winmsg/extended-window-styles

[L1] wlr-layer-shell protocol XML rendering, versioned surface lifecycle, layer/input/scale semantics: https://wayland.app/protocols/wlr-layer-shell-unstable-v1

[G1] GNOME GJS, Shell architecture and extension privileges: https://gjs.guide/extensions/overview/architecture.html

[G2] GNOME extension best practices, external backend IPC: https://gjs.guide/extensions/review-guidelines/best-practices.html

[P1] XDG RemoteDesktop portal: https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.RemoteDesktop.html

[P2] XDG ScreenCast portal: https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.ScreenCast.html

[R1] Wayscriber limitations, useful comparative reference, not a DrawDock compatibility guarantee: https://wayscriber.com/docs/troubleshooting/limitations.html

All dependency choices, IPC protocol details and numeric limits above are proposed engineering design. The references establish platform mechanisms, not completed DrawDock implementation or benchmark results.
