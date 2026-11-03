//! Purpose:
//! `fscanf()` on `wasm32-wasi`: reading one line off a stream and splitting it into fields.
//!
//! Called from:
//! - `runtime` registers the two helpers alongside the file-I/O runtime.
//! - `builtins` parses the format at COMPILE time and emits one token step per conversion.
//! - `capability` refuses a format this pair cannot answer.
//!
//! Key details:
//! - The format is not interpreted at runtime. It must be a literal — as `sprintf`'s must — so
//!   the conversions are known when the module is built and the emitted code is a straight run
//!   of token-and-render steps with no format cursor at all.
//! - **This builtin's declared type is wrong, in the front end, for both backends.** The
//!   checker types `fscanf` as `array<string>`, and php returns TYPED fields: `%d` gives an
//!   int and `%f` a float. Nothing here can fix that — the type is shared with the native
//!   backend and with `sscanf` — so every numeric conversion is REFUSED by name rather than
//!   answered with a string php would not have produced (`is_int($r[0])` is true in php). Only
//!   `%s` fields are served, and those are strings in php too.
//! - The native backend, under the same type, answers the raw captured substring and
//!   mishandles a line with leading whitespace, answering empty fields for `"   spaced 7 1.5"`.
//!   Neither behaviour is reproduced here.
//! - Two shapes the type cannot express are given the nearest thing it can. End of input,
//!   where php answers `false`, answers an EMPTY array — falsy in the `while (($row = fscanf(...)))`
//!   loop php's own manual writes, which is the property that matters. A field the line does
//!   not supply, where php answers `NULL`, answers an empty string, so `count($row)` still
//!   equals the number of conversions; the native backend truncates the array instead, which
//!   loses that.

use super::wat::WatModule;

/// Offset of this module's syscall scratch: 32 bytes.
///
/// +0/+4 the one-byte read's iovec, +8 the byte count `fd_read` reports, +16..32 unused (they held
/// a parsed double while numeric conversions were served). Carved from the hole between `parse_url`'s parts
/// table and the directory state.
pub(super) const SCAN_SCRATCH: u32 = 0x5100;

/// Bytes reserved above.
pub(super) const SCAN_SCRATCH_BYTES: u32 = 32;

/// First size tried for a line read off a real fd, doubled until the line fits.
const LINE_START: u32 = 128;

/// Capacity past which a line read off a real fd cannot grow any further, which is where
/// doubling an i32 byte count would stop being representable. A line that reaches it stops on
/// the allocator's named out-of-memory fatal rather than being cut short.
const LINE_MAX: u32 = 1 << 30;

/// The bit marking a stream handle as an in-memory stream. Mirrors `files::MEMSTREAM_FLAG`.
const MEMSTREAM_FLAG: u32 = 0x4000_0000;

/// Proves the scratch fits the hole it is carved from.
///
/// It sits BETWEEN two tables rather than at the top of the region, so the usual
/// "does it fit the region" assertion is not enough — running past 32 bytes would land in the
/// directory state, which is live. Both bounds are checked.
const _: () = assert!(
    SCAN_SCRATCH + SCAN_SCRATCH_BYTES <= 0x5200
        && SCAN_SCRATCH + SCAN_SCRATCH_BYTES <= super::runtime::FLOAT_SCRATCH_SIZE,
    "the scanf scratch overruns the hole before the directory state"
);

/// Adds the line reader and the tokenizer to `wm`.
///
/// Requires the heap runtime, the WASI `fd_read` import, and `files`'s memstream layout.
pub(super) fn emit_scanf_runtime(wm: &mut WatModule) {
    wm.add_raw_func(&rt_stream_read_line());
    wm.add_raw_func(RT_SCAN_TOKEN);
}

