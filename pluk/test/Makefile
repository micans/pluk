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
	{ gzip -c data.txt; gzip -c big.txt; } | $(p)   > o.multigz.test
	printf 'a\tb\nno newline at end' | $(p)  > o.nonewline.test
	printf 'caf\xe9\t\xff\xfe\n' | $(p)      > o.nonutf8.test
	$(p)                  < /dev/null        > o.empty.test
	seq 1 1000000 > seq.tmp; $(p) < seq.tmp | head -n 2 > o.brokenpipe.test; [ $${PIPESTATUS[0]} -eq 0 ]; rm seq.tmp

# Tests for temporary --test-* options. When such an option is removed, remove
# the target from the 'test' prerequisites (keep the tests) and note it in the
# plan step file. See plan/guidelines.txt.

runtestopt: data.txt
	{ printf '# c1\n#c2\n';  cat data.txt; } | $(p) --test-comment          > o.tc-skip.test
	{ printf '# c1\n#c2\n';  cat data.txt; } | $(p) --tc                    > o.tc-alias.test
	{ printf '# c1\n#c2\n';  cat data.txt; } | $(p)                         > o.tc-noskip.test
	{ printf '# c1\n';  head -n 3 data.txt; printf '# mid\n'; tail -n 2 data.txt; } | $(p) --tc > o.tc-midfile.test
	{ printf '# c1\n#c2\n';  cat data.txt; } | gzip -c | $(p) --tc          > o.tc-gz.test
	printf '#a\n#b\n' | $(p) --tc                                          > o.tc-allcomment.test
	$(p) --test-i 1                         < data.txt  > o.ti-first.test
	$(p) --test-i 13                        < data.txt  > o.ti-last.test
	$(p) --test-i 14                        < data.txt  > o.ti-beyond.test
	$(p) --test-i 18                        < big.txt   > o.ti-big.test
	gzip -c big.txt | $(p) --test-i 18                  > o.ti-big-gz.test
	{ printf '# c1\n';  cat data.txt; } | $(p) --tc --test-i 2  > o.ti-tc.test
	printf 'a\tb\n\n\tz\nx\t\ty\n' | $(p) --test-i 2         > o.ti-empty.test

%.ref : %.test
	diff $< $@

clean:
	rm -f o.*.test
