SHELL=/bin/bash

# Tests in the style of ../../pick/test/Makefile: each invocation writes
# o.<name>.test, which is compared against the stored o.<name>.ref .
# Run from the pluk directory with 'make test', which builds pluk first.

p=../target/release/pluk

.PHONY: test runpluk rundiff

test: runpluk rundiff
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

%.ref : %.test
	diff $< $@

clean:
	rm -f o.*.test
