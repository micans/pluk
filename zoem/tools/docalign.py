#!/usr/bin/env python3
"""Show the output of the console examples of the pluk documents aligned in columns
(plan/step33.txt decision 2).

    docalign.py DOCUMENT..

pluk writes TAB-separated output, which a rendered document shows with tab stops that
rarely line up. In every ```console block, the output lines of each command that contain
a TAB are padded with spaces into columns: each field but the last is padded to the
widest value of its column (over that command's output) plus two spaces. Lines without
a TAB, lines starting with 'pluk:' (messages) and lines starting with '@' (SAM header
lines, which have fields of their own) are left as they are. doccheck.py counts a run of
white space as one separator, so the check is unchanged. Running the script again
changes nothing (the lines it aligns keep no TAB).
"""

import re
import sys

FENCE = re.compile(r'^( {0,3})(`{3,})\s*(\S*)')
GAP = 2


def align(block):
    """Align the TAB lines of one command's output (a list of lines, in place)."""
    rows = [i for i, line in enumerate(block) if '\t' in line and not line.startswith(('pluk:', '@'))]
    if not rows:
        return 0
    fields = {i: block[i].split('\t') for i in rows}
    widths = []
    for parts in fields.values():
        for k, part in enumerate(parts[:-1]):
            if k == len(widths):
                widths.append(0)
            widths[k] = max(widths[k], len(part))
    for i, parts in fields.items():
        line = ''.join(part.ljust(widths[k] + GAP) for k, part in enumerate(parts[:-1])) + parts[-1]
        block[i] = line.rstrip()
    return len(rows)


def align_document(path):
    lines = open(path, encoding='utf-8').read().split('\n')
    i, changed = 0, 0
    while i < len(lines):
        m = FENCE.match(lines[i])
        if not m:
            i += 1
            continue
        indent, fence, lang = m.group(1), m.group(2), m.group(3)
        i += 1
        start = i
        while i < len(lines) and not re.match(r'^ {0,3}' + fence + r'`*\s*$', lines[i]):
            i += 1
        if lang == 'console':
            # The output of a command: the lines up to the next '$ ' line.
            body = [line[len(indent):] if line.startswith(indent) else line for line in lines[start:i]]
            k = 0
            while k < len(body):
                if body[k].startswith('$ '):
                    k += 1
                    while k < len(body) and body[k - 1].endswith('\\'):
                        k += 1
                    end = k
                    while end < len(body) and not body[end].startswith('$ '):
                        end += 1
                    out = body[k:end]
                    changed += align(out)
                    body[k:end] = out
                    k = end
                else:
                    k += 1
            lines[start:i] = [indent + line if line else line for line in body]
        i += 1
    open(path, 'w', encoding='utf-8').write('\n'.join(lines))
    return changed


def main():
    if len(sys.argv) < 2:
        sys.exit(__doc__)
    for path in sys.argv[1:]:
        print('%s: %d lines aligned' % (path, align_document(path)))


if __name__ == '__main__':
    main()
