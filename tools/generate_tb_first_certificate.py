"""Independently construct the finite certificate; Lean must reproduce its bytes.

This finite construction is not a substitute for Lean compilation. It does not
change files unless --write is explicitly passed. No financial runtime involved.
"""
import argparse
from itertools import product
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
FIELDS = ('committed', 'effects', 'sinkA', 'sinkB', 'acknowledged')
ALPHABET = [f'accept({n},true)' for n in range(1, 4)] + ['accept(1,false)']
ALPHABET += [f'persist{sink}({n})' for sink in 'AB' for n in range(1, 4)]
ALPHABET += [f'acknowledge({n},true)' for n in range(1, 4)]
ALPHABET += ['acknowledge(3,false)', 'timeout', 'crash', 'replay']


def transition(state, action):
    c, effects, a, b, ack = state
    out = list(state)
    if '(' not in action:
        return state
    tag, args = action.rstrip(')').split('(')
    args = args.split(',')
    n = int(args[0])
    if tag == 'accept' and args[1] == 'true' and n == c + 1:
        out[:2] = [n, effects + 1]
    elif tag == 'persistA' and n == a + 1 and n <= c:
        out[2] = n
    elif tag == 'persistB' and n == b + 1 and n <= c:
        out[3] = n
    elif tag == 'acknowledge' and args[1] == 'true' and n <= c:
        out[4] = max(ack, n)
    return tuple(out)


def certificate():
    graph = []
    for c in range(4):
        for a, b, k in product(range(c + 1), repeat=3):
            path = [f'accept({n},true)' for n in range(1, c + 1)]
            path += [f'persistA({n})' for n in range(1, a + 1)]
            path += [f'persistB({n})' for n in range(1, b + 1)]
            if k:
                path.append(f'acknowledge({k},true)')
            state = (c, c, a, b, k)
            actual = (0, 0, 0, 0, 0)
            for event in path:
                actual = transition(actual, event)
            assert actual == state
            graph.append((state, path))
    states = {state for state, _ in graph}
    assert len(states) == 100
    tsv = ['path\t' + '\t'.join(FIELDS)]
    quint = ['// Finite transition certificate; Lean exporter must reproduce all 1700 edges.',
             'module tigerBeetleFirstConformance {',
             '  import tigerBeetleFirst.* from "./tigerBeetleFirst"']
    index = 0
    for state, path in graph:
        for event in ALPHABET:
            target = transition(state, event)
            assert target in states
            names = path + [event]
            tsv.append(';'.join(names) + '\t' + '\t'.join(map(str, target)))
            record = '{ ' + ', '.join(f'{k}: {v}' for k, v in zip(FIELDS, target)) + ' }'
            actions = ''.join(f'.then({name})' for name in names)
            quint.append(f'  run tbFirstEdge{index}Test = init{actions}.expect(s == {record})')
            index += 1
    assert index == 1700
    quint.append('}')
    return {
        'formal/traces/tigerBeetleFirst.tsv': '\n'.join(tsv) + '\n',
        'formal/quint/tigerBeetleFirstConformance.qnt': '\n'.join(quint) + '\n',
    }


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--write', action='store_true')
    args = parser.parse_args()
    for name, content in certificate().items():
        path = ROOT / name
        if args.write:
            path.write_text(content)
        else:
            assert path.read_text() == content, f'certificate drift: {name}'
    print('PASS finite certificate: 100 witnessed states, 1700 closed edges; Lean check is separate')
