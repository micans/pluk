#!/usr/bin/env python3
"""Check the operator reference of the pluk manual against pluk itself
(plan/MANUAL.design.txt section 3).

    opscheck.py PLUK MANUAL            check
    opscheck.py --update PLUK MANUAL   rewrite the signature lines
    opscheck.py --skeleton PLUK        print a reference section for every operator

In the manual each operator has a marker line '<!-- op NAME -->' followed by one
generated signature line, then the hand-written description (up to the next heading or
marker). The signature line is

    `OPERANDS` → `RESULT` — DESCRIPTION

taken from pluk --list-ops-tsv. Check mode fails (exit 1) on a signature line that
differs from pluk's, and on a marker for an operator pluk does not have; operators that
are missing from the manual or have no description yet are reported, not errors (until
the reference is written).
"""

import subprocess
import sys

MARK = '<!-- op '


def operators(pluk):
    """(name, category, operands, result, description) in pluk's listing order."""
    out = subprocess.run([pluk, '--list-ops-tsv'], capture_output=True, text=True, check=True,
                         stdin=subprocess.DEVNULL).stdout
    return [tuple(line.split('\t')) for line in out.split('\n') if line]


def signature(op):
    _, _, operands, result, doc = op
    return '`%s` → `%s` — %s' % (operands, result, doc)


def marker_name(line):
    s = line.strip()
    if s.startswith(MARK) and s.endswith('-->'):
        return s[len(MARK):-3].strip()
    return None


def entries(lines):
    """(index of the marker line, name, has a description) for each marker."""
    found = []
    for i, line in enumerate(lines):
        name = marker_name(line)
        if name is None:
            continue
        k = i + 2
        described = False
        while k < len(lines) and not lines[k].startswith('#') and marker_name(lines[k]) is None:
            if lines[k].strip():
                described = True
                break
            k += 1
        found.append((i, name, described))
    return found


def skeleton(ops):
    out = []
    category = None
    for op in ops:
        if op[1] != category:
            category = op[1]
            out.append('### %s\n' % category)
        out.append('#### `%s`\n%s%s -->\n%s\n' % (op[0], MARK, op[0], signature(op)))
    return '\n'.join(out)


def main():
    args = sys.argv[1:]
    if args[:1] == ['--skeleton'] and len(args) == 2:
        print(skeleton(operators(args[1])))
        return
    update = args[:1] == ['--update']
    if update:
        args = args[1:]
    if len(args) != 2:
        sys.exit(__doc__)
    pluk, manual = args
    ops = operators(pluk)
    by_name = {op[0]: op for op in ops}
    lines = open(manual, encoding='utf-8').read().split('\n')
    errors, seen, undescribed = [], set(), []
    for i, name, described in entries(lines):
        seen.add(name)
        op = by_name.get(name)
        if op is None:
            errors.append('%s:%d: operator [%s] is not a pluk operator' % (manual, i + 1, name))
            continue
        want = signature(op)
        if i + 1 >= len(lines) or lines[i + 1] != want:
            if update:
                if i + 1 < len(lines) and lines[i + 1].startswith('`'):
                    lines[i + 1] = want
                else:
                    lines.insert(i + 1, want)
            else:
                errors.append('%s:%d: signature of [%s] differs from pluk:\n  manual: %s\n  pluk:   %s' % (
                    manual, i + 2, name, lines[i + 1] if i + 1 < len(lines) else '', want))
        if not described:
            undescribed.append(name)
    missing = [op for op in ops if op[0] not in seen]
    if update:
        open(manual, 'w', encoding='utf-8').write('\n'.join(lines))
        for op in missing:
            print('missing from the manual (to be placed by hand):\n\n#### `%s`\n%s%s -->\n%s\n' % (
                op[0], MARK, op[0], signature(op)))
    for e in errors:
        print(e)
    print('--- operator reference: %d operators, %d missing from the manual, %d without a description, '
          '%d errors ---' % (len(ops), len(missing), len(undescribed), len(errors)))
    if errors:
        sys.exit(1)


if __name__ == '__main__':
    main()
