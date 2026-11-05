# Architecture

DrawDock has two applications: a native Windows Rust host and a Kotlin Android
client. The host owns the annotation scene and draws it over the selected
physical desktop output. The client displays that output's video and supplies
pen gestures, drawing commands and mouse control.

## Components and ownership

Paths below are relative to the repository root.

| Component | Responsibility |
|---|---|
| `crates\dd-scene\src\lib.rs` | Platform-independent scene, validation, selection, affine transforms, canvas, history and transient laser |
| `apps\dd-host\src\main.rs` | Windows CLI, DPI initialization and application composition |
| `apps\dd-host\src\bin\drawdock-launcher.rs` / `launcher` | Native Win32 launcher/tray, bounded logs and owned backend/ADB lifecycle |
| `apps\dd-host\src\listener_control.rs` | Coordinated live listener set, endpoint cancellation, session events and managed commands |
| `apps\dd-host\src\host_environment.rs` | Bounded subprocess execution and read-only Windows network/profile discovery |
| `apps\dd-host\src\windows_overlay.rs` | Output enumeration, topmost layered window, relocation and presentation |
| `apps\dd-host\src\raster.rs` | Generated SVG, Windows font lookup, resvg rasterization and BGRA conversion |
| `apps\dd-host\src\session.rs` | TLS control service, authentication, protocol dispatch, source barriers and commit transactions |
| `apps\dd-host\src\source.rs` | Selected geometry/epoch and bounded completed-presentation timeline |
| `apps\dd-host\src\zoom.rs` / `windows_zoom.rs` | Bounded shared viewport geometry and native per-monitor magnifier ownership |
| `apps\dd-host\src\windows_input.rs` | Pointer generations, mode policy, held-button tracking and Win32 `SendInput` |
| `apps\dd-host\src\trust.rs` | Persistent TLS identity, DPAPI registry and device revocation |
| `apps\dd-host\src\pairing_window.rs` | Native QR invitation and local approval UI |
| `apps\dd-host\src\video.rs` | FFmpeg process, H.264 framing and authenticated video service |
| `android\app\src\main\java\com\drawdock` | Android workspace, input, radial menu, TLS client, decoder and credential store |
| `Start-DrawDockHost.ps1` | Explicit USB forwarding and loopback host launch |

`dd-scene` contains no Windows APIs. Platform operations stay in the host;
the host binary has a Windows-only compile guard. Android and Rust share a
wire contract, not a common source library.

Rust unit tests live in each crate's `tests\unit` folder, one file per
component. Production modules include them with `#[cfg(test)]` and `#[path]`;
they remain child modules so private implementation details need not become
public for testing. Keep new unit tests in these files rather than inline.
Android JVM tests live in `android\app\src\test\java\com\drawdock`, separate
from production code in `src\main`; future device tests belong in `src\androidTest`.
Existing test commands and Rust test filters are unchanged.

### Launcher and listener ownership

The native launcher starts one sibling `dd-host.exe serve-managed` without a
console. Bounded redirected pipes carry stdin JSON commands and correlated
stdout events alongside readable logs. No separate local management listener
is opened. The managed host initially binds nothing; explicit commands enable
loopback/ADB and selected private LAN endpoints independently. A periodic
profile/address check removes managed LAN endpoints that become ineligible.

All endpoints share one exclusive trust store, overlay and active annotation
session. Video authenticates on the endpoint that accepted the control session.
Disabling that endpoint closes its active socket and invokes normal input,
zoom and scene cleanup; disabling another endpoint preserves the session.
The direct `serve` CLI retains its single explicitly bound endpoint.

The launcher owns its child through a kill-on-close Windows Job Object.
Normal Stop/Quit requests graceful shutdown, removes only owned ADB mappings
and reports failures; a bounded shutdown deadline contains a stuck owned host.
Closing the window merely hides it to the tray. Existing CLI processes and
preexisting forwarding are never silently adopted or terminated.

## Input to physical output

1. `InputReducer` admits stylus contacts and tracks pointer identity independently
   of Android pointer-index changes. `InkInputView` handles gestures and
   provisional local geometry.
2. `SessionClient` serializes commands. `SessionOutbox` bounds pending work,
   paces every frame and coalesces motion without crossing control barriers.
3. The host checks authentication, source epoch, readiness, mode and payload
   limits before mutating `Scene` or injecting input.
4. Content changes advance a revision. Laser samples/expiry do not invalidate
   the full scene raster: a separate 16x16 click-through layered window moves
   one spot, interpolating toward each new sample for at most 1/60 second.
5. `raster.rs` generates SVG from typed scene data, renders with resvg and
   converts premultiplied RGBA to BGRA for the layered window.

