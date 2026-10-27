# TEST-only bounded local camera-acquisition packet

**No real camera, financial permit, custody, ATM actuator, protected account
data, settlement authority or identity credential is introduced.**

The previous PR #36 accepts untrusted Gray8, RGB888 and YUYV422 camera-like
buffers. This milestone adds a **versioned exact-length local packet** that a
future independently isolated camera capture process could submit across
a narrow IPC/file boundary. It is not a V4L2/UVC implementation.

## Packet layout (28-byte fixed header + raw packed pixels)

| Offset | Bytes | Content |
| ---: | ---: | --- |
| 0 | 8 | ASCII `HWCAM001` |
| 8 | 1 | Pixel layout: 1 Gray8, 2 RGB888, 3 YUYV422 |
| 9 | 3 | Reserved/flags, MUST be zero |
| 12 | 4 | Width, big-endian unsigned |
| 16 | 4 | Height, big-endian unsigned |
| 20 | 4 | Full packed row stride, big-endian |
| 24 | 4 | Exact raw pixel-buffer length, big-endian |
| 28 | declared length | Raw pixels, including bounded row padding |

There is no sender identity, trusted timestamp, device authorizer,
financial claim, card, phone number, bank account, transaction identifier,
or implicit money-permission bit in this format. A locally acquired packet
is **untrusted** even if its producer lives on the same machine. The camera
may be compromised or its pixels replayed.

Dimensions: 1..1024 each. For YUYV422, width must be even. Row stride
must be between width multiplied by its packed bytes-per-pixel and that
number +256. Payload length must equal stride times height **exactly**,
with no truncation or trailing bytes. Entire raw payload <=4 MiB.
The parser checks all bounds, layouts, reserved fields and integer arithmetic
before returning a borrowed frame; it does not allocate a raster or call a
decoder until the caller requests that separately.

## Acquisition-to-optical trust flow

A potential isolated acquisition helper obtains raw camera data through
an as-yet-unimplemented *physical* driver. It must deliver exactly one
bounded packet under an independently enforced process deadline and
per-device rate/byte quota.

The TEST `ImageReceiver::receive_packet` checks its poisoned/clock/quota
state **before** parsing/allocating. It converts the packet into bounded
grayscale and invokes the existing isolated QR decode worker under CPU/wall,
output, address and file-descriptor budgets.

The decoded text enters the unchanged signed air-gap protocol; the
terminal requires independently pinned authority, full QR identity and
adjacent readable name/code, exact transaction scope, trusted local time,
signed challenge/response and durable replay fencing. The only accepted
result is *held evidence* (never disbursement or custody finality).

## Verification and remaining obligations

Tests demonstrate all three packed layouts through the entire signed
precommit/challenge/response optical exchange, journal restart and
identical replay. They also challenge malformed layout/stride/dimension/
length, short/trailing bytes, poison persistence, clock deadline and
one-fragment incompleteness. A synthetic bounded file-spool test is only
IPC rehearsal, not hardware or a security sandbox.

There is still **no** OS camera driver, V4L2/UVC/CSI negotiation, capture
device isolation, sensor authenticated provenance, monotonic hardware clock,
physical field testing, video compression, genuine security-camera
integration, customer funds or cash device. A real pilot must establish
all of these controls independently and must not treat a QR video channel
as an ATM dispense authorization.
