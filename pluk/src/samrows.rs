// SAM processing per row (SAMVIEW.design.txt section 2): the sequence fill before
// the filters, and the input rewrites after the @@ stage. The SAM view itself, which
// the operators read, is in sam.rs; the fill's cache in samseq.rs.

use crate::dict;
use crate::note;
use crate::row;
use crate::sam;
use crate::samseq;

// The input rewrites of SAM records, after the @@ stage and before the computes, in
// pick's order (SAMVIEW.design.txt section 3): ND-adjust, narrowing, normal form,
// context extension.
pub struct Rewrites {
    pub normal_form: bool,              // SAM input without --sam-nonf
    pub nd_adjust:   Option<NdAdjust>,  // --sam-ND-adjust (plan/step19.txt decision 5)
    pub narrow:      Option<Narrow>,    // --sam-narrow-xy (plan/step19.txt decision 4)
    pub extension:   Option<Extend>,    // --sam-aln-context, --sam-aln-xy (plan/step19.txt decision 2)
}

impl Rewrites {
    pub fn new(normal_form: bool, nd_adjust: Option<NdAdjust>, narrow_xy: Option<(u64, u64)>,
               context: Option<sam::ContextOptions>) -> Rewrites {
        Rewrites {
            normal_form,
            nd_adjust,
            narrow: narrow_xy.map(|xy| Narrow { xy, n_outside: 0 }),
            extension: context.map(|options| Extend { options, n_no_reflen: 0 }),
        }
    }

    // Apply the rewrites to a row; the extension records its context in the view.
    pub fn apply(&mut self, row: &mut row::Row, view: &mut sam::View) {
        if let Some(nd) = &mut self.nd_adjust {
            if row.n_fields() > sam::CIGAR && !sam::is_unmapped(row) {
                nd.apply(row);
            }
        }
        // Narrowing: mapped records only; a record outside the region gets FLAG 0x4
        // and CIGAR '*' (RNAME and POS kept).
        if let Some(nar) = &mut self.narrow {
            if row.n_fields() > sam::CIGAR && !sam::is_unmapped(row) {
                if let Some(pos) = sam::number(row.field(sam::POS)) {
                    match sam::narrow(pos, row.field(sam::CIGAR), nar.xy.0, nar.xy.1) {
                        sam::Narrowed::Unchanged => {}
                        sam::Narrowed::Changed { pos, cigar } => {
                            row.replace_field(sam::POS, pos.to_string().as_bytes());
                            row.replace_field(sam::CIGAR, &cigar);
                        }
                        sam::Narrowed::Outside => {
                            nar.n_outside += 1;
                            if let Some(flag) = sam::flag(row) {
                                row.replace_field(sam::FLAG, (flag | 0x4).to_string().as_bytes());
                            }
                            row.replace_field(sam::CIGAR, b"*");
                        }
                    }
                }
            }
        }
        // Normal form: as pick, every record, unmapped ones included (CIGAR '*' never
        // changes).
        if self.normal_form && row.n_fields() > sam::CIGAR {
            if let Some((pos, cigar)) = sam::normal_form(row.field(sam::POS), row.field(sam::CIGAR)) {
                row.replace_field(sam::POS, &pos);
                row.replace_field(sam::CIGAR, &cigar);
            }
        }
        // Context extension: mapped records only.
        if let Some(ext) = &mut self.extension {
            view.context = (0, 0);
            if row.n_fields() > sam::CIGAR && !sam::is_unmapped(row) {
                if let Some(pos) = sam::number(row.field(sam::POS)) {
                    let reflen = view.reference_length(row);
                    let (extension, no_reflen) = sam::extend(pos, row.field(sam::CIGAR), reflen, &ext.options);
                    ext.n_no_reflen += u64::from(no_reflen);
                    if let Some(e) = extension {
                        row.replace_field(sam::POS, e.pos.to_string().as_bytes());
                        row.replace_field(sam::CIGAR, &e.cigar);
                        view.context = e.context;
                    }
                }
            }
        }
    }

