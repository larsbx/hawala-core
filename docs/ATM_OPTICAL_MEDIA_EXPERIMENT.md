# TEST optical raster transport experiment

This extends [the optical evidence protocol](ATM_OPTICAL_TEST_PROTOCOL.md) from
QR text to actual generated grayscale QR images and simulated video observations.
It is an experiment, not a camera deployment, financial controller or authorization
upgrade. Canonical 533-byte signed envelopes, fragmentation (503, 503, 417 text
characters), signature domains, independent transaction scope and journal semantics
are unchanged. No cash command, custody permission, offline double-spend assurance,
settlement authority or Iroh authentication shortcut is created.

## Boundary and resource contract

`hawala-optical-media-test` is a separate Linux test crate. Its normal dependency
graph contains QR rendering/detection and process utilities, **no Hawala identity,
protocol, journal, financial controller or network library**. The disposable
`optical-qr-decode` worker receives only a bounded raw grayscale image. It never
receives a key, database, expected scope, trusted clock or authorization capability.
The adapter in `tests/support/mod.rs` is shared with the reproducible experiment;
it sends untrusted text through the existing `CameraReceiver`, then callers use
`respond`, `TerminalLab::import/accept/observe` with independently pinned identities,
authority, scope and trusted time. Transport completion proves authorship of a
canonical envelope only. The durable journal decides its scoped TEST admissibility.

The renderer fixes QR version 17, M error correction, four white quiet-zone modules
and integer nearest-neighbor module scaling 1–8. Both encoder and decoder versions
and transitive dependencies are recorded in the workspace `Cargo.lock`. The 85×85
symbol plus quiet zone occupies 93×93 through 744×744 pixels. Grayscale PGM fixtures
are actual images, not mocks that recover text from filenames or metadata. The
worker independently detects QR grids and corrects/decodes QR modules with `rqrr`.

The isolated parent adapter applies these limits before executing the worker:

| Resource | Limit |
| --- | ---: |
| Width and height | 1–1024 each |
| Uncompressed pixel bytes | 1,048,576 |
| Header | 16 bytes, fixed binary layout |
| Worker address space | 256 MiB |
| Worker CPU | 2 seconds |
| Worker elapsed wall time | 2 seconds, then kill and reap |
| Worker output file | 1024 bytes |
| Accepted output | one status byte plus at most 503 ASCII bytes |
| Open file descriptors | 32 |
| Core dump bytes | 0 |
| Image observations per receiver | 32, including blanks and unreadable images |
| Assembly window | original non-renewable 10 seconds |
| Retained transport payload | original three chunks, 533 bytes total |

Dimensions are checked before pixel allocation; truncation and trailing bytes
reject. There is no compressed image/video codec, unbounded frame queue, streaming
input or network camera driver. Anonymous input/output files avoid pipe deadlock
and are closed on return. Parent output reads are capped. All worker failures,
malformed replies, adapter clock regressions, limits and detected ambiguity
permanently poison that assembly. Zero detected grids or one unreadable grid
produce no text and consume an observation via `CameraReceiver::observe_no_text`.
Detected multiple grids reject even if they decode identically. This is a rule
about **detected candidates**, not proof that every visually ambiguous scene can
be recognized. Decoder omissions or erroneous text cannot bypass the canonical
parser, digest, signature or independently pinned transaction checks.

The adapter checks supplied trusted simulation time and monotonic elapsed time
before and after decoding. Decoder work and empty images cannot renew the window.
The media schedule is test input, not a trusted video timestamp. Future hardware
must obtain time independently of camera metadata and apply host-wide quotas.

This is a resource-bounded child process, **not a hostile-code security sandbox**:
there is no syscall, filesystem or network namespace isolation. The child inherits
ordinary host process privileges; real deployment needs an OS isolation policy,
protected keys/store, provisioning, trustworthy time and independent authority.
The worker executable must be the independently built/pinned program, not selected
by a QR image. The harness uses known public test seeds only.

## Reproduce and inspect receipts

On Linux, from the repository root:

```sh
cargo test --locked -p hawala-optical-media-test --all-targets
cargo clippy --locked -p hawala-optical-media-test -p hawala-optical-test --all-targets -- -D warnings
python3 tools/check_optical_media.py
cargo test --locked --workspace --all-targets
# Run the existing formal/README.md Lean/Quint safety gates as well.
```

