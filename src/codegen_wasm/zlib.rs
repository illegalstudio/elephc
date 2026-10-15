//! Purpose:
//! DEFLATE on `wasm32-wasi`: `gzdeflate`, `gzinflate`, `gzcompress` and `gzuncompress`.
//!
//! Called from:
//! - `runtime` registers the codec when the module compresses or decompresses.
//! - `builtins` lowers the four `RuntimeFnId` identities onto the glue helpers.
//! - `capability` gates each call against the operand shapes the helpers read.
//!
//! Key details:
//! - A REIMPLEMENTATION, like `bcmath`. The native backend links zlib; a wasm module assembles
//!   one self-contained WAT text and links nothing, so RFC 1951 is written here.
//! - **Inflate is complete; deflate is not zlib.** The decompressor handles every block type
//!   the format defines — stored, fixed Huffman and dynamic Huffman — because the bytes it is
//!   given may well have come from zlib. The compressor emits ONE final fixed-Huffman block
//!   with greedy LZ77 matching, which is valid DEFLATE that any decompressor reads, but it is
//!   not byte-identical to what zlib's level 6 produces and on some inputs it is larger.
//! - That difference is measurable rather than hidden. On `examples/gzip`'s input — a 36-byte
//!   phrase repeated eight times — the encoding is 36 literals at 8 bits, one (252, 36) match
//!   at 13 + 9 bits, and the 7-bit end marker, plus the 3-bit block header: 320 bits, 40 bytes.
//!   php answers 40 for `gzdeflate` and 46 for `gzcompress`, which is those 40 plus zlib's
//!   two-byte header and four-byte Adler-32. The sizes agree here; they need not on every
//!   input, and a program that compares a compressed BLOB against a stored one will differ.
//! - The match finder keeps ONE candidate per three-byte hash rather than a chain. zlib walks
//!   a chain and does lazy matching; this takes the most recent position, verifies it, and
//!   extends. It is weaker on text with many near-repeats and exact on a periodic input.

use super::wat::{DataSegment, Global, ValType, WatModule};

/// Base length for each length code 257..285.
const LENGTH_BASE: [i32; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115,
    131, 163, 195, 227, 258,
];

/// Extra bits following each length code 257..285.
const LENGTH_EXTRA: [i32; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];

/// Base distance for each distance code 0..29.
const DIST_BASE: [i32; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];

/// Extra bits following each distance code 0..29.
const DIST_EXTRA: [i32; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12,
    13, 13,
];

/// The order a dynamic block writes its code-length code lengths in (RFC 1951 §3.2.7).
const CLEN_ORDER: [u8; 19] = [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15];

/// Lays the four code tables out in static data and declares their base globals.
///
/// Must run before `heap_base` is computed, like every other static table. Returns the
/// advanced data cursor.
pub(super) fn emit_zlib_tables(wm: &mut WatModule, mut cursor: u32) -> u32 {
    cursor = (cursor + 3) & !3;
    let place_i32 = |wm: &mut WatModule, name: &str, values: &[i32], cursor: &mut u32| {
        let base = *cursor;
        let mut bytes = Vec::with_capacity(values.len() * 4);
        for value in values {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        wm.add_data(DataSegment {
            offset: base,
            bytes,
        });
        *cursor += values.len() as u32 * 4;
        wm.add_global(Global {
            name: name.to_string(),
            ty: ValType::I32,
            mutable: false,
            init: base as i64,
        });
    };
    place_i32(wm, "__z_lbase", &LENGTH_BASE, &mut cursor);
    place_i32(wm, "__z_lextra", &LENGTH_EXTRA, &mut cursor);
    place_i32(wm, "__z_dbase", &DIST_BASE, &mut cursor);
    place_i32(wm, "__z_dextra", &DIST_EXTRA, &mut cursor);
    let order = cursor;
    wm.add_data(DataSegment {
        offset: order,
        bytes: CLEN_ORDER.to_vec(),
    });
    cursor += CLEN_ORDER.len() as u32;
    wm.add_global(Global {
        name: "__z_clorder".to_string(),
        ty: ValType::I32,
        mutable: false,
        init: order as i64,
    });
    cursor
}

/// Adds the codec to `wm`. Requires the heap and string runtimes and the tables above.
pub(super) fn emit_zlib_runtime(wm: &mut WatModule) {
    for name in [
        "__z_in", "__z_inlen", "__z_pos", "__z_bitbuf", "__z_bitcnt", "__z_out", "__z_outlen",
        "__z_outcap",
    ] {
        wm.add_global(Global {
            name: name.to_string(),
            ty: ValType::I32,
            mutable: true,
            init: 0,
        });
    }
    wm.add_raw_func(RT_Z_BITS);
    wm.add_raw_func(RT_Z_GROW);
    wm.add_raw_func(RT_Z_PUT);
    wm.add_raw_func(RT_Z_WBITS);
    wm.add_raw_func(RT_Z_WHUFF);
    wm.add_raw_func(RT_Z_BUILD);
    wm.add_raw_func(RT_Z_DECODE);
    wm.add_raw_func(RT_Z_FIXED_LENS);
    wm.add_raw_func(RT_Z_BLOCK);
    wm.add_raw_func(RT_INFLATE);
    wm.add_raw_func(RT_Z_LITERAL);
    wm.add_raw_func(RT_Z_HASH);
    wm.add_raw_func(RT_Z_EMIT_MATCH);
    wm.add_raw_func(RT_DEFLATE);
    wm.add_raw_func(RT_ADLER32);
    wm.add_raw_func(RT_GZDEFLATE);
    wm.add_raw_func(RT_GZINFLATE);
    wm.add_raw_func(RT_GZCOMPRESS);
    wm.add_raw_func(RT_GZUNCOMPRESS);
}

/// `__rt_z_bits`: the next `$n` bits of the input, LSB-first, as DEFLATE packs them.
///
/// Reading past the end yields ZEROS rather than trapping. A truncated stream then decodes
/// into something the block walk rejects, which is how a corrupt input reaches php's `false`
/// instead of killing the module.
const RT_Z_BITS: &str = r#"(func $__rt_z_bits (param $n i32) (result i32)
  (local $v i32)
  (block $ready (loop $fill
    (br_if $ready (i32.ge_u (global.get $__z_bitcnt) (local.get $n)))
    (if (i32.lt_u (global.get $__z_pos) (global.get $__z_inlen)) (then
      (global.set $__z_bitbuf (i32.or (global.get $__z_bitbuf)
        (i32.shl (i32.load8_u (i32.add (global.get $__z_in) (global.get $__z_pos)))
                 (global.get $__z_bitcnt))))
      (global.set $__z_pos (i32.add (global.get $__z_pos) (i32.const 1)))))
    (global.set $__z_bitcnt (i32.add (global.get $__z_bitcnt) (i32.const 8)))
    (br $fill)))
  (local.set $v (i32.and (global.get $__z_bitbuf)
    (i32.sub (i32.shl (i32.const 1) (local.get $n)) (i32.const 1))))
  (global.set $__z_bitbuf (i32.shr_u (global.get $__z_bitbuf) (local.get $n)))
  (global.set $__z_bitcnt (i32.sub (global.get $__z_bitcnt) (local.get $n)))
  (local.get $v))
