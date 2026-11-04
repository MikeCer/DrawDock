# DrawDock protocol status

The W2 host/client implement one authenticated TLS/TCP connection on port 47831
for pairing, control, pointer events and annotation commits. The host generates
an ephemeral TLS certificate and displays a QR invitation containing its
SHA-256 fingerprint and a single-use 128-bit token with a five-minute expiry.
The Android client pins the scanned certificate fingerprint before sending
the token; the host requires explicit approval in a native Windows dialog.
Manual entry and explicitly requested USB/ADB invitation delivery remain
available. No unverified-TLS bootstrap or short-code bearer authentication is
used.

QR invitations have the exact form
`drawdock://pair?v=1&host=<IP>&port=<port>&fingerprint=<64 hex>&token=<32 hex>`.
Only private/loopback numeric IP endpoints and the five specified unique fields
are accepted by the scanner. The host's expiry is authoritative; no synchronized
tablet clock is needed. The invitation is not a general-purpose deep link and
scanning does not grant remote input until the PC approves.

Frames are newline-delimited JSON and limited to 64 KiB. Large annotation
objects use ordered 48 KiB base64 chunks, a 512 KiB transaction limit and a
SHA-256 digest. The client queue and pending-stroke budget are bounded. Pointer
coordinates are normalized to the selected monitor; moves carry a generation
and may be coalesced, while button transitions are kept ordered. Android spaces
session frames by at least 12.5 ms (at most 80 frames/second), including chunked
commits and heartbeats, below the unchanged host guard of 120 messages/second.
The writer waits before dequeuing so new motion can replace the pending sample
even on a fast USB link. Down/up/cancel and mode/control barriers remain ordered;
one queue slot is reserved for release. Stalls do not accumulate catch-up credit.
A 1.5-second
read lease closes idle/disconnected sessions, releases any held host mouse
button and clears session annotations.

The single **Clear** control clears both tablet and PC annotations when connected.
It cancels active input, sends `clear_display` with a request ID, and pauses input
until `clear_ack` arrives before clearing the tablet preview. A five-second
timeout reports an unknown PC state rather than success. When disconnected,
Clear removes only local marks and explicitly reports that no PC is connected.
The host rebuilds every frame on a transparent surface so clear/undo remove old
pixels rather than drawing over the previous frame. Host tests cover this
redraw, but physical-display clearing still requires manual verification.

Freehand commits contain all samples; arrow, rectangle and ellipse commits
contain exactly the first and last drag endpoints, as required by host scene
validation. Android regression tests cover both drag directions.

The host defaults to loopback and accepts only explicitly selected private or
loopback addresses. Wi-Fi has not been tested; no wildcard/public listener,
firewall change, certificate bypass or automatic elevation is used. For the
current manual USB test, the loopback listener and ADB reverse port 47831 are
active; no session is yet authenticated. Stop the host and remove the mapping
after testing.

There is currently no video channel, screen capture, H.264 encoding or video
reconciliation. Ports 47832 and 47833 are not implemented or active. QUIC/UDP
is **DEFERRED**.
