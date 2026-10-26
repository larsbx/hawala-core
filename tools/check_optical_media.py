#!/usr/bin/env python3
"""Linux TEST optical-media experiment; source-bound receipt, bounded fixtures.
Default: verify committed fixtures, write a fresh receipt under target/. Use
--update-fixtures only when deliberately reviewing deterministic fixture changes.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess

ROOT = Path(__file__).resolve().parent.parent
os.chdir(ROOT)
p = argparse.ArgumentParser()
p.add_argument('--output', type=Path, default=ROOT / 'target/optical-media')
p.add_argument('--update-fixtures', action='store_true')
a = p.parse_args()
a.output = a.output.resolve()
a.output.mkdir(parents=True, exist_ok=True)
def run(*cmd):
    subprocess.run(cmd, check=True)
def capture(*cmd):
    return subprocess.check_output(cmd, text=True).strip()
run('cargo', 'build', '--locked', '--release', '-p', 'hawala-optical-media-test', '--bin', 'optical-qr-decode', '--example', 'receipt')
run('target/release/examples/receipt', str(ROOT/'target/release/optical-qr-decode'), str(a.output))
fixtures = ROOT/'crates/hawala-optical-media-test/fixtures'
report = json.loads((a.output/'measurements.json').read_text())
for f in report['fixtures']:
    name = f['path']
    data = (a.output/name).read_bytes()
    if a.update_fixtures:
        fixtures.mkdir(exist_ok=True)
        (fixtures/name).write_bytes(data)
    else:
        assert (fixtures/name).read_bytes() == data, f'fixture drift: {name}'
# Bind every tracked/untracked source input, including fixtures, docs, lockfiles,
# worker, harness and workflow. Exclude only this reference receipt (self-hash).
paths = capture('git', 'ls-files', '--cached', '--others', '--exclude-standard').splitlines()
manifest = {}
for name in sorted(set(paths)):
    path = ROOT/name
    if not path.is_file() or name == 'docs/receipts/optical-media-reference.json':
        continue
    manifest[name] = hashlib.sha256(path.read_bytes()).hexdigest()
report['source'] = {
    'head': capture('git', 'rev-parse', 'HEAD'),
    'dirty': bool(capture('git', 'status', '--porcelain')),
    'sha256_manifest': manifest,
    'manifest_sha256': hashlib.sha256(json.dumps(manifest, sort_keys=True, separators=(',', ':')).encode()).hexdigest(),
    'excluded_self_reference': 'docs/receipts/optical-media-reference.json',
    'worker_sha256': hashlib.sha256((ROOT/'target/release/optical-qr-decode').read_bytes()).hexdigest(),
    'rustc': capture('rustc', '--version'), 'platform': platform.platform(),
    'profile': 'release',
}
(a.output/'receipt.json').write_text(json.dumps(report, indent=2, sort_keys=True)+'\n')
summary = {}
for row in report['decoding']:
    group = row['condition']
    x = summary.setdefault(group, {'exact': 0, 'trials': 0, 'max_wall_micros': 0, 'max_rss_kib': 0})
    x['trials'] += 1
    x['exact'] += row['decoded_exact']
    x['max_wall_micros'] = max(x['max_wall_micros'], row['wall_micros'])
    x['max_rss_kib'] = max(x['max_rss_kib'], row['peak_rss_kib'])
print(json.dumps(summary, indent=2))
print(f'Receipt: {a.output / "receipt.json"}')
