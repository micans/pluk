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

The four arithmetic operations and integer division. Integers stay integers as long as the result fits; otherwise the result is a floating-point number. `div` always gives a float (written 3.5, or 2 for 6/3); division by zero gives inf, -inf or nan. `idiv` and `mod` are floored, so that x = y·idiv(x,y) + mod(x,y) and `mod` has the sign of y; they need integers, and give NA for a float or a divisor of 0. A value that is not a number gives NA.

```console
$ pluk id x y 'a::x:y,add' 's::x:y,sub' 'm::x:y,mul' 'd::x:y,div' 'i::x:y,idiv' 'r::x:y,mod' '@id~[ABD]' < numbers.txt
id	x	y	a	s	m	d	i	r
A	7	2	9	5	14	3.5	3	1
B	-3	4	1	-7	-12	-0.75	-1	1
D	0.1	0.2	0.30000000000000004	-0.1	0.020000000000000004	0.5	NA	NA
$ pluk 'd::^7^0,div' 'n::^-7^0,div' 'z::^0^0,div' 'i::^7^0,idiv' '@picker=Annie' < harvest.txt
d	n	z	i
inf	-inf	nan	NA
```

#### `sub`
<!-- op sub -->
`x y` → `x-y` — Subtract y from x

x minus y. Example under `add`.

#### `mul`
<!-- op mul -->
`x y` → `x*y` — Multiply x and y

x times y. Example under `add`.

#### `div`
<!-- op div -->
`x y` → `x/y` — Division (a float; by zero: inf or nan)

x divided by y, always a float; by zero inf, -inf or nan. Examples under `add`.

#### `idiv`
<!-- op idiv -->
`x y` → `x // y` — Integer division, floored (by zero: NA)

x divided by y, rounded down (floored); integers only, NA by zero. Examples under `add`.

#### `mod`
<!-- op mod -->
`x y` → `x mod y` — x modulo y, sign of y (by zero: NA)

x modulo y, with the sign of y; integers only, NA by zero. Example under `add`.

#### `pow`
<!-- op pow -->
`x y` → `x**y` — x raised to power y

`pow` raises x to the power y and always gives a float. `ipow` works in integers: it gives an integer, NA when the result does not fit or when x or y is a float, and a negative y behaves as with `pow`.

```console
$ pluk 'p::^2^10,pow' 'q::^2^10,ipow' 'r::^2^-1,ipow' 's::^2^70,ipow' 't::^2.5^2,ipow' '@picker=Annie' < harvest.txt
p	q	r	s	t
1024	1024	0.5	NA	NA
```

#### `ipow`
<!-- op ipow -->
`x y` → `x**y` — x raised to power y in integers (negative y as pow; overflow: NA)

x to the power y in integers (overflow or a float: NA). Example under `pow`.

#### `max`
<!-- op max -->
`x y` → `max(x,y)` — Maximum of x and y

The larger (`max`) or smaller (`min`) of x and y, as it is: an integer stays an integer.

```console
$ pluk id x y 'a::x:y,max' 'b::x:y,min' '@id~[ABG]' < numbers.txt
id	x	y	a	b
A	7	2	7	2
B	-3	4	4	-3
G	fox	5	NA	NA
```

#### `min`
<!-- op min -->
`x y` → `min(x,y)` — Minimum of x and y

The smaller of x and y. Example under `max`.

#### `neg`
<!-- op neg -->
`x` → `-x` — The sign-reversed value of x

Operations on one number: `neg` changes the sign, `abs` takes the absolute value, `sign` gives -1, 0 or 1, `sq` squares, `incr` and `decr` add or subtract 1. Integers stay integers while the result fits.

```console
$ pluk id x 'n::x,neg' 'a::x,abs' 's::x,sign' 'q::x,sq' 'i::x,incr' 'd::x,decr' '@id~[ABD]' < numbers.txt
id	x	n	a	s	q	i	d
A	7	-7	7	1	49	8	6
B	-3	3	3	-1	9	-2	-4
D	0.1	-0.1	0.1	1	0.010000000000000002	1.1	-0.9
```

#### `abs`
<!-- op abs -->
`x` → `abs(x)` — Absolute value of x

The absolute value of x. Example under `neg`.

#### `sign`
<!-- op sign -->
`x` → `sign(x)` — The sign of x (-1, 0 or 1)

-1, 0 or 1, as x is negative, zero or positive (nan for nan). Example under `neg`.

#### `sq`
<!-- op sq -->
`x` → `x^2` — Square of x

x squared. Example under `neg`.

#### `incr`
<!-- op incr -->
`x` → `x++` — x incremented by one

x plus 1. Example under `neg`.

#### `decr`
<!-- op decr -->
`x` → `x--` — x decremented by one

x minus 1. Example under `neg`.

#### `rand`
<!-- op rand -->
`x` → `rand(x)` — Random real in range [0, x) (x 0: [0, 1); see --seed)

A random number in [0, x); for x = 0 in [0, 1). The generator is seeded from the clock, or with `--seed=N`, which makes the output reproducible.

```console
$ pluk --seed=42 picker 'r::^10,rand^2,dd' < harvest.txt
picker	r
Annie	7.42
Bert	1.60
...
```

#### `addall`
<!-- op addall -->
`*` → `sum(Stack)` — Sum of all entries in stack

Operators that take every value on the stack. `addall` adds them, `mulall` multiplies, `maxall` and `minall` give the largest and smallest (the value itself, as `max`), `meanall` the mean, `hmeanall` the harmonic and `gmeanall` the geometric mean of the absolute values (the latter computed from logarithms, so it cannot overflow). NA among the values gives NA. On an empty stack `addall` gives 0 and `mulall` 1; the others give NA.

```console
$ pluk picker 'a::kg:bruised:ripeness,addall' 'm::kg:bruised:ripeness,mulall' 'x::kg:bruised:ripeness,maxall' 'n::kg:bruised:ripeness,minall' < harvest.txt
picker	a	m	x	n
Annie	17.42	26.68	14.5	0.92
Bert	9.75	0	9.0	0
...
$ pluk picker 'm::kg:ripeness,meanall^2,dd' 'h::kg:ripeness,hmeanall^2,dd' 'g::kg:ripeness,gmeanall^2,dd' < harvest.txt
picker	m	h	g
Annie	7.71	1.73	3.65
Bert	4.88	1.38	2.60
...
```

#### `mulall`
<!-- op mulall -->
`*` → `product(Stack)` — Product of all entries in stack

The product of all values on the stack (1 for an empty stack). Example under `addall`.

#### `maxall`
<!-- op maxall -->
`*` → `max(Stack)` — Max over all entries in stack

The largest value on the stack (NA for an empty stack). Example under `addall`.

#### `minall`
<!-- op minall -->
`*` → `min(Stack)` — Min over all entries in stack

The smallest value on the stack (NA for an empty stack). Example under `addall`.

#### `meanall`
<!-- op meanall -->
`*` → `mean(Stack)` — Mean of all entries in stack

The mean of the values on the stack (NA for an empty stack). Example under `addall`.

