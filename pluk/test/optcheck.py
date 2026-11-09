#!/usr/bin/env python3
"""Check the option reference of the manual against pluk --help (plan/step34.txt
decision 6; before: a mention anywhere in the manual, step29.txt decision 3).

    optcheck.py PLUK MANUAL            report the differences
    optcheck.py --strict PLUK MANUAL   as above, and fail (exit 1) if there are any

The option reference is the section '## N. Option reference'. Each of its entries is a
line '- `OPTION`: ..', where the lead may hold several code spans (`-H`, `--help`: ..);
the options of an entry are read from those code spans as from the help text. Every
option of pluk --help must have an entry, and every entry must name options of pluk
--help.

An option is any word of the help text that starts with '-' or '--' followed by a letter
(or '-/' and '-//', or '-%' and '-%%' as words of their own), not preceded by a letter, digit, '-' or '%'. It is reduced to its
stem: what follows '=', '<', '/', '^' or ',' is dropped, as are digits (-A2 -O12) and a
'-NAME' or '-Q' placeholder part (--kd-NAME, --quant-NAME); a trailing '-' is dropped
(--kdict-). A single-dash word of different letters is a combination of short options
(-kO12) and stands for each of them.
"""

import re
import subprocess
import sys

WORD = re.compile(r'(?<![\w%-])(-//?(?=<)|-%%?(?![\w%-])|--?[A-Za-z][\w/^<>=,.-]*)')


def stem(word):
    if word.startswith('-/') or word.startswith('-%'):
        return word
    s = re.split(r'[=<^,/]', word)[0]
    parts = s.split('-')
    # parts: '', ('',) name pieces; drop placeholder pieces NAME and Q
    keep = [p for i, p in enumerate(parts) if not (i > 1 and p in ('NAME', 'Q'))]
    s = '-'.join(keep).rstrip('-.')
    if not s.startswith('--'):
        s = s.rstrip('0123456789')
    return s


SECTION = re.compile(r'^## \d+\. Option reference\s*$')
ENTRY = re.compile(r'^- ((?:`[^`]+`(?:,? (?:or )?)?)+):')


def help_options(pluk):
    return options(subprocess.run([pluk, '--help'], capture_output=True, text=True, check=True,
                                  stdin=subprocess.DEVNULL).stdout)


def reference_options(manual):
    """The options of the entries of the option reference; None without that section."""
    lines = manual.split('\n')
    starts = [i for i, line in enumerate(lines) if SECTION.match(line)]
    if not starts:
        return None
    text = []
    for line in lines[starts[0] + 1:]:
        if line.startswith('## '):
            break
        m = ENTRY.match(line)
        if m:
            text.extend(re.findall(r'`([^`]+)`', m.group(1)))
    return options(' '.join(text))


def options(text):
    found = []
    for w in WORD.findall(text):
        s = stem(w)
        if not s.startswith('--') and len(set(s[1:])) > 1:
            stems = ['-' + c for c in s[1:]]
        else:
            stems = [s]
        for s in stems:
            if len(s) > 1 and s not in found:
                found.append(s)
    return found


def main():
    args = sys.argv[1:]
    strict = args[:1] == ['--strict']
    if strict:
        args = args[1:]
    if len(args) != 2:
        sys.exit(__doc__)
    pluk, path = args
    manual = open(path, encoding='utf-8').read()
    opts = help_options(pluk)
    listed = reference_options(manual)
    if listed is None:
        print('%s: no section \'## N. Option reference\'' % path)
        sys.exit(1)
    missing = [o for o in opts if o not in listed]
    unknown = [o for o in listed if o not in opts]
    for o in missing:
        print('%s: option [%s] of pluk --help has no entry in the option reference' % (path, o))
    for o in unknown:
        print('%s: option [%s] of the option reference is not in pluk --help' % (path, o))
    print('--- options: %d in pluk --help, %d without an entry, %d unknown in the option reference%s ---' % (
        len(opts), len(missing), len(unknown), '' if strict else ' (report only)'))
    if strict and (missing or unknown):
        sys.exit(1)


if __name__ == '__main__':
    main()
