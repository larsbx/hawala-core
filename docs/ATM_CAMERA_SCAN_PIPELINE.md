# TEST camera packet → isolated QR worker → held signed evidence

This integration is a test harness, **not a production camera security
boundary or a financial application**. It exercises the code that would
consume one raw camera frame from the separately developed V4L2 test
helper, without giving that frame any financial authority.

## One-input scanner

`optical-camera-scan-test` is a Linux command taking one explicitly
selected, absolute, non-symlink path to the independently built
`optical-qr-decode` worker. It reads exactly one bounded `HWCAM001`
camera packet from stdin (up to `MAX_PACKET_BYTES + 1` to detect
oversize). The packet parser validates layout, dimensions, padding,
truncation and exact raw payload length BEFORE conversion.

The scanner converts to at most 1 MiB grayscale and invokes the existing
resource-limited decoder child (2-second wall/CPU limits, 256 MiB virtual
memory, 1 KiB child output file, fd/core limits). The output is only
untrusted QR **text**, at most 503 ASCII characters, returned locally
over stdout. Failed/blank/malformed packets cause nonzero exit status
with no text output. There are no credentials, custody data or financial
hooks in either worker.

A future isolated camera-acquisition process could pipe its packet:

```sh
./target/debug/optical-v4l2-test /dev/video0 640 480 /usr/bin/v4l2-ctl \
  | ./target/debug/optical-camera-scan-test \
      /absolute/path/to/optical-qr-decode
```

**The pipeline is NOT a complete transaction.** An authorized
application must still pass each frame of text to the existing
`CameraReceiver` under its independent monotonic time/budget, finish
all optical fragments, and have `TerminalLab` verify signed identities,
the separately pinned protocol authority and scope, replay restrictions,
and its durable local held state. Do not stream camera frames indefinitely:
the helper is single-shot and the consumer has a strict 32-image and
10-second assembly budget. Running it without operating-system isolation
or with live customer information is not approved.

## Verified test scenario

The integration test synthesizes YUYV422 captured buffers containing
actual three-fragment QR images for a signed test precommitment, challenge
and response. It executes the standalone scanner with the existing
isolated child decoder for each image, then imports the independently
verified recovered text through `CameraReceiver` and `TerminalLab`.

Accept/reopen/replay remains `AuthenticatedHeld` /
`IdenticalReplayHeld`. Invalid, truncated, trailing, oversized, identity
QR and malformed frames produce NO accepted text. The scanner only
recovers bytes; cryptographic evidence remains the responsibility of the
different, persistent controller.

## Non-goals and next tests

No physical camera was connected in CI. The Linux V4L2 capture test
helper is exercised separately with a fake operator tool. There is no
actual camera-model compatibility, untrusted sensor isolation, device
provisioning, energy/power-fail test or real ATM. A real pilot must
independently validate capture hardware/driver/fmt/stride, camera/display
distance, lighting, autofocus, motion blur and operator controls.

Most importantly, optical evidence is NOT spendable credit, actual
custody, a machine execution command, settlement, a human receipt, or
Iroh online identity authentication. Human/human and machine/machine
peers continue to share Hawala's underlying protocol, while this adapter
only advances the test optical machine interface.
