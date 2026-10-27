# Linux V4L2 single-frame capture — TEST PILOT ONLY

This is the first **optional physical-camera acquisition helper** in the
Hawala optical TEST stack. The helper can attempt to acquire exactly one
camera frame from a *locally authorized* Linux V4L2 device, via a separately
installed/pinned `v4l2-ctl` executable, and emit one bounded `HWCAM001`
packet on stdout. No camera was attached during hosted CI; only the
acquisition process logic was tested with fake command-line workers.

**This is not an ATM device driver, security sandbox, production camera
attestation, transaction authorization, cash-dispense capability, custody
proof, or verification of physical proximity.**

## Local operator invocation

On a nonproduction Linux host with a camera that supports raw packed YUYV,
`v4l2-ctl` installed, explicit physical-camera access granted, and no
customer financial secrets exposed:

```sh
cargo build --locked -p hawala-optical-media-test --bin optical-v4l2-test
./target/debug/optical-v4l2-test /dev/video0 640 480 /usr/bin/v4l2-ctl > one-frame.hwc
```

The helper requires exactly four arguments: a direct `/dev/videoN` character
device (symlinks and arbitrary devices rejected); bounded even width;
height 1..1024; and a locally trusted, absolute, non-symlink regular
`v4l2-ctl` executable. Check your installed package path and its
provenance before invoking it. Do not select either path based on a QR.

The process requests `--set-fmt-video=width=W,height=H,pixelformat=YUYV`,
`--stream-mmap=3`, `--stream-count=1`, and a private temporary output.
Only **exactly packed** `width × height × 2` bytes are supported by this
initial experiment. Drivers with padded image rows, different negotiated
formats, compressed cameras, or extra metadata fail closed. This is a
compatibility limitation, **not proof** that a driver honored the request.

One capture is bounded to five seconds (hard maximum ten seconds for
internal controlled tests). The helper process gets its own process group;
on failure or timeout it is killed and reaped rather than blindly retried.
The output is a versioned 28-byte HWCAM001 header followed by the raw
YUYV422 bytes. There are no names, account IDs, signing keys, timestamps,
QR decisions, or financial permissions in that packet.

The temporary raw camera file lives in a private directory but is NOT
encrypted, securely erased, or permitted for real customer recording.
Running this helper as root is unnecessary and discouraged. A future
deployment requires OS-enforced capture isolation, protected file systems,
device whitelisting and validated operators, immutable binary provenance,
global process memory/file/cpu quotas and strictly bounded acquisition IPC.

## Trust path

Untrusted camera frame -> bounded IPC packet -> bounded grayscale transform
-> isolated QR worker -> cryptographic envelope verification against
independently provisioned keys/scope/time -> durable **held** evidence only.

The financial authority and actual dispenser are deliberately NOT
connected. The optical protocol cannot resolve a live Iroh temporary QR
authorization while truly air-gapped; it uses its separate TEST domain and
only previously committed synthetic evidence. No offline double-spend
prevention, customer withdrawal, or asset custody is established.

## Verified and unverified

The new Rust unit tests use disposable shell scripts as fake
`v4l2-ctl` programs and test exact 16-byte YUYV capture, rejected
short/oversize/missing buffers, nonzero exit status, process timeout,
invalid camera paths and dimensions. Existing optical-media integration
tests still exercise the full signed QR exchange and journal replay using
camera-style packets.

**No physical V4L2 camera, frame geometry, negotiated stride, lens
behavior, motion blur, illumination or real-camera security has been
validated.** A hardware pilot must report actual device, driver, resolution,
format negotiation, operator isolation, optics and observable failure modes
before any compatibility claim.

This helper may be deleted or replaced if its nonproduction V4L2 calling
profile is not reliable. It is not a financial operations milestone.
