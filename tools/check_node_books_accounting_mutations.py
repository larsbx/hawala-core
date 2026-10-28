"""Require the intended accounting assertion, never a tool/runtime failure."""
from pathlib import Path
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parent.parent
MODULE = Path('formal/evidence/node-books-accounting.mjs')
CHECKER = Path('tools/check_node_books_accounting.mjs')
FIXTURE = Path('formal/evidence/node-books-accounting-v1.json')
MUTATIONS = [
    ('skip projection equality',
     'if (!Buffer.isBuffer(wire) || !wire.equals(expected))',
     'if (false)', 'projection-byte-0', True),
    ('unlink journal legs',
     'user_data_128: eventReference, linked: true',
     'user_data_128: eventReference, linked: false', 'golden-transfer-plan', False),
    ('reverse ledger ordering',
     '.toSorted(cmp);', '.toSorted((a, b) => cmp(b, a));', 'golden-transfer-plan', False),
    ('allow reused account IDs',
     ' || ids.has(a.tb_id)', '', 'registry-invalid-2', True),
    ('skip per-ledger balance',
     'if (d !== c) throw Error(\'unbalanced ledger\');',
     'if (false) throw Error(\'unbalanced ledger\');', 'journal-invalid-10', True),
    ('allow old account remapping',
     'n.tb_id !== a.tb_id || ', '', 'registry-immutable-history', True),
]


def run(directory):
    return subprocess.run(['node', str(directory / CHECKER)],
                          capture_output=True, text=True, timeout=30)


def main():
    baseline = run(ROOT)
    if baseline.returncode:
        raise RuntimeError('baseline failed:\n' + baseline.stdout + baseline.stderr)
    source = (ROOT / MODULE).read_text()
    for name, old, new, intended, missing in MUTATIONS:
        if source.count(old) != 1:
            raise RuntimeError(f'{name}: mutation target drift')
        with tempfile.TemporaryDirectory(prefix='accounting-mutation-') as temp:
            directory = Path(temp)
            for relative in (MODULE, CHECKER, FIXTURE):
                target = directory / relative
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(ROOT / relative, target)
            (directory / MODULE).write_text(source.replace(old, new))
            result = run(directory)
            output = result.stdout + result.stderr
            if not (result.returncode != 0 and 'ERR_ASSERTION' in output
                    and intended in output
                    and (not missing or 'Missing expected exception' in output)):
                raise RuntimeError(f'{name}: intended counterexample absent:\n{output}')
        print(f'Caught unsafe accounting mutation: {name}')
    print('All six intended accounting negative controls passed.')


if __name__ == '__main__':
    main()
