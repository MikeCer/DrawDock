# ADR 0001: Windows-first proof of concept

- Status: accepted for this initial PoC
- Date: 2026-11-04

## Decision

Implement a native Rust Windows host and a Kotlin Android client, in W0-W4 order
from `DrawDock-windows-first-poc-profile.md`. Keep shared scene and coordinate
logic independent from Windows APIs. Use a software rasterizer first and a
native layered window for the initial presenter. Use authenticated TLS/TCP;
USB goes through explicit ADB reverse mappings to loopback.

Linux adapters, GNOME integration, and QUIC/UDP are deferred. They are not
reported as supported and do not weaken Windows acceptance.

## Rationale

The primary acceptance target is real tablet pen input rendered over live
Windows applications and fullscreen presentations on the physical output.
Proving that output path first reduces the risk of investing in tablet preview
or video that never satisfies the host-visible annotation requirement.

## Consequences

The initial workspace is Windows-only and must fail clearly on other platforms.
A successful compile is not visual acceptance. Fullscreen stacking, monitor/DPI
alignment, physical pen dispatch, TLS pairing, pointer injection, capture/video,
Wi-Fi, and latency remain independent gates until tested.
