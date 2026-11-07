# The pluk manual

*pluk is a Rust reimplementation of pick, a Perl program by Stijn van Dongen that
served as its prototype. All of pluk's code was written by Claude (Anthropic), in a
continuous dialogue with Stijn. The work was staged into steps, each a
small set of items; every step was discussed and its decisions written down before any
code was written, then implemented item by item, tested against pick and committed
by Stijn over a period of about three days. The `plan/` directory retains that history.
Many corner/edge cases in pick were settled and sometimes fixed.*


*This manual is being written. Sections 1 to 14 are outlines; section 15 lists every operator, with descriptions to follow.*


## 1. Introduction

What pluk is, in brief (see [INTRODUCTION.md](INTRODUCTION.md) for a tour); building pluk (cargo; the int128 build); the crates pluk uses and why.


## 2. Quick start

Common tasks, one line each, with their output.


## 3. Concepts

Header and rows, column names, `-k` and `-K`, bytes, %-encoding (`-z`, `-zz`), numbers, NA, messages and exit status, `-q`.


## 4. Selecting columns

Names, `:` lists, `+` regular expressions, `-k` ranges; `-A`, `-A<N>`, `--A=NAME`, `-x`, `-U`; `-u`; `-R`; ragged input with `-O<N>`.


## 5. Filters and assertions

`@` and `@@` filters, the operators by kind, multi-column handles, `-o` and `-s`, `@assert@` and `@@assert@`.


## 6. Computes

`NAME::SPEC` and `NAME:=SPEC`, the stack language, unnamed computes, `-i` and `__`, maps over lists, regular expressions and ranges, renaming (`P/SPEC/S`, `SPEC/S`, `P//SPEC//S`) and capture group 1.


## 7. Dictionaries and quantiles

`--kd`, `--fd`, `--cd`, `--td`, `--fasta-dict`, defaults and `__EXIT__`, `map` and `dord`, `~isin~` and friends; `--quant-NAME` with `qnl` and `qnu`.


## 8. State across rows

Registers (`save`, `fetch`), `rowno`, `r0wno`, `lineno`, `--pstore` and `pload`, `--group` and `--group-first-ref` with `groupi` and `groupno`.


## 9. Input and output

Compressed input, `-/PAT` and `-//PAT`, `-h`, `-c`, `-E<N>`, `--other`, `--assert-other`, `--demux` and `--demux-dir`, `--idx-list`, `--name-list`, `--idx-map`, `--progress`, `--threads`.


## 10. SAM and BAM

`--sam`, `--sam-h`, `--sam/FILE`, normal form, the sequence fill, positions and counts, sequence parts, the alignment display, edits, patchiness, view options, the region-bound tally, `sam_put`; NA (see [SAM-NA.md](SAM-NA.md)).


## 11. Bio operators

`rc`, `forward`, `todna`, `dnatl`, `dnagc`, `fasta`, `fastq`, `fastQ`, `strpair`.


## 12. Recipes

Complex useful patterns, collected as they turn up.


## 13. Messages and reports

The counts pluk reports at the end of a run, `-q`, exit status.


## 14. Differences from pick

The one place where pluk is compared with pick: not taken over; additions without a pick counterpart; changed behaviour by topic; pick's corner cases settled.


## 15. Operator reference

Every operator, by category, in the order of `pluk -l`. The line under each name (operands, result, short description) is generated from `pluk --list-ops-tsv` and checked by the test suite; the text below it is the description.

### stack

#### `dup`
<!-- op dup -->
`x` → `x x` — Duplicate top entry x

#### `pop`
<!-- op pop -->
`x` → `-` — Remove top entry x from stack

#### `xch`
<!-- op xch -->
`x y` → `y x` — Exchange x and y

### string

#### `cat`
<!-- op cat -->
`x y` → `xy` — Concatenation of x and y

#### `len`
<!-- op len -->
`x` → `len(x)` — Length of string x (bytes)

#### `lc`
<!-- op lc -->
`x` → `lc(x)` — Lower case of x (ASCII)

#### `uc`
<!-- op uc -->
`x` → `uc(x)` — Upper case of x (ASCII)

#### `rev`
<!-- op rev -->
`x` → `rev(x)` — String reverse of x (bytes)

#### `rot13`
<!-- op rot13 -->
`x` → `rot13(x)` — Rot13 encoding of x

