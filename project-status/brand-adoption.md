# Brand adoption milestones

## Milestone 1: foundations and documentation (2026-11-05)

**Committed as `d2a9a07`; owner approved proceeding to milestone 2.**

- Kept the supplied kit in the owner's ignored local `brand` workspace.
- Added `docs/branding.md` with component-owned asset destinations,
  runtime integration boundaries and the approved compatibility rules.
- Recorded compatibility overrides in the published guide so mockup dimensions
  cannot silently undo the thicker selection border or pen targets.
- Added a README hero and a theme-aware logo on the contributor index.
- Published selected menu/selection images as explicitly labeled design
  references in the branding guide, not screenshots of implemented UI.
- Added six optimized PNGs under `docs/assets/brand`. These are self-contained
  documentation assets; no original kit or image-generation tools are required.
- Local-only export tooling was used to resize/check the images against the
  unchanged masters; it is not a published contribution prerequisite.

The six documentation images total approximately 1.13 MiB. No Android/Rust
runtime resources, dependencies, application behavior, GitHub repository
settings or device installations changed. The GitHub social preview asset is
identified in the guide; the remote setting has not been changed.

Validation:

- All six exported images pass the read-only pixel/dimension check.
- Export failure-path checks reject missing/modified images, preserve logo
  transparency/aspect ratio, do not rewrite files in check mode, and leave
  all master PNGs unchanged.
- 71 local links/anchors across 18 README/docs/project-status Markdown files
  resolve against publishable files only, excluding the ignored workspace.
  Their code fences and tracked patch whitespace checks pass.
- All six published PNGs decode with the expected dimensions; both logo
  variants retain transparent backgrounds.
- Exported cover and dark logo inspected locally. Actual Markdown layout in
  GitHub/IDE previews and light/dark/narrow layouts remains owner review.
- Application builds/device tests were not run because no runtime files changed.

Owner review: preview root README and `docs/README.md` in light/dark themes,
then `docs/branding.md` at desktop and narrow widths. Confirm image readability,
mockup labeling and the approved compatibility rules before committing.
The owner explicitly keeps `brand` ignored. Commit the documentation assets and
guides, not the local kit. Subsequent Android milestones must check required
graphics/tokens into the Android component without referencing that folder.

## Milestone 2: Android radial menu (2026-11-05)

**Implementation and automated checks complete; owner pen/visual review and
commit pending. The owner subsequently requested milestone 3 styling in the
same pre-commit review batch.**

- Added 38 vector sources and menu-only typed color/dimension resources within
  Android. The APK has no dependency on the ignored design kit, and no new
  runtime dependency was added.
- Applied navy/cyan surfaces, labeled icons, Close/Back icons, native pressed,
  hover and keyboard-focus feedback, selected-tool discs, exact ink-color
  swatches/check marks and actual-width previews.
- Added active-family/current-color indicators, host-acknowledged Blackboard/
  Grid ON badges and accessibility states, target-mode icons and danger
  styling for Clear all.
- Preserved action order, dismissal, Clear confirmation, path actions and
  existing validation. Undo/Redo are not speculatively disabled.
- Kept a preferred 350 dp menu and at least 66 dp targets; targets grow with
  text. Small workspaces use the same actions in a scrollable list rather
  than shrinking text/targets. Transitions remain immediate.
- Left header/settings styling, canvas/selection rendering, host and protocol
  unchanged. No launcher or splash changes.

Validation:

| Result | Evidence |
|---|---|
| PASS | `testDebugUnitTest assembleDebug lintDebug`: 34 JVM tests, zero failures/errors; APK build and lint pass (existing warnings remain) |
| PASS | Three new geometry tests cover 2-8 item menus at multiple target sizes, all four viewport corners, nonoverlap, clockwise order and fallback decisions |
| PASS | All 38 imported vector XML trees match the supplied artwork; references resolve to Android-owned files |
| PASS | Final APK installed on Xiaomi 25099RP13G; all nine menu pages reachable, selected tool/color/width states exposed, exact seven swatch colors verified from device pixels |
| PASS | Synthetic stylus holds open the menu at the center and corners; moving stylus/finger holds do not; buttons stay inside the workspace without overlap; outside/Close/Back/action behavior and Clear confirmation pass |
| PASS | Remembered private-LAN TLS reconnect and screen readiness, acknowledged Blackboard/Grid toggles, unified Clear reset and both target Draw/Pointer mode states pass |
| PASS | With owner approval, 200% font scale and disabled animator tested on root/shapes/edit pages; labels and full-size targets remain reachable without overlap |
| PASS | Original font scale `1.1` and absent animator override restored and re-read; temporary host stopped; no firewall, trust enrollment or network profile changes |
| PARTIAL | Small-window fallback geometry is JVM-tested, but actual list scrolling, TalkBack speech, physical-pen hover/pressed ergonomics and hands-on owner visual review remain unverified |

