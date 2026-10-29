"""Require intended assertion failures for four unsafe TB-first core mutations.

Compilers/tool failures are infrastructure errors, never successful controls.
Only temporary source copies are changed. No financial database is contacted.
"""
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
LIB = ROOT / 'crates/hawala-tigerbeetle-flow/src/lib.rs'
TESTS = ROOT / 'crates/hawala-tigerbeetle-flow/tests/flow.rs'
MUTATIONS = [
    ('changed-body acceptance', 'actual.transfer != *expected', 'false',
     'changed_body_on_existing_id_fails_closed'),
    ('missing operation anchor', 'transfers[0].id != operation', 'false',
     'metadata_cannot_replace_the_unique_first_transfer_anchor'),
    ('partial-group retry',
     'return Err(Error::Conflict);\n    }\n    // A previously',
     'return record(port, plan);\n    }\n    // A previously',
     'partial_group_is_conflict_and_never_retried'),
    ('cursor-prefix bypass', 'group.first_timestamp() <= end', 'false',
     'destination_cursor_does_not_advance_over_a_replayed_page'),
]
REPLAY_MUTATIONS = [
    ('unsupported native mode accepted',
     'row.flags != 0 || row.pending_id != 0 || row.timeout != 0', 'false',
     'unsupported_native_modes_must_not_be_normalized_into_posted_history'),
    ('partial-group checkpoint', 'self.read = row.timestamp;',
     'self.read = row.timestamp; self.complete = self.read;',
     'page_splits_never_emit_or_checkpoint_a_partial_group'),
]


def run(source, test=None, replay_source=None, tests=TESTS):
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        path = root / 'lib.rs'
        path.write_text(source)
        # Preserve the real local module when compiling an isolated mutation.
        (root / 'replay.rs').write_text(replay_source if replay_source is not None
                                      else (LIB.parent / 'replay.rs').read_text())
        artifact = root / 'libhawala_tigerbeetle_flow.rlib'
        subprocess.run(['rustc', '--edition', '2021', '--crate-type', 'lib',
                        '--crate-name', 'hawala_tigerbeetle_flow', str(path),
                        '-o', str(artifact)], check=True, capture_output=True)
        binary = root / 'tests'
        subprocess.run(['rustc', '--edition', '2021', '--test', str(tests),
                        '--extern', f'hawala_tigerbeetle_flow={artifact}',
                        '-o', str(binary)], check=True, capture_output=True)
        args = [str(binary)] + (['--exact', test] if test else [])
        return subprocess.run(args, capture_output=True, text=True)


if __name__ == '__main__':
    original = LIB.read_text()
    baseline = run(original)
    if baseline.returncode != 0 or '16 passed' not in baseline.stdout:
        raise SystemExit(f'baseline failed: {baseline.stdout}\n{baseline.stderr}')
    for name, before, after, test in MUTATIONS:
        if original.count(before) != 1:
            raise SystemExit(f'bad mutation target: {name}')
        result = run(original.replace(before, after), test)
        if (result.returncode != 101 or '0 passed; 1 failed' not in result.stdout
                or f'test {test} ... FAILED' not in result.stdout):
            raise SystemExit(f'intended assertion did not fail: {name}: '
                             f'{result.stdout}\n{result.stderr}')
        print(f'PASS negative control: {name} caught by {test}')
    replay = (LIB.parent / 'replay.rs').read_text()
    replay_tests = TESTS.parent / 'replay.rs'
    baseline = run(original, tests=replay_tests)
    if baseline.returncode != 0 or '13 passed' not in baseline.stdout:
        raise SystemExit(f'replay baseline failed: {baseline.stdout}\n{baseline.stderr}')
    for name, before, after, test in REPLAY_MUTATIONS:
        if replay.count(before) != 1:
            raise SystemExit(f'bad replay mutation target: {name}')
        result = run(original, test, replay.replace(before, after), replay_tests)
        if (result.returncode != 101 or '0 passed; 1 failed' not in result.stdout
                or f'test {test} ... FAILED' not in result.stdout):
            raise SystemExit(f'intended replay assertion did not fail: {name}: '
                             f'{result.stdout}\n{result.stderr}')
        print(f'PASS negative control: {name} caught by {test}')
