"""Require specific assertion counterexamples, not infrastructure failures."""
from pathlib import Path
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parent.parent
MODULE = Path('formal/evidence/node-books-verification.mjs')
CHECKER = Path('tools/check_node_books_verification.mjs')
FIXTURE = Path('formal/evidence/node-books-verification-v1.json')
MUTATIONS = [
    ('skip signature',
     'if (!verify(null, expectedBytes, publicKey, signature))',
     'if (false)', 'tampered-byte-562'),
    ('skip expected binding',
     'if (!wire.subarray(0, BODY_LENGTH).equals(expectedBytes))',
     'if (false)', 'tampered-byte-34'),
    ('skip chain context',
     'if (!same) throw Error(`chain context: ${field}`);',
     'if (false) throw Error(`chain context: ${field}`);', 'scope-transplant'),
    ('allow source rollback',
     '|| c.source_sequence < p.source_sequence',
     '|| false', 'source-rollback'),
    ('skip predecessor binding',
     '|| !c.previous_receipt_digest.equals(previous.id)',
     '|| false', 'wrong-predecessor'),
    ('skip receipt adjacency',
     'c.verification_sequence !== p.verification_sequence + 1n',
     'false', 'skipped-receipt'),
    ('allow same-prefix conflict',
     'if (!c[field].equals(p[field]))',
     'if (false)', 'same-prefix-source_hash'),
]


def run(directory):
    return subprocess.run(['node', str(directory / CHECKER)],
                          capture_output=True, text=True, timeout=30)


def main():
    baseline = run(ROOT)
    if baseline.returncode != 0:
        raise RuntimeError('baseline failed:\n' + baseline.stdout + baseline.stderr)
    source = (ROOT / MODULE).read_text()
    for name, old, new, intended in MUTATIONS:
        if source.count(old) != 1:
            raise RuntimeError(f'{name}: mutation target drift')
        with tempfile.TemporaryDirectory(prefix='node-books-mutation-') as temp:
            directory = Path(temp)
            for relative in (MODULE, CHECKER, FIXTURE):
                target = directory / relative
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(ROOT / relative, target)
            (directory / MODULE).write_text(source.replace(old, new))
            result = run(directory)
            output = result.stdout + result.stderr
            if not (result.returncode != 0 and 'ERR_ASSERTION' in output
                    and 'Missing expected exception' in output and intended in output):
                raise RuntimeError(f'{name}: intended counterexample absent:\n{output}')
        print(f'Caught unsafe mutation: {name}')
    print('All seven intended negative controls passed; runtime errors cannot satisfy them.')


if __name__ == '__main__':
    main()
