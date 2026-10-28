"""Validate retained actual TLC failures, not infrastructure errors."""
import json
from pathlib import Path

def safe(s):
    return (0 <= s['stage'] <= 3 and s['stage'] <= s['effect'] <= 3
            and s['effect'] <= s['stage'] + 1
            and (s['effect'] <= s['stage'] or s['pending'])
            and (not s['pending'] or (s['target'] == s['stage'] + 1 and s['target'] <= 3))
            and s['commits'] == (1 if s['effect'] == 3 else 0)
            and s['client'] <= s['stage'] and s['desired'] <= 3 and not s['paid'])

items = json.loads((Path(__file__).resolve().parents[1] / 'formal/traces/endpoint-negative.json').read_text())
assert {r['mutation'] for r in items} == {'unsafeReceivedStep', 'unsafeUnboundStep', 'unsafeForgetStep', 'unsafeRetryStep'}
for item in items:
    states = item['states']
    assert len(states) >= 2 and all(safe(s) for s in states[:-1])
    before, after = states[-2:]
    expected = dict(before)
    mutation = item['mutation']
    if mutation == 'unsafeReceivedStep':
        expected['paid'] = True
    elif mutation == 'unsafeUnboundStep':
        expected['client'] = 3
    elif mutation == 'unsafeForgetStep':
        assert before['commits'] == 1
        expected.update(stage=0, effect=0, pending=False)
    else:
        assert before['effect'] == 3
        expected['commits'] += 1
    assert after == expected, mutation
    assert not safe(after), mutation
print('PASS: four endpoint/outbox counterexamples have safe prefixes and unsafe terminal states')
