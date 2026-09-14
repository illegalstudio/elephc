//! Purpose:
//! Preserves SJIS-mac composition state across MIME trials and output-handler feeds.
//!
//! Called from:
//! - MIME destination encoding and shared output conversion over bounded decoder chunks.
//!
//! Key details:
//! - Apple hint sequences consume mismatching characters as individually counted substitutions.
//! - PHP cannot resume its first hint-table record because its packed record index is zero.

use super::{doublebyte::DoubleByte, mapping::word, Substitute};

/// A registered composition and its canonical encoded bytes.
struct Composition { points: Vec<u32>, bytes: &'static [u8] }

/// Carries an unresolved first scalar or an incomplete Apple hint sequence between calls.
enum Pending { Scalar(u32), Hint(usize, usize) }

/// Replays exact chunk boundaries with fixed MIME replacement and an optional separate final call.
pub(super) fn encode_prefix(codec: DoubleByte, chunks: &[Vec<u32>], finish: bool) -> Vec<u8> {
    encode_with_final_policy(codec, chunks, Substitute::default(), finish, true)
}

/// Uses the live output replacement policy and marks the last actual decoder batch as final.
pub(super) fn encode_output_chunks(codec: DoubleByte, chunks: &[Vec<u32>], substitute: Substitute, finish: bool) -> Vec<u8> {
    encode_with_final_policy(codec, chunks, substitute, finish, false)
}

/// Shares composition carry while distinguishing output feeds from MIME's separate flush call.
fn encode_with_final_policy(codec: DoubleByte, chunks: &[Vec<u32>], substitute: Substitute, finish: bool, separate_final: bool) -> Vec<u8> {
    let mut compositions = Vec::new();
    let mut offset = 0;
    while offset < codec.composites.len() {
        let count = word(codec.composites, offset) as usize;
        let end = offset + 4 + count * 4;
        let length = word(codec.composites, end) as usize;
        compositions.push(Composition {
            points: (0..count).map(|index| word(codec.composites, offset + 4 + index * 4)).collect(),
            bytes: &codec.composites[end + 4..end + 4 + length],
        });
        offset = end + 4 + length;
    }
    let first_hint = compositions.iter().position(|entry| hint(entry.points[0]));
    let (mut pending, mut output) = (None, Vec::new());
    for (call, chunk) in chunks.iter().map(Vec::as_slice).chain((finish && separate_final).then_some(&[][..])).enumerate() {
        let end = finish && if separate_final { call == chunks.len() } else { call + 1 == chunks.len() };
        if let Some(Pending::Hint(index, _)) = pending {
            if Some(index) == first_hint { pending = Some(Pending::Scalar(compositions[index].points[0])); }
        }
        let mut input = chunk.iter().copied().peekable();
        loop {
            if let Some(Pending::Hint(index, mut matched)) = pending {
                pending = None;
                let entry = &compositions[index];
                let mut failed = false;
                while matched < entry.points.len() {
                    let Some(point) = input.next() else {
                        if end { append_rejected(codec, &mut output, &entry.points[..matched], substitute); }
                        else { pending = Some(Pending::Hint(index, matched)); }
                        failed = true;
                        break;
                    };
                    if point != entry.points[matched] {
                        append_rejected(codec, &mut output, &entry.points[..matched], substitute);
                        append_rejected(codec, &mut output, &[point], substitute);
                        failed = true;
                        break;
                    }
                    matched += 1;
                }
                if !failed { output.extend_from_slice(entry.bytes); }
                if pending.is_some() { break; }
                continue;
            }
            let point = if let Some(Pending::Scalar(point)) = pending.take() { point }
                else if let Some(point) = input.next() { point } else { break; };
            if codec.composite_starts(point) || hint(point) {
                let Some(&next) = input.peek() else {
                    if end { output.extend(codec.encode(&[point], substitute)); }
                    else { pending = Some(Pending::Scalar(point)); }
                    break;
                };
                if let Some(index) = compositions.iter().position(|entry| entry.points.starts_with(&[point, next])) {
                    input.next();
                    pending = Some(Pending::Hint(index, 2));
                    continue;
                }
            }
            output.extend(codec.encode(&[point], substitute));
        }
    }
    output
}

/// Emits each rejected hint unit through the ordinary counted substitution path.
fn append_rejected(codec: DoubleByte, output: &mut Vec<u8>, points: &[u32], substitute: Substitute) {
    for &point in points {
        substitute.append(point, output, |replacement, output| {
            if let Some(bytes) = codec.encoded(replacement) {
                output.extend_from_slice(bytes);
                true
            } else { false }
        });
    }
}

/// Recognizes Apple's private-use introducers for three-, four-, and five-scalar compositions.
fn hint(point: u32) -> bool { (0xf860..=0xf862).contains(&point) }
