# Pairing and threat model

## Trust and capabilities

An approved tablet can view the selected monitor, annotate it and control the
Windows mouse. The claimed client name is a display label, not an authenticated
identity. The PC approval dialog names the granted capabilities and defaults
to **No**.

Control and video use pinned TLS. USB debugging authorization or access to a
private network alone does not grant DrawDock access.

## First pairing

The host retains its TLS identity and generates a random 128-bit single-use
invitation when enrollment is enabled. A native window displays:

```text
drawdock://pair?v=1&host=<numeric-IP>&port=<port>&fingerprint=<64-hex-SHA256>&token=<32-hex-token>
```

The Android scanner accepts only the version and five unique fields above,
with a private/loopback numeric IP, valid port, full pin and invitation token.
It rejects duplicate/unknown fields and does not resolve a DNS hostname.
ZXing scanning is bundled; no external scanner or pairing service is required.

The client pins the scanned certificate before sending the token. The invitation
expires after five minutes, permits at most three failed authentication attempts
and requires explicit local approval. Closing the QR window, denial or expiry
cannot approve a new tablet. An approval dialog left open past expiry cannot
grant access. Success closes the invitation.

Treat QR contents as a secret: scan only the PC you intend to use and do not
share QR screenshots. Pinning does not prevent an attacker from persuading a
user to scan the attacker's QR or approve the wrong device.

## Remembered devices

First approval enrolls a random 128-bit device ID and independent 256-bit
reconnect credential, delivered over the approved TLS connection. The host
stores its SHA-256 digest, not the credential itself. Resume proves possession
over the saved certificate pin without another QR/approval.

| Platform | Protected state |
|---|---|
| Windows | TLS certificate/private key and up to 32 device records in current-user DPAPI-encrypted `%LOCALAPPDATA%\DrawDock\trust.dpapi` |
| Android | Last PC endpoint, pin, device ID and reconnect credential in private preferences encrypted with Android Keystore AES-256-GCM |

Windows writes replace the encrypted registry atomically. A lifetime
`trust.lock` excludes concurrent hosts/device administration; stop the host
before listing/revoking devices. DPAPI calls disallow UI. Android uses a
non-exportable key, randomized IV, versioned envelope and authenticated purpose
string; app backups are disabled.

Unreadable/corrupt state or a missing key fails explicitly. Do not recover by
silently replacing identity, accepting a changed pin or falling back to
unverified TLS.

Disconnect/restart preserves trust but ends scene/input state. Android remembers
one PC endpoint; deliberate re-pairing is used for a changed endpoint/transport.
Forget PC removes the tablet copy, not a copied credential on another device.
Host revocation prevents future resumes while preserving identity/other devices.
See [host administration](run-windows.md#remembered-devices).

Enrollment is enabled when the registry is empty or explicitly requested with
`--pair`/launcher `-Pair` (also by an explicit `--pairing-info` request).
Closing enrollment does not disable remembered-device reconnects. Failed
authentication is additionally bounded to three attempts per ten-second window.

## Transport and session boundaries

The host defaults to loopback and rejects wildcard/public bind addresses.
Private/local addresses must be selected explicitly. It does not elevate or
change firewall rules. A loopback endpoint requires an explicit ADB reverse
mapping for USB; private-network access requires the operator's network policy.

After control approval, a fresh 128-bit session credential authorizes a separate
video connection on the same TCP port. It has a 30-second acceptance window,
three-attempt limit and bounded handshake/write deadlines. It cannot grant
control access, and capture starts only after video authentication.

Control framing, transactions, scene/history storage, image decoding and traffic
are bounded. A 1.5-second read lease releases held buttons and clears session
state on loss of control. Mode/source/history/Clear transitions release input;
blackboard disallows mouse injection into the concealed desktop.
Source epochs reject queued input for outdated geometry.
See [protocol limits](protocol.md#resource-limits).

## Content and local automation

Text is plain text escaped before generated SVG rendering. The renderer does
not resolve arbitrary image paths; images are bounded, validated PNG data.
Tablet clipboard access happens only after Paste, with provider permissions
and explicit error reporting, not background polling or URL fetching.

Manual invitation entry and ADB intent delivery use the same pin/token/approval
checks. `--pairing-info` creates a new plaintext invitation file only at the
requested path. Use a private path, avoid logging/history disclosure and remove
that specific file after delivery. QR scanning does not create it.

Never commit trust stores, tokens, private keys or QR payloads. Do not use
production trust in integration tests or bypass camera/install/OS approval UI.

## Threat boundaries

DPAPI protects against another Windows account/offline file disclosure, not
malware running as the same Windows user. Android Keystore storage does not
imply a hardware-backed or StrongBox key on every device. A compromised,
already-approved tablet can exercise its granted capabilities until access is
revoked and the active session is ended.

Capture exposes the selected desktop contents to that tablet. Use an appropriate
display and avoid sensitive captures/logs. Windows secure desktops, protected
content and higher-integrity application boundaries are not bypassed by the
ordinary-user presenter/input process.
