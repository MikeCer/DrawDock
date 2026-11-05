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

**Owner committed milestones 2 and 3 as `84bbbd2` and approved milestone 4.
The owner requested milestone 3 styling in the same pre-commit review batch.**

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

**Implemented at the owner's request before committing milestone 2; owner
subsequently committed both as `84bbbd2` and approved milestone 4.**

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

## Milestone 4: adaptive launcher and system splash (2026-11-05)

**Committed by the owner as `aa523cb`. Rebuilt and device-verified after the PC
crash; API 30 runtime rendering remains unverified.**

- Added the adaptive icon to both manifest launcher declarations, using five
  original density PNGs and the supplied light background.
- The supplied foreground exceeded Android's 66 dp circular safe zone. A native
  8% inset on each edge keeps the entire artwork safe without altering source
  pixels, colors or proportions.
- Added the static transparent splash PNG and AndroidX `core-splashscreen`
  1.2.0 for API 30 compatibility and native API 31+ behavior. The starting theme
  hands off to the existing native app theme before constructing Views.
- API 33+ requests the icon rather than a plain-color starting window.
  No splash activity, timer, retention condition, connection wait, new artwork
  or onboarding was added.
- The six PNGs total 208,222 bytes. All graphics and XML live within Android;
  no build/runtime dependency on the ignored design kit was introduced.

Validation:

| Result | Evidence |
|---|---|
| PASS | Final `testDebugUnitTest assembleDebug lintDebug`: 34 JVM tests, zero failures/errors; APK build and lint pass with zero errors |
| PASS | All six imported PNGs are byte-identical to their sources; all five inset foreground variants fit the 66 dp safe circle |
| PASS | Actual tablet launcher tile displays the complete branded icon and opens the cold application |
| PASS | Screen recordings show the supplied icon on navy during both cold launch and warm activity creation, with a single splash sequence and handoff to the branded controls |
| PASS | Hot resume retains the app process, shows no new splash and returns to usable controls; no fatal exception found in the scoped application log |
| PASS | Header/panel colors, 48 dp controls, settings/Pen tools, pairing/manual labels and invalid-port rejection pass after the theme handoff |
| PASS | Forget, Text and Clear dialogs retain styling and cancellation; remembered pairing was not deleted or replaced |
| PASS | Resource XMLs parse, all six launch PNGs are packaged in the APK, and source/build files do not reference the ignored kit; patch whitespace passes |
| PARTIAL | No API 30 device or installed emulator image is available; compatibility integration builds, but API 30 runtime rendering is unverified |
| DEFERRED | Optional monochrome/themed launcher artwork was not supplied; lint has 59 warnings, including its new monochrome advisory and 58 existing warnings |

Initial recordings exposed the plain-color window on a shell-initiated warm
launch; the API 33 icon preference addresses that gap.
The device harness also needed the supported numeric NEW_TASK/CLEAR_TASK flags
instead of an unsupported shell option, and fade-aware pixel thresholds:
the native warm transition need not contain a fully opaque icon frame.
Final recordings were inspected and the corrected checks pass without adding
any app-side delay. These test details do not change app input, connection or
trust behavior.

Session-local `Test-DrawDockLaunch.py`, launch recordings, representative PNGs
and `brand-launch-results.json` preserve the evidence. The existing chrome
harness passed its basic phase on the final APK. Temporary tablet test files
and extracted scratch frames were removed; evidence remains outside the repo.

No system-wide settings, firewall rules or stored pairing were changed.
No automatic commit was made. Existing physical-pen/fullscreen/projector
acceptance gaps are unchanged; Linux and QUIC remain DEFERRED.

### Post-crash resumption (2026-11-05)

Confirmed that all four milestones survived in Git and the previous device
evidence remained intact. A fresh `testDebugUnitTest assembleDebug lintDebug`
completed successfully: 34 tests, zero failures/errors, zero lint errors and
59 warnings. Reinstalled the rebuilt APK without clearing application data.

On the connected API 36 tablet, repeated the actual launcher-tile cold start,
warm activity creation and hot resume. Both creation paths show a single branded
splash and usable app controls afterward; hot resume shows no new splash.
The scoped app log contains no fatal exception. Header/settings/Pen tools,
pairing/manual-field validation and cancellable Forget/Text/Clear dialogs pass.
Remembered pairing and tablet-wide settings were not changed.

