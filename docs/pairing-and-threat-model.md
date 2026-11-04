# Pairing and threat-model status

The host creates a temporary TLS certificate and a cryptographically random
128-bit single-use token. A native Windows window displays a QR invitation
containing the endpoint, full SHA-256 certificate fingerprint and token.
Invitations expire after five minutes with at most three failed authentication
attempts. Closing the QR window cancels pairing; pairing success closes it.
Nothing is persisted by default, and the token/QR payload is not printed to logs.

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
invitation remains open and unexpired, grants annotation/pointer access.
Manual invitation entry remains available under **Manual invitation (advanced)**;
the existing explicit USB/ADB invitation-intent path remains compatible.

Treat the QR as a secret invitation: scan it only from your own PC, never share
its screenshot, and approve only the device you are pairing. QR replacement or
social engineering is outside cryptographic certificate pinning. Persistent
trusted-device storage and automatic reconnection are not implemented; restarting
the host creates a new identity and invitation. Closing/denying/expiry ends the
pairing listener; restart the host to try again. A modal approval dialog may
remain visible until dismissed after expiry, but expired approval cannot grant
access.

The listener rejects public/unspecified addresses and defaults to loopback.
An explicitly selected private IPv4 or local IPv6 bind is allowed, but Wi-Fi
has not been exercised. The implementation uses one TLS connection for
annotation/control/pointer messages; no video channel exists. JSON frames are
limited to 64 KiB, commits to 512 KiB, chunks to 48 KiB and traffic to 120
messages per second. A 1.5-second read lease releases held mouse buttons and
clears the session scene when the connection ends.

No certificate bypass, public listener, automatic elevation or firewall change
is used. A loopback QR endpoint requires an explicitly configured ADB reverse
mapping. For Wi-Fi, start the host on an explicitly selected private IP; Wi-Fi
acceptance remains unverified.

ADB authorization is not DrawDock authentication. The Xiaomi tablet is visible
and authorized to ADB, and the built APK was installed after explicit approval.
The optional `--pairing-info` file contains the same secret invitation in plaintext
for explicitly requested USB automation; use a private path and remove it after
delivery. Do not copy its contents into terminal history or logs. QR scanning
does not create that file. Physical camera-to-PC pairing and pen-to-host behavior
still need manual acceptance evidence.
