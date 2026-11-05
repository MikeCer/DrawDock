# Video streaming

## Capture and decode path

`apps\dd-host\src\video.rs` owns a native FFmpeg process using Windows `gdigrab`.
It captures the selected output's physical rectangle, including the mouse and
annotation overlay. FFmpeg's tee muxer emits bounded `framecrc` packet metadata
to stderr **before** the matching Annex-B H.264 bytes to stdout. Rust reads
exact packet lengths instead of waiting for the next AUD, preserving
microsecond gdigrab capture timestamps with `-copyts`, passthrough frame pacing
and a 1/1000000 encoder timebase. No raw 4K frame IPC is added.
Android's `RemoteVideoDecoder` validates the format and supplies `MediaCodec`
output to the workspace `TextureView`.

FFmpeg executable resolution is:

1. The process-local `DRAWDOCK_FFMPEG` override.
2. `runtime\ffmpeg\ffmpeg.exe` beside the host, in installed/portable distributions.
3. The direct Chocolatey-installed executable, when present:
   `C:\ProgramData\chocolatey\lib\ffmpeg\tools\ffmpeg\bin\ffmpeg.exe`.
4. `ffmpeg` resolved from `PATH`.

Prefer the actual executable over a launcher shim so process ownership/cleanup
tracks capture correctly. FFmpeg runs as a separate process. Release packages
bundle a minimal native build, its notices and corresponding source;
development builds can use external tools. See [builds and releases](releasing.md)
before redistributing a changed build.

## Geometry and format

Output fits within 2560x1440 at a requested 30 fps, preserving aspect ratio,
even dimensions and avoiding upscaling. Portrait sources are also fitted to
that box. Scaling uses Lanczos and explicitly converts/signals limited-range
BT.709.

The common encoding contract is H.264 High profile, level 5.0, `yuv420p`,
GOP 30, no B-frames, maximum rate 24 Mbit/s and a 2-Mbit VBV buffer.
Configuration reports `video/avc`, dimensions, frame rate, source name, encoder,
`bt709` color space and `annotation_capture_mode: "included"`.
See [video wire framing](protocol.md#video-connection).

Android selects a decoder supporting the negotiated geometry/rate before
enabling input, checks advertised capabilities and derives timestamps from the
negotiated frame rate using a process-wide monotonic PTS allocator; reconnects
cannot reuse an old texture's timestamp. Decoder output drains
on its own worker, not only after another network packet. A monitor change
creates a new configuration/keyframe sequence. Zoom reuses capture, encoder and
decoder while still requiring matching texture-latched epochs for readiness.

## Encoders

| Choice | FFmpeg encoder | Configuration |
|---|---|---|
| `software` | `libx264` | veryfast, zerolatency, CRF 18, four threads, AUD/repeated headers, fixed key interval, no scene cut |
| `nvenc` | `h264_nvenc` | P5, ultra-low-latency, VBR/CQ 18, 12 Mbit/s target, zero lookahead/delay, spatial AQ, AUD and forced IDR |

Direct `dd-host serve` defaults to software. The PowerShell launcher defaults to
NVENC; select `-VideoEncoder software` on systems without a compatible encoder.
Failure is explicit, not a silent fallback to a different encoder.

## Backpressure and cleanup

An access unit is bounded to 8 MiB. The pending encoded queue holds one unit.
The capture reader waits up to 500 ms for ordered handoff and checks
cancellation; socket writes have a 500-ms deadline.

Never discard an arbitrary dependent H.264 unit to free a slot: later frames
can reference it. A sustained stall ends video and requires reconnection from
a fresh keyframe. The client suspends drawing when video is unavailable.
Source changes restart capture, and session shutdown stops the video service
and its owned child process.
Zoom alone does not restart capture. Frames without safe presentation metadata
are still decoded, but their output buffers are not rendered; dependent encoded
units are never discarded.

## Annotation handoff

Capture includes authoritative PC ink. Android keeps provisional pen/path/text
marks after lift until their object-correlated commit acknowledgements and a
texture-latched scene revision both arrive. Either ordering is supported.
Pending marks use normalized canonical coordinates and reproject through the
displayed viewport, so a zoom acknowledgement does not erase them.

The preview queue admits at most 128 marks with an 8 MiB accounting budget
(including conservative point/string costs). Admission failure is reported
without sending the mark. Host rejection removes that mark; source changes and
disconnect clear pending state. A 10-second handoff failure ends the session
with an explicit error rather than retaining unbounded or apparently committed
ink. These bounds are separate from the existing outbound command budget.

The Windows presenter publishes a bounded 128-interval timeline only after
successful layered-window presentation, magnifier painting and `DwmFlush`.
Frame attribution uses gdigrab's timestamp before `BitBlt`, with a 33.334-ms
margin around presentation transitions. Unknown/evicted intervals cannot
acknowledge content. A wall-clock discontinuity exceeding 10 ms relative to
monotonic time, or a stale/out-of-range capture timestamp, ends video explicitly;
a new video connection starts a fresh clock/timeline.

This is conservative attribution, not an exact capture-completion fence.
Capture/compositor and texture-update timing can briefly show both provisional
and returned translucent ink. Recorded highlighter tests expose that extra
opacity; do not describe the handoff as pixel-identical or overlap-free.

Any change to capture inclusion/exclusion or presentation acknowledgement must
update the host configuration, Android rendering policy and readiness/revision
contract together; changing one renderer alone can create doubled or missing ink.

## Process boundary and performance work

Both Android sockets enable TCP_NODELAY, matching the host sockets. The tablet
disables monitor changes until the current source renders and acknowledges
readiness. Readiness has a 10-second deadline; a video failure ends the control
session and exposes reconnect controls rather than leaving a Loading screen.
The 500 ms encoded-frame handoff and 500 ms socket-write bounds still
fail explicitly on congestion; no automatic video recovery or adaptive bitrate
is implemented.

The subprocess carries already-compressed H.264, not raw desktop frames.
FFmpeg capture/scaling uses CPU memory before hardware upload; this is not a
zero-copy GPU capture pipeline.

`rusty_ffmpeg` supplies raw FFI to the same FFmpeg libraries. With equivalent
encoder settings it does not intrinsically change image quality. In-process
integration would require compatible Windows development libraries/headers
and linking, plus Clang or pregenerated bindings; it also changes fault and
resource-lifecycle boundaries.

For changes, distinguish image quality at a given configuration/bitrate,
unpaced encoder throughput, capture/transport delay and actual input-to-photon
latency. Keep measurements and hardware-specific results in project records
rather than this implementation guide.

For native laser motion, the presenter caches a small premultiplied sprite,
moves it without raising it above Magnifier, and clips it at monitor edges.
The main loop pumps a bounded message batch and targets 60 Hz. Content revisions
do not change for laser motion, release or expiry.

References:

- [rusty_ffmpeg crate documentation](https://docs.rs/crate/rusty_ffmpeg/latest)
- [NVIDIA FFmpeg integration](https://docs.nvidia.com/video-technologies/video-codec-sdk/13.0/ffmpeg-with-nvidia-gpu/index.html)
- [FFmpeg 7.0 gdigrab capture timestamp placement](https://github.com/FFmpeg/FFmpeg/blob/n7.0/libavdevice/gdigrab.c)
