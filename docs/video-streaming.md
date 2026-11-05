# Video streaming

## Capture and decode path

`apps\dd-host\src\video.rs` owns a native FFmpeg process using Windows `gdigrab`.
It captures the selected output's physical rectangle, including the mouse and
annotation overlay. Rust reads H.264 from stdout, splits Annex-B access units
at AUD boundaries and sends complete units over a dedicated pinned-TLS stream.
Android's `RemoteVideoDecoder` validates the format and supplies `MediaCodec`
output to the workspace `TextureView`.

FFmpeg executable resolution is:

1. The process-local `DRAWDOCK_FFMPEG` override.
2. The direct Chocolatey-installed executable, when present:
   `C:\ProgramData\chocolatey\lib\ffmpeg\tools\ffmpeg\bin\ffmpeg.exe`.
3. `ffmpeg` resolved from `PATH`.

Prefer the actual executable over a launcher shim so process ownership/cleanup
tracks capture correctly. FFmpeg is external and is not downloaded or bundled
by DrawDock. Check the chosen build's licensing before redistribution.

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
negotiated frame rate. A source change creates a new configuration/keyframe
sequence and requires a matching rendered epoch before source readiness.

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

## Annotation handoff

Capture includes authoritative PC ink. Android draws provisional active ink,
not a second copy of committed host objects. On lift it removes the provisional
mark immediately. Without a capture-revision watermark there can be a short
gap before video displays the committed mark.

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

References:

- [rusty_ffmpeg crate documentation](https://docs.rs/crate/rusty_ffmpeg/latest)
- [NVIDIA FFmpeg integration](https://docs.nvidia.com/video-technologies/video-codec-sdk/13.0/ffmpeg-with-nvidia-gpu/index.html)
