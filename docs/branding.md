# Brand and UI guidelines

DrawDock uses deep navy surfaces, cyan interaction accents, coral brand artwork
and light text/icons on dark surfaces. Branding is a visual layer over the
existing [feature contract](features.md); it must not change annotation,
transport, trust or input behavior.

The visual palette is navy `#0B1B3A`, cyan `#22D3EE`, coral `#FF6B57`,
light text `#E6F0FF` and danger `#FF5470`. These are RGB values, separate
from the annotation palette below.

## Asset ownership and destinations

Each component owns the graphics and styling sources it uses. Documentation
images are checked in under `docs/assets/brand`; Android graphics and tokens
belong in its own resource tree when integrated. A fresh clone must render,
build and run without the original design kit.

The root `brand` folder is an optional, ignored local design workspace, not a
published dependency. Do not link documentation to it or load application
resources from it. This guide records the approved visual and compatibility
rules independently of that workspace.

| Surface | Asset and treatment |
|---|---|
| Root README | `docs/assets/brand/github-cover.png`; keep the introduction and setup links as real text |
| GitHub social preview | Reuse `docs/assets/brand/github-cover.png`, 1280x640; configure separately in repository settings |
| Documentation index | `logo-light.png` / `logo-dark.png`, displayed at 320 pixels using a theme-aware `picture` |
| Technical documentation | Selected, clearly labeled design references; no decorative banner on every page |
| Android radial menu | Component-owned `ic_dd_*.xml` drawables and typed Views resources under `android/app/src/main/res` |
| Android header/settings | Pen tools, monitor, mode and connection/transport icons with accessible text |
| Launcher/system splash | Adaptive launcher assets and splash icon, integrated separately from menu styling |
| Optional onboarding | At most a small selection of optimized illustrations, without delaying drawing or reconnect |
| Windows host | Preserve the authoritative scene, captured desktop and native approval behavior |

Use original artwork without stretching, recoloring logos or removing their
transparent background. Keep dark and light logo variants paired. Use text
labels alongside unfamiliar icons, and never rely on color alone for meaning.

Android eight-digit colors use ARGB (`#AARRGGBB`). Convert explicitly for SVG/CSS;
an eight-digit CSS color uses a different channel order. Standalone SVG icon
exports need explicit colors: Markdown image embedding does not reliably
inherit the surrounding page's `currentColor`.

## Radial menu design references

**Design references, not application screenshots.** These images illustrate
the intended visual language. They do not prove an implemented UI state, and
the compatibility rules below override their selection sizes, ink colors and
disabled-state examples.

<img src="assets/brand/radial-menu-root.png" alt="Design reference: eight radial root items with Pen at the top and Close in the center." width="480">

The root order stays Pen, Shapes, Select / Edit, Style, Board / Grid, Undo,
Redo, More, clockwise from the top. Center Close/Back and the existing action
versus submenu dismissal behavior remain unchanged.

<img src="assets/brand/radial-menu-more.png" alt="Design reference: More submenu with target mode, Finish path, Cancel path, Paste image, Clear all and Settings." width="480">

Use semantic action icons with labels matching the [menu inventory](features.md).
Color uses swatches; width uses a live line preview or matching width icons.
The More mode icon represents the mode to switch to,
not the current mode. Clear all uses danger styling without changing its scope
or confirmation.

## Approved compatibility rules

