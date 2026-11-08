#!/usr/bin/env python3
# Writes the hand-made BAM test files that samtools cannot produce from SAM text
# (plan/step6-notes.txt, item 2). Run once in pluk/test; the files are committed.
#   bam-nosq.bam     header text without @SQ lines; references only in the binary list
#   bam-sqdiff.bam   @SQ line whose length differs from the binary list
#   bam-cg.bam       a CIGAR stored in a CG:B:I tag (as for more than 65535 operations)
#   bam-empty.bam    header only, no records
#   ../../doc/reads.bam: doc/reads.sam as BAM, for the manual (plan/step30.txt decision 4)
# BGZF is written here as well, since bgzip may not be installed.

import struct
import zlib


def bgzf_block(data):
    c = zlib.compressobj(6, zlib.DEFLATED, -15)
    cdata = c.compress(data) + c.flush()
    bsize = 18 + len(cdata) + 8
    return (b'\x1f\x8b\x08\x04' + b'\0' * 4 + b'\0\xff' + struct.pack('<H', 6) + b'BC'
            + struct.pack('<HH', 2, bsize - 1) + cdata
            + struct.pack('<II', zlib.crc32(data) & 0xffffffff, len(data)))


def bgzf(data):
    blocks = [bgzf_block(data[i:i + 65280]) for i in range(0, len(data), 65280)]
    return b''.join(blocks) + bgzf_block(b'')        # empty block: BGZF end-of-file marker


def bam(text, refs, records):
    out = b'BAM\x01' + struct.pack('<i', len(text)) + text + struct.pack('<i', len(refs))
    for name, length in refs:
        name = name.encode() + b'\0'
        out += struct.pack('<i', len(name)) + name + struct.pack('<i', length)
    return out + b''.join(records)


CIGAR_CODES = {op: i for i, op in enumerate('MIDNSHP=X')}
BASE_CODES = {b: i for i, b in enumerate('=ACMGRSVTWYHKDBN')}


def record(name, flag, ref_id, pos, cigar, seq, tags=b''):
    """cigar: list of (length, op). pos is 1-based (0: none)."""
    name = name.encode() + b'\0'
    packed = bytearray()
    for i in range(0, len(seq), 2):
        hi = BASE_CODES[seq[i]]
        lo = BASE_CODES[seq[i + 1]] if i + 1 < len(seq) else 0
        packed.append(hi << 4 | lo)
    body = struct.pack('<iiBBHHHiiii', ref_id, pos - 1, len(name), 60, 4680, len(cigar), flag,
                       len(seq), -1, -1, 0)
    body += name + b''.join(struct.pack('<I', n << 4 | CIGAR_CODES[op]) for n, op in cigar)
    body += bytes(packed) + b'\xff' * len(seq) + tags
    return struct.pack('<i', len(body)) + body


def cg_tag(cigar):
    return b'CGBI' + struct.pack('<i', len(cigar)) + b''.join(
        struct.pack('<I', n << 4 | CIGAR_CODES[op]) for n, op in cigar)


def write(path, data):
    with open(path, 'wb') as f:
        f.write(bgzf(data))


refs = [('chr1', 1000), ('chr2', 500)]
r1 = record('r1', 0, 0, 5, [(4, 'M')], 'ACGT')
r2 = record('r2', 16, 1, 20, [(2, 'M')], 'GG')

write('bam-nosq.bam', bam(b'@HD\tVN:1.6\tSO:unsorted\n@CO\tno SQ lines\n', refs, [r1, r2]))
write('bam-sqdiff.bam', bam(b'@HD\tVN:1.6\n@SQ\tSN:chr1\tLN:1000\n@SQ\tSN:chr2\tLN:77\n', refs, [r1]))

# The real CIGAR 3M1I2M3D2M (8 bases, 10 reference positions) in CG; the stored
# CIGAR is the placeholder 8S10N. Another tag follows CG, to check that only CG
# is left out.
real = [(3, 'M'), (1, 'I'), (2, 'M'), (3, 'D'), (2, 'M')]
cg = record('r3', 0, 0, 7, [(8, 'S'), (10, 'N')], 'ACGTACGT', cg_tag(real) + b'NMC\x02')
write('bam-cg.bam', bam(b'@HD\tVN:1.6\n@SQ\tSN:chr1\tLN:1000\n@SQ\tSN:chr2\tLN:500\n', refs, [r1, cg]))

write('bam-empty.bam', bam(b'@HD\tVN:1.6\n@SQ\tSN:chr1\tLN:1000\n@SQ\tSN:chr2\tLN:500\n', refs, []))

# The manual's reads.sam as BAM (MAPQ 60, QUAL absent, NM:i:7 stored as an unsigned byte).
read1 = record('read1', 0, 0, 9, [(3, 'S'), (22, 'M'), (3, 'I'), (8, 'M'), (3, 'D'), (9, 'M')],
               'CCAGCTTAACGGTACAATGAGCTAGTTTCTAAGGTTTGCAATTCG', b'NMC\x07')
write('../../doc/reads.bam', bam(b'@SQ\tSN:transgene\tLN:53\n', [('transgene', 53)], [read1]))