    // The end-of-run counts (silenced by -q).
    pub fn report(&self) {
        if let Some(nd) = self.nd_adjust.as_ref().filter(|n| n.n_rows > 0) {
            note::note!("pluk: --sam-ND-adjust: N turned into D in {} row{} ({} bases)", nd.n_rows,
                        if nd.n_rows == 1 { "" } else { "s" }, nd.n_switched);
        }
        if let Some(nar) = self.narrow.as_ref().filter(|n| n.n_outside > 0) {
            note::note!("pluk: alignment outside the --sam-narrow-xy region, made unmapped (FLAG 0x4, CIGAR '*'): {} row{}",
                        nar.n_outside, if nar.n_outside == 1 { "" } else { "s" });
        }
        if let Some(ext) = self.extension.as_ref().filter(|e| e.n_no_reflen > 0) {
            note::note!("pluk: alignment not extended at the right (reference length not known): {} row{}",
                        ext.n_no_reflen, if ext.n_no_reflen == 1 { "" } else { "s" });
        }
    }
}

// --sam-ND-adjust: the intervals per reference, and what was changed.
pub struct NdAdjust {
    intervals:  std::collections::HashMap<Vec<u8>, Vec<(u64, u64)>>,
    n_rows:     u64,
    n_switched: u64,
}

impl NdAdjust {
    // The dictionary must exist; each value is a list of intervals.
    pub fn new(dicts: &dict::Dicts, name: &[u8]) -> Result<NdAdjust, String> {
        let d = dicts.get(name).ok_or("no dictionary of that name")?;
        let mut intervals = std::collections::HashMap::new();
        for (key, value) in &d.map {
            let list = sam::parse_intervals(value)
                .map_err(|e| format!("key [{}]: {}", String::from_utf8_lossy(key), e))?;
            intervals.insert(key.clone(), list);
        }
        Ok(NdAdjust { intervals, n_rows: 0, n_switched: 0 })
    }

    // Turn the N operations outside the intervals of RNAME into D; NM:i follows.
    pub fn apply(&mut self, row: &mut row::Row) {
        let Some(list) = self.intervals.get(row.field(sam::RNAME)) else { return };
        let Some(pos) = sam::number(row.field(sam::POS)) else { return };
        let Some((cigar, switched)) = sam::nd_adjust(pos, row.field(sam::CIGAR), list) else { return };
        row.replace_field(sam::CIGAR, &cigar);
        let nm = (sam::OPTIONAL..row.n_fields()).find_map(|i| sam::bump_nm(row.field(i), switched).map(|f| (i, f)));
        if let Some((i, field)) = nm {
            row.replace_field(i, &field);
        }
        self.n_rows += 1;
        self.n_switched += switched;
    }
}

// Narrowing: the region, and the records outside it (made unmapped).
pub struct Narrow {
    xy:        (u64, u64),
    n_outside: u64,
}

// The context extension: the options, and the records whose right side could not be
// extended because the reference length is not known.
pub struct Extend {
    options:     sam::ContextOptions,
    n_no_reflen: u64,
}

// sam_put (plan/step25.txt): the tags of a row put into column 12 after the computes;
// the tags that replaced one of the input, with a diagnostic for the first few rows.
pub struct Put {
    n_replaced: u64,        // rows in which an input tag was replaced
}

const PUT_MESSAGES: u64 = 5;

impl Put {
    pub fn new() -> Put {
        Put { n_replaced: 0 }
    }

    // Apply the row's entries (taken from the machine's buffer) to column 12.
    pub fn apply(&mut self, row: &mut row::Row, entries: &mut Vec<Vec<u8>>, lineno: u64) {
        if entries.is_empty() || row.n_fields() <= sam::OPTIONAL {
            return;
        }
        let (column, replaced) = sam::put_tags(row.field(sam::OPTIONAL), entries);
        row.replace_field(sam::OPTIONAL, &column);
        entries.clear();
        if replaced > 0 {
            self.n_replaced += 1;
            if self.n_replaced <= PUT_MESSAGES {
                note::note!("pluk: line {}: sam_put replaced a tag of read {}", lineno,
                            String::from_utf8_lossy(row.field(0)));
            }
        }
    }

