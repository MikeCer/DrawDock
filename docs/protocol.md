# DrawDock protocol status

The W2 host/client implement one authenticated TLS/TCP connection on port 47831
for pairing, control, pointer events and annotation commits. A second pinned
TLS connection on the same port carries video, so USB still needs only one ADB
reverse mapping. The host generates
a current-user DPAPI-protected, persistent TLS identity and displays a QR invitation containing its
SHA-256 fingerprint and a single-use 128-bit token with a five-minute expiry.
The Android client pins the scanned certificate fingerprint before sending
the token; the host requires explicit approval in a native Windows dialog.
Manual entry and explicitly requested USB/ADB invitation delivery remain
available. No unverified-TLS bootstrap or short-code bearer authentication is
used.

QR invitations have the exact form
`drawdock://pair?v=1&host=<IP>&port=<port>&fingerprint=<64 hex>&token=<32 hex>`.
Only private/loopback numeric IP endpoints and the five specified unique fields
are accepted by the scanner. The host's expiry is authoritative; no synchronized
tablet clock is needed. The invitation is not a general-purpose deep link and
scanning does not grant remote input until the PC approves.

On first approval, `paired.remembered_device` contains a random 128-bit device
ID and independent 256-bit reconnect credential. Android saves the endpoint,
pin and credential encrypted with an Android Keystore AES-GCM key. The host
persists only the credential's SHA-256 digest alongside the stable TLS identity,
inside its DPAPI-protected registry.

Subsequent control connections verify the saved certificate pin and send
`{"type":"resume","device_id":...,"token":...}` instead of `pair`. A recognized,
unrevoked credential receives the normal `paired` response without a local
approval prompt; `remembered_device` is null (the saved credential is unchanged).
Every connection still receives a fresh video credential and repeats video
geometry readiness. The host resets pointer state/mode and clears the scene
after each session, but keeps its listener and trust registry for reconnects.
Authentication failures are bounded to three per ten-second window.

Frames are newline-delimited JSON and limited to 64 KiB. Large annotation
objects use ordered 48 KiB base64 chunks, a 512 KiB transaction limit and a
SHA-256 digest. The client queue and pending-stroke budget are bounded. Pointer
coordinates are normalized to the selected monitor; moves carry a generation
and may be coalesced, while button transitions are kept ordered. Android spaces
session frames by at least 12.5 ms (at most 80 frames/second), including chunked
commits and heartbeats, below the unchanged host guard of 120 messages/second.
The writer waits before dequeuing so new motion can replace the pending sample
even on a fast USB link. Down/up/cancel and mode/control barriers remain ordered;
one queue slot is reserved for release. Stalls do not accumulate catch-up credit.
A 1.5-second
read lease closes idle/disconnected sessions, releases any held host mouse
button and clears session annotations.

The single **Clear** control clears both tablet and PC annotations when connected.
It cancels active input, sends `clear_display` with a request ID, and pauses input
until `clear_ack` arrives before clearing the tablet preview. A five-second
timeout reports an unknown PC state rather than success. When disconnected,
Clear removes only local marks and explicitly reports that no PC is connected.
The host rebuilds every frame on a transparent surface so clear/undo remove old
pixels rather than drawing over the previous frame. Host tests cover this
redraw, but physical-display clearing still requires manual verification.

Freehand/highlighter commits contain all samples; polygon/polyline commits
preserve their vertices. Arrow, line, rectangle, circle and ellipse commits
contain exactly two bounding/drag endpoints. Android regression tests cover
both drag directions and vertex preservation.

The host defaults to loopback and accepts only explicitly selected private or
loopback addresses. Wi-Fi has not been tested; no wildcard/public listener,
firewall change, certificate bypass or automatic elevation is used. For the
USB workflow, use the loopback listener and ADB reverse port 47831. The launcher
cleans up mappings it creates after the host stops; externally created mappings
remain the owner's responsibility.

Protocol version 2 sends the enumerated monitor list, selected monitor/geometry
epoch and an independent 128-bit video credential in the approved `paired`
response. The video connection authenticates with `{"type":"video","token":...}`
after verifying the same certificate pin. It cannot grant control access.
Its packets are `kind:u8 | epoch:u64-BE | length:u32-BE | payload`, bounded to
8 MiB. Kind 1 is JSON H.264 configuration (bounded to 64 KiB), kind 2 is a
complete Annex-B access unit, and kind 3 reports failure. SPS/PPS and IDR frames
are repeated; no dependent encoded frame is silently dropped.

The capture path remains the installed native Windows FFmpeg gdigrab process,
controlled and cleaned up by Rust. It now preserves up to 2560x1440 at a
requested 30 fps, without upscaling small sources, using Lanczos scaling and
explicit limited-range BT.709 conversion/signaling. H.264 High level 5.0 retains
4:2:0 Android compatibility. B-frames and encoder lookahead remain disabled.
Video configuration reports the selected encoder and color space.

