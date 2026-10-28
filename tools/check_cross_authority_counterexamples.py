"""Validate retained actual TLC traces independently of TLC's console label."""
import json
from pathlib import Path


def safe(s):
    return (
        all(s[k] in (0, 1, 2) for k in ('voteL', 'voteR', 'decision', 'wire'))
        and (not s['seenL'] or s['voteL'] == 1)
        and (not s['seenR'] or s['voteR'] == 1)
        and (s['decision'] != 1 or (s['seenL'] and s['seenR']))
        and all(s[k] == 0 or s[k] == s['decision'] for k in ('wire', 'localL', 'localR'))
        and all(s[k] == 0 or s[k] == s['wire'] for k in ('localL', 'localR'))
    )


traces = json.loads((Path(__file__).resolve().parents[1] /
                     'formal/traces/crossAuthority-negative.json').read_text())
assert {t['mutation'] for t in traces} == {
    f'unsafe{k}Step' for k in ('Timeout', 'Single', 'Recovery', 'Replay', 'Zombie')}
for t in traces:
    states = t['states']
    assert len(states) >= 2 and all(safe(s) for s in states[:-1]), t['mutation']
    assert not safe(states[-1]), t['mutation']
    # Last transition must be the named defect, rather than a random broken state.
    before, after = states[-2:]
    changed = {k for k in before if before[k] != after[k]}
    kind = t['mutation']
    if kind == 'unsafeTimeoutStep':
        assert before['voteL'] == 1 and before['localL'] == 0
        assert changed == {'localL'} and after['localL'] == 2
    elif kind == 'unsafeSingleStep':
        assert before['decision'] == 0 and before['seenL'] and not before['seenR']
        assert changed == {'decision'} and after['decision'] == 1
    elif kind == 'unsafeRecoveryStep':
        assert before['fenceC'] and changed <= {'decision', 'wire'}
        assert after['decision'] == after['wire'] == 0
    elif kind == 'unsafeReplayStep':
        assert changed == {'localL'} and after['localL'] == 1
    elif kind == 'unsafeZombieStep':
        assert before['fenceC'] and before['decision'] == 2
        assert changed == {'wire'} and after['wire'] == 1
print('PASS: five retained TLC counterexamples have safe prefixes and the intended failing mutation')
