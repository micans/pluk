#!/usr/bin/env python3
"""Run the console examples of the pluk documents and compare their output
(plan/MANUAL.design.txt section 4).

    doccheck.py PLUK DOCUMENT..

Every ```console block (any fence of three or more backticks) is run, except one
preceded by a line <!-- notest -->. In a block, a line starting with '$ ' is a command
(a trailing backslash joins the next line); the lines up to the next command are its
expected output. The commands of one document run in order, in a fresh copy of the
document's directory, with PLUK's directory first on the PATH.

Expected lines starting with 'pluk:' are compared with stderr: each must occur, in order.
The other lines are compared with stdout, all of them; a line '...' stands for any number
of lines. Runs of white space count as one separator. The exit status is not compared.
"""

import os
import re
import shutil
import subprocess
import sys
import tempfile

FENCE = re.compile(r'^( {0,3})(`{3,})\s*(\S*)')


def examples(path):
    """(line number, command, expected lines) for each command of a console block."""
    lines = open(path, encoding='utf-8').read().split('\n')
    found = []
    i = 0
    notest = False
    while i < len(lines):
        m = FENCE.match(lines[i])
        if not m:
            if lines[i].strip() == '<!-- notest -->':
                notest = True
            elif lines[i].strip():
                notest = False
            i += 1
            continue
        indent, fence, lang = len(m.group(1)), m.group(2), m.group(3)
        body = []
        i += 1
        while i < len(lines) and not re.match(r'^ {0,3}' + fence + r'`*\s*$', lines[i]):
            # As CommonMark: the fence's indentation is removed from the content lines.
            text = lines[i]
            k = 0
            while k < indent and k < len(text) and text[k] == ' ':
                k += 1
            body.append((i + 1, text[k:]))
            i += 1
        i += 1
        if lang != 'console' or notest:
            notest = False
            continue
        k = 0
        while k < len(body):
            lineno, text = body[k]
            if not text.startswith('$ '):
                k += 1
                continue
            command = text[2:]
            while command.endswith('\\') and k + 1 < len(body):
                k += 1
                command = command[:-1] + ' ' + body[k][1].strip()
            k += 1
            expected = []
            while k < len(body) and not body[k][1].startswith('$ '):
                expected.append(body[k][1])
                k += 1
            while expected and not expected[-1].strip():
                expected.pop()
            found.append((lineno, command, expected))
    return found


def norm(line):
    return ' '.join(line.split())


def matches(expected, actual):
    """Whether actual (lines) matches expected (lines, '...' any number of lines)."""
    if not expected:
        return not actual
    if expected[0] == '...':
        return any(matches(expected[1:], actual[k:]) for k in range(len(actual) + 1))
    return bool(actual) and expected[0] == actual[0] and matches(expected[1:], actual[1:])


def subsequence(expected, actual):
    it = iter(actual)
    return all(any(e == a for a in it) for e in expected)


def check(pluk, document):
    """Run the examples of one document; the list of failures (text)."""
    failures = []
    work = tempfile.mkdtemp(prefix='doccheck-')
    try:
        here = os.path.join(work, 'doc')
        shutil.copytree(os.path.dirname(os.path.abspath(document)), here)
        env = dict(os.environ)
        env['PATH'] = os.path.dirname(os.path.abspath(pluk)) + os.pathsep + env.get('PATH', '')
        runs = examples(document)
        for lineno, command, expected in runs:
            r = subprocess.run(command, shell=True, cwd=here, env=env, capture_output=True, text=True,
                               stdin=subprocess.DEVNULL)
            want_err = [norm(e) for e in expected if e.startswith('pluk:')]
            want_out = [norm(e) for e in expected if not e.startswith('pluk:')]
            got_out = [norm(o) for o in r.stdout.split('\n')]
            got_err = [norm(e) for e in r.stderr.split('\n')]
            while got_out and not got_out[-1]:
                got_out.pop()
            if not matches(want_out, got_out) or not subsequence(want_err, got_err):
                failures.append('%s:%d: $ %s\n  expected:\n%s\n  stdout:\n%s\n  stderr:\n%s' % (
                    document, lineno, command,
                    '\n'.join('    ' + e for e in expected),
                    '\n'.join('    ' + o for o in r.stdout.rstrip('\n').split('\n')),
                    '\n'.join('    ' + e for e in r.stderr.rstrip('\n').split('\n'))))
        return len(runs), failures
    finally:
        shutil.rmtree(work)


def main():
    if len(sys.argv) < 3:
        sys.exit(__doc__)
    pluk, documents = sys.argv[1], sys.argv[2:]
    total, failed = 0, []
    for document in documents:
        n, failures = check(pluk, document)
        total += n
        failed.extend(failures)
    for f in failed:
        print(f)
    if failed:
        print('--- %d of %d documentation examples failed ---' % (len(failed), total))
        sys.exit(1)
    print('--- %d documentation examples run ---' % total)


if __name__ == '__main__':
    main()
