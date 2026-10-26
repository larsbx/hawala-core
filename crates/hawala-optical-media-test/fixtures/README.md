# Public TEST images

`precommit-0.pgm` through `precommit-2.pgm` are deterministic 372×372 grayscale
QR images of one test-only signed commitment, generated from public seeds in
`tests/support/mod.rs`. These identities and commitments have no real authority.
They contain no private user data. Do not use their seeds in a deployment.

Run `python3 tools/check_optical_media.py` from the repository root to reproduce
and byte-compare them. The script's source-bound receipt records hashes, exact
optical transforms, frame schedules, decoding measurements and held journal
results. Additional transformed images are generated in memory by the published
recipes. See `docs/ATM_OPTICAL_MEDIA_EXPERIMENT.md` for limits and physical gaps.
