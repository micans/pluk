// The row pipeline (plan/COMPUTE.design.txt section 1, SAMVIEW.design.txt section 2):
// for each data row the joins, the fill, the @@ stage, the SAM rewrites, the computes, the kept
// state of --pstore and --group, the @ stage; and where a row goes when it is not kept
// (--rejected, --assert-failed, --demux).

use std::io::ErrorKind;

use crate::compute;
use crate::demux;
use crate::exec;
use crate::filter;
use crate::join;
use crate::note;
use crate::row;
use crate::samrows;
use crate::sink;
use crate::state;

// What happens to a data row before output: the @@ stage, the computes, the @
// stage (plan/COMPUTE.design.txt section 1).
pub struct Stages<'d> {
    pub joins:    join::Joins,              // --join, --join-inner (plan/JOIN.design.txt): first of all
    pub filters:  filter::Filters<'d>,
    pub computes: Vec<compute::Compute>,
    pub machine:  exec::Machine<'d>,
    pub rowno:    u64,                      // data rows seen (rowno, r0wno); rejected rows count too
    pub rewrites: samrows::Rewrites,        // the SAM input rewrites (SAMVIEW.design.txt section 3)
    pub pstore:   Option<state::Pstore>,    // --pstore, --group (plan/step23.txt)
    pub fill:     Option<samrows::Fill>,    // the sequence fill, when an operator needs the query
    pub put:      Option<samrows::Put>,     // sam_put, when a compute uses it (plan/step25.txt)
}

impl Stages<'_> {
    // The computed fields of the previous row are removed first, so that a row
    // rejected by the @@ stage has none (they read as empty).
    fn judge(&mut self, row: &mut row::Row, lineno: u64) -> Result<filter::Verdict, Failure> {
        row.clear_computed();
        self.rowno += 1;
        // The joins come first: their columns are input columns from here on. A row
        // that an inner join does not find is rejected as by an @@ filter
        // (plan/JOIN.design.txt section 6).
        if !self.joins.apply(row) {
            return Ok(filter::Verdict::RejectPre);
        }
        // The fill comes before every filter (SAMVIEW.design.txt section 2).
        if let Some(fill) = &mut self.fill {
            match fill.process(row, lineno).map_err(Failure::Fatal)? {
                Some(query) => self.machine.sam.query = query,
                None => return Err(Failure::Skipped),
            }
        }
        if !self.filters.pre(row).map_err(Failure::Assertion)? {
            return Ok(filter::Verdict::RejectPre);
        }
        // SAM input rewrites: after the @@ stage, before the computes.
        self.rewrites.apply(row, &mut self.machine.sam);
        if !self.computes.is_empty() {
            self.machine.run(&self.computes, row, self.rowno, lineno).map_err(Failure::Fatal)?;
        }
        // sam_put: the tags into column 12, after the computes (plan/step25.txt decision 0).
        if let Some(put) = &mut self.put {
            put.apply(row, &mut self.machine.sam_put, lineno);
        }
        // --pstore: kept after the computes, before the @ stage (plan/step23.txt decision 0);
        // with --group the first row of a group is skipped there, as the fill's skip.
        if let Some(p) = &mut self.pstore {
            if p.keep(row, &mut self.machine) {
                return Err(Failure::Skipped);
            }
        }
        let kept = self.filters.post(row).map_err(Failure::Assertion)?;
        Ok(if kept { filter::Verdict::Keep } else { filter::Verdict::RejectPost })
    }

    // The end-of-run counts of the stages (silenced by -q).
    pub fn report(&self) {
        self.joins.report();
        self.filters.report_na();
        self.machine.sam.report();
        let n = self.machine.n_quality_mismatch;
        if n > 0 {
            note::note!("pluk: fastQ: quality of another length than the sequence (NA): {} value{}", n,
                        if n == 1 { "" } else { "s" });
        }
        if let Some(n) = self.pstore.as_ref().and_then(|p| p.groups()).filter(|&n| n > 0) {
            note::note!("pluk: --group: {} group{} (the first row of each not output)", n, if n == 1 { "" } else { "s" });
        }
        self.rewrites.report();
        if let Some(put) = &self.put {
            put.report(self.machine.put_unusable, self.machine.put_wrong_type);
        }
        if let Some(fill) = &self.fill {
            fill.report();
        }
    }
}

// Why a row was not judged: an assertion failed (diverted with --assert-failed), an
// operator failed (map with __EXIT__), which ends the run, or the row is skipped.
enum Failure {
    Assertion(String),
    Fatal(String),
    Skipped,        // a SEQ '*' that cannot be filled (--sam-seqmiss=skip), the first row of a group
}

// Whether the current data row is kept. A row rejected by a filter is written to
// --rejected, and if rejected by an @ filter also counted by --demux (which opens
// its file); a row that fails an assertion is written to --assert-failed, or else
// ends the run with an error giving the line number.
pub fn route(stages: &mut Stages, row: &mut row::Row, lineno: u64,
         other: &mut Option<sink::SideFile>, assert_other: &mut Option<sink::SideFile>,
         demux: &mut Option<demux::Demux>)
    -> std::io::Result<bool>
{
    match stages.judge(row, lineno) {
        Ok(filter::Verdict::Keep) => Ok(true),
        Ok(verdict) => {
            if let Some(file) = other {
                file.write_row(row)?;
            }
            if let (filter::Verdict::RejectPost, Some(d)) = (verdict, demux) {
                d.filtered(row, lineno)?;
            }
            Ok(false)
        }
        Err(Failure::Assertion(msg)) => match assert_other {
            Some(file) => {
                file.write_row(row)?;
                Ok(false)
            }
            None => Err(std::io::Error::new(ErrorKind::InvalidData, format!("{} at line {}", msg, lineno))),
        },
        Err(Failure::Fatal(msg)) => Err(std::io::Error::new(ErrorKind::InvalidData, format!("{} at line {}", msg, lineno))),
        // Not a filter rejection: not written to --rejected (SAMSEQ.design.txt section 5,
        // plan/step23.txt decision 3).
        Err(Failure::Skipped) => Ok(false),
    }
}
