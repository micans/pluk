#!/usr/bin/env python3
"""Check the relative links of HTML pages (zoem/plan/docstep1.txt items 8 and 9).

    htmllinks.py PAGE.html..

Every href and src that is not absolute (no scheme such as https:, not starting with /)
must name a file that exists relative to the page, and a #fragment must be an id in
that file (or in the page itself, for href="#id"). This is what makes the pages work
opened from disk, on GitHub Pages or anywhere else (decision 8). A link to the site
root (/...) is an error too. Exit status 1 if anything is wrong.
"""

import os
import re
import sys

LINK = re.compile(r'\b(?:href|src)="([^"]*)"')
ID = re.compile(r'\bid="([^"]*)"')
SCHEME = re.compile(r'^[a-zA-Z][a-zA-Z0-9+.-]*:')

_ids = {}


def ids(path):
    if path not in _ids:
        _ids[path] = set(ID.findall(open(path, encoding='utf-8').read()))
    return _ids[path]


def check(page):
    failures = []
    here = os.path.dirname(os.path.abspath(page))
    for n, line in enumerate(open(page, encoding='utf-8'), 1):
        for target in LINK.findall(line):
            if SCHEME.match(target):
                continue
            if target.startswith('/'):
                failures.append('%s:%d: link to the site root: %s' % (page, n, target))
                continue
            name, _, fragment = target.partition('#')
            path = os.path.join(here, name) if name else os.path.abspath(page)
            if not os.path.isfile(path):
                failures.append('%s:%d: no such file: %s' % (page, n, target))
            elif fragment and path.endswith('.html') and fragment not in ids(path):
                failures.append('%s:%d: no id "%s" in %s' % (page, n, fragment, name or page))
    return failures


def main():
    failures = [f for page in sys.argv[1:] for f in check(page)]
    for f in failures:
        print(f)
    return 1 if failures else 0


if __name__ == '__main__':
    sys.exit(main())
