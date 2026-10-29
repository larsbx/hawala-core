#!/usr/bin/env python3
"""Unsafe storage mutations must fail their intended assertion, not compilation."""
from pathlib import Path
import os
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
MUTATIONS = [
    ('ignore-native-flags', 'src/native.rs', '|| t.flags.0 != linked', '|| false /* unsafe flags */',
     ['--lib', 'pinned_native_link_flags_are_checked_before_normalization']),
    ('accept-changed-retained-body', 'src/durable.rs',
     'if bytes != admission.bytes || sig != admission.signature {',
     'if false && (bytes != admission.bytes || sig != admission.signature) {',
     ['--test', 'durable', 'evidence_survives_reopen_rejects_changed_retry_and_reserved_range_overlap']),
    ('ignore-range-collision', 'src/durable.rs', 'INSERT INTO reservations VALUES',
     'INSERT OR IGNORE INTO reservations VALUES',
     ['--test', 'durable', 'evidence_survives_reopen_rejects_changed_retry_and_reserved_range_overlap']),
    ('split-cursor-from-bundle', 'src/durable.rs', 'UPDATE cursor SET last=?1 WHERE id=1',
     'UPDATE cursor SET last=last WHERE id=1 AND last=?1',
     ['--test', 'durable', 'signed_sink_reopens_with_exact_bytes_and_rejects_changed_body_cursor_and_signature']),
]
with tempfile.TemporaryDirectory(prefix='hawala-storage-mutations-') as temporary:
    work = Path(temporary)
    shutil.copytree(ROOT / 'crates', work / 'crates')
    for name in ['Cargo.toml', 'Cargo.lock']:
        shutil.copy2(ROOT / name, work / name)
    crate = work / 'crates/hawala-tigerbeetle-native'
    for label, path, before, after, selection in MUTATIONS:
        source = crate / path
        original = source.read_text()
        assert original.count(before) == 1, (label, 'missing unique mutation target')
        source.write_text(original.replace(before, after))
        # Preserve compilation for the flags mutation without relaxing warnings
        # in normal checks; the mutated local binding otherwise becomes unused.
        env = dict(os.environ, CARGO_TARGET_DIR=str(ROOT / 'target/storage-mutations'))
        result = subprocess.run(['cargo', 'test', '--offline', '--locked', '-p',
            'hawala-tigerbeetle-native', '--features', 'native-conformance', *selection],
            cwd=work, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        source.write_text(original)
        assert result.returncode != 0, (label, 'unsafe mutation escaped')
        assert 'panicked at' in result.stdout and 'test result: FAILED.' in result.stdout, result.stdout
        assert 'could not compile' not in result.stdout, result.stdout
        print(f'PASS: intended assertion rejects {label}', flush=True)