#### `hmeanall`
<!-- op hmeanall -->
`*` → `hmean(Stack)` — Harmonic mean of the absolute values in stack

The harmonic mean of the absolute values on the stack. Example under `addall`.

#### `gmeanall`
<!-- op gmeanall -->
`*` → `gmean(Stack)` — Geometric mean of the absolute values in stack

The geometric mean of the absolute values on the stack. Example under `addall`.

### math

#### `int`
<!-- op int -->
`x` → `int(x)` — x truncated towards zero (do not use for rounding)

Conversions to a whole number: `int` cuts off the fraction (towards zero), `floor` rounds down, `ceil` rounds up, `round` rounds to the nearest integer with halves to the even neighbour (2.5 becomes 2, 3.5 becomes 4). An integer is returned unchanged; a float beyond the integer range stays the whole float it is.

```console
$ printf '2.5\n3.5\n-2.5\n-1.7\n' | pluk -k 1 'i::1,int' 'f::1,floor' 'c::1,ceil' 'r::1,round'
2.5	2	2	3	2
3.5	3	3	4	4
-2.5	-2	-3	-2	-2
-1.7	-1	-2	-1	-2
```

#### `floor`
<!-- op floor -->
`x` → `floor(x)` — The floor of x

x rounded down. Example under `int`.

#### `ceil`
<!-- op ceil -->
`x` → `ceil(x)` — The ceil of x

x rounded up. Example under `int`.

#### `round`
<!-- op round -->
`x` → `round(x)` — The integer nearest to x, halves to even (2.5 -> 2, 3.5 -> 4)

x rounded to the nearest integer, halves to even. Example under `int`.

