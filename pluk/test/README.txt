pluk/test: the test suite
=========================

Everything is run from pluk/ (the directory above this one) through make, which builds
pluk first:

  make test        all tests: invocations, documentation examples, manual references
  make docupdate   rewrite the output shown in the documents for examples that no
                   longer match


Invocation tests
----------------
Each invocation in Makefile writes o.<name>.test, compared with the stored o.<name>.ref.
After a deliberate change in output, check the difference and accept it:

  cd test && make accept T='name1 name2'      (copies o.<name>.test to o.<name>.ref)


Documentation examples
----------------------
The ```console examples in README.md and doc/*.md are run and their output compared
(doccheck.py). To change an example:

  1. edit the command in the document;
  2. make docupdate                (in pluk/)
  3. git diff ../doc ../README.md  review the rewritten output;
  4. make test.

docupdate does not rewrite a 'pluk:' (stderr) line that no longer matches; it reports
it, and you fix it by hand.

Running doccheck.py by hand is rarely needed. If you do, from pluk/test:

  python3 doccheck.py ../target/release/pluk ../../doc/PLUK-MANUAL.md
  python3 doccheck.py ../target/release/pluk --copy ../../doc ../../README.md

The first argument is the pluk binary to test. --copy is only for README.md, whose
examples say doc/harvest.txt instead of harvest.txt. Be careful with --update outside
make: a wrong binary or a wrong --copy gives wrong output, and that output is then
written into the document.


Scripts
-------
  doccheck.py   run the console examples of documents (also checks links, section refs)
  docalign.py   align the example output in columns (used by doccheck.py --update)
  opscheck.py   operator reference of the manual against pluk --list-ops-tsv
  optcheck.py   option reference of the manual against pluk --help
  mkdata.py     write a large random TAB-separated test file
  mkbam.py      write the hand-made BAM test files (run once; the files are committed)

Each script prints its usage when run without arguments.
