//! Purpose:
//! A measurement, not a feature: does deserializing a cached script actually beat
//! re-parsing it? The whole point of an on-disk file cache is skipping the read, the
//! `<?php` scan and the parse, so a format that costs as much as parsing buys nothing.
//!
//! Called from:
//! - `cargo test -p elephc-magician file_cache_format -- --ignored --nocapture`.
//!
//! Key details:
//! - `#[ignore]`: it is a stopwatch, not an assertion. Timings are machine-dependent and a
//!   CI failure here would mean nothing.
//! - The fixture is a PHP file of realistic size whose padding is a COMMENT, so it grows
//!   the read, the scan and the parse without adding statements — the same shape the
//!   runtime-cache benchmark on the OPcache page uses.

#[cfg(test)]
mod tests {
    use super::super::segments::{segment_script, ParseMode, ScriptSegment};
    use std::time::Instant;

    /// Builds a PHP source of roughly `bytes` length with real statements plus comment padding.
    fn fixture(bytes: usize) -> Vec<u8> {
        let mut src = String::from("<?php\n");
        while src.len() < bytes {
            src.push_str("$a = 1; $b = $a + 2; if ($b > 1) { $c = $b * 3; }\n");
            src.push_str("// padding that grows the read, the scan and the parse\n");
        }
        src.into_bytes()
    }

    /// Reports parse time against serde_json round-trip time for the same segments.
    #[test]
    #[ignore]
    fn file_cache_format_costs_less_than_parsing() {
        for size in [4 * 1024usize, 64 * 1024, 256 * 1024] {
            let source = fixture(size);

            let start = Instant::now();
            let segments = segment_script(&source, ParseMode::Fresh);
            let parse = start.elapsed();

            let encoded = serde_json::to_vec(&segments).expect("segments must serialize");

            let start = Instant::now();
            let decoded: Vec<ScriptSegment> =
                serde_json::from_slice(&encoded).expect("segments must deserialize");
            let decode = start.elapsed();
            assert_eq!(decoded.len(), segments.len());

            let packed = bincode::serialize(&segments).expect("segments must pack");
            let start = Instant::now();
            let unpacked: Vec<ScriptSegment> =
                bincode::deserialize(&packed).expect("segments must unpack");
            let unpack = start.elapsed();
            assert_eq!(unpacked.len(), segments.len());

            println!(
                "{:>7} src | parse {:>9.3?} | json {:>8}B {:>9.3?} ({:.2}x) | bincode {:>8}B {:>9.3?} ({:.2}x)",
                source.len(),
                parse,
                encoded.len(),
                decode,
                parse.as_secs_f64() / decode.as_secs_f64().max(f64::MIN_POSITIVE),
                packed.len(),
                unpack,
                parse.as_secs_f64() / unpack.as_secs_f64().max(f64::MIN_POSITIVE),
            );
        }
    }
}
