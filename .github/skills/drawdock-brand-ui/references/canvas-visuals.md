# Canvas visuals

The references are `assets/mockups/canvas-01-selection.png` and `canvas-02-path-and-longpress.png`. Draw these on the tablet canvas only. Whether the PC overlay shows selection chrome is a behavior decision already made in the PoC; keep it.

## Selection (Select / Move, Rotate, Resize)

- Bounding box: 1.5 dp `selection_stroke`, dashed 6/4 dp (`DashPathEffect` / `PathEffect.dashPathEffect`), not filled.
- Corner resize handles (4): 12 dp rounded squares (2 dp radius), fill `handle_fill`, 2 dp `handle_stroke`. The touch target is 40 dp, centered on the corner.
- Rotation handle: Ø 16 dp circle, 32 dp above the top-center, joined to the box by a 1.5 dp cyan line. Fill white, 2 dp cyan stroke, with `ic_dd_rotate` at 11 dp in `brand_navy` inside.
- The handles rotate with the selection; the box follows the object's rotated bounds.
- Mirror H/V and Deselect have no extra canvas chrome.
- The same white/cyan handles work on both the white canvas and the Blackboard. Don't recolor them per background.

## Polygon / polyline construction

- Committed segments: the current ink color and width.
- Vertices: 8 dp circles, white fill, 2 dp stroke in the ink color.
- Rubber-band segment (last vertex → pen): ink color at 50% alpha, half the width (min 2 dp), dashed 8/6 dp.
- Polygon only: a 1.5 dp cyan dotted hint (3/4 dp) from the pen back to the first vertex. The first vertex gets a Ø 16 dp cyan ring as the close target.
- Finish path / Cancel path in More become enabled while construction is active (see the menu spec).

## Long-press ring (menu trigger feedback)

- Centered on the stationary pen point, Ø 40 dp, 3 dp stroke. The track uses `long_press_track`, and the progress arc `long_press_ring` (round cap, clockwise from 12 o'clock).
- Show it after `long_press_ring_delay` 150 ms of stillness so normal strokes never flash it. It fills linearly to 100% at `long_press_total` 550 ms, then the menu opens and the ring disappears.
- If the pen moves beyond the PoC's existing slop or lifts, hide it instantly. Don't change the PoC's timing or slop values; if they differ from 550 ms, use theirs.

## Board and grid

- Blackboard on: canvas fill `canvas_blackboard` (#1F2B27).
- Grid on: 1 dp lines every `grid_spacing` 24 dp. Use `grid_line_on_light` on white and `grid_line_on_dark` on the Blackboard. The grid sits under the ink.
- Both toggles are independent, and Clear all resets both (existing behavior).

## Pen tools

- Highlighter: the current color at `highlighter_alpha` 0.35 with flat caps, only if the PoC doesn't already fix an alpha. It is not a user control.
- Laser pointer: core dot 8 dp `laser_core`, glow 24 dp `laser_glow`. The trail fades over `laser_trail_fade` 400 ms. If the PoC already uses the selected ink color for the laser, keep that.
- Default ink widths: Thin 3 dp, Medium 6 dp, Thick 12 dp.
