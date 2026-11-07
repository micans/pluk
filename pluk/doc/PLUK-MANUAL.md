# The pluk manual

*pluk is a Rust reimplementation of pick, a Perl program by Stijn van Dongen that
served as its prototype. All of pluk's code was written by Claude (Anthropic), in a
continuous dialogue with Stijn. The work was staged into steps, each a
small set of items; every step was discussed and its decisions written down before any
code was written, then implemented item by item, tested against pick and committed
by Stijn over a period of about three days. The `plan/` directory retains that history.
Many corner/edge cases in pick were settled and sometimes fixed.*


*This manual is being written. Sections 4 to 14 are outlines; section 15 lists every operator, with descriptions to follow.*


## 1. Introduction

pluk works on tables: files with a header line that names the columns, and rows of
tab-separated fields. It selects columns by name, keeps or drops rows by their values,
computes new columns with a small stack language, sends rows to different files, and
checks assertions about the data. It also reads SAM and BAM alignment files, with
operators that take alignments apart, count their edits and draw them. pluk reads its
input once, line by line, and keeps little more than the current row, so the size of the
input does not matter. [INTRODUCTION.md](INTRODUCTION.md) gives a short tour; this manual
describes everything.

### Building pluk

pluk is written in Rust and built with cargo:

```sh
cargo build --release     # the program: target/release/pluk
make test                 # build, unit tests, the test suite, the manual's examples
make test-int128          # the same with 128-bit integers; binary target/release/pluk7
```

By default pluk computes with 64-bit signed integers; the int128 build uses 128-bit
integers, so that integer arithmetic stays exact over a larger range before it moves to
floating point (section 3, Numbers). Everything else is the same.

### Dependencies

pluk uses eight crates (Rust libraries) directly:

| Crate | Used for |
|-------|----------|
| flate2 | reading gzip-compressed input, writing gzip-compressed output files |
| regex | regular expressions: column selection, filters, operators, `-/PAT` |
| memchr | fast searching of bytes |
| noodles-bam, noodles-bgzf, noodles-sam | reading BAM files |
| md-5 | the `md5` operator |
| jiff | dates and time zones (`todate`) |

pluk is relatively lean: these eight crates bring in about 56 further crates when it is
built, about 34 of them for BAM support, and pluk itself is about 12,000 lines of Rust,
including some 100 unit tests (the numbers change with versions). It parses its command
line itself rather than with an argument-parsing library, formats and parses numbers
itself, has a small random-number generator of its own (splitmix64, for `rand`), and needs
neither a serialisation framework nor an asynchronous runtime.


## 2. Quick start

The examples use [harvest.txt](harvest.txt), a day in an orchard (the table is shown in
[INTRODUCTION.md](INTRODUCTION.md)). Each line is one common task.

Columns by name; without the header line; by pattern; all but some:

```console
$ pluk picker kg < harvest.txt
picker  kg
Annie   14.5
...
$ pluk -h picker kg < harvest.txt
Annie   14.5
...
$ pluk '+p.*' kg < harvest.txt
picker  kg
Annie   14.5
...
$ pluk -x tree ripeness < harvest.txt
picker  fruit   kg    bruised
Annie   apple   14.5  2
...
```

Rows by value; counted; by pattern:

```console
$ pluk -A @fruit=pear < harvest.txt
picker  fruit  tree  kg    bruised  ripeness
Bert    pear   P03   9.0   0        0.75
Gus     pear   P08   12.7  3        0.80
$ pluk -c @ripeness/ge/0.9 < harvest.txt
3
$ pluk -A '@picker~^[A-C]' < harvest.txt
picker  fruit  tree  kg    bruised  ripeness
Annie   apple  A12   14.5  2        0.92
Bert    pear   P03   9.0   0        0.75
Chen    apple  A07   22.1  5        0.88
```

A new column, and a test that gives 1 or 0:

```console
$ pluk picker 'lb::kg^2.20462,mul^1,dd' 'heavy::kg^10^/gt/,test' < harvest.txt
picker  lb    heavy
Annie   32.0  1
Bert    19.8  0
...
```

Compressed input is recognised; SAM input with `--sam` (columns are then numbered):

```console
$ gzip -c harvest.txt | pluk picker kg
picker  kg
Annie   14.5
...
$ pluk -q --sam 1 4 6 < reads.sam
read1   9   3S22M3I8M3D9M
```


## 3. Concepts

### Rows and the header line