#### `qnl`
<!-- op qnl -->
`x Q` → `q(x)` — Normalised quantile of x, lower tail: (#steps <= x, plus 1)/(n+1) (--quant-Q)

Where x falls among the quantile steps of `--quant-Q=FILE` (a file of numbers in non-decreasing order, one per line), normalised to (0, 1]: with n steps, `qnl` is (the number of steps <= x, plus 1)/(n+1) and `qnu` (the number of steps >= x, plus 1)/(n+1). Q must be a constant naming a `--quant`; x that is not a number gives NA.

```console
$ printf '5\n10\n15\n' > steps.txt
$ pluk --quant-Q=steps.txt picker kg 'l::kg^Q,qnl' 'u::kg^Q,qnu' < harvest.txt
picker	kg	l	u
Annie	14.5	0.75	0.5
Bert	9.0	0.5	0.75
...
```

#### `qnu`
<!-- op qnu -->
`x Q` → `q(x)` — Normalised quantile of x, upper tail: (#steps >= x, plus 1)/(n+1)

The normalised quantile of x, upper tail. Example under `qnl`.

#### `sqrt`
<!-- op sqrt -->
`x` → `sqrt(x)` — Square root of x

Square root, exponentials and logarithms. They give floats; outside their domain the result follows IEEE: the logarithm of 0 is -inf, of a negative number nan, as is the square root of a negative number. `log10` and `log2` are computed directly (not as log(x)/log(10)).

```console
$ printf '1\n100\n0\n-1\n' | pluk -k 1 's::1,sqrt' 'e::1,log10' 'l::1,log2' 'x::1,exp10'
1	1	0	0	10
100	10	2	6.643856189774724	1e+100
0	0	-inf	-inf	1
-1	nan	nan	nan	0.1
```

#### `exp`
<!-- op exp -->
`x` → `e**x` — Exponential function applied to x

e to the power x. Example under `sqrt`.

#### `exp2`
<!-- op exp2 -->
`x` → `2^x` — 2 to the power of x

2 to the power x. Example under `sqrt`.

#### `exp10`
<!-- op exp10 -->
`x` → `10^x` — 10 to the power of x

10 to the power x. Example under `sqrt`.

#### `log`
<!-- op log -->
`x` → `log(x)` — Natural logarithm of x

The natural logarithm of x. Example under `sqrt`.

#### `log2`
<!-- op log2 -->
`x` → `log2(x)` — Logarithm of x in base 2

The logarithm of x in base 2. Example under `sqrt`.

#### `log10`
<!-- op log10 -->
`x` → `log10(x)` — Logarithm of x in base 10

The logarithm of x in base 10. Example under `sqrt`.

#### `sin`
<!-- op sin -->
`x` → `sin(x)` — Sine of x

Sine, cosine and tangent of x, in radians.

```console
$ printf '0\n1\n' | pluk -k 1 's::1,sin^3,dd' 'c::1,cos^3,dd' 't::1,tan^3,dd'
0	0.000	1.000	0.000
1	0.841	0.540	1.557
```

#### `cos`
<!-- op cos -->
`x` → `cos(x)` — Cosine of x

The cosine of x (radians). Example under `sin`.

#### `tan`
<!-- op tan -->
`x` → `tan(x)` — Tangent of x

The tangent of x (radians). Example under `sin`.

### precision

#### `dd`
<!-- op dd -->
`x N` → `x'` — x written with N decimal digits

Numbers written with a fixed number of decimals N (an integer from 0 to 1000, otherwise NA): `dd` writes x, `sn` writes x in scientific notation; `frac`, `pct` and `pml` write x/y, 100·x/y and 1000·x/y. A division by zero gives inf or nan.

```console
$ pluk picker 'd::ripeness^1,dd' 's::kg^2,sn' 'f::bruised:kg^3,frac' 'p::bruised:kg^1,pct' 'm::bruised:kg^0,pml' < harvest.txt
picker	d	s	f	p	m
Annie	0.9	1.45e+01	0.138	13.8	138
Bert	0.8	9.00e+00	0.000	0.0	0
...
```

#### `sn`
<!-- op sn -->
`x N` → `x'` — x in scientific notation with N decimal digits

x in scientific notation with N decimals. Example under `dd`.

#### `frac`
<!-- op frac -->
`x y N` → `x/y` — Fraction x/y with N decimal digits

x/y with N decimals. Example under `dd`.

#### `pct`
<!-- op pct -->
`x y N` → `pct(x/y)` — Percentage of x relative to y with N decimal digits

x/y as a percentage, with N decimals. Example under `dd`.

#### `pml`
<!-- op pml -->
`x y N` → `pml(x/y)` — Per mille of x relative to y with N decimal digits

x/y per mille, with N decimals. Example under `dd`.

### dictionary

#### `map`
<!-- op map -->
`x D` → `map-of-x` — Value of x in dictionary D (not found: as D's default)

`map` gives the value of key x in dictionary D; for a key that is not in D it gives D's not-found result: the key itself, the default given with the dictionary (`--cd-NAME/DEFAULT=...`), or, with the default `__EXIT__`, an error that ends the run with the line number. `dord` gives the position (1-based) at which x first appeared in D, counted over everything merged into D in command-line order, and NA for a key that is not in D. D must be a constant naming a dictionary; an NA x gives NA.

```console
$ pluk --cd-PRICE/none=apple:2.10,pear:2.50,cherry:9.00 picker fruit 'p::fruit^PRICE,map' 'o::fruit^PRICE,dord' '@picker~[ABD]' < harvest.txt
picker	fruit	p	o
Annie	apple	2.10	1
Bert	pear	2.50	2
Dora	plum	none	NA
```

#### `dord`
<!-- op dord -->
`x D` → `ord-of-x` — Order of x in dictionary D (first appearance, 1-based; NA if absent)

The order of key x in dictionary D (NA if absent). Example under `map`.

### branch

#### `ifelse`
<!-- op ifelse -->
`B x y` → `x or y` — If B put x else put y (B: a non-zero number, or a non-empty string)

`ifelse` gives x if B is true and y otherwise: a number is true if it is not zero, anything else if it is not empty; an NA B gives NA. `test` applies a filter operator T (any of section 5: `/gt/`, `~eq~`, `=`, `~`, `~isin~`, `/ep/`, `/all/`, ...) to x and y and gives 1 or 0; T must be a constant, as must the pattern of `~` and the dictionary of `~isin~`; where a filter would fail on a value that is not a number, `test` gives NA. `~hasval~`, `~hasnotval~` and the predicates such as `/ISNUM` cannot be used in `test`.

```console
$ pluk picker kg 'h::kg^15^/gt/,test' 'w::kg^15^/gt/,test^heavy^light,ifelse' 'a::picker^%5E[A-D]^~,test' < harvest.txt
picker	kg	h	w	a
Annie	14.5	0	light	1
Bert	9.0	0	light	1
...
```

#### `test`
<!-- op test -->
`x y T` → `B` — Test T applied to x y: 1 or 0 (T a filter operator: /gt/ ~eq~ ~ ~isin~ ...)

Filter operator T applied to x and y: 1 or 0. Example under `ifelse`.

### state

#### `save`
<!-- op save -->
`x n` → `x` — Store x in register n (per row), x stays

`save` stores x in register n and leaves x on the stack; `fetch` gives the value stored in register n. Registers hold for one row and are shared by all computes of that row: a later compute can fetch what an earlier one saved, and every row starts without them. A constant register name is checked before input is read: a `fetch` that no earlier `save` (in compute order) stores is an error. A name taken from a column is looked up per row; a register that was not stored gives NA.

```console
$ pluk picker 'r::bruised:kg,div^r,save^r,fetch,mul^4,dd' 's::^r,fetch' < harvest.txt
picker	r	s
Annie	0.0190	0.13793103448275862
Bert	0.0000	0
...
```

#### `fetch`
<!-- op fetch -->
`n` → `reg(n)` — Value in register n (stored earlier in this row)

The value stored in register n in this row. Example under `save`.

#### `pload`
<!-- op pload -->
`c` → `prev(c)` — Column c of the previous row (--pstore; c a constant name)

Column c of the previous row, with `--pstore` (or `--group`): the value it had after the computes of that row, so that a column can build on its own previous value. c must be a constant naming an input or computed column. Before the first row the value is NA, unless `--pstore/LIST/DEFAULT` gives one (`--pstore//0`: 0 for every column). Section 8 describes `--pstore` and `--group`.

```console
$ pluk --pstore//0 picker kg 'total::^total,pload:kg,add' < harvest.txt
picker	kg	total
Annie	14.5	14.5
Bert	9.0	23.5
...
```

#### `groupi`
<!-- op groupi -->
`-` → `i` — Position of the row in its group (2 for the first output; --group)

With `--group=COL` or `--group-first-ref=COL`, rows with the same value in COL follow one another as a group, and the first row of each group is not output (it is the reference for `pload`). `groupi` is the position of the row in its group (2 for the first row output), `groupno` the number of the group. Without `--group` they are an error.

```console
$ { head -n 1 harvest.txt; tail -n +2 harvest.txt | sort -k2,2; } | pluk -q --group=fruit fruit picker 'i::,groupi' 'g::,groupno'
fruit	picker	i	g
apple	Chen	2	1
apple	Fenna	3	1
...
```

#### `groupno`
<!-- op groupno -->
`-` → `g` — Number of the row's group (1-based; --group)

The number of the row's group (1-based). Example under `groupi`.

#### `rowno`
<!-- op rowno -->
`-` → `x` — Current table row number (start one)

`rowno` is the number of the data row (1 for the first row after the header; with `-k` the first line is row 1), `r0wno` the same counted from 0, `lineno` the line number in the input. Rows that a filter rejects later still count, so the numbers are those of the input.

```console
$ pluk picker 'r::,rowno' 'z::,r0wno' 'l::,lineno' @fruit=apple < harvest.txt
picker	r	z	l
Annie	1	0	2
Chen	3	2	4
Fenna	6	5	7
```

#### `r0wno`
<!-- op r0wno -->
`-` → `x` — Current table row number (start zero)

The data row number counted from 0. Example under `rowno`.

#### `lineno`
<!-- op lineno -->
`-` → `x` — Current input line number

The line number in the input. Example under `rowno`.

### time

#### `now`
<!-- op now -->
`-` → `t` — Current epoch (unix seconds)

The current time in seconds since 1970-01-01 (the epoch), the same for every row of a run only to the second. The example tests that it is past 2001.

```console
$ pluk picker 'n::,now^1000000000^/gt/,test' '@picker=Annie' < harvest.txt
picker	n
Annie	1
```

#### `ttu`
<!-- op ttu -->
`tu` → `s` — Time unit (3h, 1.5d; units s m h d w y) in seconds

`ttu` reads a time unit, a number followed by one of s m h d w y (a year is 31556952 seconds), and gives seconds; anything else is NA (a constant that is not a time unit is an error before input is read). `rttu` writes seconds as days, hours, minutes and seconds (1d,2h,3m,4s; 0 as 0s; a negative number with a minus sign); a float is rounded to the nearest second.

```console
$ pluk 'a::^1.5d,ttu' 'b::^90061,rttu' 'c::^1.11m,ttu,rttu' '@picker=Annie' < harvest.txt
a	b	c
129600	1d,1h,1m,1s	1m,7s
```

#### `rttu`
<!-- op rttu -->
`s` → `tu` — Integer seconds as days, hours, minutes, seconds (1d,2h,3m,4s)

Seconds written as 1d,2h,3m,4s. Example under `ttu`.

#### `todate`
<!-- op todate -->
`u` → `date` — Epoch as local date and time (%Y-%m-%d-%H-%M-%S)

An epoch (seconds since 1970-01-01) as the local date and time, year-month-day-hour-minute-second. The time zone is the system's, or the one in the environment variable TZ. A float is NA.

```console
$ TZ=UTC pluk 'd::^0,todate' 'e::^1700000000,todate' '@picker=Annie' < harvest.txt
d	e
1970-01-01-00-00-00	2023-11-14-22-13-20
```

### bit

#### `bitand`
<!-- op bitand -->
`x y` → `x and y` — Bitwise and of |x| and |y|

Bitwise operations on the absolute values of integers: `bitand`, `bitor`, `bitxor`, `bitoff` (the bits of |x| without those of |y|), `bitlsh` and `bitrsh` (shift |x| left or right by |y| bits). A float, even 3.0, or a value that is not a number gives NA. A left shift whose result does not fit gives NA; a right shift by the integer width or more gives 0.

```console
$ printf '12\t10\n-12\t3\n5\t1.5\n' | pluk -k 1 2 'a::1:2,bitand' 'o::1:2,bitor' 'x::1:2,bitxor' 'f::1:2,bitoff' 'l::1:2,bitlsh' 'r::1:2,bitrsh'
12	10	8	14	6	4	12288	0
-12	3	0	15	15	12	96	1
5	1.5	NA	NA	NA	NA	NA	NA
```

#### `bitor`
<!-- op bitor -->
`x y` → `x or y` — Bitwise or of |x| and |y|

Bitwise or of |x| and |y|. Example under `bitand`.

#### `bitxor`
<!-- op bitxor -->
`x y` → `x xor y` — Bitwise exclusive or of |x| and |y|

Bitwise exclusive or of |x| and |y|. Example under `bitand`.

#### `bitoff`
<!-- op bitoff -->
`x y` → `x rm y` — Remove the bits of |y| from |x|

The bits of |x| without those of |y|. Example under `bitand`.

#### `bitlsh`
<!-- op bitlsh -->
`x y` → `x << y` — Shift |x| left by |y| (overflow: NA)

|x| shifted left by |y| bits (NA if the result does not fit). Example under `bitand`.

#### `bitrsh`
<!-- op bitrsh -->
`x y` → `x >> y` — Shift |x| right by |y|

|x| shifted right by |y| bits. Example under `bitand`.

### radix

#### `tobin`
<!-- op tobin -->
`x` → `x'` — Binary representation of x (negative: -101)

`tobin`, `tohex` and `tooct` write an integer in base 2, 16 or 8; a negative integer as a minus sign and its magnitude (-ff). A float gives NA. `binto`, `hexto` and `octto` read such a representation back: an optional minus, an optional prefix matching the base (0b, 0x, 0o, either case), and digits with at most single underscores between them. Anything else (a plus, spaces, other characters, an empty value, a number beyond the integer range) gives NA.

```console
$ printf '255\n-255\n2.5\n' | pluk -k 1 'b::1,tobin' 'h::1,tohex' 'o::1,tooct'
255	11111111	ff	377
-255	-11111111	-ff	-377
2.5	NA	NA	NA
$ printf '0xff\n-FF\n1_000\nff \n' | pluk -k 1 'h::1,hexto'
0xff	255
-FF	-255
1_000	4096
ff 	NA
```

#### `tohex`
<!-- op tohex -->
`x` → `x'` — Hex representation of x (negative: -ff)

x in hexadecimal (negative: -ff). Example under `tobin`.

#### `tooct`
<!-- op tooct -->
`x` → `x'` — Octal representation of x (negative: -17)

x in octal (negative: -17). Example under `tobin`.

#### `binto`
<!-- op binto -->
`x` → `x'` — Read binary representation x (optional - and 0b)

Reads a binary representation (optional - and 0b). Example under `tobin`.

#### `hexto`
<!-- op hexto -->
`x` → `x'` — Read hex representation x (optional - and 0x)

Reads a hexadecimal representation (optional - and 0x). Example under `tobin`.

#### `octto`
<!-- op octto -->
`x` → `x'` — Read octal representation x (optional - and 0o)

Reads an octal representation (optional - and 0o). Example under `tobin`.

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

1 if the alignment is on the forward strand, 0 if it is on the reverse strand (FLAG bit 0x10); NA if FLAG is not a number. Like `sam_get`, it works for every record, unmapped ones included. All SAM operators need `--sam` or `--sam-h` (section 10).

```console
$ pluk -q --sam 1 2 'f::,isforward' < align.sam
read1	0	1
read1	256	1
read2	16	0
read3	4	1
```

#### `ref_len`
<!-- op ref_len -->
`-` → `n` — Length of the reference sequence (seqlen: @SQ or fasta)

Positions and lengths of the alignment, 1-based and in the orientation of the reference (as SAM's POS), whatever the strand. `ref_len` is the length of the reference sequence, from the `@SQ` header lines or from the fasta of `--sam/FILE`. `ref_posx` and `ref_posy` are the first and last reference positions of the matched part; `qry_len` is the length of the read as the CIGAR gives it (M I S = X), and `qry_posx` `qry_posy` the first and last positions of the matched part in the read. The matched part of a CIGAR is what lies between its 5' trail (the leading run of S H D N I operations) and its 3' trail (the trailing run); it starts and ends with an aligned operation. For an unmapped record, or a CIGAR or POS that is not valid, these values are NA; without a reference length `ref_len` is NA ([SAM-NA.md](SAM-NA.md)).

```console
$ pluk -q --sam 1 6 'rl::,ref_len' 'rx::,ref_posx' 'ry::,ref_posy' 'ql::,qry_len' 'qx::,qry_posx' 'qy::,qry_posy' < align.sam
read1	3S22M3I8M3D9M	53	9	50	45	4	45
read1	20S25M	53	20	44	45	21	45
read2	2H4S12M3S	53	30	41	19	5	16
read3	*	NA	NA	NA	NA	NA	NA
```

#### `ref_posx`
<!-- op ref_posx -->
`-` → `p` — First reference position of the matched part (1-based)

The first reference position of the matched part. Example under `ref_len`.

#### `ref_posy`
<!-- op ref_posy -->
`-` → `p` — Last reference position of the matched part

The last reference position of the matched part. Example under `ref_len`.

#### `qry_len`
<!-- op qry_len -->
`-` → `n` — Query length from the CIGAR (M I S = X)

The length of the read according to the CIGAR (M I S = X; H not counted). Example under `ref_len`.

#### `qry_posx`
<!-- op qry_posx -->
`-` → `p` — First query position of the matched part (1-based)

The first position of the matched part in the read. Example under `ref_len`.

#### `qry_posy`
<!-- op qry_posy -->
`-` → `p` — Last query position of the matched part

The last position of the matched part in the read. Example under `ref_len`.

#### `ref_trail5p_N`
<!-- op ref_trail5p_N -->
`-` → `n` — Reference before the matched part (POS-1 + 5' trail)

How the reference and the read divide into the 5' trail, the matched part and the 3' trail. On the reference: `ref_trail5p_N` is everything before the matched part (POS - 1 plus the reference length of the 5' trail), `ref_matched_N` the matched part, `ref_trail3p_N` the rest of the reference (it needs the reference length); `ref_trail5p_V` and `ref_trail3p_V` are the reference lengths of the trails of the CIGAR alone (what an alignment display shows). On the read: `qry_trail5p_N`, `qry_matched_N` and `qry_trail3p_N` (I S M = X; H not counted).

```console
$ pluk -q --sam 1 6 'r5::,ref_trail5p_N' 'rm::,ref_matched_N' 'r3::,ref_trail3p_N' 'v5::,ref_trail5p_V' 'v3::,ref_trail3p_V' < align.sam
read1	3S22M3I8M3D9M	8	42	3	0	0
read1	20S25M	19	25	9	0	0
read2	2H4S12M3S	29	12	12	0	0
read3	*	NA	NA	NA	NA	NA
$ pluk -q --sam 1 6 'q5::,qry_trail5p_N' 'qm::,qry_matched_N' 'q3::,qry_trail3p_N' < align.sam
read1	3S22M3I8M3D9M	3	42	0
read1	20S25M	20	25	0
read2	2H4S12M3S	4	12	3
read3	*	NA	NA	NA
```

#### `ref_matched_N`
<!-- op ref_matched_N -->
`-` → `n` — Reference length of the matched part

The reference length of the matched part. Example under `ref_trail5p_N`.

#### `ref_trail3p_N`
<!-- op ref_trail3p_N -->
`-` → `n` — Reference after the matched part

The reference after the matched part (needs the reference length). Example under `ref_trail5p_N`.

#### `ref_trail5p_V`
<!-- op ref_trail5p_V -->
`-` → `n` — Reference length of the 5' trail of the CIGAR

The reference length of the CIGAR's 5' trail. Example under `ref_trail5p_N`.

#### `ref_trail3p_V`
<!-- op ref_trail3p_V -->
`-` → `n` — Reference length of the 3' trail of the CIGAR

The reference length of the CIGAR's 3' trail. Example under `ref_trail5p_N`.

#### `qry_trail5p_N`
<!-- op qry_trail5p_N -->
`-` → `n` — Query length of the 5' trail (S I)

The read length of the 5' trail. Example under `ref_trail5p_N`.

#### `qry_matched_N`
<!-- op qry_matched_N -->
`-` → `n` — Query length of the matched part

The read length of the matched part. Example under `ref_trail5p_N`.

#### `qry_trail3p_N`
<!-- op qry_trail3p_N -->
`-` → `n` — Query length of the 3' trail

The read length of the 3' trail. Example under `ref_trail5p_N`.

#### `aln_trail5p_cgr`
<!-- op aln_trail5p_cgr -->
`-` → `c` — The 5' trail of the CIGAR (leading S H D N I)

The three parts of the CIGAR as CIGAR strings: the 5' trail, the matched part and the 3' trail (substrings of the CIGAR as written).

```console
$ pluk -q --sam 1 6 't5::,aln_trail5p_cgr' 'm::,aln_matched_cgr' 't3::,aln_trail3p_cgr' < align.sam
read1	3S22M3I8M3D9M	3S	22M3I8M3D9M	
read1	20S25M	20S	25M	
read2	2H4S12M3S	2H4S	12M	3S
read3	*	NA	NA	NA
```

#### `aln_matched_cgr`
<!-- op aln_matched_cgr -->
`-` → `c` — The matched part of the CIGAR

The matched part of the CIGAR. Example under `aln_trail5p_cgr`.

#### `aln_trail3p_cgr`
<!-- op aln_trail3p_cgr -->
`-` → `c` — The 3' trail of the CIGAR

The 3' trail of the CIGAR. Example under `aln_trail5p_cgr`.

#### `aln_nmatch`
<!-- op aln_nmatch -->
`-` → `n` — M = X length of the matched part

`aln_nmatch` is the number of aligned bases in the matched part (M = X); `aln_nedit` the edit distance from the NM:i tag (0 without one, NA for a value that is not a non-negative integer; it works for every record); `aln_nmatchx` the mismatches, NM minus the deleted and inserted bases of the matched part.

```console
$ pluk -q --sam 1 6 'n::,aln_nmatch' 'e::,aln_nedit' 'x::,aln_nmatchx' < align.sam
read1	3S22M3I8M3D9M	39	7	1
read1	20S25M	25	12	12
read2	2H4S12M3S	12	1	1
read3	*	NA	0	NA
```

#### `aln_nedit`
<!-- op aln_nedit -->
`-` → `n` — Edit distance from NM:i (0 without NM)

The edit distance from NM:i. Example under `aln_nmatch`.

#### `aln_nmatchx`
<!-- op aln_nmatchx -->
`-` → `n` — NM minus the D and I of the matched part (mismatches)

The number of mismatches: NM minus the D and I of the matched part. Example under `aln_nmatch`.

#### `qry_seq`
<!-- op qry_seq -->
`-` → `seq` — Query sequence (SEQ, or filled for SEQ '*'; --sam-seqmiss)

The read's sequence and quality, as stored in SEQ and QUAL (in the orientation of the reference). A SEQ `*`, as aligners write for secondary alignments, is filled from an earlier record of the same read whose part of the read covers it (reverse-complemented when the strands differ, hard clips taken into account); the quality is filled with it. A record that cannot be filled is skipped, or ends the run with `--sam-seqmiss=exit` (section 10). A SEQ or QUAL whose length does not fit the CIGAR gives NA.

```console
$ pluk -q --sam 1 2 6 's::,qry_seq' 'q::,qry_qual' < align.sam
read1	0	3S22M3I8M3D9M	CCAGCTTAACGGTACAATGAGCTAGTTTCTAAGGTTTGCAATTCG	*
read1	256	20S25M	CCAGCTTAACGGTACAATGAGCTAGTTTCTAAGGTTTGCAATTCG	*
read2	16	2H4S12M3S	GGTTGCTAAAGTTCCACCC	IIIIIIIIIIIIIIIIIII
read3	4	*	ACGTTGCAAGGT	*
```

#### `qry_qual`
<!-- op qry_qual -->
`-` → `qual` — Query quality (QUAL, or filled with the sequence)

The read's quality (QUAL, or filled with the sequence; `*` if absent; with `--sam-qlt-sim` simplified, section 10). Example under `qry_seq`.

#### `aln_ref`
<!-- op aln_ref -->
`-` → `aln` — Alignment: reference line (needs --sam/FILE)

The alignment drawn as strings, from POS, the CIGAR, the read and the reference (which comes from the fasta of `--sam/FILE`; a compile-time error without it). `aln_ref` is the reference line, `aln_aln` the line between, `aln_qry` the read, `aln_rlr` a ruler with the reference position written at every multiple of 10, `aln_qlt` the quality under the read (`!` for every base when QUAL is `*`). In `aln_aln`: `|` match and `x` mismatch (upper and lower case compare equal), `~` insertion and `-` deletion, `=` an N (an intron); at either end a run of insertions is drawn `_` and a run of deletions `` ` `` (dangling), and context added with `--sam-aln-context` is drawn `.` (section 10). Soft clips are not drawn, hard clips and padding are ignored. An unmapped record, or a CIGAR that runs past the end of the reference, gives NA.

```console
$ pluk -q --sam/transgene.fa 1 'r::,aln_ref' 'a::,aln_aln' 'q::,aln_qry' 'l::,aln_qlt' '@1=read2' < align.sam
read2	GCTAAGGTTCCA	|||||x||||||	GCTAAAGTTCCA	IIIIIIIIIIII
```

#### `aln_aln`
<!-- op aln_aln -->
`-` → `aln` — Alignment: match line (| x ~ - = _ `)

The alignment line (| x ~ - = _ ` .). Example under `aln_ref`.

#### `aln_qry`
<!-- op aln_qry -->
`-` → `aln` — Alignment: query line

The read line of the alignment. Example under `aln_ref`.

#### `aln_rlr`
<!-- op aln_rlr -->
`-` → `aln` — Alignment: ruler with reference positions

The ruler of the alignment. Examples under `aln_ref` and `aln_all`.

#### `aln_qlt`
<!-- op aln_qlt -->
`-` → `aln` — Alignment: query quality line

The quality line of the alignment (with `--sam-qlt-sim` simplified, section 10). Example under `aln_ref`.

#### `aln_all`
<!-- op aln_all -->
`-` → `aln` — Alignment: ruler, reference, match, query lines

`aln_all` joins the ruler, the reference, the alignment and the read line with newlines, so that the output shows the alignment as a picture (the field then contains newlines); `aln_allq` adds the quality line; `aln_all_pack` joins with the byte 0x01 instead of newlines (one line per record, for further processing). Every line begins with the text of `--sam-aln-prefix=STR` (for example `'++ '`); `--sam-aln-mrk` adds markers and the part lengths to the reference and read lines (section 10).

```console
$ pluk -q --sam/transgene.fa '--sam-aln-prefix=++ ' 'a::,aln_allq' '@1=read2' < align.sam
++ 30        40
++ GCTAAGGTTCCA
++ |||||x||||||
++ GCTAAAGTTCCA
++ IIIIIIIIIIII
$ pluk -q --sam/transgene.fa 1 'p::,aln_all_pack' '@1=read2' < align.sam | tr '\001' '#'
read2	30        40#GCTAAGGTTCCA#|||||x||||||#GCTAAAGTTCCA
```

#### `aln_allq`
<!-- op aln_allq -->
`-` → `aln` — Alignment: as aln_all, with the quality line

As `aln_all`, with the quality line. Example under `aln_all`.

#### `aln_all_pack`
<!-- op aln_all_pack -->
`-` → `aln` — Alignment: as aln_all, lines joined by byte 0x01

As `aln_all`, the lines joined by the byte 0x01. Example under `aln_all`.

#### `aln_nedit_pia`
<!-- op aln_nedit_pia -->
`-` → `n` — Edits in aln_aln, intron aware (x - ~)

The number of edits in the alignment line: `aln_nedit_pia` counts the columns `x`, `-` and `~` (an N, `=`, is an intron and not an edit), `aln_nedit_piu` counts `=` as well. Dangling columns (`` ` `` `_` `.`) are never edits.

```console
$ pluk -q --sam/transgene.fa 1 6 'a::,aln_nedit_pia' 'u::,aln_nedit_piu' '@@2/none/260' < align.sam
read1	3S22M3I8M3D9M	7	7
read2	2H4S12M3S	1	1
```

#### `aln_nedit_piu`
<!-- op aln_nedit_piu -->
`-` → `n` — Edits in aln_aln, intron unaware (x - ~ =)

The number of edits, introns (=) included. Example under `aln_nedit_pia`.

#### `aln_posinfo`
<!-- op aln_posinfo -->
`n` → `info` — Mismatch and indel positions; indel sequences up to length n (0: all)

The differences between read and reference, in CIGAR order, joined by `:`: `x=P,c=RQ` for a mismatch at reference position P (R the reference base, Q the read's, upper case), `i=P,n=L` for an insertion of L bases before position P, `d=P,n=L` for a deletion and `e=P,n=L` for an N starting at P. For an insertion, deletion or N the bases are added as `,s=SEQ` when n is 0 or L is at most n; n must be a non-negative integer, else NA. Context is not reported. The empty string means no difference.

```console
$ pluk -q --sam/transgene.fa 1 'a::^0,aln_posinfo' 'b::^2,aln_posinfo' '@@2/none/260' < align.sam
read1	x=21,c=CA:i=31,n=3,s=TTT:d=39,n=3,s=CCA	x=21,c=CA:i=31,n=3:d=39,n=3
read2	x=35,c=GA	x=35,c=GA
```

#### `patchiness`
<!-- op patchiness -->
`s d` → `score` — Largest penalty sum of runs in alignment string s within window d

A score for how clustered the differences in an alignment line s are, over a sliding window of d columns: s is read as runs of one symbol, each run counting once with a penalty (`|` `=` `.` `` ` `` `_` 0, `-` `~` `+` 1, `x` 2; `(` and `)` count as `|`, a space as `x`), and the result is the largest sum of the penalties of the runs that start within d columns of each other. `patchiness2` measures the window in aligned columns only (the dangling, deletion and insertion columns do not count). A symbol outside these, or a d that is not a non-negative integer, gives NA. Like the other SAM operators they need `--sam` (also for an alignment string from elsewhere, here a constant).

```console
$ pluk -q --sam/transgene.fa 1 'a::,aln_aln' 'p::,aln_aln^10,patchiness' 'q::,aln_aln^10,patchiness2' '@1=read1' '@@2=0' < align.sam
read1	||||||||||||x|||||||||~~~||||||||---|||||||||	3	3
$ pluk -q --sam 'a::^xx|x||||||x-x^1,patchiness' 'b::^xx|x||||||x-x^3,patchiness' 'c::^xx|x||||||x-x^3,patchiness2' '@1=read2' < align.sam
3	5	5
```

#### `patchiness2`
<!-- op patchiness2 -->
`s d` → `score` — As patchiness; the window counts aligned columns only

As `patchiness`, the window counting aligned columns only. Example under `patchiness`.

#### `sam_rbt`
<!-- op sam_rbt -->
`x y` → `-` — Set the rbt region to reference positions x..y (for the rbt_ operators)

The region-bound tally looks at the part of an alignment that lies within a region of the reference, positions x to y. The region is set for the rest of the row by `sam_rbt` (it takes x and y, pushes nothing, and is normally written as a hidden compute: `r:=^20^35,sam_rbt`; x and y may come from columns), or for every row by `--sam-rbt=X,Y`; the later `sam_rbt` in a row wins. An rbt_ operator without either is a compile-time error; operands that are not integers 1 <= x <= y give NA.

```console
$ pluk -q --sam/transgene.fa 1 'r:=^20^35,sam_rbt' 'e::,rbt_nedit' 's:=^1^5,sam_rbt' 'f::,rbt_nedit' '@1=read1' '@@2=0' < align.sam
read1	4	NA
```

#### `rbt_refx`
<!-- op rbt_refx -->
`-` → `p` — rbt: first reference position (region clipped to the matched part)

The region is clipped to the matched part of the alignment. `rbt_refx` and `rbt_refy` are the clipped region on the reference, `rbt_dx` and `rbt_dy` the positions cut off at the start and at the end; `rbt_qryx`, `rbt_qryy` and `rbt_qrylen` the corresponding part of the read (1-based, in SEQ orientation, soft clips counted; a region that lies within a deletion has `rbt_qrylen` 0), and `rbt_qry` its sequence. A region that does not overlap the matched part gives NA for every rbt_ value (counted at the end).

```console
$ pluk -q --sam/transgene.fa --sam-rbt=5,30 1 'rx::,rbt_refx' 'ry::,rbt_refy' 'dx::,rbt_dx' 'dy::,rbt_dy' 'qx::,rbt_qryx' 'qy::,rbt_qryy' 'ql::,rbt_qrylen' 'q::,rbt_qry' '@@2/none/260' < align.sam
read1	9	30	4	0	4	25	22	GCTTAACGGTACAATGAGCTAG
read2	30	30	25	0	5	5	1	G
```

#### `rbt_refy`
<!-- op rbt_refy -->
`-` → `p` — rbt: last reference position

The last reference position of the clipped region. Example under `rbt_refx`.

#### `rbt_dx`
<!-- op rbt_dx -->
`-` → `n` — rbt: positions cut off at the start (rbt_refx - x)

The positions cut off the region at the start (rbt_refx - x). Example under `rbt_refx`.

#### `rbt_dy`
<!-- op rbt_dy -->
`-` → `n` — rbt: positions cut off at the end (y - rbt_refy)

The positions cut off the region at the end (y - rbt_refy). Example under `rbt_refx`.

#### `rbt_qryx`
<!-- op rbt_qryx -->
`-` → `p` — rbt: first query position

The first read position of the region. Example under `rbt_refx`.

#### `rbt_qryy`
<!-- op rbt_qryy -->
`-` → `p` — rbt: last query position

The last read position of the region. Example under `rbt_refx`.

#### `rbt_qrylen`
<!-- op rbt_qrylen -->
`-` → `n` — rbt: query length

The number of read bases in the region. Example under `rbt_refx`.

#### `rbt_qry`
<!-- op rbt_qry -->
`-` → `seq` — rbt: query sequence

The read's sequence in the region. Example under `rbt_refx`.

#### `rbt_nmatch`
<!-- op rbt_nmatch -->
`-` → `n` — rbt: aligned bases (| x)

Counts over the columns of `aln_aln` within the region: `rbt_nmatch` the aligned bases (`|` and `x`), `rbt_nmatchx` the mismatches (`x`), `rbt_ndel` the deleted bases (`-`), `rbt_nins` the inserted ones (`~`), and `rbt_nedit` their total of edits (`x` `-` `~`; an N, `=`, counts in none). An insertion counts only when it lies between two positions of the region.

```console
$ pluk -q --sam/transgene.fa --sam-rbt=20,35 1 'n::,rbt_nmatch' 'x::,rbt_nmatchx' 'd::,rbt_ndel' 'i::,rbt_nins' 'e::,rbt_nedit' '@@2/none/260' < align.sam
read1	16	1	0	3	4
read2	6	1	0	0	1
```

#### `rbt_nmatchx`
<!-- op rbt_nmatchx -->
`-` → `n` — rbt: mismatched aligned bases (x)

The mismatches in the region. Example under `rbt_nmatch`.

#### `rbt_ndel`
<!-- op rbt_ndel -->
`-` → `n` — rbt: deleted bases (-)

The deleted bases in the region. Example under `rbt_nmatch`.

#### `rbt_nins`
<!-- op rbt_nins -->
`-` → `n` — rbt: inserted bases (~)

The inserted bases in the region. Example under `rbt_nmatch`.

#### `rbt_nedit`
<!-- op rbt_nedit -->
`-` → `n` — rbt: edits (- ~ x)

The edits in the region. Examples under `rbt_nmatch` and `sam_rbt`.

#### `rbt_alnref`
<!-- op rbt_alnref -->
`-` → `aln` — rbt: the region's columns of aln_ref

The columns of the alignment strings that lie within the region: `rbt_alnref`, `rbt_alnaln`, `rbt_alnqry`, `rbt_alnrlr` and `rbt_alnqlt` cut `aln_ref` and the others; `rbt_alnall` joins the region's ruler, reference, alignment, read and quality lines with newlines, each line beginning with `--sam-aln-prefix` (a ruler number that begins before the region shows only its last digits).

```console
$ pluk -q --sam/transgene.fa --sam-rbt=20,35 'a::,rbt_alnall' '@1=read1' '@@2=0' < align.sam
20        30       
CCATGAGCTAG---CTAAG
|x|||||||||~~~|||||
CAATGAGCTAGTTTCTAAG
!!!!!!!!!!!!!!!!!!!
$ pluk -q --sam/transgene.fa --sam-rbt=20,35 1 'r::,rbt_alnref' 'a::,rbt_alnaln' 'q::,rbt_alnqry' '@1=read1' '@@2=0' < align.sam
read1	CCATGAGCTAG---CTAAG	|x|||||||||~~~|||||	CAATGAGCTAGTTTCTAAG
```

#### `rbt_alnaln`
<!-- op rbt_alnaln -->
`-` → `aln` — rbt: the region's columns of aln_aln

The region's columns of `aln_aln`. Example under `rbt_alnref`.

#### `rbt_alnqry`
<!-- op rbt_alnqry -->
`-` → `aln` — rbt: the region's columns of aln_qry

The region's columns of `aln_qry`. Example under `rbt_alnref`.

#### `rbt_alnrlr`
<!-- op rbt_alnrlr -->
`-` → `aln` — rbt: the region's columns of aln_rlr

The region's columns of `aln_rlr`. Example under `rbt_alnref`.

#### `rbt_alnqlt`
<!-- op rbt_alnqlt -->
`-` → `aln` — rbt: the region's columns of aln_qlt

The region's columns of `aln_qlt`. Example under `rbt_alnref`.

#### `rbt_alnall`
<!-- op rbt_alnall -->
`-` → `aln` — rbt: ruler, reference, match, query, quality lines of the region

The region's ruler, reference, alignment, read and quality lines. Example under `rbt_alnref`.

#### `ref_trail5p`
<!-- op ref_trail5p -->
`-` → `seq` — Reference before the matched part (needs --sam/FILE)

The sequences of the three parts. `ref_trail5p`, `ref_matched` and `ref_trail3p` cut the reference (from the fasta of `--sam/FILE`, which they need; a compile-time error without it); `qry_trail5p`, `qry_matched` and `qry_trail3p` cut the read (SEQ, or the filled sequence). A record that is unmapped, or whose matched part runs past the end of the reference, gives NA.

```console
$ pluk -q --sam/transgene.fa 1 'a::,ref_trail5p' 'b::,ref_matched' 'c::,ref_trail3p' '@@2/none/4' < align.sam
read1	GATTACAG	GCTTAACGGTACCATGAGCTAGCTAAGGTTCCATGCAATTCG	AGG
read1	GATTACAGGCTTAACGGTA	CCATGAGCTAGCTAAGGTTCCATGC	AATTCGAGG
read2	GATTACAGGCTTAACGGTACCATGAGCTA	GCTAAGGTTCCA	TGCAATTCGAGG
$ pluk -q --sam 1 'a::,qry_trail5p' 'b::,qry_matched' 'c::,qry_trail3p' '@@2/none/4' < align.sam
read1	CCA	GCTTAACGGTACAATGAGCTAGTTTCTAAGGTTTGCAATTCG	
read1	CCAGCTTAACGGTACAATGA	GCTAGTTTCTAAGGTTTGCAATTCG	
read2	GGTT	GCTAAAGTTCCA	CCC
```

#### `ref_matched`
<!-- op ref_matched -->
`-` → `seq` — Reference of the matched part (needs --sam/FILE)

The reference sequence of the matched part. Example under `ref_trail5p`.

#### `ref_trail3p`
<!-- op ref_trail3p -->
`-` → `seq` — Reference after the matched part (needs --sam/FILE)

The reference after the matched part. Example under `ref_trail5p`.

#### `qry_trail5p`
<!-- op qry_trail5p -->
`-` → `seq` — Query sequence of the 5' trail

The read's sequence of the 5' trail. Example under `ref_trail5p`.

#### `qry_matched`
<!-- op qry_matched -->
`-` → `seq` — Query sequence of the matched part

The read's sequence of the matched part. Example under `ref_trail5p`.

#### `qry_trail3p`
<!-- op qry_trail3p -->
`-` → `seq` — Query sequence of the 3' trail

The read's sequence of the 3' trail. Example under `ref_trail5p`.

#### `sam_put`
<!-- op sam_put -->
`tag val` → `-` — Put tag:val in the optional fields (column 12; replaces the same tag)

Puts tag:val into the optional fields (column 12), replacing the same tag. Example under `sam_get`.

#### `sam_get`
<!-- op sam_get -->
`tag def` → `value` — Value of optional field tag (XX:T, XX.T, XX-T), def if absent

`sam_get` gives the value of the first optional field with tag and type XX:T, or def if there is none. `sam_put` puts XX:T:val into the optional fields (column 12, which `--sam` makes hold them all), after the computes: a tag already there is replaced (counted, with a message for the first few rows), else the field is appended. The tag must be a constant, written XX.T or XX-T in a compute (':' separates the parts of a compute); T is one of A i f Z H B. sam_put checks the value against its type and does not put a value that is NA, contains a TAB or newline, or does not fit the type (counted). `sam_put` pushes nothing; its compute is normally hidden (`:=`).

```console
$ pluk -q --sam 1 'n::^NM.i^0,sam_get' 'a::^AS.i^none,sam_get' < align.sam
read1	7	30
read1	12	none
read2	1	none
read3	0	none
$ pluk -q --sam 1 12 't:=^XP.i:4,sam_put' 'u:=^NM.i^0,sam_get,incr^NM.i,xch,sam_put' < align.sam
read1	NM:i:8	AS:i:30	XP:i:9
read1	NM:i:13	XP:i:20
read2	NM:i:2	XP:i:30
read3	XP:i:0	NM:i:1
```

### bio

#### `rc`
<!-- op rc -->
`x` → `rc(x)` — Reverse complement (IUPAC codes complemented, U as A)

`rc` gives the reverse complement of x: A C G T become T G C A in either case, U becomes A, the IUPAC codes are complemented (R Y, K M, B V, D H) and N, S, W and every other byte stay as they are. `todna` makes x DNA: u and U become t and T, other letters n or N (keeping the case), and every other byte N.

```console
$ printf 'ACGTN\nacgu\nRYKM-\n' | pluk -k 1 'r::1,rc' 'd::1,todna'
ACGTN	NACGT	ACGTN
acgu	acgt	acgt
RYKM-	-KMRY	NNNNN
```

#### `forward`
<!-- op forward -->
`B seq` → `forward` — seq if B is true, else rc(seq)

seq if B is true, otherwise its reverse complement (`rc`); B is true as for `ifelse`. With SAM input, `isforward` gives B: `,isforward,qry_seq,forward` is the read as it was sequenced (SAM stores the sequence of a reverse-strand alignment reverse-complemented).

```console
$ printf '1\tACGTT\n0\tACGTT\n' | pluk -k 1 2 'f::1:2,forward'
1	ACGTT	ACGTT
0	ACGTT	AACGT
```

#### `todna`
<!-- op todna -->
`x` → `x'` — DNA letters: uU to tT, other letters n N, other bytes N

x as DNA letters. Example under `rc`.

#### `dnatl`
<!-- op dnatl -->
`x f` → `protein` — DNA (or RNA) x translated from offset f (stop *, unknown ?, rest {..})

`dnatl` translates x from offset f (0, 1, 2, ...) with the standard genetic code: x is read in upper case, U as T; a stop codon is *, a codon with other letters (N, IUPAC codes) ?, and the one or two bases left at the end are added between braces. An x not longer than f gives the empty string; f must be a non-negative integer, else NA. `dnagc` gives the percentage of G and C among the A C G T U of x (either case), with N decimals (0 to 1000); without any of those bases it is NA.

```console
$ printf 'ATGGCCTAA\nAUGUUU\nACGTNNNN\nNNNN\n' | pluk -k 1 't::1^0,dnatl' 'g::1^1,dnagc'
ATGGCCTAA	MA*	44.4
AUGUUU	MF	16.7
ACGTNNNN	T?{NN}	50.0
NNNN	?{N}	NA
```

#### `dnagc`
<!-- op dnagc -->
`x N` → `gc(x)` — Percentage G C among the A C G T U of x, N decimals (none: NA)

The G C percentage of x, with N decimals. Example under `dnatl`.

#### `fasta`
<!-- op fasta -->
`i s` → `fasta` — FASTA record: >i newline s

`fasta` writes a FASTA record of identifier i and sequence s; `fastq` a FASTQ record with the quality Z for every base, `fastQ` one with the quality q. A q of `*` (SAM's missing quality) gives Z for every base, as `fastq` (unless s is one base long); another q whose length differs from s gives NA, counted at the end. The record contains newlines; output only that column to get a file.

```console
$ pluk -h 'f::picker^ACGT,fasta' '@fruit=pear' < harvest.txt
>Bert
ACGT
>Gus
ACGT
$ pluk -h 'q::picker^ACGT^!!%23%23,fastQ' '@picker=Bert' < harvest.txt
@Bert
ACGT
+
!!##
```

#### `fastq`
<!-- op fastq -->
`i s` → `fastq` — FASTQ record of i and s, quality Z for every base

A FASTQ record with quality Z for every base. Example under `fasta`.

#### `fastQ`
<!-- op fastQ -->
`i s q` → `fastq` — FASTQ record of i, s and quality q (q * gives Z; other length: NA)

A FASTQ record with quality q. Example under `fasta`.

#### `strpair`
<!-- op strpair -->
`x y` → `pair(x,y)` — Per position | where x and y are equal, x where not (shorter length)

For each position up to the shorter of x and y, | where the bytes are equal and x where they differ (exact, case matters). The result uses the symbols of `aln_aln`, so `patchiness` can score it.

```console
$ printf 'ACGTA\tACCTA\nacgt\tACGT\n' | pluk -k 1 2 's::1:2,strpair'
ACGTA	ACCTA	||x||
acgt	ACGT	xxxx
```

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
