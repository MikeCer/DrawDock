# Feature → icon map

Drawable = `R.drawable.ic_dd_<id>` (SVG: `assets/icons/svg/dd_<id>.svg`). The menu label is the short text under the icon. The content description is for TalkBack and stylus-hover tooltips; make it the full name.

## Root (8 items, recommended clockwise order from 12 o'clock) + center

| # | Item | id | Label | Opens |
|---|---|---|---|---|
| 1 | Pen | `pen` | Pen | submenu |
| 2 | Shapes | `shapes` | Shapes | submenu |
| 3 | Select / Edit | `select_edit` | Select / Edit | submenu |
| 4 | Style | `style` | Style | submenu |
| 5 | Board / Grid | `board_grid` | Board / Grid | submenu |
| 6 | Undo | `undo` | Undo | action |
| 7 | Redo | `redo` | Redo | action |
| 8 | More | `more` | More | submenu |
| center (root) | Close | `close` | (none) | dismiss |
| center (submenu) | Back | `back` | parent title, 9 sp | up one level |

## Submenus

| Parent | Item | id | Label | Content description |
|---|---|---|---|---|
| Pen | Freehand | `freehand` | Freehand | Freehand pen |
| Pen | Highlighter | `highlighter` | Highlighter | Highlighter |
| Pen | Laser pointer | `laser_pointer` | Laser pointer | Laser pointer |
| Shapes | Rectangle | `rectangle` | Rectangle | Rectangle |
| Shapes | Circle | `circle` | Circle | Circle |
| Shapes | Ellipse | `ellipse` | Ellipse | Ellipse |
| Shapes | Line | `line` | Line | Straight line |
| Shapes | Polygon | `polygon` | Polygon | Polygon (closed path) |
| Shapes | Polyline | `polyline` | Polyline | Polyline (open path) |
| Shapes | Arrow | `arrow` | Arrow | Arrow |
| Shapes | Text | `text` | Text | Text |
| Select / Edit | Select / Move | `select_move` | Move | Select and move |
| Select / Edit | Rotate | `rotate` | Rotate | Rotate selection |
| Select / Edit | Resize | `resize` | Resize | Resize selection |
| Select / Edit | Mirror horizontally | `mirror_horizontal` | Mirror H | Mirror horizontally |
| Select / Edit | Mirror vertically | `mirror_vertical` | Mirror V | Mirror vertically |
| Select / Edit | Deselect | `deselect` | Deselect | Deselect |
| Style | Color | `color` | Color | Color (submenu) |
| Style | Width | `width` | Width | Stroke width (submenu) |
| Style → Color | Red…White | (swatch, no icon) | Red, Orange, Yellow, Green, Blue, Black, White | "<Color> ink" |
| Style → Width | Thin / Medium / Thick | `width_thin` / `width_medium` / `width_thick`, or a live line preview (preferred) | Thin, Medium, Thick | "Thin, 3 dp" etc. |
| Board / Grid | Blackboard | `blackboard` | Blackboard | "Blackboard, on/off" |
| Board / Grid | Grid | `grid` | Grid | "Grid, on/off" |
| More | Switch Pointer/Draw | `mode_pointer` when in Draw mode, `mode_draw` when in Pointer mode | Pointer mode / Draw mode | "Switch to pointer mode" / "Switch to draw mode" |
| More | Finish path | `finish_path` | Finish path | Finish polygon or polyline |
| More | Cancel path | `cancel_path` | Cancel path | Cancel polygon or polyline |
| More | Paste image | `paste_image` | Paste image | Paste image from tablet clipboard |
| More | Clear all | `clear_all` | Clear all | Clear all annotations on tablet and PC |
| More | Settings | `settings` | Settings | Connection settings |

## Outside the radial menu

| Element | id |
|---|---|
| Pen tools button (opens menu) | `pen_tools` |
| Monitor selector | `monitor_select` (single), `multi_monitor` (picker list header) |
| Connection status / transport | `usb`, `wifi`, `connected`, `disconnected` |
| Draw/Pointer status chip | `mode_draw`, `mode_pointer` |

The style swatch order is fixed: Red, Orange, Yellow, Green, Blue, Black, White (`DDColor.inkPalette`).