The first line of the input is the header: it names the columns. Every other line is a
row, with as many fields as the header has names. The arguments name columns (to output
them), computes (new columns, they contain `::` or `:=`) and filters (they start with
`@`); options come first, before all of these: an option after a column argument is taken
as a column name.

- `-k` the input has no header line; the columns are named 1, 2, ... .
- `-K` as `-k`, and the output gets a header line with those names.
- `-h` no header line in the output.
- `-R` the header line has one name fewer than the rows (a first column of row names
  without a name, as R writes tables): that column is named `_`.
- `-u` make repeated names in the header unique (`kg`, `kg_v02`, ...); without it, a
  repeated name is an error.

```console
$ tail -n +2 harvest.txt | pluk -K 1 4
1       4
Annie   14.5
...
```

Rows with varying numbers of fields (ragged input, such as SAM) are described in
section 4.

### Writing names and values

The arguments use a few characters for their own syntax: `:` `^` `,` `/` `~` `=` `@`
`+` `%` and white space. Where such a character is part of a name or a value, write it
%-encoded: `%3A` for `:`, `%2C` for `,`, `%2F` for `/`, `%20` for a space, and so on.
`-z` prints the encoding of its arguments and `-zz` decodes:

```console
$ pluk -z 'a:b,c^d' 'x y'
a%3Ab%2Cc%5Ed x%20y
$ pluk picker 'w::^kg%3A:kg,cat' < harvest.txt
picker  w
Annie   kg:14.5
...
```

In regular expressions `\Q...\E` quotes the text between (no character in it is special):

```console
$ pluk -A '@picker~\QA.n\E' < harvest.txt
picker  fruit  tree  kg  bruised  ripeness
```

### Bytes

pluk assumes no character encoding: fields are bytes, lengths count bytes, comparisons
compare bytes, and anything is passed through unchanged.

### Numbers

A field is a number if it reads as one: integers (`7`, `-3`), decimals and exponents
(`0.1`, `1e300`), and `nan` and `inf`. An empty field or a word is not a number.
Integers are exact (64-bit signed, 128-bit in the int128 build); when an integer result
does not fit, the result is a floating-point number. Floating-point numbers are written
so that they read back as the same number, which shows their nature:

```console
$ pluk id 'sum::x:y,add' 'prod::x:y,mul' < numbers.txt
id  sum                    prod
A   9                      14
B   1                      -12
C   9.223372036854776e+18  9223372036854775807
D   0.30000000000000004    0.020000000000000004
E   nan                    nan
F   NA                     NA
G   NA                     NA
H   1e+300                 inf
```

([numbers.txt](numbers.txt) has the columns id, x and y.) `--num-perl` writes numbers
the way Perl prints them (15 significant digits):

```console
$ pluk --num-perl id 'sum::x:y,add' < numbers.txt
id  sum
A   9
B   1
C   9223372036854775808
D   0.3
...
```

Division by zero gives `inf` or `nan` (`div`) or NA (`idiv`, `mod`); the square root
or logarithm of a negative number gives `nan`.

### NA

NA marks a value that cannot be computed: an arithmetic operator given a field that is
not a number (rows F and G above), a SAM value of an unmapped read. It is written
`NA`. An operator with an NA
operand gives NA; a numeric filter fails on NA, so the row is not kept, and the number
of such values is reported. A later compute in the same run that refers to a column
holding NA reads the text `NA` (a string of two letters, not a number); so does a later
run that reads pluk's output. For SAM, [SAM-NA.md](SAM-NA.md) follows NA
through the most likely cases.

### Messages, reports and exit status

Errors are written to stderr, as `pluk: ...`, and end the run with exit status 1: an
option or argument that is not valid, a name that is not a column, a failed assertion,
a dictionary lookup with `__EXIT__`, an `-E<N>` count that does not match. Otherwise
the exit status is 0. At the end of a run pluk reports counts on stderr where something
deserves attention (values that were not numbers, NA in SAM values, rows skipped,
groups, ...); `-q` silences everything except errors. `-E<N>` makes the run an error
unless exactly N rows are output:

```console
$ pluk -E2 @fruit=apple picker < harvest.txt
picker
Annie
Chen
Fenna
pluk: -E2: expected 2 output rows, found 3
```


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

Pushes a second copy of the top value. `pop` removes the top value and `xch` exchanges the top two; with them a value can be reused or put in place without computing it again.

```console
$ pluk picker 'd::fruit,dup,cat' 'p::^a^b,pop' 'x::^a^b,xch' < harvest.txt
picker	d	p	x
Annie	appleapple	a	ba
Bert	pearpear	a	ba
...
```