/// `__rt_stream_read_line`: one line off a stream, as an OWNED heap block the caller frees.
///
/// The trailing newline is not part of the answer and is consumed, so the next call starts on
/// the next line. `ok = 0` means there was nothing left at all, which is distinct from an empty
/// line — that answers `ok = 1` with a zero length, because php's `fscanf` treats a blank line
/// as a line.
///
/// An in-memory stream is scanned in place and its position advanced; a real fd is read ONE
/// BYTE AT A TIME. That is deliberate rather than lazy: reading a block and seeking back is
/// faster but wrong on a stream that cannot seek, and `fscanf` has to leave the position
/// exactly after the newline for the next call. Lines are short and this runs once per line.
fn rt_stream_read_line() -> String {
    format!(
        r#"(func $__rt_stream_read_line (param $h i32) (result i32 i32 i32)
  (local $d i32) (local $pos i64) (local $len i64) (local $base i32)
  (local $i i32) (local $end i32) (local $line i32) (local $n i32)
  (local $cap i32) (local $buf i32) (local $grown i32) (local $byte i32) (local $got i32)
  (if (i32.lt_s (local.get $h) (i32.const 0))
    (then (return (i32.const 0) (i32.const 0) (i32.const 0))))     ;; not a stream
  (if (i32.and (local.get $h) (i32.const {flag})) (then
    ;; -- in-memory: the bytes are already here, so scan them and move the position --
    (local.set $d (i32.and (local.get $h) (i32.const {unflag})))
    (local.set $pos (i64.load (i32.add (local.get $d) (i32.const 16))))
    (local.set $len (i64.load (local.get $d)))
    (if (i64.ge_u (local.get $pos) (local.get $len))
      (then (return (i32.const 0) (i32.const 0) (i32.const 0))))   ;; nothing left
    (local.set $base (i32.load (i32.add (local.get $d) (i32.const 32))))
    (local.set $i (i32.wrap_i64 (local.get $pos)))
    (local.set $end (i32.wrap_i64 (local.get $len)))
    (block $found (loop $scan
      (br_if $found (i32.ge_u (local.get $i) (local.get $end)))
      (br_if $found (i32.eq (i32.load8_u (i32.add (local.get $base) (local.get $i)))
                            (i32.const 10)))
      (local.set $i (i32.add (local.get $i) (i32.const 1)))
      (br $scan)))
    (local.set $n (i32.sub (local.get $i) (i32.wrap_i64 (local.get $pos))))
    ;; step past the newline when there was one; otherwise stop at the end
    (i64.store (i32.add (local.get $d) (i32.const 16))
      (i64.extend_i32_u (select (i32.add (local.get $i) (i32.const 1))
                                (local.get $i)
                                (i32.lt_u (local.get $i) (local.get $end)))))
    (local.set $line (call $__rt_heap_alloc
      (select (local.get $n) (i32.const 1) (local.get $n))))       ;; never a zero-byte request
    (if (i32.eqz (local.get $line))
      (then (return (i32.const 0) (i32.const 0) (i32.const 0))))
    (if (local.get $n) (then
      (memory.copy (local.get $line)
                   (i32.add (local.get $base) (i32.wrap_i64 (local.get $pos)))
                   (local.get $n))))
    (return (local.get $line) (local.get $n) (i32.const 1))))
  ;; -- a real fd: one byte at a time, so the position lands exactly after the newline --
  (local.set $cap (i32.const {start}))
  (local.set $buf (call $__rt_heap_alloc (local.get $cap)))
  (if (i32.eqz (local.get $buf))
    (then (return (i32.const 0) (i32.const 0) (i32.const 0))))
  (block $done (loop $read
    (i32.store (i32.add (global.get $__float_scratch) (i32.const {iov}))
               (i32.add (global.get $__float_scratch) (i32.const {stage})))
    (i32.store (i32.add (global.get $__float_scratch) (i32.const {iovlen})) (i32.const 1))
    (br_if $done (i32.ne (call $wasi_fd_read
        (local.get $h)
        (i32.add (global.get $__float_scratch) (i32.const {iov}))
        (i32.const 1)
        (i32.add (global.get $__float_scratch) (i32.const {nread})))
      (i32.const 0)))
    (br_if $done (i32.eqz (i32.load (i32.add (global.get $__float_scratch) (i32.const {nread})))))
    (local.set $got (i32.const 1))                                 ;; a blank line is still a line
    (local.set $byte (i32.load8_u (i32.add (global.get $__float_scratch) (i32.const {stage}))))
    (br_if $done (i32.eq (local.get $byte) (i32.const 10)))        ;; the newline ends it
    (if (i32.eq (local.get $n) (local.get $cap)) (then
      ;; Growth is by checked doubling, never a truncated answer: past the ceiling the line
      ;; cannot be assembled, and a half line published as success would also have eaten the
      ;; byte that was just read.
      (if (i32.ge_u (local.get $cap) (i32.const {max}))
        (then (call $__rt_oom) unreachable))                       ;; elephc-trap:deterministic-oom:scanf-line-ceiling
      (local.set $grown (call $__rt_heap_alloc (i32.shl (local.get $cap) (i32.const 1))))
      (br_if $done (i32.eqz (local.get $grown)))
      (memory.copy (local.get $grown) (local.get $buf) (local.get $n))
      (call $__rt_heap_free (local.get $buf))
      (local.set $buf (local.get $grown))
      (local.set $cap (i32.shl (local.get $cap) (i32.const 1)))))
    (i32.store8 (i32.add (local.get $buf) (local.get $n)) (local.get $byte))
    (local.set $n (i32.add (local.get $n) (i32.const 1)))
    (br $read)))
  (if (i32.eqz (local.get $got)) (then
    (call $__rt_heap_free (local.get $buf))
    (return (i32.const 0) (i32.const 0) (i32.const 0))))            ;; end of input
  (local.get $buf) (local.get $n) (i32.const 1))
"#,
        flag = MEMSTREAM_FLAG,
        unflag = !MEMSTREAM_FLAG,
        start = LINE_START,
        max = LINE_MAX,
        iov = SCAN_SCRATCH,
        iovlen = SCAN_SCRATCH + 4,
        nread = SCAN_SCRATCH + 8,
        stage = SCAN_SCRATCH + 12
    )
}

