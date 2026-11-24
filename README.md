# Introduction to pluk

| **Pluk in a nutshell** |
| :--- |
| `pluk` is akin to a **column-name** and **column-type aware** `grep` married with `cut`.  Using column names as variables it can update existing columns, compute new columns or construct formatted output from the rows like `sed`. |

*pluk is a Rust reimplementation of [pick](http://github.com/micans/pick),
a Perl program by [Stijn van Dongen](http://github.com/micans) that served as pluk's prototype. All code was written by Claude (Anthropic) in
dialogue with Stijn.  The work was staged into steps, each a small set of items.  Every step was discussed and its
decisions written down before any code was written, then implemented item by item, tested against pick and committed by
Stijn over a period of about four days. The `plan/` directory retains that history.  Many corner/edge cases in pick
were settled and sometimes fixed, syntax was made cleaner in a few places, features were added.
Nearly all pick command lines are also valid pluk invocations, but some lesser-used pick options were
renamed in pluk to increase consistency.*


`pluk` distinguishes strings, integer and floats, infers types from operators and filters, and issues appropriate conversions
and warnings as necessary. Errors propagate to `NA` and are summarised in diagnostic messages.

`pluk` streams a table, i.e. a file with a header line and tab-separated columns, and lets you manipulate rows using column
names. It processes the table line by line and **munches through large data fast using almost no memory**.  It supports

-  filters and assertions over input data and output data
-  joining annotation tables onto the input data, considered part of the input data
-  editing/changing existing columns
-  computing new columns
-  reading compressed data transparently
-  tables without headers, including ragged data
-  SAM/BAM input with compute operators to extract alignment information
-  spliting a table into files

Everything is specified on the command line. Pluk aims for expressive and concise invocations
that naturally read from left to right, using a stack-based syntax for new columns.
This syntax is perhaps idiosyncratic but simple and effective, or possibly simple and effective but idiosyncratic.

-  Short options set often-used modalities, e.g. `-A` (select all), `-k` (headerless input).
-  Short options combine, e.g. `-Ak/^#` (as above, skip lines starting with `#`).
-  A column is selected simply by naming it.
-  A filter is indicated by `@`, e.g. `@weight/gt/1.0`, filtering rows by column `weight`.
-  A new column looks like this: `gram::kg^1000,mul`, or `fahrenheit::^9^5,div:celcius,mul^32,add`.

Further examples below use the data [`doc/harvest.txt`](doc/harvest.txt) (the
examples run from the top of the repository, with pluk found in PATH):

```text
picker  fruit   tree  kg    bruised  ripeness
Annie   apple   A12   14.5  2        0.92
Bert    pear    P03   9.0   0        0.75
Chen    apple   A07   22.1  5        0.88
Dora    plum    U11   6.4   11       0.99
Eamon   cherry  C02   3.2   0        0.61
Fenna   apple   A12   18.0  1        0.95
Gus     pear    P08   12.7  3        0.80
Hilde   plum    U02   7.9   0        0.70
```


## Install

```sh
cargo build --release     # the program: target/release/pluk
make test                 # build, unit tests, the test suite, the manual's examples
make test-int128          # the same with 128-bit integers; binary target/release/pluk7
```

On Macos I've used brew for installing rust:

```sh
brew install rustup-init
rustup-init
rustup update stable
```

On Linux I've used the scary `curl`-pipe-to-`sh` command line that seems the installation
advice front, right and center everywhere. It does lead to a lingering feeling of queasiness.


## A quick tour

**Pick columns by name**, in the order you want them.

```console
$ pluk picker kg < doc/harvest.txt
picker  kg
Annie   14.5
Bert    9.0
...
```

**Leave column names out**, by name or by pattern (`+` starts a regular expression that must
match the whole name).

```console
$ pluk -x tree '+r.*' < doc/harvest.txt
picker  fruit   kg    bruised
Annie   apple   14.5  2
...
```

**Specify rows to keep.** Filters start with `@`. Below selects apple rows with a weight minimum.

```console
$ pluk -A @fruit=apple @kg/gt/15 < doc/harvest.txt
picker  fruit   tree  kg    bruised  ripeness
Chen    apple   A07   22.1  5        0.88
Fenna   apple   A12   18.0  1        0.95
```

```console
$ pluk -c @fruit=apple < doc/harvest.txt
3
```

**Make new columns.** A compute is `NAME::` followed by a small stack program: `:` pushes a
column, `^` a constant, `,` applies an operator. Here, pounds (to one decimal) and
bruises per kilo (to two decimals). No quotes are needed: `:` `^` `,` `@` and `/` are not special to
the shell. An argument with `*`, `?`, `[`, `\`, `(`, `$` or a space does need them.

```console
$ pluk picker kg lb::kg^2.20462,mul^1,dd bpk::bruised:kg,div^2,dd < doc/harvest.txt
picker  kg    lb    bpk
Annie   14.5  32.0  0.14
Bert    9.0   19.8  0.00
Chen    22.1  48.7  0.23
...
```

**Look things up.** A dictionary (in the file `doc/prices.txt`), and the revenue per row:

```console
$ pluk --fd-PRICE=doc/prices.txt picker fruit kg 'eur::fruit^PRICE,map:kg,mul^2,dd' < doc/harvest.txt
picker  fruit  kg    eur
Annie   apple  14.5  30.45
Bert    pear   9.0   22.50
...
```

**Join a table**: its columns are appended to each row, looked up by key. A failed look-up yields NA.

```console
$ pluk --join^tree=doc/trees.txt picker tree planted row < doc/harvest.txt
picker  tree  planted  row
Annie   A12   2009     1
Bert    P03   2011     2
...
Hilde   U02   NA       NA
```

**Edit in place**, `sed`-style but per column (`-i`; `__` is the column being changed).

```console
$ pluk -i -A fruit::__,uc picker::__^e^3,edg < doc/harvest.txt
picker  fruit  tree  kg    bruised  ripeness
Anni3   APPLE  A12   14.5  2        0.92
B3rt    PEAR   P03   9.0   0        0.75
Ch3n    APPLE  A07   22.1  5        0.88
...
```

**Sort the harvest into crates**: one file per fruit, each with the header (`-q` silences the tally pluk writes to stderr).

```console
$ mkdir crates
$ pluk -q --demux=fruitfile --demux-dir=crates -A fruitfile:=fruit^.txt < doc/harvest.txt
$ ls crates
apple.txt
cherry.txt
pear.txt
plum.txt
```

**Validate the data.** An assertion, if failed, stops the run. Here Eamon's cherries
fail the ripeness threshold.

```console
$ pluk -A @assert@ripeness/ge/0.65 < doc/harvest.txt
picker  fruit   tree  kg    bruised  ripeness
Annie   apple   A12   14.5  2        0.92
...
Dora    plum    U11   6.4   11       0.99
pluk: assertion @assert@ripeness/ge/0.65 failed: column ripeness value [0.61] at line 6
```

`pluk -l` lists the operators by category (184 of them, and 34 filter operators), and
`pluk -l TERM` finds them by name.


## SAM and BAM support.

pluk reads SAM and BAM. Given the reference sequences it computes the trailing and aligned parts of both
query and reference, counts edits, and supports similar quantification and output of sub-regions of aligned parts.
It is intended for relatively short non-genomic references such as transcripts, transgenes, plasmids and amplicons.
Here is [one read](doc/reads.sam) against a [53-base transgene](doc/transgene.fa) in this case showing
the full alignment. Below `aln_posinfo` gives _Mismatch and indel positions; indel sequences up to length n_.

```console
$ pluk --sam/doc/transgene.fa 1 6 n::,aln_nedit p::^0,aln_posinfo < doc/reads.sam
read1   3S22M3I8M3D9M   7   x=21,c=CA:i=31,n=3,s=TTT:d=39,n=3,s=CCA
```

```console
$ pluk -q --sam/doc/transgene.fa a::,aln_all < doc/reads.sam
 10        20        30           40        5
GCTTAACGGTACCATGAGCTAG---CTAAGGTTCCATGCAATTCG
||||||||||||x|||||||||~~~||||||||---|||||||||
GCTTAACGGTACAATGAGCTAGTTTCTAAGGTT---TGCAATTCG
```

A ruler, the reference, the alignment (`|` match, `x` mismatch, `~` insertion, `-`
deletion) and the read. Alignment context can be narrowed or widened. Below shows
the context along the clipped bases with `--sam-aln-context=5`:

````console
$ pluk -q --sam/doc/transgene.fa --sam-aln-context=5 a::,aln_all < doc/reads.sam
      10        20        30           40        50
TACAGGCTTAACGGTACCATGAGCTAG---CTAAGGTTCCATGCAATTCGAGG
``...||||||||||||x|||||||||~~~||||||||---|||||||||```
--CCAGCTTAACGGTACAATGAGCTAGTTTCTAAGGTT---TGCAATTCG---
````

Or zoom in on a region and tally what happens there (positions 20 to 35: one mismatch,
three inserted bases):

```console
$ pluk -q --sam/doc/transgene.fa --sam-rbt-region=20,35 e::,rbt_nedit < doc/reads.sam
4
```


## Where next

- [The pluk manual](doc/PLUK-MANUAL.md): everything above in detail, and the rest.
- `pluk --help` for the options, `pluk -l` for the operators.
- [SAM-NA.md](doc/SAM-NA.md): what happens when a SAM value cannot be computed.
