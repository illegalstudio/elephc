//! Purpose:
//! Hand-rolled encoder for the pprof `Profile` protobuf, gzip-compressed — the
//! interchange format Grafana Pyroscope, Parca, and `go tool pprof` ingest.
//!
//! Called from:
//! - `crate::monitor` when `--pprof` exports a capture.
//!
//! Key details:
//! - Only the message subset a folded sampled profile needs is emitted: string
//!   table, one sample type, per-frame synthetic functions/locations, and samples
//!   as location chains. No addresses or mappings — consumers display names.
//! - pprof orders a sample's locations LEAF FIRST; callers pass root-first stacks
//!   and the encoder reverses them.
//! - Wire format is protobuf: varint keys `(field << 3) | wire_type`, wire type 0
//!   for ints, 2 for length-delimited (strings and nested messages).

use std::collections::HashMap;
use std::io::Write as _;

/// Appends a varint-encoded unsigned integer.
fn put_varint(out: &mut Vec<u8>, mut value: u64) {
    loop {
        let byte = (value & 0x7f) as u8;
        value >>= 7;
        if value == 0 {
            out.push(byte);
            break;
        }
        out.push(byte | 0x80);
    }
}

/// Appends a varint field (wire type 0).
fn put_int(out: &mut Vec<u8>, field: u64, value: u64) {
    if value == 0 {
        return; // proto3 default — omitted
    }
    put_varint(out, field << 3);
    put_varint(out, value);
}

/// Appends a length-delimited field (wire type 2).
fn put_bytes(out: &mut Vec<u8>, field: u64, payload: &[u8]) {
    put_varint(out, (field << 3) | 2);
    put_varint(out, payload.len() as u64);
    out.extend_from_slice(payload);
}

/// Encodes a `ValueType { type, unit }` message.
fn value_type(type_index: u64, unit_index: u64) -> Vec<u8> {
    let mut out = Vec::new();
    put_int(&mut out, 1, type_index);
    put_int(&mut out, 2, unit_index);
    out
}

