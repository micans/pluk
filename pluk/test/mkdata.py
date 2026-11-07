#!/usr/bin/env python3
"""Write a gzipped TAB-separated test file of NROW rows and NCOL columns.

    mkdata.py NROW NCOL [SEED]

The file is test/tempest-NROWxNCOL.txt.gz (next to this script), with a header line.
The same arguments give the same file, byte for byte (SEED defaults to 1).
NCOL is at least 5. The columns:

    id      integers, mostly positive
    weight  floats, now and then in exponent notation
    mixed   integers and floats; about 1 in 20 is NA, empty or nan
    call    a word of the NATO phonetic alphabet
    line    consecutive words of Shakespeare's The Tempest, at most 50 bytes
    c6 ..   small integers, for the columns after the fifth

tempest.txt is the play as published by Project Gutenberg (eBook #1540), with the
Gutenberg header and licence removed (public domain). The phrases are drawn from the
spoken text: speaker names, headings and stage directions are left out.
"""

import gzip
import os
import random
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))

CALLS = ['Alfa', 'Bravo', 'Charlie', 'Delta', 'Echo', 'Foxtrot', 'Golf', 'Hotel', 'India',
         'Juliett', 'Kilo', 'Lima', 'Mike', 'November', 'Oscar', 'Papa', 'Quebec', 'Romeo',
         'Sierra', 'Tango', 'Uniform', 'Victor', 'Whiskey', 'Xray', 'Yankee', 'Zulu']

MAX_LINE = 50


def tempest_words():
    """The words of the spoken text of the play, in order."""
    lines = open(os.path.join(HERE, 'tempest.txt'), encoding='utf-8').read().split('\n')
    # The play proper starts at the first scene; before it are the title, the contents
    # and the list of characters.
    start = next(i for i, line in enumerate(lines) if line.startswith('SCENE I.'))
    words = []
    for line in lines[start:]:
        if not line.strip() or line.startswith(' '):         # stage directions are indented
            continue
        if not re.search(r'[a-z]', line):                     # speaker names, headings
            continue
        if line.startswith(('SCENE', 'ACT ')):
            continue
        line = re.sub(r'\[.*?\]', ' ', line).replace('_', '')  # inline stage directions
        words.extend(line.split())
    return words


def phrase(rng, words):
    """Consecutive words from a random place, at most MAX_LINE bytes."""
    i = rng.randrange(len(words))
    out = words[i]
    for word in words[i + 1:]:
        if len((out + ' ' + word).encode('utf-8')) > MAX_LINE:
            break
        out += ' ' + word
    return out.encode('utf-8')[:MAX_LINE].decode('utf-8', 'ignore')


def integer(rng):
    return str(rng.randint(1, 1000000) if rng.random() < 0.9 else rng.randint(-1000, 0))


def real(rng):
    r = rng.random()
    if r < 0.05:
        return '%.3g' % (rng.random() * 1e-4)
    if r < 0.10:
        return '%.3g' % (rng.random() * 1e9)
    return '%.3f' % rng.lognormvariate(2, 1)


def mixed(rng):
    r = rng.random()
    if r < 0.05:
        return rng.choice(['NA', '', 'nan'])
    if r < 0.5:
        return str(rng.randint(-50, 500))
    return '%.2f' % rng.uniform(-50, 500)


def main():
    args = sys.argv[1:]
    if len(args) not in (2, 3) or not all(a.isdigit() for a in args):
        sys.exit(__doc__)
    nrow, ncol = int(args[0]), int(args[1])
    seed = int(args[2]) if len(args) == 3 else 1
    if ncol < 5:
        sys.exit('mkdata.py: NCOL must be at least 5')
    rng = random.Random(seed)
    words = tempest_words()
    path = os.path.join(HERE, 'tempest-%dx%d.txt.gz' % (nrow, ncol))
    header = ['id', 'weight', 'mixed', 'call', 'line'] + ['c%d' % k for k in range(6, ncol + 1)]
    # mtime 0 and no file name in the gzip header: the same arguments, the same bytes.
    with open(path, 'wb') as raw, gzip.GzipFile(filename='', mode='wb', fileobj=raw, mtime=0) as gz:
        gz.write(('\t'.join(header) + '\n').encode('utf-8'))
        for _ in range(nrow):
            row = [integer(rng), real(rng), mixed(rng), rng.choice(CALLS), phrase(rng, words)]
            row += [str(rng.randint(0, 99)) for _ in range(ncol - 5)]
            gz.write(('\t'.join(row) + '\n').encode('utf-8'))
    print(path)


if __name__ == '__main__':
    main()
