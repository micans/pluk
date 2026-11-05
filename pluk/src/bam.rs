// BAM input: BamReader presents a BAM stream as SAM text (plan/step6.txt item 2).
//
// BamReader implements BufRead. It produces the SAM header first, then one SAM
// line per record, so that LineReader reads it as it reads plain SAM text and
// everything downstream is unchanged. Decoding is done by noodles; the SAM text
// is formatted here, to match samtools view (float tags as C's %g).

use std::io::{self, BufRead, Read};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use noodles_bam as bam;
use noodles_sam::alignment::record::data::field::value::array::Array;
use noodles_sam::alignment::record::data::field::{Tag, Value};

use crate::io::parse_sq;

// BamReader refills its text buffer with records up to about this size.
const TEXT_BUFSIZE: usize = 128 * 1024;

// BAM 4-bit base codes and CIGAR operation codes.
const BASES: &[u8; 16] = b"=ACMGRSVTWYHKDBN";
const CIGAR_OPS: &[u8; 9] = b"MIDNSHP=X";

// The BGZF end-of-file marker: an empty block that ends every complete BGZF file.
const BGZF_EOF: [u8; 28] = [
    0x1f, 0x8b, 0x08, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0xff, 0x06, 0x00, 0x42, 0x43,
    0x02, 0x00, 0x1b, 0x00, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];

// A pass-through reader for the compressed stream that checks whether it ends
// with the BGZF end-of-file marker. noodles takes a stream that ends at a block
// boundary, or within the header of a block, for a complete one; the marker
// tells a complete file from a truncated one. The result is shared through an
// Arc, as the stream may be read by noodles' decompression threads (item 3).
pub struct EofCheck<R: Read> {
    inner: R,
    tail:  Vec<u8>,                 // the last bytes read, at most BGZF_EOF.len()
    seen:  Arc<AtomicBool>,         // the stream ended with the marker
}

impl<R: Read> EofCheck<R> {
    pub fn new(inner: R) -> (EofCheck<R>, Arc<AtomicBool>) {
        let seen = Arc::new(AtomicBool::new(false));
        (EofCheck { inner, tail: Vec::with_capacity(2 * BGZF_EOF.len()), seen: Arc::clone(&seen) }, seen)
    }
}

impl<R: Read> Read for EofCheck<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.read(buf)?;
        if n == 0 {
            self.seen.store(self.tail == BGZF_EOF, Ordering::Relaxed);
        } else {
            let keep = BGZF_EOF.len();
            let new = &buf[n.saturating_sub(keep)..n];
            self.tail.extend_from_slice(new);
            if self.tail.len() > keep {
                self.tail.drain(..self.tail.len() - keep);
            }
        }
        Ok(n)
    }
}

pub struct BamReader<R: Read> {
    reader:  bam::io::Reader<R>,
    record:  bam::Record,
    names:   Vec<Vec<u8>>,      // reference sequence names, by index (binary list)
    text:    Vec<u8>,           // SAM text not yet consumed: text[pos..]
    pos:     usize,
    n_records: u64,
    done:    bool,              // all records have been read
    eof_seen: Arc<AtomicBool>,  // set by EofCheck on the compressed stream
}

impl<R: Read> BamReader<R> {
    // decoder is a decompressed BAM stream (a BGZF reader). The header is read
    // here; it becomes the first text returned.
    // eof_seen comes from the EofCheck that wraps the compressed stream.
    pub fn new(decoder: R, eof_seen: Arc<AtomicBool>) -> io::Result<BamReader<R>> {
        let mut reader = bam::io::Reader::from(decoder);
        let (text, refs) = read_header(&mut reader).map_err(truncated)?;
        let header = header_text(text, &refs)?;
        Ok(BamReader {
            reader,
            record: bam::Record::default(),
            names: refs.into_iter().map(|(name, _)| name).collect(),
            text: header,
            pos: 0,
            n_records: 0,
            done: false,
            eof_seen,
        })
    }

