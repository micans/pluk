SHELL=/bin/bash

# Tests in the style of ../../pick/test/Makefile: each invocation writes
# o.<name>.test, which is compared against the stored o.<name>.ref .
# Run from the pluk directory with 'make test', which builds pluk first.
# All invocations run, even if one fails (.IGNORE); rundiff then compares every
# reference and reports the tally (plan/step22.txt item 2). Every invocation reads
# a file or /dev/null (one that reads the terminal makes the run hang).
# make accept T='name ..' copies o.<name>.test to o.<name>.ref.

p=../target/release/pluk

.PHONY: test runpluk runtestopt rundiff runretired accept clean

test: runpluk runtestopt rundiff
	@echo "--- All tests passed ---"

# A failing invocation does not stop the others; its output is compared as usual.
.IGNORE: runpluk runtestopt

# Reference files produced only by retired invocations (target runretired).
RETIRED_REFS = o.tc-midfile.ref
REFS = $(filter-out $(RETIRED_REFS), $(wildcard *.ref))

# Every reference against its output, whatever the file times: a missing output
# or a difference is a failure; the first lines of each difference are shown.
rundiff:
	@failed=""; n=0; \
	for r in $(REFS); do \
	  n=$$((n + 1)); t=$${r%.ref}.test; \
	  if [ ! -f $$t ]; then echo "missing: $$t"; failed="$$failed $${r%.ref}"; \
	  elif ! cmp -s $$t $$r; then echo "differs: $$t $$r"; diff $$t $$r | head -n 20; failed="$$failed $${r%.ref}"; fi; \
	done; \
	nf=$$(echo $$failed | wc -w); \
	if [ $$nf -gt 0 ]; then echo "--- $$nf of $$n tests failed:$$failed ---"; exit 1; fi; \
	echo "--- $$n tests compared ---"