No implementation repair was required. The API 30 runtime and optional
monochrome-artwork limitations remain as recorded above. Stopped for owner
visual review without creating a commit.

## Milestone 5: native Windows host launcher

The owner inserted this milestone before shared zoom. Build a native Windows
launcher with a normal window/taskbar icon plus a notification-area (tray) icon,
host controls and a readable live log viewer, using the DrawDock visual identity
and component-owned assets. The owner selected independent listener toggles:
USB/ADB and selected private LAN interfaces can run simultaneously.
Shared zoom already implemented below is now milestone 6; preserve that work.

Scope:

- Enable/disable USB via ADB reverse forwarding and explicitly selected private
  Ethernet/Wi-Fi interfaces. Ethernet and Wi-Fi use the same authenticated
  TLS/TCP transport; neither implies ADB-over-Wi-Fi or a new protocol. Toggling
  one endpoint must not inadvertently stop the other enabled listeners.
- Show available/selected ADB devices, eligible local interfaces/IPs, output
  monitor, port and encoder, reusing current validation and explicit error
  behavior rather than hardcoding this machine's defaults.
- Show listener/session state, start/stop results and connection failures.
  Keep pairing invitations and native PC approval usable without a terminal.
- Provide a bounded, scrollable live log view with clear source/severity and
  timestamps. Do not expose remembered credentials, invitation tokens or private
  key material in ordinary logs.
- Preserve the direct CLI/PowerShell entry points. No automatic elevation,
  firewall/profile changes, public/wildcard binds, certificate bypass, Windows
  startup registration or silently enabled listeners.
- Verify startup/shutdown, process/listener ownership, ADB mapping cleanup,
  unavailable devices/interfaces and real tablet connections. Changing/stopping
  a live session must make its drawing/session consequences explicit.

### Implementation and evidence

Implemented `drawdock-launcher.exe` in native Rust/Win32, as selected by the
owner instead of WPF. A coordinated `serve-managed` child now shares trust,
overlay and session ownership across independent listeners. Startup enables
nothing. Closing hides to the tray; explicit Quit stops owned services.
Configuration locks while the owned backend exists; Stop all releases it.
The UI embeds a host-owned multi-resolution icon and uses branded controls,
bounded/redacted live logs, native pairing and accessible labeled inputs.
Direct CLI and PowerShell entry points remain available.

| Result | Evidence |
|---|---|
| PASS | Rust workspace: all 55 tests passed with `--include-ignored --test-threads=1`, including native TLS video/monitor-switch and persistent-trust/revocation checks |
| PASS | Cargo build, rustfmt and Clippy with warnings denied |
| PASS | Native GUI creates/removes actual USB reverse forwarding and loopback TCP listeners; Stop all releases the backend/configuration |
| PASS | Ethernet and loopback endpoints enabled simultaneously; the actual remembered tablet authenticated over LAN and received NVENC video plus host-committed injected stylus ink |
| PASS | Disabling USB during that LAN session preserved its video, session and drawing |
| PASS | Native title-bar Close hides without stopping the owned host, USB mapping or reachable endpoint; relaunch restores the same window |
| PASS | Explicit Quit exits the owned launcher/host, removes its mapping and closes the TCP listener; a reentrant close-message bug discovered during UI testing was fixed and retested |
| PASS | Invalid port produces an explicit native error and log entry; launcher Pair opens/closes the native invitation without enrolling or changing production trust |
| PASS | Live logs follow the newest line, pause without blocking host controls, and resume with retained entries; deliberate local invalid-TLS probes generated the test errors |
| PASS | After the reported tray bug was fixed, the owner confirmed X-to-tray, single-click restoration, the right-click menu and non-resizable borders all pass |
| PARTIAL | Explorer-restart recovery and individual tray listener-checkbox actions were not separately verified |
| PARTIAL | Only one eligible Ethernet interface was available; physical Wi-Fi NIC/multiple-LAN-interface and profile-change runtime tests were not performed |
| PARTIAL | Real-device ink used injected input, not an owner's physical pen/fullscreen/projector acceptance |

Session-local evidence includes `launcher-tablet-results.json` and
`launcher-tablet-native-ink.png`. These are test artifacts, not bundled assets.
The Android source is unchanged in this milestone. No elevation, firewall,
network-profile, startup or certificate-policy changes were made.

