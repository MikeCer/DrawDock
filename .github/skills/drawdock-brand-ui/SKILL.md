---
name: drawdock-brand-ui
description: "Apply the DrawDock brand to the Android tablet app: radial pen menu visuals, icons, color/width tokens, canvas selection and path handles, launcher and splash assets. Use when styling, theming, or adding icons to DrawDock UI. Not for changing menu behavior, input handling, or the PC backend."
metadata:
  version: "2.0"
  product: DrawDock
---

# DrawDock brand UI

You are applying a visual layer to a working, tested PoC. Change how things look, never what they do.

For this repository, first read [Brand and UI guidelines](../../../docs/branding.md).
Its approved compatibility rules override conflicting kit samples, especially
selection dimensions, annotation colors, host-rendered backgrounds and disabled
history states. Treat mockups as design references, never implementation evidence.

## Hard rules

- Keep behavior exactly as implemented: open triggers (stationary pen long-press ~550 ms, Pen tools button), item order, close/dismiss rules, toggles, undo/redo, path finish/cancel, clipboard source, Clear all scope, Settings target.
- Do not add features the PoC lacks: no eraser, no per-object delete, no free color picker, no opacity slider. Icons for those do not exist on purpose.
- Use the app's existing typed resources, not literal hex/dp. Kit tokens in `assets/android/res/values/dd_*.xml` are design references, not replacements for the component-owned values documented in the brand guide. Do not migrate the existing Views app to Compose.
- Keep the PoC's existing item order and annotation color values if they differ from this kit. Mention the difference to the user instead of changing it.
- Use the `dd_` / `ic_dd_` prefixes for copied resources. Never overwrite existing app resources, except launcher/splash resources when the user asks to rebrand them.

## Workflow

1. **Detect the toolkit.** Search the app module for `@Composable` and `setContent {` (Compose) or for XML layouts and custom `View.onDraw` (Views). Find the radial menu, the canvas/overlay drawing code, and the tool/mode enums.
2. **Map features to assets.** Read `references/icon-map.md`. Build a mapping from each PoC enum/action to its `ic_dd_*` drawable, label and content description. Report any PoC item with no icon; don't invent one silently.
3. **Reuse component-owned resources.** Inspect the app's existing `dd_` / `ic_dd_` resources first. Copy only missing, required assets from `assets/android/res/`; do not overwrite existing tokens or import the entire template tree.
4. **Style the radial menu.** Read `references/radial-menu-spec.md`. Apply the geometry, states (selected, pressed, hover, disabled, toggle-on, danger), center Close/Back, labels and motion. Compare against `assets/mockups/menu-*.png`.
5. **Style the canvas visuals.** Read `references/canvas-visuals.md` when touching selection handles, polygon/polyline construction, the long-press ring, Blackboard/Grid, the highlighter or the laser pointer. Compare against `assets/mockups/canvas-*.png`.
6. **Branding (only if asked):** launcher icons, the Android 12+ splash and splash images. Read `references/android-integration.md`.
7. **Verify.** Follow `docs/development.md`: run `gradle -p .\android testDebugUnitTest assembleDebug lintDebug` from the repository root in PowerShell. Then check every menu level and state on a device or emulator, applying the guide's compatibility rules rather than blindly matching mockups. Report missing physical-device checks explicitly.

## Assets

- `assets/android/res/`: 48 VectorDrawables (white, tint at the use site), tokens, launcher mipmaps, splash icon and theme
- `assets/icons/svg/dd_*.svg`: the same icons (24 px grid, 2 px stroke, `currentColor`); preview in `assets/icons/drawdock-icon-sheet.png`
- `assets/mockups/`: pixel references at 3× (1 dp = 3 px). `menu-00-anatomy.png` has the dimensions.
- `assets/tokens/drawdock-tokens.json`: platform-neutral tokens
- `assets/logo/`, `assets/app-icon/`, `assets/splash/`, `assets/github/`: brand imagery

## Changing the kit

Edit the source tables, never the generated files:

- Icons: `scripts/build_icons.py`
- Tokens: `scripts/build_tokens.py`
- Mockups: `scripts/render_mockups.py`

Then run them from the skill root in that order. They need `python3`; previews and mockups also need `pip install cairosvg pillow`. New icons must use the script's path helpers and match the 24 px grid, 2 px round stroke style.

`scripts/export_docs.py` exports the six documentation images from these masters.
Run it with `python` from any working directory; `--check` verifies without writing.
Application builds and normal documentation viewing do not require these tools.