The machine-specific launcher defaults to `h264_nvenc` (RTX 3080 Ti verified):
P5/ultra-low-latency, CQ 18, VBR 12 Mbit/s target, 24 Mbit/s maximum, 2-Mbit VBV.
Direct host CLI defaults to software `libx264`: veryfast/zerolatency, CRF 18,
the same maximum/VBV, four threads. `--video-encoder software|nvenc` explicitly
selects either; an unavailable encoder fails visibly rather than silently
changing quality or hardware requirements.

A one-access-unit pending queue and 500-ms socket write timeout remain bounded.
The reader tolerates pipe batches by waiting up to 100 ms for that same slot,
preserving all dependent frames in order; a prolonged stall closes video and
requires reconnecting from a fresh keyframe. Shutdown cancels that wait.
Android validates even dimensions/rate, selects a decoder supporting the format,
checks its advertised size/rate capability, and uses the negotiated frame rate
for timestamps. High bitrate/Wi-Fi behavior and physical decoder playback still
require acceptance; this is not a measured input-to-photon latency claim.

`select_source` carries a source name, request ID and current `source_epoch`.
The host validates the output, releases buttons, resets any in-flight commit,
retains scenes per monitor for this session, and increments the epoch. The
overlay moves to that physical output and the encoder restarts with a fresh
configuration/keyframe. Android cancels contact and fits video and ink to the
same aspect-preserving rectangle. Input stays suspended until a decoded frame
for that epoch is rendered and `source_ready` / `source_ready_ack` completes.
All scene/pointer commands carry their enqueue-time epoch; stale geometry is
rejected. Output removal/geometry changes require reconnection.

The current capture contract is INCLUDED: Android shows only active provisional
pen ink over video, not another copy of committed host objects. On pen-up,
provisional ink hands off immediately to captured authoritative ink. This avoids
indefinite double ink but can show a short handoff gap; a conservative
presentation/capture revision watermark is **not implemented**. Reconciliation
and physical visual acceptance remain **BLOCKED**, not passed.

Ports 47832 and 47833 are unused. QUIC/UDP is **DEFERRED**.

## Drawing/editor extensions

The same authenticated, paced, epoch-scoped control channel now supports
`line`, `circle`, `polygon`, `polyline`, `text`, `image` and `highlighter` objects
alongside the original tools. Optional `text`, `image_data` (raw base64 PNG)
and `transform` fields extend `InkObject`; absent transform means identity.
Text uses normalized `style.width` as font size and is limited to 2048 UTF-8
bytes/eight lines. Images are limited to 320 KiB compressed and 1280x1280
decoded, with bounded decode allocation. Payload sizes count toward existing
scene/history budgets and the 512 KiB chunked-commit limit. Image import does
not add host clipboard/file access or a new network endpoint.

Transforms use normalized affine coordinates:
`x' = a*x + c*y + tx`, `y' = b*x + d*y + ty`. Coefficients must be finite,
bounded and nonsingular. Edits replace an object's absolute transform and add
one undo state. Rendering converts this matrix to physical monitor geometry
before drawing; rotating a wide monitor does not stretch shapes.

New commands carry `request_id` and `source_epoch`:

```json
{"type":"select_object","x":0.5,"y":0.5,"request_id":1000000001,"source_epoch":1}
{"type":"transform_object","object_id":42,"transform":{"a":1,"b":0,"c":0,"d":1,"tx":0.1,"ty":0},"request_id":1000000002,"source_epoch":1}
{"type":"set_canvas","blackboard":true,"grid":true,"request_id":1000000003,"source_epoch":1}
```

`editor_ack` includes the request/epoch, `canvas:{blackboard,grid}`, `mode`,
and `selection:null` or `{id,bounds:{left,top,right,bottom},transform:{...}}`.
Bounds describe the transformed normalized object. Errors correlate their
request and epoch so the client does not retain an indefinite pending edit.
Clear/history acknowledgements and `source_ready_ack` also report canvas/mode.
Blackboard forces Draw and prohibits Pointer mode, including when undo restores
it. Clear atomically resets objects and canvas, with undo restoring both.

Laser messages use
`{"type":"laser","active":true,"x":0.5,"y":0.5,"rgba":[255,0,0,255],"source_epoch":1}`.
They are transient, coalesced without crossing stops/source/control barriers,
and do not add undo entries or a per-sample acknowledgement. Android refreshes
the current point every 100 ms while contacting/hovering; the host expires it
after roughly 300 ms without a refresh and invalidates the overlay. Menu open,
cancellation, source changes and disconnect stop the laser.

The native presenter uses resvg with locally installed Windows fonts for
escaped plain text, SVG-generated paths, affine edits, correctly composited
highlighter strokes and validated PNG images. It accepts no user-supplied SVG.
Every redraw starts from a transparent framebuffer, or the selected blackboard,
and writes premultiplied BGRA to the layered window. All content is authoritative
on the PC and therefore included in monitor video; no independent tablet-only
background or image is presented as remote success.
