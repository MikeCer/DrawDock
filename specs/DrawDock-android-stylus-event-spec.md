# DrawDock — Android Stylus Input and Event Handling Specification

Document ID: DD-ANDROID-001 | Version: 0.2 | Date: 2026-11-04  
Companions: `DrawDock-rust-overlay-architecture.md` and `DrawDock-feasibility-agent-instructions.md`.

## 1. Purpose and terminology

Implement a normal Android application that receives stylus events, mirrors the selected PC screen, and sends drawing/pointer commands to the Rust backend. “Stylus driver” here means an application-level input adapter and event pipeline, not a kernel driver, custom digitizer firmware, accessibility service or root component. Use Android MotionEvent APIs and existing device drivers [A1,A2].

Host-visible annotations are mandatory. Immediate tablet ink is feedback, never an acceptable substitute. The Rust backend owns canonical scene state; Android owns responsive local previews and user interaction. Neither Android pressure capture nor eraser tool detection implies pressure-aware Windows/Linux HID emulation.

Minimum Android API: proposed 30 (Android 11). Pin compile/target SDK and library versions at kickoff. Do not raise minimum SDK merely to use newer optional input/rendering features. API 33 cancellation behavior must be feature-gated [A3].

## 2. App modules and UI

```text
app/              # connection screen, toolbar, lifecycle
input/            # MotionEvent adapter and pure state machine
geometry/         # viewport/source transforms and epochs
protocol/         # generated protobuf + golden fixtures
session/          # TLS, authorization, reconnect, ordered sends
render/           # video Surface + transparent local ink layer
video/            # MediaCodec capabilities and recovery
metrics/          # local bounded diagnostics, export on request
```

Use a dedicated custom View for input so stylus MotionEvent details/history are explicit; the surrounding UI may use Compose. Do not perform networking or protobuf work inside onTouchEvent/onHoverEvent. Avoid a full Activity recomposition per point.

Screen includes connection status, active monitor, Draw/Pointer mode, tool, color, width, undo/redo, clear and disconnect. Make active mode unmistakable. Disable contact until authentication, geometry and mode acknowledgment are current. Controls remain operable by finger; finger contact on the drawing viewport does not create ink or PC clicks in the first prototype.

Keep the display awake only during an active foreground session. Use the normal Activity lifecycle; no background input service. Screen capture occurs on the PC, so the tablet does not need MediaProjection. Request only permissions actually needed, including INTERNET; QR camera scanning is optional, not required when a manual invitation can be entered.

## 3. Event extraction contract

For each relevant event, copy primitives immediately; do not retain the framework MotionEvent beyond its dispatch callback unless explicitly obtaining/recycling a copy. Track pointer ID, never assume pointer index is stable [A1].

Capture:

| Field | Meaning |
|---|---|
| pointerId | Current gesture pointer identity, not persistent device identity |
| toolType | STYLUS, ERASER, FINGER, MOUSE, UNKNOWN |
| actionMasked/actionIndex | Transition and affected pointer index |
| x/y | View-local coordinates; not screen rawX/rawY |
| eventTime/history timestamps | Monotonic sample time converted consistently to microseconds |
| pressure | Raw observed value plus range/availability metadata |
| tilt/orientation | Axis values when actually supported |
| buttonState | Stylus primary/secondary barrel bitmask |
| deviceId | Local diagnostic identity, not wire authentication |
| flags | Cancellation handling with SDK guards |
```

Normalize pressure with the device's available motion range only when valid. Pressure is not guaranteed to be within a universal 0..1 range; reject nonfinite values and preserve raw diagnostic data. Unsupported axes remain absent, not fabricated zeros. Use getMotionRange for availability. Tilt/orientation are optional metadata, not required to draw.

For ACTION_MOVE, emit historical samples oldest first, then the current sample for the active pointer using its current index. Preserve sample timestamps. Do not send historical samples again after a duplicate callback; deduplicate by gesture/sample sequence without assuming two equal coordinates indicate the same sample. Unknown/nonfinite coordinates are ignored with a bounded error counter.

### Required dispatch outline

```kotlin
// Domain outline, not drop-in Android implementation.
when (event.actionMasked) {
    ACTION_DOWN, ACTION_POINTER_DOWN -> handleContactStart(event, event.actionIndex)
    ACTION_MOVE -> handleHistoricalThenCurrentSamples(event)
    ACTION_POINTER_UP, ACTION_UP -> {
        val canceled = Build.VERSION.SDK_INT >= 33 &&
            (event.flags and FLAG_CANCELED) != 0
        handleContactEnd(event, event.actionIndex, canceled)
    }
    ACTION_CANCEL -> cancelAllActiveContacts() // ALWAYS, on all supported SDKs
}
```

Hover is processed through onHoverEvent/generic motion as appropriate; test actual device dispatch. ACTION_HOVER_EXIT means stop hover indication, not a synthesized click or button release for an unrelated gesture. Barrel ACTION_BUTTON_PRESS/RELEASE handling must also inspect button-state changes during ordinary motion/contact; devices differ in reporting paths [A1,A2].

## 4. Pointer ownership and input state machine

Only one stylus/eraser pointer owns the viewport interaction at a time. A finger down must not prevent a later stylus from being recognized; consume the viewport touch stream so pointer transitions continue arriving. A second pen-like pointer is ignored until the active one ends. UI controls have their own normal touch handling.

States:

```text
Disconnected / NotReady
ReadyHover
PointerContact(generation, pointerId, heldButton)
DrawContact(objectId, pointerId, selectedTool, style, sourceEpoch)
Suspended
```

The input reducer is a pure testable Kotlin class independent of Activity/View. Inputs include pen events, mode/source changes, connection state and lifecycle changes. Outputs are local-preview commands and bounded semantic wire commands. The View does not directly send network traffic or inject OS input.

Source, mode and geometry have separate generations. Capture them at gesture start. Any incompatible change cancels that gesture before applying the new state. Do not reinterpret half of a freehand stroke as a mouse drag.

## 5. Pointer mode semantics

- Hover: send replaceable absolute motion when supported. Devices without hover still support contact pointing.
- Tip down: send reliable left down with current normalized coordinate and interaction generation. No speculative PC click on hover/prediction.
- Move: send absolute motion, preserving order relative to down/up barriers. Motion may be coalesced to the latest real sample in a send interval.
- Tip up: send reliable up at final real coordinate; end the generation.
- Cancel: send reliable cancel/release-all for that interaction; never commit a pending click afterward.
- UI right-click button: emits a reliable right down/up at the last valid source position; disabled if no valid position exists.

Initial barrel mapping: if primary barrel is held BEFORE contact, that contact becomes a right-button interaction instead of left; it remains latched until lift/cancel even if the barrel is released mid-contact. Hover barrel presses alone do not click. Secondary barrel is unassigned initially. This deliberately simple behavior avoids ambiguous left-to-right transitions. Device-specific alternative mapping requires an explicit setting/test, not silent double clicks.

Physical mouse users may still operate the host; the prototype does not claim arbitration among simultaneous local/remote dragging. Host stop always wins. PC backend must release all remote-held buttons on connection lease expiry, independently of Android delivering an up event.

## 6. Draw mode semantics and tools

No PC mouse down/up commands may originate from viewport contact in Draw mode.

- Freehand: render local samples immediately; send actual samples as previews; commit final geometry at lift.
- Arrow: contact defines start, motion defines endpoint, lift commits. Derive arrowhead from a shared geometry rule on Rust and Android, or transmit canonical polygon geometry; do not produce mismatched endpoints.
- Rectangle: two contact corners; optional constraint creates square.
- Ellipse: two bounding-box corners; optional constraint creates circle.
- Tool/style is latched at contact start. Changing a toolbar selection during contact cancels the unfinished shape before selection changes.
- Hardware ERASER tool may select object erase/delete if implemented; if not, display a clear unsupported-tool indicator. It must never silently draw an ordinary colored stroke.

Minimum UI: six distinguishable colors including red, yellow and blue; three widths; undo/redo; clear with confirmation. Defaults: opaque red, constant width, freehand. Finger drawing and pressure-varying width are deferred, not accidentally enabled by toolType fallback.

A tap in freehand creates a visible dot. Arrow/rectangle/ellipse shorter than a declared minimum screen-space threshold are canceled or become a documented minimum shape; select one rule and test it. Recommended prototype rule: freehand dot, other tools canceled below 3 tablet pixels.

## 7. Palm rejection and cancellation

ACTION_CANCEL always cancels the active interaction, on every supported API. Do not require FLAG_CANCELED for this action. On API 33+, ACTION_POINTER_UP with FLAG_CANCELED cancels that pointer's interaction instead of committing it. Earlier Android versions cannot provide the same multi-pointer cancellation detail [A3,A4].

Keep pending ink reversible until commit. On cancellation remove local preview and send CancelPreview for its object ID. Do not remove unrelated committed strokes. If cancellation races a transmitted commit, use an explicit compensating delete with a command ID and report the action; do not pretend the original commit was never sent.

Ignore fingers on the viewport as an additional app policy, not as proof of perfect hardware palm rejection. Handle palm-first, pen-first, pen-plus-finger and changing pointer indices. ACTION_OUTSIDE is not a reliable gesture termination mechanism. Lifecycle/disconnect watchdogs are required.

## 8. Coordinate mapping

Geometry message defines: source ID, geometry epoch, canonical width/height, rotation convention, video dimensions, content crop (initially none), and annotation capture mode. Source geometry is authoritative from Rust. Tablet view changes create a new local viewport transform and cancel contact before reuse.

Use the actual displayed video content rectangle after insets, toolbars and letterboxing. For a fit transform:

```text
s = min(viewWidth/sourceWidth, viewHeight/sourceHeight)
displayWidth = s*sourceWidth; displayHeight = s*sourceHeight
left = (viewWidth-displayWidth)/2; top = (viewHeight-displayHeight)/2
u = (x-left)/displayWidth; v = (y-top)/displayHeight
```

Apply the agreed rotation transform exactly once. New contacts in letterboxing are ignored. An active drag leaving the image is clamped to its edge and still ends reliably on up/cancel. A pointer returning from the margin does not create a second gesture.

Store annotations in source-normalized coordinates, not tablet pixels. Test four corners, center, portrait display, Android rotation, monitor switch and toolbar/inset changes. Never use the tablet's physical resolution as if it were PC desktop coordinates.

## 9. Preview, canonical state and capture reconciliation

Two rendering layers: decoded video Surface below, transparent local ink above. Low-latency front-buffer rendering/prediction are optional optimizations after correct handling exists [A2]. Predictions are local-only; never transmit predicted coordinates as final ink or mouse events. Reconcile them against actual samples on every update.

Object IDs generated at BeginPreview persist through Commit. Rust acknowledgment determines canonical revision. Protocol details, bounds and chunked commit transactions are in DD-ARCH-001; a 20,000-point stroke must not be shoved into a 64 KiB message. Actual final geometry can be simplified under a declared pixel error bound before transfer; log the method, not the point content.

INCLUDED video: committed ink is already in video; Android must not draw it again. Remove provisional ink only after the protocol's validated video/capture watermark covers the relevant revision. EXCLUDED video: Android renders canonical ink and merges preview by ID. Surface presentation, host submit acknowledgments and video capture are different events. If watermark accuracy is not yet proven, mark reconciliation gate blocked rather than hide ghost/double ink.

On reconnect, remove uncertain previews and receive a canonical scene snapshot before enabling contact. Do not replay old button events. Reconcile final commands by command ID if the outcome is uncertain; never duplicate a stroke solely because its ack was lost.

## 10. Scheduling and backpressure

Input callbacks: copy/extract and enqueue only. Local preview renders on the next available frame, independently of network and video decode. Default outbound preview interval <=8 ms, batch <=64 points; these are proposed tuning settings, not guarantees of a particular device sample rate. Input frequency is discovered, not hardcoded to 240 Hz.

A single ordered semantic writer owns control/annotation transitions. Bounded channels preserve Begin/Commit/Cancel and down/up. Replaceable hover/pointer motion may coalesce only within a generation. If actual stroke samples exceed preview budget, thin intermediate preview samples while retaining unsimplified final geometry within global limits. If final commit cannot be queued, cancel explicitly and notify the user; never allocate without bound.

Suggested client bounds: one active pen, 4,096 pending preview samples, 8 MiB final-object/scene working budget subject to shared limits. Do not wait for a network round-trip to render local ink. Do not allow video backlog to occupy the semantic writer.

## 11. Video and transport integration

TLS/TCP is the feasibility baseline on Wi-Fi and ADB. Use separate authenticated session-bound connections for control/input (47831), video (47832), annotations (47833). Bind each with credentials for the same approved session; session ID alone is not authentication. TCP_NODELAY for control/annotation. Android connects to the explicit PC address on Wi-Fi and 127.0.0.1 through ADB reverse on USB [A6].

MediaCodec decodes negotiated H.264 to a Surface. Query profile/level/resolution support and low-latency feature support rather than assuming API 30 guarantees a low-latency decoder [A5]. Reconfigure on stream epoch changes; never feed old frames after a source switch. Video reconnect requires fresh configuration/keyframe as appropriate.

No raw unauthenticated UDP or disabled TLS checks. QUIC is a later measured Wi-Fi experiment; it does not block proving host annotation viability.

## 12. Lifecycle, metrics and diagnostics

onPause, focus loss affecting the input surface, screen lock, surface destruction, disconnect and source/mode changes cancel active gestures and enqueue release/cancel before suspension when possible. Backend lease expiry handles cases when Android cannot send. When resumed, require an active session, current geometry acknowledgment and no inherited held buttons.

Diagnostics page: device/API, detected tool types, axis ranges, observed sample interval distribution, hover/barrel availability, cancellation counters, current source/epochs, decoder, queue ages and connection status. Export metrics CSV only on explicit user action. No raw point trails/screenshots/secrets by default. Monotonic Android and PC clocks are different; round-trip and cross-device latency must not use naive subtraction.

## 13. Tests and acceptance

Pure reducer/geometry tests: pointer index changes, palm-first stylus, no hover device, up/cancel rules, barrel latching, out-of-bounds start versus active drag, stale epochs, mode switch mid-contact, rotation and reconnect. Shared protobuf golden vectors with Rust.

Instrumented real tablet tests:

1. Real pen freehand/arrow/rectangle/ellipse appear on the physical host output and tablet.
2. Finger/palm contact does not draw or click. API 33 cancellation behavior verified if available; older-device limits documented.
3. Pointer drag, right click and cancel have no stuck host button.
4. Hover works where hardware reports it; absence is labeled, not emulated with fabricated events.
5. At least 100 repeated strokes and 20 mode/source changes without duplicates or stale input.
6. Background app, pull cable mid-contact, rotate, lock and reconnect safely.
7. Local pen-to-ink target p95 <=25 ms and pen-to-host-visible target p95 <=50 ms USB / <=70 ms Wi-Fi on reference hardware. Targets are proposed; use camera evidence and record actual refresh rates. Automated event injection does not establish real stylus latency or palm behavior.

## 14. References

[A1] Android MotionEvent API: https://developer.android.com/reference/android/view/MotionEvent

[A2] Android Developers, Advanced stylus features: https://developer.android.com/develop/ui/views/touch-and-input/stylus-input/advanced-stylus-features

[A3] Android Developers, Input compatibility — historical samples and cancellation: https://developer.android.com/develop/ui/views/touch-and-input/input-compatibility-on-large-screens

[A4] Android Developers, Stylus palm rejection: https://developer.android.com/develop/adaptive-apps/cookbook/stylus-palm-rejection

[A5] AOSP, Low-latency decoding in MediaCodec: https://source.android.com/docs/core/media/low-latency-media

[A6] AOSP, ADB manual: https://android.googlesource.com/platform/packages/modules/adb/+/refs/heads/master/docs/user/adb.1.md

[A7] Android Developers, Advanced stylus support codelab: https://developer.android.com/codelabs/large-screens/advanced-stylus-support

Numeric limits, batch intervals, module boundaries and tool mappings are proposed DrawDock design decisions. The source documentation explains platform input behavior, not achieved project latency or device-specific capability.