# make accept T='name ..': the output o.<name>.test becomes the reference.
accept:
	@if [ -z "$(T)" ]; then echo "usage: make accept T='name ..' (o.<name>.test becomes o.<name>.ref)"; exit 1; fi
	@for t in $(T); do t=$${t#o.}; t=$${t%.test}; t=$${t%.ref}; cp o.$$t.test o.$$t.ref && echo "accepted o.$$t.ref"; done

runpluk: data.txt big.txt big.keep.txt sam.txt sam-clean.txt sam.bam sam-clean.bgz bam-tags.bam bam-nosq.bam bam-cg.bam bam-empty.bam bam-sqdiff.bam
	rm -rf out && mkdir out
	rm -f o.*.test
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
	for a in 'x::y' '+x.*::^1'; do $(p) "$$a" < data.txt; [ $$? -eq 1 ] || exit 1; done 2> o.sel-unsupported.test
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
	{ cat sam.txt; printf '@SQ\tSN:chr9\tLN:5\n@SQ\tSN:chr8\tLN:5\n'; } | $(p) --sam -c  > o.seqlen-late.test  2>&1
	$(p) -A                          < sam.bam    > /dev/null 2> o.bam-nosam.test; [ $$? -eq 1 ]
	$(p) --sam -A                    < sam-clean.bgz  > o.sam-bgzf.test
	gzip -c sam-clean.txt | $(p) --sam -A           > o.sam-clean-gz.test
	$(p) --sam -A                    < sam.bam        > o.bam-sam.test
	$(p) --sam-h -A                  < sam.bam        > o.bam-samh.test
	$(p) --sam '-/\tchr2\t' 1 3 4    < sam.bam        > o.bam-pat.test
	$(p) --sam-h -A < bam-tags.bam  | sed 's/\t$$//'  > o.bam-tags.test
	$(p) --sam-h -A < bam-nosq.bam  | sed 's/\t$$//'  > o.bam-nosq.test
	$(p) --sam-h -A < bam-cg.bam    | sed 's/\t$$//'  > o.bam-cg.test
	$(p) --sam-h -A < bam-empty.bam | sed 's/\t$$//'  > o.bam-empty.test
	$(p) --sam -A < bam-sqdiff.bam > /dev/null 2> o.bam-sqdiff.test; [ $$? -eq 1 ]
	for n in 200 400; do head -c $$n bam-tags.bam | $(p) --sam -c; [ $$? -eq 1 ] || exit 1; done 2> o.bam-truncated.test
	$(p) --threads=1 --sam-h -A < bam-tags.bam | sed 's/\t$$//'  > o.bam-tags-t1.test
	$(p) --threads=2 --sam-h -A < bam-tags.bam | sed 's/\t$$//'  > o.bam-tags-t2.test
	for n in 200 400; do head -c $$n bam-tags.bam | $(p) --threads=1 --sam -c; [ $$? -eq 1 ] || exit 1; done 2> o.bam-truncated-t1.test
	for a in --threads=0 --threads=x --threads= --threads=-2; do $(p) "$$a" -A < data.txt; [ $$? -eq 1 ] || exit 1; done 2> o.threads-errors.test
	$(p) --threads=3 -c               < data.txt        > o.threads-text.test
	$(p) fib @fib~le~5 @fib~gt~20   < data.txt   > o.strcmp.test
	$(p) -o id n1 f1 @n1/lt/-40 @f1/ge/0.95  < big.txt  > o.big-or.test
	$(p) id n1 f1 @n1/lt/-40 @f1/ge/0.95     < big.txt  > o.big-and.test
	{ for a in @ud/lt/0 @ud/le/-2 @ud/eq/1 @ud/ne/1 @ud/ge/1.0 @ms/gt/0.2 @ms/le/%2E25; do echo "$$a"; $(p) -h name ud ms "$$a" < data.txt; done; } > o.filter-num.test
	{ for a in @fib~lt~3 @fib~eq~13 @fib~ne~13 @fib~ge~5 @fx~gt~d @fx~eq~ @foo11~le~b_o_b; do echo "$$a"; $(p) -h name fib fx foo11 "$$a" < data.txt; done; } > o.filter-str.test
	$(p) -h name fib fib2 @fib/gt/:fib2 @@fx~ne~:name  < data.txt  > o.filter-column.test
	{ $(p) -s name ud @@ud/eq/1 @@ud/eq/-2 < data.txt; $(p) -s name ud @@ud/eq/1 @ud/eq/-2 < data.txt; $(p) -so name ud @ud/eq/1 @ud/eq/-2 < data.txt; } > o.filter-or.test
	$(p) -c @f1/lt/0.5 @@n2/ge/0     < big.txt    > o.filter-count.test
	$(p) -k 1 2 @2/gt/3              < data.txt   > o.filter-k.test
	printf 'x\ty\n\ta\nfox\tb\n3\tc\n1e1\td\n0x10\te\n 4\tf\ninf\tg\nnan\th\n' | $(p) x y @x/lt/5 @@y~ne~a > o.filter-na.test 2> o.filter-na-msg.test
	printf 'x\ty\n\ta\nfox\tb\n3\tc\n1e1\td\ninf\tg\nnan\th\n' | $(p) x y @x/ne/5    > o.filter-ne.test 2>&1
	printf 'a\tb\n9223372036854775807\t9223372036854775806\n99999999999999999999\t9223372036854775807\n1e3\t1000\n-0\t0\n' | $(p) -h a b @a/gt/:b > o.filter-int.test
	printf 'a\tb\n9223372036854775807\t9223372036854775806\n99999999999999999999\t9223372036854775807\n1e3\t1000\n-0\t0\n' | $(p) -h a b @a/eq/:b > o.filter-int-eq.test
	$(p) name ud @ud~eq~fx @@ud~eq~^fx  < data.txt   > o.filter-hint.test 2>&1
	for a in @x/gt/abc @x/foo/3 @x/gt/3/1 @/gt/3 @+/gt/3 @assert@x/gt/3 @x @x/gt/:; do $(p) name "$$a" < data.txt; [ $$? -eq 1 ] || exit 1; done 2> o.filter-errors.test
	{ for a in @fx=dog @fx/=dog @fx= @fx/= @@foo11~_o_ @foo11/~_o_ '@fx~^[a-c]' '@fx~x|y' @name~^%5EB '@dna~(?i)^acg' '@foo11~^\w+$$' @foo11~%2F; do echo "$$a"; $(p) -h name fx foo11 dna "$$a" < data.txt; done; } > o.filter-regex.test
	printf 'x\ty\nab.cd\t.\naxbcd\t.\nab|cd\tb|c\nabcd\tb|c\nabcd\t\n\t\n' | $(p) -h x y @x~:y   > o.filter-regex-column.test
	printf 'x\ty\nab.cd\t.\naxbcd\t.\nab|cd\tb|c\nabcd\tb|c\nabcd\t\n\t\n' | $(p) -h x y @x/~:y  > o.filter-regex-column-neg.test
	$(p) -c @grp/=B @@tag~^k         < big.txt    > o.filter-regex-count.test
	for a in @x~ @x/~ @x~^ '@x~a(' '@x~(.)\1' '@x~a(?=b)' @x~a/b @x=a~b @x=3/4; do $(p) name "$$a" < data.txt; [ $$? -eq 1 ] || exit 1; done 2> o.filter-regex-errors.test
	$(p) -A --kdict-K=big.keep.txt @id~isin~K       < big.txt  > o.big-isin.test
	{ for a in @id~isnotin~K @@id~isin~K @grp~hasval~G~2 @grp~hasval~G~ @grp~hasnotval~G~2 @grp~isin~G @grp~isnotin~G @grp~hasval~G~%3A2 @n1~isin~Z; do echo "$$a"; $(p) -c --kd-K=big.keep.txt --cd-G/zz=A:2,B:,C:3,D:%3A2 --cd-Z/__EXIT__=0:1 "$$a" < big.txt; done; } > o.filter-dict.test
	{ for pair in '@id~isnotin~K @id~notin~K' '@@id~isnotin~K @@id~notin~K' '@grp~hasnotval~G~2 @grp~notval~G~2' '@grp~hasnotval~G~ @grp~notval~G~' '@grp:id~isnotin~G @grp:id~notin~G' '@grp~isnotin~G @grp~notin~G'; do set -- $$pair; echo "$$1 $$2"; $(p) -c --kd-K=big.keep.txt --cd-G/zz=A:2,B:,C:3,D:%3A2 "$$1" < big.txt; $(p) -c --kd-K=big.keep.txt --cd-G/zz=A:2,B:,C:3,D:%3A2 "$$2" < big.txt; done; $(p) --kd-K=big.keep.txt id 'a::id^K^~isnotin~,test' 'b::id^K^~notin~,test' < big.txt | md5sum; $(p) --kd-K=big.keep.txt 'a::id^K^~notval~,test' < big.txt; echo "exit $$?"; $(p) -c '@id~notin~NOPE' < big.txt; echo "exit $$?"; $(p) -l notin notval < /dev/null; } > o.filter-synonyms.test 2>&1
	{ yes < /dev/null | head -n 10 | $(p) -k --pstore/x:1,y:0 'x::^y,pload' 'y::x^x,pload,add'; for a in '--pstore//0 n::^n,pload,incr' '--pstore//0 name prev::^name,pload sum::^sum,pload:fib,add' '--pstore//0 name n::^n,pload,incr @fib/gt/2' '--pstore//0 name n::^n,pload,incr @@fib/gt/2' '--pstore/fib:100 name fib p::^fib,pload' '-i --pstore//0 name fib fib::^fib,pload:fib,add' '--pstore name n::^n,pload,incr p::^name,pload' '--pstore/name:%2Fx/%3A name p::^name,pload q::^fib,pload'; do echo "== $$a"; $(p) $$a < data.txt; done; $(p) --pstore//0 -c 'n::^n,pload,incr' '@n/gt/3' < data.txt; $(p) --pstore//0 --other=out/ps.other name 'n::^n,pload,incr' '@n/gt/9' < data.txt; cat out/ps.other; for a in "name p::^name,pload" "--pstore p::name,pload" "--pstore p::^nope,pload" "--pstore/nope:1 name" "--pstore/nope name"; do $(p) $$a < data.txt; echo "exit $$?"; done; $(p) --pstore -l pload < /dev/null; } > o.pstore.test 2>&1
	{ printf 'gene\texon_start\texon_end\nA\t100\t200\nA\t300\t400\nA\t500\t600\nB\t50\t80\nC\t10\t20\nC\t40\t60\n' > out/exons.txt; for a in '--group=gene gene intron_start::^exon_end,pload,incr intron_end::exon_start,decr' '--group-first-ref=gene gene exon_start first::^exon_start,pload' '--group=gene gene exon_start i::,groupi g::,groupno' '--group-first-ref=gene gene i::,groupi g::,groupno' '--group=gene -A @exon_start/gt/400' '--group=gene -A @@exon_start/gt/40' '--group=k gene k::exon_start^100,idiv i::,groupi' '--pstore//0 --group=gene gene n::^n,pload,incr' '--pstore/exon_end:0 --group=gene gene p::^exon_end,pload' '--group=gene -c gene' '-q --group=gene gene'; do echo "== $$a"; $(p) $$a < out/exons.txt; done; $(p) --group=gene --other=out/grp.other -A '@exon_start/lt/300' < out/exons.txt; cat out/grp.other; $(p) --group=gene --demux=gene --demux-dir=out -A < out/exons.txt; cat out/A out/C; for a in 'gene i::,groupi' 'gene g::,groupno' '--group=nope gene' '--group=gene --group-first-ref=gene gene' '--group= gene'; do $(p) $$a < out/exons.txt; echo "exit $$?"; done; } > o.group.test 2>&1
	{ printf '10.5\n20.5\n30.5\n40.5\n50.5\n60.5\n70.5\n80.5\n90.5\n' > out/q.txt; printf '5\n5\n5\n10\n' > out/qt.txt; echo 7 > out/q1.txt; gzip -c out/q.txt > out/q.txt.gz; seq 2 13 100 | $(p) -k -q --quant-Q=out/q.txt 1 'l::1^Q,qnl' 'u::1^Q,qnu'; seq 2 13 100 | $(p) -k -q --num-perl --quant-Q=out/q.txt.gz 1 'l::1^Q,qnl' 'u::1^Q,qnu' | head -n 3; printf '4\n5\n6\n10\n11\nfox\n-1e9\n' | $(p) -k --quant-T=out/qt.txt --quant-O=out/q1.txt 1 'l::1^T,qnl' 'u::1^T,qnu' 'o::1^O,qnl' 'p::1^O,qnu'; printf '1\n3\n2\n' > out/qbad1.txt; printf '1\n\n3\n' > out/qbad2.txt; : > out/qbad3.txt; printf '1\ninf\n' > out/qbad4.txt; for a in --quant-Q=out/qbad1.txt --quant-Q=out/qbad2.txt --quant-Q=out/qbad3.txt --quant-Q=out/qbad4.txt --quant-Q=out/nofile '--quant-Q=out/q.txt --quant-Q=out/q1.txt' --quant-=x --quant-Q=; do echo 5 | $(p) -k $$a 1; echo "exit $$?"; done; echo 5 | $(p) -k --quant-Q=out/q.txt 'a::1^R,qnl'; echo 5 | $(p) -k --quant-Q=out/q.txt 'a::1:1,qnl'; $(p) -l qn < /dev/null; } > o.quant.test 2>&1
	$(p) -o --kd-G=big.keep.txt --cd-H=r001:x id grp @id~isin~G @id~hasval~H~x  < big.txt  > o.filter-dict-or.test
	$(p) --sam 1 3 @3~isin~seqlen   < sam.txt   2> /dev/null > o.filter-dict-sam.test
	$(p) --sam 1 3 @3~isnotin~seqlen < sam.bam   > o.filter-dict-bam.test
	for a in @fx~isin~Q @fx~isin~ @fx~isin~:D @fx~isin~^D @fx~isin~D~v @fx~hasval~D @fx~hasval~D/v @fx~hasval~D~:y @fx~hasval~D~a~b; do $(p) --cd-D=a:b name "$$a" < data.txt; [ $$? -eq 1 ] || exit 1; done 2> o.filter-dict-errors.test
	$(p) id f3 f4 @f3/ep/:f4/20      < big.txt    > o.big-ep.test
	$(p) id n4 n5 @n4/om/:n5/1.5     < big.txt    > o.big-om.test
	{ for a in @ms/ep/0.25/0.05 @ms/epx/0.25/0.05 @ud/ep/1 @ud/ep/-2/3 @ud/om/-1 @ud/omx/-1 @ms/om/0.3/1.5 @fib/om/:fib2 @fib/omx/:fib2 @@fib/ep/:sq/2 @ms/om/0.2/inf; do echo "$$a"; $(p) -h name fib fib2 sq ud ms "$$a" < data.txt; done; } > o.filter-ep-om.test
	{ for a in @x/ep/:y/0 @x/ep/:y @x/om/:y @x/omx/:y/1; do echo "$$a"; printf 'x\ty\n0.14286\t0.142857\n0\t0\n0\t1\n-3\t3\n' | $(p) -h x y "$$a"; done; } > o.filter-ep-zero.test
	{ for a in @a/ep/:b/0 @a/ep/:b/1 @a/ep/:b/inf @a/om/:b @a/omx/:b; do echo "$$a"; printf 'a\tb\n9007199254740993\t9007199254740992\n9223372036854775807\t-9223372036854775808\n9223372036854775807\t4611686018427387904\n-9223372036854775808\t9223372036854775807\n1\t9223372036854775807\n' | $(p) -h a b "$$a"; done; } > o.filter-ep-int.test
	printf 'x\ty\n\t1\nfox\t1\n1\t\n5\t1\nnan\t1\n' | $(p) -h x y @x/epx/:y > o.filter-ep-na.test 2>&1
	for a in @x/ep/1/ @x/ep/1/-1 @x/ep/1/nan @x/ep/1/abc @x/ep/1~2 @x/ep/1/:y @x/om/1/0.5 @x/om/1/0 @x/omx/1/2/3 @x/ep/abc; do $(p) name "$$a" < data.txt; [ $$? -eq 1 ] || exit 1; done 2> o.filter-ep-errors.test
	$(p) id s1 flag @s1~lt~m @flag/all/5   < big.txt  > o.big-strbit.test
	{ for a in @fib/all/5 @fib/any/5 @fib/none/5 @fib/notall/5 @ud/all/1 @ud/any/12 @fib/all/0 @fib/any/0 @sq/all/:fib @@sq/any/:fib2 @sq/none/:fib2 @sq/notall/:fib; do echo "$$a"; $(p) -h name fib fib2 sq ud "$$a" < data.txt; done; } > o.filter-bit.test
	{ for a in @x/all/:m @x/notall/:m @x/any/:m; do echo "$$a"; printf 'x\tm\n-7\t5\n7\t-5\n-9223372036854775808\t-9223372036854775808\n2\t-5\n' | $(p) -h x m "$$a"; done; } > o.filter-bit-neg.test
	{ $(p) --sam 1 2 @2/any/16 < sam.txt; $(p) --sam 1 2 @2/none/4 < sam.txt; } > o.filter-bit-sam.test 2> o.filter-bit-sam-msg.test
	$(p) --sam 1 2 @2/any/16 @2/none/4  < sam.bam   > o.filter-bit-bam.test
	printf 'x\tm\n5\t1\n5.0\t1\n1e3\t8\n400000000000000000000000000000000000000000\t1\nfox\t1\n\t1\n7\t1.0\n7\t1\n' | { $(p) -h x m @x/all/:m; } > o.filter-bit-nonint.test 2>&1
	printf 'x\n5\n5.0\nfox\n4\n' | $(p) -h x @x/notall/1 > o.filter-bit-nonint-neg.test 2>&1
	for a in @x/all/5.0 @x/any/-1 @x/none/abc @x/notall/1e3 @x/all/ @x/all/5/1; do $(p) name "$$a" < data.txt; [ $$? -eq 1 ] || exit 1; done 2> o.filter-bit-errors.test
	{ for a in fib:fx '+num\d' '+f.*' 'fx:+num\d:name' '+(?i)FI.*' '+.*o.*' '+num(\d)' 'fib:fib:+fi.'; do echo "$$a"; $(p) "$$a" < data.txt 2>&1 | head -n 3; done; } > o.colspec.test
	{ $(p) -x '+num\d:fib' < data.txt | head -n 2; $(p) -A '+num\d' < data.txt | head -n 2; $(p) -k 3:1 < data.txt | head -n 2; $(p) -F fib < data.txt | head -n 2; } > o.colspec-x-A-k.test
	{ printf 'a:b\ta@b\t+c\n1\t2\t3\n' | $(p) 'a%3Ab:a@b:%2Bc'; printf 'a:b\ta@b\t+c\n1\t2\t3\n' | $(p) '+a%3A.:+\+c'; } > o.colspec-decode.test
	for a in '+' 'a:+' '+a(' '+(?<=a)b' 'x::y'; do $(p) "$$a" < data.txt; [ $$? -eq 1 ] || exit 1; done 2> o.colspec-errors.test
	for a in '+zz.*' 'fib:+zz' 'fib:' 'num(\d)' '+fo.'; do $(p) "$$a" < data.txt; [ $$? -eq 1 ] || exit 1; done 2> o.colspec-unknown.test
	{ for a in 3-5 11- 2+1-3 1-5*2 3:5-7:1 1-02:2 2-3:+1.; do echo "$$a"; $(p) -k "$$a" < data.txt 2>&1 | head -n 3; done; } > o.colspec-range.test
	{ for a in 2+1-3*4 12-15 0-2; do echo "$$a"; $(p) -k "$$a" < data.txt 2>&1 | head -n 3; done; } > o.colspec-range-msg.test
	{ $(p) -k -x 2-12 < data.txt | head -n 2; $(p) --sam 1-4:10- < sam.txt 2> /dev/null; $(p) 3-5 < data.txt 2>&1; [ $$? -eq 1 ]; } > o.colspec-range-other.test
	{ for a in 5-3 14 007 1-5*0 1-99999999999999999999999 0; do $(p) -k "$$a" < data.txt; [ $$? -eq 1 ] || exit 1; done; } 2> o.colspec-range-errors.test
	{ for a in @fib:fib2/gt/1 '@+num\d/le/2' '@+num\d:fib/ge/2' '@@+f.*~ge~1' '@+num\d/ge/:fib2' '@+fib.?/ep/:sq/2' @fib:fib:+fi./gt/20 '@+num%5Cd/=1'; do echo "$$a"; $(p) -h name "$$a" < data.txt; done; } > o.filter-multi.test
	{ $(p) -k 1 @2-3/gt/3 < data.txt; $(p) -k 1 @1-3*2/gt/0 < data.txt; } > o.filter-multi-k.test 2>&1
	{ $(p) -o name @fib:fib2/gt/30 @ud/eq/-2 < data.txt; $(p) -o name @fib/gt/30 @fib2/gt/30 @ud/eq/-2 < data.txt; } > o.filter-multi-or.test
	printf 'a\tb\tc\n1\t2\t3\n1\tfox\t3\nfox\tfox\t3\n9\t\t3\n1\t2\t4.5\n' | { $(p) -h a b c @+a.?:b:c/lt/5; } > o.filter-multi-na.test 2>&1
	printf 'a\tb\tc\n1\t2\t3\n1\tfox\t3\n1\t2\t4.5\n' | { $(p) -h a b c @+./all/0; } > o.filter-multi-nonint.test 2>&1
	for a in '@+zz.*/gt/0' @fib:zz/gt/0 @fib/gt/:a:b @fib/gt/:+a @fib/gt/:fib2:fib @fib:/gt/0 @fib::x/gt/0; do $(p) name "$$a" < data.txt; [ $$? -eq 1 ] || exit 1; done 2> o.filter-multi-errors.test
	{ $(p) -k 1 @1/gt/:2-3 < data.txt; [ $$? -eq 1 ]; } 2>> o.filter-multi-errors.test
	{ for a in @x/ISINT @x/NOTINT @x/ISNUM @x/NOTNUM @@x/ISNUM; do echo "$$a"; printf 'x\ty\n5\t1\n-3\t2\n+7\t3\n007\t4\n5.0\t5\n1e3\t6\ninf\t7\nnan\t8\n\t9\nfox\t10\n 4\t11\n0x10\t12\n400000000000000000000000000000000000000000\t13\n' | $(p) -h x y "$$a"; done; } > o.filter-pred.test 2>&1
	{ for a in '@+.*/ISINT' '@x:y/NOTINT' '@@+x|y/ISNUM'; do echo "$$a"; printf 'x\ty\n5\t1\n-3\t2\n+7\t3\n007\t4\n5.0\t5\n1e3\t6\ninf\t7\nnan\t8\n\t9\nfox\t10\n 4\t11\n0x10\t12\n400000000000000000000000000000000000000000\t13\n' | $(p) -h x y "$$a"; done; } > o.filter-pred-multi.test 2>&1
	{ $(p) --sam 1 2 @2/NOTINT < sam.txt; $(p) -o name ms @ms/ISINT @fib/gt/50 < data.txt; } > o.filter-pred-other.test 2> /dev/null
	for a in @x/ISFOO @x/isint @/ISINT @x/ISINT/3 @x/ @+/ISINT @fib/gt/ISINT; do $(p) name "$$a" < data.txt; [ $$? -eq 1 ] || exit 1; done 2> o.filter-pred-errors.test
	{ $(p) name fib @assert@fib/gt/0 '@@assert@+num\d/ISINT' < data.txt; echo "exit $$?"; } > o.assert-pass.test 2>&1
	{ $(p) name ud @assert@ud/gt/-5 < data.txt; echo "exit $$?"; } > o.assert-fail.test 2>&1
	{ $(p) name ud @@ud/gt/0 @assert@ud/gt/0 < data.txt; echo "exit $$?"; $(p) name ud @@ud/gt/0 @@assert@ud/gt/0 < data.txt; echo "exit $$?"; } > o.assert-stage.test 2>&1
	{ $(p) -o name ud @fib/gt/50 @assert@ud/gt/-11 < data.txt; echo "exit $$?"; $(p) -c @assert@ud/gt/-5 < data.txt; echo "exit $$?"; } > o.assert-or-count.test 2>&1
	{ $(p) name '@assert@+num\d/le/3' < data.txt; echo "exit $$?"; printf 'a\tb\n1\t2\n3\tfox\n' | $(p) a '@@assert@a:b/ISNUM'; echo "exit $$?"; printf 'a\tb\n1\t2\n3\tfox\n' | $(p) a '@assert@b/lt/5'; echo "exit $$?"; printf 'a\n1\n2.5\n' | $(p) a '@assert@a/all/1'; echo "exit $$?"; } > o.assert-multi.test 2>&1
	{ $(p) --sam 1 2 @@assert@2/ISINT < sam.txt; echo "exit $$?"; } > o.assert-sam.test 2>&1
	{ $(p) --other=out/other.txt name fib @fib/gt/5 < data.txt; echo "-- out/other.txt"; cat out/other.txt; } > o.other.test
	{ $(p) --other=out/other-c.txt.gz -c @@fib/gt/5 @ud/gt/0 < data.txt; echo "-- out/other-c.txt.gz"; gzip -dc out/other-c.txt.gz; } > o.other-c-gz.test
	{ $(p) -h --other=out/other-h.txt name @fib/gt/5 < data.txt > /dev/null; $(p) -k --other=out/other-k.txt 1 @2/gt/5 < data.txt > /dev/null; cat out/other-h.txt out/other-k.txt; } > o.other-h-k.test
	{ $(p) --sam-h --other=out/other-sam.txt 1 2 @2/eq/0 < sam.txt 2> /dev/null; echo "-- out/other-sam.txt"; cat out/other-sam.txt; } > o.other-sam.test
	{ $(p) --other=out/nodir/x.txt name < data.txt; echo "exit $$?"; $(p) --other= name < data.txt; echo "exit $$?"; } > o.other-errors.test 2>&1
	{ $(p) --assert-other=out/ao.txt name ud @assert@ud/gt/-5 < data.txt; echo "exit $$?"; echo "-- out/ao.txt"; cat out/ao.txt; } > o.assert-other.test 2>&1
	{ $(p) --assert-other=out/ao-mix.txt --other=out/ao-mix-other.txt name ud fib @@assert@ud/ne/-2 @@fib/gt/1 @assert@ud/gt/-10 @ud/gt/0 < data.txt; echo "-- out/ao-mix.txt"; cat out/ao-mix.txt; echo "-- out/ao-mix-other.txt"; cat out/ao-mix-other.txt; } > o.assert-other-mix.test 2>&1
	{ $(p) -c --assert-other=out/ao-c.txt.gz '@@assert@+num\d/le/100' < data.txt; echo "-- out/ao-c.txt.gz"; gzip -dc out/ao-c.txt.gz; $(p) --assert-other=out/ao-none.txt name @assert@fib/gt/0 < data.txt > /dev/null; echo "-- out/ao-none.txt"; cat out/ao-none.txt; } > o.assert-other-c.test 2>&1
	{ $(p) --other=out/x --assert-other=out/x name < data.txt; echo "exit $$?"; $(p) --assert-other= name < data.txt; echo "exit $$?"; $(p) --assert-other=out/nodir/x name < data.txt; echo "exit $$?"; } > o.assert-other-errors.test 2>&1
	mkdir out/dm1 && { $(p) --demux=fx --demux-dir=out/dm1 name fib @fib/gt/5 < data.txt; echo "exit $$?"; (cd out/dm1 && find . -type f | sort | while read -r f; do echo "-- $$f"; case "$$f" in *.gz) gzip -dc "$$f";; *) cat "$$f";; esac; done); } > o.demux.test 2>&1
	mkdir out/dm2 && { $(p) --demux=fx --demux-dir=out/dm2 name fib @@fib/gt/5 @ud/gt/0 < data.txt; (cd out/dm2 && find . -type f | sort | while read -r f; do echo "-- $$f"; case "$$f" in *.gz) gzip -dc "$$f";; *) cat "$$f";; esac; done); } > o.demux-pre.test 2>&1
	mkdir out/dm3 out/dm3/sub && { printf 'f\tv\na.gz\t1\nsub/b.txt\t2\na.gz\t3\n*\t4\n' | $(p) -h --demux=f --demux-dir=out/dm3 v; (cd out/dm3 && find . -type f | sort | while read -r f; do echo "-- $$f"; case "$$f" in *.gz) gzip -dc "$$f";; *) cat "$$f";; esac; done); } > o.demux-gz-h.test 2>&1
	mkdir out/dm4 && { $(p) --sam-h --demux=3 --demux-dir=out/dm4 1 2 3 < sam.txt; (cd out/dm4 && find . -type f | sort | while read -r f; do echo "-- $$f"; case "$$f" in *.gz) gzip -dc "$$f";; *) cat "$$f";; esac; done); } > o.demux-sam.test 2>&1
	mkdir out/dm5 && { $(p) --demux=fx --demux-dir=out/dm5 --demux-buffer=10 --other=out/dm5-other.txt --assert-other=out/dm5-assert.txt name ud @@assert@ud/ne/-2 @ud/gt/0 < data.txt; (cd out/dm5 && find . -type f | sort | while read -r f; do echo "-- $$f"; case "$$f" in *.gz) gzip -dc "$$f";; *) cat "$$f";; esac; done); echo "-- out/dm5-other.txt"; cat out/dm5-other.txt; echo "-- out/dm5-assert.txt"; cat out/dm5-assert.txt; } > o.demux-other.test 2>&1
	mkdir out/dm6 && { ulimit -n 20; $(p) --demux=id --demux-dir=out/dm6 id < big.txt 2>&1 | grep -o -e 'cannot open more files' -e 'split the run with @@ filters on the demux column'; } > o.demux-many.test
	mkdir out/dm7 && { for t in 'a\tb\n../y\t2\n' 'a\tb\n\t1\n' 'a\tb\n/abs\t1\n' 'a\tb\nx//y\t1\n' 'a\tb\nx/\t1\n' 'a\tb\n./x\t1\n' 'a\tb\nnodir/x\t1\n'; do printf "$$t" | $(p) --demux=a --demux-dir=out/dm7 a b; echo "exit $$?"; done; printf 'a\tb\nx\t1\n' | $(p) --demux=a --demux-dir=out/dm7 --other=out/dm7/x a b @b/gt/5; echo "exit $$?"; for a in '-c --demux=fx' '--demux=zz name' '--demux-dir=out/nodir --demux=fx name' '--demux-dir=out name' '--demux=a:b name' '--demux=+f.* name' '--demux-buffer=x name'; do $(p) $$a < data.txt; echo "exit $$?"; done; } > o.demux-errors.test 2>&1
	$(p) name 'x::^1' < data.txt > o.compute-simple.test
	{ for a in 'name fib x::fib^1,add y::^a:fib,cat' 'name z::num1:num2,add w::ms:ud,add' 'name d::fx^o,del d2::foo11^_,del d3::name^[aeiou],del' 'name s::^a^b,xch u::fib,dup,cat p::^a^b,pop' 'name x:=fib^10,add y::x^2,add @x/gt/20' 'name x::fib^1,add @@fib/gt/5 @x/lt/50' 'name x::^chr:fib' '-A x::ms^0.1,add' '-x fib x::ms:ms,add name' '-h name q::ms:ud:sq,addall'; do echo "$$a"; $(p) --num-perl $$a < data.txt; done; } > o.compute-run.test
	{ $(p) name 's::+num\d,addall' 't::^1^2^3,addall' 'e::,addall' < data.txt; $(p) --num-perl id f::f1:f2,add g::f3:f4,add h::n1:n2,add < big.txt; $(p) -c i::n1^1,add @i/gt/0 < big.txt; } > o.compute-run-big.test
	{ $(p) name ms x::ms^0.1,add y::ms^1e20,add n::fx^1,add z::^170141183460469231731687303715884105727^1,add < data.txt | head -n 4; $(p) --num-perl name ms x::ms^0.1,add y::ms^1e20,add n::fx^1,add z::^170141183460469231731687303715884105727^1,add < data.txt | head -n 4; printf 'a\tb\n1e15\t0\n1.5\t1e-5\n-0.0\t0\ninf\t1\nnan\t1\n' | $(p) a b s::a:b,add t::a:b,cat; printf 'a\tb\n1e15\t0\n1.5\t1e-5\n-0.0\t0\ninf\t1\nnan\t1\n' | $(p) --num-perl a b s::a:b,add; } > o.compute-num.test
	{ $(p) name e::^a,pop c::^a^b x::fib:fib < data.txt | head -n 3; $(p) -k 1 '::2-4,addall' '::1^-:2' < data.txt | head -n 3; } > o.compute-stack.test
	{ $(p) --other=out/cmp-other.txt name x::fib^1,add @@fib/gt/5 @x/lt/50 < data.txt; echo "-- out/cmp-other.txt"; cat out/cmp-other.txt; } > o.compute-other.test
	mkdir out/dm8 && { $(p) --demux=g --demux-dir=out/dm8 name g:=fx^.txt,cat @fib/gt/30 < data.txt; (cd out/dm8 && find . -type f | sort | while read -r f; do echo "-- $$f"; cat "$$f"; done); } > o.compute-demux.test 2>&1
	for a in 'x::nosuch' 'fib::^1' 'x::^1 x::^2' 'x::^1,add' 'x::,cat' 'x::^1,pop,pop' 'x::fx:fib,del' 'x::fx^,del' 'x::fx^(,del' 'x::^1 @@x/gt/0' 'x::y y::^1'; do $(p) name $$a < data.txt; [ $$? -eq 1 ] || exit 1; done 2> o.compute-compile-errors.test
	{ $(p) name x::^1 @x/gt/0 < data.txt; echo "exit $$?"; $(p) --demux=x --demux-dir=out name x:=^1 < data.txt; echo "exit $$?"; } > o.compute-compile-ok.test 2>&1
	{ for a in '-i name fib::fib^1,add' '-i name fib fib::fib^1,add' '-i fib::fib^1,add name fib' '-i name fib fib::fib^1,add x::fib^100,add' '-i name x::fib^100,add fib::fib^1,add' '-i name fib::fib^1,add @@fib/gt/55' '-i name fib::fib^1,add @fib/gt/55' '-i fib:=fib^1,add name fib' '-i -A fib::fib^1,add' '-i -x fib::fib^1,add name' '-i -A fib:=fib^1,add x::^1' '-i name fib::__^1,add' '-i name x:=^1 x::x^1,add' '-i name x::^1 y::x x::x^1,add' '-i -x fib fib:=fib^1,add'; do echo "$$a"; $(p) --num-perl $$a < data.txt 2>&1 | head -n 4; done; } > o.inplace.test
	{ printf '1\t2\t3\n4\t5\t6\n' | $(p) -k -i 2::2^10,add s::1-3,addall; printf '__\ta\n1\t2\n' | $(p) x::%5F%5F^5,add y::a^1,add; printf '__\ta\n1\t2\n' | $(p) -i a::__^7,add __; } > o.inplace-other.test
	for a in 'fib::fib^1,add' '-i fib::fib^1,add fib::fib^10,add' '-i fib::^1 fib:=^2' 'x::__^1,add' 'x::^1 x::^2'; do $(p) $$a name < data.txt; [ $$? -eq 1 ] || exit 1; done 2> o.inplace-errors.test
	{ for a in '-i name fib:sq::__^1,add' '-i name +fib.*::__^1,add' '-i +fib.*::__^1,add name' '-i name fib:sq::__:fib,add' '-i -A +num\d::__^10,add' '-i name +num\d:=__^10,add num1 num4' '-i -x fib:sq::__^1,add name' '-i name +num\d::__^10,add s::+num\d,addall'; do echo "$$a"; $(p) --num-perl $$a < data.txt 2>&1 | head -n 4; done; printf '1\t2\t3\t4\n5\t6\t7\t8\n' | $(p) -k -i 1 2-3::__^10,add 4; $(p) -i name fib:fib::__^1,add < data.txt 2>&1 | head -n 3; } > o.map.test
	for a in 'name fib:sq::__^1,add' '-i name a:b::^1' '-i name +fib.*::^1 fib::^2' '-i name +zz.*::^1'; do $(p) $$a < data.txt; [ $$? -eq 1 ] || exit 1; done 2> o.map-errors.test
	{ for a in '-A0 x::^X y:=^Y z::^Z' '-A1 x::^X y:=^Y z::^Z' '-A2 x::^X y:=^Y z::^Z' '-A13 x::^X z::^Z' '-A20 x::^X z::^Z' '-A-1 x::^X z::^Z' '-A-13 x::^X z::^Z' '-A-20 x::^X z::^Z' '-A2' '-i -A2 fib::fib^1,add x::^X' '-i -A-1 +num\d::__^1,add x::^X' '-A3 fib x::^X' '-kA1 x::^X'; do echo "$$a"; $(p) $$a < data.txt | head -n 2; done; } > o.insert.test
	{ $(p) -A2 -x fib x::^X < data.txt; echo "exit $$?"; } > o.insert-errors.test 2>&1
	{ for a in '-K 3 1' '-K 3 1 x::2^10,add' '-K -A' '-KA x::1:2,add' '-K -h 3 1' '-K -c' '-K -x 2' '-K -i 2::2^1,add 1 2' '-K usefulname::1 anotherusefulname::2 usefullycomputed::3:2,add' '-K 2-3 ::1^a,cat'; do echo "$$a"; printf '1\t2\t3\n4\t5\t6\n' | $(p) $$a 2>&1; done; } > o.kheader.test
	mkdir out/dm9 && { printf 'a\t1\nb\t2\nb\t3\n' | $(p) -K --other=out/kh-other.txt --demux=1 --demux-dir=out/dm9 two::2 @2/lt/3; echo "-- out/kh-other.txt"; cat out/kh-other.txt; (cd out/dm9 && find . -type f | sort | while read -r f; do echo "-- $$f"; cat "$$f"; done); } > o.kheader-files.test 2>&1
	{ for a in '--sam -K 1' '-K --sam-h 1' '-KR 1'; do $(p) $$a < sam.txt; echo "exit $$?"; done; } > o.kheader-errors.test 2>&1
	{ for a in 'name fib ud a::fib:ud,sub b::fib:ud,mul c::fib:sq,div d::sq:ud,idiv e::sq:ud,mod f::ud^3,mod g::ud^-3,mod h::ud^3,idiv' 'name ms a::ms:ud,sub b::ms:ud,mul c::ms^0.25,div d::ud^3,pow e::ud^2,ipow f::fib^-1,ipow g::ms^2,pow' 'name a::fib:ud,max b::fib:ud,min c::ms:ud,max d::ms:ud,min e::ud,neg f::ms,neg g::ud,abs h::ms,abs' 'name a::ud,sign b::ms,sign c::ud,sq d::ms,sq e::ud,incr f::ud,decr g::ms,incr h::^0,sign'; do echo "$$a"; $(p) --num-perl $$a < data.txt; done; } > o.math-arith.test
	{ $(p) --num-perl id n1 n2 a::n1:n2,mul d::n1^7,mod e::f1:f2,div f::f3:f4,sub g::n3^3,ipow h::n1^-7,idiv i::n1^-7,mod < big.txt; $(p) --num-perl id f3 f4 a::f3:f4,max b::f3:f4,min c::f3,abs d::f3,sign e::f3:f4,mul f::f4,sq < big.txt; } > o.math-arith-big.test
	printf 'x\ty\n7\t0\n0\t0\n-7\t0\n7.5\t2\nfox\t2\n-7\t-2\nnan\t3\n2\t200\n-1\t3\n' | $(p) x y d::x:y,div i::x:y,idiv m::x:y,mod p::x:y,ipow w::x:y,pow s::x,sign n::x,neg mx::x:y,max mn::y:x,min > o.math-arith-edge.test
	{ for a in 'name ms ud a::ms,int b::ms,floor c::ms,ceil d::ms,round e::ud,int f::ud,floor g::ud^0.5,add,round h::ms^-1,mul,ceil' 'name fib ms a::fib,sqrt b::ms,sqrt c::fib,log f::ms,exp g::ud,exp2 h::ud,exp10' 'name ms ud a::ms,sin b::ms,cos c::ms,tan d::ud,sin e::ud,cos f::ud,tan'; do echo "$$a"; $(p) --num-perl $$a < data.txt; done; } > o.math-func.test
	{ $(p) --num-perl id f3 f4 a::f3,int b::f3,floor c::f3,ceil d::f3,round e::f3,sin f::f4,cos < big.txt; $(p) --num-perl id n4 a::n4,sqrt c::n4,log < big.txt; } > o.math-func-big.test
	printf 'x\n1000\n8\n0.001\n0\n-1\n2.5\n-2.5\n0.5\n-0.5\n1e300\n-1e300\nnan\ninf\nfox\n42.5177\n' | $(p) x l::x,log a::x,log10 b::x,log2 s::x,sqrt r::x,round i::x,int f::x,floor c::x,ceil e::x,exp > o.math-func-edge.test
	{ $(p) --num-perl name a::fib:ud:sq,mulall b::fib:ud:ms,maxall c::fib:ud:ms,minall d::fib:ud:sq,meanall e::fib:sq,hmeanall < data.txt; $(p) --num-perl name a::num1:num2:num4,mulall b::ms:ud,maxall c::ms:ud,minall d::ms:ud,meanall e::ms:ud,hmeanall g::^5,meanall < data.txt; $(p) --num-perl id a::n1:n2:n3,maxall b::f1:f2:f3:f4,minall c::f1:f2,meanall e::n4:n5,mulall f::f1:f2,hmeanall < big.txt; } > o.math-whole.test
	printf 'a\tb\tc\n1\tnan\t3\nnan\tnan\tnan\n-inf\t-inf\t-inf\n2\tfox\t1\n0\t2\t4\n1.50\t1.5\t-2\n' | $(p) a b c mx::a:b:c,maxall mn::a:b:c,minall me::a:b:c,meanall h::a:b:c,hmeanall g::a:b:c,gmeanall p::a:b:c,mulall e::,meanall z::,mulall > o.math-whole-edge.test
	{ $(p) name f::fib:ud:sq,gmeanall g::ms:ud,gmeanall < data.txt; $(p) id d::f1:f2:n4,gmeanall < big.txt | head -n 5; printf 'x\n1e200\n' | $(p) x g::x:x:x:x:x,gmeanall; } > o.math-gmean.test
	{ $(p) --num-perl name ms ud a::ms^2,dd b::ms^0,dd c::ud^3,dd d::ms^3,sn e::ud^1,sn f::fib:sq^2,frac g::fib:sq^1,pct h::fib:sq^0,pml < data.txt; $(p) --num-perl id f1 f3 a::f1^3,dd b::f3^1,dd c::f3^4,sn d::f1:f2^4,frac e::n1:n4^2,pct f::f3:f4^1,pml g::f3^0,dd < big.txt; $(p) --num-perl name x::ms^20,dd y::ms^15,sn < data.txt; } > o.math-precision.test
	{ printf 'x\ty\tn\n1\t0\t2\n0\t0\t2\n-1\t0\t2\n2\t3\t-1\n2\t3\t1.5\n2\t3\tfox\nfox\t3\t2\n0.125\t1\t2\n2\t3\t1001\n' | $(p) x y n d::x:n,dd s::x:n,sn f::x:y:n,frac p::x:y:n,pct m::x:y:n,pml; printf 'x\ty\n1\t0\n0\t0\n' | $(p) --num-perl x y f::x:y^2,frac; } > o.math-precision-edge.test
	{ $(p) --seed=42 name r::^1,rand s::^10,rand z::^0,rand n::^-5,rand f::fx,rand < data.txt; $(p) --seed=7 -c 'r1:=^10,rand' 'r2:=^-3,rand' @assert@r1/ge/0 @assert@r1/lt/10 @assert@r2/gt/-3 @assert@r2/le/0 < big.txt; $(p) --seed=x name < data.txt; echo "exit $$?"; } > o.math-rand.test 2>&1
	{ for a in 'name a::name,len b::name,lc c::name,uc d::name,rev e::name,rot13 f::foo11,len g::ms,len h::ms,rev' 'name a::name^2,head b::name^2,tail c::name^2,skip d::name^2,shed e::name^10,head f::name^-1,tail g::name^10,skip h::name^0,shed' 'name a::name^1^2,substr b::name^-2^1,substr c::name^1^-1,substr d::name^1^1,strim e::foo11^-1^2,strim f::name^5^5,strim'; do echo "$$a"; $(p) --num-perl $$a < data.txt; done; $(p) id s1 s2 a::s2^3,head b::s2^-3^2,substr c::s1,uc d::s2,rev e::s1:s2,cat,len < big.txt; printf 'x\n\303\211mile\ncaf\303\251\nNA\303\217VE\n\351t\351\n' | $(p) x a::x,len b::x,lc c::x,uc d::x,rev e::x^2,head; } > o.string-basic.test
	{ printf 's\ti\tk\n'; for s in '' a ab abc; do for i in -4 -3 -2 -1 0 1 2 3 4; do for k in -4 -2 -1 0 1 2 4; do printf '%s\t%s\t%s\n' "$$s" $$i $$k; done; done; done; } | $(p) s i k r::s:i:k,substr > o.string-substr.test
	printf 'x\tk\nabcdef\t2\nabcdef\t1.5\nabcdef\tfox\nfox\t2\nabcdef\t-3\n' | $(p) x k a::x:k,head b::x:k^1,substr c::x^1:k,strim d::x^1,add,len e::x^1,add,uc f::x:k,tail > o.string-edge.test
	{ $(p) --num-perl name a::name^6,ppl b::name^6,ppr c::ud^4,zp d::name^2,ppr e::ms^12,ppr f::name^0,zp g::name^n,nchar h::foo11^_,nchar i::dna^A,nchar < data.txt; $(p) id a::s2^12,ppl b::n1^5,zp c::dna^G,nchar d::s1^20,ppr < big.txt; } > o.string-pad.test
	printf 'x\tn\nabc\t5\nabc\t-1\nabc\t2.5\nabc\tfox\naaaa\t3\n' | $(p) x n a::x:n,ppl b::x:n,ppr c::x:n,zp d::x^aa,nchar e::x^,nchar f::x^1,add:n,zp > o.string-pad-edge.test
	{ $(p) --num-perl name a::name:fx:ud,catall b::name:fx:ud^-,joinall c::name:fx^%3A,join d::name:ms^_,join e::^:fx,uie f::fx:name,uie g::^-,joinall h::,catall < data.txt; $(p) id a::id:grp:n1^/,joinall b::s1:s2,catall c::tag:grp^x,join < big.txt; } > o.string-join.test
	{ printf 'x\ty\n\tb\nfox\tb\na\t\n' | $(p) x y a::x^1,add:y,uie b::x:y,uie c::x^1,add:y^-,join d::x:y^1,add,catall e::x:y^1,add,joinall f::x:y,catall; $(p) name 'x::,joinall' < data.txt; echo "exit $$?"; } > o.string-join-edge.test 2>&1
	{ $(p) --num-perl name a::name,md5 b::foo11,urlec c::ms,urlec d::foo11,urlec,urldc e::^a%2520b,urldc f::ms,md5 < data.txt; $(p) id a::s1:s2,cat,md5 b::dna,urlec c::^%25zz%41%4a%4,urldc < big.txt; printf 'x\n\303\211mile/x y?z=1&a\ncaf\351\n' | $(p) x a::x,urlec b::x,urlec,urldc c::x,md5; printf 'x\nfox\n' | $(p) x a::x^1,add,urlec b::x^1,add,md5 c::^,md5; } > o.string-encode.test
	{ $(p) --num-perl name a::^#=#:name:fib,fmtall 'b::^\#[#]:ud,fmtall' c::^x,fmtall 'd::^#%3A#%3A#:fx:ms:sq,fmtall' 'e::^<#|#|#>:num1:num2:num4,fmtall' < data.txt; $(p) id a::^#/#/#:grp:n1:tag,fmtall < big.txt; } > o.string-fmtall.test
	{ $(p) name 'x::^<#|#|#>:+num\d,fmtall' < data.txt | head -n 3; for a in 'x::^#-#:name,fmtall' 'x::^#:name:fib,fmtall'; do $(p) name "$$a" < data.txt; echo "exit $$?"; done; printf 'f\tv\n#+#\t1\n<#>\t2\n\\#\t3\n' | $(p) f v x::f:v,fmtall y::f:v^1,add,fmtall; } > o.string-fmtall-edge.test 2>&1
	{ $(p) --num-perl name 'a::foo11^_,delg' 'b::name^[aeiou],delg' 'c::foo11^_^-,ed' 'd::foo11^_^-,edg' 'e::name^(.)(.)^$$2$$1,edg' 'f::ms^0^,edg' 'g::dna^A+^x,edg' 'h::name^[A-Z]^$$0,ed' 'i::dna^C*^.,edg' 'j::name^n*,delg' < data.txt; $(p) id 'a::s2^[0-9]+,delg' 'b::dna^(GA|TC)^-,edg' 'c::s1^a^A,ed' 'd::dna^%5E.^>,ed' < big.txt; } > o.regex-ed.test
	printf 'x\nabc\n\naaa\nbaab\n' | $(p) x 'a::x^x*^-,edg' 'b::x^a*^-,edg' 'c::x^\b^-,edg' 'd::x^b*$$^-,edg' 'e::x^a|^-,edg' 'f::x^a?^-,edg' 'g::x^a*?^-,edg' 'h::x^a*,delg' > o.regex-empty.test
	{ for a in 'x::name:fib,ed' 'x::name:fx^-,ed' 'x::name^^-,edg' 'x::name^(^-,ed' 'x::name:fx,delg'; do $(p) name "$$a" < data.txt; echo "exit $$?"; done; printf 'x\tr\nfox\tR\n' | $(p) x r 'a::x^1,add^o^-,ed' 'b::x^o:r,edg' 'c::x^o:r^1,add,edg' 'd::x^1,add^o,del'; } > o.regex-ed-edge.test 2>&1
	{ $(p) --num-perl name 'a::name^(.)(.)^$$2$$1,edbr' 'b::name^(.)(.)^$$2$$1,edgbr' 'c::foo11^(.)_^[$$1],edgbr' 'd::name^(x)?(n)^<$$1$$2$$3>,edgbr' 'e::name^[aeiou]^($$0),edgbr' 'f::dna^(A)(C)?^$$2$$1x,edgbr' 'g::ms^(\d)\.^$$1$$$$1,edbr' 'h::name^n*^[$$0],edgbr' < data.txt; $(p) id 'a::dna^(.)(.)(.)^$$3$$2$$1,edgbr' 'b::s2^([a-z]+)([0-9]+)^$$2-$$1,edbr' 'c::s1^(.)^$$1$$1,edgbr' < big.txt; } > o.regex-br.test
	{ $(p) --num-perl name 'a::name^[aeiou].,get' 'b::name^(.)n,get' 'c::foo11^_(.)_,get' 'd::name^(x)?n+,get' 'e::name^z,get' 'f::name^n,npat' 'g::dna^[GC],npat' 'h::name^n*,npat' 'i::ms^\d,npat' 'j::dna^(A|C)(G)?,get' < data.txt; $(p) id 'a::s2^[0-9]+,get' 'b::s2^([a-z])[a-z]*,get' 'c::dna^A+,npat' 'e::dna^x*,npat' < big.txt; } > o.regex-get.test
	{ printf 'x\nabc\n\naaa\nbaab\n' | $(p) x 'a::x^x*,npat' 'b::x^a*,npat' 'c::x^a|,npat' 'd::x^a?,npat' 'e::x^\b,npat' 'f::x^(a)|b,get' 'g::x^q,get'; printf 'x\nfox\n' | $(p) x 'a::x^1,add^o,get' 'b::x^1,add^o,npat'; $(p) id 'd::dna^,npat' < big.txt; echo "exit $$?"; } > o.regex-get-edge.test 2>&1
	{ $(p) --num-perl name ms 'a::ms^\Q.\E,del' 'b::ms^\Q.\E^[.],ed' 'c::ms^\Q0.\E\d,get' 'd::foo11^\Q_\E,npat' < data.txt; $(p) name ms '@ms~\Q1.\E' < data.txt; $(p) name ms '@ms/~\Q.2\E' < data.txt; } > o.regex-quote.test
	{ printf 'a.b\ta*b\tab\n1\t2\t3\nx.y\t4\t5\n' > out/q.txt; $(p) '-/\Q.\E' -k 1 < out/q.txt; $(p) '+a\Q.\Eb' < out/q.txt; $(p) '+a\Q*b' < out/q.txt; $(p) 'x::a.b^\Q.,del' a.b < out/q.txt; $(p) '+a\\Qb' < out/q.txt; echo "exit $$?"; } > o.regex-quote-other.test 2>&1
	{ $(p) -A --cdict-123/0=2,3 'x:=num4^123,map' '@x=0' < data.txt; $(p) --cdict-F=dog:hund,cat:katze name fx 'a::fx^F,map' < data.txt; $(p) --cdict-F/none=dog:hund,cat:katze name fx 'a::fx^F,map' 'b::name^F,map' < data.txt; $(p) --kdict-K=big.keep.txt id 'a::id^K,map' < big.txt; printf 'A\tapple\nB\tbanana\nC\tcherry\n' > out/g.dict; $(p) --fdict-G=out/g.dict id grp 'a::grp^G,map' < big.txt; } > o.dict-map.test
	{ $(p) --cdict-F=dog:hund,cat:katze name fx 'b::fx^F,dord' < data.txt; $(p) --kdict-K=big.keep.txt id 'b::id^K,dord' < big.txt | head -n 12; printf 'k\nb\nfox\nc\n' | $(p) --cd-D=a:1,b:2 --cd-D=c:3,a:9 k 'o::k^D,dord' 'm::k^D,map' 'n::k^1,add^D,map'; } > o.dict-dord.test 2>&1
	{ $(p) --cd-E/__EXIT__=dog:1 name 'x::fx^E,map' < data.txt; echo "exit $$?"; $(p) --cd-E=a:1 name 'x::fx:name,map' < data.txt; echo "exit $$?"; $(p) name 'x::fx^Q,map' < data.txt; echo "exit $$?"; $(p) --sam 1 3 'x::3^seqlen,map' 'y::3^seqlen,dord' < sam.txt 2>/dev/null; } > o.dict-ops-edge.test 2>&1
	{ $(p) --num-perl name 'a::fib^3^/gt/,test' 'b::fib:fib2^/le/,test' 'c::fx^cat^~eq~,test' 'd::fx^cat^=,test' 'e::name^%5E[A-D]^~,test' 'f::name^n$$^/~,test' 'g::ud^1^/all/,test' 'h::ms^0.15^/ep/,test' 'i::fib:sq^/om/,test' < data.txt; $(p) --num-perl --cdict-K=dog,owl name 'a::fx^K^~isin~,test' 'b::fx^K^~isnotin~,test' 'c::fib^3^/gt/,test^big^small,ifelse' 'd::ud^yes^no,ifelse' 'e::fx^Y^N,ifelse' 'f::^0.0^Y^N,ifelse' 'g::^^Y^N,ifelse' 'h::ms^Y^N,ifelse' < data.txt; $(p) --num-perl id 'a::n1^0^/gt/,test:s1:s2,ifelse' 'b::s1^%5E[a-m]^~,test:n4:n5,ifelse' 'c::f1:f2^/om/,test' 'd::flag^4^/any/,test' < big.txt; } > o.branch-test.test
	{ printf 'x\ty\nfox\t1\n3\tbar\n2.5\t1\n' | $(p) x y 'a::x:y^/gt/,test' 'b::x:y^~lt~,test' 'c::x^1^/all/,test' 'd::x^1,add^A^B,ifelse'; for a in 'x::fib^3^/foo/,test' 'x::fib^3:fx,test' 'x::fib^D^~hasval~,test' 'x::fib^%5E^/ISNUM,test' 'x::fib:fx^~,test' 'x::fib^Q^~isin~,test'; do $(p) name "$$a" < data.txt; echo "exit $$?"; done; } > o.branch-edge.test 2>&1
	$(p) --num-perl -h --cdict-G=A:1,B:2,C:3,D:4,E:5 --cdict-V/=A:vowel,E:vowel id ::n1^0^/gt/,test:s1:s2,ifelse ::grp^G,dord^10,mul:n3,add ::grp^V,map^cons,uie ::s1^%5E[a-m]^~,test:n4:n5,ifelse k:=n4^reg,save ::^reg,fetch:k,sub:n4,add ::f1:f2^/om/,test < big.txt > o.ops-branch.test
	{ $(p) name 'a::name^r,save^x,cat' 'b::^r,fetch' 'c::fib^r,save,pop^r,fetch^!,cat' < data.txt; $(p) name 'r::,rowno' 's::,r0wno' 'l::,lineno' '@@fib/gt/5' < data.txt; $(p) -k 1 'r::,rowno' 's::,r0wno' 'l::,lineno' < data.txt | head -n 3; printf '# c\nname\tv\na\t1\n# mid\nb\t2\n' | $(p) '-/^#' name 'r::,rowno' 'l::,lineno'; } > o.state-ops.test
	{ printf 'k\tv\nreg1\t5\nreg2\t6\n' | $(p) k v 'a::v:k,save' 'b::^reg1,fetch' 'c::k,fetch' 'd::^zz:k,save,pop:k,fetch'; $(p) name 'x::^q,fetch' < data.txt; echo "exit $$?"; $(p) name 'x::^q,fetch' 'y::^1^q,save' < data.txt; echo "exit $$?"; } > o.state-ops-edge.test 2>&1
	{ printf 'x\n3h\n1.5d\n-2m\n1e1s\n2y\n0s\n1w\n' | $(p) --num-perl -h x 'a::x,ttu' 'b::^1.5h,ttu' 'c::x,ttu,rttu'; printf 'x\n0\n59\n60\n3600\n93784\n-93784\n86400\n1700000000\n' > out/rt.txt; $(p) -h x 'a::x,rttu' < out/rt.txt; for tz in UTC Europe/Amsterdam America/New_York; do TZ=$$tz $(p) -h x 'd::x,todate' < out/rt.txt; done; } > o.time-ops.test
	{ printf 'x\n 3h\n3x\nfox\n1.5\n0.5s\nnan\n' | $(p) -h x 'a::x,ttu' 'b::x,rttu' 'c::x,todate'; printf 'x\n1.11m\n0.5s\n1.5s\n2.5s\n-1.11m\n' | $(p) -h x 'a::x,ttu,rttu'; $(p) name 'x::^3q,ttu' < data.txt; echo "exit $$?"; $(p) -h 'n::,now' < data.txt | head -n 1 | awk -v t=$$(date +%s) '{ d = $$1 - t; print (d >= -2 && d <= 2) ? "now ok" : "now off by " d }'; } > o.time-edge.test 2>&1
	$(p) name fib sq a::fib:sq,bitand b::fib:sq,bitor c::fib:sq,bitxor d::sq:fib,bitoff e::sq:fib,bitlsh f::sq:fib,bitrsh g::sq^2,bitrsh h::fib^3,bitlsh < data.txt | head -n 11 > o.bit-ops.test
	{ printf 'x\ty\n12\t10\n-12\t10\n12\t-10\n3.0\t1\n3\t1.5\nfox\t1\n\t1\n1\t200\n0\t200\n5\t0\n1\t62\n3\t126\n1e3\t1\n4611686018427387904\t66\n' | $(p) x y 'a::x:y,bitand' 'b::x:y,bitor' 'c::x:y,bitxor' 'd::x:y,bitoff' 'e::x:y,bitlsh' 'f::x:y,bitrsh'; $(p) -h id 'a::flag:n4,bitand' 'b::flag:n4,bitor' 'c::flag:n4,bitoff' 'd::n1:n2,bitxor' < big.txt | head -n 5; } > o.bit-edge.test 2>&1
	{ $(p) name sq fib a::sq,tobin b::sq,tohex c::sq,tooct d::sq,tobin,binto e::sq,tohex,hexto f::sq,tooct,octto g::num1,tobin,binto h::fib,tohex,hexto^1,add j::^ff,hexto k::^777,octto l::^0x1F,hexto n::^1_000,octto < data.txt; $(p) id n4 n6 a::n4,tobin b::n4,tohex c::n6,tooct d::n4,tohex,hexto e::n6,tooct,octto f::n4,tobin,binto g::n6,hexto < big.txt; } > o.radix-ops.test
	{ printf 'x\n0\n1\n101\n1_01\n0b101\n0B101\n_101\n101_\n1__01\n102\nfox\n\n-101\n0x1f\n0X1F\n0x_1f\nx1f\n0o17\n017\n18\n3.0\n-5\n 1\n' | $(p) -h x 'b::x,binto' 'h::x,hexto' 'o::x,octto' 'tb::x,tobin' 'th::x,tohex' 'to::x,tooct'; printf 'x\n7fffffffffffffff\nffffffffffffffffffffffffffffffffff\n-7fffffffffffffff\n-0x1f\n--1f\n-\n+1f\n' | $(p) -h x 'h::x,hexto' 't::x,tohex' 'r::x,tohex,hexto'; printf 'x\n-5\n-255\n0\n-9223372036854775807\n9223372036854775807\n' | $(p) -h x 'b::x,tobin,binto' 'h::x,tohex,hexto' 'o::x,tooct,octto'; $(p) -h name 'a::num4,binto' 'b::^0b101,octto' < data.txt | head -n 4; } > o.radix-edge.test 2>&1
	{ $(p) --sam 1 4 6 < qry3vs.sam.gz; $(p) --sam 1 4 6 < qry3mm.sam.gz | head -n 20; } > o.sam-nf-align.test
	{ printf 'r1\t0\tc1\t1\t60\t2D3I10M2I1D\t*\t0\t0\tAAAAAAAAAAAAAAA\t*\tNM:i:1\nr2\t16\tc1\t7\t60\t3I2D10M\t*\t0\t0\tAAAAAAAAAAAAA\t*\nr3\t0\tc1\t7\t60\t5S3I10M\t*\t0\t0\tAAAAAAAAAAAAAAAAAA\t*\nr4\t4\tc1\t7\t60\t2D10M\t*\t0\t0\tAAAAAAAAAA\t*\nr5\t0\t*\t7\t60\t2D10M\t*\t0\t0\tAAAAAAAAAA\t*\nr6\t0\tc1\t7\t60\t10M3I\t*\t0\t0\tAAAAAAAAAAAAA\t*\nr7\t0\tc1\t7\t60\t2D2D10M\t*\t0\t0\tAAAAAAAAAA\t*\nr8\t0\tc1\t5\t60\t4I\t*\t0\t0\tAAAA\t*\nr9\t0\tc1\t5\t60\t5D\t*\t0\t0\t*\t*\nr10\t0\tc1\t5\t60\t*\t*\t0\t0\tAA\t*\n' > out/nf.sam; $(p) --sam -A < out/nf.sam; $(p) --sam-nonf --sam -A < out/nf.sam; $(p) --sam 1 4 6 @@4/lt/7 < out/nf.sam; $(p) --sam --other=out/nf.other 1 4 6 '@4/gt/7' < out/nf.sam; cat out/nf.other; $(p) --sam -c '@6~D' < out/nf.sam; { printf '@HD\tVN:1.6\n'; cat out/nf.sam; } | $(p) --sam-h -A | head -n 3; $(p) --sam-nonf -k 1 < out/nf.sam; echo "exit $$?"; } > o.sam-nf.test 2>&1
	{ for f in qry3mm.sam.gz qry3vs.sam.gz; do for nf in '' --sam-nonf; do $(p) --sam $$nf 1 4 6 isforward::,isforward ref_len::,ref_len ref_posx::,ref_posx ref_posy::,ref_posy qry_len::,qry_len qry_posx::,qry_posx qry_posy::,qry_posy ref_trail5p_N::,ref_trail5p_N ref_matched_N::,ref_matched_N ref_trail3p_N::,ref_trail3p_N ref_trail5p_V::,ref_trail5p_V ref_trail3p_V::,ref_trail3p_V qry_trail5p_N::,qry_trail5p_N qry_matched_N::,qry_matched_N qry_trail3p_N::,qry_trail3p_N aln_trail5p_cgr::,aln_trail5p_cgr aln_matched_cgr::,aln_matched_cgr aln_trail3p_cgr::,aln_trail3p_cgr aln_nmatch::,aln_nmatch aln_nedit::,aln_nedit aln_nmatchx::,aln_nmatchx g::^NM.i^none,sam_get h::^XM-i^x,sam_get k::^MD.Z^-,sam_get @@2/none/4 < $$f | md5sum; done; done; $(p) --sam 1 4 6 isforward::,isforward ref_len::,ref_len ref_posx::,ref_posx ref_posy::,ref_posy qry_len::,qry_len qry_posx::,qry_posx qry_posy::,qry_posy ref_trail5p_N::,ref_trail5p_N ref_matched_N::,ref_matched_N ref_trail3p_N::,ref_trail3p_N ref_trail5p_V::,ref_trail5p_V ref_trail3p_V::,ref_trail3p_V qry_trail5p_N::,qry_trail5p_N qry_matched_N::,qry_matched_N qry_trail3p_N::,qry_trail3p_N aln_trail5p_cgr::,aln_trail5p_cgr aln_matched_cgr::,aln_matched_cgr aln_trail3p_cgr::,aln_trail3p_cgr aln_nmatch::,aln_nmatch aln_nedit::,aln_nedit aln_nmatchx::,aln_nmatchx g::^NM.i^none,sam_get h::^XM-i^x,sam_get k::^MD.Z^-,sam_get @@2/none/4 < qry3vs.sam.gz | head -n 4; $(p) --sam -q 1 'x::,ref_posx' < qry3mm.sam.gz | tail -n 3; } > o.sam-counts.test
	{ printf '@SQ\tSN:c1\tLN:100\nr1\t0\tc1\t10\t60\t5H3S10M2I5M4S2H\t*\t0\t0\t*\t*\tNM:i:4\tXX:Z:a b\nr2\t16\tc1\t10\t60\t2X8M3X\t*\t0\t0\t*\t*\nr3\t4\tc1\t10\t60\t10M\t*\t0\t0\t*\t*\tNM:i:2\nr4\t0\tc2\t10\t60\t10M\t*\t0\t0\t*\t*\nr5\t0\tc1\t10\t60\t10Q\t*\t0\t0\t*\t*\nr6\t0\tc1\t10\t60\t5S5I\t*\t0\t0\t*\t*\nr7\t0\tc1\t10\t60\t5M2N5M\t*\t0\t0\t*\t*\tNM:i:x\nr8\t0\tc1\t10\t60\t3S5M1P5M\t*\t0\t0\t*\t*\nr9\t0\tc1\t0\t60\t5M\t*\t0\t0\t*\t*\nr10\tx\tc1\t10\t60\t5M\t*\t0\t0\t*\t*\n' > out/sv.sam; $(p) --sam 1 6 'f::,isforward' 'l::,ref_len' 'x::,ref_posx' 'y::,ref_posy' 'q::,qry_len' 'qx::,qry_posx' 'qy::,qry_posy' 't3::,ref_trail3p_N' 'c5::,aln_trail5p_cgr' 'cm::,aln_matched_cgr' 'c3::,aln_trail3p_cgr' 'nm::,aln_nmatch' 'ne::,aln_nedit' 'nx::,aln_nmatchx' 'g::^XX.Z^-,sam_get' < out/sv.sam; for a in 'x::,ref_posx' 'x::^NM:i^-,sam_get' 'x::^NM.q^-,sam_get' 'x::^NMM.i^-,sam_get' 'x::2^-,sam_get'; do $(p) --sam 1 "$$a" < out/sv.sam > /dev/null; echo "exit $$?"; done; $(p) name 'x::,isforward' < data.txt; echo "exit $$?"; $(p) -l sam | head -n 4; } > o.sam-counts-edge.test 2>&1
	{ printf 'c\ts\n5S10M2I3M1D4M5S\tM\n5S10M2I3M1D4M5S\tS\n5S10M2I3M1D4M5S\tMID\n5S10M2I3M1D4M5S\tQ\n*\tM\n10M\t\n3H2P4=1X\tHP=X\n10Q\tM\n\tM\n10m\tm\nfox\tM\n' | $(p) c s 'a::c:s,cgsum' 'b::c:s,cgmax' 'n::c:s,cgcount'; $(p) --sam 1 6 'i::6^I,cgmax' 'd::6^D,cgsum' 'n::6^ID,cgcount' < qry3mm.sam.gz | head -n 6; } > o.cigar-ops.test 2>&1
	{ for f in qry3mm.sam.gz qry3vs.sam.gz; do $(p) --sam 1 2 's::,qry_seq' 'q::,qry_qual' < $$f | md5sum; done; $(p) --sam 1 2 's::,qry_seq' '@2/ge/256' < qry3mm.sam.gz | cut -c 1-60; } > o.sam-fill.test 2>&1
	{ printf 'a\t0\tc1\t1\t60\t10M\t*\t0\t0\tAACCGGTTAC\tABCDEFGHIJ\na\t272\tc1\t50\t0\t10M\t*\t0\t0\t*\t*\na\t2048\tc1\t90\t0\t4H6M\t*\t0\t0\t*\t*\nb\t256\tc1\t5\t0\t10M\t*\t0\t0\t*\t*\nb\t0\tc1\t1\t60\t10M\t*\t0\t0\tAAAAACCCCC\t*\nc\t0\tc1\t1\t60\t10M\t*\t0\t0\tAAAAA\t*\nd\t65\tc1\t1\t60\t5M\t*\t0\t0\tACGTA\tIIIII\nd\t129\tc1\t1\t60\t5M\t*\t0\t0\tTTTTT\t*\nd\t385\tc1\t1\t60\t5M\t*\t0\t0\t*\t*\nd\t321\tc1\t1\t60\t5M\t*\t0\t0\t*\t*\ne\t1\tc1\t1\t60\t5M\t*\t0\t0\t*\t*\n' > out/fill.sam; $(p) --sam 1 2 6 's::,qry_seq' 'q::,qry_qual' < out/fill.sam; echo "exit $$?"; $(p) --sam --sam-seqmiss=exit 1 's::,qry_seq' < out/fill.sam; echo "exit $$?"; $(p) --sam --sam-seqmiss=skip -c 's::,qry_seq' < out/fill.sam; $(p) --sam --other=out/fill.other 1 's::,qry_seq' '@1=a' < out/fill.sam; cat out/fill.other; $(p) --sam -q 1 2 's::,qry_seq' @@2/ge/256 < out/fill.sam; $(p) --sam -A < out/fill.sam | head -n 2; { printf '@HD\tVN:1.6\tSO:coordinate\n'; cat out/fill.sam; } | $(p) --sam 1 's::,qry_seq' 2>&1 | head -n 4; $(p) --sam-seqmiss=exit 1 < out/fill.sam; echo "exit $$?"; $(p) --sam --sam-seqmiss=n 1 < out/fill.sam; echo "exit $$?"; $(p) -k 1 's::,qry_seq' < out/fill.sam; echo "exit $$?"; } > o.sam-fill-edge.test 2>&1
	{ for f in qry3mm.sam.gz qry3vs.sam.gz; do for nf in '' --sam-nonf; do $(p) -q --sam/ref3.fa $$nf @@2/none/4 1 't5::,ref_trail5p' 'm::,ref_matched' 't3::,ref_trail3p' 'q5::,qry_trail5p' 'qm::,qry_matched' 'q3::,qry_trail3p' < $$f | md5sum; done; done; $(p) -q --sam/ref3.fa @@2/none/4 1 't5::,ref_trail5p' 'm::,ref_matched' 't3::,ref_trail3p' 'q5::,qry_trail5p' 'qm::,qry_matched' 'q3::,qry_trail3p' < qry3mm.sam.gz | head -n 3 | cut -c 1-100; } > o.sam-parts.test 2>&1
	{ printf '>c1\nACGTACGTAC\nGTTCAGGTAA\n>c2\nacgtnnnnuu\n' > out/parts.fa; printf 'a\t0\tc1\t3\t60\t2S5M1I3M2S\t*\t0\t0\tTTCCCCCGAAAGG\t*\na\t256\tc1\t9\t0\t4S9M\t*\t0\t0\t*\t*\nb\t4\t*\t0\t0\t*\t*\t0\t0\tACGT\t*\nc\t0\tc1\t18\t60\t5M\t*\t0\t0\tAAAAA\t*\ne\t0\tc1\t2\t60\t3D2M1D2M2I\t*\t0\t0\tGGCCTT\t*\nf\t0\tc1\t1\t60\t5M\t*\t0\t0\tACG\t*\ng\t16\tc2\t2\t60\t3H4M1S\t*\t0\t0\tcgtnA\t*\nh\t0\tc1\tx\t60\t5M\t*\t0\t0\tACGTA\t*\n' > out/parts.sam; $(p) --sam/out/parts.fa 1 4 6 't5::,ref_trail5p' 'm::,ref_matched' 't3::,ref_trail3p' 'q5::,qry_trail5p' 'qm::,qry_matched' 'q3::,qry_trail3p' < out/parts.sam; $(p) --sam/out/parts.fa --sam-nonf 1 4 6 't5::,ref_trail5p' 'm::,ref_matched' 't3::,ref_trail3p' 'q5::,qry_trail5p' 'qm::,qry_matched' 'q3::,qry_trail3p' < out/parts.sam; $(p) -q --sam/out/parts.fa 1 'm::,ref_matched' < out/parts.sam; { printf '@SQ\tSN:c1\tLN:20\n'; cat out/parts.sam; } | $(p) --sam-h/out/parts.fa 1 'm::,ref_matched' 'qm::,qry_matched'; $(p) --sam 1 'qm::,qry_matched' < out/parts.sam; { cat out/parts.sam; printf 'd\t0\tc9\t1\t60\t5M\t*\t0\t0\tACGTA\t*\n'; } | $(p) --sam/out/parts.fa 1 'm::,ref_matched'; echo "exit $$?"; { cat out/parts.sam; printf 'd\t0\tc9\t1\t60\t5M\t*\t0\t0\tACGTA\t*\n'; } | $(p) --sam/out/parts.fa 1 'q::,qry_matched'; echo "exit $$?"; $(p) --sam 1 'm::,ref_matched' < out/parts.sam; echo "exit $$?"; $(p) --fasta-dict-R=out/parts.fa --sam 1 't3::,ref_trail3p' < out/parts.sam; echo "exit $$?"; $(p) -k 1 't5::,qry_trail5p' < out/parts.sam; echo "exit $$?"; } > o.sam-parts-edge.test 2>&1
	{ for f in qry3mm.sam.gz qry3vs.sam.gz; do for o in '' --sam-nonf --sam-aln-mrk '--sam-nonf --sam-aln-mrk --sam-qlt-sim --sam-aln-prefix=++%20'; do $(p) -q --sam/ref3.fa $$o @@2/none/4 1 'r::,aln_ref' 'a::,aln_aln' 'q::,aln_qry' 'l::,aln_rlr' 't::,aln_qlt' 'al::,aln_all' 'aq::,aln_allq' 'ap::,aln_all_pack' < $$f | md5sum; done; done; $(p) -q --sam/ref3.fa --sam-aln-mrk '--sam-aln-prefix=++ ' @@2/none/4 1 'x::,aln_allq' < qry3vs.sam.gz | head -n 6 | cut -c 1-100; } > o.sam-aln.test 2>&1
	{ printf '>c1\nACGTACGTACGTTCAGGTAACCGGTTAAGCTTGA\n>c2\nacgtnnnnuuACGU\n' > out/aln.fa; printf 'n1\t0\tc1\t3\t60\t2S4M3N5M1S\t*\t0\t0\tTTGTACTCAGGA\tABCDEFGHIJKL\nn2\t0\tc1\t5\t60\t3=1X2=\t*\t0\t0\tACGAAC\t*\nn3\t0\tc1\t2\t60\t2H3M2P3M\t*\t0\t0\tCGTCGT\t######\nn4\t0\tc2\t1\t60\t14M\t*\t0\t0\tACGTNNNNTTacgt\t*\nn5\t0\tc1\t1\t60\t3I2D6M2D2I\t*\t0\t0\tGGGTACGTATT\t*\nn6\t0\tc1\t30\t60\t8M\t*\t0\t0\tAAAAAAAA\t*\nn7\t0\tc1\t8\t60\t2M2D3M\t*\t0\t0\tACAGG\t!+5?I\nn8\t4\t*\t0\t0\t*\t*\t0\t0\tACGT\t*\nn9\t0\tc1\t1\t60\t5M\t*\t0\t0\tACGTA\tII\n' > out/aln.sam; for o in '' --sam-nonf '--sam-aln-mrk --sam-qlt-sim'; do echo "== $$o"; $(p) --sam/out/aln.fa $$o 1 4 6 'x::,aln_allq' 'q::,qry_qual' < out/aln.sam; done; $(p) -q --sam/out/aln.fa '--sam-aln-prefix=++ ' 1 'p::,aln_all_pack' < out/aln.sam | head -n 2 | od -c | head -n 4; $(p) -q --sam/out/aln.fa '--sam-aln-prefix=> ' 1 'x::,aln_all' 'r::,aln_ref' < out/aln.sam | head -n 5; $(p) -q --sam-qlt-sim --sam 1 'q::,qry_qual' < out/aln.sam; $(p) --sam 1 'x::,aln_aln' < out/aln.sam; echo "exit $$?"; for o in --sam-aln-mrk --sam-qlt-sim --sam-aln-prefix=x; do $(p) $$o 1 < out/aln.sam; echo "exit $$?"; done; } > o.sam-aln-edge.test 2>&1
	{ for f in qry3mm.sam.gz qry3vs.sam.gz; do for o in '' --sam-nonf; do $(p) -q --sam/ref3.fa $$o @@2/none/4 1 'pa::,aln_nedit_pia' 'pu::,aln_nedit_piu' 'p0::^0,aln_posinfo' 'p3::^3,aln_posinfo' < $$f | md5sum; done; done; $(p) -q --sam/ref3.fa @@2/none/4 1 'pa::,aln_nedit_pia' 'pu::,aln_nedit_piu' 'p0::^0,aln_posinfo' 'p3::^3,aln_posinfo' < qry3mm.sam.gz | head -n 3 | cut -c 1-120; } > o.sam-edits.test 2>&1
	{ printf 'm1\t0\tc2\t1\t60\t14M\t*\t0\t0\tACGTNnNNTTACGT\t*\n' | cat out/aln.sam - > out/edits.sam; for o in '' --sam-nonf; do $(p) --sam/out/aln.fa $$o 1 6 'pa::,aln_nedit_pia' 'pu::,aln_nedit_piu' 'p0::^0,aln_posinfo' 'p3::^3,aln_posinfo' < out/edits.sam; done; $(p) -q --sam/out/aln.fa 1 'a::^-1,aln_posinfo' 'b::^x,aln_posinfo' 'c::^1.5,aln_posinfo' 'd::4,aln_posinfo' < out/edits.sam; { cat out/edits.sam; printf 'd\t0\tc9\t1\t60\t5M\t*\t0\t0\tACGTA\t*\n'; } | $(p) -q --sam/out/aln.fa 1 'e::,aln_nedit_pia' ; echo "exit $$?"; $(p) --sam 1 'p::^0,aln_posinfo' < out/edits.sam; echo "exit $$?"; } > o.sam-edits-edge.test 2>&1
	{ for f in qry3mm.sam.gz qry3vs.sam.gz; do for o in '' --sam-nonf; do $(p) -q --sam/ref3.fa $$o @@2/none/4 1 'a::,aln_aln' 'p1::a^10,patchiness' 'p2::a^10,patchiness2' 'p3::a^0,patchiness' 'p4::a^50,patchiness2' < $$f | md5sum; done; done; $(p) -q --sam/ref3.fa @@2/none/4 1 'a::,aln_aln' 'p1::a^10,patchiness' 'p2::a^10,patchiness2' 'p3::a^0,patchiness' 'p4::a^50,patchiness2' < qry3mm.sam.gz | cut -f 1,3- | head -n 5; } > o.sam-patchiness.test 2>&1
	{ head -n 1 out/aln.sam > out/one.sam; for s in '' x '(x)%20x' '|.+~-_`=x' 'xx|x||||||x-x' '~~~x---x___x' 'x%0Ax' ab; do for d in 0 1 3; do printf '%s %s\t' "$$s" $$d; $(p) --sam "p::^$$s^$$d,patchiness" "q::^$$s^$$d,patchiness2" < out/one.sam; done; done; $(p) --sam 'b::^x^-1,patchiness' 'c::^x^1.5,patchiness2' 'd::^x^y,patchiness' 'e::^x^99999999999999999999,patchiness' < out/one.sam; $(p) -k 'a::^x^3,patchiness' < out/one.sam; echo "exit $$?"; } > o.sam-patchiness-edge.test 2>&1
	{ for f in qry3mm.sam.gz qry3vs.sam.gz; do for c in 0 10 40 90; do $(p) -q --sam/ref3.fa --sam-aln-mrk --sam-aln-context=$$c @@2/none/4 1 4 6 'a::,qry_trail5p_N' 'b::,qry_matched_N' 'c::,qry_trail3p_N' 'd::,ref_trail5p_N' 'e::,ref_matched_N' 'f::,ref_trail3p_N' 'g::,ref_trail5p_V' 'h::,ref_trail3p_V' 'al::,aln_allq' < $$f | md5sum; done; $(p) -q --sam/ref3.fa --sam-nonf --sam-aln-context=40 @@2/none/4 1 4 6 'a::,qry_trail5p_N' 'b::,qry_matched_N' 'c::,qry_trail3p_N' 'd::,ref_trail5p_N' 'e::,ref_matched_N' 'f::,ref_trail3p_N' 'g::,ref_trail5p_V' 'h::,ref_trail3p_V' 'al::,aln_allq' < $$f | md5sum; done; for c in 0 10 40 90; do $(p) -q --sam/ref3.fa --sam-aln-context=$$c @@2/none/4 'a::,qry_trail5p_N' 'b::,qry_matched_N' 'c::,qry_trail3p_N' 'd::,ref_trail5p_N' 'e::,ref_matched_N' 'f::,ref_trail3p_N' < qry3mm.sam.gz | md5sum; done; $(p) -q --sam/ref3.fa --sam-aln-mrk --sam-aln-context=10 @@2/none/4 1 4 6 'al::,aln_all' < qry3mm.sam.gz | head -n 5 | cut -c 1-100; } > o.sam-context.test 2>&1
	{ printf '>c1\nAAAAACCCCCGGGGGTTTTTAAAAACCCCCGGGGGTTTTT\n' > out/ctx.fa; printf 'r1\t0\tc1\t11\t60\t3S10M4S\t*\t0\t0\tTTTCCCCCGGGGGAAAA\t*\tNM:i:0\nr2\t0\tc1\t3\t60\t5S10M\t*\t0\t0\tGGGGGAAACCCCCGG\t*\nr3\t16\tc1\t5\t60\t2H3S10M1H\t*\t0\t0\tTTTACCCCCGGGG\t*\nr4\t0\tc1\t1\t60\t3S5M2X3M\t*\t0\t0\tAAAAAAAACCTTT\t*\nr5\t4\tc1\t9\t0\t3S10M\t*\t0\t0\tACGTACGTACGTA\t*\nr6\t0\tc1\t20\t60\t*\t*\t0\t0\tACGT\t*\nr7\t0\tc1\t30\t60\t2S10M\t*\t0\t0\tTTAAAAACCCCC\t*\n' > out/ctx.sam; for o in --sam-aln-context=2 --sam-aln-context=10 --sam-aln-xy=8,30 --sam-aln-xy=12,18 '--sam-aln-xy=8,30 --sam-aln-context=1' '--sam-aln-context=3 --sam-nonf'; do echo "== $$o"; $(p) --sam/out/ctx.fa $$o 1 2 4 6 'a::,aln_aln' 'pa::,aln_nedit_pia' 'pu::,aln_nedit_piu' 'ps::^0,aln_posinfo' 'd::,ref_trail5p_N' 'e::,ref_matched_N' 'v::,ref_trail5p_V' < out/ctx.sam; done; $(p) -q --sam/out/ctx.fa --sam-aln-context=3 1 'x::,aln_all' < out/ctx.sam | head -n 8; $(p) --sam --sam-aln-context=3 -A < out/ctx.sam; { printf '@SQ\tSN:c1\tLN:40\n'; cat out/ctx.sam; } | $(p) --sam-h --sam-aln-context=3 -A; $(p) --sam --sam-aln-context=3 '@@4/eq/11' '@4/eq/8' 1 4 6 < out/ctx.sam; $(p) -q --sam --sam-aln-context=2 -c < out/ctx.sam; for o in --sam-aln-context=-1 --sam-aln-context=x --sam-aln-context= --sam-aln-xy=3 --sam-aln-xy=0,4 --sam-aln-xy=5,4 --sam-aln-xy=a,b --sam-aln-xy=1,%2B2; do $(p) --sam $$o 1 < out/ctx.sam; echo "exit $$?"; done; $(p) --sam-aln-context=1 1 < out/ctx.sam; echo "exit $$?"; $(p) --sam-aln-xy=1,2 1 < out/ctx.sam; echo "exit $$?"; } > o.sam-context-edge.test 2>&1
	{ for f in qry3mm.sam.gz qry3vs.sam.gz; do for o in --sam-narrow-xy=100,200 --sam-narrow-xy=1,40 --sam-narrow-xy=50,330 '--sam-narrow-xy=50,330 --sam-nonf' '--sam-narrow-xy=100,200 --sam-aln-context=20' '--sam-narrow-xy=150,160 --sam-nonf --sam-aln-context=5 --sam-aln-mrk'; do $(p) -q --sam/ref3.fa $$o @@2/none/4 1 4 6 'a::,qry_trail5p_N' 'b::,qry_matched_N' 'c::,qry_trail3p_N' 'd::,ref_trail5p_N' 'e::,ref_matched_N' 'f::,ref_trail3p_N' 'al::,aln_allq' < $$f | md5sum; done; done; for o in --sam-narrow-xy=50,330 '--sam-narrow-xy=50,330 --sam-aln-xy=50,330' '--sam-narrow-xy=300,400'; do $(p) --sam/ref3.fa $$o @@2/none/4 1 2 4 6 < qry3vs.sam.gz | head -n 3; done; } > o.sam-narrow.test 2>&1
	{ printf 'n1\t0\tc1\t1\t60\t5M2I5M\t*\t0\t0\tAAAAATTCCCCC\t*\nn2\t16\tc1\t1\t60\t5M3D5M\t*\t0\t0\tAAAAACCGGG\t*\nn3\t0\tc1\t1\t60\t5M3N5M2H\t*\t0\t0\tAAAAACCGGG\t*\nn4\t272\tc1\t21\t0\t10M\t*\t0\t0\tAAAAACCCCC\t*\nn5\t0\tc1\t1\t60\t12D5M3D\t*\t0\t0\tGGGTT\t*\nn6\t4\tc1\t1\t0\t10M\t*\t0\t0\tAAAAACCCCC\t*\nn7\tx\tc1\t30\t0\t5M\t*\t0\t0\tAAAAA\t*\n' > out/nar.sam; for o in --sam-narrow-xy=6,10 --sam-narrow-xy=1,5 --sam-narrow-xy=7,20 --sam-narrow-xy=1,6 --sam-narrow-xy=13,14 '--sam-narrow-xy=6,10 --sam-aln-context=3' '--sam-narrow-xy=13,16 --sam-nonf'; do echo "== $$o"; $(p) --sam/out/ctx.fa $$o 1 2 4 6 'a::,aln_aln' 'd::,ref_trail5p_N' 'e::,ref_matched_N' < out/nar.sam; done; $(p) --sam/out/ctx.fa --sam-narrow-xy=14,18 1 2 4 6 'a::,aln_aln' < out/ctx.sam; $(p) --sam --sam-narrow-xy=3,8 -A < out/nar.sam; $(p) --sam --sam-narrow-xy=3,8 '@@4/eq/1' '@2/eq/4' 1 2 4 6 < out/nar.sam; for o in --sam-narrow-xy=0,4 --sam-narrow-xy=5,4 --sam-narrow-xy=4; do $(p) --sam $$o 1 < out/nar.sam; echo "exit $$?"; done; $(p) --sam-narrow-xy=1,2 1 < out/nar.sam; echo "exit $$?"; } > o.sam-narrow-edge.test 2>&1
	{ printf 'm1\t0\tc1\t1\t60\t10M9N5M\t*\t0\t0\tAAAAACCCCCTTTTT\t*\tNM:i:1\nm2\t0\tc1\t1\t60\t10M10N5M\t*\t0\t0\tAAAAACCCCCTTTTT\t*\tNM:i:1\nm3\t0\tc1\t21\t60\t10M4N5M\t*\t0\t0\tAAAAACCCCCTTTTT\t*\tAS:i:5\tNM:i:0\tMD:Z:x\nm4\t0\tc1\t20\t60\t10M4N5M\t*\t0\t0\tAAAAACCCCCTTTTT\t*\nm5\t0\tc2\t1\t60\t10M9N5M\t*\t0\t0\tAAAAACCCCCTTTTT\t*\tNM:i:1\nm6\t4\tc1\t1\t0\t10M9N5M\t*\t0\t0\tAAAAACCCCCTTTTT\t*\tNM:i:1\nm7\t0\tc1\t1\t60\t2M2N3M2N3M\t*\t0\t0\tAAAAACCCCC\t*\tNM:i:x\n' > out/nd.sam; printf 'c1\t20-30\nc3\t\n' > out/iv.txt; printf 'c1\t20-30,0-4\n' > out/iv-bad.txt; $(p) --sam --fd-IV=out/iv.txt --sam-ND-adjust=IV -A < out/nd.sam; $(p) -q --sam --cd-IV=c1:5-6 --sam-ND-adjust=IV 1 6 12 < out/nd.sam; $(p) --sam/out/ctx.fa --fd-IV=out/iv.txt --sam-ND-adjust=IV '@@3=c1' 1 6 'a::,aln_aln' 'pa::,aln_nedit_pia' 'pu::,aln_nedit_piu' < out/nd.sam; $(p) -q --sam --fd-IV=out/iv.txt --sam-ND-adjust=IV --sam-narrow-xy=8,25 1 4 6 12 < out/nd.sam; $(p) -q --sam --fd-IV=out/iv.txt --sam-ND-adjust=IV '@@6~N' '@6~D' 1 6 < out/nd.sam; $(p) --sam --sam-ND-adjust=IV 1 < out/nd.sam; echo "exit $$?"; $(p) --sam --fd-IV=out/iv-bad.txt --sam-ND-adjust=IV 1 < out/nd.sam; echo "exit $$?"; $(p) --sam --sam-ND-adjust= 1 < out/nd.sam; echo "exit $$?"; $(p) --fd-IV=out/iv.txt --sam-ND-adjust=IV 1 < out/nd.sam; echo "exit $$?"; } > o.sam-nd-adjust.test 2>&1
	{ for f in qry3mm.sam.gz qry3vs.sam.gz; do for r in 51,150 1,100 330,351; do for o in '' --sam-nonf --sam-aln-context=20; do $(p) -q --sam/ref3.fa --sam-rbt=$$r $$o @@2/none/4 1 4 6 'rx::,rbt_refx' 'ry::,rbt_refy' 'dx::,rbt_dx' 'dy::,rbt_dy' 'qx::,rbt_qryx' 'qy::,rbt_qryy' 'ql::,rbt_qrylen' 'n::,rbt_nmatch' 'nx::,rbt_nmatchx' 'd::,rbt_ndel' 'i::,rbt_nins' 'e::,rbt_nedit' 'q::,rbt_qry' < $$f | md5sum; done; done; done; $(p) -q --sam/ref3.fa --sam-rbt=51,150 @@2/none/4 1 4 6 'rx::,rbt_refx' 'ry::,rbt_refy' 'dx::,rbt_dx' 'dy::,rbt_dy' 'qx::,rbt_qryx' 'qy::,rbt_qryy' 'ql::,rbt_qrylen' 'n::,rbt_nmatch' 'nx::,rbt_nmatchx' 'd::,rbt_ndel' 'i::,rbt_nins' 'e::,rbt_nedit' 'q::,rbt_qry' < qry3mm.sam.gz | head -n 3 | cut -c 1-150; $(p) --sam/ref3.fa --sam-rbt=330,351 -c @@2/none/4 1 'e::,rbt_nedit' < qry3mm.sam.gz; } > o.sam-rbt.test 2>&1
	{ printf '>c1\nAAAAACCCCCGGGGGTTTTTAAAAACCCCCGGGGGTTTTT\n' > out/rbt.fa; printf 'w1\t0\tc1\t5\t60\t2S4M3D4M2I4M2S\t*\t0\t0\tTTACCCGGGTAATTTTGG\t*\nw2\t0\tc1\t3\t60\t3=1X2N3M\t*\t0\t0\tAAATCCG\t*\nw3\t4\t*\t0\t0\t*\t*\t0\t0\tACGT\t*\n' > out/rbt.sam; for r in 1,4 1,6 7,9 8,12 9,11 12,13 14,15 15,16 15,30 30,40 3,9; do printf '%s\t' $$r; $(p) -q --sam/out/rbt.fa --sam-rbt=$$r 1 4 6 'rx::,rbt_refx' 'ry::,rbt_refy' 'dx::,rbt_dx' 'dy::,rbt_dy' 'qx::,rbt_qryx' 'qy::,rbt_qryy' 'ql::,rbt_qrylen' 'n::,rbt_nmatch' 'nx::,rbt_nmatchx' 'd::,rbt_ndel' 'i::,rbt_nins' 'e::,rbt_nedit' 'q::,rbt_qry' < out/rbt.sam | tr '\n' ' '; echo; done; $(p) --sam/out/rbt.fa 1 'r:=^8^12,sam_rbt' 'a::,rbt_nedit' 's:=^14^15,sam_rbt' 'b::,rbt_nedit' 'c::,rbt_refx' < out/rbt.sam; $(p) -q --sam/out/rbt.fa 1 'r:=4^12,sam_rbt' 'a::,rbt_refx' 'b::,rbt_refy' < out/rbt.sam; $(p) --sam/out/rbt.fa --sam-rbt=1,6 1 'a::,rbt_refx' 'r:=^9^8,sam_rbt' 'b::,rbt_refx' 'r2:=^x^8,sam_rbt' 'c::,rbt_refx' 'r3:=^0^8,sam_rbt' 'd::,rbt_refx' 'r4:=^1.5^8,sam_rbt' 'e::,rbt_refx' < out/rbt.sam; $(p) --sam/out/rbt.fa 1 'r::^8^12,sam_rbt' 'a::,rbt_refx' < out/rbt.sam; $(p) --sam/out/rbt.fa --sam-rbt=1,6 1 < out/rbt.sam; $(p) --sam/out/rbt.fa 1 'a::,rbt_refx' 'r:=^8^12,sam_rbt' < out/rbt.sam; echo "exit $$?"; $(p) --sam 1 'r:=^8^12,sam_rbt' 'a::,rbt_refx' < out/rbt.sam; echo "exit $$?"; for o in --sam-rbt=0,4 --sam-rbt=5,4 --sam-rbt=4; do $(p) --sam $$o 1 < out/rbt.sam; echo "exit $$?"; done; $(p) --sam-rbt=1,2 1 < out/rbt.sam; echo "exit $$?"; $(p) -l rbt; } > o.sam-rbt-edge.test 2>&1
	{ for f in qry3mm.sam.gz qry3vs.sam.gz; do for r in 51,150 330,351; do for o in '' '--sam-aln-context=20 --sam-aln-mrk' "--sam-qlt-sim '--sam-aln-prefix=++ '"; do eval $(p) -q --sam/ref3.fa --sam-rbt=$$r $$o @@2/none/4 1 "4 6 'a::,rbt_alnref' 'b::,rbt_alnaln' 'c::,rbt_alnqry' 'd::,rbt_alnrlr' 'e::,rbt_alnqlt' 'f::,rbt_alnall'" < $$f | md5sum; done; done; done; $(p) -q --sam/ref3.fa '--sam-aln-prefix=++ ' --sam-rbt=101,130 @@2/none/4 1 'f::,rbt_alnall' < qry3mm.sam.gz | head -n 12; for r in 1,6 8,12 9,11 14,15 15,16 15,30 30,40 3,9 7,8; do echo "== $$r"; $(p) -q --sam/out/rbt.fa --sam-rbt=$$r --sam-aln-mrk 1 4 6 'a::,rbt_alnref' 'b::,rbt_alnaln' 'c::,rbt_alnqry' 'd::,rbt_alnrlr' 'e::,rbt_alnqlt' 'f::,rbt_alnall' < out/rbt.sam; done; $(p) -q --sam/out/rbt.fa 1 'r:=^8^12,sam_rbt' 'a::,rbt_alnaln' 's:=^14^16,sam_rbt' 'b::,rbt_alnaln' 'c::,aln_aln' < out/rbt.sam; } > o.sam-rbt-aln.test 2>&1
	{ printf 'ACGTN\tacgtn\t0\nRYKMSWBDHVN\tUUAG\t1\n\t-.*x\tfoo\n' > out/bio.txt; $(p) -k 'a::1,rc' 'b::2,rc' 'c::1,todna' 'd::2,todna' 'e::1,rc,rc' < out/bio.txt; $(p) -k 'a::3:1,forward' 'b::^0:2,forward' 'c::^:2,forward' 'd::^x:2,forward' 'e::^0.0:1,forward' 'f::^-1:1,forward' < out/bio.txt; $(p) -k 'a::^x,sqrt,rc' 'b::^x,sqrt,todna' 'c::^ACG^x,sqrt,xch,forward' 'd::^ACG^x,sqrt,forward' < out/bio.txt | head -n 1; for f in qry3mm.sam.gz qry3vs.sam.gz; do $(p) -q --sam 1 's::,qry_seq' 'f::,isforward,qry_seq,forward' < $$f | md5sum; $(p) -q --sam 's::,qry_seq' 'r::,qry_seq,rc,rc' < $$f | awk -F'\t' '$$1 != $$2' | wc -l; done; $(p) -q --sam 1 2 'f::,isforward,qry_seq,forward' < qry3mm.sam.gz | awk -F'\t' '$$2 == 16' | head -n 1 | cut -c 1-80; $(p) -l bio; } > o.bio-transform.test 2>&1
	{ printf 'ATGGCCTAA\nAUGUUU\nXATGGC\nATGNNNTGA\nNNNN\n\nACGTNNNN\natg\n' > out/tl.txt; $(p) -k 1 't0::1^0,dnatl' 't1::1^1,dnatl' 't2::1^2,dnatl' 't9::1^9,dnatl' 'g2::1^2,dnagc' 'g0::1^0,dnagc' < out/tl.txt; $(p) -k 'a::1^-1,dnatl' 'b::1^x,dnatl' 'c::1^1.5,dnatl' 'd::1^-1,dnagc' 'e::1^1001,dnagc' 'f::1^x,dnagc' 'g::^x,sqrt^0,dnatl' 'h::^x,sqrt^1,dnagc' < out/tl.txt | head -n 1; awk '/^>/{if(s)print n"\t"s; n=substr($$0,2); s=""; next}{s=s $$0}END{print n"\t"s}' ref3.fa > out/ref3.txt; for f in 0 1 2 3; do $(p) -k 1 "t::2^$$f,dnatl" "g::2^$$f,dnagc" < out/ref3.txt | md5sum; done; $(p) -k 1 't::2^0,dnatl' < out/ref3.txt | cut -c 1-60; $(p) -q --sam 1 't::,qry_seq^1,dnatl' 'g::,qry_seq^3,dnagc' < qry3mm.sam.gz | md5sum; $(p) -q --sam 1 'g::,qry_seq^3,dnagc' 't::,isforward,qry_seq,forward^0,dnatl' < qry3mm.sam.gz | head -n 2 | cut -c 1-70; } > o.bio-stats.test 2>&1
	{ printf 'r1\tACGT\t!#%%&\tACCTA\nr2\tA\t*\ta\nr3\tACG\t*\tACG\nr4\tACG\t!!\t\nr5\t\t*\tx\n' > out/rec.txt; $(p) -k 'f::1:2,fasta' 'q::1:2,fastq' 'Q::1:2:3,fastQ' 'p::2:4,strpair' < out/rec.txt; $(p) -q -k 'Q::1:2:3,fastQ' < out/rec.txt | tail -n 2; $(p) -k 'a::^x,sqrt^ACG,fasta' 'b::^ACG^x,sqrt,fastq' 'c::^i^ACG^x,sqrt,fastQ' 'd::^x,sqrt^ACG,strpair' < out/rec.txt | head -n 1; for c in 'x::1,qry_seq,fastq' 'x::1,qry_seq,qry_qual,fastQ'; do $(p) -q --sam "$$c" < qry3mm.sam.gz | md5sum; done; $(p) -q --sam/ref3.fa '--sam-aln-prefix=' @@2/none/4 'p::,aln_ref,aln_qry,strpair' < qry3mm.sam.gz | head -n 2 | cut -c 1-60; $(p) -q --sam/ref3.fa @@2/none/4 'p::,aln_ref,aln_qry,strpair^10,patchiness' < qry3mm.sam.gz | head -n 3; $(p) -l bio; } > o.bio-records.test 2>&1
	for a in @z/gt/3 @@name/gt/:z @fib~eq~:z; do $(p) name "$$a" < data.txt; [ $$? -eq 1 ] || exit 1; done 2> o.filter-unknown.test
	$(p) --sam 1 2 4 @2/eq/16 @@4/ge/9 < sam.txt   2> /dev/null > o.filter-sam.test
	$(p) --sam 1 2 4 @2/eq/16 @@4/ge/9 < sam.bam   > o.filter-bam.test
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
	{ $(p) -z 'a:b^c' 'x%y' 'p,q/r~s=t@u+v' 'a.b c' 1.5 -k ''; $(p) -z "$$(printf 'tab\there\nnl\x7f\xe9')"; $(p) -zz 'a%3Ab' 'a%20b' '%zz%4' 'x%2c%2C' -k; $(p) -zz "$$($(p) -z 'p,q/r~s=t@u+v%')"; $(p) -z; $(p) -zz | head -n 1; } > o.z-encode.test
	{ for a in -E12 -E4 -E3 -cE4 -cE5 -E0; do $(p) $$a name @fib/gt/30 < data.txt; echo "exit $$?"; done; $(p) -E0 name < /dev/null; echo "exit $$?"; $(p) -E1 --demux=fx --demux-dir=out name '@fib/gt/30' < data.txt; echo "exit $$?"; $(p) -E9 --other=out/e.other name '@fib/gt/3' < data.txt > /dev/null; echo "exit $$?"; for a in -E -Ex -E-1; do $(p) $$a name < data.txt; echo "exit $$?"; done; } > o.expect-rows.test 2>&1
	$(p) -e name 'x:: fib ^1 ,add' '@ fib /gt/ 3' '@@ sq /lt/ 100' 'y:=	name ^%20 , len' 'z::y' < data.txt > o.squash.test
	{ $(p) name 'x:: fib ^1 ,add' < data.txt; echo "exit $$?"; $(p) -e 'na me' < data.txt; echo "exit $$?"; } > o.squash-edge.test 2>&1
	{ for q in '' -q -qq; do echo "== $$q"; $(p) $$q name '@fx/gt/1' < data.txt; $(p) $$q -k 1-20 < data.txt > out/q.out; head -n 1 out/q.out; $(p) $$q --cd-D=a:1 --cd-D=b:2 name < data.txt > out/q.out; head -n 1 out/q.out; $(p) $$q --demux=fx --demux-dir=out name '@fib/gt/30' < data.txt; $(p) $$q --assert-other=out/q.assert name '@assert@fib/lt/30' < data.txt > out/q.out; head -n 1 out/q.out; $(p) $$q --sam 1 < sam.txt > out/q.out; head -n 1 out/q.out; $(p) $$q name '@fx=fib' < data.txt > out/q.out; head -n 1 out/q.out; $(p) $$q name '@zz/gt/1' < data.txt; echo "exit $$?"; done; } > o.quiet.test 2>&1
	{ $(p) --A=fib 'x::fib^1,add' 'y::sq^2,mul' < data.txt; $(p) --A=dna 'x::fib^1,add' < data.txt | head -n 2; $(p) --A=name 'x::fib^1,add' '@fib/gt/30' < data.txt; $(p) -k --A=2 'x::1^%3A,cat' < data.txt | head -n 2; $(p) --A=fib -A2 'x::fib' < data.txt | head -n 1; $(p) -A2 --A=sq 'x::fib' < data.txt | head -n 1; $(p) --A=n%61me 'x::fib' < data.txt | head -n 1; } > o.a-name.test
	{ $(p) --idx-list name sq 'x::fib^1,add' < data.txt; $(p) --name-list name sq 'x::fib^1,add' < data.txt; $(p) --idx-map name sq 'x::fib^1,add' < data.txt; $(p) -q --idx-list name 'x::fib' < data.txt; $(p) --idx-list -A < data.txt; $(p) --idx-map -x name fib < data.txt; $(p) --idx-list sq name < data.txt; $(p) --idx-map -k 3-5 < data.txt; $(p) --idx-list -kA < data.txt; $(p) --idx-list --sam-h 1 3 < sam.txt; $(p) --idx-list name < /dev/null; echo "exit $$?"; } > o.list-cols.test 2>&1
	{ for a in --progress=2/3 --progress=5/0 '--progress=2/3 @fib/gt/30' '--progress=4/1 -c' '--progress=13/1' '--progress=1/0 -h @fib/gt/1000'; do echo "== $$a"; $(p) $$a name < data.txt 2>&1 > /dev/null; done; $(p) --progress name < data.txt 2>&1 > /dev/null | od -c | head -n 2; } > o.progress.test
	{ for a in --A=nosuch --A= '--A=fib -x name' '--A=x' '--idx-list -c' '--idx-list --demux=fx' '--name-list --other=out/o' '--idx-map --assert-other=out/a' '--idx-list --name-list' --progress=0/1 --progress=3 --progress=3/ --progress=/3 --progress=a/b; do $(p) $$a name 'x::fib' < data.txt; echo "exit $$?"; done; } > o.opts-edge.test 2>&1
	{ $(p) -l; $(p) -l bit,time md5; $(p) -k -l filter; $(p) -l eq isin; $(p) -l -h; echo "exit $$?"; $(p) -l nosuch,zz; echo "exit $$?"; $(p) -l | head -n 1; } > o.list-ops.test 2>&1
	{ $(p) -kl; echo "exit $$?"; $(p) -ll; echo "exit $$?"; $(p) name -l < data.txt; echo "exit $$?"; } > o.list-errors.test 2>&1
	{ $(p) -zzz a; echo "exit $$?"; $(p) -kz a; echo "exit $$?"; $(p) name -z < data.txt; echo "exit $$?"; } > o.z-errors.test 2>&1
	seq 1 1000000 > seq.tmp; $(p) --test-lineno < seq.tmp | head -n 2 > o.brokenpipe.test; [ $${PIPESTATUS[0]} -eq 0 ]; rm seq.tmp

# Tests for temporary --test-* options. When such an option is removed, remove
# the target from the 'test' prerequisites (keep the tests) and note it in the
# plan step file. See plan/guidelines.txt.

runtestopt: data.txt
	$(p) --test-compute name 'x::fib^1,add' 'y:=+num\d,addall' 'z::x^a,cat' 'd::fx^o,del' 's::^a^b,xch,dup,pop' '::^1:y' < data.txt > o.compute-compile.test
	{ for a in 'name x::^1 fib' 'x::^1 name y:=^2 fib z::^3' '-A x::^1 y:=^2 z::^3' '-x fib x::^1 name' 'name x:=^1 x' 'name ::^1 ::^2' '-k 1 x::2^1'; do echo "$$a"; $(p) --test-select $$a < data.txt; done; } > o.compute-select.test
	$(p) --test-compute 'x::fib^1,add' 'y:=^a:fib,cat' '::name,dup,xch,pop' '::^' 's::+num\d,addall' 'a%2Fb::^x%2Cy%3Az%5E' 'd::fx^o,del' name '::^2' < /dev/null > o.compute-parse.test
	$(p) --test-compute -k 'r::2-4,addall' '5::^x' < /dev/null > o.compute-parse-k.test
	for a in 'x::' 'x::,' 'x::^a:' 'x::^a,nosuch' 'p/x/s::^1' 'x::+(a' 'x::^1,addall,nosuch' '+(::^1'; do $(p) --test-compute name "$$a" < /dev/null; [ $$? -eq 1 ] || exit 1; done 2> o.compute-parse-errors.test
	$(p) --test-compute -k '2-3::__^1,add' '+x.*:=^1' 'a:b::^1' < /dev/null > o.compute-parse-map.test
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
	$(p) --sam --test-dict                  < sam.txt    > o.seqlen.test
	$(p) --sam-h/sam.fa --test-dict         < sam.txt    > o.seqlen-ref.test      2> o.seqlen-ref-msg.test
	printf '>chr1\nACGT\n>chr3\nGG\n' > fa.tmp; grep -v 'SN:chr2' sam.txt | $(p) --sam/fa.tmp --test-dict  > o.seqlen-conflict.test  2> o.seqlen-conflict-msg.test; rm fa.tmp
	{ printf '@SQ\tSN:x\n@SQ\tLN:5\tSN:y\n@SQ\tSN:z\tLN:abc\n@SQ\tSN:y\tLN:6\n'; cat sam.txt; } | $(p) --sam --test-dict  > o.seqlen-malformed.test  2> o.seqlen-malformed-msg.test
	$(p) --test-dict --cd-a=b               < sam.txt    > o.seqlen-nosam.test
	printf 'k\tseqlen\n1\t2\n' > td.tmp; for a in --fd-seqlen=data.txt --cd-seqlen=a --td=td.tmp; do $(p) --test-dict "$$a" < /dev/null; [ $$? -eq 1 ] || exit 1; done 2> o.seqlen-errors.test; rm td.tmp
	$(p) --sam --test-samsort               < sam.txt    > o.samsort.test
	sed '1s/SO:unsorted/SO:coordinate\tGO:query/' sam.txt | $(p) --sam-h --test-samsort   > o.samsort-coord.test
	sed '1s/SO:unsorted/GO:reference/' sam.txt | $(p) --sam --test-samsort                > o.samsort-goref.test
	sed '1s/SO:unsorted/SO:sorted\tGO:x/' sam.txt | $(p) --sam --test-samsort             > o.samsort-unrec.test 2>&1
	{ grep -v '^@HD' sam.txt; printf '@HD\tVN:1.6\tSO:coordinate\n'; } | $(p) --sam --test-samsort  > o.samsort-late.test 2>&1
	sed '1s/SO:unsorted/SO:coordinate/' sam.txt | $(p) -k --test-samsort                  > o.samsort-nosam.test
	sed '1s/SO:unsorted/SO:coordinate/' sam.txt | $(p) --sam '-/^@HD' --test-samsort      > o.samsort-pat.test
	$(p) --sam-h/sam.fa --test-dict  < sam.bam      > o.bam-seqlen.test 2> o.bam-seqlen-msg.test
	$(p) --sam --test-dict            < bam-nosq.bam > o.bam-nosq-seqlen.test
	$(p) --sam --test-samsort         < bam-tags.bam > o.bam-samsort.test

clean:
	rm -f o.*.test
	rm -rf out

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