#### `substr`
<!-- op substr -->
`x i k` → `x[i:i+k-1]` — Substring of x starting at i (zero-based) of length k (as Perl)

#### `head`
<!-- op head -->
`x k` → `x[0:k-1]` — Initial part of string x, of length k [head,tail,skip,shed]

#### `tail`
<!-- op tail -->
`x k` → `x[#x-k:#x-1]` — Trailing part of string x, of length k [head,tail,skip,shed]

#### `skip`
<!-- op skip -->
`x k` → `x[k:#x-1]` — Trailing part of string x, skipping first k [head,tail,skip,shed]

#### `shed`
<!-- op shed -->
`x k` → `x[0:#x-k-1]` — Leading part of string x, shedding last k [head,tail,skip,shed]

#### `strim`
<!-- op strim -->
`s nx ny` → `s'` — Trim nx bytes from the start and ny from the end of s

#### `ppl`
<!-- op ppl -->
`x N` → `x'` — x right-padded with blanks to width N (left-aligned)

#### `ppr`
<!-- op ppr -->
`x N` → `x'` — x left-padded with blanks to width N (right-aligned)

#### `zp`
<!-- op zp -->
`x N` → `x'` — x left zero-padded to width N

#### `nchar`
<!-- op nchar -->
`x c` → `nc` — Number of occurrences of c in x (non-overlapping)

#### `catall`
<!-- op catall -->
`*` → `Stack-joined` — Stringified stack

#### `joinall`
<!-- op joinall -->
`* s` → `Stack-joined-by-s` — Stringified stack with s as separator

#### `join`
<!-- op join -->
`x y j` → `xjy` — Join two elements with string separator

#### `uie`
<!-- op uie -->
`x y` → `x-or-y` — Use x if not empty (and not NA), otherwise use y

#### `urlec`
<!-- op urlec -->
`x` → `urlec(x)` — URL encoding of x (all but A-Z a-z 0-9 _ as %XX)

#### `urldc`
<!-- op urldc -->
`x` → `urldc(x)` — URL decoding of x (%XX)

#### `cgsum`
<!-- op cgsum -->
`c s` → `Sum of s in c` — Sum of the lengths of the s operations in CIGAR c

#### `cgmax`
<!-- op cgmax -->
`c s` → `Max of s in c` — Largest length of an s operation in CIGAR c

#### `cgcount`
<!-- op cgcount -->
`c s` → `Count of s in c` — Number of s operations in CIGAR c

#### `md5`
<!-- op md5 -->
`x` → `md5(x)` — MD5 sum of x (hexadecimal)

#### `fmtall`
<!-- op fmtall -->
`f *` → `Map-*-into-f` — In f replace # instances by stack (\# is a literal #)

### arithmetic

#### `add`
<!-- op add -->
`x y` → `x+y` — Add x and y

#### `sub`
<!-- op sub -->
`x y` → `x-y` — Subtract y from x

#### `mul`
<!-- op mul -->
`x y` → `x*y` — Multiply x and y

#### `div`
<!-- op div -->
`x y` → `x/y` — Division (a float; by zero: inf or nan)

#### `idiv`
<!-- op idiv -->
`x y` → `x // y` — Integer division, floored (by zero: NA)

#### `mod`
<!-- op mod -->
`x y` → `x mod y` — x modulo y, sign of y (by zero: NA)

#### `pow`
<!-- op pow -->
`x y` → `x**y` — x raised to power y

#### `ipow`
<!-- op ipow -->
`x y` → `x**y` — x raised to power y in integers (negative y as pow; overflow: NA)

#### `max`
<!-- op max -->
`x y` → `max(x,y)` — Maximum of x and y

#### `min`
<!-- op min -->
`x y` → `min(x,y)` — Minimum of x and y

#### `neg`
<!-- op neg -->
`x` → `-x` — The sign-reversed value of x

#### `abs`
<!-- op abs -->
`x` → `abs(x)` — Absolute value of x

#### `sign`
<!-- op sign -->
`x` → `sign(x)` — The sign of x (-1, 0 or 1)

#### `sq`
<!-- op sq -->
`x` → `x^2` — Square of x

#### `incr`
<!-- op incr -->
`x` → `x++` — x incremented by one

