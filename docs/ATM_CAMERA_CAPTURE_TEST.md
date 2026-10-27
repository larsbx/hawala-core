# TEST camera-buffer adapter for air-gapped Hawala evidence

**No physical camera, cash payout, wallet, custody controller, or authority.**

PR #35 established isolated QR raster decoding and held optical evidence.
This milestone models the boundary from an UNTRUSTED uncompressed camera
frame to the same grayscale QR decoder. No camera metadata is trusted.

## Format and resource contract

A borrowed CaptureFrame declares width, height, row stride, packed
pixel layout and an exact-length byte slice. Supported layouts:

| Layout | Input encoding | Output |
| --- | --- | --- |
| Gray8 | One luminance byte per pixel | Unchanged gray byte |
| Rgb888 | R,G,B triplets | Integer BT.601 gray (77R+150G+29B+128) / 256 truncated |
| Yuyv422 | Y0,U,Y1,V, even pixel width | Y bytes only |

Dimensions must be 1–1024. Stride is at least packed row size and at
most packed row size plus 256 padding bytes. The full data slice must
equal stride * height with no truncation or trailing bytes. Raw input
is bounded by 4 MiB and grayscale output by 1 MiB. Layout/size checks
precede allocation; there is no compressed stream, codec or frame queue.
Row padding is never interpreted as image content.

A TEST ImageReceiver accepts one converted raster, then calls the same
Linux resource-bounded isolated worker. Malformed captures permanently
poison the receiver. Readable/blank frames share the original maximum
32 observations, non-renewable ten-second window, independently supplied
trusted simulation clock, and cryptographic checks.

## Evidence boundary and tests

The tests feed full precommit/challenge/response signed optical exchanges
through all three layouts, padded rows, isolated decoding and terminal
journal verification. The journal can report only AuthenticatedHeld or
IdenticalReplayHeld, with no device execution or monetary state change.

Adversarial fixtures test malformed dimensions/stride, uneven YUYV width,
trailing or missing bytes, receiver poisoning and blank-buffer exhaustion.

**NOT yet implemented:** V4L2, UVC, CSI, actual security cameras, DMA,
video/compression, physical capture timestamps, lens focus, rolling shutter,
perspective, glare, motion blur, frame-drop/loss simulation, or device
provisioning. Physical presence and honest cameras cannot be inferred from
QR transport. Real ATM permissions and real customer funds remain disabled.

Next gate: isolated physical camera driver and reproducible image tests
under real display/camera geometries, while retaining an entirely separate
verified financial authority and permanent execution fence.
