#!/usr/bin/env python3
"""Report .azm source lines that would change meaning in the Markdown output.

    azmlint.py FILE.azm..

The md device keeps the line breaks of the source, and escapes the Markdown characters
that matter anywhere in a line, but not those that matter only at the start of one:
- + = and a number followed by . or ) (a list item, a setext heading underline, a
thematic break). Escaping them everywhere would clutter the text (non\\-negative);
instead this script reports a line of prose that starts with one of them, for the
author to rewrap (zoem/plan/docstep1-notes.txt, item 6).

Skipped: the lines of \\console blocks (literal), zoem comments (\\:). A line's leading
\\par{ \\car{ \\item is looked through, since the text after it starts an output line.
Exit status 1 if anything is reported.
"""

import re
import sys

LEAD = re.compile(r'^(?:\\par\{|\\car\{|\\item\b\s*)+')
BAD = re.compile(r'^(?:(?:[-+=]|\d+[.)])(?:\s|$)|[-=]{2,}\s*$)')
CONSOLE_OPEN = re.compile(r'^\s*\\console\{\s*$')
CONSOLE_CLOSE = re.compile(r'^\}\s*$')


def lint(path):
    found = []
    inside = False
    for n, line in enumerate(open(path, encoding='utf-8'), 1):
        if inside:
            inside = not CONSOLE_CLOSE.match(line)
            continue
        if CONSOLE_OPEN.match(line):
            inside = True
            continue
        text = line.strip()
        if text.startswith('\\:'):
            continue
        text = LEAD.sub('', text)
        if BAD.match(text):
            found.append('%s:%d: a line of prose starting with %r becomes Markdown syntax; '
                         'rewrap it' % (path, n, text.split()[0] if text.split() else text))
    return found


def main():
    found = [f for path in sys.argv[1:] for f in lint(path)]
    for f in found:
        print(f)
    return 1 if found else 0


if __name__ == '__main__':
    sys.exit(main())