#### `decr`
<!-- op decr -->
`x` → `x--` — x decremented by one

#### `rand`
<!-- op rand -->
`x` → `rand(x)` — Random real in range [0, x) (x 0: [0, 1); see --seed)

#### `addall`
<!-- op addall -->
`*` → `sum(Stack)` — Sum of all entries in stack

#### `mulall`
<!-- op mulall -->
`*` → `product(Stack)` — Product of all entries in stack

#### `maxall`
<!-- op maxall -->
`*` → `max(Stack)` — Max over all entries in stack

#### `minall`
<!-- op minall -->
`*` → `min(Stack)` — Min over all entries in stack

#### `meanall`
<!-- op meanall -->
`*` → `mean(Stack)` — Mean of all entries in stack

#### `hmeanall`
<!-- op hmeanall -->
`*` → `hmean(Stack)` — Harmonic mean of the absolute values in stack

#### `gmeanall`
<!-- op gmeanall -->
`*` → `gmean(Stack)` — Geometric mean of the absolute values in stack

### math

#### `int`
<!-- op int -->
`x` → `int(x)` — x truncated towards zero (do not use for rounding)

#### `floor`
<!-- op floor -->
`x` → `floor(x)` — The floor of x

#### `ceil`
<!-- op ceil -->
`x` → `ceil(x)` — The ceil of x

#### `round`
<!-- op round -->
`x` → `round(x)` — The integer nearest to x, halves to even (2.5 -> 2, 3.5 -> 4)

