# Pairing and threat-model status

## Remembered-device design

At the owner's request, persistent trust replaces the original ephemeral-only
milestone policy. First pairing still requires the QR invitation, certificate
pin check and explicit PC approval. Approval now enrolls a random device ID and
a separate 256-bit reconnect credential, delivered only over that approved TLS
connection. The host stores only its SHA-256 digest; subsequent connections
prove possession over pinned TLS without another approval prompt.

The host TLS certificate/private key and device registry are protected with
Windows current-user DPAPI (`CryptProtectData`, UI forbidden), under
`%LOCALAPPDATA%\DrawDock`. Writes replace the encrypted registry atomically.
An exclusive state lock prevents concurrent hosts or revocation racing an active
session; stop the host before managing remembered devices. Android stores the
last PC endpoint, certificate pin and reconnect credential as AES-256-GCM
ciphertext in private preferences, using a non-exportable Android Keystore key.
Neither platform stores remembered credentials in plaintext; invitation tokens
are not retained in either trust store.
An unreadable/invalid store fails explicitly, never silently replacing trust.

Disconnecting or restarting either app does not revoke trust. Host device
revocation rejects subsequent resumes; Android forgetting removes its local
credential. New enrollment still needs an invitation (`--pair` / launcher
`-Pair` when trusted devices already exist). TLS pin changes require deliberate
re-pairing, not automatic trust-on-first-use. Sessions remain single-tablet,
with bounded authentication attempts and the existing lease/input cleanup.

The host retains its DPAPI-protected TLS certificate and creates a cryptographically
random 128-bit single-use invitation token when enrollment is enabled. A native
Windows window displays a QR invitation
containing the endpoint, full SHA-256 certificate fingerprint and token.
Invitations expire after five minutes with at most three failed authentication
attempts. Closing the QR window cancels pairing; pairing success closes it.
Only the approved device credential and host identity are retained as described
above; the invitation token/QR payload is not printed to logs.

On the tablet, choose **Connect > Scan QR**, allow camera access if prompted,
and scan the PC's window. The decoder is bundled (ZXing Android Embedded 4.3.0,
Apache-2.0); it does not need a third-party scanner, Play Services download or
an external pairing service. Confirm the displayed endpoint. The app rejects
malformed/duplicate fields, unsupported versions, missing credentials and
non-private/non-loopback IPs before connecting. DNS is not used when parsing QR
invitations. The Android trust manager accepts only a certificate matching the
scanned fingerprint, before sending the invitation token. No trust-on-first-use
or certificate-validation bypass is introduced.

After validating the invitation token, the PC shows a native **approve tablet**
dialog with the claimed device name and peer endpoint. The name is not an
authenticated identity. The default is **No**. Only explicit **Yes**, while the
invitation remains open and unexpired, grants screen-viewing, annotation and
pointer access. The approval dialog names these capabilities.
Manual invitation entry remains available under **Manual invitation (advanced)**;
the existing explicit USB/ADB invitation-intent path remains compatible.

Treat the QR as a secret invitation: scan it only from your own PC, never share
its screenshot, and approve only the device you are pairing. QR replacement or
social engineering is outside cryptographic certificate pinning. Tap **Connect**
to reuse the last PC's encrypted credential. The host retains its TLS identity
across restarts and keeps listening after disconnects. Closing/denying/expiry
disables new enrollment, not remembered-device connections; restart with `--pair`
to issue another invitation. A modal approval dialog may
remain visible until dismissed after expiry, but expired approval cannot grant
access.

The listener rejects public/unspecified addresses and defaults to loopback.
An explicitly selected private IPv4 or local IPv6 bind is allowed, but Wi-Fi
has not been exercised. The implementation uses one TLS connection for
annotation/control/pointer messages. A separate pinned-TLS connection to the same
port authenticates with a fresh 128-bit video credential delivered only inside
the approved session. It cannot grant annotation/control access; wrong credentials
are rejected before capture starts. Video authentication has a 30-second window,
three-attempt limit and bounded handshake/write deadlines. The capture process
and video service stop with the session. JSON frames are
limited to 64 KiB, commits to 512 KiB, chunks to 48 KiB and traffic to 120
messages per second. A 1.5-second read lease releases held mouse buttons and
clears the session scene when the connection ends.

No certificate bypass, public listener, automatic elevation or firewall change
is used. A loopback QR endpoint requires an explicitly configured ADB reverse
mapping. For Wi-Fi, start the host on an explicitly selected private IP; Wi-Fi
acceptance remains unverified.

The host limits failed authentication to three attempts per ten-second window,
in addition to the three-attempt/five-minute invitation limits. Corrupted or
unreadable protected state is an error, never a reason to silently reset identity.
DPAPI protects against another Windows account/offline file disclosure, not
malware already running as this Windows user. No hardware-backed/StrongBox
guarantee is claimed for the Android Keystore key.

ADB authorization is not DrawDock authentication. The Xiaomi tablet is visible
and authorized to ADB, and the built APK was installed after explicit approval.
The optional `--pairing-info` file contains the same secret invitation in plaintext
for explicitly requested USB automation; use a private path and remove it after
delivery. Do not copy its contents into terminal history or logs. QR scanning
does not create that file. Physical camera-to-PC pairing and pen-to-host behavior
still need manual acceptance evidence.
