SHELL=/bin/bash

# Tests in the style of ../../pick/test/Makefile: each invocation writes
# o.<name>.test, which is compared against the stored o.<name>.ref .
# Run from the pluk directory with 'make test', which builds pluk first.

p=../target/release/pluk

.PHONY: test runpluk runtestopt rundiff

test: runpluk runtestopt rundiff
	@echo "--- All tests passed ---"

rundiff: $(wildcard *.ref)

runpluk: data.txt big.txt
	$(p)                  < data.txt         > o.lineno.test
	$(p)                  < big.txt          > o.big-lineno.test
	gzip -c big.txt | $(p)                   > o.big-lineno-gz.test
	{ gzip -c data.txt; gzip -c data.txt; } | $(p)  > o.multigz.test
	printf 'a\tb\nno newline\tat end' | $(p) > o.nonewline.test
	printf 'caf\xe9\t\xff\xfe\n' | $(p)      > o.nonutf8.test
	$(p)                  < /dev/null        > o.empty.test
	$(p) -c                     < data.txt   > o.count.test
	$(p) -kc                    < data.txt   > o.count-k.test
	$(p) -c                     < big.txt    > o.big-count-all.test
	$(p) -c                     < /dev/null  > o.count-empty.test
	head -n 1 data.txt | $(p) -c             > o.count-headeronly.test
	gzip -c big.txt | $(p) -kc               > o.count-gz.test
	$(p) -O3 < data.txt > /dev/null 2> o.omega-needs-k.test; [ $$? -eq 1 ]
	printf 'a\tb\nr1\t1\t2\nr2\t3\t4\n'  | $(p) -R          > o.R-short.test
	printf '\ta\tb\nr1\t1\t2\n'           | $(p) -R          > o.R-emptyfirst.test
	printf '\ta\tb\nr1\t1\t2\n'           | $(p)             > o.emptyfirst.test
	printf 'x\ta\tb\nr1\t1\t2\n'  | $(p) -R  2> o.R-full.test       > /dev/null; [ $$? -eq 1 ]
	printf 'a\tb\nr1\t1\t2\n'      | $(p)     2> o.short-header.test > /dev/null; [ $$? -eq 1 ]
	printf 'a\tb\tc\n1\t2\n'        | $(p)     2> o.long-header.test  > /dev/null; [ $$? -eq 1 ]
	printf '1\t2\n3\n'               | $(p) -k  2> o.k-ragged.test     > /dev/null; [ $$? -eq 1 ]
	printf 'a\tb\n1\t2\n\n'          | $(p)     2> o.emptyline.test    > /dev/null; [ $$? -eq 1 ]
	sed '50s/$$/\textra/' big.txt     | $(p)     2> o.big-badline.test  > /dev/null; [ $$? -eq 1 ]
	gzip -c big.txt | $(p) -c                                  > o.big-valid-gz.test
	$(p) -kR                    < data.txt  2> o.kR.test  > /dev/null; [ $$? -eq 1 ]
	tail -n +2 big.txt | $(p) -kO5 -c                  > o.big-omega-count.test
	printf 'a\tb\tc\td\te\nx\ny\tz\n\tq\tr\n' | $(p) -kO3  > o.omega-lineno.test
	seq 1 1000000 > seq.tmp; $(p) < seq.tmp | head -n 2 > o.brokenpipe.test; [ $${PIPESTATUS[0]} -eq 0 ]; rm seq.tmp

# Tests for temporary --test-* options. When such an option is removed, remove
# the target from the 'test' prerequisites (keep the tests) and note it in the
# plan step file. See plan/guidelines.txt.

runtestopt: data.txt
	{ printf '# c1\n#c2\n';  cat data.txt; } | $(p) --test-comment          > o.tc-skip.test
	{ printf '# c1\n#c2\n';  cat data.txt; } | $(p) --tc                    > o.tc-alias.test
	{ printf '# c1\n#c2\n';  cat data.txt; } | $(p) -kO13                   > o.tc-noskip.test
	{ printf '# c1\n';  head -n 3 data.txt; printf '# mid\n'; tail -n 2 data.txt; } | $(p) -kO13 --tc > o.tc-midfile.test
	{ printf '# c1\n#c2\n';  cat data.txt; } | gzip -c | $(p) --tc          > o.tc-gz.test
	printf '#a\n#b\n' | $(p) --tc                                          > o.tc-allcomment.test
	$(p) --test-i 1                         < data.txt  > o.ti-first.test
	$(p) --test-i 13                        < data.txt  > o.ti-last.test
	$(p) --test-i 14                        < data.txt  > o.ti-beyond.test
	$(p) --test-i 18                        < big.txt   > o.ti-big.test
	gzip -c big.txt | $(p) --test-i 18                  > o.ti-big-gz.test
	{ printf '# c1\n';  cat data.txt; } | $(p) --tc --test-i 2  > o.ti-tc.test
	printf 'a\tb\n\n\tz\nx\t\ty\n' | $(p) -kO3 --test-i 2    > o.ti-empty.test

	$(p) -k                     < data.txt   > o.k-lineno.test
	{ printf '#c\n';  cat data.txt; } | $(p) -c --tc   > o.tc-count.test
	$(p) --test-counts          < data.txt   2> o.counts-header.test > /dev/null
	$(p) -k --test-counts       < data.txt   2> o.counts-k.test      > /dev/null
	$(p) --test-counts          < /dev/null  2> o.counts-empty.test  > /dev/null
	printf 'a\tb\nr1\t1\t2\n' | $(p) -R --test-counts  2> o.counts-R.test  > /dev/null
	printf 'a\tb\tc\td\te\nx\ny\tz\n\tq\tr\n' | $(p) -kO3 --test-counts  2> o.counts-omega.test  > /dev/null
	{ printf '#a\n#b\n'; cat big.txt; } | $(p) -kO5 -c --tc --test-counts 2> o.counts-big-omega.test > /dev/null
	printf 'a\tb\tc\td\te\nx\ny\tz\n\tq\tr\n' | $(p) -kO3 --test-i 2   > o.omega-ti2.test
	printf 'a\tb\tc\td\te\nx\ny\tz\n\tq\tr\n' | $(p) -kO3 --test-i 3   > o.omega-ti3.test
	tail -n +2 big.txt | $(p) -kO5  --test-i 5                     > o.big-omega5-ti5.test
	tail -n +2 big.txt | $(p) -kO1  --test-i 1                     > o.big-omega1-ti1.test
	tail -n +2 big.txt | $(p) -kO25 --test-i 25                    > o.big-omega25-ti25.test
	tail -n +2 big.txt | $(p) -kO25 --test-counts  2> o.counts-big-omega25.test > /dev/null

%.ref : %.test
	diff $< $@

clean:
	rm -f o.*.test
