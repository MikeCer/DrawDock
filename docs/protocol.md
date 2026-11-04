# Protocol specification

The control protocol is version **2**, implemented by
`apps\dd-host\src\session.rs` and Android `SessionClient`.
It uses authenticated TLS/TCP, with separate control and video connections on
the same listener (default `127.0.0.1:47831`). ADB is transport only; it does not
replace TLS or DrawDock authentication.

## Framing and authentication

Control messages are UTF-8 JSON followed by LF. Keep each complete frame within
64 KiB, including its terminator. Numeric coordinates must be finite.

The client verifies the host's exact SHA-256 certificate pin before sending an
invitation or remembered credential. The first message is one of these templates:

```json
{"type":"pair","token":"<32-hex invitation token>","client":"<device label>"}
{"type":"resume","device_id":"<32-hex device ID>","token":"<64-hex reconnect credential>","client":"<device label>"}
```

New pairing requires an open, unexpired, single-use invitation and native PC
approval. Resume requires a recognized, unrevoked credential. Authentication
failure returns `authentication_failed` with a message and closes that attempt.

Successful authentication returns:

```json
{
  "type": "paired",
  "version": 2,
  "sources": [
    {"name":"\\\\.\\DISPLAY2","left":0,"top":0,"right":2560,"bottom":1440,"primary":true}
  ],
  "selected": {
    "source":{"name":"\\\\.\\DISPLAY2","left":0,"top":0,"right":2560,"bottom":1440,"primary":true},
    "source_epoch":1
  },
  "video_token":"<fresh 32-hex session video token>",
  "remembered_device":{"id":"<device ID>","token":"<reconnect credential>"}
}
```

`remembered_device` is supplied on enrollment and is `null` on resume.
Source bounds are physical Windows virtual-desktop coordinates; negative origins
are valid. Initial mode is Draw. Trust persists independently of scene/session
state. See [pairing and threat model](pairing-and-threat-model.md).

## Epochs and readiness

Every non-heartbeat command carries its enqueue-time `source_epoch`, including
source selection and commit chunks. The host rejects a missing/stale epoch.
Do not retag already queued coordinates with a new epoch.

`select_source` carries `source`, `request_id` and the current epoch:

```json
{"type":"select_source","source":"\\\\.\\DISPLAY2","request_id":1,"source_epoch":1}
```

The host releases buttons, cancels the transaction/laser, saves/restores that
monitor's session scene, advances the epoch, relocates the overlay and restarts
capture. It returns `source_selected` with `request_id`, `selected`, `canvas`
and `mode`.

After rendering a video frame for the selected epoch, Android sends
`{"type":"source_ready","source_epoch":2}`. The host replies with
`source_ready_ack`, that epoch, `canvas` and `mode`.
Pointer, editing, scene/history, Clear and mode commands are suspended until
readiness. Video failure revokes readiness; output removal/geometry change
requires reconnection.

`{"type":"heartbeat"}` carries no coordinates or epoch and receives no response.
The client sends it when idle to maintain the control lease.

## Annotation objects

Example direct commit:

```json
{
  "type":"commit",
  "source_epoch":1,
  "object":{
    "id":42,
    "tool":"rectangle",
    "style":{"rgba":[255,0,0,255],"width":0.004},
    "points":[{"x":0.2,"y":0.2},{"x":0.6,"y":0.5}]
  }
}
```

| Field | Contract |
|---|---|
| `id` | Unsigned 64-bit identifier, unique in the scene |
| `tool` | `freehand`, `highlighter`, `line`, `arrow`, `rectangle`, `circle`, `ellipse`, `polygon`, `polyline`, `text`, `image` |
| `style.rgba` | Four unsigned bytes, red/green/blue/alpha |
| `style.width` | Finite normalized width, greater than zero and at most 0.2; relative to the shorter source edge |
| `points` | Original normalized XY geometry in `[0,1]` |
| `transform` | Optional affine matrix; absent means identity |
| `text` | Required for text; nonblank, at most 2048 UTF-8 bytes/eight lines, with permitted newline/tab controls |
| `image_data` | Required for images; canonical base64 PNG without a data-URI prefix |

Freehand/highlighter preserve all samples, including a single-point dot.
Polyline needs at least two vertices; polygon needs at least three. Other tools
use exactly two endpoints/bounding corners. Text width is interpreted as font
size. Images/text occupy their bounding rectangle. Circles account for physical
source aspect ratio.

The host validates PNG signature, complete decoding/end, canonical base64,
dimensions and allocation bounds, and rejects animated PNG. It does not accept
arbitrary image paths, remote URLs or user-supplied SVG.

Successful direct commits return `{"type":"ack"}`. This acknowledgement does not
identify a rendered video frame or establish physical presentation.

## Large commits

Use an ordered transaction when object JSON does not fit a control frame:

| Command | Fields in addition to `source_epoch` |
|---|---|
| `begin_commit` | `object_id`, `total_bytes`, `digest` (64-hex SHA-256 of exact object JSON bytes) |
| `commit_chunk` | `object_id`, zero-based byte `offset`, base64 `data` |
| `finish_commit` | `object_id` |
| `cancel_commit` | No additional fields |

There is one active transaction per session. Chunks must be nonempty and
contiguous, match the object ID and stay within the declared length. Finish
checks total length, digest, parsed object ID and ordinary scene validation
before committing atomically. Invalid ordering/bounds discard the transaction.
Expiry discards it; source changes and session teardown cancel it.
Each successful transaction command returns `ack`.