/// `__rt_scan_token`: the next whitespace-delimited run, and where it ended.
///
/// Leading whitespace is skipped, which is what makes `"   spaced 7 1.5"` scan the same as
/// `"spaced 7 1.5"` — php's rule, and the one the native backend misses. A `tok_len` of zero
/// means the line had nothing left, which the caller renders as php's missing field.
///
/// The token is a SLICE of the line, not a copy: the caller either persists it or hands it to a
/// parser before freeing the line.
const RT_SCAN_TOKEN: &str = r#"(func $__rt_scan_token (param $ptr i32) (param $len i32) (param $pos i32) (result i32 i32 i32)
  (local $i i32) (local $start i32) (local $c i32)
  (local.set $i (local.get $pos))
  (block $lead (loop $skip
    (br_if $lead (i32.ge_u (local.get $i) (local.get $len)))
    (local.set $c (i32.load8_u (i32.add (local.get $ptr) (local.get $i))))
    (br_if $lead (i32.eqz (i32.or
      (i32.eq (local.get $c) (i32.const 32))                       ;; space
      (i32.and (i32.ge_u (local.get $c) (i32.const 9))
               (i32.le_u (local.get $c) (i32.const 13))))))        ;; \t \n \v \f \r
    (local.set $i (i32.add (local.get $i) (i32.const 1)))
    (br $skip)))
  (local.set $start (local.get $i))
  (block $end (loop $take
    (br_if $end (i32.ge_u (local.get $i) (local.get $len)))
    (local.set $c (i32.load8_u (i32.add (local.get $ptr) (local.get $i))))
    (br_if $end (i32.or
      (i32.eq (local.get $c) (i32.const 32))
      (i32.and (i32.ge_u (local.get $c) (i32.const 9))
               (i32.le_u (local.get $c) (i32.const 13)))))
    (local.set $i (i32.add (local.get $i) (i32.const 1)))
    (br $take)))
  (i32.add (local.get $ptr) (local.get $start))
  (i32.sub (local.get $i) (local.get $start))
  (local.get $i))
"#;

/// One step of a `fscanf` format, settled when the module is built.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ScanConversion {
    /// `%s` — the token's own bytes.
    Str,
}

