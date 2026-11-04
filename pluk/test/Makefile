SHELL=/bin/bash

# Tests in the style of ../../pick/test/Makefile: each invocation writes
# o.<name>.test, which is compared against the stored o.<name>.ref .
# Run from the pluk directory with 'make test', which builds pluk first.

p=../target/release/pluk

.PHONY: test runpluk runtestopt rundiff runretired

test: runpluk runtestopt rundiff
	@echo "--- All tests passed ---"

# Reference files produced only by retired invocations (target runretired).
RETIRED_REFS = o.tc-midfile.ref

rundiff: $(filter-out $(RETIRED_REFS), $(wildcard *.ref))

runpluk: data.txt big.txt sam.txt
	$(p) --test-lineno                  < data.txt         > o.lineno.test
	$(p) --test-lineno                  < big.txt          > o.big-lineno.test
	gzip -c big.txt | $(p) --test-lineno                   > o.big-lineno-gz.test
	{ gzip -c data.txt; gzip -c data.txt; } | $(p) --test-lineno  > o.multigz.test
	printf 'a\tb\nno newline\tat end' | $(p) --test-lineno > o.nonewline.test
	printf 'caf\xe9\t\xff\xfe\n' | $(p) --test-lineno      > o.nonutf8.test
	$(p) --test-lineno                  < /dev/null        > o.empty.test
	$(p) -c                     < data.txt   > o.count.test
	$(p) -kc                    < data.txt   > o.count-k.test
	$(p) -c                     < big.txt    > o.big-count-all.test
	$(p) -c                     < /dev/null  > o.count-empty.test
	head -n 1 data.txt | $(p) -c             > o.count-headeronly.test
	gzip -c big.txt | $(p) -kc               > o.count-gz.test
	$(p) --test-lineno -O3 < data.txt > /dev/null 2> o.omega-needs-k.test; [ $$? -eq 1 ]
	printf 'a\tb\nr1\t1\t2\nr2\t3\t4\n'  | $(p) --test-lineno -R          > o.R-short.test
	printf '\ta\tb\nr1\t1\t2\n'           | $(p) --test-lineno -R          > o.R-emptyfirst.test
	printf '\ta\tb\nr1\t1\t2\n'           | $(p) --test-lineno             > o.emptyfirst.test
	printf 'x\ta\tb\nr1\t1\t2\n'  | $(p) --test-lineno -R  2> o.R-full.test       > /dev/null; [ $$? -eq 1 ]
	printf 'a\tb\nr1\t1\t2\n'      | $(p) --test-lineno     2> o.short-header.test > /dev/null; [ $$? -eq 1 ]
	printf 'a\tb\tc\n1\t2\n'        | $(p) --test-lineno     2> o.long-header.test  > /dev/null; [ $$? -eq 1 ]
	printf '1\t2\n3\n'               | $(p) --test-lineno -k  2> o.k-ragged.test     > /dev/null; [ $$? -eq 1 ]
	printf 'a\tb\n1\t2\n\n'          | $(p) --test-lineno     2> o.emptyline.test    > /dev/null; [ $$? -eq 1 ]
	sed '50s/$$/\textra/' big.txt     | $(p) --test-lineno     2> o.big-badline.test  > /dev/null; [ $$? -eq 1 ]
	gzip -c big.txt | $(p) -c                                  > o.big-valid-gz.test
	$(p) --test-lineno -kR                    < data.txt  2> o.kR.test  > /dev/null; [ $$? -eq 1 ]
	tail -n +2 big.txt | $(p) -kO5 -c                  > o.big-omega-count.test
	printf 'a\tb\tc\td\te\nx\ny\tz\n\tq\tr\n' | $(p) --test-lineno -kO3  > o.omega-lineno.test
	printf 'a\tb\ta\tb\n1\t2\t3\t4\n'  | $(p) --test-lineno     2> o.dupnames.test  > /dev/null; [ $$? -eq 1 ]
	printf 'a\t_\nr\t1\t2\n'           | $(p) --test-lineno -R  2> o.dupnames-R.test > /dev/null; [ $$? -eq 1 ]
	printf 'a\tb\ta\tb\n1\t2\t3\t4\n'  | $(p) --test-lineno -u                 > o.dupnames-u.test
	{ head -n 1 data.txt | sed 's/fib2/fib/'; tail -n +2 data.txt; } | $(p) -uc > o.dupnames-u-count.test
	$(p) name nosuch            < data.txt  2> o.sel-unknown.test    > /dev/null; [ $$? -eq 1 ]
	$(p) -k 14                  < data.txt  2> o.sel-unknown-k.test  > /dev/null; [ $$? -eq 1 ]
	head -n 1 data.txt | $(p) nosuch        2> o.sel-unknown-headeronly.test > /dev/null; [ $$? -eq 1 ]
	$(p) nosuch                 < /dev/null  > o.sel-empty.test
	for a in '+fo.' 'a:b' '@x/gt/0' 'x%3A' 'x::y'; do $(p) "$$a" < data.txt; [ $$? -eq 1 ] || exit 1; done 2> o.sel-unsupported.test
	$(p) sq fib                      < data.txt   > o.sq-fib.test        # from pick/test
	$(p) -A                          < data.txt   > o.-A.test            # from pick/test
	$(p) -x                          < data.txt   > o.-x.test            # from pick/test
	$(p) dna name fib                < data.txt   > o.out-sel.test
	$(p) -h name fib                 < data.txt   > o.out-h.test
	$(p) fib fib name fib            < data.txt   > o.out-dup.test 2> /dev/null
	$(p) name fib fib2 sq fx ud ms num1 num2 num4 foo11 abc dna  < data.txt  > o.out-all-inorder.test
	$(p) -k 13 1 2                   < data.txt   > o.out-k.test
	$(p) -kh 3                       < data.txt   > o.out-kh.test
	$(p) id x dna n1                 < big.txt    > o.out-big.test
	$(p) -h s2 s1 flag               < big.txt    > o.out-big-h.test
	gzip -c big.txt | $(p) x id                   > o.out-big-gz.test
	tail -n +2 big.txt | $(p) -kO12 12 1          > o.out-omega12.test
	tail -n +2 big.txt | $(p) -kO25 25 1 3        > o.out-omega25.test
	printf 'a\tb\tc\td\nx\n' | $(p) -kO3 1 2 3  > o.out-omega-pad.test
	printf 'a\tb\nr1\t1\t2\n'   | $(p) -R _ a     > o.out-R.test
	printf '\ta\tb\nr1\t1\t2\n' | $(p) _ b        > o.out-emptyfirst.test
	head -n 1 data.txt | $(p) name fib            > o.out-headeronly.test
	printf 'a\tb\ta\n1\t2\t3\n' | $(p) -u a_v02 a b  > o.out-u.test
	$(p) < data.txt > o.out-usage.test 2>&1; [ $$? -eq 0 ]
	$(p) -A fib                      < data.txt   > o.A-names.test
	$(p) -A name name                < data.txt   > o.A-dupnames.test 2> o.A-dupnames-msg.test
	$(p) -x fib name                 < data.txt   > o.x-names.test
	$(p) -x dna fib fib              < data.txt   > o.x-dup.test
	$(p) -Ax fib                     < data.txt   > o.Ax.test
	$(p) -kx 1                       < data.txt   > o.kx.test
	$(p) -Ah                         < data.txt   > o.Ah.test
	$(p) -x name fib fib2 sq fx ud ms num1 num2 num4 foo11 abc dna  < data.txt  > o.x-all.test
	$(p) -A                          < big.txt    > o.A-big.test
	$(p) -hx id                      < big.txt    > o.x-big-h.test
	gzip -c big.txt | $(p) -x dna s1 s2           > o.x-big-gz.test
	printf 'a\tb\tc\td\nx\n' | $(p) -kO3 -A     > o.A-omega.test
	printf 'a\tb\tc\td\nx\n' | $(p) -kO3 -x 2   > o.x-omega.test
	tail -n +2 big.txt | $(p) -kO12 -A            > o.A-omega12.test
	printf 'a\tb\nr1\t1\t2\n' | $(p) -RA        > o.A-R.test
	printf 'a\tb\nr1\t1\t2\n' | $(p) -R -x a    > o.x-R.test
	$(p) -A                          < /dev/null  > o.A-empty.test
	$(p) -x nosuch                   < data.txt  2> o.x-unknown.test > /dev/null; [ $$? -eq 1 ]
	$(p) -A nosuch                   < data.txt  2> o.A-unknown.test > /dev/null; [ $$? -eq 1 ]
	$(p) --sam -A                    < sam.txt    > o.sam-A.test        2> o.sam-A-msg.test
	$(p) --sam-h -A                  < sam.txt    > o.samh-A.test       2> /dev/null
	$(p) --sam 1 2 12                < sam.txt    > o.sam-sel.test      2> /dev/null
	$(p) --sam -x 10 11              < sam.txt    > o.sam-x.test        2> /dev/null
	$(p) --sam -c                    < sam.txt    > o.sam-count.test    2> /dev/null
	$(p) --sam-h -c                  < sam.txt    > o.samh-count.test   2> /dev/null
	$(p) --sam-h 1 4                 < sam.txt    > o.samh-sel.test     2> /dev/null
	gzip -c sam.txt | $(p) --sam -A               > o.sam-gz.test       2> /dev/null
	$(p) --sam -O3 3                 < sam.txt    > o.sam-O3.test       2> /dev/null
	$(p) --sam -R 1                  < sam.txt   2> o.sam-R.test   > /dev/null; [ $$? -eq 1 ]
	$(p) --sam/sam.fa -A             < sam.txt    > o.samref-A.test     2> /dev/null
	$(p) --sam-h/sam.fa 1 4          < sam.txt    > o.samref-h-sel.test 2> /dev/null
	$(p) -Ahk                        < data.txt   > o.opt-bundle.test
	$(p) -A-1                        < data.txt   > o.opt-A-neg.test
	$(p) -- name fib                 < data.txt   > o.opt-dashdash.test
	$(p) name -h                     < data.txt  2> o.opt-col-dash.test > /dev/null; [ $$? -eq 1 ]
	$(p) -kO3 --sam 3                < sam.txt    > o.opt-order-sam.test 2> /dev/null
	for a in -kO -kO0 -kw --nosuch -//a/b -/a/b -/ -// --test-i=0 --sam/ -A99999999999999999999; do $(p) "$$a" < data.txt; [ $$? -eq 1 ] || exit 1; done 2> o.opt-errors.test
	{ printf '# c1\n#c2\n'; head -n 4 data.txt; printf '# mid\n'; tail -n 2 data.txt; } | $(p) '-/^#' name fib   > o.pat-purge.test
	{ printf '# c1\n#c2\n'; head -n 4 data.txt; printf '# mid\n'; tail -n 2 data.txt; } | $(p) '-//^#' name fib  > o.pat-pass.test
	{ printf '# c1\n#c2\n'; head -n 4 data.txt; printf '# mid\n'; tail -n 2 data.txt; } | $(p) '-//^#' -c       > o.pat-pass-count.test
	$(p) '-/^Bob//^Ann' name fib     < data.txt   > o.pat-both.test
	$(p) '-/A//An' name fib          < data.txt   > o.pat-pass-first.test
	$(p) '-/(?i)^ann' name           < data.txt   > o.pat-regex.test
	$(p) '-/\d\d' name ms            < data.txt   > o.pat-header.test
	printf 'a\tb\n\xe9x\t1\nzz\t2\n' | $(p) '-/^.x' a   > o.pat-bytes.test
	printf 'a\tb\ncaf\xc3\xa9\t1\nzz\t2\n' | $(p) '-/\xc3\xa9' a   > o.pat-utf8.test
	gzip -c data.txt | $(p) '-/^C' name           > o.pat-gz.test
	$(p) --sam '-/\tchr2\t' -A          < sam.txt    > o.pat-sam.test       2> /dev/null
	$(p) --sam-h '-/^@SQ' -A         < sam.txt    > o.pat-samh.test      2> /dev/null
	$(p) '-/^r00' --sam-h 1          < sam.txt    > o.pat-samh-purge.test 2> /dev/null
	for a in '-/(' '-/%FF'; do $(p) "$$a" name < data.txt; [ $$? -eq 1 ] || exit 1; done 2> o.pat-errors.test
	$(p) -H                          < /dev/null  > o.help.test
	$(p) --help                      < /dev/null  > o.help-long.test
	$(p) -kH                         < data.txt  2> o.help-bundle.test > /dev/null; [ $$? -eq 1 ]
	seq 1 1000000 > seq.tmp; $(p) --test-lineno < seq.tmp | head -n 2 > o.brokenpipe.test; [ $${PIPESTATUS[0]} -eq 0 ]; rm seq.tmp

