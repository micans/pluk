#!/usr/bin/env python3
"""Update the console examples of a zoem document from pluk's actual output
(zoem/plan/docstep1.txt item 7, decision 3).

    azmupdate.py PLUK DOCUMENT.azm DOCUMENT.md

DOCUMENT.md is the Markdown made from DOCUMENT.azm (make, in zoem/). It is copied
beside itself (so that its links and the data files resolve as in make check) and
doccheck.py --update runs on the copy: the expected output of failing examples is
rewritten there, and docalign.py aligns it. The k-th ```console block of the Markdown
is the k-th \\console block of the .azm; each block whose content changed is written
back into the .azm, its text escaped for zoem (\\ { } as \\\\ \\{ \\}). Blocks that did
not change are left exactly as written.

Before writing, each changed block is checked against the .azm: the .azm text,
unescaped, must equal the block of DOCUMENT.md as it was. Otherwise the .md is stale
(the .azm was edited after it was made) or the block uses zoem escapes other than
these three; nothing is written then.

Exit status: 1 if doccheck still reports failures (for instance a 'pluk:' line, which
--update does not rewrite) or the .azm cannot be updated, else 0.
"""

import os
import re
import shutil
import subprocess
import sys

FENCE = re.compile(r'^( {0,3})(`{3,})\s*(\S*)')
CONSOLE_OPEN = re.compile(r'^\s*\\console\{\s*$')
CONSOLE_CLOSE = re.compile(r'^\}\s*$')


def md_blocks(path):
    """The bodies (lists of lines, fence indentation removed) of the ```console blocks."""
    lines = open(path, encoding='utf-8').read().split('\n')
    blocks, i = [], 0
    while i < len(lines):
        m = FENCE.match(lines[i])
        if not m:
            i += 1
            continue
        indent, fence, lang = len(m.group(1)), m.group(2), m.group(3)
        body = []
        i += 1
        while i < len(lines) and not re.match(r'^ {0,3}' + fence + r'`*\s*$', lines[i]):
            text = lines[i]
            k = 0
            while k < indent and k < len(text) and text[k] == ' ':
                k += 1
            body.append(text[k:])
            i += 1
        i += 1
        if lang == 'console':
            blocks.append(body)
    return blocks


def azm_blocks(lines):
    """(first, end) per \\console block: lines[first:end] are its content lines."""
    found, start = [], None
    for n, line in enumerate(lines):
        if start is None:
            if CONSOLE_OPEN.match(line):
                start = n + 1
        elif CONSOLE_CLOSE.match(line):
            found.append((start, n))
            start = None
    return found


def unescape(line):
    return re.sub(r'\\([\\{}])', r'\1', line)


def escape(line):
    return re.sub(r'([\\{}])', r'\\\1', line)


def main():
    if len(sys.argv) != 4:
        sys.exit(__doc__)
    pluk, azm, md = sys.argv[1:]
    tools = os.path.dirname(os.path.abspath(__file__))
    copy = os.path.join(os.path.dirname(os.path.abspath(md)), '.update-' + os.path.basename(md))
    shutil.copyfile(md, copy)
    try:
        r = subprocess.run([sys.executable, os.path.join(tools, 'doccheck.py'), '--update', pluk, copy])
        old, new = md_blocks(md), md_blocks(copy)
    finally:
        os.remove(copy)

    lines = open(azm, encoding='utf-8').read().split('\n')
    spans = azm_blocks(lines)
    if not (len(spans) == len(old) == len(new)):
        sys.exit('azmupdate.py: %s has %d \\console blocks, %s %d (%d after the update); '
                 'remake the .md' % (azm, len(spans), md, len(old), len(new)))

    changed = [k for k in range(len(old)) if old[k] != new[k]]
    for k in changed:
        first, end = spans[k]
        if [unescape(line) for line in lines[first:end]] != old[k]:
            sys.exit('azmupdate.py: %s:%d: console block %d differs from %s; remake the .md '
                     '(nothing written)' % (azm, first, k + 1, md))
    for k in reversed(changed):
        first, end = spans[k]
        lines[first:end] = [escape(line) for line in new[k]]
    if changed:
        open(azm, 'w', encoding='utf-8').write('\n'.join(lines))
        for k in changed:
            print('%s:%d: console block %d updated' % (azm, spans[k][0], k + 1))
    return 1 if r.returncode else 0


if __name__ == '__main__':
    sys.exit(main())