#### `qnl`
<!-- op qnl -->
`x Q` → `q(x)` — Normalised quantile of x, lower tail: (#steps <= x, plus 1)/(n+1) (--quant-Q)

#### `qnu`
<!-- op qnu -->
`x Q` → `q(x)` — Normalised quantile of x, upper tail: (#steps >= x, plus 1)/(n+1)

#### `sqrt`
<!-- op sqrt -->
`x` → `sqrt(x)` — Square root of x

#### `exp`
<!-- op exp -->
`x` → `e**x` — Exponential function applied to x

#### `exp2`
<!-- op exp2 -->
`x` → `2^x` — 2 to the power of x

#### `exp10`
<!-- op exp10 -->
`x` → `10^x` — 10 to the power of x

#### `log`
<!-- op log -->
`x` → `log(x)` — Natural logarithm of x

#### `log2`
<!-- op log2 -->
`x` → `log2(x)` — Logarithm of x in base 2

#### `log10`
<!-- op log10 -->
`x` → `log10(x)` — Logarithm of x in base 10

#### `sin`
<!-- op sin -->
`x` → `sin(x)` — Sine of x

#### `cos`
<!-- op cos -->
`x` → `cos(x)` — Cosine of x

#### `tan`
<!-- op tan -->
`x` → `tan(x)` — Tangent of x

### precision

#### `dd`
<!-- op dd -->
`x N` → `x'` — x written with N decimal digits

#### `sn`
<!-- op sn -->
`x N` → `x'` — x in scientific notation with N decimal digits

#### `frac`
<!-- op frac -->
`x y N` → `x/y` — Fraction x/y with N decimal digits

#### `pct`
<!-- op pct -->
`x y N` → `pct(x/y)` — Percentage of x relative to y with N decimal digits

#### `pml`
<!-- op pml -->
`x y N` → `pml(x/y)` — Per mille of x relative to y with N decimal digits

### dictionary

#### `map`
<!-- op map -->
`x D` → `map-of-x` — Value of x in dictionary D (not found: as D's default)

#### `dord`
<!-- op dord -->
`x D` → `ord-of-x` — Order of x in dictionary D (first appearance, 1-based; NA if absent)

### branch

#### `ifelse`
<!-- op ifelse -->
`B x y` → `x or y` — If B put x else put y (B: a non-zero number, or a non-empty string)

#### `test`
<!-- op test -->
`x y T` → `B` — Test T applied to x y: 1 or 0 (T a filter operator: /gt/ ~eq~ ~ ~isin~ ...)

### state

#### `save`
<!-- op save -->
`x n` → `x` — Store x in register n (per row), x stays

#### `fetch`
<!-- op fetch -->
`n` → `reg(n)` — Value in register n (stored earlier in this row)

#### `pload`
<!-- op pload -->
`c` → `prev(c)` — Column c of the previous row (--pstore; c a constant name)

#### `groupi`
<!-- op groupi -->
`-` → `i` — Position of the row in its group (2 for the first output; --group)

#### `groupno`
<!-- op groupno -->
`-` → `g` — Number of the row's group (1-based; --group)

#### `rowno`
<!-- op rowno -->
`-` → `x` — Current table row number (start one)

#### `r0wno`
<!-- op r0wno -->
`-` → `x` — Current table row number (start zero)

#### `lineno`
<!-- op lineno -->
`-` → `x` — Current input line number

### time

#### `now`
<!-- op now -->
`-` → `t` — Current epoch (unix seconds)

#### `ttu`
<!-- op ttu -->
`tu` → `s` — Time unit (3h, 1.5d; units s m h d w y) in seconds

#### `rttu`
<!-- op rttu -->
`s` → `tu` — Integer seconds as days, hours, minutes, seconds (1d,2h,3m,4s)

#### `todate`
<!-- op todate -->
`u` → `date` — Epoch as local date and time (%Y-%m-%d-%H-%M-%S)

### bit

#### `bitand`
<!-- op bitand -->
`x y` → `x and y` — Bitwise and of |x| and |y|

#### `bitor`
<!-- op bitor -->
`x y` → `x or y` — Bitwise or of |x| and |y|

#### `bitxor`
<!-- op bitxor -->
`x y` → `x xor y` — Bitwise exclusive or of |x| and |y|

#### `bitoff`
<!-- op bitoff -->
`x y` → `x rm y` — Remove the bits of |y| from |x|

#### `bitlsh`
<!-- op bitlsh -->
`x y` → `x << y` — Shift |x| left by |y| (overflow: NA)

#### `bitrsh`
<!-- op bitrsh -->
`x y` → `x >> y` — Shift |x| right by |y|

### radix

#### `tobin`
<!-- op tobin -->
`x` → `x'` — Binary representation of x (negative: -101)

#### `tohex`
<!-- op tohex -->
`x` → `x'` — Hex representation of x (negative: -ff)

#### `tooct`
<!-- op tooct -->
`x` → `x'` — Octal representation of x (negative: -17)

#### `binto`
<!-- op binto -->
`x` → `x'` — Read binary representation x (optional - and 0b)

#### `hexto`
<!-- op hexto -->
`x` → `x'` — Read hex representation x (optional - and 0x)

#### `octto`
<!-- op octto -->
`x` → `x'` — Read octal representation x (optional - and 0o)

### regex

#### `del`
<!-- op del -->
`x p` → `x =~ s/p//` — Delete pattern p in x

#### `delg`
<!-- op delg -->
`x p` → `x =~ s/p//g` — Globally delete pattern p in x

#### `ed`
<!-- op ed -->
`x p s` → `x =~ s/p/s/` — Substitute pattern p by s in x (s literal)

#### `edg`
<!-- op edg -->
`x p s` → `x =~ s/p/s/g` — Globally substitute pattern p by s in x (s literal)

#### `get`
<!-- op get -->
`x p` → `p-match-of-x` — Group 1 of the match of p in x, or the entire match; empty if no match

#### `npat`
<!-- op npat -->
`x p` → `np` — Number of (non-overlapping) matches of p in x

#### `edbr`
<!-- op edbr -->
`x p s` → `x =~ s/p/s/` — As ed, $1 $2 .. (groups) and $0 (match) in s

#### `edgbr`
<!-- op edgbr -->
`x p s` → `x =~ s/p/s/g` — As edg, $1 $2 .. (groups) and $0 (match) in s

### sam

#### `isforward`
<!-- op isforward -->
`-` → `B` — 1 if the alignment is on the forward strand, 0 if reverse (FLAG 0x10)

#### `ref_len`
<!-- op ref_len -->
`-` → `n` — Length of the reference sequence (seqlen: @SQ or fasta)

#### `ref_posx`
<!-- op ref_posx -->
`-` → `p` — First reference position of the matched part (1-based)

#### `ref_posy`
<!-- op ref_posy -->
`-` → `p` — Last reference position of the matched part

#### `qry_len`
<!-- op qry_len -->
`-` → `n` — Query length from the CIGAR (M I S = X)

#### `qry_posx`
<!-- op qry_posx -->
`-` → `p` — First query position of the matched part (1-based)

#### `qry_posy`
<!-- op qry_posy -->
`-` → `p` — Last query position of the matched part

#### `ref_trail5p_N`
<!-- op ref_trail5p_N -->
`-` → `n` — Reference before the matched part (POS-1 + 5' trail)

#### `ref_matched_N`
<!-- op ref_matched_N -->
`-` → `n` — Reference length of the matched part

#### `ref_trail3p_N`
<!-- op ref_trail3p_N -->
`-` → `n` — Reference after the matched part

#### `ref_trail5p_V`
<!-- op ref_trail5p_V -->
`-` → `n` — Reference length of the 5' trail of the CIGAR

#### `ref_trail3p_V`
<!-- op ref_trail3p_V -->
`-` → `n` — Reference length of the 3' trail of the CIGAR

#### `qry_trail5p_N`
<!-- op qry_trail5p_N -->
`-` → `n` — Query length of the 5' trail (S I)

#### `qry_matched_N`
<!-- op qry_matched_N -->
`-` → `n` — Query length of the matched part

#### `qry_trail3p_N`
<!-- op qry_trail3p_N -->
`-` → `n` — Query length of the 3' trail

#### `aln_trail5p_cgr`
<!-- op aln_trail5p_cgr -->
`-` → `c` — The 5' trail of the CIGAR (leading S H D N I)

#### `aln_matched_cgr`
<!-- op aln_matched_cgr -->
`-` → `c` — The matched part of the CIGAR

#### `aln_trail3p_cgr`
<!-- op aln_trail3p_cgr -->
`-` → `c` — The 3' trail of the CIGAR

#### `aln_nmatch`
<!-- op aln_nmatch -->
`-` → `n` — M = X length of the matched part

#### `aln_nedit`
<!-- op aln_nedit -->
`-` → `n` — Edit distance from NM:i (0 without NM)

#### `aln_nmatchx`
<!-- op aln_nmatchx -->
`-` → `n` — NM minus the D and I of the matched part (mismatches)

#### `qry_seq`
<!-- op qry_seq -->
`-` → `seq` — Query sequence (SEQ, or filled for SEQ '*'; --sam-seqmiss)

#### `qry_qual`
<!-- op qry_qual -->
`-` → `qual` — Query quality (QUAL, or filled with the sequence)

#### `aln_ref`
<!-- op aln_ref -->
`-` → `aln` — Alignment: reference line (needs --sam/FILE)

#### `aln_aln`
<!-- op aln_aln -->
`-` → `aln` — Alignment: match line (| x ~ - = _ `)

#### `aln_qry`
<!-- op aln_qry -->
`-` → `aln` — Alignment: query line

#### `aln_rlr`
<!-- op aln_rlr -->
`-` → `aln` — Alignment: ruler with reference positions

#### `aln_qlt`
<!-- op aln_qlt -->
`-` → `aln` — Alignment: query quality line

#### `aln_all`
<!-- op aln_all -->
`-` → `aln` — Alignment: ruler, reference, match, query lines

#### `aln_allq`
<!-- op aln_allq -->
`-` → `aln` — Alignment: as aln_all, with the quality line

#### `aln_all_pack`
<!-- op aln_all_pack -->
`-` → `aln` — Alignment: as aln_all, lines joined by byte 0x01

#### `aln_nedit_pia`
<!-- op aln_nedit_pia -->
`-` → `n` — Edits in aln_aln, intron aware (x - ~)

#### `aln_nedit_piu`
<!-- op aln_nedit_piu -->
`-` → `n` — Edits in aln_aln, intron unaware (x - ~ =)

#### `aln_posinfo`
<!-- op aln_posinfo -->
`n` → `info` — Mismatch and indel positions; indel sequences up to length n (0: all)

#### `patchiness`
<!-- op patchiness -->
`s d` → `score` — Largest penalty sum of runs in alignment string s within window d

#### `patchiness2`
<!-- op patchiness2 -->
`s d` → `score` — As patchiness; the window counts aligned columns only

#### `sam_rbt`
<!-- op sam_rbt -->
`x y` → `-` — Set the rbt region to reference positions x..y (for the rbt_ operators)

#### `rbt_refx`
<!-- op rbt_refx -->
`-` → `p` — rbt: first reference position (region clipped to the matched part)

#### `rbt_refy`
<!-- op rbt_refy -->
`-` → `p` — rbt: last reference position

#### `rbt_dx`
<!-- op rbt_dx -->
`-` → `n` — rbt: positions cut off at the start (rbt_refx - x)

#### `rbt_dy`
<!-- op rbt_dy -->
`-` → `n` — rbt: positions cut off at the end (y - rbt_refy)

#### `rbt_qryx`
<!-- op rbt_qryx -->
`-` → `p` — rbt: first query position

#### `rbt_qryy`
<!-- op rbt_qryy -->
`-` → `p` — rbt: last query position

#### `rbt_qrylen`
<!-- op rbt_qrylen -->
`-` → `n` — rbt: query length

#### `rbt_qry`
<!-- op rbt_qry -->
`-` → `seq` — rbt: query sequence

#### `rbt_nmatch`
<!-- op rbt_nmatch -->
`-` → `n` — rbt: aligned bases (| x)

#### `rbt_nmatchx`
<!-- op rbt_nmatchx -->
`-` → `n` — rbt: mismatched aligned bases (x)

#### `rbt_ndel`
<!-- op rbt_ndel -->
`-` → `n` — rbt: deleted bases (-)

#### `rbt_nins`
<!-- op rbt_nins -->
`-` → `n` — rbt: inserted bases (~)

#### `rbt_nedit`
<!-- op rbt_nedit -->
`-` → `n` — rbt: edits (- ~ x)

#### `rbt_alnref`
<!-- op rbt_alnref -->
`-` → `aln` — rbt: the region's columns of aln_ref

#### `rbt_alnaln`
<!-- op rbt_alnaln -->
`-` → `aln` — rbt: the region's columns of aln_aln

#### `rbt_alnqry`
<!-- op rbt_alnqry -->
`-` → `aln` — rbt: the region's columns of aln_qry

#### `rbt_alnrlr`
<!-- op rbt_alnrlr -->
`-` → `aln` — rbt: the region's columns of aln_rlr

#### `rbt_alnqlt`
<!-- op rbt_alnqlt -->
`-` → `aln` — rbt: the region's columns of aln_qlt

#### `rbt_alnall`
<!-- op rbt_alnall -->
`-` → `aln` — rbt: ruler, reference, match, query, quality lines of the region

#### `ref_trail5p`
<!-- op ref_trail5p -->
`-` → `seq` — Reference before the matched part (needs --sam/FILE)

#### `ref_matched`
<!-- op ref_matched -->
`-` → `seq` — Reference of the matched part (needs --sam/FILE)

#### `ref_trail3p`
<!-- op ref_trail3p -->
`-` → `seq` — Reference after the matched part (needs --sam/FILE)

#### `qry_trail5p`
<!-- op qry_trail5p -->
`-` → `seq` — Query sequence of the 5' trail

#### `qry_matched`
<!-- op qry_matched -->
`-` → `seq` — Query sequence of the matched part

#### `qry_trail3p`
<!-- op qry_trail3p -->
`-` → `seq` — Query sequence of the 3' trail

#### `sam_put`
<!-- op sam_put -->
`tag val` → `-` — Put tag:val in the optional fields (column 12; replaces the same tag)

#### `sam_get`
<!-- op sam_get -->
`tag def` → `value` — Value of optional field tag (XX:T, XX.T, XX-T), def if absent

### bio

#### `rc`
<!-- op rc -->
`x` → `rc(x)` — Reverse complement (IUPAC codes complemented, U as A)

#### `forward`
<!-- op forward -->
`B seq` → `forward` — seq if B is true, else rc(seq)

#### `todna`
<!-- op todna -->
`x` → `x'` — DNA letters: uU to tT, other letters n N, other bytes N

#### `dnatl`
<!-- op dnatl -->
`x f` → `protein` — DNA (or RNA) x translated from offset f (stop *, unknown ?, rest {..})

#### `dnagc`
<!-- op dnagc -->
`x N` → `gc(x)` — Percentage G C among the A C G T U of x, N decimals (none: NA)

#### `fasta`
<!-- op fasta -->
`i s` → `fasta` — FASTA record: >i newline s

#### `fastq`
<!-- op fastq -->
`i s` → `fastq` — FASTQ record of i and s, quality Z for every base

#### `fastQ`
<!-- op fastQ -->
`i s q` → `fastq` — FASTQ record of i, s and quality q (q * gives Z; other length: NA)

#### `strpair`
<!-- op strpair -->
`x y` → `pair(x,y)` — Per position | where x and y are equal, x where not (shorter length)

### filter

#### `/lt/`
<!-- op /lt/ -->
`x y` → `x < y` — Numeric less than

#### `/le/`
<!-- op /le/ -->
`x y` → `x <= y` — Numeric less than or equal

#### `/eq/`
<!-- op /eq/ -->
`x y` → `x == y` — Numeric equal

#### `/ge/`
<!-- op /ge/ -->
`x y` → `x >= y` — Numeric greater than or equal

#### `/gt/`
<!-- op /gt/ -->
`x y` → `x > y` — Numeric greater than

#### `/ne/`
<!-- op /ne/ -->
`x y` → `x != y` — Numeric not equal

#### `~lt~`
<!-- op ~lt~ -->
`x y` → `x lt y` — String (byte) less than

#### `~le~`
<!-- op ~le~ -->
`x y` → `x le y` — String (byte) less than or equal

#### `~eq~`
<!-- op ~eq~ -->
`x y` → `x eq y` — String equal

#### `~ge~`
<!-- op ~ge~ -->
`x y` → `x ge y` — String (byte) greater than or equal

#### `~gt~`
<!-- op ~gt~ -->
`x y` → `x gt y` — String (byte) greater than

#### `~ne~`
<!-- op ~ne~ -->
`x y` → `x ne y` — String not equal

#### `=`
<!-- op = -->
`x y` → `x eq y` — String equal

#### `/=`
<!-- op /= -->
`x y` → `x ne y` — String not equal

#### `~`
<!-- op ~ -->
`x p` → `x =~ p` — Regex p matches x (:NAME: a fixed string)

#### `/~`
<!-- op /~ -->
`x p` → `x !~ p` — Regex p does not match x

#### `~isin~`
<!-- op ~isin~ -->
`x D` → `x in D` — x is a key of dictionary D

#### `~isnotin~`
<!-- op ~isnotin~ -->
`x D` → `x not in D` — x is not a key of dictionary D

#### `~hasval~`
<!-- op ~hasval~ -->
`x D~v` → `D{x} eq v` — x is a key of D with value v (not in test)

#### `~hasnotval~`
<!-- op ~hasnotval~ -->
`x D~v` → `D{x} ne v` — x is not a key of D with value v (not in test)

#### `~notin~`
<!-- op ~notin~ -->
`x D` → `x not in D` — Synonym of ~isnotin~

#### `~notval~`
<!-- op ~notval~ -->
`x D~v` → `D{x} ne v` — Synonym of ~hasnotval~ (not in test)

#### `/ep/`
<!-- op /ep/ -->
`x y/e` → `|x-y| <= e` — x within e of y (default e 0.0001)

#### `/epx/`
<!-- op /epx/ -->
`x y/e` → `|x-y| > e` — x not within e of y

#### `/om/`
<!-- op /om/ -->
`x y/k` → `x ~ y` — |x| within a factor k of |y| (default k 2)

#### `/omx/`
<!-- op /omx/ -->
`x y/k` → `x !~ y` — |x| not within a factor k of |y|

#### `/all/`
<!-- op /all/ -->
`x m` → `all m in |x|` — All bits of mask m set in |x|

#### `/any/`
<!-- op /any/ -->
`x m` → `any m in |x|` — At least one bit of mask m set in |x|

#### `/notall/`
<!-- op /notall/ -->
`x m` → `!all m in |x|` — Not all bits of mask m set in |x|

#### `/none/`
<!-- op /none/ -->
`x m` → `!any m in |x|` — No bit of mask m set in |x|

#### `/ISINT`
<!-- op /ISINT -->
`x` → `B` — x is an integer (no VALUE; not in test)

#### `/NOTINT`
<!-- op /NOTINT -->
`x` → `B` — x is not an integer (not in test)

#### `/ISNUM`
<!-- op /ISNUM -->
`x` → `B` — x is a number, not nan (not in test)

#### `/NOTNUM`
<!-- op /NOTNUM -->
`x` → `B` — x is not a number (not in test)