    pub fn report(&self, unusable: u64, wrong_type: u64) {
        let s = |n: u64| if n == 1 { "" } else { "s" };
        if self.n_replaced > 0 {
            note::note!("pluk: sam_put replaced a tag of the input: {} row{}", self.n_replaced, s(self.n_replaced));
        }
        if unusable > 0 {
            note::note!("pluk: sam_put: tag not put (value NA, or with a TAB or newline): {} time{}", unusable, s(unusable));
        }
        if wrong_type > 0 {
            note::note!("pluk: sam_put: tag not put (value not of the tag's type): {} time{}", wrong_type, s(wrong_type));
        }
    }
}

// Why a SEQ '*' could not be filled (SAMSEQ.design.txt sections 2-5).
const FILL_REASONS: [&str; 4] = [
    "segment ambiguous (FLAG 0x1 with 0x40 and 0x80 both set or both unset)",
    "CIGAR '*' or not valid, so the part of the read is not known",
    "the stored sequences of the read have another length",
    "no earlier row of the read holds the sequence",
];
const FILL_MESSAGES: u64 = 5;   // per-row messages, then only the counts

// The sequence fill: the cache of the current read, the mode and the counts.
pub struct Fill {
    cache:  samseq::GroupCache,
    exit:   bool,               // --sam-seqmiss=exit
    filled: u64,
    missed: [u64; 4],           // per reason in FILL_REASONS
}

impl Fill {
    pub fn new(exit: bool) -> Fill {
        Fill { cache: samseq::GroupCache::new(), exit, filled: 0, missed: [0; 4] }
    }

    // The query for this row, or why the row cannot go on: Ok(None) skip, Err exit.
    pub fn process(&mut self, row: &row::Row, lineno: u64) -> Result<Option<sam::Query>, String> {
        let flag = sam::flag(row).and_then(|f| u16::try_from(f).ok()).unwrap_or(0);
        let outcome = self.cache.process(row.field(0), flag, row.field(sam::CIGAR), row.field(sam::SEQ),
                                         row.field(sam::QUAL));
        let reason = match outcome {
            samseq::Fill::Present | samseq::Fill::NotSource => return Ok(Some(sam::Query::Row)),
            samseq::Fill::LengthMismatch => return Ok(Some(sam::Query::Mismatch)),
            samseq::Fill::Filled { seq, qual } => {
                self.filled += 1;
                return Ok(Some(sam::Query::Filled { seq, qual }));
            }
            samseq::Fill::Ambiguous => 0,
            samseq::Fill::NoCigar => 1,
            samseq::Fill::LengthConflict { .. } => 2,
            samseq::Fill::NotFound { .. } => 3,
        };
        let qname = String::from_utf8_lossy(row.field(0));
        // An ambiguous segment is skipped whatever the mode (SAMSEQ.design.txt section 2).
        if self.exit && reason != 0 {
            return Err(format!("SEQ '*' of read {} cannot be filled: {} (--sam-seqmiss=exit)",
                               qname, FILL_REASONS[reason]));
        }
        self.missed[reason] += 1;
        if self.missed.iter().sum::<u64>() <= FILL_MESSAGES {
            note::note!("pluk: line {}: SEQ '*' of read {} cannot be filled: {}; row skipped",
                        lineno, qname, FILL_REASONS[reason]);
        }
        Ok(None)
    }

    pub fn report(&self) {
        if self.filled > 0 {
            note::note!("pluk: SEQ '*' filled from another row of the read: {} row{}", self.filled,
                        if self.filled == 1 { "" } else { "s" });
        }
        for (n, reason) in self.missed.iter().zip(FILL_REASONS) {
            if *n > 0 {
                note::note!("pluk: SEQ '*' not filled, row skipped: {} row{}: {}", n, if *n == 1 { "" } else { "s" },
                            reason);
            }
        }
    }
}
