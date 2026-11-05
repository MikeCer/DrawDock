# Features and interaction

## Workspace and radial menu

The Android workspace combines a top monitor selector with aspect-fit video and
an ink view covering the same rectangle. Settings opens connection/pairing
controls without reserving permanent drawing space.

Hold the pen still for about 550 ms, lift it, then tap a radial menu action.
The menu is anchored near the contact and clamped inside the workspace.
Finger holds do not open it. **Pen tools** in the header is an accessible fallback.
The center button closes the root or returns from a submenu.

The hold gesture cancels provisional ink rather than committing a dot. Movement
beyond slop disarms it. In Pointer mode, mouse-down waits until a drag or short
tap is recognized, preventing a long-press menu from clicking the desktop.
Contacts started while input is suspended can open the menu but cannot become
ink/clicks when readiness changes.

For the menu's visual vocabulary and labeled design references, see
[Brand and UI guidelines](branding.md#radial-menu-design-references). The
interaction descriptions here, not mockup styling, define its behavior.

## Drawing tools

| Tool | Interaction and scene representation |
|---|---|
| Freehand | Drag; preserve all sampled XY points, including a single-point dot |
| Highlighter | Drag; preserve samples with translucent color and a wider stroke |
| Line / arrow | Drag between two endpoints |
| Rectangle / ellipse | Drag opposite bounding corners |
| Circle | Drag diagonally to define equal physical width/height |
| Polygon | Tap at least three vertices, then More > Finish path; close the path |
| Polyline | Tap at least two vertices, then More > Finish path; leave it open |
| Text | Tap an anchor, type, then choose Place |
| Image | More > Paste image imports an image from the tablet clipboard |

Style offers colors including white, and thin/medium/thick widths. Text uses
the width selection as a font-size choice. Text input accepts up to 512
characters/eight lines; the host independently enforces its UTF-8/line limits.

Long-pressing during a polygon/polyline does not append another vertex.
More > Cancel path discards it. Changing tool/style, monitor or session also
cancels unfinished geometry.

Drawing is stylus-only: finger and eraser tool types do not create strokes.
Local pen samples include pressure, tilt and orientation, but annotation commits
carry XY geometry and a uniform object width, not per-point pressure rendering.
Hover does not create permanent ink.

## Selection and transformations

Choose Select / Move and tap an object. The host chooses the topmost matching
transformed geometry. The tablet outlines its original bounding rectangle
through the full affine transform, preserving rotation, mirroring and shear.
The border is 4 dp blue with a white contrast outline. Corner double-arrow icons
and a curved-arrow rotation handle have 48 dp pen hit targets.

- Move: drag inside the selected bounds.
- Rotate: drag the curved-arrow handle, or choose Rotate and drag around the frame's center.
- Resize: drag any corner; its opposite stays fixed and scaling follows the
  object's local axes. Choosing Resize also permits dragging inside the frame,
  using the original bottom-right corner and opposite top-left anchor.
- Mirror H / Mirror V: reflect the selected object around its frame's center.
- Deselect: remove the tablet selection.

Edits compose with the existing affine matrix while retaining original points
and payloads. Hover cursors indicate move, rotation or the current resize
direction where supported by the tablet. The rotation handle stays within the
viewport even near its edges.

The tablet previews the oriented frame during a drag and retains that pose
until acknowledgement; the PC object updates on pen-up. A rejected edit restores
the acknowledged frame. Each committed edit is undoable.

Selection is single-object and is cleared by history/source/session changes.
It is a client interaction state, not persistent document metadata.

## Presentation

Laser is a temporary colored spot, shown for pen contact or hover. Android
refreshes it every 100 ms; the host expires it after about 300 ms without a
refresh. It does not create an object or undo entry. Opening the menu, cancelling
contact, changing mode/source or ending the session stops it.

Blackboard fills the selected physical monitor black. Grid adds 32-pixel cells
over either the transparent desktop or blackboard. Both are authoritative host
canvas state and therefore appear in the mirrored video.

Blackboard forces Draw mode and prohibits Pointer mode, including when restored
by Undo, so the covered desktop cannot receive concealed mouse clicks.

## Tablet clipboard images

Paste is an explicit user action in a ready Draw session. It reads a clipboard
image URI, not a copied URL/text, the PC clipboard or a PC file chooser.
There is no background clipboard monitor or broad storage permission.

`ClipboardImage` decodes on a worker with orientation handling. Source images
above 32 megapixels are rejected. The result is fitted within a 1280-pixel
longest edge and reduced further as needed to fit 320 KiB of lossless PNG.
The host validates canonical base64, PNG structure, dimensions and decode bounds.

Paste centers the image and activates Select / Move. Once it appears, tap it
to select and position it. Queueing is not reported as confirmed insertion:
transport and host validation errors are surfaced separately.

## Pointer mode

Draw/Pointer mode is available under More and changes only after host
acknowledgement. In Pointer mode, hover moves the mouse, short taps click and
drags hold the selected button. The primary stylus barrel button selects the
right mouse button; ordinary contact selects the left.

Input generations reject stale/overlapping transitions. Mode changes, source
changes, Clear/history operations and session teardown release held buttons.

## Clear, history and lifecycle

Undo/redo cover annotations, text/images, affine edits and canvas flags.
Clear is a single undoable operation that removes objects and resets both
Blackboard and Grid. It cancels active input and waits for the PC's correlated
acknowledgement before clearing the tablet preview. A timeout reports unknown
PC state rather than success. Disconnected Clear affects local preview only.

Scenes/history are separate per monitor for one connection. Backgrounding the
Android app ends its session and clears host scene/history; drawings are not
stored as persistent documents. Copy a clipboard image before connecting, or
reconnect after copying.

Committed host ink is included in video, so Android suppresses independently
rendered committed objects. Provisional ink ends on lift and can briefly
disappear before the corresponding captured frame arrives.

See [architecture](architecture.md) and [protocol](protocol.md) for the underlying
state and message contracts.