"#;

/// `__rt_z_grow`: makes room for `$more` output bytes. 0 on refusal.
const RT_Z_GROW: &str = r#"(func $__rt_z_grow (param $more i32) (result i32)
  (local $cap i32) (local $next i32)
  (local.set $cap (global.get $__z_outcap))
  (if (i32.le_u (i32.add (global.get $__z_outlen) (local.get $more)) (local.get $cap))
    (then (return (i32.const 1))))
  (local.set $cap (select (local.get $cap) (i32.const 256) (local.get $cap)))
  (block $big (loop $double
    (br_if $big (i32.ge_u (local.get $cap)
      (i32.add (global.get $__z_outlen) (local.get $more))))
    (local.set $cap (i32.shl (local.get $cap) (i32.const 1)))
    (br $double)))
  (local.set $next (call $__rt_heap_alloc (local.get $cap)))
  (if (i32.eqz (local.get $next)) (then (return (i32.const 0))))
  (if (global.get $__z_out) (then
    (memory.copy (local.get $next) (global.get $__z_out) (global.get $__z_outlen))
    (call $__rt_heap_free (global.get $__z_out))))
  (global.set $__z_out (local.get $next))
  (global.set $__z_outcap (local.get $cap))
  (i32.const 1))
"#;

/// `__rt_z_put`: appends one output byte. 0 on refusal.
const RT_Z_PUT: &str = r#"(func $__rt_z_put (param $byte i32) (result i32)
  (if (i32.eqz (call $__rt_z_grow (i32.const 1))) (then (return (i32.const 0))))
  (i32.store8 (i32.add (global.get $__z_out) (global.get $__z_outlen)) (local.get $byte))
  (global.set $__z_outlen (i32.add (global.get $__z_outlen) (i32.const 1)))
  (i32.const 1))
"#;

/// `__rt_z_wbits`: writes `$n` bits of `$v`, LSB-first, flushing whole bytes as they fill.
const RT_Z_WBITS: &str = r#"(func $__rt_z_wbits (param $v i32) (param $n i32) (result i32)
  (global.set $__z_bitbuf (i32.or (global.get $__z_bitbuf)
    (i32.shl (i32.and (local.get $v) (i32.sub (i32.shl (i32.const 1) (local.get $n)) (i32.const 1)))
             (global.get $__z_bitcnt))))
  (global.set $__z_bitcnt (i32.add (global.get $__z_bitcnt) (local.get $n)))
  (block $done (loop $flush
    (br_if $done (i32.lt_u (global.get $__z_bitcnt) (i32.const 8)))
    (if (i32.eqz (call $__rt_z_put (i32.and (global.get $__z_bitbuf) (i32.const 255))))
      (then (return (i32.const 0))))
    (global.set $__z_bitbuf (i32.shr_u (global.get $__z_bitbuf) (i32.const 8)))
    (global.set $__z_bitcnt (i32.sub (global.get $__z_bitcnt) (i32.const 8)))
    (br $flush)))
  (i32.const 1))
"#;

/// `__rt_z_whuff`: writes a Huffman code, most significant bit FIRST.
///
/// The two orders in one format are not a mistake to work around: DEFLATE packs its bit fields
/// LSB-first but its Huffman codes MSB-first, so a code has to be reversed on the way into the
/// same LSB-first writer every other field uses.
const RT_Z_WHUFF: &str = r#"(func $__rt_z_whuff (param $code i32) (param $n i32) (result i32)
  (local $i i32)
  (local.set $i (local.get $n))
  (block $done (loop $bit
    (br_if $done (i32.eqz (local.get $i)))
    (local.set $i (i32.sub (local.get $i) (i32.const 1)))
    (if (i32.eqz (call $__rt_z_wbits
        (i32.and (i32.shr_u (local.get $code) (local.get $i)) (i32.const 1)) (i32.const 1)))
      (then (return (i32.const 0))))
    (br $bit)))
  (i32.const 1))
"#;