/// Parses a `fscanf` format into its conversions, or says why it cannot.
///
/// Only whitespace and `%s` are served. `%d`, `%f`, `%e` and `%g` are refused by name: php returns
/// an int or a float for them, and this builtin's `array<string>` type cannot hold either, so any
/// answer would have the wrong type (`is_int($r[0])` is true in php). A LITERAL character in the format has to
/// match the input exactly, and a width or any other conversion changes what a token is — both
/// are refused rather than approximated, because a format this pair cannot honour would answer
/// the wrong fields rather than fail.
pub(super) fn parse_scan_format(format: &str) -> Result<Vec<ScanConversion>, String> {
    let bytes = format.as_bytes();
    let mut conversions = Vec::new();
    let mut index = 0;
    // Whether the previous step was a conversion with no whitespace since. php ends a `%d` at the
    // first non-digit, so `%d%s` over `12abc` is 12 and `abc`; this tokenizer only knows
    // whitespace, so it would hand `12abc` to the `%d` and nothing to the `%s`.
    let mut after_conversion = false;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte.is_ascii_whitespace() {
            after_conversion = false;
            index += 1;
            continue;
        }
        if byte == b'%' && after_conversion {
            return Err(
                "two conversions with no whitespace between them (like `%d%s`): php ends each \
                 field where its own syntax ends, this target only splits on whitespace"
                    .to_string(),
            );
        }
        if byte != b'%' {
            return Err(format!(
                "a literal {:?} in the format has to match the input, which is not lowered",
                byte as char
            ));
        }
        index += 1;
        let Some(conversion) = bytes.get(index) else {
            return Err("the format ends in a bare `%`".to_string());
        };
        conversions.push(match conversion {
            b's' => ScanConversion::Str,
            b'd' | b'f' | b'e' | b'g' => {
                return Err(format!(
                    "%{} yields a typed int or float field in php, but fscanf is typed \
                     array<string> here, so only %s is scanned",
                    *conversion as char
                ))
            }
            other => {
                return Err(format!(
                    "%{} is not one of the conversions this target scans (only %s)",
                    *other as char
                ))
            }
        });
        index += 1;
        after_conversion = true;
    }
    if conversions.is_empty() {
        return Err("the format has no conversions".to_string());
    }
    Ok(conversions)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::RuntimeFnId;

    /// The format parser admits exactly what the emitted steps can honour.
    ///
    /// Refusing is the point. A width (`%5s`) or a literal separator (`%d,%d`) changes where a
    /// token ENDS, and the tokenizer only knows whitespace — so accepting either would answer
    /// the wrong fields rather than fail, which is the one outcome worth preventing. `%e` and
    /// `%g` join `%f` because php scans all three as a double and this target renders a double
    /// one way.
    #[test]
    fn the_scan_format_admits_only_what_the_tokenizer_can_split() {
        // Both halves of the round trip are served, and they are each other's reason to exist:
        // the example writes rows with one and reads them back with the other.
        for target in [RuntimeFnId::Fprintf, RuntimeFnId::Fscanf] {
            assert!(
                crate::codegen_wasm::capability::runtime_function_is_supported(target),
                "{target:?} must be supported on this target"
            );
        }

        use ScanConversion::Str;
        assert_eq!(parse_scan_format("%s %s %s"), Ok(vec![Str, Str, Str]));
        assert_eq!(parse_scan_format("  %s\t%s\n"), Ok(vec![Str, Str]));

        // Numeric conversions are refused because php types their fields int/float.
        for numeric in ["%d", "%f", "%e", "%g", "%s %d %f", "%d %d"] {
            let why = parse_scan_format(numeric).unwrap_err();
            assert!(why.contains("typed int or float"), "{numeric:?}: {why}");
        }
        for refused in ["%5s", "%c", "%x", "%d,%d", "plain %d", "%", "", "%d%s", "%s%d %f"] {
            assert!(
                parse_scan_format(refused).is_err(),
                "{refused:?} must be refused rather than approximated"
            );
        }
    }
}