# Tests for temporary --test-* options. When such an option is removed, remove
# the target from the 'test' prerequisites (keep the tests) and note it in the
# plan step file. See plan/guidelines.txt.

runtestopt: data.txt
	{ printf '# c1\n#c2\n';  cat data.txt; } | $(p) --test-lineno '-/^#'          > o.tc-skip.test
	{ printf '# c1\n#c2\n';  cat data.txt; } | $(p) --test-lineno '-/^%23'                    > o.tc-alias.test
	{ printf '# c1\n#c2\n';  cat data.txt; } | $(p) --test-lineno -kO13                   > o.tc-noskip.test
	{ printf '# c1\n';  head -n 3 data.txt; printf '# mid\n'; tail -n 2 data.txt; } | $(p) --test-lineno -kO13 '-/^#' > o.pat-midfile.test
	{ printf '# c1\n#c2\n';  cat data.txt; } | gzip -c | $(p) --test-lineno '-/^#'          > o.tc-gz.test
	printf '#a\n#b\n' | $(p) --test-lineno '-/^#'                                          > o.tc-allcomment.test
	$(p) --test-i=1                         < data.txt  > o.ti-first.test
	$(p) --test-i=13                        < data.txt  > o.ti-last.test
	$(p) --test-i=14                        < data.txt  > o.ti-beyond.test
	$(p) --test-i=18                        < big.txt   > o.ti-big.test
	gzip -c big.txt | $(p) --test-i=18                  > o.ti-big-gz.test
	{ printf '# c1\n';  cat data.txt; } | $(p) '-/^#' --test-i=2  > o.ti-tc.test
	printf 'a\tb\n\n\tz\nx\t\ty\n' | $(p) -kO3 --test-i=2    > o.ti-empty.test

	$(p) --test-lineno -k                     < data.txt   > o.k-lineno.test
	{ printf '#c\n';  cat data.txt; } | $(p) -c '-/^#'   > o.tc-count.test
	$(p) --test-lineno --test-counts          < data.txt   2> o.counts-header.test > /dev/null
	$(p) --test-lineno -k --test-counts       < data.txt   2> o.counts-k.test      > /dev/null
	$(p) --test-lineno --test-counts          < /dev/null  2> o.counts-empty.test  > /dev/null
	printf 'a\tb\nr1\t1\t2\n' | $(p) --test-lineno -R --test-counts  2> o.counts-R.test  > /dev/null
	printf 'a\tb\tc\td\te\nx\ny\tz\n\tq\tr\n' | $(p) --test-lineno -kO3 --test-counts  2> o.counts-omega.test  > /dev/null
	{ printf '#a\n#b\n'; cat big.txt; } | $(p) -kO5 -c '-/^#' --test-counts 2> o.counts-big-omega.test > /dev/null
	printf 'a\tb\tc\td\te\nx\ny\tz\n\tq\tr\n' | $(p) -kO3 --test-i=2   > o.omega-ti2.test
	printf 'a\tb\tc\td\te\nx\ny\tz\n\tq\tr\n' | $(p) -kO3 --test-i=3   > o.omega-ti3.test
	tail -n +2 big.txt | $(p) -kO5  --test-i=5                     > o.big-omega5-ti5.test
	tail -n +2 big.txt | $(p) -kO1  --test-i=1                     > o.big-omega1-ti1.test
	tail -n +2 big.txt | $(p) -kO25 --test-i=25                    > o.big-omega25-ti25.test
	tail -n +2 big.txt | $(p) --test-lineno -kO25 --test-counts  2> o.counts-big-omega25.test > /dev/null
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
	$(p) --sam-h -A --test-counts    < sam.txt    2> o.counts-sam.test  > /dev/null
	$(p) --test-dict --fd-x=data.txt                        < /dev/null  > o.dict-fd.test
	$(p) --test-dict --kdict-k=<(gzip -c data.txt)          < /dev/null  > o.dict-kd-gz.test
	$(p) --test-dict --fdict-b=big.txt --kd-c=big.txt       < /dev/null  > o.dict-big.test
	$(p) --test-dict --fd-x=<(printf 'a\nb\tB\tC\n\tE\n') < /dev/null  > o.dict-fd-fields.test
	$(p) --test-dict --cd-c/a%3Db=a:A,b,%41:%2C, --cdict-e/=x:  < /dev/null  > o.dict-cd.test
	$(p) --test-dict --kd-a/__EXIT__=data.txt --kd-b/%5F_EXIT__=data.txt  < /dev/null  > o.dict-exit.test
	$(p) --test-dict --fd-x/d1=data.txt --cd-x=Ann:Z,new:N --cd-y=a --cd-x/d2=q  < /dev/null  > o.dict-merge.test 2> o.dict-merge-msg.test
	$(p) --test-dict --cd-x/d1=a --cd-x/__EXIT__=b --cd-x/d2=c --cd-x=d  < /dev/null  > o.dict-merge-exit.test 2> /dev/null
	for a in --fd-x --fd-=f --fd-x= --fd-x/d --kd-x+=f --cdict-x --cd-x=a:b:c --fd-x=/nonexistent; do $(p) --test-dict "$$a" < /dev/null; [ $$? -eq 1 ] || exit 1; done 2> o.dict-errors.test
	$(p) --test-dict --td=data.txt                          < /dev/null  > o.tdict.test
	$(p) --test-dict '--tdict^dna/DF=data.txt'              < /dev/null  > o.tdict-key.test
	$(p) --test-dict --table-dict^id/__EXIT__=<(cut -f 17,1,20 big.txt | gzip -c)  < /dev/null  > o.tdict-big-gz.test
	$(p) --test-dict --td^b%2Fc=<(printf 'a\tb/c\n1\tk\n')  < /dev/null  > o.tdict-keyenc.test
	$(p) --test-dict --fd-fib=<(printf 'zz\t9\n') --td/__EXIT__=data.txt --cd-fib/x=Q:1  < /dev/null  > o.tdict-merge.test 2> o.tdict-merge-msg.test
	printf 'a\tb\n' | $(p) --test-dict --td=/dev/stdin     > o.tdict-headeronly.test
	printf 'a\n1\n2\n' | $(p) --test-dict --td=/dev/stdin  > o.tdict-keyonly.test
	for a in '--td^fib=data.txt' '--td^nosuch=data.txt' --td=/dev/null --td-x=f --tdict-x=f --td/d^dna=data.txt '--td^=f' --td^k --td=; do $(p) --test-dict "$$a" < /dev/null; [ $$? -eq 1 ] || exit 1; done 2> o.tdict-errors.test
	for t in 'a\tb\tb\nk\t1\t2\n' 'a\tb\nk\t1\t2\n' 'a\tb\nk\t1\nk\t2\n'; do printf "$$t" | $(p) --test-dict --td=/dev/stdin; [ $$? -eq 1 ] || exit 1; done 2> o.tdict-errors-file.test
	$(p) --test-dict --fasta-dict-f=<(printf '\n>s1 description\nACGT\nTTGG\n\nCC\n>s2\n>s3\tx\nGATTACA\n')  < /dev/null  > o.fadict.test
	$(p) --test-dict --fasta-dict-g/NF=<(printf '>a\nAC\nGT\n>b\nTT' | gzip -c)  < /dev/null  > o.fadict-gz.test
	printf '>a\nAC\n' > fa.tmp; $(p) --test-dict --cd-f/d=a:1,z:2 --fasta-dict-f=fa.tmp --cd-f/d2=y:3  < /dev/null  > o.fadict-merge.test 2> o.fadict-merge-msg.test; rm fa.tmp
	for t in '>\nAC\n' '> s\nAC\n' '>a\nA\n>a\nC\n' 'AC\n>a\n'; do printf "$$t" > fa.tmp; $(p) --test-dict --fasta-dict-e=fa.tmp < /dev/null; [ $$? -eq 1 ] || exit 1; done 2> o.fadict-errors-file.test; rm fa.tmp
	for a in --fasta-dict-=f --fasta-dict-x --fasta-dict-x= --fasta-dict-x=/nonexistent; do $(p) --test-dict "$$a" < /dev/null; [ $$? -eq 1 ] || exit 1; done 2> o.fadict-errors.test
	$(p) --test-dict --sam/sam.fa                           < /dev/null  > o.samref-dict.test
	$(p) --test-dict --sam-h/<(gzip -c sam.fa) --fd-x=data.txt  < /dev/null | grep '^dict'  > o.samref-dict-gz.test
	printf '>chr3\nAC\n>chr1\nT\n' > fa.tmp; $(p) --test-dict --sam/sam.fa --sam-h/fa.tmp  < /dev/null  > o.samref-merge.test 2> o.samref-merge-msg.test; rm fa.tmp
	printf 'k\tSAMFA\n1\t2\n' > td.tmp; for a in --sam/ --sam-h/ --sam/nonexistent --fd-SAMFA=data.txt --fasta-dict-SAMFA/x=sam.fa --td=td.tmp; do $(p) --test-dict "$$a" < /dev/null; [ $$? -eq 1 ] || exit 1; done 2> o.samref-errors.test; rm td.tmp