The launcher deliberately supports one active tablet, not multi-user drawing.
Other clients can wait/time out while it is active. Abrupt process termination
can leave external ADB forwarding behind; normal shutdown ownership is tested.
Runtime taskbar/window/tray artwork is embedded; installer and executable-shell
icon resources are not included.

### Tray restoration and fixed-window follow-up

The owner reported a hidden launcher becoming unreachable after clicking the
tray icon. The original handler restored only on legacy double-click events.
It now explicitly negotiates version-4 notifications, decodes their packed
event word, and restores on single-click, double-click or keyboard selection.
Tray/lifecycle messages bypass keyboard-dialog filtering, and restoration and
native popup message pumping no longer hold a mutable UI-state borrow.
Failed tray updates recreate the icon; a registration failure leaves the
window visible with an explicit error. X and Hide use the same checked path.

The window no longer has a sizing frame or enabled Maximize action; tracking
bounds are fixed at its DPI-scaled size. Build, Clippy and all five launcher
tests pass, including the new notification-routing regression. Native
accessibility exposes Maximize as disabled. The owner tested the rebuilt
application and confirmed all requested tray and resizing checks pass.

**Implemented; stop for owner UI/UX testing and optional commit.**
Shared zoom was already owner-committed as `6039b56`; performance work and
the remaining mandatory physical Windows acceptance stay in milestone 6.

## Milestone 6: shared zoom, performance optimization and final UX acceptance

**PARTIAL: shared zoom is implemented, built and installed; automated device
checks PASS. Performance optimization and physical-pen/fullscreen/projector
and final accessibility acceptance remain outstanding. No commit was created.**

The owner approved shared desktop/annotation zoom, two-finger pinch/pan and
menu Zoom in / Zoom out / Reset, bounded to 100-400%. Reset is 100%;
monitor changes and disconnect reset zoom; Clear/Undo do not change it.

The native Windows presenter now owns a monitor-scoped Magnifier control,
including the original desktop/annotation overlay and excluding its own
presentation window. An isolated known-pattern feasibility probe passed 48/48
interior pixel checks before implementation. This is not tablet-only video
scaling, global Windows Magnifier configuration or an application zoom shortcut.
No elevation, UIAccess, accessibility settings, firewall or trust changes were
required.

The host acknowledges a pixel-quantized viewport, advances the source epoch and
restarts capture. Android waits for matching rendered video/readiness before
new pen input and coalesces gesture targets during transitions. Canonical scene
geometry is unchanged: new marks/widths, laser, Pointer and affine editing
coordinates pass through the inverse viewport. Navigation cancels on workspace
size changes. The Zoom submenu uses three new component-owned vectors following
the existing icon vocabulary; no suitable zoom artwork exists in the kit.

Testing exposed the previous 100 ms encoded-frame handoff as too short during
decoder recreation. Its bounded allowance is now 500 ms, still with one queued
frame and no arbitrary dropping of dependent H.264 frames. The ordered-handoff
test now includes a 150 ms consumer stall; stop/timeout checks remain.

### Automated Windows/API 36 results

| Check | Result and evidence |
|---|---|
| Rust | PASS: `cargo test --workspace`, 47 passed / 2 ignored; Clippy with `-D warnings`, host build and formatting pass |
| Android | PASS: `testDebugUnitTest assembleDebug lintDebug`; 38 JVM tests, zero failures/errors; lint zero errors / 62 warnings (three new status-string translation warnings follow existing inline-string usage) |
| Private-LAN TLS | PASS: actual remembered tablet session over `192.168.1.16` to `192.168.1.32`; ADB drives tests/installation only |
| Shared presentation | PASS: desktop GDI capture and actual tablet screenshots agree at 36/36 interior samples each for baseline, 125% menu zoom, pinch, pan and zoomed Blackboard/highlighter |
| Navigation isolation | PASS: injected two-finger gestures navigate the shared view; single-finger motion neither changes zoom nor pans |
| Zoomed pen/editing | PASS: injected stylus rectangle, rotated selection/reselection, fixed-anchor resize and enlarged pen handle target |
| Tablet Pointer | PASS: zoomed pen target maps to the expected underlying Windows desktop position, including negative monitor origin |
| Canvas/history | PASS on final installed APK: Blackboard remains enabled/physically black across zoom; new highlighter ink aligns; Clear/Undo preserve zoom and restore canvas/ink |
| Limits/menu | PASS: Zoom in reaches/stays at 400%, Zoom out decreases it, Reset removes magnification |
| Lifecycle | PASS: monitor switch resets crop, returning starts at 100%; disconnect removes the native magnifier; remembered reconnect starts at 100% |