/// `__rt_z_build`: a canonical Huffman decoder from `$n` code lengths. 0 on refusal.
///
/// The block holds sixteen per-length counts and then the symbols ordered by (length, symbol),
/// which is all a canonical code needs — no lookup table, because `__rt_z_decode` walks one
/// bit at a time and the alphabets here are small.
const RT_Z_BUILD: &str = r#"(func $__rt_z_build (param $lens i32) (param $n i32) (result i32)
  (local $h i32) (local $i i32) (local $len i32) (local $offset i32) (local $slot i32)
  (local.set $h (call $__rt_heap_alloc (i32.add (i32.const 128) (i32.shl (local.get $n) (i32.const 2)))))
  (if (i32.eqz (local.get $h)) (then (return (i32.const 0))))
  (memory.fill (local.get $h) (i32.const 0)
               (i32.add (i32.const 128) (i32.shl (local.get $n) (i32.const 2))))
  ;; counts[len] over the 1..15 lengths; length 0 means "this symbol is not in the code"
  (local.set $i (i32.const 0))
  (block $counted (loop $count
    (br_if $counted (i32.ge_u (local.get $i) (local.get $n)))
    (local.set $len (i32.load8_u (i32.add (local.get $lens) (local.get $i))))
    (if (local.get $len) (then
      (i32.store (i32.add (local.get $h) (i32.shl (local.get $len) (i32.const 2)))
        (i32.add (i32.load (i32.add (local.get $h) (i32.shl (local.get $len) (i32.const 2))))
                 (i32.const 1)))))
    (local.set $i (i32.add (local.get $i) (i32.const 1)))
    (br $count)))
  ;; running offset per length, reused as the write cursor while the symbols are placed
  (local.set $len (i32.const 1))
  (local.set $offset (i32.const 0))
  (block $offsets (loop $walk
    (br_if $offsets (i32.gt_u (local.get $len) (i32.const 15)))
    (i32.store (i32.add (i32.add (local.get $h) (i32.const 64))
                        (i32.shl (local.get $len) (i32.const 2)))
               (local.get $offset))
    (local.set $offset (i32.add (local.get $offset)
      (i32.load (i32.add (local.get $h) (i32.shl (local.get $len) (i32.const 2))))))
    (local.set $len (i32.add (local.get $len) (i32.const 1)))
    (br $walk)))
  (local.set $i (i32.const 0))
  (block $placed (loop $place
    (br_if $placed (i32.ge_u (local.get $i) (local.get $n)))
    (local.set $len (i32.load8_u (i32.add (local.get $lens) (local.get $i))))
    (if (local.get $len) (then
      (local.set $slot (i32.add (i32.add (local.get $h) (i32.const 64))
                                (i32.shl (local.get $len) (i32.const 2))))
      (i32.store (i32.add (i32.add (local.get $h) (i32.const 128))
                          (i32.shl (i32.load (local.get $slot)) (i32.const 2)))
                 (local.get $i))
      (i32.store (local.get $slot) (i32.add (i32.load (local.get $slot)) (i32.const 1)))))
    (local.set $i (i32.add (local.get $i) (i32.const 1)))
    (br $place)))
  (local.get $h))
"#;

/// `__rt_z_decode`: the next symbol from a canonical code, or -1 when the bits match none.
///
/// The classic length-by-length walk: accumulate one bit, and if the accumulated code falls
/// inside this length's run of codes, the symbol is that run's base plus the offset.
const RT_Z_DECODE: &str = r#"(func $__rt_z_decode (param $h i32) (result i32)
  (local $code i32) (local $first i32) (local $index i32) (local $len i32) (local $count i32)
  (local.set $len (i32.const 1))
  (block $fail (loop $walk
    (br_if $fail (i32.gt_u (local.get $len) (i32.const 15)))
    (local.set $code (i32.or (local.get $code) (call $__rt_z_bits (i32.const 1))))
    (local.set $count (i32.load (i32.add (local.get $h) (i32.shl (local.get $len) (i32.const 2)))))
    (if (i32.lt_u (i32.sub (local.get $code) (local.get $first)) (local.get $count))
      (then (return (i32.load (i32.add (i32.add (local.get $h) (i32.const 128))
        (i32.shl (i32.add (local.get $index) (i32.sub (local.get $code) (local.get $first)))
                 (i32.const 2)))))))
    (local.set $index (i32.add (local.get $index) (local.get $count)))
    (local.set $first (i32.shl (i32.add (local.get $first) (local.get $count)) (i32.const 1)))
    (local.set $code (i32.shl (local.get $code) (i32.const 1)))
    (local.set $len (i32.add (local.get $len) (i32.const 1)))
    (br $walk)))
  (i32.const -1))
"#;

/// `__rt_z_fixed_lens`: writes RFC 1951's fixed code lengths into `$lit` (288) and `$dist` (30).
const RT_Z_FIXED_LENS: &str = r#"(func $__rt_z_fixed_lens (param $lit i32) (param $dist i32)
  (memory.fill (local.get $lit) (i32.const 8) (i32.const 144))
  (memory.fill (i32.add (local.get $lit) (i32.const 144)) (i32.const 9) (i32.const 112))
  (memory.fill (i32.add (local.get $lit) (i32.const 256)) (i32.const 7) (i32.const 24))
  (memory.fill (i32.add (local.get $lit) (i32.const 280)) (i32.const 8) (i32.const 8))
  (memory.fill (local.get $dist) (i32.const 5) (i32.const 30)))
"#;

/// `__rt_z_block`: decodes one Huffman-coded block's symbols. 1 on success, 0 on corruption.
const RT_Z_BLOCK: &str = r#"(func $__rt_z_block (param $lit i32) (param $dist i32) (result i32)
  (local $sym i32) (local $len i32) (local $d i32) (local $from i32) (local $i i32)
  (block $end (loop $symbols
    (local.set $sym (call $__rt_z_decode (local.get $lit)))
    (if (i32.lt_s (local.get $sym) (i32.const 0)) (then (return (i32.const 0))))
    (br_if $end (i32.eq (local.get $sym) (i32.const 256)))
    (if (i32.lt_u (local.get $sym) (i32.const 256)) (then
      (if (i32.eqz (call $__rt_z_put (local.get $sym))) (then (return (i32.const 0))))
      (br $symbols)))
    (if (i32.gt_u (local.get $sym) (i32.const 285)) (then (return (i32.const 0))))
    (local.set $len (i32.add
      (i32.load (i32.add (global.get $__z_lbase)
                         (i32.shl (i32.sub (local.get $sym) (i32.const 257)) (i32.const 2))))
      (call $__rt_z_bits (i32.load (i32.add (global.get $__z_lextra)
        (i32.shl (i32.sub (local.get $sym) (i32.const 257)) (i32.const 2)))))))
    (local.set $sym (call $__rt_z_decode (local.get $dist)))
    (if (i32.or (i32.lt_s (local.get $sym) (i32.const 0))
                (i32.gt_u (local.get $sym) (i32.const 29)))
      (then (return (i32.const 0))))
    (local.set $d (i32.add
      (i32.load (i32.add (global.get $__z_dbase) (i32.shl (local.get $sym) (i32.const 2))))
      (call $__rt_z_bits (i32.load (i32.add (global.get $__z_dextra)
        (i32.shl (local.get $sym) (i32.const 2)))))))
    (if (i32.gt_u (local.get $d) (global.get $__z_outlen)) (then (return (i32.const 0))))
    ;; the copy is byte by byte on purpose: an overlapping match repeats what it just wrote,
    ;; which is how a run longer than its own distance is encoded
    (local.set $from (i32.sub (global.get $__z_outlen) (local.get $d)))
    (local.set $i (i32.const 0))
    (block $copied (loop $copy
      (br_if $copied (i32.ge_u (local.get $i) (local.get $len)))
      (if (i32.eqz (call $__rt_z_put
          (i32.load8_u (i32.add (global.get $__z_out) (i32.add (local.get $from) (local.get $i))))))
        (then (return (i32.const 0))))
      (local.set $i (i32.add (local.get $i) (i32.const 1)))
      (br $copy)))
    (br $symbols)))
  (i32.const 1))