%.ref : %.test
	diff $< $@

clean:
	rm -f o.*.test

# Retired invocations, no longer run (see plan/step3-notes.txt item 4).
# --test-comment/--tc was removed in step3 item 4; equivalent invocations using -/^#
# in runtestopt write the same .test files, except o.tc-midfile (--tc only skipped
# leading lines; -/^# skips all), replaced by o.pat-midfile.

runretired: data.txt big.txt
	{ printf '# c1\n#c2\n';  cat data.txt; } | $(p) --test-lineno --test-comment          > o.tc-skip.test
	{ printf '# c1\n#c2\n';  cat data.txt; } | $(p) --test-lineno --tc                    > o.tc-alias.test
	{ printf '# c1\n';  head -n 3 data.txt; printf '# mid\n'; tail -n 2 data.txt; } | $(p) --test-lineno -kO13 --tc > o.tc-midfile.test
	{ printf '# c1\n#c2\n';  cat data.txt; } | gzip -c | $(p) --test-lineno --tc          > o.tc-gz.test
	printf '#a\n#b\n' | $(p) --test-lineno --tc                                          > o.tc-allcomment.test
	{ printf '# c1\n';  cat data.txt; } | $(p) --tc --test-i=2  > o.ti-tc.test
	{ printf '#c\n';  cat data.txt; } | $(p) -c --tc   > o.tc-count.test
	{ printf '#a\n#b\n'; cat big.txt; } | $(p) -kO5 -c --tc --test-counts 2> o.counts-big-omega.test > /dev/null
