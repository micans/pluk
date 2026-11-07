#!/usr/bin/env python3
"""Check that every option of pluk --help is mentioned in the manual (plan/step29.txt
decision 3).

    optcheck.py PLUK MANUAL            report the options the manual does not mention
    optcheck.py --strict PLUK MANUAL   as above, and fail (exit 1) if there are any

An option is any word of the help text that starts with '-' or '--' followed by a letter
(or '-/' and '-//'), not preceded by a letter, digit, '-' or '%'. It is reduced to its
stem: what follows '=', '<', '/', '^' or ',' is dropped, as are digits (-A2 -O12) and a
'-NAME' or '-Q' placeholder part (--kd-NAME, --quant-NAME); a trailing '-' is dropped
(--kdict-). A single-dash word of different letters is a combination of short options
(-kO12) and stands for each of them. The manual mentions the stem if it occurs there not
followed by a letter or by '-' and a lower-case letter (so --sam-h does not count as a
mention of --sam, while --kd-NAME counts for --kd and -A2 for -A).
"""

import re
import subprocess
import sys

WORD = re.compile(r'(?<![\w%-])(-//?(?=<)|--?[A-Za-z][\w/^<>=,.-]*)')


def stem(word):
    if word.startswith('-/'):
        return word
    s = re.split(r'[=<^,/]', word)[0]
    parts = s.split('-')
    # parts: '', ('',) name pieces; drop placeholder pieces NAME and Q
    keep = [p for i, p in enumerate(parts) if not (i > 1 and p in ('NAME', 'Q'))]
    s = '-'.join(keep).rstrip('-.')
    if not s.startswith('--'):
        s = s.rstrip('0123456789')
    return s


def options(pluk):
    text = subprocess.run([pluk, '--help'], capture_output=True, text=True, check=True,
                          stdin=subprocess.DEVNULL).stdout
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


def mentioned(stem_, manual):
    return re.search(r'(?<![\w-])' + re.escape(stem_) + r'(?![A-Za-z]|-[a-z])', manual) is not None


def main():
    args = sys.argv[1:]
    strict = args[:1] == ['--strict']
    if strict:
        args = args[1:]
    if len(args) != 2:
        sys.exit(__doc__)
    pluk, path = args
    manual = open(path, encoding='utf-8').read()
    opts = options(pluk)
    missing = [o for o in opts if not mentioned(o, manual)]
    for o in missing:
        print('%s: option [%s] of pluk --help is not mentioned' % (path, o))
    print('--- options: %d in pluk --help, %d not mentioned in the manual%s ---' % (
        len(opts), len(missing), '' if strict else ' (report only)'))
    if strict and missing:
        sys.exit(1)


if __name__ == '__main__':
    main()