"#;

/// `__rt_inflate`: RFC 1951 decompression. Answers `(pointer, length, ok)`.
///
/// Every block type is handled, because the bytes may have come from zlib rather than from
/// this module's own compressor. The output block is owned by the caller.
const RT_INFLATE: &str = r#"(func $__rt_inflate (param $ptr i32) (param $len i32) (result i32 i32 i32)
  (local $final i32) (local $type i32) (local $n i32) (local $i i32)
  (local $lit i32) (local $dist i32) (local $lens i32) (local $clens i32)
  (local $hlit i32) (local $hdist i32) (local $hclen i32) (local $clh i32)
  (local $sym i32) (local $prev i32) (local $repeat i32) (local $ok i32)
  (global.set $__z_in (local.get $ptr))
  (global.set $__z_inlen (local.get $len))
  (global.set $__z_pos (i32.const 0))
  (global.set $__z_bitbuf (i32.const 0))
  (global.set $__z_bitcnt (i32.const 0))
  (global.set $__z_out (i32.const 0))
  (global.set $__z_outlen (i32.const 0))
  (global.set $__z_outcap (i32.const 0))
  (local.set $lens (call $__rt_heap_alloc (i32.const 320)))
  (local.set $clens (call $__rt_heap_alloc (i32.const 19)))
  (if (i32.or (i32.eqz (local.get $lens)) (i32.eqz (local.get $clens)))
    (then (return (i32.const 0) (i32.const 0) (i32.const 0))))
  (local.set $ok (i32.const 1))
  (block $stream (loop $blocks
    (local.set $final (call $__rt_z_bits (i32.const 1)))
    (local.set $type (call $__rt_z_bits (i32.const 2)))
    (if (i32.eq (local.get $type) (i32.const 0)) (then
      ;; stored: discard the partial byte, then LEN / ~LEN and the raw bytes
      (global.set $__z_bitbuf (i32.const 0))
      (global.set $__z_bitcnt (i32.const 0))
      (if (i32.gt_u (i32.add (global.get $__z_pos) (i32.const 4)) (global.get $__z_inlen))
        (then (local.set $ok (i32.const 0)) (br $stream)))
      (local.set $n (i32.or
        (i32.load8_u (i32.add (global.get $__z_in) (global.get $__z_pos)))
        (i32.shl (i32.load8_u (i32.add (global.get $__z_in)
          (i32.add (global.get $__z_pos) (i32.const 1)))) (i32.const 8))))
      (global.set $__z_pos (i32.add (global.get $__z_pos) (i32.const 4)))
      (if (i32.gt_u (i32.add (global.get $__z_pos) (local.get $n)) (global.get $__z_inlen))
        (then (local.set $ok (i32.const 0)) (br $stream)))
      (if (i32.eqz (call $__rt_z_grow (local.get $n)))
        (then (local.set $ok (i32.const 0)) (br $stream)))
      (memory.copy (i32.add (global.get $__z_out) (global.get $__z_outlen))
                   (i32.add (global.get $__z_in) (global.get $__z_pos))
                   (local.get $n))
      (global.set $__z_outlen (i32.add (global.get $__z_outlen) (local.get $n)))
      (global.set $__z_pos (i32.add (global.get $__z_pos) (local.get $n))))
    (else
      (if (i32.eq (local.get $type) (i32.const 3))
        (then (local.set $ok (i32.const 0)) (br $stream)))
      (if (i32.eq (local.get $type) (i32.const 1))
        (then
          (call $__rt_z_fixed_lens (local.get $lens) (i32.add (local.get $lens) (i32.const 288)))
          (local.set $hlit (i32.const 288))
          (local.set $hdist (i32.const 30)))
        (else
          ;; dynamic: the code-length code first, then the two alphabets it encodes
          (local.set $hlit (i32.add (call $__rt_z_bits (i32.const 5)) (i32.const 257)))
          (local.set $hdist (i32.add (call $__rt_z_bits (i32.const 5)) (i32.const 1)))
          (local.set $hclen (i32.add (call $__rt_z_bits (i32.const 4)) (i32.const 4)))
          (memory.fill (local.get $clens) (i32.const 0) (i32.const 19))
          (local.set $i (i32.const 0))
          (block $clread (loop $cl
            (br_if $clread (i32.ge_u (local.get $i) (local.get $hclen)))
            (i32.store8 (i32.add (local.get $clens)
              (i32.load8_u (i32.add (global.get $__z_clorder) (local.get $i))))
              (call $__rt_z_bits (i32.const 3)))
            (local.set $i (i32.add (local.get $i) (i32.const 1)))
            (br $cl)))
          (local.set $clh (call $__rt_z_build (local.get $clens) (i32.const 19)))
          (if (i32.eqz (local.get $clh)) (then (local.set $ok (i32.const 0)) (br $stream)))
          (memory.fill (local.get $lens) (i32.const 0) (i32.const 320))
          (local.set $i (i32.const 0))
          (block $lenread (loop $lenloop
            (br_if $lenread (i32.ge_u (local.get $i) (i32.add (local.get $hlit) (local.get $hdist))))
            (local.set $sym (call $__rt_z_decode (local.get $clh)))
            (if (i32.lt_s (local.get $sym) (i32.const 0))
              (then (local.set $ok (i32.const 0)) (br $lenread)))
            (if (i32.lt_u (local.get $sym) (i32.const 16)) (then
              (i32.store8 (i32.add (local.get $lens) (local.get $i)) (local.get $sym))
              (local.set $prev (local.get $sym))
              (local.set $i (i32.add (local.get $i) (i32.const 1)))
              (br $lenloop)))
            ;; 16 repeats the previous length, 17 and 18 run zeros of two different widths
            (if (i32.eq (local.get $sym) (i32.const 16))
              (then (local.set $repeat (i32.add (call $__rt_z_bits (i32.const 2)) (i32.const 3))))
              (else
                (local.set $prev (i32.const 0))
                (if (i32.eq (local.get $sym) (i32.const 17))
                  (then (local.set $repeat (i32.add (call $__rt_z_bits (i32.const 3)) (i32.const 3))))
                  (else (local.set $repeat (i32.add (call $__rt_z_bits (i32.const 7)) (i32.const 11)))))))
            (block $rep (loop $repeat_loop
              (br_if $rep (i32.eqz (local.get $repeat)))
              (br_if $rep (i32.ge_u (local.get $i) (i32.const 320)))
              (i32.store8 (i32.add (local.get $lens) (local.get $i)) (local.get $prev))
              (local.set $i (i32.add (local.get $i) (i32.const 1)))
              (local.set $repeat (i32.sub (local.get $repeat) (i32.const 1)))
              (br $repeat_loop)))
            (br $lenloop)))
          (call $__rt_heap_free (local.get $clh))
          (if (i32.eqz (local.get $ok)) (then (br $stream)))))
      (local.set $lit (call $__rt_z_build (local.get $lens) (local.get $hlit)))
      (local.set $dist (call $__rt_z_build
        (i32.add (local.get $lens) (local.get $hlit)) (local.get $hdist)))
      (if (i32.or (i32.eqz (local.get $lit)) (i32.eqz (local.get $dist)))
        (then (local.set $ok (i32.const 0)) (br $stream)))
      (local.set $ok (call $__rt_z_block (local.get $lit) (local.get $dist)))
      (call $__rt_heap_free (local.get $lit))
      (call $__rt_heap_free (local.get $dist))
      (if (i32.eqz (local.get $ok)) (then (br $stream)))))
    (br_if $stream (local.get $final))
    (br $blocks)))
  (call $__rt_heap_free (local.get $lens))
  (call $__rt_heap_free (local.get $clens))
  (if (i32.eqz (local.get $ok)) (then
    (call $__rt_heap_free (global.get $__z_out))
    (return (i32.const 0) (i32.const 0) (i32.const 0))))
  (global.get $__z_out) (global.get $__z_outlen) (i32.const 1))
