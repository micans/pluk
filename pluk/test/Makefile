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
	printf 'a\tb\ta\tb\n1\t2\t3\t4\n'  | $(p)     2> o.dupnames.test  > /dev/null; [ $$? -eq 1 ]
	printf 'a\t_\nr\t1\t2\n'           | $(p) -R  2> o.dupnames-R.test > /dev/null; [ $$? -eq 1 ]
	printf 'a\tb\ta\tb\n1\t2\t3\t4\n'  | $(p) -u                 > o.dupnames-u.test
	{ head -n 1 data.txt | sed 's/fib2/fib/'; tail -n +2 data.txt; } | $(p) -uc > o.dupnames-u-count.test
	$(p) name nosuch            < data.txt  2> o.sel-unknown.test    > /dev/null; [ $$? -eq 1 ]
	$(p) -k 14                  < data.txt  2> o.sel-unknown-k.test  > /dev/null; [ $$? -eq 1 ]
	head -n 1 data.txt | $(p) nosuch        2> o.sel-unknown-headeronly.test > /dev/null; [ $$? -eq 1 ]
	$(p) nosuch                 < /dev/null  > o.sel-empty.test
	for a in '+fo.' 'a:b' '@x/gt/0' 'x%3A' 'x::y'; do $(p) "$$a" < data.txt; [ $$? -eq 1 ] || exit 1; done 2> o.sel-unsupported.test
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
	$(p) --test-names           < data.txt   > o.names.test
	$(p) -k --test-names        < data.txt   > o.names-k.test
	$(p) --test-names           < big.txt    > o.names-big.test
	printf 'x\n' | $(p) -kO4 --test-names   > o.names-omega.test
	$(p) --test-names           < /dev/null  > o.names-empty.test
	printf 'a\tb\nr\t1\t2\n'    | $(p) -R --test-names  > o.names-R.test
	printf '\ta\tb\nr\t1\t2\n'  | $(p) -R --test-names  > o.names-R-emptyfirst.test
	printf '\ta\tb\nr\t1\t2\n'  | $(p) --test-names    > o.names-emptyfirst.test
	printf 'foo\tfoo\tfoo_v02\tbar\tfoo\n' | $(p) -u --test-names  > o.names-u-restart.test
	printf 'a\tb\ta\ta\tb\n'   | $(p) -u --test-names   > o.names-u.test
	printf 'x\tx\tx_v02\tx_v002\n' | $(p) -u --test-names  > o.names-u-restart2.test
	printf 'a\t_\nr\t1\t2\n'    | $(p) -Ru --test-names  > o.names-Ru.test
	$(p) --test-select dna name fib          < data.txt  > o.sel.test
	$(p) --test-select fib fib name fib      < data.txt  > o.sel-dup.test 2> o.sel-dup-msg.test
	$(p) -k --test-select 13 1               < data.txt  > o.sel-k.test
	$(p) --test-select id x dna n1           < big.txt   > o.sel-big.test
	tail -n +2 big.txt | $(p) -kO12 --test-select 12 1   > o.sel-omega.test
	printf 'a\tb\nr\t1\t2\n' | $(p) -R --test-select b _   > o.sel-R.test
	printf 'log2(FC)\tcost($$)\n1\t2\n' | $(p) --test-select 'cost($$)' 'log2(FC)'  > o.sel-regexchars.test
	printf 'a\tb\ta\n1\t2\t3\n' | $(p) -u --test-select a_v02 a  > o.sel-u.test

%.ref : %.test
	diff $< $@

clean:
	rm -f o.*.test