    // Replace the consumed text with the SAM lines of the next records.
    fn refill(&mut self) -> io::Result<()> {
        self.text.clear();
        self.pos = 0;
        while self.text.len() < TEXT_BUFSIZE {
            if self.reader.read_record(&mut self.record).map_err(truncated)? == 0 {
                self.done = true;
                if !self.eof_seen.load(Ordering::Relaxed) {
                    return Err(io::Error::new(io::ErrorKind::UnexpectedEof,
                        "BAM input is truncated (no BGZF end-of-file marker)"));
                }
                break;
            }
            self.n_records += 1;
            format_record(&self.record, &self.names, &mut self.text).map_err(|e| {
                io::Error::new(io::ErrorKind::InvalidData, format!("BAM record {}: {}", self.n_records, e))
            })?;
        }
        Ok(())
    }
}

// A stream that ends within a block or a record gives UnexpectedEof from noodles
// ("failed to fill whole buffer"); say what it means.
fn truncated(e: io::Error) -> io::Error {
    if e.kind() == io::ErrorKind::UnexpectedEof {
        io::Error::new(io::ErrorKind::UnexpectedEof, format!("BAM input is truncated ({})", e))
    } else {
        e
    }
}

impl<R: Read> BufRead for BamReader<R> {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        if self.pos >= self.text.len() && !self.done {
            self.refill()?;
        }
        Ok(&self.text[self.pos..])
    }

    fn consume(&mut self, amt: usize) {
        self.pos = (self.pos + amt).min(self.text.len());
    }
}

impl<R: Read> Read for BamReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let available = self.fill_buf()?;
        let n = available.len().min(buf.len());
        buf[..n].copy_from_slice(&available[..n]);
        self.consume(n);
        Ok(n)
    }
}


// The header: magic bytes, header text (verbatim, NUL padding removed) and the
// binary reference list as (name, length) pairs.
fn read_header<R: Read>(reader: &mut bam::io::Reader<R>) -> io::Result<(Vec<u8>, Vec<(Vec<u8>, usize)>)> {
    let mut header_reader = reader.header_reader();
    if header_reader.read_magic_number()? != *b"BAM\x01" {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "invalid BAM magic number"));
    }
    let mut text = Vec::new();
    {
        let mut text_reader = header_reader.raw_sam_header_reader()?;
        text_reader.read_to_end(&mut text)?;
        text_reader.discard_to_end()?;
    }
    let refs = header_reader
        .read_reference_sequences()?
        .into_iter()
        .map(|(name, map)| (name.to_vec(), map.length().get()))
        .collect();
    Ok((text, refs))
}

// The SAM header to output (plan/step6.txt item 2). The text is kept as it is,
// with a final newline added if missing. Without @SQ lines in the text, @SQ lines
// are generated from the binary list and appended, as samtools does. @SQ lines
// that differ from the binary list are an error.
fn header_text(mut text: Vec<u8>, refs: &[(Vec<u8>, usize)]) -> io::Result<Vec<u8>> {
    if !text.is_empty() && text.last() != Some(&b'\n') {
        text.push(b'\n');
    }
    let mut text_refs = Vec::new();
    for (i, line) in text.split(|&b| b == b'\n').enumerate() {
        if line.starts_with(b"@SQ\t") {
            match parse_sq(line) {
                Some((name, length)) => text_refs.push((name, length as usize)),
                None => return Err(header_error(format!(
                    "BAM header @SQ line {} has no SN: or valid LN: field", i + 1))),
            }
        }
    }
    if text_refs.is_empty() {
        for (name, length) in refs {
            text.extend_from_slice(b"@SQ\tSN:");
            text.extend_from_slice(name);
            text.extend_from_slice(format!("\tLN:{}\n", length).as_bytes());
        }
    } else if let Some(difference) = sq_difference(&text_refs, refs) {
        return Err(header_error(difference));
    }
    Ok(text)
}

fn header_error(msg: String) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg)
}