"#;

/// `__rt_z_literal`: writes one literal byte with the fixed Huffman code. 0 on refusal.
const RT_Z_LITERAL: &str = r#"(func $__rt_z_literal (param $sym i32) (result i32)
  (if (i32.lt_u (local.get $sym) (i32.const 144))
    (then (return (call $__rt_z_whuff (i32.add (i32.const 48) (local.get $sym)) (i32.const 8)))))
  (call $__rt_z_whuff
    (i32.add (i32.const 400) (i32.sub (local.get $sym) (i32.const 144))) (i32.const 9)))
"#;

/// `__rt_z_hash`: the three-byte hash a match candidate is looked up by.
const RT_Z_HASH: &str = r#"(func $__rt_z_hash (param $ptr i32) (param $at i32) (result i32)
  (i32.and (i32.xor (i32.xor
    (i32.shl (i32.load8_u (i32.add (local.get $ptr) (local.get $at))) (i32.const 10))
    (i32.shl (i32.load8_u (i32.add (local.get $ptr) (i32.add (local.get $at) (i32.const 1))))
             (i32.const 5)))
    (i32.load8_u (i32.add (local.get $ptr) (i32.add (local.get $at) (i32.const 2)))))
    (i32.const 32767)))
"#;

/// `__rt_z_emit_match`: one length/distance pair in the fixed Huffman code.
///
/// Both tables are searched from the top down for the last entry whose base is not past the
/// value, which is what makes them a partition: the entry found is the only one whose extra
/// bits can express the remainder.
const RT_Z_EMIT_MATCH: &str = r#"(func $__rt_z_emit_match (param $len i32) (param $dist i32)
  (local $i i32) (local $code i32)
  (local.set $i (i32.const 28))
  (block $lfound (loop $lscan
    (br_if $lfound (i32.le_u
      (i32.load (i32.add (global.get $__z_lbase) (i32.shl (local.get $i) (i32.const 2))))
      (local.get $len)))
    (local.set $i (i32.sub (local.get $i) (i32.const 1)))
    (br $lscan)))
  (local.set $code (i32.add (local.get $i) (i32.const 257)))
  (if (i32.lt_u (local.get $code) (i32.const 280))
    (then (drop (call $__rt_z_whuff (i32.sub (local.get $code) (i32.const 256)) (i32.const 7))))
    (else (drop (call $__rt_z_whuff
      (i32.add (i32.const 192) (i32.sub (local.get $code) (i32.const 280))) (i32.const 8)))))
  (drop (call $__rt_z_wbits
    (i32.sub (local.get $len)
             (i32.load (i32.add (global.get $__z_lbase) (i32.shl (local.get $i) (i32.const 2)))))
    (i32.load (i32.add (global.get $__z_lextra) (i32.shl (local.get $i) (i32.const 2))))))
  (local.set $i (i32.const 29))
  (block $dfound (loop $dscan
    (br_if $dfound (i32.le_u
      (i32.load (i32.add (global.get $__z_dbase) (i32.shl (local.get $i) (i32.const 2))))
      (local.get $dist)))
    (local.set $i (i32.sub (local.get $i) (i32.const 1)))
    (br $dscan)))
  (drop (call $__rt_z_whuff (local.get $i) (i32.const 5)))          ;; distances are 5 fixed bits
  (drop (call $__rt_z_wbits
    (i32.sub (local.get $dist)
             (i32.load (i32.add (global.get $__z_dbase) (i32.shl (local.get $i) (i32.const 2)))))
    (i32.load (i32.add (global.get $__z_dextra) (i32.shl (local.get $i) (i32.const 2)))))))