An earlier Pointer assertion sampled the PC cursor before asynchronous network
delivery completed. Waiting for delivery fixes the harness; the final check
passes without changing production Pointer logic. Screenshots, recordings,
`zoom-probe-results.json`, `zoom-device-results.json`,
`zoom-scene-results.json` and selection results are session-local evidence,
not shipped runtime assets.
The final APK remains installed. Temporary tablet injector/XML files and local
class/dex scratch directories were removed. The test host is stopped, with no
listener, magnifier or known-pattern fixture window remaining.

### Performance optimization scope

The owner explicitly included performance optimization in milestone 6.
This is implementation work driven by measurements, not only a test/report phase.

- Establish repeatable USB and private-LAN baselines for pen-to-PC and
  pen-to-tablet latency, frame pacing, zoom/pan turnaround, CPU/GPU usage,
  memory and network throughput. Distinguish measured physical presentation
  latency from software timestamps/proxies.
- Profile capture, encoding, TLS queues, decoding, native magnifier refresh
  and rendering. Optimize demonstrated bottlenecks, especially decoder/capture
  recreation and repeated zoom transitions, without adding unbounded buffering.
- Improve pinch/pan responsiveness and video/ink handoff continuity. Address
  capture-to-scene revision coordination so provisional ink is not removed
  before its committed counterpart is present in the displayed frame.
- Preserve authenticated transport, bounded queues, source-epoch/readiness
  safety and coordinate alignment. Do not mask stalls by increasing timeouts,
  dropping dependent H.264 frames or substituting tablet-only magnification.
- Repeat equivalent before/after workloads and extended drawing/presentation
  soak tests, including fullscreen and repeated source/zoom changes. Record
  regressions, remaining limits and unavailable hardware honestly.

Agree measurable acceptance targets from the baseline before implementing
optimizations; do not invent achieved latency/FPS or claim that this newly
added work is already complete.

### Remaining acceptance and limitations

The device tests use injected stylus/touch input on the actual tablet and
captured Windows output. They do **not** substitute for the owner's physical pen,
natural multi-touch, direct monitor/projector observation or fullscreen
presentation acceptance. Live laser-hover ergonomics, mixed pen/palm sequences,
latency/soak, capture-revision handoff, rotation during a gesture, TalkBack and
small-window scrolling remain unverified. Zoom was tested over private LAN;
USB's unchanged TLS transport was not rerun for this milestone.

The physical PC mouse is intentionally not intercepted/remapped while zoomed;
use tablet Pointer mode or Reset to 100% before clicking controls with that
mouse. Gesture updates can have visible capture/decoder turnaround rather than
continuous local-only animation. Secure desktop, protected content and exclusive
fullscreen behavior retain existing Windows overlay/capture limitations.

API 30 runtime acceptance is BLOCKED by the absence of an API 30 device/image;
no system-wide installation was performed. Illustrated onboarding is deferred.
Linux/GNOME and QUIC remain DEFERRED. Stop here for owner testing and optional
commit; do not mark the mandatory physical Windows annotation acceptance PASS.

## Owner-gated milestone plan

| Milestone | Scope | Gate |
|---|---|---|
| 2 | Android Views radial-menu tokens, icons and visual states | Committed by owner as `84bbbd2` with milestone 3 |
| 3 | Header/settings and dialogs; preserve canvas rendering | Committed by owner as `84bbbd2`; owner approved milestone 4 |
| 4 | Adaptive launcher and API-compatible system splash | Owner committed `aa523cb`; post-crash rebuild/device checks pass on API 36; API 30 runtime unverified |
| 5 | Native Windows host launcher/window plus tray icon; independent simultaneous USB/private-LAN listeners and live logs | Implemented; automated/native/device checks PASS; tray/UX owner review and optional commit pending |
| 6 | Shared desktop/annotation zoom, performance optimization, latency/soak and final UX/accessibility acceptance | Zoom owner-committed as `6039b56`; PARTIAL: performance work and owner physical/fullscreen/accessibility acceptance outstanding |

Do not roll these milestones into one change or create commits automatically.