The device test harness initially needed corrected selectors for uppercase
native dialog buttons and the hidden Disconnect control; focused reruns passed.
These were test-selector issues, not application failures.

Session-local screenshots, `Test-DrawDockBrandMenu.py` and JSON results record
the device evidence; they are not APK resources or published documentation
dependencies. Synthetic stylus checks are not a fresh claim of physical-pen,
fullscreen/projector or latency acceptance. Earlier Windows acceptance gaps
remain in the feasibility records; Linux and QUIC remain DEFERRED.

Owner review: exercise the installed radial menu with the real pen, especially
hover/press feedback, color/width choices and selected-state readability over
live applications. No commit was created automatically.

## Milestone 3: header, settings and dialogs (2026-11-05)

**Implemented at the owner's request before committing milestone 2. Final
APK installed; owner visual review and commit pending. Stop before milestone 4.**

- Replaced the light stock app theme with a native navy/cyan Views theme and
  shared rounded button, field, panel and dialog resources. No runtime
  dependencies were added.
- Styled the monitor dropdown and popup, Pen tools/Settings controls, connection
  panel and Connect/Disconnect action. Imported only four additional control
  vectors; all 42 icon sources are Android-owned and match the supplied artwork.
- Unified pairing chooser/manual fields, scanned-PC confirmation and text/
  Forget/Clear dialogs, with primary versus danger roles and button spacing.
  Added persistent manual-field labels without changing input types/defaults
  or validation.
- Preserved callbacks, remembered trust, certificate pinning, PC approval,
  source-readiness barriers, radial menu behavior and canvas/selection rendering.
  Native camera/OS permission dialogs are not overridden.

Validation:

| Result | Evidence |
|---|---|
| PASS | Final `testDebugUnitTest assembleDebug lintDebug`: 34 tests, zero failures/errors; build/lint pass (warnings remain) |
| PASS | Final APK installed; device pixel checks confirm navy/surface/cyan colors; labeled header/connection buttons have at least 48 dp targets |
| PASS | Settings hide/show, Pen tools, pairing chooser and all four accessible manual fields work; invalid port is still rejected without contacting a host |
| PASS | Forget, text and Clear dialogs display the shared styling and cancel correctly; saved trust was not deleted |
| PASS | With owner approval, 200% font sizing checked for nonoverlapping header controls, pairing choices and scrollable manual fields; original `1.1` restored and re-read |
| PASS | Remembered private-LAN reconnect, themed live-output dropdown, ready-gated switch to DISPLAY1 and Disconnect disabled-selector state pass |
| PASS | All nine radial pages and selected tool/color/width states rechecked after the app-wide theme change; exact seven ink swatches preserved |
| PASS | Resource XML audit, source-vector comparison and patch whitespace checks pass; temporary host stopped without network/firewall changes |
| PARTIAL | Fresh QR enrollment/scanned-PC confirmation was not exercised again; it uses the common dialog theme, but enrollment was not replaced or approved automatically |

Screenshots and `Test-DrawDockChrome.py`/JSON evidence remain session-local.
Physical pen/TalkBack review and previous fullscreen/projector acceptance gaps
remain owner checks. Launcher/adaptive-icon/system-splash artwork is deferred
to milestone 4. No automatic commit was made.

## Owner-gated milestone plan

| Milestone | Scope | Gate |
|---|---|---|
| 2 | Android Views radial-menu tokens, icons and visual states | Implemented and installed; stopped for owner tablet review/commit |
| 3 | Header/settings and dialogs; preserve canvas rendering | Owner requested adding it before the milestone 2 commit; implemented/installed, stopped for review |
| 4 | Adaptive launcher and API-compatible system splash | Owner approves milestone 3; illustrated onboarding remains optional, separate scope |

Do not roll these milestones into one change or create commits automatically.
