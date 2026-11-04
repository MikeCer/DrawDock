# DrawDock — Windows-First PoC Execution Profile

Document ID: DD-WIN-POC-001 | Version: 0.1 | Date: 2026-11-04
Status: user-selected initial implementation platform; engineering details remain proposed.

## 1. Scope and precedence

The initial DrawDock proof of concept MUST be developed and tested on the owner's current Windows system, with Windows 11 as the target specified in the project requirements. Verify and record the actual Windows version during environment discovery.

This execution profile overrides the initial milestone order and cross-platform completion gates in `DrawDock-feasibility-agent-instructions.md`. It does not remove the project's eventual Linux X11/Wayland requirements.

Read together:

1. `DrawDock-rust-overlay-architecture.md` — shared architecture and Windows adapter.
2. `DrawDock-android-stylus-event-spec.md` — Android input behavior.
3. `DrawDock-feasibility-agent-instructions.md` — general engineering/safety rules.
4. This document — Windows-first scope and acceptance.

Host-visible annotations remain mandatory. A Windows PoC can pass without Linux implementation, provided its verdict is explicitly Windows-only. Linux must be marked DEFERRED, not PASS or BLOCKED solely because no Linux machine is currently available.

## 2. Initial scope

MUST implement:

- Rust host application, Windows native overlay, local status and emergency stop.
- Kotlin Android application with real stylus input and immediate preview.
- Host-visible freehand, arrow, rectangle and ellipse/circle; colors, widths, undo/redo and clear.
- Explicit Draw/Pointer modes; drawing must not click the underlying application.
- USB/ADB and Wi-Fi authenticated TLS/TCP communication.
- Actual Windows mouse pointing, click, drag and right-click.
- Selected-monitor mirroring to Android, aspect-preserving mapping, and monitor switching when multiple monitors are available.
- Correct cancellation, reconnect, stop and annotation/video reconciliation.
- Fullscreen presentation validation on the owner's actual presentation/demo application.

DEFER:

- X11, Wayland, libei, portal/PipeWire integration and GNOME extension implementation.
- GNOME JavaScript exception approval; it is not needed to start Windows work.
- QUIC/UDP implementation and comparison until core Windows feasibility is demonstrated.
- Linux build packaging, cross-compilation, installers and production distribution.

Do not introduce placeholder Linux implementations that report success. Keep shared scene, geometry, protocol and session logic separate from Windows APIs. One real Windows adapter is sufficient; do not spend the PoC building untested generic abstractions for hypothetical Linux behavior.

## 3. Windows-only implementation milestones

### W0 — discover and prepare

Inspect the existing repository, Windows version, architecture, GPU/driver, monitors and DPI settings. Check Rust toolchain, Windows build prerequisites, JDK/Android SDK, Gradle and ADB. Record actual Android tablet/API/stylus availability. Do not assume tools or devices exist; supply exact setup steps for missing prerequisites.

Use PowerShell commands for Windows host instructions. Do not require WSL, a Linux VM or Docker to run the native Windows desktop app. Existing remote/CI builds may supplement but cannot prove interactive desktop functionality.

Deliver environment report, pinned dependencies, workspace skeleton, unit-test commands and a Windows-only implementation plan. Do not make privileged changes, install drivers, alter firewall rules or expose network listeners without explicit owner action.

### W1 — prove native Windows overlays first

Implement the shared scene/geometry reducer and a real Windows presenter. Use a transparent, non-activating, click-through layered window on the selected monitor, as specified in DD-ARCH-001. Begin with a raster renderer rather than speculative GPU optimization.

Provide a runnable overlay demo and fixture replay. Draw all required shapes over the original live desktop/application. Verify:

- Ink appears on the physical selected display, not only inside the host UI.
- Underlying application continues animating and updating.
- Clicking painted ink still reaches the underlying application.
- Keyboard navigation and application focus are preserved.
- Ink remains visible in the actual fullscreen presentation mode.
- Clear and stop remove the overlay cleanly.
- Monitor position/DPI changes do not misplace the overlay.

A local synthetic scene is acceptable only for this presenter gate. It does not count as Android or end-to-end success. Report and investigate fullscreen failures before proceeding deeply into video work.

### W2 — real tablet pen over USB first

Implement authenticated TLS/TCP, the Android input reducer and tool controls. Follow the pairing rules in DD-AGENT-001. Start host listeners on loopback and connect through explicitly selected ADB reverse mappings. Test with tablet Wi-Fi disabled.

An explicitly labeled no-video drawing pad is acceptable at this intermediate stage. Prove actual tablet pen strokes produce host-visible annotations. Validate cancellation, long strokes, undo/redo, colors/widths, palm/finger exclusion and mode transitions.

Keep communication authenticated even over loopback/ADB. Do not disable certificate validation for convenience. Record the actual tablet model, API, pen capabilities and connection details.

