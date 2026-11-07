# NA in SAM processing

This note describes the most likely ways in which a SAM value becomes NA in pluk, how an
NA travels on through computes, filters and output, and what pluk reports about it. It
does not try to cover every unusual record. The reasons and their exact scope are in
plan/SAMVIEW.design.txt section 6.

pick, in the same situations, mostly discards the row without a message, or computes a
value from data that is not there (a reference length of 0, an empty sequence). pluk
keeps the row, writes NA where a value cannot be computed, and counts the reasons.


## 1. Where NA comes from

Each reason is counted once per row and reported at the end of the run (on stderr,
silenced by `-q`). The examples use [mixed.sam](mixed.sam), one mapped and one unmapped
read against [transgene.fa](transgene.fa):

```console
$ pluk --sam 1 q::,qry_len < mixed.sam
read1   45
read2   NA
pluk: SAM values not available (NA) for 1 row: unmapped (FLAG 0x4, RNAME or CIGAR '*')
```

| Reason (as reported)                            | Typical cause                                           | Operators that give NA |
|-------------------------------------------------|---------------------------------------------------------|------------------------|
| unmapped (FLAG 0x4, RNAME or CIGAR '*')         | unmapped reads in aligner output                        | all position, count, CIGAR-part, sequence-part, alignment and rbt operators |
| CIGAR or POS not valid                          | a damaged or hand-edited record                         | the same |
| reference length not known                      | SAM without header and without `--sam/FILE`              | `ref_len`, `ref_trail3p_N`; the alignment strings with `--sam-aln-mrk` |
| SEQ or QUAL length differs from the CIGAR       | a SEQ or QUAL that does not fit the CIGAR (also after a fill) | `qry_seq`, `qry_qual`, the `qry_` parts, the alignment strings and everything computed from them |
| CIGAR past the reference end                    | a reference fasta that does not match the alignment     | the `ref_` parts, the alignment strings and everything computed from them |
| rbt region outside the matched part             | `sam_rbt` or `--sam-rbt` region not covered by the read | all `rbt_` operators |
| rbt region not valid                            | `sam_rbt` operands that are not integers 1 <= x <= y    | all `rbt_` operators |

Operators that need no alignment work for every record, unmapped ones included:
`isforward`, `sam_get`, `aln_nedit` (NA only if NM:i is present but not a non-negative
integer), `cgsum` `cgmax` `cgcount` (NA only for a CIGAR that does not parse).

Three situations end the run or skip the row instead of giving NA:

- An RNAME that is not in the reference fasta of `--sam/FILE` ends the run with the line
  number (the fasta's not-found value is `__EXIT__`, as in pick). It is looked up only
  when an operator needs the reference.
- A SEQ `*` that cannot be filled from an earlier row of the read is skipped
  (`--sam-seqmiss=skip`, the default) or ends the run (`--sam-seqmiss=exit`); the skipped
  rows are counted per reason.
- With `--group`, the first row of each group is not output (not an NA, but rows that
  disappear; counted).

Outside the SAM operators, two more places produce NA or drop a value: `fastQ` with a
quality of another length than the sequence (NA, counted), and `sam_put` with a value
that is NA, contains a TAB or newline, or does not fit the tag's type (the tag is not
put, counted).


## 2. How an NA travels

Within one compute an NA is a value of its own:

- An operator with an NA operand gives NA (arithmetic, string and bio operators alike).
- `ifelse` and `test` with an NA condition give NA.
- A numeric operator given something that is not a number also gives NA.

When the compute is written, NA becomes the text `NA` in its column. Later computes that
read the column see that text, not an NA value:

```console
$ pluk -q --sam 1 n::,ref_trail3p_N l::n,len 'm::n^1,add' k::,ref_trail3p_N,len < mixed.sam
read1   3    1   4    1
read2   NA   2   NA   NA
```

For read2, `l` (the length of the text `NA`) is 2; `m` is NA because `NA` is not a number; `k`
computes the length inside one compute and is NA. Keep a chain that must stay NA-aware in
one compute, or test for it.

Filters:

- A numeric filter (`/gt/`, `/eq/`, `/ep/`, bit filters, ...) fails on NA, so the row is
  not kept; the number of values that were not numbers is reported:

  ```console
  $ pluk --sam 1 n::,ref_trail3p_N @n/gt/1 < mixed.sam
  read1   3
  pluk: @n/gt/1: 1 value not a number (row not kept)
  ```

  A negated numeric filter (`/ne/`) fails on NA as well: NA is not a number that differs.
- A string filter (`=`, `~eq~`, `~`, ...) compares the text `NA`.
- To keep or drop the rows with an NA explicitly: `@n=NA`, `@n/=NA`, or `/ISNUM`.

Output, `--other`, `--demux`: NA is written as `NA`. `--pstore` keeps the text `NA` for
the next row.


## 3. The most likely cascades

**Unmapped reads.** Aligner output usually contains unmapped records. Every alignment
operator gives NA for them, and a numeric filter on such a value drops them (with a
count). To leave them out from the start, filter on FLAG before the computes (bit 4,
unmapped, not set):

```console
$ pluk -q --sam/transgene.fa @@2/none/4 1 a::,aln_all < mixed.sam
read1    10        20        30           40        5
GCTTAACGGTACCATGAGCTAG---CTAAGGTTCCATGCAATTCG
||||||||||||x|||||||||~~~||||||||---|||||||||
GCTTAACGGTACAATGAGCTAGTTTCTAAGGTT---TGCAATTCG
```

**No reference length.** Without SAM header lines (`samtools view` without `-h`) and
without `--sam/FILE`, `ref_len` and `ref_trail3p_N` are NA for every row; with
`--sam-aln-mrk` the alignment strings are NA as well, and `--sam-aln-context` does not
extend the right side (counted). Use `--sam-h`/`samtools view -h`, or give the fasta.

**A reference that does not match.** If the fasta given with `--sam/FILE` is not the one
the reads were aligned to, alignments may run past its end: the reference parts and the
alignment strings become NA ('CIGAR past the reference end'), and so does everything
computed from them (`aln_posinfo`, `aln_nedit_pia`, `patchiness` of `aln_aln`, the rbt
values). pluk also reports when the fasta lengths differ from the `@SQ` lengths.

**SEQ and QUAL that do not fit.** A SEQ or QUAL whose length differs from what the CIGAR
says (a damaged record, or a fill gone wrong) makes the query sequence unavailable:
`qry_seq`, `qry_qual`, the query parts and all alignment-based values are NA for that
row, even those that would not need QUAL.

**Region queries.** `rbt_` values for a read that does not reach the region are NA;
filtering on `rbt_nedit` (for example `@e/le/2`) therefore drops such reads, which is
usually what is wanted (pick reports the region length as the edit count instead).
