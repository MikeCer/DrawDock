# Radial menu visual spec

The visual reference is `assets/mockups/menu-00-anatomy.png` (dimensions) and `menu-01…10-*.png` (one per level/state). Token names are listed below. The Views prefix is `dd_`; Compose uses `DDColor` / `DDDimen` / `DDMotion` in camelCase.

## Geometry

- Disc: Ø `menu_diameter` 288 dp, fill `menu_surface` (navy, 94% alpha), 1 dp `menu_ring_stroke` rim. Shadow: `menu_shadow`, blur 16 dp, y offset 4 dp.
- Inner divider ring: radius = center radius + 14 dp, 1 dp `menu_divider`.
- Items: centers on radius `menu_item_ring_radius` 100 dp, evenly spaced (`360°/n`), item 0 at 12 o'clock, clockwise. The same rule applies to submenus (2 to 8 items).
- Item slot: 64 dp. Icon 24 dp, centered 8 dp above the slot center. Label 11 sp, baseline 22 dp below the slot center, color `on_dark`, medium weight, single line.
- Center button: Ø 64 dp, fill `menu_center_surface`, 1.5 dp `menu_center_stroke`. Root: `ic_dd_close`, 24 dp. Submenu: `ic_dd_back`, 22 dp, plus the parent title (9 sp, `on_dark_muted`) under it.
- Placement: centered on the pen point (or the Pen tools button). Shift it so the whole disc stays `menu_edge_margin` 8 dp inside the visible canvas; never shrink it.
- Touch targets: each item gets the full angular sector between the inner divider and the disc edge. The center gets its circle. Taps outside the disc dismiss the menu (existing behavior).

## Item states

| State | When | Visual |
|---|---|---|
| normal | default | icon + label `on_dark` |
| hover | stylus hover over the item (if hover events are handled) | Ø 56 dp disc `menu_item_hover` |
| pressed | pen/finger down on the item | Ø 56 dp disc `menu_item_pressed` |
| selected | current tool in a tool submenu | Ø 34 dp disc `menu_item_selected_bg` behind the icon, icon `menu_item_selected_icon`, label `brand_cyan` semibold |
| active family | root item whose submenu contains the current tool (Pen or Shapes); Style shows the current color | 5 dp `brand_cyan` dot, 29 dp below the slot center |
| toggle on | Blackboard / Grid enabled | icon + label `menu_toggle_on`, plus a small "ON" pill (16×10 dp, cyan, navy 6.5 sp text) at the icon's top-right |
| disabled | Undo/Redo with an empty stack; Finish/Cancel path when no polygon/polyline is being built | whole item at `disabled_alpha` 0.38, not clickable, but still announced |
| danger | Clear all | icon + label `danger` |

Color submenu: 26 dp swatches (`swatch_diameter`) instead of icons. Black gets a 1.5 dp `swatch_outline` ring. The selected swatch gets a 3 dp `brand_cyan` ring at +4 dp and a check mark (navy on Yellow/White, white otherwise).

Width submenu: show a 22 dp horizontal line at the true width (3/6/12 dp) in the current ink color. The selected width gets a 2 dp cyan ring of Ø 34 dp.

Mode item (More): show the mode you would switch to (`mode_pointer` while drawing, `mode_draw` while pointing).

## Motion

| Transition | Spec |
|---|---|
| Long-press feedback | see `canvas-visuals.md` (ring 150 → 550 ms) |
| Open | scale 0.88→1 + alpha 0→1, `menu_open` 140 ms, FastOutSlowIn. Items fade in with a 12 ms clockwise stagger. |
| Into a submenu | outgoing level scale 1→0.92 + fade out, 90 ms; then incoming scale 1.06→1 + fade in, 140 ms. The disc itself stays still. |
| Back | the same, reversed |
| Close / select action / tap outside | alpha→0 + scale→0.95, 100 ms |

If animations are disabled (`Settings.Global.ANIMATOR_DURATION_SCALE == 0` or reduced motion), switch states instantly.

## Accessibility

- Every item has a `contentDescription` (see `icon-map.md`). Toggles announce their state, and disabled items announce "unavailable".
- Contrast: `on_dark` on `menu_surface` is above 12:1, and `brand_cyan` on navy is above 8:1. Don't place labels on the cyan selected disc.