The overlay is topmost, click-through and non-activating. It does not own the
underlying application's document or intercept its ordinary input.
Pointer mode uses a separate `SendInput` adapter rather than converting
annotations into mouse strokes.

## Geometry

Windows DPI awareness is initialized before enumerating outputs. Each output
has a device name and physical `left`, `top`, `right`, `bottom` bounds; virtual
desktop coordinates may be negative.

Objects and pointer messages use normalized coordinates in the selected output.
The Android `ViewportFit` calculation gives video and ink the same aspect-fit
rectangle; letterbox space is not an annotation target. Pointer injection maps
selected-output coordinates to the complete Windows virtual desktop.

An object's original points remain unchanged by editing. A normalized affine
matrix records the edit; the renderer converts it to physical pixel geometry.
Circle bounds and rotations account for output aspect ratio.

Shared zoom leaves that canonical coordinate space unchanged. For normalized
source point `(x,y)`, the visible point is
`((x-left)*scale, (y-top)*scale)`. Android applies the inverse to new annotation,
laser and Pointer coordinates and divides new stroke widths by `scale`.
Selection frames use `viewport * object_transform`; submitted absolute edits
are converted back with `inverse(viewport) * edited_frame_transform`.

The presenter owns a click-through, nonactivating layered host and native
Magnifier child on the selected monitor. It includes the original annotation
overlay beneath it but excludes its own host from the magnification source,
preventing recursive capture. GDI capture sees the composed magnified desktop,
so both physical output and tablet video share the same view. Native source
origins are quantized to physical pixels and acknowledged back to Android.
No global fullscreen magnification, UIAccess or accessibility setting is used.

The TLS worker submits a bounded zoom request to the presenter and waits for
its acknowledgement before advancing the source epoch. Zoom preserves the
capture process and decoder; new input still waits for matching texture-latched
video and `source_ready`. Continuous
gesture targets coalesce on Android while a view transition is pending.
Reset/session teardown hide magnification but retain the native control warm.
Monitor switching and presenter exit destroy its monitor-scoped windows. The
laser source stays below Magnifier, never as a second unzoomed dot above it.

## Scene, history and session state

The canonical scene contains objects, blackboard/grid flags, a revision,
undo/redo snapshots and a transient laser. Object payloads include style,
points, optional text/PNG data and an affine transform.

Undoable snapshots contain objects and canvas flags, not laser state. Each
undo/redo stack evicts oldest snapshots to satisfy its own count/estimated-byte
budget. Clear is one undoable operation removing objects and resetting canvas
flags. Session reset additionally removes all history and transient state.
See [protocol limits](protocol.md#resource-limits) for bounds.

Scenes are kept separately per monitor during one authenticated session.
Switching output releases held mouse buttons, cancels an in-flight commit and
laser, saves/restores the relevant scene, moves the overlay and restarts capture.
The source epoch advances. New input waits until Android renders the matching
video geometry and completes `source_ready`.

Disconnect, lease expiry and session failure release input and discard session
scenes/history. Remembered device trust survives these operations; drawings do
not persist across connections.

## Video and ink

FFmpeg captures the selected Windows desktop, including the host overlay, and
encodes H.264. A separate pinned-TLS connection on the same TCP port carries
configuration and Annex-B access units. Android uses `MediaCodec` and a
`TextureView`.

The capture mode is `included`: committed annotations come from video rather
than a second local object renderer. On pen-up, bounded normalized provisional
ink remains until both its commit acknowledgement and a matching
texture-latched frame watermark are available. Either can arrive first.
Previews follow the displayed viewport through zoom; rejection, source change
and disconnect clean them up. Conservative capture attribution can briefly
double translucent ink, so this is not an exact no-overlap presentation proof.
See [video streaming](video-streaming.md).

## Concurrency and failure boundaries

The Windows presentation loop runs separately from the TLS session worker.
Scene and selected-source state are shared through `Arc<Mutex<...>>`.
The video service manages a cancellable capture reader and bounded frame
handoff; FFmpeg is an owned child process.

Android performs network reads/writes, video decoding, clipboard image decoding
and TLS close on workers. UI changes are posted to the main thread. Session
generations and source epochs prevent late callbacks from affecting a replacement
connection or display.
Decoder output drains independently of network input, avoiding a wait for the
next packet before releasing an already-decoded frame.

Do not perform blocking network/process work on the Android UI thread, retain
held buttons across mode/source changes, silently replace trust on errors or
drop dependent H.264 frames. Failures must be surfaced, with input suspended
when video/source readiness is lost.

## Related documentation

- [Protocol](protocol.md) for wire and ordering rules.
- [Features](features.md) for gesture and editing semantics.
- [Pairing and threat model](pairing-and-threat-model.md) for trust boundaries.