"#;

/// `__rt_deflate`: one final fixed-Huffman block with LAZY LZ77. Answers `(pointer, length)`.
///
/// Lazy matching is zlib's `deflate_slow`: a match found at one position is held back until
/// the next position has been examined, and emitted only if that next position does not offer
/// a longer one. Holding it back costs a literal and can save several, and the difference is
/// measurable rather than assumed — modelled against this compressor's own greedy form, lazy
/// is 5.3% smaller on this repository's `targets.md`, 6.1% on a Rust source file and 4.5% on
/// repetitive JSON, and identical on a strictly periodic input where the first match found is
/// already the best one.
///
/// The match finder keeps ONE candidate per three-byte hash, where zlib walks a chain of up to
/// 128. That is the remaining gap on text with many near-repeats, along with the bigger one:
/// zlib picks DYNAMIC Huffman for real text and this emits a fixed block, which is most of why
/// zlib's output is roughly a third smaller on prose.
///
/// A three-byte match further away than 4096 bytes is discarded, which is zlib's `TOO_FAR`: at
/// that distance the code for the pair is no shorter than the three literals it replaces.
const RT_DEFLATE: &str = r#"(func $__rt_deflate (param $ptr i32) (param $len i32) (result i32 i32)
  (local $head i32) (local $pos i32) (local $h i32) (local $cand i32)
  (local $cur_len i32) (local $cur_match i32) (local $prev_len i32) (local $prev_match i32)
  (local $available i32) (local $m i32) (local $limit i32) (local $i i32) (local $stop i32)
  (global.set $__z_out (i32.const 0))
  (global.set $__z_outlen (i32.const 0))
  (global.set $__z_outcap (i32.const 0))
  (global.set $__z_bitbuf (i32.const 0))
  (global.set $__z_bitcnt (i32.const 0))
  (local.set $head (call $__rt_heap_alloc (i32.const 131072)))     ;; 32768 slots of i32
  (if (i32.eqz (local.get $head)) (then (return (i32.const 0) (i32.const 0))))
  (memory.fill (local.get $head) (i32.const 255) (i32.const 131072))  ;; -1 = never seen
  (drop (call $__rt_z_wbits (i32.const 1) (i32.const 1)))          ;; BFINAL
  (drop (call $__rt_z_wbits (i32.const 1) (i32.const 2)))          ;; BTYPE = 01, fixed Huffman
  (local.set $prev_len (i32.const 2))                              ;; MIN_MATCH - 1
  (block $done (loop $scan
    (br_if $done (i32.ge_u (local.get $pos) (local.get $len)))
    (local.set $cand (i32.const -1))
    (if (i32.le_u (i32.add (local.get $pos) (i32.const 3)) (local.get $len)) (then
      (local.set $h (call $__rt_z_hash (local.get $ptr) (local.get $pos)))
      (local.set $cand (i32.load (i32.add (local.get $head) (i32.shl (local.get $h) (i32.const 2)))))
      (i32.store (i32.add (local.get $head) (i32.shl (local.get $h) (i32.const 2))) (local.get $pos))))
    (local.set $cur_len (i32.const 2))
    (local.set $cur_match (i32.const 0))
    ;; zlib stops looking once the held match is already long enough (max_lazy_match at level 6)
    (if (i32.and (i32.and (i32.ge_s (local.get $cand) (i32.const 0))
                          (i32.lt_u (local.get $prev_len) (i32.const 16)))
                 (i32.le_u (i32.sub (local.get $pos) (local.get $cand)) (i32.const 32768)))
      (then
        (local.set $limit (i32.sub (local.get $len) (local.get $pos)))
        (if (i32.gt_u (local.get $limit) (i32.const 258))
          (then (local.set $limit (i32.const 258))))
        (local.set $m (i32.const 0))
        (block $stopext (loop $extend
          (br_if $stopext (i32.ge_u (local.get $m) (local.get $limit)))
          (br_if $stopext (i32.ne
            (i32.load8_u (i32.add (local.get $ptr) (i32.add (local.get $cand) (local.get $m))))
            (i32.load8_u (i32.add (local.get $ptr) (i32.add (local.get $pos) (local.get $m))))))
          (local.set $m (i32.add (local.get $m) (i32.const 1)))
          (br $extend)))
        (if (i32.ge_u (local.get $m) (i32.const 3)) (then
          (local.set $cur_len (local.get $m))
          (local.set $cur_match (local.get $cand))))
        ;; TOO_FAR: a bare three-byte match this far back is not worth its own code
        (if (i32.and (i32.eq (local.get $cur_len) (i32.const 3))
                     (i32.gt_u (i32.sub (local.get $pos) (local.get $cur_match)) (i32.const 4096)))
          (then (local.set $cur_len (i32.const 2))))))
    (if (i32.and (i32.ge_u (local.get $prev_len) (i32.const 3))
                 (i32.le_u (local.get $cur_len) (local.get $prev_len)))
      (then
        ;; the held match wins: it started one position back
        (call $__rt_z_emit_match (local.get $prev_len)
          (i32.sub (i32.sub (local.get $pos) (i32.const 1)) (local.get $prev_match)))
        (local.set $stop (i32.add (i32.sub (local.get $pos) (i32.const 1)) (local.get $prev_len)))
        (if (i32.gt_u (local.get $stop) (i32.sub (local.get $len) (i32.const 2)))
          (then (local.set $stop (i32.sub (local.get $len) (i32.const 2)))))
        (local.set $i (i32.add (local.get $pos) (i32.const 1)))
        (block $hashed (loop $hash
          (br_if $hashed (i32.ge_s (local.get $i) (local.get $stop)))
          (i32.store (i32.add (local.get $head)
            (i32.shl (call $__rt_z_hash (local.get $ptr) (local.get $i)) (i32.const 2)))
            (local.get $i))
          (local.set $i (i32.add (local.get $i) (i32.const 1)))
          (br $hash)))
        (local.set $pos (i32.add (i32.sub (local.get $pos) (i32.const 1)) (local.get $prev_len)))
        (local.set $prev_len (i32.const 2))
        (local.set $available (i32.const 0)))
      (else
        (if (local.get $available) (then
          (drop (call $__rt_z_literal
            (i32.load8_u (i32.add (local.get $ptr) (i32.sub (local.get $pos) (i32.const 1)))))))
          (else (local.set $available (i32.const 1))))
        (local.set $prev_len (local.get $cur_len))
        (local.set $prev_match (local.get $cur_match))
        (local.set $pos (i32.add (local.get $pos) (i32.const 1)))))
    (br $scan)))
  ;; a match held at the very end has no next position to lose to, so its first byte is a literal
  (if (local.get $available)
    (then (drop (call $__rt_z_literal
      (i32.load8_u (i32.add (local.get $ptr) (i32.sub (local.get $len) (i32.const 1))))))))
  (drop (call $__rt_z_whuff (i32.const 0) (i32.const 7)))           ;; symbol 256 ends the block
  (if (global.get $__z_bitcnt)
    (then (drop (call $__rt_z_wbits (i32.const 0) (i32.sub (i32.const 8) (global.get $__z_bitcnt))))))
  (call $__rt_heap_free (local.get $head))
  (global.get $__z_out) (global.get $__z_outlen))