| Area | Required treatment |
|---|---|
| Input and menu | Preserve stationary stylus hold timing/slop, deferred pointer-down, finger exclusion, item order, action dismissal and readiness gating |
| Selection | Keep the 4 dp solid border, 7 dp white contrast underlay, 28 dp visible handles, 48 dp hit targets and 48 dp rotation offset; brand colors/icons may change |
| Transform geometry | Preserve oriented corners, opposite resize anchors, mirrored/sheared transforms, viewport clamping and acknowledgement handling |
| Ink colors | Keep the application palette below; brand colors do not redefine annotation colors |
| Widths | Preserve Thin 3 dp, Medium 6 dp and Thick 12 dp, including their existing text-size mapping |
| Blackboard/Grid | Keep host-authoritative black fill and 32-physical-pixel grid; do not paint a different local tablet background over video |
| Highlighter/Laser | Preserve the current alpha, cap, selected-color and refresh/expiry behavior; do not add a fading trail as a styling side effect |
| Polygon/polyline | Preview improvements must not introduce tap-first-vertex completion; Finish/Cancel path semantics stay unchanged |
| Availability | Reflect known application state only; do not infer host Undo/Redo availability from the local preview |
| Feature scope | No eraser, per-object deletion, arbitrary color picker or opacity slider |
| Layout/accessibility | Preserve usable targets and readable labels at large font sizes and viewport edges; do not blindly shrink the disc or clip long labels |
| Animation | Respect system animation settings; animation must not delay input cancellation, source changes or a queued action |

The annotation palette, in menu order, is Red `#FF0000`, Orange `#FF9800`,
Yellow `#FFFF00`, Green `#00FF00`, Blue `#0000FF`, Black `#000000`,
White `#FFFFFF`. These are RGB values; highlighter alpha is applied separately.

<img src="assets/brand/selection-reference.png" alt="Design reference showing selection on light and dark backgrounds; use the larger project-specific border and handle dimensions instead." width="960">

This selection reference illustrates contrast, not the approved dimensions.
Do not copy its 1.5 dp dashed perimeter, 40 dp hit targets, reduced handles,
green-black background or grid spacing into the application.

## Android integration boundaries

Use the existing Kotlin Views/custom Canvas implementation; a Compose migration
is not required. Keep actionable items accessible instead of replacing them
with an unlabeled bitmap. Cache tinted drawables outside drawing callbacks.
Keep `dd_` / `ic_dd_` resource prefixes and avoid collisions with existing names.

Copy only the required icons into the Android component and define its colors,
dimensions and motion values in typed Views resources. Alpha and animation
scales also need typed scalar resources, not hardcoded UI literals. Reconcile
the compatibility rules before importing values. If generated resources are
introduced, their generator and inputs must be checked in with that component;
update sources and outputs together without requiring the ignored design kit.

The host does not currently send authoritative `canUndo`/`canRedo` state. Keep
existing actions usable rather than presenting guessed disabled states. Adding
host availability metadata would be a separate behavior/protocol change.

Launcher/splash graphics need the actual app theme and API 30-compatible
integration. Do not copy a template resource tree wholesale. Do not bundle
mockups, unused illustrations, Compose samples or documentation exports into
the APK.

## Maintaining documentation assets

The six checked-in PNGs are the documentation's usable source assets, not
outputs that contributors must regenerate before viewing the guides. No Python,
Pillow or original kit is needed to render them.

When replacing an image, preserve its aspect ratio and transparency, optimize
the PNG, and review it at the displayed size. Keep these maximum widths:

| Image | Maximum width |
|---|---|
| `github-cover.png` | 1280 px (1280x640 for GitHub social preview) |
| `logo-light.png`, `logo-dark.png` | 640 px |
| `radial-menu-root.png`, `radial-menu-more.png` | 760 px |
| `selection-reference.png` | 960 px |

Commit updated images and their referencing documentation together. Any future
reproducible image-generation workflow must include its required sources and
tooling in the published component rather than reference the local workspace.

## Review checklist

Review README and documentation images in light/dark themes and narrow layouts.
Check relative links, image alternatives and explicit mockup labels. For Android
changes, run the existing tests/build/lint and check every menu page, large text,
TalkBack, pen/finger gestures, reduced motion and edge placement on the tablet.
Retest rotated selection and USB/Wi-Fi source readiness when touching UI state.

Record milestone approvals, validation results and pending acceptance in
[`project-status`](../project-status/README.md), not in these technical guides.