#### `pop`
<!-- op pop -->
`x` → `-` — Remove top entry x from stack

Removes the top value of the stack. Example under `dup`.

#### `xch`
<!-- op xch -->
`x y` → `y x` — Exchange x and y

Exchanges the top two values of the stack. Example under `dup`.

### string

#### `cat`
<!-- op cat -->
`x y` → `xy` — Concatenation of x and y

Concatenates x and y. A number is written as text first, as it would be in the output (with `--num-perl` as Perl writes it). `len` gives the length in bytes, so the length of a computed number depends on how it is written: 0.1 + 0.2 is written 0.30000000000000004.

```console
$ pluk picker 'c::picker:fruit,cat' 'n::picker,len' 'l::^0.1^0.2,add,len' < harvest.txt
picker	c	n	l
Annie	Annieapple	5	19
Bert	Bertpear	4	19
...
```

#### `len`
<!-- op len -->
`x` → `len(x)` — Length of string x (bytes)

The length of x in bytes (a multi-byte character counts all its bytes). Example under `cat`.

#### `lc`
<!-- op lc -->
`x` → `lc(x)` — Lower case of x (ASCII)

Changes the ASCII letters of x to lower case (`lc`) or upper case (`uc`); other bytes are unchanged. `rev` reverses x byte by byte; `rot13` rotates the ASCII letters by 13 places (applied twice it gives x back).

```console
$ pluk picker 'l::picker,lc' 'u::picker,uc' 'r::picker,rev' 't::picker,rot13' < harvest.txt
picker	l	u	r	t
Annie	annie	ANNIE	einnA	Naavr
Bert	bert	BERT	treB	Oreg
...
```

#### `uc`
<!-- op uc -->
`x` → `uc(x)` — Upper case of x (ASCII)

Upper case of the ASCII letters of x. Example under `lc`.

#### `rev`
<!-- op rev -->
`x` → `rev(x)` — String reverse of x (bytes)

Reverses x byte by byte (a multi-byte character is reversed too, and no longer valid). Example under `lc`.

#### `rot13`
<!-- op rot13 -->
`x` → `rot13(x)` — Rot13 encoding of x

Rotates the ASCII letters of x by 13 places. Example under `lc`.

#### `substr`
<!-- op substr -->
`x i k` → `x[i:i+k-1]` — Substring of x starting at i (zero-based) of length k (as Perl)

The part of x that starts at offset i (counting from 0) and is k bytes long, as Perl's substr: a negative i counts from the end of x, a negative k leaves that many bytes off the end. A part that lies outside x is empty. i and k must be integers, otherwise the result is NA.

```console
$ pluk tree 'a::tree^1^2,substr' 'b::tree^-2^9,substr' 'c::tree^0^-1,substr' 'd::tree^7^1,substr' < harvest.txt
tree	a	b	c	d
A12	12	12	A1	
P03	03	03	P0	
...
```

#### `head`
<!-- op head -->
`x k` → `x[0:k-1]` — Initial part of string x, of length k [head,tail,skip,shed]

`head` keeps the first k bytes of x, `tail` the last k; `skip` drops the first k, `shed` the last k. k is clamped to the length of x (a k beyond it takes or drops everything, a negative k counts as 0); a k that is not an integer gives NA.

```console
$ pluk picker 'h::picker^2,head' 't::picker^2,tail' 's::picker^2,skip' 'd::picker^2,shed' 'a::picker^99,head' < harvest.txt
picker	h	t	s	d	a
Annie	An	ie	nie	Ann	Annie
Bert	Be	rt	rt	Be	Bert
...
```

#### `tail`
<!-- op tail -->
`x k` → `x[#x-k:#x-1]` — Trailing part of string x, of length k [head,tail,skip,shed]

The last k bytes of x. Example under `head`.

#### `skip`
<!-- op skip -->
`x k` → `x[k:#x-1]` — Trailing part of string x, skipping first k [head,tail,skip,shed]

x without its first k bytes. Example under `head`.

#### `shed`
<!-- op shed -->
`x k` → `x[0:#x-k-1]` — Leading part of string x, shedding last k [head,tail,skip,shed]

x without its last k bytes. Example under `head`.

#### `strim`
<!-- op strim -->
`s nx ny` → `s'` — Trim nx bytes from the start and ny from the end of s

Removes nx bytes from the start and ny bytes from the end of s. A negative count is 0; a count that is not an integer gives NA.