// The first difference between the @SQ lines of the header text and the binary
// reference list, if any. Order is not compared: records refer to the binary
// list, and the names identify the sequences.
fn sq_difference(text_refs: &[(Vec<u8>, usize)], refs: &[(Vec<u8>, usize)]) -> Option<String> {
    let show = |name: &[u8]| String::from_utf8_lossy(name).into_owned();
    if text_refs.len() != refs.len() {
        return Some(format!("BAM header has {} @SQ lines and {} reference sequences in its binary list",
                            text_refs.len(), refs.len()));
    }
    for (name, length) in refs {
        match text_refs.iter().find(|(n, _)| n == name) {
            None => return Some(format!(
                "BAM reference sequence {} (binary list) has no @SQ line in the header", show(name))),
            Some((_, l)) if l != length => return Some(format!(
                "BAM reference sequence {} has length {} in its @SQ line and {} in the binary list",
                show(name), l, length)),
            Some(_) => {}
        }
    }
    None
}


// Append one record as a SAM line (with newline) to out.
pub fn format_record(record: &bam::Record, names: &[Vec<u8>], out: &mut Vec<u8>) -> io::Result<()> {
    // QNAME
    match record.name() {
        Some(name) => out.extend_from_slice(name),
        None => out.push(b'*'),
    }
    out.push(b'\t');

    // FLAG
    push_number(out, record.flags().bits());
    out.push(b'\t');

    // RNAME
    let reference_id = record.reference_sequence_id().transpose()?;
    push_reference(out, reference_id, names)?;
    out.push(b'\t');

    // POS (1-based; 0 if absent)
    push_number(out, record.alignment_start().transpose()?.map_or(0, |p| p.get()));
    out.push(b'\t');

    // MAPQ (255 if absent)
    push_number(out, record.mapping_quality().map_or(255, u8::from));
    out.push(b'\t');

    // CIGAR. noodles substitutes the CG tag of a long CIGAR (more than 65535
    // operations); see push_tags.
    let cigar = record.cigar();
    if cigar.is_empty() {
        out.push(b'*');
    } else {
        for op in cigar.as_bytes().chunks_exact(4) {
            let n = u32::from_le_bytes([op[0], op[1], op[2], op[3]]);
            push_number(out, n >> 4);
            match CIGAR_OPS.get((n & 0xf) as usize) {
                Some(&c) => out.push(c),
                None => return Err(invalid(format!("invalid CIGAR operation code {}", n & 0xf))),
            }
        }
    }
    out.push(b'\t');

    // RNEXT: '=' when equal to RNAME
    let mate_id = record.mate_reference_sequence_id().transpose()?;
    if mate_id.is_some() && mate_id == reference_id {
        out.push(b'=');
    } else {
        push_reference(out, mate_id, names)?;
    }
    out.push(b'\t');

    // PNEXT, TLEN
    push_number(out, record.mate_alignment_start().transpose()?.map_or(0, |p| p.get()));
    out.push(b'\t');
    push_number(out, record.template_length());
    out.push(b'\t');

    // SEQ: two bases per byte, high nibble first
    let sequence = record.sequence();
    let n_bases = sequence.len();
    if n_bases == 0 {
        out.push(b'*');
    } else {
        for (i, &b) in sequence.as_bytes().iter().enumerate() {
            out.push(BASES[(b >> 4) as usize]);
            if 2 * i + 1 < n_bases {
                out.push(BASES[(b & 0xf) as usize]);
            }
        }
    }
    out.push(b'\t');

    // QUAL: Phred + 33; '*' if absent (first byte 0xFF, as htslib)
    let quality = record.quality_scores().as_bytes();
    if quality.is_empty() || quality[0] == 0xff {
        out.push(b'*');
    } else {
        out.extend(quality.iter().map(|&q| q.saturating_add(33)));
    }

    push_tags(record, out)?;
    out.push(b'\n');
    Ok(())
}

fn invalid(msg: String) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg)
}

fn push_number<T: std::fmt::Display>(out: &mut Vec<u8>, n: T) {
    use std::io::Write;
    let _ = write!(out, "{}", n);     // writing to a Vec cannot fail
}

fn push_reference(out: &mut Vec<u8>, id: Option<usize>, names: &[Vec<u8>]) -> io::Result<()> {
    match id {
        None => out.push(b'*'),
        Some(i) => match names.get(i) {
            Some(name) => out.extend_from_slice(name),
            None => return Err(invalid(format!("reference sequence index {} not in the header", i))),
        },
    }
    Ok(())
}