"#;

/// `__rt_adler32`: the checksum zlib's wrapper carries, over the UNCOMPRESSED bytes.
const RT_ADLER32: &str = r#"(func $__rt_adler32 (param $ptr i32) (param $len i32) (result i32)
  (local $a i32) (local $b i32) (local $i i32)
  (local.set $a (i32.const 1))
  (block $done (loop $walk
    (br_if $done (i32.ge_u (local.get $i) (local.get $len)))
    (local.set $a (i32.rem_u
      (i32.add (local.get $a) (i32.load8_u (i32.add (local.get $ptr) (local.get $i))))
      (i32.const 65521)))
    (local.set $b (i32.rem_u (i32.add (local.get $b) (local.get $a)) (i32.const 65521)))
    (local.set $i (i32.add (local.get $i) (i32.const 1)))
    (br $walk)))
  (i32.or (i32.shl (local.get $b) (i32.const 16)) (local.get $a)))
"#;

/// `__rt_gzdeflate`: raw DEFLATE, as a persisted string.
const RT_GZDEFLATE: &str = r#"(func $__rt_gzdeflate (param $ptr i32) (param $len i64) (result i32 i64)
  (local $out i32) (local $n i32) (local $sptr i32) (local $slen i64)
  (call $__rt_deflate (local.get $ptr) (i32.wrap_i64 (local.get $len)))
  (local.set $n)
  (local.set $out)
  (call $__rt_str_persist (local.get $out) (i64.extend_i32_u (local.get $n)))
  (local.set $slen)
  (local.set $sptr)
  (call $__rt_heap_free (local.get $out))
  (local.get $sptr) (local.get $slen))
"#;

/// `__rt_gzinflate`: raw INFLATE, boxed as php's `string|false`.
const RT_GZINFLATE: &str = r#"(func $__rt_gzinflate (param $ptr i32) (param $len i64) (result i32)
  (local $out i32) (local $n i32) (local $ok i32) (local $sptr i32) (local $slen i64)
  (call $__rt_inflate (local.get $ptr) (i32.wrap_i64 (local.get $len)))
  (local.set $ok)
  (local.set $n)
  (local.set $out)
  (if (i32.eqz (local.get $ok))
    (then (return (call $__rt_mixed_from_value (i64.const 3) (i64.const 0) (i64.const 0)))))
  (call $__rt_str_persist (local.get $out) (i64.extend_i32_u (local.get $n)))
  (local.set $slen)
  (local.set $sptr)
  (call $__rt_heap_free (local.get $out))
  (call $__rt_mixed_from_value (i64.const 1)
    (i64.extend_i32_u (local.get $sptr)) (local.get $slen)))
"#;

/// `__rt_gzcompress`: zlib's wrapper — a two-byte header, the DEFLATE stream, and Adler-32.
///
/// The header bytes are `0x78 0x9c`: CM 8 with a 32 KiB window, and the check byte that makes
/// the pair a multiple of 31 at the default compression level. zlib writes the same two for a
/// level-6 stream, which is what php's default produces.
const RT_GZCOMPRESS: &str = r#"(func $__rt_gzcompress (param $ptr i32) (param $len i64) (result i32 i64)
  (local $body i32) (local $n i32) (local $out i32) (local $adler i32)
  (local $sptr i32) (local $slen i64)
  (call $__rt_deflate (local.get $ptr) (i32.wrap_i64 (local.get $len)))
  (local.set $n)
  (local.set $body)
  (local.set $out (call $__rt_heap_alloc (i32.add (local.get $n) (i32.const 6))))
  (if (i32.eqz (local.get $out)) (then
    (call $__rt_heap_free (local.get $body))
    (return (call $__rt_str_persist (i32.const 0) (i64.const 0)))))
  (i32.store8 (local.get $out) (i32.const 120))                     ;; 0x78
  (i32.store8 offset=1 (local.get $out) (i32.const 156))            ;; 0x9c
  (memory.copy (i32.add (local.get $out) (i32.const 2)) (local.get $body) (local.get $n))
  (local.set $adler (call $__rt_adler32 (local.get $ptr) (i32.wrap_i64 (local.get $len))))
  ;; the checksum is big-endian, unlike every other multi-byte field in the format
  (i32.store8 (i32.add (local.get $out) (i32.add (local.get $n) (i32.const 2)))
              (i32.shr_u (local.get $adler) (i32.const 24)))
  (i32.store8 (i32.add (local.get $out) (i32.add (local.get $n) (i32.const 3)))
              (i32.shr_u (local.get $adler) (i32.const 16)))
  (i32.store8 (i32.add (local.get $out) (i32.add (local.get $n) (i32.const 4)))
              (i32.shr_u (local.get $adler) (i32.const 8)))
  (i32.store8 (i32.add (local.get $out) (i32.add (local.get $n) (i32.const 5)))
              (local.get $adler))
  (call $__rt_str_persist (local.get $out) (i64.extend_i32_u (i32.add (local.get $n) (i32.const 6))))
  (local.set $slen)
  (local.set $sptr)
  (call $__rt_heap_free (local.get $body))
  (call $__rt_heap_free (local.get $out))
  (local.get $sptr) (local.get $slen))