```console
$ pluk tree 's::tree^1^1,strim' 'p::picker^0^2,strim' < harvest.txt
tree	s	p
A12	1	Ann
P03	0	Be
...
```

#### `ppl`
<!-- op ppl -->
`x N` → `x'` — x right-padded with blanks to width N (left-aligned)

Pads x with spaces to a width of N bytes: `ppl` adds them on the right (left-aligned), `ppr` on the left (right-aligned); `zp` pads on the left with zeroes. An x that is already N bytes or longer is unchanged. A negative width, or one that is not an integer, gives NA. `zp` pads the text as it is, so -2 to width 4 is 00-2. (Below the padded values are put between brackets, to show the spaces.)

```console
$ pluk picker 'l::^[:fruit^7,ppl,cat^],cat' 'r::^[:fruit^7,ppr,cat^],cat' 'z::bruised^3,zp' 'n::^-2^4,zp' < harvest.txt
picker	l	r	z	n
Annie	[apple  ]	[  apple]	002	00-2
Bert	[pear   ]	[   pear]	000	00-2
...
```

#### `ppr`
<!-- op ppr -->
`x N` → `x'` — x left-padded with blanks to width N (right-aligned)

x left-padded with spaces to width N (right-aligned). Example under `ppl`.

#### `zp`
<!-- op zp -->
`x N` → `x'` — x left zero-padded to width N

x left-padded with zeroes to width N. Example under `ppl`.

#### `nchar`
<!-- op nchar -->
`x c` → `nc` — Number of occurrences of c in x (non-overlapping)

The number of non-overlapping occurrences of the string c in x; c may be longer than one byte. An empty c counts 0.

```console
$ pluk picker 'n::picker^n,nchar' 'e::fruit^pp,nchar' < harvest.txt
picker	n	e
Annie	2	1
Bert	0	0
...
```

#### `catall`
<!-- op catall -->
`*` → `Stack-joined` — Stringified stack

Concatenates all values on the stack, from the bottom up; `joinall` joins them with the top value as separator; `join` joins just x and y with j. An empty stack gives an empty string; joining an NA gives NA.

```console
$ pluk 'a::picker:fruit:tree,catall' 'j::picker:fruit:tree^-,joinall' 'k::picker:fruit^%20&%20,join' < harvest.txt
a	j	k
AnnieappleA12	Annie-apple-A12	Annie & apple
BertpearP03	Bert-pear-P03	Bert & pear
...
```

#### `joinall`
<!-- op joinall -->
`* s` → `Stack-joined-by-s` — Stringified stack with s as separator

All values on the stack below the top, joined with the top value as separator. Example under `catall`.

#### `join`
<!-- op join -->
`x y j` → `xjy` — Join two elements with string separator

x and y joined with separator j. Example under `catall`.

#### `uie`
<!-- op uie -->
`x y` → `x-or-y` — Use x if not empty (and not NA), otherwise use y

x, unless x is empty or NA; then y. Useful for a default value.

```console
$ pluk id x 'v::x^none,uie' '@id~[EFG]' < numbers.txt
id	x	v
E	nan	nan
F		none
G	fox	fox
```

#### `urlec`
<!-- op urlec -->
`x` → `urlec(x)` — URL encoding of x (all but A-Z a-z 0-9 _ as %XX)

`urlec` encodes every byte of x except A-Z a-z 0-9 and _ as %XX (upper-case hexadecimal); `urldc` decodes %XX (either case) and leaves everything else, including +, as it is.

```console
$ echo 'Annie & Bert' | pluk -k 1 'e::1,urlec' 'd::1,urlec,urldc'
Annie & Bert	Annie%20%26%20Bert	Annie & Bert
```

#### `urldc`
<!-- op urldc -->
`x` → `urldc(x)` — URL decoding of x (%XX)

Decodes %XX in x. Example under `urlec`.

#### `cgsum`
<!-- op cgsum -->
`c s` → `Sum of s in c` — Sum of the lengths of the s operations in CIGAR c

Take a CIGAR string c (a value, not necessarily from SAM input) and a set of operation letters s: `cgsum` adds up the lengths of the operations in s, `cgmax` gives the largest length (0 if there is none), `cgcount` the number of such operations. A CIGAR `*` gives 0; a CIGAR that does not parse (also the empty string and lower-case operations) gives NA. Letters in s that are not CIGAR operations count nothing.

