# ADR 0001: Native Windows host and shared scene model

Date: 2026-11-04

## Decision

Use a native Rust Windows host and Kotlin Android client. Keep shared scene
validation, history and normalized geometry in `dd-scene`, separate from Windows
presentation and input APIs.

The presenter is a software-rasterized, topmost layered window with click-through
and no-activate behavior. Use authenticated TLS/TCP for control and video, and
explicit ADB reverse mappings to loopback for USB transport.

The host binary is Windows-specific; portability is expressed through the
platform-independent scene boundary, not speculative adapter implementations.

## Rationale

Annotations belong on the selected physical PC output over the application being
presented. A tablet-local canvas or mouse-only path does not satisfy this
interaction model.

Native Windows presentation and `SendInput` keep desktop behavior explicit and
do not require WSL, Docker or elevated execution. Shared normalized geometry
keeps client layout independent of output resolution while allowing physical
aspect/DPI corrections at the platform boundary.

## Consequences

The host fails compilation on non-Windows platforms. Windows-specific rendering,
output enumeration and mouse injection stay outside the shared crate.

Source changes must coordinate overlay placement, scene selection, input release
and capture geometry. Physical-output, fullscreen, pen and transport checks are
independent test dimensions; compilation is not a substitute for visual/input
testing.

Linux adapters and QUIC are deferred. The
[architecture guide](../architecture.md) describes the component implementation.
