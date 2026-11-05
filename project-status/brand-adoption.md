# Brand adoption milestones

## Milestone 1: foundations and documentation (2026-11-05)

**Implementation complete; owner visual review and commit pending.**
Stop here before making Android UI changes.

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

## Subsequent owner-gated milestones

| Milestone | Scope | Gate |
|---|---|---|
| 2 | Android Views radial-menu tokens, icons and visual states | Owner approves milestone 1; stop again for tablet testing/commit |
| 3 | Header/settings and compatible canvas chrome | Owner approves milestone 2; retain transformed geometry, pen targets and host rendering |
| 4 | Adaptive launcher and API-compatible system splash | Owner approves milestone 3; illustrated onboarding remains optional, separate scope |

Do not roll these milestones into one change or create commits automatically.