### W3 — mouse mode and live monitor mirroring

Implement Windows pointer injection and selected-monitor capture. Add bounded H.264 encoding, Android decoding, source geometry and stream epochs.

Verify hover where supported, click, right-click and drag. Pull the cable mid-drag and prove the host releases held buttons. Do not automatically elevate to control higher-integrity applications.

Fit the selected source into the tablet viewport without distortion. Validate corners, center, letterboxing, DPI and monitor selection. Source changes must release buttons, cancel previews and reject old geometry.

Complete the included/excluded annotation capture contract from DD-ARCH-001. The tablet must not show indefinite double ink or recursive capture; the physical host overlay must not flicker or disappear to simplify capture. Local preview latency cannot stand in for measured host response.

### W4 — add Wi-Fi and complete evidence

Enable an explicitly selected private-network interface, using the same authenticated protocol and session model. No wildcard/public binding or automatic firewall changes by default. Provide owner-run firewall instructions if required.

Repeat real pen, mouse, cancellation and video tests with USB unplugged. Compare actual USB/Wi-Fi behavior before optimizing. Run a 30-minute mixed workload and record queue age, dropped frames, memory trend and failure recovery.

A lack of a second physical monitor or camera must be recorded as NOT RUN, with an owner-run procedure. Do not claim those tests passed. Multi-monitor acceptance remains open until performed.

## 4. Windows PoC acceptance

Use these verdicts per requirement: PASS, FAIL, NOT RUN, DEFERRED. Windows-only overall verdict is PASS only when all mandatory Windows tests have real evidence.

| Gate | Required evidence |
|---|---|
| Host-visible ink | Real pen creates every required shape on the physical Windows output |
| Live presentation | Ink over an actual fullscreen presentation; focus/navigation preserved |
| Tools | Colors, widths, undo/redo and clear demonstrated |
| Mode isolation | Draw sends no underlying mouse clicks; Pointer works correctly |
| USB | Wi-Fi disabled; ADB transport verified |
| Wi-Fi | Cable removed; authenticated wireless session verified |
| Mirroring | Live selected monitor visible on Android; fitted geometry correct |
| Multi-monitor | Actual source switching, source epochs and alignment validated |
| Reconciliation | No persistent double ink, flicker or feedback loop |
| Recovery | Cable pull, app background, reconnect and emergency stop safe |
| Performance | Measured metrics or explicit NOT MEASURED; no invented numbers |

A PARTIAL verdict is appropriate when core Windows features work but required tests remain unperformed. Linux and UDP remain future phases, not reasons to withhold a truthful Windows-only feasibility result.

Deliver a Windows run guide, Android/ADB setup guide, buildable source/APK where possible, capability report, actual test results, known limitations and a short next-step recommendation.

## 5. Copy/paste coding-agent prompt

```text
Implement the initial DrawDock PoC on my current Windows system.

Read:
- specs/DrawDock-rust-overlay-architecture.md
- specs/DrawDock-android-stylus-event-spec.md
- specs/DrawDock-feasibility-agent-instructions.md
- specs/DrawDock-windows-first-poc-profile.md

The Windows-first profile takes precedence for implementation scope and order.
Start with W0 and proceed through W1-W4. Build a native Rust Windows host and
Kotlin Android client. Do not require WSL, Linux or Docker for the desktop app.
Defer Linux adapters, GNOME integration and QUIC; preserve shared architecture
without building speculative abstractions.

The central acceptance requirement is real tablet pen input drawing graphical
elements over live Windows applications and fullscreen presentations, visible
on the physical PC monitor/projector. Tablet-only ink or mouse-only control is
not success.

Prove the native Windows overlay first, connect actual pen input through USB/ADB
second, then add mouse mode and live monitor video, and finally validate Wi-Fi.
Use authenticated TLS/TCP. No certificate bypass, automatic elevation, silent
firewall changes or public network listeners.

Inspect my actual environment before choosing setup steps. Provide PowerShell
build/run/test commands and ask before system-wide modifications. Run available
tests, preserve existing work and record missing hardware honestly. Compilation
alone is not proof of real desktop, tablet or fullscreen behavior.

Report Windows-only results as PASS, PARTIAL or BLOCKED with evidence. Mark Linux
and QUIC DEFERRED. Do not weaken mandatory Windows annotation requirements.
```

## 6. Source and design boundaries

This profile changes implementation scope and order; platform mechanics remain grounded in the reference registers of DD-ARCH-001 and DD-ANDROID-001. In particular, the Windows overlay design follows Microsoft's layered-window documentation:
https://learn.microsoft.com/en-us/windows/win32/winmsg/window-features

Milestone order, acceptance wording and scope deferrals are DrawDock project decisions, not external technical findings.