The script builds the release worker and experiment, reproduces and byte-compares
all three committed `fixtures/precommit-*.pgm` images, then writes
`target/optical-media/receipt.json`. Deliberate fixture updates use
`--update-fixtures` and require review. The receipt contains per-observation exact
decode outcomes, child CPU/wall time and Linux `wait4` peak RSS; every synthetic
video schedule and outcome; durable held/replay/conflict results; fixture BLAKE3
hashes; worker SHA-256; toolchain/platform; HEAD and dirty status; SHA-256 hashes
of every repository source input and a canonical manifest digest. The reference
receipt excludes only itself to avoid a self-hash cycle. HEAD may name the parent
for a dirty local reference: its explicit content manifest, not an implied clean
commit, identifies the exact tested inputs. Fresh CI receipts bind the actual
checkout SHA and manifest. Timing/RSS and executable hashes can vary across hosts.
`wait4` records each child separately, not parent/build/host-wide memory.

The optical workflow uploads images and the fresh receipt as
`optical-media-<checkout SHA>`. The existing Rust, Lean, Quint and vector workflows
remain mandatory. The optical experiment does not claim a new formal refinement
proof for QR detection or physical cameras.

## Measured synthetic conditions and regressions

The reference experiment uses one deterministic signed precommitment split into
three QR images. Each trial is a fresh child, without hidden retry or preprocessing.
The complete commitment/challenge/response exchange additionally renders and
recovers all nine QR frames before journal consumption. Reopened replay returns
`IdenticalReplayHeld`; two contradictory terminal-signed reports remain retained,
with final state `consumed_held` and no financial effect.

| Condition | Exact decodes / trials in reference run |
| --- | ---: |
| Clean black/white, scales 1, 2, 4, 8; rotations 0°, 90°, 180°, 270° | 48 / 48 |
| Scale 4 centered on 800×600 gray display, dark=32, light=224, background=160 | 3 / 3 |
| Scale 4, integer 3×3 box blur (unchanged outer border) | 0 / 3 |
| Scale 4, right half replaced with white | 0 / 3 |

These are exact fixture outcomes, not probability estimates, lighting tolerances
or general claims about QR performance. Blur is a demonstrated limitation of this
renderer/decoder combination under that precise transform. Neither unreadable
condition completes an exchange. The receipt records resource measurements for
all 57 trials; limits above are enforced regardless of these measured values.

Adversarial tests separately cover three-frame bidirectional exchanges, byte-exact
text recovery under clean scaling/rotation/display, blur loss, repeated/reordered
observations, dropped fragments, half visibility, delayed presentation, blank
budget exhaustion, backward time, mixed signed-image sequences, two simultaneous
QR candidates (identical and different), malformed/uppercase QR text, altered frame
payload with valid QR error correction, reframed bad signatures, independently
signed wrong transaction scopes, poisoned receiver reuse, parser allocation bombs,
worker crashes, oversized replies and CPU/wall exhaustion. The original optical
suite retains replay/race/crash/journal tests; no recovery shortcut is added.

## Still outside this experiment

No physical camera, ATM display, recorded camera footage or video-container decoder
is used. Simulated video is an explicit schedule of generated raster images.
Perspective, arbitrary angles, focus, motion blur, glare, rolling shutter, exposure,
compression, dropped camera buffers and security-camera hardware integration remain
unmeasured. A physical pilot must use isolated hardware, independently provisioned
terminal/holder keys, protected persistent storage and trustworthy time, enforce
aggregate camera/decoder quotas, and report its actual optics and capture settings.

Cameras remain untrusted optical receivers, never cash-taken sensors or financial
witnesses. Successful optical evidence does not establish physical proximity,
human review, custody availability, external freshness, unique global offline
allocation or actual delivery. An unchanged live relay and cloned terminal storage
remain possible under the limitations already stated in the protocol. Physical
execution needs its own independently authorized hardware path and permanent
execution fence; reconciliation and financial authority remain separate obligations.

## Synthetic camera-buffer adapter (next test milestone)

The adapter converts borrowed raw Gray8, Rgb888 and Yuyv422 buffers into
the original bounded grayscale raster and isolated QR decoder. Yuyv422
requires even widths; chroma bytes are ignored. RGB uses bounded
integer luminance arithmetic, not floating-point corrections.

Dimensions 1..1024, full-row strides no more than 256 bytes above packed
row length, exact data size and at most 4 MiB raw input are enforced
before allocation. A malformed capture permanently poisons the assembly.
Clean captures count against the same 32-observation and ten-second
deadline, and the result remains held evidence only. The camera buffer
contains neither policy nor trusted timestamp; no physical camera is wired.
More detail: [camera-buffer test contract](ATM_CAMERA_CAPTURE_TEST.md).