The host decoded-chunk bound is 48 KiB, but JSON/base64 overhead must still fit
the frame limit. Android sends **36 KiB raw chunks**, producing 48 KiB of base64
plus JSON overhead.

## Editing and canvas

These commands require `request_id` and `source_epoch`:

| Type | Additional fields | Effect |
|---|---|---|
| `select_object` | Normalized `x`, `y` | Topmost transformed bounding-box selection |
| `transform_object` | `object_id`, absolute `transform` | One undoable affine edit |
| `set_canvas` | Boolean `blackboard`, `grid` | One undoable canvas change; releases buttons |

A transform is `{a,b,c,d,tx,ty}`:

```text
x' = a*x + c*y + tx
y' = b*x + d*y + ty
```

Coefficients are finite with absolute value at most 16; translation components
are finite with absolute value at most 4. The determinant's absolute value
must be at least 0.0001. To render on a `W` by `H` physical source, use
SVG matrix `(a, b*H/W, c*W/H, d, tx*W, ty*H)`.

Responses have this shape (`selection` can be `null`):

```json
{
  "type":"editor_ack",
  "request_id":1000000001,
  "source_epoch":1,
  "selection":{
    "id":42,
    "bounds":{"left":0.2,"top":0.2,"right":0.6,"bottom":0.5},
    "transform":{"a":1,"b":0,"c":0,"d":1,"tx":0,"ty":0}
  },
  "canvas":{"blackboard":false,"grid":false},
  "mode":"draw"
}
```

Bounds are the transformed normalized selection bounds. Canvas changes return
null selection. Blackboard forces Draw, and Pointer mode is prohibited while
it is active, including when restored by Undo.

## Modes, pointer and history

`set_mode` supplies `mode: "draw" | "pointer"` and a monotonically increasing
`request_id`. It releases held buttons and returns `mode_ack` with mode and ID.
Annotation commits, selection/transforms and laser require Draw mode.

Pointer messages require Pointer mode:

```json
{"type":"pointer","action":"down","generation":1,"x":0.5,"y":0.5,"button":"left","source_epoch":1}
```

Actions are `hover`, `move`, `down`, `up`, `cancel`; buttons are `left` or
`right` (missing/empty defaults left). Down starts a strictly newer generation;
motion/release must match the active or last applicable generation. Overlapping
downs and stale transitions are rejected. Cancellation with no active button
is harmless. Applied pointer messages can return generic `ack`; clients must
not require an acknowledgement for every motion.

`undo`, `redo` and legacy `clear` return `ack` with epoch, canvas and mode.
`clear_display` requires a positive `request_id` and returns `clear_ack` with
that ID and the same state fields. Clear atomically removes objects, resets
blackboard/grid and clears laser; Undo can restore objects/canvas, not laser.
History/Clear operations release buttons.

## Laser

```json
{"type":"laser","active":true,"x":0.5,"y":0.5,"rgba":[255,0,0,255],"source_epoch":1}
```

Active laser requires normalized coordinates; absent color defaults red.
`active:false` stops it. There is no per-sample acknowledgement or history
entry. Android refreshes at 100-ms intervals, and the host expires the point
after 300 ms without refresh. Coalescing must preserve stops and control/source
barriers.

## Errors and ordering

Command rejection returns `error` with `message`, `request_id` and `source_epoch`
copied from the request (unprovided values are null). Semantic command errors
are reported without claiming mutation succeeded. Framing, rate, lease and
transport failures end the session.

Android spaces every control frame by at least 12.5 ms, including transaction
chunks and heartbeats. Pending motion coalesces only across compatible adjacent
samples. Preserve button transitions, laser stops, source epochs and control
barriers. One queue slot is reserved for release; stalls do not grant catch-up
burst credit.

## Video connection

The second connection verifies the same certificate pin, then sends:

```json
{"type":"video","token":"<fresh video_token from paired>"}
```

Video authentication has a 30-second window and three-attempt limit. It does not
authorize control. The authenticated connection switches to binary packets:

| Header field | Encoding |
|---|---|
| Kind | Unsigned byte |
| Source epoch | Unsigned 64-bit big-endian |
| Payload length | Unsigned 32-bit big-endian |
| Payload | Exactly the indicated byte count |

Kind 1 contains configuration JSON (`codec`, `width`, `height`, `fps`, `source`,
`annotation_capture_mode`, `encoder`, `color_space`); Android bounds it to 64 KiB.
Kind 2 contains a complete H.264 Annex-B access unit, including startup
SPS/PPS/IDR. Kind 3 contains a UTF-8 error message; the host uses epoch zero for
that failure packet. Packet payloads are nonempty and bounded to 8 MiB.
See [video streaming](video-streaming.md) for codec and queue configuration.

## Resource limits

| Resource | Bound |
|---|---|
| Control frame | 64 KiB including newline on host |
| Serialized transaction/object | 512 KiB transaction; direct messages must fit a frame |
| Transaction lifetime | 3 seconds |
| Control traffic | Host guard: 120 messages/second; Android pacing: at most 80 frames/second |
| Control read lease | 1.5 seconds |
| Objects / points | 2,000 objects; 20,000 points/object; 200,000 points/scene |
| Scene estimated storage | 8 MiB, including text/base64 payloads |
| Each undo/redo stack | 32 snapshots and 8 MiB estimated storage |
| PNG | 320 KiB compressed; at most 1280x1280; 16 MiB decode-allocation bound |
| Android pending mark budget | 8 MiB; normal queue 128 entries plus reserved release slot |
| Video access unit | 8 MiB |

Keep Rust validation, Kotlin encoding and tests aligned when changing a bound.