// The optional fields, each as TAB TAG:TYPE:VALUE, in record order. All integer
// types are written as i, as samtools does. A CG tag that noodles used as the
// CIGAR is left out, as samtools does.
fn push_tags(record: &bam::Record, out: &mut Vec<u8>) -> io::Result<()> {
    let cigar = record.cigar();
    for field in record.data().iter() {
        let (tag, value) = field?;
        if tag == Tag::CIGAR && is_cigar_source(&value, cigar.as_bytes()) {
            continue;
        }
        out.push(b'\t');
        out.extend_from_slice(tag.as_ref());
        match value {
            Value::Character(c) => { out.extend_from_slice(b":A:"); out.push(c); }
            Value::Int8(n)   => { out.extend_from_slice(b":i:"); push_number(out, n); }
            Value::UInt8(n)  => { out.extend_from_slice(b":i:"); push_number(out, n); }
            Value::Int16(n)  => { out.extend_from_slice(b":i:"); push_number(out, n); }
            Value::UInt16(n) => { out.extend_from_slice(b":i:"); push_number(out, n); }
            Value::Int32(n)  => { out.extend_from_slice(b":i:"); push_number(out, n); }
            Value::UInt32(n) => { out.extend_from_slice(b":i:"); push_number(out, n); }
            Value::Float(x)  => { out.extend_from_slice(b":f:"); push_g(out, x); }
            Value::String(s) => { out.extend_from_slice(b":Z:"); out.extend_from_slice(s); }
            Value::Hex(s)    => { out.extend_from_slice(b":H:"); out.extend_from_slice(s); }
            Value::Array(array) => {
                out.extend_from_slice(b":B:");
                push_array(out, array)?;
            }
        }
    }
    Ok(())
}

// Whether a CG:B:I value holds the operations noodles returned as the CIGAR.
fn is_cigar_source(value: &Value, cigar: &[u8]) -> bool {
    let Value::Array(Array::UInt32(values)) = value else {
        return false;
    };
    if values.len() * 4 != cigar.len() {
        return false;
    }
    values.iter().zip(cigar.chunks_exact(4)).all(|(v, op)| {
        matches!(v, Ok(v) if v.to_le_bytes() == [op[0], op[1], op[2], op[3]])
    })
}

fn push_array(out: &mut Vec<u8>, array: Array) -> io::Result<()> {
    match array {
        Array::Int8(values)   => push_values(out, b'c', values.iter(), push_number),
        Array::UInt8(values)  => push_values(out, b'C', values.iter(), push_number),
        Array::Int16(values)  => push_values(out, b's', values.iter(), push_number),
        Array::UInt16(values) => push_values(out, b'S', values.iter(), push_number),
        Array::Int32(values)  => push_values(out, b'i', values.iter(), push_number),
        Array::UInt32(values) => push_values(out, b'I', values.iter(), push_number),
        Array::Float(values)  => push_values(out, b'f', values.iter(), push_g),
    }
}

// An array subtype followed by its values, each preceded by a comma.
fn push_values<T>(out: &mut Vec<u8>, subtype: u8, values: impl Iterator<Item = io::Result<T>>,
                  push: fn(&mut Vec<u8>, T)) -> io::Result<()> {
    out.push(subtype);
    for v in values {
        out.push(b',');
        push(out, v?);
    }
    Ok(())
}

// A float as C's printf %g (precision 6), as samtools writes float tags.
// %g uses the exponent X of the value rounded to 6 significant digits: with
// -4 <= X < 6 fixed notation with 5 - X decimals, otherwise e-notation with 5
// decimals and an exponent of at least two digits; trailing zeros (and a
// trailing '.') are removed. Rust rounds ties to even, as glibc does.
pub fn push_g(out: &mut Vec<u8>, x: f32) {
    let x = x as f64;
    if x.is_nan() {
        out.extend_from_slice(if x.is_sign_negative() { b"-nan" } else { b"nan" });
        return;
    }
    if x.is_infinite() {
        out.extend_from_slice(if x < 0.0 { b"-inf" } else { b"inf" });
        return;
    }
    if x == 0.0 {
        out.extend_from_slice(if x.is_sign_negative() { b"-0" } else { b"0" });
        return;
    }
    let e_form = format!("{:.5e}", x);          // e.g. 1.23457e-5
    let (mantissa, exponent) = e_form.split_once('e').unwrap();
    let exponent: i32 = exponent.parse().unwrap();
    if (-4..6).contains(&exponent) {
        let fixed = format!("{:.*}", (5 - exponent) as usize, x);
        out.extend_from_slice(strip_zeros(&fixed).as_bytes());
    } else {
        out.extend_from_slice(strip_zeros(mantissa).as_bytes());
        out.push(b'e');
        out.push(if exponent < 0 { b'-' } else { b'+' });
        out.extend_from_slice(format!("{:02}", exponent.abs()).as_bytes());
    }
}