"#;

/// `__rt_gzuncompress`: the zlib wrapper checked and stripped, then INFLATE.
///
/// The header is validated the way the format specifies — CM must be 8, the two bytes together
/// must be a multiple of 31, and no preset dictionary — so a string that is not a zlib stream
/// answers `false` rather than being decoded as if it were one.
const RT_GZUNCOMPRESS: &str = r#"(func $__rt_gzuncompress (param $ptr i32) (param $len i64) (result i32)
  (local $n i32) (local $cmf i32) (local $flg i32)
  (local $out i32) (local $olen i32) (local $ok i32) (local $sptr i32) (local $slen i64)
  (local.set $n (i32.wrap_i64 (local.get $len)))
  (if (i32.lt_u (local.get $n) (i32.const 6))
    (then (return (call $__rt_mixed_from_value (i64.const 3) (i64.const 0) (i64.const 0)))))
  (local.set $cmf (i32.load8_u (local.get $ptr)))
  (local.set $flg (i32.load8_u offset=1 (local.get $ptr)))
  (if (i32.or (i32.or
        (i32.ne (i32.and (local.get $cmf) (i32.const 15)) (i32.const 8))
        (i32.ne (i32.rem_u (i32.add (i32.shl (local.get $cmf) (i32.const 8)) (local.get $flg))
                           (i32.const 31)) (i32.const 0)))
        (i32.and (local.get $flg) (i32.const 32)))                  ;; FDICT: a preset dictionary
    (then (return (call $__rt_mixed_from_value (i64.const 3) (i64.const 0) (i64.const 0)))))
  (call $__rt_inflate (i32.add (local.get $ptr) (i32.const 2))
                      (i32.sub (local.get $n) (i32.const 6)))
  (local.set $ok)
  (local.set $olen)
  (local.set $out)
  (if (i32.eqz (local.get $ok))
    (then (return (call $__rt_mixed_from_value (i64.const 3) (i64.const 0) (i64.const 0)))))
  (call $__rt_str_persist (local.get $out) (i64.extend_i32_u (local.get $olen)))
  (local.set $slen)
  (local.set $sptr)
  (call $__rt_heap_free (local.get $out))
  (call $__rt_mixed_from_value (i64.const 1)
    (i64.extend_i32_u (local.get $sptr)) (local.get $slen)))
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::RuntimeFnId;

    /// The four zlib identities are served, and the tables they read are RFC 1951's.
    ///
    /// The tables are the part a typo would corrupt silently: a wrong length base produces a
    /// stream that still decodes, just into different bytes, and only a round trip against a
    /// real decompressor would notice. So the boundaries are pinned here — the first and last
    /// entry of each table, and the property that makes them a partition of the length and
    /// distance ranges.
    #[test]
    fn the_deflate_tables_are_the_format_s() {
        for target in [
            RuntimeFnId::Gzdeflate,
            RuntimeFnId::Gzinflate,
            RuntimeFnId::Gzcompress,
            RuntimeFnId::Gzuncompress,
        ] {
            assert!(
                crate::codegen_wasm::capability::runtime_function_is_supported(target),
                "{target:?} must be supported on this target"
            );
        }

        assert_eq!(LENGTH_BASE.len(), 29, "length codes 257..285");
        assert_eq!(LENGTH_EXTRA.len(), 29);
        assert_eq!(DIST_BASE.len(), 30, "distance codes 0..29");
        assert_eq!(DIST_EXTRA.len(), 30);
        assert_eq!(CLEN_ORDER.len(), 19);

        assert_eq!(LENGTH_BASE[0], 3, "the shortest match DEFLATE encodes");
        assert_eq!(LENGTH_BASE[28], 258, "the longest, and it takes no extra bits");
        assert_eq!(LENGTH_EXTRA[28], 0);
        assert_eq!(DIST_BASE[0], 1);
        assert_eq!(DIST_BASE[29], 24577);
        assert_eq!(DIST_BASE[29] + (1 << DIST_EXTRA[29]) - 1, 32768, "the window");

        // Each base plus its extra range must reach exactly the next base: the codes partition
        // the lengths 3..=258 and the distances 1..=32768 with no gap and no overlap.
        for index in 0..27 {
            assert_eq!(
                LENGTH_BASE[index] + (1 << LENGTH_EXTRA[index]),
                LENGTH_BASE[index + 1],
                "length code {} does not meet the next one", index + 257
            );
        }
        for index in 0..29 {
            assert_eq!(
                DIST_BASE[index] + (1 << DIST_EXTRA[index]),
                DIST_BASE[index + 1],
                "distance code {index} does not meet the next one"
            );
        }

        // The code-length order is a permutation of 0..=18, which is what makes a dynamic
        // block's header readable at all.
        let mut seen = CLEN_ORDER.to_vec();
        seen.sort_unstable();
        assert_eq!(seen, (0u8..19).collect::<Vec<_>>());
    }
}