```console
$ pluk -q --sam 1 6 'm::6^M,cgsum' 'i::6^ID,cgsum' 'x::6^MID,cgmax' 'n::6^ID,cgcount' < reads.sam
read1	3S22M3I8M3D9M	39	6	22	2
```

#### `cgmax`
<!-- op cgmax -->
`c s` → `Max of s in c` — Largest length of an s operation in CIGAR c

The largest length of an operation of s in the CIGAR c. Example under `cgsum`.

#### `cgcount`
<!-- op cgcount -->
`c s` → `Count of s in c` — Number of s operations in CIGAR c

The number of operations of s in the CIGAR c. Example under `cgsum`.

#### `md5`
<!-- op md5 -->
`x` → `md5(x)` — MD5 sum of x (hexadecimal)

The MD5 digest of the bytes of x, as 32 lower-case hexadecimal digits.

```console
$ pluk picker 'm::picker,md5' < harvest.txt
picker	m
Annie	0a420beb77b963b9a14066f08b7865e8
Bert	bddfdb28870c5800d938fa0993c46d65
...
```

#### `fmtall`
<!-- op fmtall -->
`f *` → `Map-*-into-f` — In f replace # instances by stack (\# is a literal #)

Fills in a format: f is the bottom value of the stack, and each # in it is replaced by the next value, in order (\# is a literal #). The number of # must equal the number of values: for a constant f a mismatch is an error before input is read, for an f from a column it gives NA for that row.

```console
$ pluk 'f::^#%20picked%20#%20kg:picker:kg,fmtall' < harvest.txt
f
Annie picked 14.5 kg
Bert picked 9.0 kg
...
```

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

`del` deletes the first match of the pattern p in x, `delg` every match. As for all regex operators, p must be a constant (`^PATTERN`), written in the syntax of Rust's regex crate (no backreferences or look-around), matched on bytes; `\Q...\E` quotes literal text; an empty pattern is an error.

```console
$ pluk picker 'd::picker^[aeiou],del' 'g::picker^[aeiou],delg' < harvest.txt
picker	d	g
Annie	Anne	Ann
Bert	Brt	Brt
...
```

#### `delg`
<!-- op delg -->
`x p` → `x =~ s/p//g` — Globally delete pattern p in x

Deletes every match of p in x. Example under `del`.

#### `ed`
<!-- op ed -->
`x p s` → `x =~ s/p/s/` — Substitute pattern p by s in x (s literal)

`ed` replaces the first match of p in x by s, `edg` every match. s is literal text: `$` has no special meaning here (see `edbr` for groups).

```console
$ pluk picker 'e::picker^n^N,ed' 'g::picker^n^N,edg' < harvest.txt
picker	e	g
Annie	ANnie	ANNie
Bert	Bert	Bert
...
```

#### `edg`
<!-- op edg -->
`x p s` → `x =~ s/p/s/g` — Globally substitute pattern p by s in x (s literal)

Replaces every match of p in x by the literal s. Example under `ed`.

#### `get`
<!-- op get -->
`x p` → `p-match-of-x` — Group 1 of the match of p in x, or the entire match; empty if no match

`get` gives group 1 of the first match of p in x if p has a group, otherwise the whole match; empty if p does not match. `npat` counts the non-overlapping matches of p in x.

```console
$ pluk tree 'n::tree^\d+,get' 'l::tree^([A-Z])\d,get' 'v::picker^[aeiou],npat' < harvest.txt
tree	n	l	v
A12	12	A	2
P03	03	P	1
...
```

#### `npat`
<!-- op npat -->
`x p` → `np` — Number of (non-overlapping) matches of p in x

The number of non-overlapping matches of p in x. Example under `get`.

#### `edbr`
<!-- op edbr -->
`x p s` → `x =~ s/p/s/` — As ed, $1 $2 .. (groups) and $0 (match) in s

As `ed` and `edg`, but in s `$1`, `$2`, ... stand for the groups of the match and `$0` for the whole match; a `$` that is not followed by a digit is a literal `$`.

```console
$ pluk tree 'b::tree^([A-Z])(\d+)^$2-$1,edbr' 'g::picker^([aeiou])^<$1>,edgbr' < harvest.txt
tree	b	g
A12	12-A	Ann<i><e>
P03	03-P	B<e>rt
...
```

#### `edgbr`
<!-- op edgbr -->
`x p s` → `x =~ s/p/s/g` — As edg, $1 $2 .. (groups) and $0 (match) in s

As `edg`, with `$1` `$2` .. and `$0` in s. Example under `edbr`.

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