// Remove trailing zeros after a decimal point, and the point if nothing follows.
fn strip_zeros(s: &str) -> &str {
    if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.')
    } else {
        s
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    fn g(x: f32) -> String {
        let mut out = Vec::new();
        push_g(&mut out, x);
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn printf_g() {
        // Expected values from C's printf("%g") on the same f32 values (via samtools).
        assert_eq!(g(1234565.0), "1.23456e+06");
        assert_eq!(g(0.0), "0");
        assert_eq!(g(-0.0), "-0");
        assert_eq!(g(100000.0), "100000");
        assert_eq!(g(1000000.0), "1e+06");
        assert_eq!(g(0.0001), "0.0001");
        assert_eq!(g(0.00001234), "1.234e-05");
        assert_eq!(g(123456789.0), "1.23457e+08");
        assert_eq!(g(f32::NAN), "nan");
        assert_eq!(g(f32::INFINITY), "inf");
        assert_eq!(g(f32::NEG_INFINITY), "-inf");
        assert_eq!(g(0.1), "0.1");
        assert_eq!(g(2.5), "2.5");
        assert_eq!(g(3.4028235e38), "3.40282e+38");
        assert_eq!(g(1e-45), "1.4013e-45");
        assert_eq!(g(3.14159265), "3.14159");
        assert_eq!(g(-999999.5), "-1e+06");
        assert_eq!(g(1e20), "1e+20");
    }

    // Read all of data through EofCheck, chunk bytes at a time.
    fn eof_seen(data: &[u8], chunk: usize) -> bool {
        let (mut check, seen) = EofCheck::new(data);
        let mut buf = vec![0u8; chunk];
        while check.read(&mut buf).unwrap() > 0 {}
        seen.load(Ordering::Relaxed)
    }

    #[test]
    fn eof_marker() {
        let mut data = vec![7u8; 100];
        data.extend_from_slice(&BGZF_EOF);
        for chunk in [1, 5, 28, 29, 1000] {
            assert!(eof_seen(&data, chunk));
            assert!(eof_seen(&BGZF_EOF, chunk));
            assert!(!eof_seen(&data[..data.len() - 1], chunk));
            assert!(!eof_seen(&data[..100], chunk));
            assert!(!eof_seen(b"", chunk));
        }
    }

    #[test]
    fn header_sq() {
        let refs = vec![(b"chr1".to_vec(), 1000), (b"chr2".to_vec(), 500)];
        let text = b"@HD\tVN:1.6\n@CO\tc".to_vec();
        assert_eq!(header_text(text, &refs).unwrap(),
                   b"@HD\tVN:1.6\n@CO\tc\n@SQ\tSN:chr1\tLN:1000\n@SQ\tSN:chr2\tLN:500\n");
        let text = b"@SQ\tSN:chr2\tLN:500\n@SQ\tSN:chr1\tLN:1000\n".to_vec();
        assert_eq!(header_text(text.clone(), &refs).unwrap(), text);
        assert!(header_text(b"@SQ\tSN:chr1\tLN:1000\n".to_vec(), &refs).is_err());
        assert!(header_text(b"@SQ\tSN:chr1\tLN:1000\n@SQ\tSN:chr2\tLN:5\n".to_vec(), &refs).is_err());
        assert!(header_text(b"@SQ\tSN:chr1\tLN:1000\n@SQ\tSN:chrX\tLN:500\n".to_vec(), &refs).is_err());
        assert!(header_text(b"@SQ\tSN:chr1\n".to_vec(), &refs).is_err());
        assert_eq!(header_text(Vec::new(), &[]).unwrap(), b"");
    }
}