/// Serializes folded stacks as a gzip-compressed pprof `Profile`.
///
/// `stacks` are root-first frame-name lists with a sample-count weight; equal
/// stacks should be pre-merged by the caller but duplicates are still valid pprof.
pub(crate) fn encode_folded_profile(stacks: &[(Vec<String>, u64)]) -> Vec<u8> {
    // String table: index 0 must be the empty string.
    let mut strings: Vec<String> = vec![String::new()];
    let mut string_index: HashMap<String, u64> = HashMap::new();
    let mut intern = |text: &str, strings: &mut Vec<String>| -> u64 {
        if text.is_empty() {
            return 0;
        }
        if let Some(&index) = string_index.get(text) {
            return index;
        }
        let index = strings.len() as u64;
        string_index.insert(text.to_string(), index);
        strings.push(text.to_string());
        index
    };

    let samples_index = intern("samples", &mut strings);
    let count_index = intern("count", &mut strings);

    // One synthetic function + location per unique frame name; ids are 1-based.
    let mut location_for_name: HashMap<String, u64> = HashMap::new();
    let mut functions: Vec<u64> = Vec::new(); // name string index per function id
    let mut body = Vec::new();

    // sample_type = [{samples, count}]
    put_bytes(&mut body, 1, &value_type(samples_index, count_index));

    let mut samples_payload = Vec::new();
    for (stack, weight) in stacks {
        let mut sample = Vec::new();
        // Leaf first, per the pprof contract.
        for name in stack.iter().rev() {
            let next_id = location_for_name.len() as u64 + 1;
            let id = *location_for_name.entry(name.clone()).or_insert_with(|| {
                functions.push(intern(name, &mut strings));
                next_id
            });
            put_int(&mut sample, 1, id);
        }
        // value = [weight] — field 2 is repeated int64; a single varint entry.
        put_int(&mut sample, 2, *weight);
        put_bytes(&mut samples_payload, 2, &sample);
    }
    body.extend_from_slice(&samples_payload);

    for (id, name_index) in functions.iter().enumerate() {
        let id = id as u64 + 1;
        let mut function = Vec::new();
        put_int(&mut function, 1, id);
        put_int(&mut function, 2, *name_index); // name
        put_int(&mut function, 3, *name_index); // system_name
        put_bytes(&mut body, 5, &function);

        let mut line = Vec::new();
        put_int(&mut line, 1, id); // function_id
        let mut location = Vec::new();
        put_int(&mut location, 1, id);
        put_bytes(&mut location, 4, &line);
        put_bytes(&mut body, 4, &location);
    }

    for text in &strings {
        put_bytes(&mut body, 6, text.as_bytes());
    }

    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(&body).expect("gzip write cannot fail on a Vec");
    encoder.finish().expect("gzip finish cannot fail on a Vec")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read as _;

    /// Minimal wire-format reader used to verify the encoder's framing.
    struct Reader<'a> {
        data: &'a [u8],
        pos: usize,
    }

    impl<'a> Reader<'a> {
        fn varint(&mut self) -> u64 {
            let mut value = 0u64;
            let mut shift = 0;
            loop {
                let byte = self.data[self.pos];
                self.pos += 1;
                value |= ((byte & 0x7f) as u64) << shift;
                if byte & 0x80 == 0 {
                    return value;
                }
                shift += 7;
            }
        }

        fn next_field(&mut self) -> Option<(u64, Field<'a>)> {
            if self.pos >= self.data.len() {
                return None;
            }
            let key = self.varint();
            let field = key >> 3;
            match key & 7 {
                0 => Some((field, Field::Int(self.varint()))),
                2 => {
                    let len = self.varint() as usize;
                    let payload = &self.data[self.pos..self.pos + len];
                    self.pos += len;
                    Some((field, Field::Bytes(payload)))
                }
                other => panic!("unexpected wire type {other}"),
            }
        }
    }

    enum Field<'a> {
        Int(u64),
        Bytes(&'a [u8]),
    }

    fn gunzip(data: &[u8]) -> Vec<u8> {
        let mut decoder = flate2::read::GzDecoder::new(data);
        let mut out = Vec::new();
        decoder.read_to_end(&mut out).expect("valid gzip");
        out
    }

    #[test]
    fn encodes_a_decodable_profile_with_leaf_first_stacks() {
        let stacks = vec![
            (vec!["main".to_string(), "hot".to_string()], 7u64),
            (vec!["main".to_string()], 3u64),
        ];
        let raw = gunzip(&encode_folded_profile(&stacks));
        let mut reader = Reader { data: &raw, pos: 0 };
        let mut strings = Vec::new();
        let mut samples = Vec::new();
        let mut functions = 0;
        let mut locations = 0;
        while let Some((field, value)) = reader.next_field() {
            match (field, value) {
                (6, Field::Bytes(b)) => strings.push(String::from_utf8(b.to_vec()).unwrap()),
                (2, Field::Bytes(b)) => {
                    let mut inner = Reader { data: b, pos: 0 };
                    let mut location_ids = Vec::new();
                    let mut weight = 0;
                    while let Some((f, v)) = inner.next_field() {
                        match (f, v) {
                            (1, Field::Int(id)) => location_ids.push(id),
                            (2, Field::Int(w)) => weight = w,
                            _ => {}
                        }
                    }
                    samples.push((location_ids, weight));
                }
                (5, Field::Bytes(_)) => functions += 1,
                (4, Field::Bytes(_)) => locations += 1,
                _ => {}
            }
        }
        assert_eq!(strings[0], "");
        assert!(strings.contains(&"main".to_string()));
        assert!(strings.contains(&"hot".to_string()));
        assert_eq!(functions, 2);
        assert_eq!(locations, 2);
        // Stack [main, hot] serializes leaf-first: hot's location id precedes main's.
        assert_eq!(samples.len(), 2);
        assert_eq!(samples[0].1, 7);
        assert_eq!(samples[0].0.len(), 2);
        assert_eq!(samples[1].1, 3);
        assert_eq!(samples[1].0.len(), 1);
        // The single-frame sample's location must be main's id — the same id used
        // LAST (root position) in the two-frame sample.
        assert_eq!(samples[1].0[0], *samples[0].0.last().unwrap());
    }
}
