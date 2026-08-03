//! Purpose:
//! Lowers PHP builtin functions that compile to a handful of WebAssembly instructions with no
//! runtime helper and no allocation, plus the audit contract each of them is admitted under.
//!
//! Called from:
//! - `crate::codegen_wasm::inst::lower_runtime_call` for emission.
//! - `crate::codegen_wasm::capability::runtime_function_shape_issue` for the static audit.
//!
//! Key details:
//! - Everything here is an EXACT identity: the WebAssembly instruction and the PHP builtin agree
//!   on every input including NaN, both infinities and negative zero, so there is no diagnostic
//!   to emit and no profile to branch on. A builtin that needs a table, an allocation or a
//!   warning does not belong in this module.
//! - The audit and the emitter read the same operand contract, so a shape the emitter cannot
//!   lower is refused before planning rather than producing an invalid module.

use super::context::{FnCtx, Result};
use super::wat::WatModule;
use super::inst::{operand, store_result};
use super::WasmError;
use crate::ir::{
    Function, Immediate, Instruction, IrHeapKind, IrType, Module, Op, RuntimeFnId,
    UnaryStringRuntime,
};
use crate::types::PhpType;

/// Registers the WAT helpers the builtins in this module call.
///
/// Emitted for every module: none of these touch WASI directly. `has_main` selects whether
/// `chr`/`ord` can reach the PHP 8.5 deprecation helpers, which do — a reactor gets the same
/// answer with no diagnostic rather than a reference it cannot resolve.
pub(super) fn emit_builtin_runtime(wm: &mut WatModule, has_main: bool) {
    let diagnoses = has_main
        && matches!(
            crate::codegen_support::compile_php_version(),
            crate::web_prelude::PhpVersion::Php85
        );
    let (chr, ord) = str_chr_ord(diagnoses);
    wm.add_raw_func(&chr);
    wm.add_raw_func(&ord);
    wm.add_raw_func(RT_STR_REGION_EQ);
    wm.add_raw_func(RT_STR_CONTAINS);
    wm.add_raw_func(RT_STR_MAP_CASE);
    wm.add_raw_func(RT_STR_REVERSE);
    wm.add_raw_func(RT_ROUND);
    wm.add_raw_func(RT_STR_ALLOC);
    wm.add_raw_func(RT_STR_BIN2HEX);
    wm.add_raw_func(RT_STR_ADDSLASHES);
    wm.add_raw_func(RT_STR_STRIPSLASHES);
    wm.add_raw_func(RT_STR_NL2BR);
    wm.add_raw_func(RT_HEX_DIGIT_VALUE);
    wm.add_raw_func(RT_STR_URL_ENCODE);
    wm.add_raw_func(RT_STR_URL_DECODE);
    wm.add_raw_func(RT_B64_CHAR);
    wm.add_raw_func(RT_B64_VALUE);
    wm.add_raw_func(RT_STR_BASE64_ENCODE);
    wm.add_raw_func(RT_STR_BASE64_DECODE);
    wm.add_raw_func(RT_STR_CASE_EDGE);
    wm.add_raw_func(RT_STR_UCWORDS);
    wm.add_raw_func(RT_STR_CMP);
    wm.add_raw_func(RT_TRIM_MATCHES);
    wm.add_raw_func(RT_STR_TRIM);
    wm.add_raw_func(RT_STR_SUBSTR);
    wm.add_raw_func(RT_STR_REPEAT);
    wm.add_raw_func(RT_STR_FIND);
    wm.add_raw_func(RT_STR_RFIND);
    wm.add_raw_func(RT_IMPLODE);
    if has_main {
        // Compares through `__rt_str_loose_eq`, which lives in the command-only numeric runtime
        // because its diagnostics write to stderr. A reactor module has no such scan to make.
        wm.add_raw_func(RT_ARRAY_FIND_STR);
        // `__rt_sort_string` orders by `__rt_str_smart_cmp`, which lives in that same
        // command-only numeric runtime.
        wm.add_raw_func(super::arrays::RT_SORT_STRING);
    }
    wm.add_raw_func(RT_IMPLODE_OWNED);
    wm.add_raw_func(RT_EXPLODE);
    wm.add_raw_func(RT_STR_SPLIT);
    wm.add_raw_func(RT_WORDWRAP_GENERAL);
    wm.add_raw_func(RT_WORDWRAP);
    wm.add_raw_func(RT_FMT_PAD);
    wm.add_raw_func(RT_FMT_INT);
    wm.add_raw_func(RT_FMT_UINT_RADIX);
    wm.add_raw_func(RT_FMT_STR);
    wm.add_raw_func(RT_FMT_FLOAT);
    wm.add_raw_func(RT_PAD_BYTE);
    wm.add_raw_func(RT_STR_PAD);
    wm.add_raw_func(RT_STR_REPLACE);
    wm.add_raw_func(RT_CRC32);
    wm.add_raw_func(RT_SHA1_HEX);
    wm.add_raw_func(RT_MD5_K);
    wm.add_raw_func(RT_MD5_S);
    wm.add_raw_func(RT_MD5_HEX);
    wm.add_raw_func(RT_UTF8_SEQ_LEN);
    wm.add_raw_func(RT_UTF8_BAD_SPAN);
    wm.add_raw_func(RT_HTML_PUT);
    wm.add_raw_func(RT_HTMLSPECIALCHARS);
}

/// `__rt_str_map_case`: owns a copy of a string with its ASCII letters case-mapped.
///
/// `$upper` selects the direction. Since PHP 8.2 `strtoupper` and `strtolower` are
/// LOCALE-INDEPENDENT and touch `A-Z` / `a-z` only — byte `0xE9` comes back unchanged, which is
/// what makes a pure byte map correct here rather than an approximation.
const RT_STR_MAP_CASE: &str = r#"(func $__rt_str_map_case (param $ptr i32) (param $len i64) (param $upper i32) (result i32) (result i64)
  (local $out i32)
  (local $olen i64)
  (local $i i64)
  (local $byte i32)
  (call $__rt_str_persist (local.get $ptr) (local.get $len))      ;; own a copy to transform in place
  (local.set $olen)                                               ;; persisted length
  (local.set $out)                                                ;; persisted pointer
  (local.set $i (i64.const 0))                                    ;; i = 0
  (block $end (loop $map
    (br_if $end (i64.ge_s (local.get $i) (local.get $olen)))      ;; every byte visited
    (local.set $byte (i32.load8_u (i32.add (local.get $out) (i32.wrap_i64 (local.get $i)))))
    (if (local.get $upper)
      (then
        (if (i32.and (i32.ge_u (local.get $byte) (i32.const 97)) (i32.le_u (local.get $byte) (i32.const 122)))
          (then (i32.store8 (i32.add (local.get $out) (i32.wrap_i64 (local.get $i)))
                            (i32.sub (local.get $byte) (i32.const 32))))))  ;; a-z -> A-Z
      (else
        (if (i32.and (i32.ge_u (local.get $byte) (i32.const 65)) (i32.le_u (local.get $byte) (i32.const 90)))
          (then (i32.store8 (i32.add (local.get $out) (i32.wrap_i64 (local.get $i)))
                            (i32.add (local.get $byte) (i32.const 32)))))))  ;; A-Z -> a-z
    (local.set $i (i64.add (local.get $i) (i64.const 1)))         ;; i++
    (br $map)))
  (local.get $out) (local.get $olen))                             ;; owned result
"#;

/// `__rt_str_reverse`: owns a byte-reversed copy of a string.
///
/// `strrev` operates on BYTES, not characters, so a multi-byte sequence comes back with its
/// bytes in reverse order — which is what PHP does.
const RT_STR_REVERSE: &str = r#"(func $__rt_str_reverse (param $ptr i32) (param $len i64) (result i32) (result i64)
  (local $out i32)
  (local $olen i64)
  (local $i i64)
  (call $__rt_str_persist (local.get $ptr) (local.get $len))      ;; own a copy sized like the source
  (local.set $olen)                                               ;; persisted length
  (local.set $out)                                                ;; persisted pointer
  (local.set $i (i64.const 0))                                    ;; i = 0
  (block $end (loop $rev
    (br_if $end (i64.ge_s (local.get $i) (local.get $olen)))      ;; every byte placed
    (i32.store8
      (i32.add (local.get $out) (i32.wrap_i64 (local.get $i)))
      (i32.load8_u (i32.add (local.get $ptr)
                            (i32.wrap_i64 (i64.sub (i64.sub (local.get $olen) (i64.const 1)) (local.get $i))))))
    (local.set $i (i64.add (local.get $i) (i64.const 1)))         ;; i++
    (br $rev)))
  (local.get $out) (local.get $olen))                             ;; owned result
"#;

/// `__rt_round`: PHP's `round()` at precision 0 — half away from ZERO.
///
/// WebAssembly's `f64.nearest` is half-to-EVEN, so it answers 2 for `round(2.5)` where PHP answers
/// 3. The naive repair, `floor(|x| + 0.5)`, is worse: the addition is inexact, so
/// `round(0.49999999999999994)` answers 1 where PHP answers 0, and above 2^52 it perturbs values
/// that are already integers.
///
/// Comparing against the TRUNCATED part instead is exact — `x - trunc(x)` loses nothing for
/// |x| < 2^53, and above it the difference is zero because x is already an integer. `f64.trunc`
/// also carries the sign of zero, which PHP prints: `round(-0.4)` is `-0`.
///
/// Verified on 104 values against php-src 8.5.6, including both halfway directions, the
/// 0.49999999999999994 trap, the 2^52/2^53 boundaries, infinities and NaN.
const RT_ROUND: &str = r#"(func $__rt_round (param $x f64) (result f64)
  (local $t f64)
  (if (f64.ne (local.get $x) (local.get $x))                ;; NaN rounds to itself
    (then (return (local.get $x))))
  (if (f64.eq (f64.abs (local.get $x)) (f64.const inf))     ;; so does an infinity
    (then (return (local.get $x))))
  (local.set $t (f64.trunc (local.get $x)))                 ;; keeps the sign of zero
  (if (f64.ge (f64.abs (f64.sub (local.get $x) (local.get $t))) (f64.const 0.5))
    (then (return (f64.add (local.get $t) (f64.copysign (f64.const 1) (local.get $x))))))
  (local.get $t))
"#;

/// `__rt_str_alloc`: reserves an owned kind-1 string block of `bytes` capacity.
///
/// A re-encoding transform cannot size its result before it runs, so it reserves the worst case
/// here and returns the length it actually produced. The block stays at its reserved size, which
/// costs slack rather than correctness: a PHP string is the `(ptr, len)` pair, and the header the
/// release path reads is the reservation. `__rt_checked_layout` is what rejects a negative or
/// wasm32-overflowing size before the allocation rather than after.
const RT_STR_ALLOC: &str = r#"(func $__rt_str_alloc (param $bytes i64) (result i32)
  (local $new i32)                                                ;; reserved block
  (local.set $new
    (call $__rt_heap_alloc
      (call $__rt_checked_layout
        (local.get $bytes)
        (i64.const 1)
        (i64.const 0))))                                          ;; checked byte count -> block
  (i64.store (i32.sub (local.get $new) (i32.const 8)) (i64.const 1)) ;; stamp header kind = 1 (string)
  (local.get $new))                                               ;; reserved string block
"#;

/// `__rt_str_bin2hex`: owns the lowercase hex expansion of a string's bytes.
///
/// `bin2hex` is total and exactly doubles the length: every byte becomes its high then low
/// nibble as `0-9a-f`. The digit map is arithmetic rather than a table because `'a' - 10` is 87.
const RT_STR_BIN2HEX: &str = r#"(func $__rt_str_bin2hex (param $ptr i32) (param $len i64) (result i32) (result i64)
  (local $out i32)                                                ;; owned result block
  (local $i i64)                                                  ;; source cursor
  (local $byte i32)                                               ;; current source byte
  (local $nib i32)                                                ;; nibble being written
  (local $w i32)                                                  ;; destination cursor
  (local.set $out (call $__rt_str_alloc (i64.mul (local.get $len) (i64.const 2))))
  (local.set $i (i64.const 0))                                    ;; i = 0
  (local.set $w (i32.const 0))                                    ;; w = 0
  (block $end (loop $hex
    (br_if $end (i64.ge_s (local.get $i) (local.get $len)))       ;; every byte expanded
    (local.set $byte (i32.load8_u (i32.add (local.get $ptr) (i32.wrap_i64 (local.get $i)))))
    (local.set $nib (i32.shr_u (local.get $byte) (i32.const 4)))  ;; high nibble first
    (i32.store8 (i32.add (local.get $out) (local.get $w))
      (i32.add (local.get $nib)
        (select (i32.const 48) (i32.const 87)
                (i32.lt_u (local.get $nib) (i32.const 10)))))     ;; 0-9 else a-f
    (local.set $w (i32.add (local.get $w) (i32.const 1)))         ;; w++
    (local.set $nib (i32.and (local.get $byte) (i32.const 15)))   ;; low nibble
    (i32.store8 (i32.add (local.get $out) (local.get $w))
      (i32.add (local.get $nib)
        (select (i32.const 48) (i32.const 87)
                (i32.lt_u (local.get $nib) (i32.const 10)))))     ;; 0-9 else a-f
    (local.set $w (i32.add (local.get $w) (i32.const 1)))         ;; w++
    (local.set $i (i64.add (local.get $i) (i64.const 1)))         ;; i++
    (br $hex)))
  (local.get $out) (i64.extend_i32_u (local.get $w)))             ;; owned result
"#;

/// `__rt_str_addslashes`: owns a copy with PHP's four escaped bytes backslash-prefixed.
///
/// `addslashes` escapes exactly `'`, `"`, `\` and NUL, and NUL becomes the two characters
/// `\0` rather than a backslash plus a zero byte — measured against php-src, where
/// `"\x00"` comes back as the bytes `5c 30`. Every other byte, including UTF-8 continuation
/// bytes, passes through untouched. Worst case is two output bytes per input byte.
const RT_STR_ADDSLASHES: &str = r#"(func $__rt_str_addslashes (param $ptr i32) (param $len i64) (result i32) (result i64)
  (local $out i32)                                                ;; owned result block
  (local $i i64)                                                  ;; source cursor
  (local $byte i32)                                               ;; current source byte
  (local $w i32)                                                  ;; destination cursor
  (local.set $out (call $__rt_str_alloc (i64.mul (local.get $len) (i64.const 2))))
  (local.set $i (i64.const 0))                                    ;; i = 0
  (local.set $w (i32.const 0))                                    ;; w = 0
  (block $end (loop $esc
    (br_if $end (i64.ge_s (local.get $i) (local.get $len)))       ;; every byte examined
    (local.set $byte (i32.load8_u (i32.add (local.get $ptr) (i32.wrap_i64 (local.get $i)))))
    (if (i32.or
          (i32.eqz (local.get $byte))
          (i32.or
            (i32.eq (local.get $byte) (i32.const 39))
            (i32.or
              (i32.eq (local.get $byte) (i32.const 34))
              (i32.eq (local.get $byte) (i32.const 92)))))        ;; NUL, ' , " or backslash
      (then
        (i32.store8 (i32.add (local.get $out) (local.get $w)) (i32.const 92))  ;; leading backslash
        (local.set $w (i32.add (local.get $w) (i32.const 1)))     ;; w++
        (i32.store8 (i32.add (local.get $out) (local.get $w))
          (select (i32.const 48) (local.get $byte)
                  (i32.eqz (local.get $byte))))                   ;; NUL escapes to the digit zero
        (local.set $w (i32.add (local.get $w) (i32.const 1))))    ;; w++
      (else
        (i32.store8 (i32.add (local.get $out) (local.get $w)) (local.get $byte))
        (local.set $w (i32.add (local.get $w) (i32.const 1)))))   ;; w++
    (local.set $i (i64.add (local.get $i) (i64.const 1)))         ;; i++
    (br $esc)))
  (local.get $out) (i64.extend_i32_u (local.get $w)))             ;; owned result
"#;

/// `__rt_str_stripslashes`: owns a copy with one level of backslash escaping removed.
///
/// It is NOT the inverse of a C unescape: `\n` yields the letter `n`, not a newline. Only `\0`
/// is special, producing a NUL byte. A backslash consumes the byte after it whatever that is
/// (so `\\` yields one backslash), and a trailing lone backslash is dropped. Measured against
/// php-src, including `\\0` yielding a backslash followed by the digit zero.
const RT_STR_STRIPSLASHES: &str = r#"(func $__rt_str_stripslashes (param $ptr i32) (param $len i64) (result i32) (result i64)
  (local $out i32)                                                ;; owned result block
  (local $i i64)                                                  ;; source cursor
  (local $byte i32)                                               ;; current source byte
  (local $w i32)                                                  ;; destination cursor
  (local.set $out (call $__rt_str_alloc (local.get $len)))        ;; never grows
  (local.set $i (i64.const 0))                                    ;; i = 0
  (local.set $w (i32.const 0))                                    ;; w = 0
  (block $end (loop $strip
    (br_if $end (i64.ge_s (local.get $i) (local.get $len)))       ;; every byte examined
    (local.set $byte (i32.load8_u (i32.add (local.get $ptr) (i32.wrap_i64 (local.get $i)))))
    (if (i32.eq (local.get $byte) (i32.const 92))                 ;; a backslash escapes what follows
      (then
        (local.set $i (i64.add (local.get $i) (i64.const 1)))     ;; consume the backslash
        (if (i64.lt_s (local.get $i) (local.get $len))            ;; a trailing lone backslash is dropped
          (then
            (local.set $byte (i32.load8_u (i32.add (local.get $ptr) (i32.wrap_i64 (local.get $i)))))
            (i32.store8 (i32.add (local.get $out) (local.get $w))
              (select (i32.const 0) (local.get $byte)
                      (i32.eq (local.get $byte) (i32.const 48)))) ;; \0 alone becomes a NUL byte
            (local.set $w (i32.add (local.get $w) (i32.const 1))) ;; w++
            (local.set $i (i64.add (local.get $i) (i64.const 1)))))) ;; consume the escaped byte
      (else
        (i32.store8 (i32.add (local.get $out) (local.get $w)) (local.get $byte))
        (local.set $w (i32.add (local.get $w) (i32.const 1)))     ;; w++
        (local.set $i (i64.add (local.get $i) (i64.const 1)))))   ;; i++
    (br $strip)))
  (local.get $out) (i64.extend_i32_u (local.get $w)))             ;; owned result
"#;

/// `__rt_str_nl2br`: owns a copy with `<br />` inserted BEFORE each line break.
///
/// The break itself is kept, which is what `nl2br` does — it inserts rather than replaces. A
/// `\r\n` or `\n\r` pair counts as ONE break and both bytes survive after the single tag;
/// `\n\n` is two breaks. Measured against php-src, whose default XHTML form is `<br />`.
/// Worst case is seven output bytes per input byte, when every byte is a lone break.
const RT_STR_NL2BR: &str = r#"(func $__rt_str_nl2br (param $ptr i32) (param $len i64) (result i32) (result i64)
  (local $out i32)                                                ;; owned result block
  (local $i i64)                                                  ;; source cursor
  (local $byte i32)                                               ;; current source byte
  (local $next i32)                                               ;; byte after a line break
  (local $w i32)                                                  ;; destination cursor
  (local.set $out (call $__rt_str_alloc (i64.mul (local.get $len) (i64.const 7))))
  (local.set $i (i64.const 0))                                    ;; i = 0
  (local.set $w (i32.const 0))                                    ;; w = 0
  (block $end (loop $scan
    (br_if $end (i64.ge_s (local.get $i) (local.get $len)))       ;; every byte examined
    (local.set $byte (i32.load8_u (i32.add (local.get $ptr) (i32.wrap_i64 (local.get $i)))))
    (if (i32.or (i32.eq (local.get $byte) (i32.const 10))
                (i32.eq (local.get $byte) (i32.const 13)))        ;; a line feed or carriage return
      (then
        (i32.store8 (i32.add (local.get $out) (local.get $w)) (i32.const 60))   ;; <
        (i32.store8 offset=1 (i32.add (local.get $out) (local.get $w)) (i32.const 98))  ;; b
        (i32.store8 offset=2 (i32.add (local.get $out) (local.get $w)) (i32.const 114)) ;; r
        (i32.store8 offset=3 (i32.add (local.get $out) (local.get $w)) (i32.const 32))  ;; space
        (i32.store8 offset=4 (i32.add (local.get $out) (local.get $w)) (i32.const 47))  ;; /
        (i32.store8 offset=5 (i32.add (local.get $out) (local.get $w)) (i32.const 62))  ;; >
        (local.set $w (i32.add (local.get $w) (i32.const 6)))     ;; the six tag bytes
        (i32.store8 (i32.add (local.get $out) (local.get $w)) (local.get $byte)) ;; keep the break
        (local.set $w (i32.add (local.get $w) (i32.const 1)))     ;; w++
        (local.set $i (i64.add (local.get $i) (i64.const 1)))     ;; i++
        (if (i64.lt_s (local.get $i) (local.get $len))
          (then
            (local.set $next (i32.load8_u (i32.add (local.get $ptr) (i32.wrap_i64 (local.get $i)))))
            (if (i32.and
                  (i32.or (i32.eq (local.get $next) (i32.const 10))
                          (i32.eq (local.get $next) (i32.const 13)))
                  (i32.ne (local.get $next) (local.get $byte)))   ;; the OTHER break byte pairs with it
              (then
                (i32.store8 (i32.add (local.get $out) (local.get $w)) (local.get $next))
                (local.set $w (i32.add (local.get $w) (i32.const 1)))  ;; w++
                (local.set $i (i64.add (local.get $i) (i64.const 1)))))))) ;; the pair is one break
      (else
        (i32.store8 (i32.add (local.get $out) (local.get $w)) (local.get $byte))
        (local.set $w (i32.add (local.get $w) (i32.const 1)))     ;; w++
        (local.set $i (i64.add (local.get $i) (i64.const 1)))))   ;; i++
    (br $scan)))
  (local.get $out) (i64.extend_i32_u (local.get $w)))             ;; owned result
"#;

/// `__rt_hex_digit_value`: the value of one ASCII hex digit, or -1 for any other byte.
///
/// Both cases are accepted, which is what makes `urldecode("%aB")` and `urldecode("%Ab")` agree
/// with php-src. The -1 sentinel is what lets a caller distinguish "not hex" from the digit zero.
const RT_HEX_DIGIT_VALUE: &str = r#"(func $__rt_hex_digit_value (param $c i32) (result i32)
  (if (i32.and (i32.ge_u (local.get $c) (i32.const 48))
               (i32.le_u (local.get $c) (i32.const 57)))
    (then (return (i32.sub (local.get $c) (i32.const 48)))))     ;; 0-9
  (if (i32.and (i32.ge_u (local.get $c) (i32.const 65))
               (i32.le_u (local.get $c) (i32.const 70)))
    (then (return (i32.sub (local.get $c) (i32.const 55)))))     ;; A-F
  (if (i32.and (i32.ge_u (local.get $c) (i32.const 97))
               (i32.le_u (local.get $c) (i32.const 102)))
    (then (return (i32.sub (local.get $c) (i32.const 87)))))     ;; a-f
  (i32.const -1))                                                ;; not a hex digit
"#;

/// `__rt_str_url_encode`: owns the percent-encoded form of a string.
///
/// `$raw` selects `rawurlencode` over `urlencode`. Measured over all 256 bytes against php-src:
/// both leave `A-Z a-z 0-9 - . _` alone, `rawurlencode` additionally leaves `~`, and `urlencode`
/// alone maps a space to `+`. Everything else becomes `%` and two UPPERCASE hex digits. Worst
/// case is three output bytes per input byte.
const RT_STR_URL_ENCODE: &str = r#"(func $__rt_str_url_encode (param $ptr i32) (param $len i64) (param $raw i32) (result i32) (result i64)
  (local $out i32)                                                ;; owned result block
  (local $i i64)                                                  ;; source cursor
  (local $byte i32)                                               ;; current source byte
  (local $nib i32)                                                ;; nibble being written
  (local $w i32)                                                  ;; destination cursor
  (local.set $out (call $__rt_str_alloc (i64.mul (local.get $len) (i64.const 3))))
  (local.set $i (i64.const 0))                                    ;; i = 0
  (local.set $w (i32.const 0))                                    ;; w = 0
  (block $end (loop $enc
    (br_if $end (i64.ge_s (local.get $i) (local.get $len)))       ;; every byte examined
    (local.set $byte (i32.load8_u (i32.add (local.get $ptr) (i32.wrap_i64 (local.get $i)))))
    (if (i32.or
          (i32.or
            (i32.and (i32.ge_u (local.get $byte) (i32.const 48))
                     (i32.le_u (local.get $byte) (i32.const 57)))  ;; 0-9
            (i32.or
              (i32.and (i32.ge_u (local.get $byte) (i32.const 65))
                       (i32.le_u (local.get $byte) (i32.const 90)))  ;; A-Z
              (i32.and (i32.ge_u (local.get $byte) (i32.const 97))
                       (i32.le_u (local.get $byte) (i32.const 122))))) ;; a-z
          (i32.or
            (i32.or (i32.eq (local.get $byte) (i32.const 45))
                    (i32.eq (local.get $byte) (i32.const 46)))    ;; - and .
            (i32.or (i32.eq (local.get $byte) (i32.const 95))
                    (i32.and (local.get $raw)
                             (i32.eq (local.get $byte) (i32.const 126)))))) ;; _ and raw-only ~
      (then
        (i32.store8 (i32.add (local.get $out) (local.get $w)) (local.get $byte))
        (local.set $w (i32.add (local.get $w) (i32.const 1))))    ;; unreserved byte passes through
      (else
        (if (i32.and (i32.eqz (local.get $raw))
                     (i32.eq (local.get $byte) (i32.const 32)))   ;; urlencode alone folds space
          (then
            (i32.store8 (i32.add (local.get $out) (local.get $w)) (i32.const 43))  ;; +
            (local.set $w (i32.add (local.get $w) (i32.const 1))))
          (else
            (i32.store8 (i32.add (local.get $out) (local.get $w)) (i32.const 37))  ;; %
            (local.set $w (i32.add (local.get $w) (i32.const 1)))
            (local.set $nib (i32.shr_u (local.get $byte) (i32.const 4)))
            (i32.store8 (i32.add (local.get $out) (local.get $w))
              (i32.add (local.get $nib)
                (select (i32.const 48) (i32.const 55)
                        (i32.lt_u (local.get $nib) (i32.const 10)))))  ;; 0-9 else A-F
            (local.set $w (i32.add (local.get $w) (i32.const 1)))
            (local.set $nib (i32.and (local.get $byte) (i32.const 15)))
            (i32.store8 (i32.add (local.get $out) (local.get $w))
              (i32.add (local.get $nib)
                (select (i32.const 48) (i32.const 55)
                        (i32.lt_u (local.get $nib) (i32.const 10)))))  ;; 0-9 else A-F
            (local.set $w (i32.add (local.get $w) (i32.const 1)))))))
    (local.set $i (i64.add (local.get $i) (i64.const 1)))         ;; i++
    (br $enc)))
  (local.get $out) (i64.extend_i32_u (local.get $w)))             ;; owned result
"#;

/// `__rt_str_url_decode`: owns the percent-decoded form of a string.
///
/// `$plus` selects `urldecode` over `rawurldecode`, which differ only in whether `+` becomes a
/// space. Decoding is TOLERANT and never fails: a `%` without two hex digits after it stays a
/// literal `%`, which is what php-src does for `"a%2"` and `"a%zz"`. Never grows.
const RT_STR_URL_DECODE: &str = r#"(func $__rt_str_url_decode (param $ptr i32) (param $len i64) (param $plus i32) (result i32) (result i64)
  (local $out i32)                                                ;; owned result block
  (local $i i64)                                                  ;; source cursor
  (local $byte i32)                                               ;; current source byte
  (local $hi i32)                                                 ;; high hex digit value
  (local $lo i32)                                                 ;; low hex digit value
  (local $w i32)                                                  ;; destination cursor
  (local.set $out (call $__rt_str_alloc (local.get $len)))        ;; never grows
  (local.set $i (i64.const 0))                                    ;; i = 0
  (local.set $w (i32.const 0))                                    ;; w = 0
  (block $end (loop $dec
    (br_if $end (i64.ge_s (local.get $i) (local.get $len)))       ;; every byte examined
    (local.set $byte (i32.load8_u (i32.add (local.get $ptr) (i32.wrap_i64 (local.get $i)))))
    (local.set $hi (i32.const -1))                                ;; assume no escape here
    (if (i32.and (i32.eq (local.get $byte) (i32.const 37))        ;; a percent
                 (i64.le_s (i64.add (local.get $i) (i64.const 3)) (local.get $len)))
      (then
        (local.set $hi (call $__rt_hex_digit_value
          (i32.load8_u (i32.add (local.get $ptr)
                                (i32.wrap_i64 (i64.add (local.get $i) (i64.const 1)))))))
        (local.set $lo (call $__rt_hex_digit_value
          (i32.load8_u (i32.add (local.get $ptr)
                                (i32.wrap_i64 (i64.add (local.get $i) (i64.const 2)))))))
        (if (i32.or (i32.lt_s (local.get $hi) (i32.const 0))
                    (i32.lt_s (local.get $lo) (i32.const 0)))
          (then (local.set $hi (i32.const -1))))))                ;; not both hex: stay literal
    (if (i32.ge_s (local.get $hi) (i32.const 0))
      (then
        (i32.store8 (i32.add (local.get $out) (local.get $w))
          (i32.or (i32.shl (local.get $hi) (i32.const 4)) (local.get $lo)))
        (local.set $w (i32.add (local.get $w) (i32.const 1)))     ;; w++
        (local.set $i (i64.add (local.get $i) (i64.const 3))))    ;; consume %HH
      (else
        (i32.store8 (i32.add (local.get $out) (local.get $w))
          (select (i32.const 32) (local.get $byte)
                  (i32.and (local.get $plus)
                           (i32.eq (local.get $byte) (i32.const 43))))) ;; urldecode folds + to space
        (local.set $w (i32.add (local.get $w) (i32.const 1)))     ;; w++
        (local.set $i (i64.add (local.get $i) (i64.const 1)))))   ;; i++
    (br $dec)))
  (local.get $out) (i64.extend_i32_u (local.get $w)))             ;; owned result
"#;

/// `__rt_b64_char`: the base64 alphabet character for a 6-bit value.
///
/// Arithmetic rather than a data segment: the four runs are contiguous, so each is one offset.
const RT_B64_CHAR: &str = r#"(func $__rt_b64_char (param $v i32) (result i32)
  (if (i32.lt_u (local.get $v) (i32.const 26))
    (then (return (i32.add (local.get $v) (i32.const 65)))))      ;; 0-25  -> A-Z
  (if (i32.lt_u (local.get $v) (i32.const 52))
    (then (return (i32.add (local.get $v) (i32.const 71)))))      ;; 26-51 -> a-z
  (if (i32.lt_u (local.get $v) (i32.const 62))
    (then (return (i32.sub (local.get $v) (i32.const 4)))))       ;; 52-61 -> 0-9
  (select (i32.const 47) (i32.const 43)
          (i32.eq (local.get $v) (i32.const 63))))                ;; 62 -> + , 63 -> /
"#;

/// `__rt_b64_value`: the 6-bit value of one base64 character, or -1 for any other byte.
///
/// The -1 sentinel is what makes non-strict decoding possible: php-src's one-argument
/// `base64_decode` SKIPS every byte outside the alphabet, padding and whitespace included.
const RT_B64_VALUE: &str = r#"(func $__rt_b64_value (param $c i32) (result i32)
  (if (i32.and (i32.ge_u (local.get $c) (i32.const 65))
               (i32.le_u (local.get $c) (i32.const 90)))
    (then (return (i32.sub (local.get $c) (i32.const 65)))))      ;; A-Z
  (if (i32.and (i32.ge_u (local.get $c) (i32.const 97))
               (i32.le_u (local.get $c) (i32.const 122)))
    (then (return (i32.sub (local.get $c) (i32.const 71)))))      ;; a-z
  (if (i32.and (i32.ge_u (local.get $c) (i32.const 48))
               (i32.le_u (local.get $c) (i32.const 57)))
    (then (return (i32.add (local.get $c) (i32.const 4)))))       ;; 0-9
  (if (i32.eq (local.get $c) (i32.const 43)) (then (return (i32.const 62))))  ;; +
  (if (i32.eq (local.get $c) (i32.const 47)) (then (return (i32.const 63))))  ;; /
  (i32.const -1))                                                 ;; outside the alphabet
"#;

/// `__rt_str_base64_encode`: owns the base64 form of a string, padded to a multiple of four.
///
/// The final group is padded with `=` to a full quartet, which is what php-src emits. Output is
/// exactly four characters per three input bytes, rounded up.
const RT_STR_BASE64_ENCODE: &str = r#"(func $__rt_str_base64_encode (param $ptr i32) (param $len i64) (result i32) (result i64)
  (local $out i32)                                                ;; owned result block
  (local $i i64)                                                  ;; source cursor
  (local $left i64)                                               ;; bytes left in this group
  (local $acc i32)                                                ;; the 24-bit group
  (local $w i32)                                                  ;; destination cursor
  (local.set $out
    (call $__rt_str_alloc
      (i64.mul (i64.div_s (i64.add (local.get $len) (i64.const 2)) (i64.const 3))
               (i64.const 4))))                                   ;; four chars per three bytes
  (local.set $i (i64.const 0))                                    ;; i = 0
  (local.set $w (i32.const 0))                                    ;; w = 0
  (block $end (loop $enc
    (br_if $end (i64.ge_s (local.get $i) (local.get $len)))       ;; every group emitted
    (local.set $left (i64.sub (local.get $len) (local.get $i)))   ;; bytes remaining
    (local.set $acc
      (i32.shl (i32.load8_u (i32.add (local.get $ptr) (i32.wrap_i64 (local.get $i))))
               (i32.const 16)))                                   ;; first byte of the group
    (if (i64.ge_s (local.get $left) (i64.const 2))
      (then (local.set $acc (i32.or (local.get $acc)
        (i32.shl (i32.load8_u (i32.add (local.get $ptr)
                                       (i32.wrap_i64 (i64.add (local.get $i) (i64.const 1)))))
                 (i32.const 8))))))                               ;; second byte when present
    (if (i64.ge_s (local.get $left) (i64.const 3))
      (then (local.set $acc (i32.or (local.get $acc)
        (i32.load8_u (i32.add (local.get $ptr)
                              (i32.wrap_i64 (i64.add (local.get $i) (i64.const 2)))))))))
    (i32.store8 (i32.add (local.get $out) (local.get $w))
      (call $__rt_b64_char (i32.and (i32.shr_u (local.get $acc) (i32.const 18)) (i32.const 63))))
    (i32.store8 offset=1 (i32.add (local.get $out) (local.get $w))
      (call $__rt_b64_char (i32.and (i32.shr_u (local.get $acc) (i32.const 12)) (i32.const 63))))
    (i32.store8 offset=2 (i32.add (local.get $out) (local.get $w))
      (select
        (call $__rt_b64_char (i32.and (i32.shr_u (local.get $acc) (i32.const 6)) (i32.const 63)))
        (i32.const 61)
        (i64.ge_s (local.get $left) (i64.const 2))))              ;; = pads a one-byte tail
    (i32.store8 offset=3 (i32.add (local.get $out) (local.get $w))
      (select
        (call $__rt_b64_char (i32.and (local.get $acc) (i32.const 63)))
        (i32.const 61)
        (i64.ge_s (local.get $left) (i64.const 3))))              ;; = pads a two-byte tail
    (local.set $w (i32.add (local.get $w) (i32.const 4)))         ;; one quartet written
    (local.set $i (i64.add (local.get $i) (i64.const 3)))         ;; next group
    (br $enc)))
  (local.get $out) (i64.extend_i32_u (local.get $w)))             ;; owned result
"#;

/// `__rt_str_base64_decode`: owns the base64-decoded bytes of a string, php-src's tolerant way.
///
/// One-argument `base64_decode` is non-strict: every byte outside the alphabet is SKIPPED rather
/// than rejected, padding, whitespace and punctuation alike, and a trailing group of fewer than
/// eight accumulated bits is discarded. So `"YWJj="`, `"YW Jj"` and `"YWJj\n"` all decode to
/// `abc`, `"YWJ"` decodes to `ab`, and `"!!!!"` and `"a"` both decode to the empty string —
/// measured against php-src. Six bits per input byte means the source length is a safe bound.
const RT_STR_BASE64_DECODE: &str = r#"(func $__rt_str_base64_decode (param $ptr i32) (param $len i64) (result i32) (result i64)
  (local $out i32)                                                ;; owned result block
  (local $i i64)                                                  ;; source cursor
  (local $v i32)                                                  ;; value of the current char
  (local $acc i32)                                                ;; accumulated bits
  (local $bits i32)                                               ;; how many bits are accumulated
  (local $w i32)                                                  ;; destination cursor
  (local.set $out (call $__rt_str_alloc (local.get $len)))        ;; 6 bits in, 8 bits out: never grows
  (local.set $i (i64.const 0))                                    ;; i = 0
  (local.set $w (i32.const 0))                                    ;; w = 0
  (local.set $acc (i32.const 0))                                  ;; empty bit accumulator
  (local.set $bits (i32.const 0))                                 ;; no bits yet
  (block $end (loop $dec
    (br_if $end (i64.ge_s (local.get $i) (local.get $len)))       ;; every byte examined
    (local.set $v (call $__rt_b64_value
      (i32.load8_u (i32.add (local.get $ptr) (i32.wrap_i64 (local.get $i))))))
    (if (i32.ge_s (local.get $v) (i32.const 0))                   ;; anything else is skipped
      (then
        (local.set $acc (i32.or (i32.shl (local.get $acc) (i32.const 6)) (local.get $v)))
        (local.set $bits (i32.add (local.get $bits) (i32.const 6)))
        (if (i32.ge_u (local.get $bits) (i32.const 8))
          (then
            (local.set $bits (i32.sub (local.get $bits) (i32.const 8)))
            (i32.store8 (i32.add (local.get $out) (local.get $w))
              (i32.and (i32.shr_u (local.get $acc) (local.get $bits)) (i32.const 255)))
            (local.set $w (i32.add (local.get $w) (i32.const 1)))))))  ;; a whole byte is ready
    (local.set $i (i64.add (local.get $i) (i64.const 1)))         ;; i++
    (br $dec)))
  (local.get $out) (i64.extend_i32_u (local.get $w)))             ;; leftover bits are discarded
"#;

/// `__rt_str_chr`: owns the one-byte string PHP's `chr` returns for any integer.
///
/// PHP does not reject an out-of-range codepoint: it constrains it with `% 256`, and a NEGATIVE
/// remainder is brought back up by adding 256, so `chr(-1)` is `\xff` and `chr(1000000)` is
/// `\x40` — measured. `$deprecate` carries whether this profile diagnoses the out-of-range
/// argument; the RESULT is the same either way, which is why the flag only gates the message.
const RT_STR_CHR_TEMPLATE: &str = r#"(func $__rt_str_chr (param $n i64) (result i32) (result i64)
  (local $out i32)                                                ;; owned result block
  (local $byte i64)                                               ;; the constrained byte value
{deprecation}  (local.set $byte (i64.rem_s (local.get $n) (i64.const 256)))    ;; % 256 keeps the sign in C
  (if (i64.lt_s (local.get $byte) (i64.const 0))
    (then (local.set $byte (i64.add (local.get $byte) (i64.const 256))))) ;; back into [0, 255]
  (local.set $out (call $__rt_str_alloc (i64.const 1)))           ;; exactly one byte
  (i32.store8 (local.get $out) (i32.wrap_i64 (local.get $byte)))  ;; the byte itself
  (local.get $out) (i64.const 1))                                 ;; owned one-byte string
"#;

/// `__rt_str_ord`: the first byte of a string as PHP's `ord` reports it.
///
/// An EMPTY string answers 0 rather than failing, and a longer string answers its FIRST byte —
/// both measured. `$deprecate` carries whether this profile diagnoses a length other than one;
/// as with `chr`, the answer does not depend on it.
const RT_STR_ORD_TEMPLATE: &str = r#"(func $__rt_str_ord (param $ptr i32) (param $len i64) (result i64)
{deprecation}  (if (i64.le_s (local.get $len) (i64.const 0))
    (then (return (i64.const 0))))                                ;; the empty string is zero
  (i64.extend_i32_u (i32.load8_u (local.get $ptr))))              ;; the first byte, unsigned
"#;

/// Renders `__rt_str_chr` and `__rt_str_ord` for this module's diagnostic capability.
///
/// The deprecation helpers they would call are command-only (they write to stderr through WASI)
/// and PHP 8.5-only, so a reactor or an earlier profile gets the same helper WITHOUT the call
/// rather than a dangling reference. The answer is identical either way — only the message is
/// conditional — which is exactly why eliding it is sound rather than a silent divergence.
fn str_chr_ord(diagnoses: bool) -> (String, String) {
    let (chr_deprecation, ord_deprecation) = if diagnoses {
        (
            concat!(
                "  (if (i32.or (i64.lt_s (local.get $n) (i64.const 0))\n",
                "              (i64.gt_s (local.get $n) (i64.const 255)))\n",
                "    (then (call $__rt_deprecated_chr_range)))                    ;; 8.5 diagnoses, then answers\n",
            ),
            concat!(
                "  (if (i64.ne (local.get $len) (i64.const 1))\n",
                "    (then (call $__rt_deprecated_ord_length)))                   ;; 8.5 diagnoses, then answers\n",
            ),
        )
    } else {
        ("", "")
    };
    (
        RT_STR_CHR_TEMPLATE.replace("{deprecation}", chr_deprecation),
        RT_STR_ORD_TEMPLATE.replace("{deprecation}", ord_deprecation),
    )
}

/// `__rt_str_case_edge`: owns a copy with only the FIRST byte case-mapped.
///
/// `$upper` selects `ucfirst` over `lcfirst`. Only an ASCII letter moves — `héllo` keeps its
/// `0xc3 0xa9` — and the empty string comes back empty rather than failing.
const RT_STR_CASE_EDGE: &str = r#"(func $__rt_str_case_edge (param $ptr i32) (param $len i64) (param $upper i32) (result i32) (result i64)
  (local $out i32)                                                ;; owned result block
  (local $olen i64)                                               ;; persisted length
  (local $byte i32)                                               ;; the first byte
  (call $__rt_str_persist (local.get $ptr) (local.get $len))      ;; own a copy to edit in place
  (local.set $olen)                                               ;; persisted length
  (local.set $out)                                                ;; persisted pointer
  (if (i64.gt_s (local.get $olen) (i64.const 0))                  ;; the empty string is unchanged
    (then
      (local.set $byte (i32.load8_u (local.get $out)))
      (if (local.get $upper)
        (then
          (if (i32.and (i32.ge_u (local.get $byte) (i32.const 97))
                       (i32.le_u (local.get $byte) (i32.const 122)))
            (then (i32.store8 (local.get $out) (i32.sub (local.get $byte) (i32.const 32))))))
        (else
          (if (i32.and (i32.ge_u (local.get $byte) (i32.const 65))
                       (i32.le_u (local.get $byte) (i32.const 90)))
            (then (i32.store8 (local.get $out) (i32.add (local.get $byte) (i32.const 32)))))))))
  (local.get $out) (local.get $olen))                             ;; owned result
"#;

/// `__rt_str_ucwords`: owns a copy with the first ASCII letter after each delimiter upper-cased.
///
/// PHP's default delimiter set is exactly space, tab, newline, carriage return, form feed and
/// VERTICAL TAB — measured, which is why `\x0b` starts a word here but `-`, `_` and `.` do not.
/// Two delimiters in a row do not skip a word: the byte after each one is a word start.
const RT_STR_UCWORDS: &str = r#"(func $__rt_str_ucwords (param $ptr i32) (param $len i64) (result i32) (result i64)
  (local $out i32)                                                ;; owned result block
  (local $olen i64)                                               ;; persisted length
  (local $i i64)                                                  ;; cursor
  (local $byte i32)                                               ;; current byte
  (local $start i32)                                              ;; is this byte a word start?
  (call $__rt_str_persist (local.get $ptr) (local.get $len))      ;; own a copy to edit in place
  (local.set $olen)                                               ;; persisted length
  (local.set $out)                                                ;; persisted pointer
  (local.set $i (i64.const 0))                                    ;; i = 0
  (local.set $start (i32.const 1))                                ;; the first byte starts a word
  (block $end (loop $scan
    (br_if $end (i64.ge_s (local.get $i) (local.get $olen)))      ;; every byte visited
    (local.set $byte (i32.load8_u (i32.add (local.get $out) (i32.wrap_i64 (local.get $i)))))
    (if (local.get $start)
      (then
        (if (i32.and (i32.ge_u (local.get $byte) (i32.const 97))
                     (i32.le_u (local.get $byte) (i32.const 122)))
          (then (i32.store8 (i32.add (local.get $out) (i32.wrap_i64 (local.get $i)))
                            (i32.sub (local.get $byte) (i32.const 32)))))))  ;; a-z -> A-Z
    (local.set $start
      (i32.or
        (i32.or (i32.eq (local.get $byte) (i32.const 32))         ;; space
                (i32.eq (local.get $byte) (i32.const 9)))         ;; tab
        (i32.or
          (i32.or (i32.eq (local.get $byte) (i32.const 10))       ;; line feed
                  (i32.eq (local.get $byte) (i32.const 13)))      ;; carriage return
          (i32.or (i32.eq (local.get $byte) (i32.const 12))       ;; form feed
                  (i32.eq (local.get $byte) (i32.const 11))))))   ;; vertical tab
    (local.set $i (i64.add (local.get $i) (i64.const 1)))         ;; i++
    (br $scan)))
  (local.get $out) (local.get $olen))                             ;; owned result
"#;

/// `__rt_str_cmp`: PHP's `strcmp` / `strcasecmp` result for two byte strings.
///
/// The two halves of the answer follow DIFFERENT rules, which is the whole subtlety here and was
/// measured rather than assumed: a byte mismatch yields the raw UNSIGNED difference
/// (`strcmp("ABC", "abc")` is -32 and `strcmp("\xff", "\x01")` is 254), while a pure length
/// difference is normalized to -1 or 1 (`strcmp("abcd", "a")` is 1, not 3). `$fold` lowercases
/// ASCII letters before comparing, so `strcasecmp("Z", "a")` is 25 — the distance between the
/// FOLDED bytes, not the original ones.
const RT_STR_CMP: &str = r#"(func $__rt_str_cmp (param $aptr i32) (param $alen i64) (param $bptr i32) (param $blen i64) (param $fold i32) (result i64)
  (local $i i64)                                                  ;; cursor
  (local $shortest i64)                                           ;; bytes both strings have
  (local $x i32)                                                  ;; byte from the left
  (local $y i32)                                                  ;; byte from the right
  (local.set $shortest
    (select (local.get $alen) (local.get $blen)
            (i64.lt_s (local.get $alen) (local.get $blen))))      ;; min(alen, blen)
  (local.set $i (i64.const 0))                                    ;; i = 0
  (block $end (loop $cmp
    (br_if $end (i64.ge_s (local.get $i) (local.get $shortest)))  ;; common prefix exhausted
    (local.set $x (i32.load8_u (i32.add (local.get $aptr) (i32.wrap_i64 (local.get $i)))))
    (local.set $y (i32.load8_u (i32.add (local.get $bptr) (i32.wrap_i64 (local.get $i)))))
    (if (local.get $fold)
      (then
        (if (i32.and (i32.ge_u (local.get $x) (i32.const 65))
                     (i32.le_u (local.get $x) (i32.const 90)))
          (then (local.set $x (i32.add (local.get $x) (i32.const 32)))))  ;; A-Z -> a-z
        (if (i32.and (i32.ge_u (local.get $y) (i32.const 65))
                     (i32.le_u (local.get $y) (i32.const 90)))
          (then (local.set $y (i32.add (local.get $y) (i32.const 32)))))))
    (if (i32.ne (local.get $x) (local.get $y))
      (then (return (i64.extend_i32_s (i32.sub (local.get $x) (local.get $y))))))  ;; raw byte distance
    (local.set $i (i64.add (local.get $i) (i64.const 1)))         ;; i++
    (br $cmp)))
  (if (i64.lt_s (local.get $alen) (local.get $blen))
    (then (return (i64.const -1))))                               ;; a prefix sorts first
  (if (i64.gt_s (local.get $alen) (local.get $blen))
    (then (return (i64.const 1))))                                ;; ...and its extension last
  (i64.const 0))                                                  ;; identical
"#;

/// `__rt_str_trim`: owns a copy with bytes stripped from one or both ends.
///
/// `$mode` bit 0 strips the left end and bit 1 the right, so one helper covers `trim`, `ltrim`
/// and `rtrim`. A `$cl_len` of -1 selects PHP's DEFAULT set — space, tab, newline, carriage
/// return, NUL and vertical tab — which is passed as a sentinel rather than a data segment so a
/// module that never calls the one-argument form carries no extra bytes. An explicitly EMPTY
/// charlist strips nothing, which is what php-src does.
const RT_STR_TRIM: &str = r#"(func $__rt_str_trim (param $ptr i32) (param $len i64) (param $cl_ptr i32) (param $cl_len i64) (param $mode i32) (result i32) (result i64)
  (local $start i64)                                              ;; first kept byte
  (local $stop i64)                                               ;; one past the last kept byte
  (local $out i32)                                                ;; owned result block
  (local $w i32)                                                  ;; copy cursor
  (local.set $start (i64.const 0))                                ;; nothing stripped yet
  (local.set $stop (local.get $len))                              ;; ...from either end
  (if (i32.and (local.get $mode) (i32.const 1))                   ;; strip the left end
    (then
      (block $ldone (loop $lscan
        (br_if $ldone (i64.ge_s (local.get $start) (local.get $stop)))
        (br_if $ldone (i32.eqz (call $__rt_trim_matches
          (i32.load8_u (i32.add (local.get $ptr) (i32.wrap_i64 (local.get $start))))
          (local.get $cl_ptr) (local.get $cl_len))))
        (local.set $start (i64.add (local.get $start) (i64.const 1)))
        (br $lscan)))))
  (if (i32.and (local.get $mode) (i32.const 2))                   ;; strip the right end
    (then
      (block $rdone (loop $rscan
        (br_if $rdone (i64.le_s (local.get $stop) (local.get $start)))
        (br_if $rdone (i32.eqz (call $__rt_trim_matches
          (i32.load8_u (i32.add (local.get $ptr)
                                (i32.wrap_i64 (i64.sub (local.get $stop) (i64.const 1)))))
          (local.get $cl_ptr) (local.get $cl_len))))
        (local.set $stop (i64.sub (local.get $stop) (i64.const 1)))
        (br $rscan)))))
  (local.set $out (call $__rt_str_alloc (i64.sub (local.get $stop) (local.get $start))))
  (local.set $w (i32.const 0))                                    ;; w = 0
  (block $end (loop $copy
    (br_if $end (i64.ge_s (local.get $start) (local.get $stop)))  ;; every kept byte copied
    (i32.store8 (i32.add (local.get $out) (local.get $w))
      (i32.load8_u (i32.add (local.get $ptr) (i32.wrap_i64 (local.get $start)))))
    (local.set $w (i32.add (local.get $w) (i32.const 1)))         ;; w++
    (local.set $start (i64.add (local.get $start) (i64.const 1))) ;; next kept byte
    (br $copy)))
  (local.get $out) (i64.extend_i32_u (local.get $w)))             ;; owned result
"#;

/// `__rt_trim_matches`: whether one byte belongs to a trim character set.
///
/// A `$cl_len` of -1 means PHP's default set rather than a caller-provided list, so the one- and
/// two-argument forms of `trim` share the same scan.
const RT_TRIM_MATCHES: &str = r#"(func $__rt_trim_matches (param $byte i32) (param $cl_ptr i32) (param $cl_len i64) (result i32)
  (local $i i64)                                                  ;; charlist cursor
  (if (i64.lt_s (local.get $cl_len) (i64.const 0))                ;; the default set
    (then (return (i32.or
      (i32.or (i32.eq (local.get $byte) (i32.const 32))           ;; space
              (i32.eq (local.get $byte) (i32.const 9)))           ;; tab
      (i32.or
        (i32.or (i32.eq (local.get $byte) (i32.const 10))         ;; line feed
                (i32.eq (local.get $byte) (i32.const 13)))        ;; carriage return
        (i32.or (i32.eqz (local.get $byte))                       ;; NUL
                (i32.eq (local.get $byte) (i32.const 11))))))))   ;; vertical tab
  (local.set $i (i64.const 0))                                    ;; i = 0
  (block $end (loop $scan
    (br_if $end (i64.ge_s (local.get $i) (local.get $cl_len)))    ;; charlist exhausted
    (if (i32.eq (local.get $byte)
                (i32.load8_u (i32.add (local.get $cl_ptr) (i32.wrap_i64 (local.get $i)))))
      (then (return (i32.const 1))))                              ;; listed
    (local.set $i (i64.add (local.get $i) (i64.const 1)))         ;; i++
    (br $scan)))
  (i32.const 0))                                                  ;; not listed
"#;

/// `__rt_str_substr`: owns PHP's `substr` slice of a string.
///
/// Every out-of-range case answers the EMPTY string rather than false, which is PHP 8's
/// behaviour. A negative `$offset` counts from the end and saturates at 0, so `substr("hello",
/// -9)` is the whole string. `$has_len` distinguishes the two-argument form from an explicit
/// length; a negative length names an end offset from the right, and an end at or before the
/// start yields the empty string. All measured against php-src.
const RT_STR_SUBSTR: &str = r#"(func $__rt_str_substr (param $ptr i32) (param $len i64) (param $offset i64) (param $count i64) (param $has_len i32) (result i32) (result i64)
  (local $start i64)                                              ;; first byte taken
  (local $stop i64)                                               ;; one past the last byte taken
  (local $out i32)                                                ;; owned result block
  (local $w i32)                                                  ;; copy cursor
  (local.set $start (local.get $offset))                          ;; assume a forward offset
  (if (i64.lt_s (local.get $start) (i64.const 0))
    (then
      (local.set $start (i64.add (local.get $len) (local.get $start)))  ;; count from the end
      (if (i64.lt_s (local.get $start) (i64.const 0))
        (then (local.set $start (i64.const 0))))))                ;; ...saturating at the start
  (if (i64.gt_s (local.get $start) (local.get $len))
    (then (local.set $start (local.get $len))))                   ;; past the end takes nothing
  (local.set $stop (local.get $len))                              ;; the two-argument form runs to the end
  (if (local.get $has_len)
    (then
      (if (i64.lt_s (local.get $count) (i64.const 0))
        (then (local.set $stop (i64.add (local.get $len) (local.get $count))))  ;; end from the right
        (else (local.set $stop (i64.add (local.get $start) (local.get $count)))))
      (if (i64.gt_s (local.get $stop) (local.get $len))
        (then (local.set $stop (local.get $len))))                ;; clamp to the end
      (if (i64.lt_s (local.get $stop) (local.get $start))
        (then (local.set $stop (local.get $start))))))            ;; an inverted range is empty
  (local.set $out (call $__rt_str_alloc (i64.sub (local.get $stop) (local.get $start))))
  (local.set $w (i32.const 0))                                    ;; w = 0
  (block $end (loop $copy
    (br_if $end (i64.ge_s (local.get $start) (local.get $stop)))  ;; every selected byte copied
    (i32.store8 (i32.add (local.get $out) (local.get $w))
      (i32.load8_u (i32.add (local.get $ptr) (i32.wrap_i64 (local.get $start)))))
    (local.set $w (i32.add (local.get $w) (i32.const 1)))         ;; w++
    (local.set $start (i64.add (local.get $start) (i64.const 1))) ;; next byte
    (br $copy)))
  (local.get $out) (i64.extend_i32_u (local.get $w)))             ;; owned result
"#;

/// `__rt_str_repeat`: owns a string repeated a non-negative number of times.
///
/// The caller has already refused a negative count, so this only has to handle 0 — which yields
/// the empty string, not a failure — and the ordinary case. `__rt_checked_layout` inside
/// `__rt_str_alloc` is what rejects a product that would overflow wasm32 rather than wrapping.
const RT_STR_REPEAT: &str = r#"(func $__rt_str_repeat (param $ptr i32) (param $len i64) (param $times i64) (result i32) (result i64)
  (local $out i32)                                                ;; owned result block
  (local $i i64)                                                  ;; source cursor within one copy
  (local $left i64)                                               ;; copies still to write
  (local $w i32)                                                  ;; destination cursor
  (local.set $out (call $__rt_str_alloc (i64.mul (local.get $len) (local.get $times))))
  (local.set $left (local.get $times))                            ;; every copy still pending
  (local.set $w (i32.const 0))                                    ;; w = 0
  (block $done (loop $copies
    (br_if $done (i64.le_s (local.get $left) (i64.const 0)))      ;; zero copies is the empty string
    (local.set $i (i64.const 0))                                  ;; restart at the source
    (block $end (loop $bytes
      (br_if $end (i64.ge_s (local.get $i) (local.get $len)))     ;; one whole copy written
      (i32.store8 (i32.add (local.get $out) (local.get $w))
        (i32.load8_u (i32.add (local.get $ptr) (i32.wrap_i64 (local.get $i)))))
      (local.set $w (i32.add (local.get $w) (i32.const 1)))       ;; w++
      (local.set $i (i64.add (local.get $i) (i64.const 1)))       ;; i++
      (br $bytes)))
    (local.set $left (i64.sub (local.get $left) (i64.const 1)))   ;; one fewer copy to go
    (br $copies)))
  (local.get $out) (i64.extend_i32_u (local.get $w)))             ;; owned result
"#;

/// `__rt_str_find`: the index of a needle in a haystack, or -1 when it is absent.
///
/// `$fold` lowercases ASCII letters on both sides, which is the only difference between `strpos`
/// and `stripos`. An EMPTY needle matches at 0 — `strpos("abc", "")` is 0, not false — and a
/// needle longer than the haystack cannot match. The -1 sentinel is what the caller turns into
/// PHP's `false`, and it is unambiguous because a real index is never negative.
const RT_STR_FIND: &str = r#"(func $__rt_str_find (param $hptr i32) (param $hlen i64) (param $nptr i32) (param $nlen i64) (param $fold i32) (result i64)
  (local $at i64)                                                 ;; candidate start offset
  (local $i i64)                                                  ;; cursor within the needle
  (local $x i32)                                                  ;; haystack byte
  (local $y i32)                                                  ;; needle byte
  (local.set $at (i64.const 0))                                   ;; start at the beginning
  (block $none (loop $scan
    (br_if $none (i64.gt_s (i64.add (local.get $at) (local.get $nlen)) (local.get $hlen)))
    (local.set $i (i64.const 0))                                  ;; compare the needle here
    (block $mismatch
      (block $matched (loop $bytes
        (br_if $matched (i64.ge_s (local.get $i) (local.get $nlen)))  ;; an empty needle matches at once
        (local.set $x (i32.load8_u (i32.add (local.get $hptr)
          (i32.wrap_i64 (i64.add (local.get $at) (local.get $i))))))
        (local.set $y (i32.load8_u (i32.add (local.get $nptr) (i32.wrap_i64 (local.get $i)))))
        (if (local.get $fold)
          (then
            (if (i32.and (i32.ge_u (local.get $x) (i32.const 65))
                         (i32.le_u (local.get $x) (i32.const 90)))
              (then (local.set $x (i32.add (local.get $x) (i32.const 32)))))  ;; A-Z -> a-z
            (if (i32.and (i32.ge_u (local.get $y) (i32.const 65))
                         (i32.le_u (local.get $y) (i32.const 90)))
              (then (local.set $y (i32.add (local.get $y) (i32.const 32)))))))
        (br_if $mismatch (i32.ne (local.get $x) (local.get $y)))  ;; this offset is out
        (local.set $i (i64.add (local.get $i) (i64.const 1)))     ;; i++
        (br $bytes)))
      (return (local.get $at)))                                   ;; every needle byte matched
    (local.set $at (i64.add (local.get $at) (i64.const 1)))       ;; try the next offset
    (br $scan)))
  (i64.const -1))                                                 ;; absent
"#;

/// Lowers `strstr` in both its arities.
///
/// PHP's result is `string|false`, so the two outcomes are boxed under different Mixed tags the
/// way `strpos` boxes its own. The returned slice is a REGION of the haystack — from the match to
/// the end, or from the start up to the match when `$before_needle` is true — and boxing under
/// the string tag persists a copy, so pointing into the source is safe rather than aliasing it.
/// An empty needle matches at 0, which is why `strstr("abcdef", "")` is the whole string and its
/// `before` form is empty.
fn lower_strstr(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let found = ctx.fresh_temp(super::wat::ValType::I64);
    let hptr = ctx.fresh_temp(super::wat::ValType::I32);
    let hlen = ctx.fresh_temp(super::wat::ValType::I64);
    ctx.emit_load_value(operand(inst, 0)?)?;
    ctx.fb.ins(&format!("local.set {hlen}"), "spill haystack length");
    ctx.fb.ins(&format!("local.set {hptr}"), "spill haystack pointer");
    ctx.fb.ins(&format!("local.get {hptr}"), "haystack pointer");
    ctx.fb.ins(&format!("local.get {hlen}"), "haystack length");
    ctx.emit_load_value(operand(inst, 1)?)?;
    ctx.fb.ins("i32.const 0", "strstr is case-sensitive");
    ctx.fb
        .ins("call $__rt_str_find", "first offset, or -1 when absent");
    ctx.fb
        .ins(&format!("local.set {found}"), "spill the scan result");

    ctx.fb.ins(&format!("local.get {found}"), "scan result");
    ctx.fb.ins("i64.const 0", "the absent sentinel is negative");
    ctx.fb.ins("i64.lt_s", "was the needle absent?");
    ctx.fb
        .ins("if (result i32)", "string|false travels as a Mixed cell");
    ctx.fb.ins("i64.const 3", "mixed tag (bool)");
    ctx.fb.ins("i64.const 0", "the value false");
    ctx.fb.ins("i64.const 0", "hi unused");
    ctx.fb.ins("call $__rt_mixed_from_value", "box PHP's false");
    ctx.fb.ins("else", "the needle was found");
    ctx.fb.ins("i64.const 1", "mixed tag (string)");
    if inst.operands.len() == 3 {
        // `$before_needle` selects which side of the match survives, and it is a runtime value
        // rather than a literal, so both regions are computed and one is selected.
        ctx.fb.ins(&format!("local.get {hptr}"), "the haystack start");
        ctx.fb.ins("i64.extend_i32_u", "start pointer -> lo");
        ctx.fb.ins(&format!("local.get {hptr}"), "the haystack start");
        ctx.fb.ins("i64.extend_i32_u", "widen before adding the offset");
        ctx.fb.ins(&format!("local.get {found}"), "the match offset");
        ctx.fb.ins("i64.add", "pointer to the match");
        ctx.emit_load_value(operand(inst, 2)?)?;
        ctx.fb.ins("i64.const 0", "compare the flag against false");
        ctx.fb
            .ins("i64.ne", "a truthy flag selects the leading region");
        ctx.fb.ins("select", "which region's pointer");
        ctx.fb.ins(&format!("local.get {found}"), "bytes before the match");
        ctx.fb.ins(&format!("local.get {hlen}"), "haystack length");
        ctx.fb.ins(&format!("local.get {found}"), "the match offset");
        ctx.fb.ins("i64.sub", "bytes from the match to the end");
        ctx.emit_load_value(operand(inst, 2)?)?;
        ctx.fb.ins("i64.const 0", "compare the flag against false");
        ctx.fb
            .ins("i64.ne", "a truthy flag selects the leading region");
        ctx.fb.ins("select", "which region's length");
    } else {
        ctx.fb.ins(&format!("local.get {hptr}"), "the haystack start");
        ctx.fb.ins("i64.extend_i32_u", "widen before adding the offset");
        ctx.fb.ins(&format!("local.get {found}"), "the match offset");
        ctx.fb.ins("i64.add", "lo: pointer to the match");
        ctx.fb.ins(&format!("local.get {hlen}"), "haystack length");
        ctx.fb.ins(&format!("local.get {found}"), "the match offset");
        ctx.fb.ins("i64.sub", "hi: bytes from the match to the end");
    }
    ctx.fb
        .ins("call $__rt_mixed_from_value", "box the region (persists a copy)");
    ctx.fb.ins("end", "end string|false selection");
    store_result(ctx, inst)
}

/// Validates `strstr`: two strings, an optional bool, and PHP's `string|false` Mixed out.
fn strstr_shape_issue(function: &Function, call: &Instruction) -> Option<String> {
    if !matches!(call.operands.len(), 2 | 3) {
        return Some(format!(
            "expected a haystack, a needle and an optional flag, got {} operands",
            call.operands.len()
        ));
    }
    for (index, operand) in call.operands.iter().enumerate() {
        let Some(value) = function.value(*operand) else {
            return Some("operand is missing from the value table".to_string());
        };
        let (want_ir, want_php) = if index < 2 {
            (IrType::Str, PhpType::Str)
        } else {
            (IrType::I64, PhpType::Bool)
        };
        if value.ir_type != want_ir || value.php_type.codegen_repr() != want_php {
            return Some(format!(
                "strstr operand {index} is {:?}/{:?}, expected {want_ir:?}/{want_php:?}",
                value.ir_type,
                value.php_type.codegen_repr()
            ));
        }
    }
    if call.result.is_none()
        || call.result_type != IrType::Heap(IrHeapKind::Mixed)
        || call.result_php_type.codegen_repr() != PhpType::Mixed
    {
        return Some(format!(
            "strstr result {:?}/{:?} is not the Mixed cell PHP's string|false needs",
            call.result_type,
            call.result_php_type.codegen_repr()
        ));
    }
    None
}

/// `__rt_str_pad`: owns a copy padded to `$target` bytes, cycling the pad string.
///
/// `$mode` is PHP's `STR_PAD_*`: 0 left, 1 right, 2 both. When both sides pad, the LEFT side gets
/// the smaller half — `str_pad("ab", 7, "xyz", STR_PAD_BOTH)` is `xyabxyz`, two on the left and
/// three on the right. Each side cycles the pad string from ITS OWN start, which is why the right
/// side there is `xyz` and not a continuation of the left. A target at or below the current
/// length returns the string unchanged, and the caller has already refused an empty pad string in
/// the only case where PHP does.
/// `__rt_pad_byte`: the pad byte at cycle position `$i`, defaulting to a space.
///
/// A null `$pptr` is PHP's default `" "` pad. Resolving it here rather than interning a one-byte
/// data segment keeps a module that never calls the two-argument form byte-identical.
const RT_PAD_BYTE: &str = r#"(func $__rt_pad_byte (param $pptr i32) (param $plen i64) (param $i i64) (result i32)
  (if (i32.eqz (local.get $pptr))
    (then (return (i32.const 32))))                               ;; the default pad is a space
  (i32.load8_u (i32.add (local.get $pptr)
    (i32.wrap_i64 (i64.rem_s (local.get $i) (local.get $plen))))))  ;; cycle through the pad
"#;

const RT_STR_PAD: &str = r#"(func $__rt_str_pad (param $ptr i32) (param $len i64) (param $target i64) (param $pptr i32) (param $plen i64) (param $mode i32) (result i32) (result i64)
  (local $out i32)                                                ;; owned result block
  (local $total i64)                                              ;; bytes of padding to add
  (local $left i64)                                               ;; bytes on the left
  (local $i i64)                                                  ;; write cursor
  (local $w i32)                                                  ;; destination cursor
  (if (i64.le_s (local.get $target) (local.get $len))
    (then (return (call $__rt_str_persist (local.get $ptr) (local.get $len)))))  ;; nothing to add
  (local.set $total (i64.sub (local.get $target) (local.get $len)))
  (local.set $left (i64.const 0))                                 ;; STR_PAD_RIGHT adds nothing left
  (if (i32.eqz (local.get $mode))
    (then (local.set $left (local.get $total))))                  ;; STR_PAD_LEFT adds it all
  (if (i32.eq (local.get $mode) (i32.const 2))
    (then (local.set $left (i64.div_s (local.get $total) (i64.const 2)))))  ;; BOTH: the left half rounds down
  (local.set $out (call $__rt_str_alloc (local.get $target)))
  (local.set $w (i32.const 0))                                    ;; w = 0
  (local.set $i (i64.const 0))                                    ;; i = 0
  (block $lend (loop $lpad
    (br_if $lend (i64.ge_s (local.get $i) (local.get $left)))     ;; left padding written
    (i32.store8 (i32.add (local.get $out) (local.get $w))
      (call $__rt_pad_byte (local.get $pptr) (local.get $plen) (local.get $i)))  ;; cycle the pad
    (local.set $w (i32.add (local.get $w) (i32.const 1)))         ;; w++
    (local.set $i (i64.add (local.get $i) (i64.const 1)))         ;; i++
    (br $lpad)))
  (local.set $i (i64.const 0))                                    ;; i = 0
  (block $send (loop $scopy
    (br_if $send (i64.ge_s (local.get $i) (local.get $len)))      ;; subject copied
    (i32.store8 (i32.add (local.get $out) (local.get $w))
      (i32.load8_u (i32.add (local.get $ptr) (i32.wrap_i64 (local.get $i)))))
    (local.set $w (i32.add (local.get $w) (i32.const 1)))         ;; w++
    (local.set $i (i64.add (local.get $i) (i64.const 1)))         ;; i++
    (br $scopy)))
  (local.set $i (i64.const 0))                                    ;; i = 0, restarting the pad cycle
  (block $rend (loop $rpad
    (br_if $rend (i64.ge_s (i64.add (local.get $left) (local.get $i)) (local.get $total)))
    (i32.store8 (i32.add (local.get $out) (local.get $w))
      (call $__rt_pad_byte (local.get $pptr) (local.get $plen) (local.get $i)))  ;; cycle from its own start
    (local.set $w (i32.add (local.get $w) (i32.const 1)))         ;; w++
    (local.set $i (i64.add (local.get $i) (i64.const 1)))         ;; i++
    (br $rpad)))
  (local.get $out) (i64.extend_i32_u (local.get $w)))             ;; owned result
"#;

/// `__rt_str_replace`: owns a copy with every occurrence of a needle replaced.
///
/// Scanning is left to right and NON-overlapping, and the replacement is never rescanned — which
/// is what makes `str_replace("a", "ab", "a")` answer `"ab"` rather than looping. An EMPTY needle
/// matches nothing and returns the subject unchanged, which is php-src's guard against exactly
/// that loop. The reservation is `slen * (rlen + 1)`, which bounds the worst case of a
/// single-byte needle replaced by a longer string at every position.
const RT_STR_REPLACE: &str = r#"(func $__rt_str_replace (param $sptr i32) (param $slen i64) (param $nptr i32) (param $nlen i64) (param $rptr i32) (param $rlen i64) (result i32) (result i64)
  (local $out i32)                                                ;; owned result block
  (local $i i64)                                                  ;; subject cursor
  (local $j i64)                                                  ;; needle/replacement cursor
  (local $w i32)                                                  ;; destination cursor
  (if (i64.le_s (local.get $nlen) (i64.const 0))
    (then (return (call $__rt_str_persist (local.get $sptr) (local.get $slen)))))  ;; empty needle matches nothing
  (local.set $out
    (call $__rt_str_alloc
      (i64.mul (local.get $slen) (i64.add (local.get $rlen) (i64.const 1)))))
  (local.set $i (i64.const 0))                                    ;; i = 0
  (local.set $w (i32.const 0))                                    ;; w = 0
  (block $end (loop $scan
    (br_if $end (i64.ge_s (local.get $i) (local.get $slen)))      ;; whole subject consumed
    (if (i32.and
          (i64.le_s (i64.add (local.get $i) (local.get $nlen)) (local.get $slen))
          (i32.wrap_i64 (call $__rt_str_region_eq
            (local.get $sptr) (local.get $nptr) (local.get $nlen) (local.get $i))))
      (then
        (local.set $j (i64.const 0))                              ;; write the replacement
        (block $rend (loop $rcopy
          (br_if $rend (i64.ge_s (local.get $j) (local.get $rlen)))
          (i32.store8 (i32.add (local.get $out) (local.get $w))
            (i32.load8_u (i32.add (local.get $rptr) (i32.wrap_i64 (local.get $j)))))
          (local.set $w (i32.add (local.get $w) (i32.const 1)))   ;; w++
          (local.set $j (i64.add (local.get $j) (i64.const 1)))   ;; j++
          (br $rcopy)))
        (local.set $i (i64.add (local.get $i) (local.get $nlen)))) ;; skip the match, never rescan
      (else
        (i32.store8 (i32.add (local.get $out) (local.get $w))
          (i32.load8_u (i32.add (local.get $sptr) (i32.wrap_i64 (local.get $i)))))
        (local.set $w (i32.add (local.get $w) (i32.const 1)))     ;; w++
        (local.set $i (i64.add (local.get $i) (i64.const 1)))))   ;; i++
    (br $scan)))
  (local.get $out) (i64.extend_i32_u (local.get $w)))             ;; owned result
"#;

/// `__rt_crc32`: PHP's `crc32`, the reflected IEEE 802.3 polynomial.
///
/// Computed bitwise rather than through a 256-entry table: eight shifts per byte cost a little
/// time but keep a module that calls this from carrying a kilobyte of static data. PHP's result
/// is the UNSIGNED 32-bit value, which on a 64-bit build is a positive int — so the final
/// complement is masked rather than sign-extended.
const RT_CRC32: &str = r#"(func $__rt_crc32 (param $ptr i32) (param $len i64) (result i64)
  (local $crc i32)                                                ;; running remainder
  (local $i i64)                                                  ;; byte cursor
  (local $k i32)                                                  ;; bit counter
  (local.set $crc (i32.const -1))                                 ;; 0xFFFFFFFF
  (local.set $i (i64.const 0))                                    ;; i = 0
  (block $end (loop $bytes
    (br_if $end (i64.ge_s (local.get $i) (local.get $len)))       ;; every byte folded in
    (local.set $crc
      (i32.xor (local.get $crc)
        (i32.load8_u (i32.add (local.get $ptr) (i32.wrap_i64 (local.get $i))))))
    (local.set $k (i32.const 0))                                  ;; eight bits per byte
    (block $bend (loop $bits
      (br_if $bend (i32.ge_u (local.get $k) (i32.const 8)))
      (local.set $crc
        (i32.xor
          (i32.shr_u (local.get $crc) (i32.const 1))
          (i32.and (i32.const 0xEDB88320)
                   (i32.sub (i32.const 0) (i32.and (local.get $crc) (i32.const 1))))))
      (local.set $k (i32.add (local.get $k) (i32.const 1)))       ;; k++
      (br $bits)))
    (local.set $i (i64.add (local.get $i) (i64.const 1)))         ;; i++
    (br $bytes)))
  (i64.and (i64.extend_i32_u (i32.xor (local.get $crc) (i32.const -1)))
           (i64.const 0xFFFFFFFF)))                               ;; PHP's unsigned 32-bit result
"#;

/// `__rt_sha1_hex`: PHP's `sha1`, returning the 40-character lowercase hex digest.
///
/// The message is padded into an owned buffer rather than processed in place with a special-cased
/// tail: one `0x80` byte, zeros up to 56 bytes past a 64-byte boundary, then the BIT length as a
/// big-endian 64-bit word. Every word in SHA-1 is big-endian, which is the difference from MD5
/// and the usual place an implementation goes wrong. The 80-word message schedule lives in the
/// same buffer's tail so a single allocation covers both, and it is freed before returning.
const RT_SHA1_HEX: &str = r#"(func $__rt_sha1_hex (param $ptr i32) (param $len i64) (result i32) (result i64)
  (local $blocks i32)                                             ;; padded length in 64-byte blocks
  (local $buf i32)                                                ;; padded message
  (local $w i32)                                                  ;; 80-word schedule
  (local $i i32)                                                  ;; general cursor
  (local $t i32)                                                  ;; round counter
  (local $n i32)                                                  ;; source length as i32
  (local $base i32)                                               ;; current block offset
  (local $h0 i32) (local $h1 i32) (local $h2 i32) (local $h3 i32) (local $h4 i32)
  (local $a i32) (local $b i32) (local $c i32) (local $d i32) (local $e i32)
  (local $f i32) (local $k i32) (local $tmp i32)
  (local $out i32) (local $ow i32) (local $nib i32)
  (local.set $n (i32.wrap_i64 (local.get $len)))
  (local.set $blocks
    (i32.div_u (i32.add (local.get $n) (i32.const 72)) (i32.const 64)))  ;; room for 0x80 + 8 length bytes
  (local.set $buf (call $__rt_heap_alloc
    (i32.add (i32.mul (local.get $blocks) (i32.const 64)) (i32.const 320))))  ;; message + schedule
  (local.set $w (i32.add (local.get $buf) (i32.mul (local.get $blocks) (i32.const 64))))
  (local.set $i (i32.const 0))
  (block $cend (loop $copy
    (br_if $cend (i32.ge_u (local.get $i) (local.get $n)))
    (i32.store8 (i32.add (local.get $buf) (local.get $i))
      (i32.load8_u (i32.add (local.get $ptr) (local.get $i))))
    (local.set $i (i32.add (local.get $i) (i32.const 1)))
    (br $copy)))
  (i32.store8 (i32.add (local.get $buf) (local.get $i)) (i32.const 128))  ;; the 0x80 terminator
  (local.set $i (i32.add (local.get $i) (i32.const 1)))
  (block $zend (loop $zeros
    (br_if $zend (i32.ge_u (local.get $i) (i32.mul (local.get $blocks) (i32.const 64))))
    (i32.store8 (i32.add (local.get $buf) (local.get $i)) (i32.const 0))
    (local.set $i (i32.add (local.get $i) (i32.const 1)))
    (br $zeros)))
  (local.set $i (i32.const 0))
  (block $lend (loop $lenbytes                                    ;; big-endian bit length in the last 8 bytes
    (br_if $lend (i32.ge_u (local.get $i) (i32.const 8)))
    (i32.store8
      (i32.sub (i32.add (local.get $buf) (i32.mul (local.get $blocks) (i32.const 64)))
               (i32.sub (i32.const 8) (local.get $i)))
      (i32.wrap_i64 (i64.and
        (i64.shr_u (i64.mul (local.get $len) (i64.const 8))
                   (i64.extend_i32_u (i32.mul (i32.sub (i32.const 7) (local.get $i)) (i32.const 8))))
        (i64.const 255))))
    (local.set $i (i32.add (local.get $i) (i32.const 1)))
    (br $lenbytes)))
  (local.set $h0 (i32.const 0x67452301))
  (local.set $h1 (i32.const 0xEFCDAB89))
  (local.set $h2 (i32.const 0x98BADCFE))
  (local.set $h3 (i32.const 0x10325476))
  (local.set $h4 (i32.const 0xC3D2E1F0))
  (local.set $base (i32.const 0))
  (block $bend (loop $block
    (br_if $bend (i32.ge_u (local.get $base) (i32.mul (local.get $blocks) (i32.const 64))))
    (local.set $t (i32.const 0))
    (block $wend (loop $w16                                       ;; the first 16 words, big-endian
      (br_if $wend (i32.ge_u (local.get $t) (i32.const 16)))
      (i32.store (i32.add (local.get $w) (i32.mul (local.get $t) (i32.const 4)))
        (i32.or (i32.or
          (i32.shl (i32.load8_u (i32.add (local.get $buf)
            (i32.add (local.get $base) (i32.mul (local.get $t) (i32.const 4))))) (i32.const 24))
          (i32.shl (i32.load8_u (i32.add (local.get $buf)
            (i32.add (i32.add (local.get $base) (i32.mul (local.get $t) (i32.const 4))) (i32.const 1)))) (i32.const 16)))
          (i32.or
          (i32.shl (i32.load8_u (i32.add (local.get $buf)
            (i32.add (i32.add (local.get $base) (i32.mul (local.get $t) (i32.const 4))) (i32.const 2)))) (i32.const 8))
          (i32.load8_u (i32.add (local.get $buf)
            (i32.add (i32.add (local.get $base) (i32.mul (local.get $t) (i32.const 4))) (i32.const 3)))))))
      (local.set $t (i32.add (local.get $t) (i32.const 1)))
      (br $w16)))
    (block $xend (loop $expand                                    ;; w[t] = rotl(w[t-3]^w[t-8]^w[t-14]^w[t-16], 1)
      (br_if $xend (i32.ge_u (local.get $t) (i32.const 80)))
      (local.set $tmp (i32.xor (i32.xor
        (i32.load (i32.add (local.get $w) (i32.mul (i32.sub (local.get $t) (i32.const 3)) (i32.const 4))))
        (i32.load (i32.add (local.get $w) (i32.mul (i32.sub (local.get $t) (i32.const 8)) (i32.const 4)))))
        (i32.xor
        (i32.load (i32.add (local.get $w) (i32.mul (i32.sub (local.get $t) (i32.const 14)) (i32.const 4))))
        (i32.load (i32.add (local.get $w) (i32.mul (i32.sub (local.get $t) (i32.const 16)) (i32.const 4)))))))
      (i32.store (i32.add (local.get $w) (i32.mul (local.get $t) (i32.const 4)))
        (i32.rotl (local.get $tmp) (i32.const 1)))
      (local.set $t (i32.add (local.get $t) (i32.const 1)))
      (br $expand)))
    (local.set $a (local.get $h0)) (local.set $b (local.get $h1))
    (local.set $c (local.get $h2)) (local.set $d (local.get $h3))
    (local.set $e (local.get $h4))
    (local.set $t (i32.const 0))
    (block $rend (loop $rounds
      (br_if $rend (i32.ge_u (local.get $t) (i32.const 80)))
      (if (i32.lt_u (local.get $t) (i32.const 20))
        (then
          (local.set $f (i32.or (i32.and (local.get $b) (local.get $c))
                                (i32.and (i32.xor (local.get $b) (i32.const -1)) (local.get $d))))
          (local.set $k (i32.const 0x5A827999)))
        (else (if (i32.lt_u (local.get $t) (i32.const 40))
          (then
            (local.set $f (i32.xor (i32.xor (local.get $b) (local.get $c)) (local.get $d)))
            (local.set $k (i32.const 0x6ED9EBA1)))
          (else (if (i32.lt_u (local.get $t) (i32.const 60))
            (then
              (local.set $f (i32.or (i32.or
                (i32.and (local.get $b) (local.get $c))
                (i32.and (local.get $b) (local.get $d)))
                (i32.and (local.get $c) (local.get $d))))
              (local.set $k (i32.const 0x8F1BBCDC)))
            (else
              (local.set $f (i32.xor (i32.xor (local.get $b) (local.get $c)) (local.get $d)))
              (local.set $k (i32.const 0xCA62C1D6))))))))
      (local.set $tmp (i32.add (i32.add (i32.add (i32.add
        (i32.rotl (local.get $a) (i32.const 5)) (local.get $f)) (local.get $e)) (local.get $k))
        (i32.load (i32.add (local.get $w) (i32.mul (local.get $t) (i32.const 4))))))
      (local.set $e (local.get $d))
      (local.set $d (local.get $c))
      (local.set $c (i32.rotl (local.get $b) (i32.const 30)))
      (local.set $b (local.get $a))
      (local.set $a (local.get $tmp))
      (local.set $t (i32.add (local.get $t) (i32.const 1)))
      (br $rounds)))
    (local.set $h0 (i32.add (local.get $h0) (local.get $a)))
    (local.set $h1 (i32.add (local.get $h1) (local.get $b)))
    (local.set $h2 (i32.add (local.get $h2) (local.get $c)))
    (local.set $h3 (i32.add (local.get $h3) (local.get $d)))
    (local.set $h4 (i32.add (local.get $h4) (local.get $e)))
    (local.set $base (i32.add (local.get $base) (i32.const 64)))
    (br $block)))
  (call $__rt_heap_free (local.get $buf))                         ;; the scratch is done with
  (local.set $out (call $__rt_str_alloc (i64.const 40)))          ;; 5 words, 8 hex digits each
  (local.set $ow (i32.const 0))
  (local.set $i (i32.const 0))
  (block $hend (loop $hex
    (br_if $hend (i32.ge_u (local.get $i) (i32.const 40)))
    (local.set $tmp (local.get $h0))
    (if (i32.ge_u (local.get $i) (i32.const 8))  (then (local.set $tmp (local.get $h1))))
    (if (i32.ge_u (local.get $i) (i32.const 16)) (then (local.set $tmp (local.get $h2))))
    (if (i32.ge_u (local.get $i) (i32.const 24)) (then (local.set $tmp (local.get $h3))))
    (if (i32.ge_u (local.get $i) (i32.const 32)) (then (local.set $tmp (local.get $h4))))
    (local.set $nib (i32.and
      (i32.shr_u (local.get $tmp)
        (i32.mul (i32.sub (i32.const 7) (i32.rem_u (local.get $i) (i32.const 8))) (i32.const 4)))
      (i32.const 15)))
    (i32.store8 (i32.add (local.get $out) (local.get $ow))
      (i32.add (local.get $nib)
        (select (i32.const 48) (i32.const 87) (i32.lt_u (local.get $nib) (i32.const 10)))))
    (local.set $ow (i32.add (local.get $ow) (i32.const 1)))
    (local.set $i (i32.add (local.get $i) (i32.const 1)))
    (br $hex)))
  (local.get $out) (i64.const 40))                                ;; owned 40-character digest
"#;

/// `__rt_md5_k`: MD5's per-round additive constant, `floor(|sin(i+1)| * 2^32)`.
///
/// A branch chain rather than a data segment: WebAssembly has no `sin`, so the 64 values have to
/// be materialized somehow, and a segment would mean threading a new reservation through
/// `plan_module`'s static-data cursor for a table only this builtin reads.
const RT_MD5_K: &str = r#"(func $__rt_md5_k (param $i i32) (result i32)
  (if (i32.eq (local.get $i) (i32.const 0)) (then (return (i32.const 0xd76aa478))))
  (if (i32.eq (local.get $i) (i32.const 1)) (then (return (i32.const 0xe8c7b756))))
  (if (i32.eq (local.get $i) (i32.const 2)) (then (return (i32.const 0x242070db))))
  (if (i32.eq (local.get $i) (i32.const 3)) (then (return (i32.const 0xc1bdceee))))
  (if (i32.eq (local.get $i) (i32.const 4)) (then (return (i32.const 0xf57c0faf))))
  (if (i32.eq (local.get $i) (i32.const 5)) (then (return (i32.const 0x4787c62a))))
  (if (i32.eq (local.get $i) (i32.const 6)) (then (return (i32.const 0xa8304613))))
  (if (i32.eq (local.get $i) (i32.const 7)) (then (return (i32.const 0xfd469501))))
  (if (i32.eq (local.get $i) (i32.const 8)) (then (return (i32.const 0x698098d8))))
  (if (i32.eq (local.get $i) (i32.const 9)) (then (return (i32.const 0x8b44f7af))))
  (if (i32.eq (local.get $i) (i32.const 10)) (then (return (i32.const 0xffff5bb1))))
  (if (i32.eq (local.get $i) (i32.const 11)) (then (return (i32.const 0x895cd7be))))
  (if (i32.eq (local.get $i) (i32.const 12)) (then (return (i32.const 0x6b901122))))
  (if (i32.eq (local.get $i) (i32.const 13)) (then (return (i32.const 0xfd987193))))
  (if (i32.eq (local.get $i) (i32.const 14)) (then (return (i32.const 0xa679438e))))
  (if (i32.eq (local.get $i) (i32.const 15)) (then (return (i32.const 0x49b40821))))
  (if (i32.eq (local.get $i) (i32.const 16)) (then (return (i32.const 0xf61e2562))))
  (if (i32.eq (local.get $i) (i32.const 17)) (then (return (i32.const 0xc040b340))))
  (if (i32.eq (local.get $i) (i32.const 18)) (then (return (i32.const 0x265e5a51))))
  (if (i32.eq (local.get $i) (i32.const 19)) (then (return (i32.const 0xe9b6c7aa))))
  (if (i32.eq (local.get $i) (i32.const 20)) (then (return (i32.const 0xd62f105d))))
  (if (i32.eq (local.get $i) (i32.const 21)) (then (return (i32.const 0x02441453))))
  (if (i32.eq (local.get $i) (i32.const 22)) (then (return (i32.const 0xd8a1e681))))
  (if (i32.eq (local.get $i) (i32.const 23)) (then (return (i32.const 0xe7d3fbc8))))
  (if (i32.eq (local.get $i) (i32.const 24)) (then (return (i32.const 0x21e1cde6))))
  (if (i32.eq (local.get $i) (i32.const 25)) (then (return (i32.const 0xc33707d6))))
  (if (i32.eq (local.get $i) (i32.const 26)) (then (return (i32.const 0xf4d50d87))))
  (if (i32.eq (local.get $i) (i32.const 27)) (then (return (i32.const 0x455a14ed))))
  (if (i32.eq (local.get $i) (i32.const 28)) (then (return (i32.const 0xa9e3e905))))
  (if (i32.eq (local.get $i) (i32.const 29)) (then (return (i32.const 0xfcefa3f8))))
  (if (i32.eq (local.get $i) (i32.const 30)) (then (return (i32.const 0x676f02d9))))
  (if (i32.eq (local.get $i) (i32.const 31)) (then (return (i32.const 0x8d2a4c8a))))
  (if (i32.eq (local.get $i) (i32.const 32)) (then (return (i32.const 0xfffa3942))))
  (if (i32.eq (local.get $i) (i32.const 33)) (then (return (i32.const 0x8771f681))))
  (if (i32.eq (local.get $i) (i32.const 34)) (then (return (i32.const 0x6d9d6122))))
  (if (i32.eq (local.get $i) (i32.const 35)) (then (return (i32.const 0xfde5380c))))
  (if (i32.eq (local.get $i) (i32.const 36)) (then (return (i32.const 0xa4beea44))))
  (if (i32.eq (local.get $i) (i32.const 37)) (then (return (i32.const 0x4bdecfa9))))
  (if (i32.eq (local.get $i) (i32.const 38)) (then (return (i32.const 0xf6bb4b60))))
  (if (i32.eq (local.get $i) (i32.const 39)) (then (return (i32.const 0xbebfbc70))))
  (if (i32.eq (local.get $i) (i32.const 40)) (then (return (i32.const 0x289b7ec6))))
  (if (i32.eq (local.get $i) (i32.const 41)) (then (return (i32.const 0xeaa127fa))))
  (if (i32.eq (local.get $i) (i32.const 42)) (then (return (i32.const 0xd4ef3085))))
  (if (i32.eq (local.get $i) (i32.const 43)) (then (return (i32.const 0x04881d05))))
  (if (i32.eq (local.get $i) (i32.const 44)) (then (return (i32.const 0xd9d4d039))))
  (if (i32.eq (local.get $i) (i32.const 45)) (then (return (i32.const 0xe6db99e5))))
  (if (i32.eq (local.get $i) (i32.const 46)) (then (return (i32.const 0x1fa27cf8))))
  (if (i32.eq (local.get $i) (i32.const 47)) (then (return (i32.const 0xc4ac5665))))
  (if (i32.eq (local.get $i) (i32.const 48)) (then (return (i32.const 0xf4292244))))
  (if (i32.eq (local.get $i) (i32.const 49)) (then (return (i32.const 0x432aff97))))
  (if (i32.eq (local.get $i) (i32.const 50)) (then (return (i32.const 0xab9423a7))))
  (if (i32.eq (local.get $i) (i32.const 51)) (then (return (i32.const 0xfc93a039))))
  (if (i32.eq (local.get $i) (i32.const 52)) (then (return (i32.const 0x655b59c3))))
  (if (i32.eq (local.get $i) (i32.const 53)) (then (return (i32.const 0x8f0ccc92))))
  (if (i32.eq (local.get $i) (i32.const 54)) (then (return (i32.const 0xffeff47d))))
  (if (i32.eq (local.get $i) (i32.const 55)) (then (return (i32.const 0x85845dd1))))
  (if (i32.eq (local.get $i) (i32.const 56)) (then (return (i32.const 0x6fa87e4f))))
  (if (i32.eq (local.get $i) (i32.const 57)) (then (return (i32.const 0xfe2ce6e0))))
  (if (i32.eq (local.get $i) (i32.const 58)) (then (return (i32.const 0xa3014314))))
  (if (i32.eq (local.get $i) (i32.const 59)) (then (return (i32.const 0x4e0811a1))))
  (if (i32.eq (local.get $i) (i32.const 60)) (then (return (i32.const 0xf7537e82))))
  (if (i32.eq (local.get $i) (i32.const 61)) (then (return (i32.const 0xbd3af235))))
  (if (i32.eq (local.get $i) (i32.const 62)) (then (return (i32.const 0x2ad7d2bb))))
  (if (i32.eq (local.get $i) (i32.const 63)) (then (return (i32.const 0xeb86d391))))
  (i32.const 0))
"#;

/// `__rt_md5_s`: MD5's per-round rotation amount.
///
/// The four groups repeat `[7,12,17,22]`, `[5,9,14,20]`, `[4,11,16,23]`, `[6,10,15,21]`, so this
/// IS computable from `(i/16, i%4)` — it is written out for the same reason the constants are,
/// which is that one lookup shape is easier to check against the spec than two.
const RT_MD5_S: &str = r#"(func $__rt_md5_s (param $i i32) (result i32)
  (if (i32.eq (local.get $i) (i32.const 0)) (then (return (i32.const 0x00000007))))
  (if (i32.eq (local.get $i) (i32.const 1)) (then (return (i32.const 0x0000000c))))
  (if (i32.eq (local.get $i) (i32.const 2)) (then (return (i32.const 0x00000011))))
  (if (i32.eq (local.get $i) (i32.const 3)) (then (return (i32.const 0x00000016))))
  (if (i32.eq (local.get $i) (i32.const 4)) (then (return (i32.const 0x00000007))))
  (if (i32.eq (local.get $i) (i32.const 5)) (then (return (i32.const 0x0000000c))))
  (if (i32.eq (local.get $i) (i32.const 6)) (then (return (i32.const 0x00000011))))
  (if (i32.eq (local.get $i) (i32.const 7)) (then (return (i32.const 0x00000016))))
  (if (i32.eq (local.get $i) (i32.const 8)) (then (return (i32.const 0x00000007))))
  (if (i32.eq (local.get $i) (i32.const 9)) (then (return (i32.const 0x0000000c))))
  (if (i32.eq (local.get $i) (i32.const 10)) (then (return (i32.const 0x00000011))))
  (if (i32.eq (local.get $i) (i32.const 11)) (then (return (i32.const 0x00000016))))
  (if (i32.eq (local.get $i) (i32.const 12)) (then (return (i32.const 0x00000007))))
  (if (i32.eq (local.get $i) (i32.const 13)) (then (return (i32.const 0x0000000c))))
  (if (i32.eq (local.get $i) (i32.const 14)) (then (return (i32.const 0x00000011))))
  (if (i32.eq (local.get $i) (i32.const 15)) (then (return (i32.const 0x00000016))))
  (if (i32.eq (local.get $i) (i32.const 16)) (then (return (i32.const 0x00000005))))
  (if (i32.eq (local.get $i) (i32.const 17)) (then (return (i32.const 0x00000009))))
  (if (i32.eq (local.get $i) (i32.const 18)) (then (return (i32.const 0x0000000e))))
  (if (i32.eq (local.get $i) (i32.const 19)) (then (return (i32.const 0x00000014))))
  (if (i32.eq (local.get $i) (i32.const 20)) (then (return (i32.const 0x00000005))))
  (if (i32.eq (local.get $i) (i32.const 21)) (then (return (i32.const 0x00000009))))
  (if (i32.eq (local.get $i) (i32.const 22)) (then (return (i32.const 0x0000000e))))
  (if (i32.eq (local.get $i) (i32.const 23)) (then (return (i32.const 0x00000014))))
  (if (i32.eq (local.get $i) (i32.const 24)) (then (return (i32.const 0x00000005))))
  (if (i32.eq (local.get $i) (i32.const 25)) (then (return (i32.const 0x00000009))))
  (if (i32.eq (local.get $i) (i32.const 26)) (then (return (i32.const 0x0000000e))))
  (if (i32.eq (local.get $i) (i32.const 27)) (then (return (i32.const 0x00000014))))
  (if (i32.eq (local.get $i) (i32.const 28)) (then (return (i32.const 0x00000005))))
  (if (i32.eq (local.get $i) (i32.const 29)) (then (return (i32.const 0x00000009))))
  (if (i32.eq (local.get $i) (i32.const 30)) (then (return (i32.const 0x0000000e))))
  (if (i32.eq (local.get $i) (i32.const 31)) (then (return (i32.const 0x00000014))))
  (if (i32.eq (local.get $i) (i32.const 32)) (then (return (i32.const 0x00000004))))
  (if (i32.eq (local.get $i) (i32.const 33)) (then (return (i32.const 0x0000000b))))
  (if (i32.eq (local.get $i) (i32.const 34)) (then (return (i32.const 0x00000010))))
  (if (i32.eq (local.get $i) (i32.const 35)) (then (return (i32.const 0x00000017))))
  (if (i32.eq (local.get $i) (i32.const 36)) (then (return (i32.const 0x00000004))))
  (if (i32.eq (local.get $i) (i32.const 37)) (then (return (i32.const 0x0000000b))))
  (if (i32.eq (local.get $i) (i32.const 38)) (then (return (i32.const 0x00000010))))
  (if (i32.eq (local.get $i) (i32.const 39)) (then (return (i32.const 0x00000017))))
  (if (i32.eq (local.get $i) (i32.const 40)) (then (return (i32.const 0x00000004))))
  (if (i32.eq (local.get $i) (i32.const 41)) (then (return (i32.const 0x0000000b))))
  (if (i32.eq (local.get $i) (i32.const 42)) (then (return (i32.const 0x00000010))))
  (if (i32.eq (local.get $i) (i32.const 43)) (then (return (i32.const 0x00000017))))
  (if (i32.eq (local.get $i) (i32.const 44)) (then (return (i32.const 0x00000004))))
  (if (i32.eq (local.get $i) (i32.const 45)) (then (return (i32.const 0x0000000b))))
  (if (i32.eq (local.get $i) (i32.const 46)) (then (return (i32.const 0x00000010))))
  (if (i32.eq (local.get $i) (i32.const 47)) (then (return (i32.const 0x00000017))))
  (if (i32.eq (local.get $i) (i32.const 48)) (then (return (i32.const 0x00000006))))
  (if (i32.eq (local.get $i) (i32.const 49)) (then (return (i32.const 0x0000000a))))
  (if (i32.eq (local.get $i) (i32.const 50)) (then (return (i32.const 0x0000000f))))
  (if (i32.eq (local.get $i) (i32.const 51)) (then (return (i32.const 0x00000015))))
  (if (i32.eq (local.get $i) (i32.const 52)) (then (return (i32.const 0x00000006))))
  (if (i32.eq (local.get $i) (i32.const 53)) (then (return (i32.const 0x0000000a))))
  (if (i32.eq (local.get $i) (i32.const 54)) (then (return (i32.const 0x0000000f))))
  (if (i32.eq (local.get $i) (i32.const 55)) (then (return (i32.const 0x00000015))))
  (if (i32.eq (local.get $i) (i32.const 56)) (then (return (i32.const 0x00000006))))
  (if (i32.eq (local.get $i) (i32.const 57)) (then (return (i32.const 0x0000000a))))
  (if (i32.eq (local.get $i) (i32.const 58)) (then (return (i32.const 0x0000000f))))
  (if (i32.eq (local.get $i) (i32.const 59)) (then (return (i32.const 0x00000015))))
  (if (i32.eq (local.get $i) (i32.const 60)) (then (return (i32.const 0x00000006))))
  (if (i32.eq (local.get $i) (i32.const 61)) (then (return (i32.const 0x0000000a))))
  (if (i32.eq (local.get $i) (i32.const 62)) (then (return (i32.const 0x0000000f))))
  (if (i32.eq (local.get $i) (i32.const 63)) (then (return (i32.const 0x00000015))))
  (i32.const 0))
"#;

/// `__rt_md5_hex`: PHP's `md5`, returning the 32-character lowercase hex digest.
///
/// The padding rule matches SHA-1's shape — one `0x80` byte, zeros up to 56 bytes past a 64-byte
/// boundary, then the BIT length — but every word here is LITTLE-endian, which is the single
/// biggest difference between the two and the usual place a port of one into the other goes
/// wrong. The digest bytes come out little-endian per word too, so `a0`'s low byte is printed
/// first.
const RT_MD5_HEX: &str = r#"(func $__rt_md5_hex (param $ptr i32) (param $len i64) (result i32) (result i64)
  (local $blocks i32)                                             ;; padded length in 64-byte blocks
  (local $buf i32)                                                ;; padded message
  (local $i i32)                                                  ;; general cursor
  (local $t i32)                                                  ;; round counter
  (local $g i32)                                                  ;; message word index
  (local $n i32)                                                  ;; source length as i32
  (local $base i32)                                               ;; current block offset
  (local $h0 i32) (local $h1 i32) (local $h2 i32) (local $h3 i32)
  (local $a i32) (local $b i32) (local $c i32) (local $d i32)
  (local $f i32) (local $tmp i32)
  (local $out i32) (local $ow i32) (local $nib i32) (local $word i32)
  (local.set $n (i32.wrap_i64 (local.get $len)))
  (local.set $blocks
    (i32.div_u (i32.add (local.get $n) (i32.const 72)) (i32.const 64)))
  (local.set $buf (call $__rt_heap_alloc (i32.mul (local.get $blocks) (i32.const 64))))
  (local.set $i (i32.const 0))
  (block $cend (loop $copy
    (br_if $cend (i32.ge_u (local.get $i) (local.get $n)))
    (i32.store8 (i32.add (local.get $buf) (local.get $i))
      (i32.load8_u (i32.add (local.get $ptr) (local.get $i))))
    (local.set $i (i32.add (local.get $i) (i32.const 1)))
    (br $copy)))
  (i32.store8 (i32.add (local.get $buf) (local.get $i)) (i32.const 128))
  (local.set $i (i32.add (local.get $i) (i32.const 1)))
  (block $zend (loop $zeros
    (br_if $zend (i32.ge_u (local.get $i) (i32.mul (local.get $blocks) (i32.const 64))))
    (i32.store8 (i32.add (local.get $buf) (local.get $i)) (i32.const 0))
    (local.set $i (i32.add (local.get $i) (i32.const 1)))
    (br $zeros)))
  (local.set $i (i32.const 0))
  (block $lend (loop $lenbytes                                    ;; LITTLE-endian bit length
    (br_if $lend (i32.ge_u (local.get $i) (i32.const 8)))
    (i32.store8
      (i32.add (i32.sub (i32.add (local.get $buf) (i32.mul (local.get $blocks) (i32.const 64)))
                        (i32.const 8))
               (local.get $i))
      (i32.wrap_i64 (i64.and
        (i64.shr_u (i64.mul (local.get $len) (i64.const 8))
                   (i64.extend_i32_u (i32.mul (local.get $i) (i32.const 8))))
        (i64.const 255))))
    (local.set $i (i32.add (local.get $i) (i32.const 1)))
    (br $lenbytes)))
  (local.set $h0 (i32.const 0x67452301))
  (local.set $h1 (i32.const 0xEFCDAB89))
  (local.set $h2 (i32.const 0x98BADCFE))
  (local.set $h3 (i32.const 0x10325476))
  (local.set $base (i32.const 0))
  (block $bend (loop $block
    (br_if $bend (i32.ge_u (local.get $base) (i32.mul (local.get $blocks) (i32.const 64))))
    (local.set $a (local.get $h0)) (local.set $b (local.get $h1))
    (local.set $c (local.get $h2)) (local.set $d (local.get $h3))
    (local.set $t (i32.const 0))
    (block $rend (loop $rounds
      (br_if $rend (i32.ge_u (local.get $t) (i32.const 64)))
      (if (i32.lt_u (local.get $t) (i32.const 16))
        (then
          (local.set $f (i32.or (i32.and (local.get $b) (local.get $c))
                                (i32.and (i32.xor (local.get $b) (i32.const -1)) (local.get $d))))
          (local.set $g (local.get $t)))
        (else (if (i32.lt_u (local.get $t) (i32.const 32))
          (then
            (local.set $f (i32.or (i32.and (local.get $d) (local.get $b))
                                  (i32.and (i32.xor (local.get $d) (i32.const -1)) (local.get $c))))
            (local.set $g (i32.rem_u (i32.add (i32.mul (i32.const 5) (local.get $t)) (i32.const 1))
                                     (i32.const 16))))
          (else (if (i32.lt_u (local.get $t) (i32.const 48))
            (then
              (local.set $f (i32.xor (i32.xor (local.get $b) (local.get $c)) (local.get $d)))
              (local.set $g (i32.rem_u (i32.add (i32.mul (i32.const 3) (local.get $t)) (i32.const 5))
                                       (i32.const 16))))
            (else
              (local.set $f (i32.xor (local.get $c)
                                     (i32.or (local.get $b) (i32.xor (local.get $d) (i32.const -1)))))
              (local.set $g (i32.rem_u (i32.mul (i32.const 7) (local.get $t)) (i32.const 16)))))))))
      (local.set $word                                            ;; M[g], little-endian
        (i32.or (i32.or
          (i32.load8_u (i32.add (local.get $buf)
            (i32.add (local.get $base) (i32.mul (local.get $g) (i32.const 4)))))
          (i32.shl (i32.load8_u (i32.add (local.get $buf)
            (i32.add (i32.add (local.get $base) (i32.mul (local.get $g) (i32.const 4))) (i32.const 1)))) (i32.const 8)))
          (i32.or
          (i32.shl (i32.load8_u (i32.add (local.get $buf)
            (i32.add (i32.add (local.get $base) (i32.mul (local.get $g) (i32.const 4))) (i32.const 2)))) (i32.const 16))
          (i32.shl (i32.load8_u (i32.add (local.get $buf)
            (i32.add (i32.add (local.get $base) (i32.mul (local.get $g) (i32.const 4))) (i32.const 3)))) (i32.const 24)))))
      (local.set $f (i32.add (i32.add (i32.add (local.get $f) (local.get $a))
                                      (call $__rt_md5_k (local.get $t)))
                             (local.get $word)))
      (local.set $a (local.get $d))
      (local.set $d (local.get $c))
      (local.set $c (local.get $b))
      (local.set $b (i32.add (local.get $b)
        (i32.rotl (local.get $f) (call $__rt_md5_s (local.get $t)))))
      (local.set $t (i32.add (local.get $t) (i32.const 1)))
      (br $rounds)))
    (local.set $h0 (i32.add (local.get $h0) (local.get $a)))
    (local.set $h1 (i32.add (local.get $h1) (local.get $b)))
    (local.set $h2 (i32.add (local.get $h2) (local.get $c)))
    (local.set $h3 (i32.add (local.get $h3) (local.get $d)))
    (local.set $base (i32.add (local.get $base) (i32.const 64)))
    (br $block)))
  (call $__rt_heap_free (local.get $buf))
  (local.set $out (call $__rt_str_alloc (i64.const 32)))
  (local.set $ow (i32.const 0))
  (local.set $i (i32.const 0))
  (block $hend (loop $hex                                         ;; each word's bytes, low first
    (br_if $hend (i32.ge_u (local.get $i) (i32.const 32)))
    (local.set $tmp (local.get $h0))
    (if (i32.ge_u (local.get $i) (i32.const 8))  (then (local.set $tmp (local.get $h1))))
    (if (i32.ge_u (local.get $i) (i32.const 16)) (then (local.set $tmp (local.get $h2))))
    (if (i32.ge_u (local.get $i) (i32.const 24)) (then (local.set $tmp (local.get $h3))))
    (local.set $tmp
      (i32.and (i32.shr_u (local.get $tmp)
        (i32.mul (i32.div_u (i32.rem_u (local.get $i) (i32.const 8)) (i32.const 2)) (i32.const 8)))
        (i32.const 255)))                                         ;; the byte this pair prints
    (local.set $nib
      (select (i32.shr_u (local.get $tmp) (i32.const 4))
              (i32.and (local.get $tmp) (i32.const 15))
              (i32.eqz (i32.and (local.get $i) (i32.const 1)))))  ;; high nibble first within a byte
    (i32.store8 (i32.add (local.get $out) (local.get $ow))
      (i32.add (local.get $nib)
        (select (i32.const 48) (i32.const 87) (i32.lt_u (local.get $nib) (i32.const 10)))))
    (local.set $ow (i32.add (local.get $ow) (i32.const 1)))
    (local.set $i (i32.add (local.get $i) (i32.const 1)))
    (br $hex)))
  (local.get $out) (i64.const 32))                                ;; owned 32-character digest
"#;

/// `__rt_utf8_seq_len`: the length of a VALID UTF-8 sequence at `$p`, or 0 when there is none.
///
/// Rejects what the encoding forbids rather than only what is structurally short: overlong forms
/// (`C0`/`C1`, `E0 80`, `F0 80`), UTF-16 surrogates (`ED A0`..`ED BF`), and anything above
/// U+10FFFF (`F4 90`.., `F5`..`FF`). A caller that gets 0 is looking at an invalid sequence.
const RT_UTF8_SEQ_LEN: &str = r#"(func $__rt_utf8_seq_len (param $ptr i32) (param $len i64) (param $p i64) (result i32)
  (local $b i32) (local $c1 i32) (local $c2 i32) (local $c3 i32) (local $need i32)
  (local.set $b (i32.load8_u (i32.add (local.get $ptr) (i32.wrap_i64 (local.get $p)))))
  (if (i32.lt_u (local.get $b) (i32.const 128))
    (then (return (i32.const 1))))                                ;; plain ASCII
  (if (i32.lt_u (local.get $b) (i32.const 0xC2))
    (then (return (i32.const 0))))                                ;; stray continuation, or overlong C0/C1
  (local.set $need (i32.const 2))
  (if (i32.ge_u (local.get $b) (i32.const 0xE0)) (then (local.set $need (i32.const 3))))
  (if (i32.ge_u (local.get $b) (i32.const 0xF0)) (then (local.set $need (i32.const 4))))
  (if (i32.gt_u (local.get $b) (i32.const 0xF4))
    (then (return (i32.const 0))))                                ;; above U+10FFFF
  (if (i64.gt_s (i64.add (local.get $p) (i64.extend_i32_u (local.get $need))) (local.get $len))
    (then (return (i32.const 0))))                                ;; truncated at the end of input
  (local.set $c1 (i32.load8_u (i32.add (local.get $ptr)
    (i32.wrap_i64 (i64.add (local.get $p) (i64.const 1))))))
  (if (i32.ne (i32.and (local.get $c1) (i32.const 0xC0)) (i32.const 0x80))
    (then (return (i32.const 0))))                                ;; not a continuation byte
  (if (i32.eq (local.get $b) (i32.const 0xE0))
    (then (if (i32.lt_u (local.get $c1) (i32.const 0xA0)) (then (return (i32.const 0))))))  ;; overlong
  (if (i32.eq (local.get $b) (i32.const 0xED))
    (then (if (i32.ge_u (local.get $c1) (i32.const 0xA0)) (then (return (i32.const 0))))))  ;; surrogate
  (if (i32.eq (local.get $b) (i32.const 0xF0))
    (then (if (i32.lt_u (local.get $c1) (i32.const 0x90)) (then (return (i32.const 0))))))  ;; overlong
  (if (i32.eq (local.get $b) (i32.const 0xF4))
    (then (if (i32.ge_u (local.get $c1) (i32.const 0x90)) (then (return (i32.const 0))))))  ;; above U+10FFFF
  (if (i32.ge_u (local.get $need) (i32.const 3))
    (then
      (local.set $c2 (i32.load8_u (i32.add (local.get $ptr)
        (i32.wrap_i64 (i64.add (local.get $p) (i64.const 2))))))
      (if (i32.ne (i32.and (local.get $c2) (i32.const 0xC0)) (i32.const 0x80))
        (then (return (i32.const 0))))))
  (if (i32.eq (local.get $need) (i32.const 4))
    (then
      (local.set $c3 (i32.load8_u (i32.add (local.get $ptr)
        (i32.wrap_i64 (i64.add (local.get $p) (i64.const 3))))))
      (if (i32.ne (i32.and (local.get $c3) (i32.const 0xC0)) (i32.const 0x80))
        (then (return (i32.const 0))))))
  (local.get $need))                                              ;; a whole valid sequence
"#;

/// `__rt_utf8_bad_span`: how many bytes an INVALID sequence at `$p` consumes.
///
/// PHP replaces one span with a single U+FFFD, not one per byte, and the span is WIDER than the
/// usual "maximal subpart": a valid lead absorbs following bytes, up to what it announced,
/// stopping only at a byte that could START a sequence — ASCII, or a lead in `C2`..`F4`. So a
/// byte that can never lead is absorbed too, which is why `"\xc2\xc0"` yields ONE replacement
/// while `"\xc2\xc2"` yields two. A byte that is not a valid lead at all stands alone, which is
/// why `"\xc0\x80"` yields two and `"\xf5\x80\x80\x80"` yields four.
///
/// Measured over every byte pair across the lead and continuation boundaries; a plain
/// continuation-byte test gets 102 of those pairs wrong.
const RT_UTF8_BAD_SPAN: &str = r#"(func $__rt_utf8_bad_span (param $ptr i32) (param $len i64) (param $p i64) (result i64)
  (local $b i32) (local $need i32) (local $span i64) (local $next i32)
  (local.set $b (i32.load8_u (i32.add (local.get $ptr) (i32.wrap_i64 (local.get $p)))))
  (if (i32.or (i32.lt_u (local.get $b) (i32.const 0xC2)) (i32.gt_u (local.get $b) (i32.const 0xF4)))
    (then (return (i64.const 1))))                                ;; never a lead byte: it stands alone
  (local.set $need (i32.const 2))
  (if (i32.ge_u (local.get $b) (i32.const 0xE0)) (then (local.set $need (i32.const 3))))
  (if (i32.ge_u (local.get $b) (i32.const 0xF0)) (then (local.set $need (i32.const 4))))
  (local.set $span (i64.const 1))                                 ;; the lead byte itself
  (block $end (loop $follow
    (br_if $end (i64.ge_s (local.get $span) (i64.extend_i32_u (local.get $need))))
    (br_if $end (i64.ge_s (i64.add (local.get $p) (local.get $span)) (local.get $len)))
    (local.set $next (i32.load8_u (i32.add (local.get $ptr)
      (i32.wrap_i64 (i64.add (local.get $p) (local.get $span))))))
    (br_if $end (i32.lt_u (local.get $next) (i32.const 128)))     ;; ASCII can start a sequence
    (br_if $end (i32.and (i32.ge_u (local.get $next) (i32.const 0xC2))
                         (i32.le_u (local.get $next) (i32.const 0xF4))))  ;; so can a valid lead
    (local.set $span (i64.add (local.get $span) (i64.const 1)))   ;; anything else belongs to the bad span
    (br $follow)))
  (local.get $span))                                              ;; the maximal subpart
"#;

/// `__rt_htmlspecialchars`: PHP's `htmlspecialchars` under its 8.1+ default flags.
///
/// The defaults are `ENT_QUOTES | ENT_SUBSTITUTE | ENT_HTML401`, which means BOTH quote styles
/// are escaped — `'` becomes `&#039;`, not left alone as it was before 8.1 — and invalid UTF-8 is
/// replaced with U+FFFD rather than making the whole call return the empty string. Control bytes
/// and NUL are valid UTF-8 and pass through untouched.
///
/// Worst case is six output bytes per input byte, which `&quot;` and `&#039;` both reach.
const RT_HTMLSPECIALCHARS: &str = r#"(func $__rt_htmlspecialchars (param $ptr i32) (param $len i64) (result i32) (result i64)
  (local $out i32) (local $i i64) (local $w i32) (local $b i32) (local $n i32) (local $span i64)
  (local.set $out (call $__rt_str_alloc (i64.mul (local.get $len) (i64.const 6))))
  (local.set $i (i64.const 0))
  (local.set $w (i32.const 0))
  (block $end (loop $scan
    (br_if $end (i64.ge_s (local.get $i) (local.get $len)))
    (local.set $b (i32.load8_u (i32.add (local.get $ptr) (i32.wrap_i64 (local.get $i)))))
    (if (i32.eq (local.get $b) (i32.const 38))                    ;; &
      (then
        (local.set $w (call $__rt_html_put (local.get $out) (local.get $w) (i32.const 0)))
        (local.set $i (i64.add (local.get $i) (i64.const 1))))
      (else (if (i32.eq (local.get $b) (i32.const 60))            ;; <
        (then
          (local.set $w (call $__rt_html_put (local.get $out) (local.get $w) (i32.const 1)))
          (local.set $i (i64.add (local.get $i) (i64.const 1))))
        (else (if (i32.eq (local.get $b) (i32.const 62))          ;; >
          (then
            (local.set $w (call $__rt_html_put (local.get $out) (local.get $w) (i32.const 2)))
            (local.set $i (i64.add (local.get $i) (i64.const 1))))
          (else (if (i32.eq (local.get $b) (i32.const 34))        ;; "
            (then
              (local.set $w (call $__rt_html_put (local.get $out) (local.get $w) (i32.const 3)))
              (local.set $i (i64.add (local.get $i) (i64.const 1))))
            (else (if (i32.eq (local.get $b) (i32.const 39))      ;; '
              (then
                (local.set $w (call $__rt_html_put (local.get $out) (local.get $w) (i32.const 4)))
                (local.set $i (i64.add (local.get $i) (i64.const 1))))
              (else
                (local.set $n (call $__rt_utf8_seq_len (local.get $ptr) (local.get $len) (local.get $i)))
                (if (local.get $n)
                  (then                                           ;; a valid sequence: copy it whole
                    (block $cend (loop $copy
                      (br_if $cend (i32.eqz (local.get $n)))
                      (i32.store8 (i32.add (local.get $out) (local.get $w))
                        (i32.load8_u (i32.add (local.get $ptr) (i32.wrap_i64 (local.get $i)))))
                      (local.set $w (i32.add (local.get $w) (i32.const 1)))
                      (local.set $i (i64.add (local.get $i) (i64.const 1)))
                      (local.set $n (i32.sub (local.get $n) (i32.const 1)))
                      (br $copy))))
                  (else                                           ;; one U+FFFD per maximal subpart
                    (local.set $span (call $__rt_utf8_bad_span (local.get $ptr) (local.get $len) (local.get $i)))
                    (i32.store8 (i32.add (local.get $out) (local.get $w)) (i32.const 0xEF))
                    (i32.store8 offset=1 (i32.add (local.get $out) (local.get $w)) (i32.const 0xBF))
                    (i32.store8 offset=2 (i32.add (local.get $out) (local.get $w)) (i32.const 0xBD))
                    (local.set $w (i32.add (local.get $w) (i32.const 3)))
                    (local.set $i (i64.add (local.get $i) (local.get $span)))))))))))))))
    (br $scan)))
  (local.get $out) (i64.extend_i32_u (local.get $w)))
"#;

/// `__rt_html_put`: writes one HTML entity and returns the advanced cursor.
///
/// Selected by index rather than by byte so the caller's dispatch stays a flat comparison chain:
/// 0 `&amp;`, 1 `&lt;`, 2 `&gt;`, 3 `&quot;`, 4 `&#039;`.
const RT_HTML_PUT: &str = r#"(func $__rt_html_put (param $out i32) (param $w i32) (param $which i32) (result i32)
  (i32.store8 (i32.add (local.get $out) (local.get $w)) (i32.const 38))   ;; every entity opens with &
  (local.set $w (i32.add (local.get $w) (i32.const 1)))
  (if (i32.eqz (local.get $which))
    (then                                                          ;; amp;
      (i32.store8 (i32.add (local.get $out) (local.get $w)) (i32.const 97))
      (i32.store8 offset=1 (i32.add (local.get $out) (local.get $w)) (i32.const 109))
      (i32.store8 offset=2 (i32.add (local.get $out) (local.get $w)) (i32.const 112))
      (i32.store8 offset=3 (i32.add (local.get $out) (local.get $w)) (i32.const 59))
      (return (i32.add (local.get $w) (i32.const 4)))))
  (if (i32.eq (local.get $which) (i32.const 1))
    (then                                                          ;; lt;
      (i32.store8 (i32.add (local.get $out) (local.get $w)) (i32.const 108))
      (i32.store8 offset=1 (i32.add (local.get $out) (local.get $w)) (i32.const 116))
      (i32.store8 offset=2 (i32.add (local.get $out) (local.get $w)) (i32.const 59))
      (return (i32.add (local.get $w) (i32.const 3)))))
  (if (i32.eq (local.get $which) (i32.const 2))
    (then                                                          ;; gt;
      (i32.store8 (i32.add (local.get $out) (local.get $w)) (i32.const 103))
      (i32.store8 offset=1 (i32.add (local.get $out) (local.get $w)) (i32.const 116))
      (i32.store8 offset=2 (i32.add (local.get $out) (local.get $w)) (i32.const 59))
      (return (i32.add (local.get $w) (i32.const 3)))))
  (if (i32.eq (local.get $which) (i32.const 3))
    (then                                                          ;; quot;
      (i32.store8 (i32.add (local.get $out) (local.get $w)) (i32.const 113))
      (i32.store8 offset=1 (i32.add (local.get $out) (local.get $w)) (i32.const 117))
      (i32.store8 offset=2 (i32.add (local.get $out) (local.get $w)) (i32.const 111))
      (i32.store8 offset=3 (i32.add (local.get $out) (local.get $w)) (i32.const 116))
      (i32.store8 offset=4 (i32.add (local.get $out) (local.get $w)) (i32.const 59))
      (return (i32.add (local.get $w) (i32.const 5)))))
  (i32.store8 (i32.add (local.get $out) (local.get $w)) (i32.const 35))   ;; #039;
  (i32.store8 offset=1 (i32.add (local.get $out) (local.get $w)) (i32.const 48))
  (i32.store8 offset=2 (i32.add (local.get $out) (local.get $w)) (i32.const 51))
  (i32.store8 offset=3 (i32.add (local.get $out) (local.get $w)) (i32.const 57))
  (i32.store8 offset=4 (i32.add (local.get $out) (local.get $w)) (i32.const 59))
  (i32.add (local.get $w) (i32.const 5)))
"#;

/// `__rt_str_rfind`: the LAST index of a needle in a haystack, or -1 when it is absent.
///
/// Scanning runs from the last offset that could still fit the needle down to zero, so overlapping
/// matches resolve to the rightmost one — `strrpos("aaa", "aa")` is 1, not 0. An empty needle
/// starts the scan at `hlen` and matches immediately, which is why `strrpos("abcabc", "")` is 6:
/// the position just past the end.
const RT_STR_RFIND: &str = r#"(func $__rt_str_rfind (param $hptr i32) (param $hlen i64) (param $nptr i32) (param $nlen i64) (result i64)
  (local $at i64)                                                 ;; candidate start offset
  (if (i64.gt_s (local.get $nlen) (local.get $hlen))
    (then (return (i64.const -1))))                               ;; cannot fit anywhere
  (local.set $at (i64.sub (local.get $hlen) (local.get $nlen)))   ;; the rightmost offset that fits
  (block $none (loop $scan
    (if (i32.wrap_i64 (call $__rt_str_region_eq
          (local.get $hptr) (local.get $nptr) (local.get $nlen) (local.get $at)))
      (then (return (local.get $at))))                            ;; rightmost match wins
    (br_if $none (i64.le_s (local.get $at) (i64.const 0)))         ;; offset zero was the last try
    (local.set $at (i64.sub (local.get $at) (i64.const 1)))       ;; walk left
    (br $scan)))
  (i64.const -1))                                                 ;; absent
"#;

/// `__rt_implode`: owns the elements of an indexed STRING array joined by a glue string.
///
/// Two passes rather than a worst-case reservation: the total is the sum of the element lengths
/// plus `(count - 1)` glues, which is exactly knowable before writing, so the result block is
/// sized precisely. An empty array joins to the empty string with no glue at all, and a
/// single-element array to that element — the glue count is one FEWER than the elements, which is
/// the off-by-one this shape makes hard to get wrong.
const RT_IMPLODE: &str = r#"(func $__rt_implode (param $array i32) (param $gptr i32) (param $glen i64) (result i32) (result i64)
  (local $n i64)                                                  ;; element count
  (local $i i64)                                                  ;; element cursor
  (local $j i64)                                                  ;; byte cursor within a piece
  (local $total i64)                                              ;; exact output length
  (local $eptr i32)                                               ;; current element pointer
  (local $elen i64)                                               ;; current element length
  (local $out i32)                                                ;; owned result block
  (local $w i32)                                                  ;; destination cursor
  (local.set $n (i64.load (local.get $array)))                    ;; the length lives at [array+0]
  (local.set $total (i64.const 0))
  (local.set $i (i64.const 0))
  (block $mend (loop $measure
    (br_if $mend (i64.ge_s (local.get $i) (local.get $n)))
    (call $__rt_array_get_str (local.get $array) (local.get $i))
    (local.set $elen)                                             ;; pop the length
    (local.set $eptr)                                             ;; pop the pointer
    (local.set $total (i64.add (local.get $total) (local.get $elen)))
    (local.set $i (i64.add (local.get $i) (i64.const 1)))
    (br $measure)))
  (if (i64.gt_s (local.get $n) (i64.const 0))
    (then (local.set $total (i64.add (local.get $total)
      (i64.mul (i64.sub (local.get $n) (i64.const 1)) (local.get $glen))))))  ;; one fewer glue than elements
  (local.set $out (call $__rt_str_alloc (local.get $total)))
  (local.set $w (i32.const 0))
  (local.set $i (i64.const 0))
  (block $wend (loop $write
    (br_if $wend (i64.ge_s (local.get $i) (local.get $n)))
    (if (i64.gt_s (local.get $i) (i64.const 0))
      (then                                                       ;; glue goes BETWEEN, not after
        (local.set $j (i64.const 0))
        (block $gend (loop $glue
          (br_if $gend (i64.ge_s (local.get $j) (local.get $glen)))
          (i32.store8 (i32.add (local.get $out) (local.get $w))
            (i32.load8_u (i32.add (local.get $gptr) (i32.wrap_i64 (local.get $j)))))
          (local.set $w (i32.add (local.get $w) (i32.const 1)))
          (local.set $j (i64.add (local.get $j) (i64.const 1)))
          (br $glue)))))
    (call $__rt_array_get_str (local.get $array) (local.get $i))
    (local.set $elen)                                             ;; pop the length
    (local.set $eptr)                                             ;; pop the pointer
    (local.set $j (i64.const 0))
    (block $eend (loop $elem
      (br_if $eend (i64.ge_s (local.get $j) (local.get $elen)))
      (i32.store8 (i32.add (local.get $out) (local.get $w))
        (i32.load8_u (i32.add (local.get $eptr) (i32.wrap_i64 (local.get $j)))))
      (local.set $w (i32.add (local.get $w) (i32.const 1)))
      (local.set $j (i64.add (local.get $j) (i64.const 1)))
      (br $elem)))
    (local.set $i (i64.add (local.get $i) (i64.const 1)))
    (br $write)))
  (local.get $out) (i64.extend_i32_u (local.get $w)))             ;; owned result
"#;

/// `__rt_explode`: splits a string on a separator into a freshly built indexed string array.
///
/// Scanning is left to right and non-overlapping. Every separator produces a boundary, so a
/// leading or trailing one yields an EMPTY element rather than being trimmed away, and the tail
/// after the last separator is always pushed — which is why `explode(",", "")` is `[""]`, one
/// empty element, and never the empty array. The caller has already refused an empty separator,
/// which is what would otherwise make this loop forever.
///
/// `__rt_array_push_str` may reallocate, so its result is threaded back rather than discarded.
const RT_EXPLODE: &str = r#"(func $__rt_explode (param $sptr i32) (param $slen i64) (param $pptr i32) (param $plen i64) (result i32)
  (local $arr i32)                                                ;; the array being built
  (local $i i64)                                                  ;; scan cursor
  (local $start i64)                                              ;; start of the current piece
  (local.set $arr (call $__rt_array_new (i64.const 4) (i64.const 16)))  ;; specialized on first push
  (local.set $i (i64.const 0))
  (local.set $start (i64.const 0))
  (block $end (loop $scan
    (br_if $end (i64.gt_s (i64.add (local.get $i) (local.get $plen)) (local.get $slen)))
    (if (i32.wrap_i64 (call $__rt_str_region_eq
          (local.get $sptr) (local.get $pptr) (local.get $plen) (local.get $i)))
      (then
        (local.set $arr (call $__rt_array_push_str (local.get $arr)
          (i32.add (local.get $sptr) (i32.wrap_i64 (local.get $start)))
          (i64.sub (local.get $i) (local.get $start))))           ;; the piece before this separator
        (local.set $i (i64.add (local.get $i) (local.get $plen))) ;; skip it, never rescan
        (local.set $start (local.get $i)))
      (else (local.set $i (i64.add (local.get $i) (i64.const 1)))))
    (br $scan)))
  (call $__rt_array_push_str (local.get $arr)
    (i32.add (local.get $sptr) (i32.wrap_i64 (local.get $start)))
    (i64.sub (local.get $slen) (local.get $start))))              ;; the tail is always an element
"#;

/// `__rt_str_split`: cuts a string into fixed-size chunks as a fresh indexed string array.
///
/// The final chunk is SHORT when the length does not divide evenly, and an EMPTY subject yields
/// the EMPTY array rather than one empty element — which is PHP 8.2's behaviour and the opposite
/// of `explode`, where the tail is always pushed. The caller has already refused a non-positive
/// chunk length, which is what would otherwise make this loop forever.
const RT_STR_SPLIT: &str = r#"(func $__rt_str_split (param $ptr i32) (param $len i64) (param $chunk i64) (result i32)
  (local $arr i32)                                                ;; the array being built
  (local $i i64)                                                  ;; start of the current chunk
  (local $take i64)                                               ;; bytes in the current chunk
  (local.set $arr (call $__rt_array_new (i64.const 4) (i64.const 16)))
  (local.set $i (i64.const 0))
  (block $end (loop $cut
    (br_if $end (i64.ge_s (local.get $i) (local.get $len)))       ;; an empty subject yields no chunks
    (local.set $take (i64.sub (local.get $len) (local.get $i)))
    (if (i64.gt_s (local.get $take) (local.get $chunk))
      (then (local.set $take (local.get $chunk))))                ;; the last chunk may be short
    (local.set $arr (call $__rt_array_push_str (local.get $arr)
      (i32.add (local.get $ptr) (i32.wrap_i64 (local.get $i)))
      (local.get $take)))
    (local.set $i (i64.add (local.get $i) (local.get $take)))
    (br $cut)))
  (local.get $arr))                                               ;; the built array
"#;

/// `__rt_wordwrap_general`: PHP's `wordwrap` with an arbitrary break string and the cut flag.
///
/// php-src's general branch, transcribed and validated on 314 cases against 8.5.6 — 14 chosen
/// plus 300 generated over an alphabet of just `a`, `b`, `c` and space, which is where the awkward
/// shapes live: `wordwrap("a  b", 2, "-", true)` is `a -b` (the first space BECOMES the break, the
/// second survives) and `wordwrap("  lead", 3, "-", true)` is ` -lea-d`.
///
/// Unlike the one-byte in-place form this BUILDS its result, because a multi-byte break and a cut
/// both lengthen the text. The bound is one break per input byte plus one, which no input can
/// exceed.
///
/// `laststart` is where the current line began and `lastspace` the most recent space. A space at
/// or past the width breaks THERE; any other byte at or past it breaks back at the last space, or
/// — only when cutting — right here when the line holds no space at all. An occurrence of the
/// break string in the input resets both.
const RT_WORDWRAP_GENERAL: &str = r#"(func $__rt_wordwrap_general (param $ptr i32) (param $len i64) (param $width i64) (param $bptr i32) (param $blen i64) (param $cut i32) (result i32) (result i64)
  (local $out i32)
  (local $olen i64)
  (local $cur i64)
  (local $laststart i64)
  (local $lastspace i64)
  (local $b i32)
  (local $n i64)
  (local.set $out (call $__rt_str_alloc
    (i64.add (local.get $len) (i64.mul (i64.add (local.get $len) (i64.const 1)) (local.get $blen)))))
  (block $end (loop $scan
    (br_if $end (i64.ge_s (local.get $cur) (local.get $len)))
    (local.set $b (i32.load8_u (i32.add (local.get $ptr) (i32.wrap_i64 (local.get $cur)))))
    (if (i32.and (i32.and (i64.gt_s (local.get $blen) (i64.const 0))
                          (i64.le_s (i64.add (local.get $cur) (local.get $blen)) (local.get $len)))
                 (i32.wrap_i64 (call $__rt_str_region_eq
                   (local.get $ptr) (local.get $bptr) (local.get $blen) (local.get $cur))))
      (then                                                       ;; the break already occurs here
        (local.set $n (i64.sub (i64.add (local.get $cur) (local.get $blen)) (local.get $laststart)))
        (memory.copy (i32.add (local.get $out) (i32.wrap_i64 (local.get $olen)))
                     (i32.add (local.get $ptr) (i32.wrap_i64 (local.get $laststart)))
                     (i32.wrap_i64 (local.get $n)))
        (local.set $olen (i64.add (local.get $olen) (local.get $n)))
        (local.set $laststart (i64.add (local.get $cur) (local.get $blen)))
        (local.set $lastspace (local.get $laststart))
        (local.set $cur (i64.sub (i64.add (local.get $cur) (local.get $blen)) (i64.const 1))))
      (else (if (i32.eq (local.get $b) (i32.const 32))             ;; a space may become the break
        (then
          (if (i64.ge_s (i64.sub (local.get $cur) (local.get $laststart)) (local.get $width))
            (then
              (local.set $n (i64.sub (local.get $cur) (local.get $laststart)))
              (memory.copy (i32.add (local.get $out) (i32.wrap_i64 (local.get $olen)))
                           (i32.add (local.get $ptr) (i32.wrap_i64 (local.get $laststart)))
                           (i32.wrap_i64 (local.get $n)))
              (local.set $olen (i64.add (local.get $olen) (local.get $n)))
              (memory.copy (i32.add (local.get $out) (i32.wrap_i64 (local.get $olen)))
                           (local.get $bptr) (i32.wrap_i64 (local.get $blen)))
              (local.set $olen (i64.add (local.get $olen) (local.get $blen)))
              (local.set $laststart (i64.add (local.get $cur) (i64.const 1)))))
          (local.set $lastspace (local.get $cur)))
        (else (if (i64.ge_s (i64.sub (local.get $cur) (local.get $laststart)) (local.get $width))
          (then (if (i64.ge_s (local.get $laststart) (local.get $lastspace))
            (then (if (local.get $cut)                             ;; no space to break at
              (then
                (local.set $n (i64.sub (local.get $cur) (local.get $laststart)))
                (memory.copy (i32.add (local.get $out) (i32.wrap_i64 (local.get $olen)))
                             (i32.add (local.get $ptr) (i32.wrap_i64 (local.get $laststart)))
                             (i32.wrap_i64 (local.get $n)))
                (local.set $olen (i64.add (local.get $olen) (local.get $n)))
                (memory.copy (i32.add (local.get $out) (i32.wrap_i64 (local.get $olen)))
                             (local.get $bptr) (i32.wrap_i64 (local.get $blen)))
                (local.set $olen (i64.add (local.get $olen) (local.get $blen)))
                (local.set $laststart (local.get $cur))
                (local.set $lastspace (local.get $cur)))))
            (else                                                  ;; break back at the last space
              (local.set $n (i64.sub (local.get $lastspace) (local.get $laststart)))
              (memory.copy (i32.add (local.get $out) (i32.wrap_i64 (local.get $olen)))
                           (i32.add (local.get $ptr) (i32.wrap_i64 (local.get $laststart)))
                           (i32.wrap_i64 (local.get $n)))
              (local.set $olen (i64.add (local.get $olen) (local.get $n)))
              (memory.copy (i32.add (local.get $out) (i32.wrap_i64 (local.get $olen)))
                           (local.get $bptr) (i32.wrap_i64 (local.get $blen)))
              (local.set $olen (i64.add (local.get $olen) (local.get $blen)))
              (local.set $laststart (i64.add (local.get $lastspace) (i64.const 1)))
              (local.set $lastspace (local.get $laststart))))))))))
    (local.set $cur (i64.add (local.get $cur) (i64.const 1)))
    (br $scan)))
  (if (i64.ne (local.get $laststart) (local.get $cur))             ;; the tail after the last break
    (then
      (local.set $n (i64.sub (local.get $cur) (local.get $laststart)))
      (memory.copy (i32.add (local.get $out) (i32.wrap_i64 (local.get $olen)))
                   (i32.add (local.get $ptr) (i32.wrap_i64 (local.get $laststart)))
                   (i32.wrap_i64 (local.get $n)))
      (local.set $olen (i64.add (local.get $olen) (local.get $n)))))
  (local.get $out)
  (local.get $olen))
"#;

/// `__rt_wordwrap`: PHP's `wordwrap` with the default one-byte break and no long-word cutting.
///
/// The transform is IN PLACE and length-preserving: a space is REPLACED by the break, never
/// inserted, which is why `wordwrap("a ", 1)` is `"a\n"` — the trailing space becomes the break —
/// and why a word longer than the width is left whole rather than split.
///
/// This is php-src's fast path, transcribed: `laststart` is where the current line began,
/// `lastspace` the most recent space seen. A space at or past the width breaks THERE; any other
/// byte at or past the width breaks at the last space instead, but only when one has been seen
/// since the line started — `laststart != lastspace` is what leaves an unbreakable run alone. An
/// existing break resets both. A width of zero or less needs no special case: the comparison is
/// then always true.
const RT_WORDWRAP: &str = r#"(func $__rt_wordwrap (param $ptr i32) (param $len i64) (param $width i64) (result i32) (result i64)
  (local $out i32)                                                ;; owned result block
  (local $olen i64)                                               ;; persisted length
  (local $cur i64)                                                ;; scan cursor
  (local $laststart i64)                                          ;; start of the current line
  (local $lastspace i64)                                          ;; most recent space
  (local $b i32)                                                  ;; current byte
  (call $__rt_str_persist (local.get $ptr) (local.get $len))      ;; own a copy to rewrite in place
  (local.set $olen)
  (local.set $out)
  (local.set $cur (i64.const 0))
  (local.set $laststart (i64.const 0))
  (local.set $lastspace (i64.const 0))
  (block $end (loop $scan
    (br_if $end (i64.ge_s (local.get $cur) (local.get $olen)))
    (local.set $b (i32.load8_u (i32.add (local.get $out) (i32.wrap_i64 (local.get $cur)))))
    (if (i32.eq (local.get $b) (i32.const 10))                    ;; an existing break resets the line
      (then
        (local.set $laststart (i64.add (local.get $cur) (i64.const 1)))
        (local.set $lastspace (local.get $laststart)))
      (else (if (i32.eq (local.get $b) (i32.const 32))            ;; a space can become the break
        (then
          (if (i64.ge_s (i64.sub (local.get $cur) (local.get $laststart)) (local.get $width))
            (then
              (i32.store8 (i32.add (local.get $out) (i32.wrap_i64 (local.get $cur))) (i32.const 10))
              (local.set $laststart (i64.add (local.get $cur) (i64.const 1)))))
          (local.set $lastspace (local.get $cur)))
        (else
          (if (i32.and
                (i64.ge_s (i64.sub (local.get $cur) (local.get $laststart)) (local.get $width))
                (i64.ne (local.get $laststart) (local.get $lastspace)))
            (then                                                 ;; break back at the last space
              (i32.store8 (i32.add (local.get $out) (i32.wrap_i64 (local.get $lastspace))) (i32.const 10))
              (local.set $laststart (i64.add (local.get $lastspace) (i64.const 1)))))))))
    (local.set $cur (i64.add (local.get $cur) (i64.const 1)))
    (br $scan)))
  (local.get $out) (local.get $olen))                             ;; owned result, same length
"#;

/// One piece of a `sprintf` format that has been resolved at COMPILE time.
///
/// The format is required to be a literal, so the parse happens once here rather than becoming a
/// format interpreter in the emitted module. That is the whole point of doing this in an AOT
/// compiler: a call whose format is known becomes a fixed sequence of appends.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum FormatPiece {
    /// A contiguous run of the FORMAT's own bytes, given as an offset and length into it.
    ///
    /// Referencing the source rather than copying means the run needs no data segment of its
    /// own: the format literal is already laid out, so a run is a slice of it. `%%` is emitted
    /// as a one-byte run pointing at the first `%` of the pair, which is what keeps every run
    /// contiguous even though the pair collapses.
    Literal { offset: usize, length: usize },
    /// One conversion, with its argument already resolved to a zero-based index.
    Conversion {
        argument: usize,
        conversion: u8,
        left: bool,
        plus: bool,
        pad: u8,
        width: usize,
        precision: Option<usize>,
    },
}

/// Parses a `sprintf` format into pieces, or explains why this backend will not lower it.
///
/// The admitted subset is `%%`, `%d`, `%s` and `%f` with php-src's flags, width and precision.
/// Every rule here was measured against php-src 8.5.6 rather than taken from C:
///
/// - the LAST padding flag wins, so `%'x03d` pads with zeros and `%0'x3d` pads with `x`;
/// - `-` cancels a ZERO pad on `%d` and `%f` but NOT on `%s`, and never cancels an explicit
///   `'X` — `%-08d` is space-padded while `%-03s` is zero-padded;
/// - the space flag is IGNORED by PHP, unlike C.
///
/// `-` together with `0` on `%f` is refused rather than reproduced: php-src loses the precision
/// there (`%-08.2f` of 1.5 gives "1.500000"), which is a quirk this target should not bake in.
pub(super) fn parse_sprintf_format(
    format: &[u8],
    argument_count: usize,
) -> std::result::Result<Vec<FormatPiece>, String> {
    let mut pieces = Vec::new();
    let mut run_start = 0usize;
    let mut next_positional = 0usize;
    let mut i = 0usize;
    while i < format.len() {
        if format[i] != b'%' {
            i += 1;
            continue;
        }
        if i > run_start {
            pieces.push(FormatPiece::Literal {
                offset: run_start,
                length: i - run_start,
            });
        }
        let percent = i;
        i += 1;
        if i >= format.len() {
            return Err("format ends with a lone '%'".to_string());
        }
        if format[i] == b'%' {
            pieces.push(FormatPiece::Literal {
                offset: percent,
                length: 1,
            });
            i += 1;
            run_start = i;
            continue;
        }
        // An explicit `N$` argument selector, which may repeat an argument.
        let start = i;
        let mut digits = 0usize;
        while i < format.len() && format[i].is_ascii_digit() {
            digits = digits * 10 + usize::from(format[i] - b'0');
            i += 1;
        }
        let mut argument = None;
        if i > start && i < format.len() && format[i] == b'$' {
            if digits == 0 {
                return Err("argument numbers start at 1".to_string());
            }
            argument = Some(digits - 1);
            i += 1;
        } else {
            i = start;
        }

        let mut left = false;
        let mut plus = false;
        let mut pad = b' ';
        let mut zero_flag = false;
        loop {
            if i >= format.len() {
                return Err("format ends inside a conversion".to_string());
            }
            match format[i] {
                b'-' => left = true,
                b'+' => plus = true,
                b'0' => {
                    pad = b'0';
                    zero_flag = true;
                }
                b'\'' => {
                    i += 1;
                    if i >= format.len() {
                        return Err("a padding flag needs the character after it".to_string());
                    }
                    pad = format[i];
                    zero_flag = false;
                }
                _ => break,
            }
            i += 1;
        }

        let mut width = 0usize;
        while i < format.len() && format[i].is_ascii_digit() {
            width = width * 10 + usize::from(format[i] - b'0');
            i += 1;
        }
        let mut precision = None;
        if i < format.len() && format[i] == b'.' {
            i += 1;
            let mut value = 0usize;
            while i < format.len() && format[i].is_ascii_digit() {
                value = value * 10 + usize::from(format[i] - b'0');
                i += 1;
            }
            precision = Some(value);
        }
        if i >= format.len() {
            return Err("format ends before its conversion character".to_string());
        }
        let conversion = format[i];
        i += 1;
        if !matches!(conversion, b'd' | b's' | b'f' | b'x' | b'X' | b'b' | b'o') {
            return Err(format!(
                "conversion '%{}' is outside the lowered subset (%%, %d, %s, %f, %x, %X, %b, %o)",
                conversion as char
            ));
        }
        if left && zero_flag && conversion == b'f' {
            return Err(
                "'-' with '0' on %f loses the precision in php-src; refused rather than copied"
                    .to_string(),
            );
        }
        // Left-justifying cancels a zero pad on the numeric conversions only.
        if left && pad == b'0' && matches!(conversion, b'd' | b'f') {
            pad = b' ';
        }
        let argument = match argument {
            Some(explicit) => explicit,
            None => {
                let next = next_positional;
                next_positional += 1;
                next
            }
        };
        if argument >= argument_count {
            return Err(format!(
                "format wants argument #{} but {argument_count} were passed",
                argument + 1
            ));
        }
        run_start = i;
        pieces.push(FormatPiece::Conversion {
            argument,
            conversion,
            left,
            plus,
            pad,
            width,
            precision,
        });
    }
    if format.len() > run_start {
        pieces.push(FormatPiece::Literal {
            offset: run_start,
            length: format.len() - run_start,
        });
    }
    Ok(pieces)
}

/// `__rt_fmt_pad`: places a rendered body inside its field width, sign included.
///
/// The two padding characters behave differently, which is measured php-src behaviour rather
/// than a choice: ZEROS go AFTER the sign — `%05d` of -7 is `-0007` — while spaces and an
/// explicit `'X` go BEFORE it, so `%'x8.2f` of -1.5 is `xxx-1.50`. `$sign` is 0 when there is
/// none, otherwise the character to emit.
const RT_FMT_PAD: &str = r#"(func $__rt_fmt_pad (param $bptr i32) (param $blen i64) (param $sign i32) (param $width i64) (param $pad i32) (param $left i32) (result i32) (result i64)
  (local $signed i64)                                             ;; body length plus any sign
  (local $total i64)                                              ;; final field length
  (local $fill i64)                                               ;; padding characters to add
  (local $out i32)                                                ;; owned result block
  (local $w i32)                                                  ;; write cursor
  (local $i i64)                                                  ;; copy cursor
  (local.set $signed (i64.add (local.get $blen)
    (i64.extend_i32_u (i32.ne (local.get $sign) (i32.const 0)))))
  (local.set $total (local.get $signed))
  (if (i64.gt_s (local.get $width) (local.get $total))
    (then (local.set $total (local.get $width))))                 ;; a shorter width never truncates
  (local.set $fill (i64.sub (local.get $total) (local.get $signed)))
  (local.set $out (call $__rt_str_alloc (local.get $total)))
  (local.set $w (i32.const 0))
  (if (i32.and (i32.eqz (local.get $left)) (i32.ne (local.get $pad) (i32.const 48)))
    (then                                                         ;; spaces and 'X precede the sign
      (block $e1 (loop $l1
        (br_if $e1 (i64.le_s (local.get $fill) (i64.const 0)))
        (i32.store8 (i32.add (local.get $out) (local.get $w)) (local.get $pad))
        (local.set $w (i32.add (local.get $w) (i32.const 1)))
        (local.set $fill (i64.sub (local.get $fill) (i64.const 1)))
        (br $l1)))))
  (if (i32.ne (local.get $sign) (i32.const 0))
    (then
      (i32.store8 (i32.add (local.get $out) (local.get $w)) (local.get $sign))
      (local.set $w (i32.add (local.get $w) (i32.const 1)))))
  (if (i32.and (i32.eqz (local.get $left)) (i32.eq (local.get $pad) (i32.const 48)))
    (then                                                         ;; zeros follow the sign
      (block $e2 (loop $l2
        (br_if $e2 (i64.le_s (local.get $fill) (i64.const 0)))
        (i32.store8 (i32.add (local.get $out) (local.get $w)) (i32.const 48))
        (local.set $w (i32.add (local.get $w) (i32.const 1)))
        (local.set $fill (i64.sub (local.get $fill) (i64.const 1)))
        (br $l2)))))
  (local.set $i (i64.const 0))
  (block $e3 (loop $l3
    (br_if $e3 (i64.ge_s (local.get $i) (local.get $blen)))
    (i32.store8 (i32.add (local.get $out) (local.get $w))
      (i32.load8_u (i32.add (local.get $bptr) (i32.wrap_i64 (local.get $i)))))
    (local.set $w (i32.add (local.get $w) (i32.const 1)))
    (local.set $i (i64.add (local.get $i) (i64.const 1)))
    (br $l3)))
  (block $e4 (loop $l4                                            ;; left-justified padding trails
    (br_if $e4 (i64.le_s (local.get $fill) (i64.const 0)))
    (i32.store8 (i32.add (local.get $out) (local.get $w)) (local.get $pad))
    (local.set $w (i32.add (local.get $w) (i32.const 1)))
    (local.set $fill (i64.sub (local.get $fill) (i64.const 1)))
    (br $l4)))
  (local.get $out) (i64.extend_i32_u (local.get $w)))
"#;

/// `__rt_fmt_int`: renders `%d` — decimal digits, an optional sign, then the field.
///
/// The digits are produced without a sign so `__rt_fmt_pad` can place it, which is what makes
/// `%05d` of -7 come out as `-0007` rather than `000-7`. `PHP_INT_MIN` is negated in UNSIGNED
/// space, where its magnitude is representable.
const RT_FMT_INT: &str = r#"(func $__rt_fmt_int (param $value i64) (param $plus i32) (param $width i64) (param $pad i32) (param $left i32) (result i32) (result i64)
  (local $mag i64)                                                ;; magnitude, sign removed
  (local $sign i32)                                               ;; the sign character, or 0
  (local $digits i32)                                             ;; scratch for the digits
  (local $n i32)                                                  ;; digit count
  (local $w i32)                                                  ;; back-to-front cursor
  (local.set $sign (i32.const 0))
  (if (i64.lt_s (local.get $value) (i64.const 0))
    (then
      (local.set $sign (i32.const 45))                             ;; '-'
      (local.set $mag (i64.sub (i64.const 0) (local.get $value)))) ;; wraps only at PHP_INT_MIN,
    (else                                                          ;; whose unsigned form is right
      (local.set $mag (local.get $value))
      (if (local.get $plus) (then (local.set $sign (i32.const 43)))))) ;; '+'
  (local.set $digits (call $__rt_str_alloc (i64.const 24)))       ;; an i64 needs at most 20
  (local.set $w (i32.const 24))
  (block $end (loop $emit
    (local.set $w (i32.sub (local.get $w) (i32.const 1)))
    (i32.store8 (i32.add (local.get $digits) (local.get $w))
      (i32.add (i32.const 48)
        (i32.wrap_i64 (i64.rem_u (local.get $mag) (i64.const 10)))))
    (local.set $mag (i64.div_u (local.get $mag) (i64.const 10)))
    (br_if $end (i64.eqz (local.get $mag)))                       ;; zero still emits one digit
    (br $emit)))
  (local.set $n (i32.sub (i32.const 24) (local.get $w)))
  (call $__rt_fmt_pad (i32.add (local.get $digits) (local.get $w))
    (i64.extend_i32_u (local.get $n)) (local.get $sign)
    (local.get $width) (local.get $pad) (local.get $left)))
"#;

/// `__rt_fmt_uint_radix`: renders `%x`, `%X`, `%b` and `%o` — the value as UNSIGNED, in `$radix`.
///
/// PHP reads the argument as an unsigned 64-bit word for these, so `-1` prints as
/// `ffffffffffffffff` and no sign is ever emitted, whatever the `+` flag says. Measured across
/// 0, 1, 255, 4095, -1, -255 and both i64 extremes in all four conversions.
///
/// 64 binary digits is the widest any radix produces, so the scratch is sized for that.
const RT_FMT_UINT_RADIX: &str = r#"(func $__rt_fmt_uint_radix (param $value i64) (param $radix i64) (param $upper i32) (param $width i64) (param $pad i32) (param $left i32) (result i32) (result i64)
  (local $digits i32)
  (local $n i32)
  (local $w i32)
  (local $d i32)
  (local.set $digits (call $__rt_str_alloc (i64.const 64)))       ;; %b of -1 is 64 digits
  (local.set $w (i32.const 64))
  (block $end (loop $emit
    (local.set $w (i32.sub (local.get $w) (i32.const 1)))
    (local.set $d (i32.wrap_i64 (i64.rem_u (local.get $value) (local.get $radix))))
    (if (i32.lt_u (local.get $d) (i32.const 10))
      (then (local.set $d (i32.add (local.get $d) (i32.const 48))))         ;; '0'..'9'
      (else (local.set $d (i32.add (local.get $d)
              (select (i32.const 55) (i32.const 87) (local.get $upper)))))) ;; 'A'-10 or 'a'-10
    (i32.store8 (i32.add (local.get $digits) (local.get $w)) (local.get $d))
    (local.set $value (i64.div_u (local.get $value) (local.get $radix)))
    (br_if $end (i64.eqz (local.get $value)))                     ;; zero still emits one digit
    (br $emit)))
  (local.set $n (i32.sub (i32.const 64) (local.get $w)))
  (call $__rt_fmt_pad (i32.add (local.get $digits) (local.get $w))
    (i64.extend_i32_u (local.get $n)) (i32.const 0)               ;; never a sign character
    (local.get $width) (local.get $pad) (local.get $left)))
"#;

/// `__rt_fmt_str`: renders `%s` — the string, truncated by any precision, then the field.
///
/// A precision TRUNCATES rather than pads, so `%.2s` of "abcdef" is "ab"; `$has_prec` tells an
/// absent precision from an explicit `.0`, which yields the empty string.
const RT_FMT_STR: &str = r#"(func $__rt_fmt_str (param $ptr i32) (param $len i64) (param $prec i64) (param $has_prec i32) (param $width i64) (param $pad i32) (param $left i32) (result i32) (result i64)
  (if (i32.and (local.get $has_prec) (i64.lt_s (local.get $prec) (local.get $len)))
    (then (local.set $len (local.get $prec))))                    ;; precision cuts, never extends
  (call $__rt_fmt_pad (local.get $ptr) (local.get $len) (i32.const 0)
    (local.get $width) (local.get $pad) (local.get $left)))
"#;

/// Recovers a `sprintf` format's literal bytes, or `None` when it is not a literal.
///
/// The whole design rests on this: a known format is parsed once at compile time, so the module
/// carries a fixed sequence of appends instead of a format interpreter. A computed format has no
/// such sequence and is refused.
fn sprintf_literal_format(
    function: &Function,
    module: &Module,
    value: crate::ir::ValueId,
) -> Option<Vec<u8>> {
    let defined = function.value(value)?;
    let crate::ir::ValueDef::Instruction { inst, .. } = defined.def else {
        return None;
    };
    let defining = function.instruction(inst)?;
    if defining.op != Op::ConstStr {
        return None;
    }
    let Some(Immediate::Data(id)) = defining.immediate else {
        return None;
    };
    let interned = module.data.strings.get(id.as_raw() as usize)?;
    Some(crate::string_bytes::literal_bytes(interned))
}

/// Resolves the data segment the literal format was laid out in.
fn sprintf_format_segment(ctx: &FnCtx, value: crate::ir::ValueId) -> Result<(u32, u32)> {
    let defined = ctx
        .function
        .value(value)
        .ok_or_else(|| WasmError::Unsupported("sprintf format value is missing".to_string()))?;
    let crate::ir::ValueDef::Instruction { inst, .. } = defined.def else {
        return Err(WasmError::Unsupported(
            "sprintf format is not a literal".to_string(),
        ));
    };
    let defining = ctx.function.instruction(inst).ok_or_else(|| {
        WasmError::Unsupported("sprintf format has no defining instruction".to_string())
    })?;
    let Some(Immediate::Data(id)) = defining.immediate else {
        return Err(WasmError::Unsupported(
            "sprintf format is not a data literal".to_string(),
        ));
    };
    ctx.str_literals
        .get(id.as_raw() as usize)
        .copied()
        .ok_or_else(|| WasmError::Unsupported("sprintf format has no data segment".to_string()))
}

/// Lowers `sprintf` with a literal format into a fixed sequence of appends.
fn lower_sprintf(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    emit_formatted_string(ctx, inst)?;
    store_result(ctx, inst)
}

/// Lowers `printf`: the same formatting, written to stdout, answering the BYTE count.
///
/// PHP returns the number of BYTES written rather than characters, so `printf("h\xc3\xa9")`
/// answers 3. The length is already on the stack from the shared builder, which is why this is
/// the same work as `sprintf` plus one write.
fn lower_printf(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let length = ctx.fresh_temp(super::wat::ValType::I64);
    emit_formatted_string(ctx, inst)?;
    ctx.fb
        .ins(&format!("local.tee {length}"), "keep the byte count to answer with");
    ctx.fb.ins("call $__rt_echo_str", "write the formatted bytes");
    ctx.fb
        .ins(&format!("local.get {length}"), "PHP answers the byte count");
    store_result(ctx, inst)
}

/// Builds the formatted string, leaving `(pointer, length)` on the stack.
fn emit_formatted_string(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let format_value = operand(inst, 0)?;
    let format = sprintf_literal_format(ctx.function, ctx.module, format_value)
        .ok_or_else(|| WasmError::Unsupported("sprintf format is not a literal".to_string()))?;
    let pieces = parse_sprintf_format(&format, inst.operands.len() - 1)
        .map_err(|why| WasmError::Unsupported(format!("sprintf format: {why}")))?;
    // Every literal run is a slice of the format's own data segment, so its address is that
    // segment's base plus the run's offset. No run needs a segment of its own.
    let format_base = sprintf_format_segment(ctx, format_value)?.0;

    let acc_ptr = ctx.fresh_temp(super::wat::ValType::I32);
    let acc_len = ctx.fresh_temp(super::wat::ValType::I64);
    // Start from the empty string so a format with no pieces still yields one, and so every
    // piece below is an ordinary append rather than a special first case.
    ctx.fb.ins("i32.const 0", "empty accumulator pointer");
    ctx.fb.ins(&format!("local.set {acc_ptr}"), "accumulator");
    ctx.fb.ins("i64.const 0", "empty accumulator length");
    ctx.fb.ins(&format!("local.set {acc_len}"), "accumulator length");

    for piece in &pieces {
        ctx.fb.ins(&format!("local.get {acc_ptr}"), "accumulated so far");
        ctx.fb.ins(&format!("local.get {acc_len}"), "accumulated length");
        match piece {
            FormatPiece::Literal { offset, length } => {
                ctx.fb.ins(
                    &format!("i32.const {}", format_base + *offset as u32),
                    "a run of the format's own bytes",
                );
                ctx.fb.ins(&format!("i64.const {length}"), "its byte length");
            }
            FormatPiece::Conversion {
                argument,
                conversion,
                left,
                plus,
                pad,
                width,
                precision,
            } => {
                let value = operand(inst, argument + 1)?;
                match conversion {
                    b'd' => {
                        ctx.emit_load_value(value)?;
                        ctx.fb.ins(
                            &format!("i32.const {}", i32::from(*plus)),
                            "does a positive value carry a plus?",
                        );
                        ctx.fb.ins(&format!("i64.const {width}"), "field width");
                        ctx.fb.ins(&format!("i32.const {pad}"), "padding character");
                        ctx.fb
                            .ins(&format!("i32.const {}", i32::from(*left)), "left-justified?");
                        ctx.fb.ins("call $__rt_fmt_int", "render %d");
                    }
                    b'x' | b'X' | b'b' | b'o' => {
                        // PHP reads these as UNSIGNED, so no sign is ever emitted and the `+`
                        // flag does not reach them.
                        let (radix, upper) = match conversion {
                            b'x' => (16, 0),
                            b'X' => (16, 1),
                            b'b' => (2, 0),
                            _ => (8, 0),
                        };
                        ctx.emit_load_value(value)?;
                        ctx.fb.ins(&format!("i64.const {radix}"), "radix");
                        ctx.fb
                            .ins(&format!("i32.const {upper}"), "uppercase alphabet?");
                        ctx.fb.ins(&format!("i64.const {width}"), "field width");
                        ctx.fb.ins(&format!("i32.const {pad}"), "padding character");
                        ctx.fb
                            .ins(&format!("i32.const {}", i32::from(*left)), "left-justified?");
                        ctx.fb
                            .ins("call $__rt_fmt_uint_radix", "render an unsigned radix field");
                    }
                    b's' => {
                        ctx.emit_load_value(value)?;
                        ctx.fb.ins(
                            &format!("i64.const {}", precision.unwrap_or(0)),
                            "precision, when one was written",
                        );
                        ctx.fb.ins(
                            &format!("i32.const {}", i32::from(precision.is_some())),
                            "an absent precision is not a zero one",
                        );
                        ctx.fb.ins(&format!("i64.const {width}"), "field width");
                        ctx.fb.ins(&format!("i32.const {pad}"), "padding character");
                        ctx.fb
                            .ins(&format!("i32.const {}", i32::from(*left)), "left-justified?");
                        ctx.fb.ins("call $__rt_fmt_str", "render %s");
                    }
                    b'f' => {
                        ctx.emit_load_value(value)?;
                        ctx.fb
                            .ins("i64.reinterpret_f64", "the exact bits are what gets rounded");
                        ctx.fb.ins(
                            &format!("i64.const {}", precision.unwrap_or(6)),
                            "an absent precision is PHP's six, not zero",
                        );
                        ctx.fb.ins(
                            &format!("i32.const {}", i32::from(*plus)),
                            "does a positive value carry a plus?",
                        );
                        ctx.fb.ins(&format!("i64.const {width}"), "field width");
                        ctx.fb.ins(&format!("i32.const {pad}"), "padding character");
                        ctx.fb
                            .ins(&format!("i32.const {}", i32::from(*left)), "left-justified?");
                        ctx.fb.ins("call $__rt_fmt_float", "render %f");
                    }
                    other => {
                        return Err(WasmError::Unsupported(format!(
                            "sprintf conversion %{}",
                            *other as char
                        )))
                    }
                }
            }
        }
        ctx.fb.ins("call $__rt_concat", "append this piece");
        ctx.fb.ins(&format!("local.set {acc_len}"), "new length");
        ctx.fb.ins(&format!("local.set {acc_ptr}"), "new accumulator");
    }
    ctx.fb.ins(&format!("local.get {acc_ptr}"), "the formatted string");
    ctx.fb.ins(&format!("local.get {acc_len}"), "its length");
    Ok(())
}

/// Validates `sprintf`: a LITERAL format whose conversions match the argument types.
fn sprintf_shape_issue(
    function: &Function,
    module: &Module,
    call: &Instruction,
    target: RuntimeFnId,
) -> Option<String> {
    let Some(format_value) = call.operands.first() else {
        return Some("sprintf needs a format".to_string());
    };
    let Some(format) = sprintf_literal_format(function, module, *format_value) else {
        return Some(
            "only a LITERAL format is lowered; a computed one would need a format interpreter"
                .to_string(),
        );
    };
    let pieces = match parse_sprintf_format(&format, call.operands.len() - 1) {
        Ok(pieces) => pieces,
        Err(why) => return Some(format!("sprintf format: {why}")),
    };
    for piece in &pieces {
        let FormatPiece::Conversion {
            argument,
            conversion,
            ..
        } = piece
        else {
            continue;
        };
        let Some(value) = function.value(call.operands[argument + 1]) else {
            return Some("argument is missing from the value table".to_string());
        };
        let (want_ir, want_php) = match conversion {
            b'd' | b'x' | b'X' | b'b' | b'o' => (IrType::I64, PhpType::Int),
            b'f' => (IrType::F64, PhpType::Float),
            _ => (IrType::Str, PhpType::Str),
        };
        // PHP coerces an argument to the conversion's type; that coercion carries its own
        // diagnostics, so an exact match is required rather than converted here.
        if value.ir_type != want_ir || value.php_type.codegen_repr() != want_php {
            return Some(format!(
                "%{} wants {want_ir:?}/{want_php:?}, argument #{} is {:?}/{:?}",
                *conversion as char,
                argument + 1,
                value.ir_type,
                value.php_type.codegen_repr()
            ));
        }
    }
    // `printf` writes the string and answers its BYTE count; `sprintf` answers the string.
    let (want_ir, want_php) = if target == RuntimeFnId::Printf {
        (IrType::I64, PhpType::Int)
    } else {
        (IrType::Str, PhpType::Str)
    };
    if call.result.is_none()
        || call.result_type != want_ir
        || call.result_php_type.codegen_repr() != want_php
    {
        return Some(format!(
            "{target:?} result {:?}/{:?} is not the expected {want_ir:?}/{want_php:?}",
            call.result_type,
            call.result_php_type.codegen_repr()
        ));
    }
    None
}

/// `__rt_fmt_float`: renders `%f` from the EXACT decimal digits of the double.
///
/// PHP's `%f` rounds the exact binary value with ties-to-even — `%.0f` of 0.5 is 0 and of 2.5 is
/// 2 — which is C's rule and NOT `number_format`'s. `number_format` rounds the shortest decimal
/// that round-trips, so `number_format(2.675, 2)` is 2.68 while `sprintf("%.2f", 2.675)` is 2.67.
/// Working from `__rt_f64_digits`' exact digits is what makes the difference reproducible.
///
/// Non-finite values IGNORE the field entirely: `%08.2f` of INF is `INF`, not `00000INF`, and the
/// `+` flag does not reach them either — measured. A true zero drops its sign, so `-0.0` prints
/// `0.00`, while a negative value that merely ROUNDS to zero keeps it and prints `-0.00`.
const RT_FMT_FLOAT: &str = r#"(func $__rt_fmt_float (param $bits i64) (param $prec i64) (param $plus i32) (param $width i64) (param $pad i32) (param $left i32) (result i32) (result i64)
  (local $scratch i32) (local $big i32) (local $dbuf i32) (local $kept i32) (local $body i32)
  (local $sign i32) (local $class i32) (local $digptr i32) (local $ndigits i32) (local $p i32)
  (local $keep i64) (local $klen i64) (local $i i64) (local $first i32) (local $up i32)
  (local $rest i64) (local $carry i64) (local $intlen i64) (local $w i32) (local $signch i32)
  (local.set $scratch (call $__rt_heap_alloc
    (i32.add (i32.const 2112) (i32.wrap_i64 (i64.mul (local.get $prec) (i64.const 2))))))
  (local.set $big (local.get $scratch))                           ;; 80 limbs, must start zeroed
  (local.set $dbuf (i32.add (local.get $scratch) (i32.const 640)))   ;; 792 bytes: see __rt_f64_digits
  (local.set $kept (i32.add (local.get $scratch) (i32.const 1440)))  ;; past dbuf's 640+792
  (local.set $i (i64.const 0))
  (block $ze (loop $zl                                            ;; zero the bignum limbs
    (br_if $ze (i64.ge_s (local.get $i) (i64.const 640)))
    (i32.store8 (i32.add (local.get $big) (i32.wrap_i64 (local.get $i))) (i32.const 0))
    (local.set $i (i64.add (local.get $i) (i64.const 1)))
    (br $zl)))
  (call $__rt_f64_digits (local.get $bits) (local.get $big) (i32.const 80)
        (local.get $dbuf) (i32.const 792))
  (local.set $p) (local.set $ndigits) (local.set $digptr)
  (local.set $class) (local.set $sign)
  (if (i32.eq (local.get $class) (i32.const 1))                   ;; infinity ignores the field
    (then
      (local.set $body (call $__rt_str_alloc (i64.const 4)))
      (local.set $w (i32.const 0))
      (if (local.get $sign)
        (then
          (i32.store8 (local.get $body) (i32.const 45))
          (local.set $w (i32.const 1))))
      (i32.store8 (i32.add (local.get $body) (local.get $w)) (i32.const 73))          ;; I
      (i32.store8 offset=1 (i32.add (local.get $body) (local.get $w)) (i32.const 78)) ;; N
      (i32.store8 offset=2 (i32.add (local.get $body) (local.get $w)) (i32.const 70)) ;; F
      (return (local.get $body) (i64.extend_i32_u (i32.add (local.get $w) (i32.const 3))))))
  (if (i32.eq (local.get $class) (i32.const 2))                   ;; PHP spells it NaN, not NAN
    (then
      (local.set $body (call $__rt_str_alloc (i64.const 3)))
      (i32.store8 (local.get $body) (i32.const 78))               ;; N
      (i32.store8 offset=1 (local.get $body) (i32.const 97))      ;; a
      (i32.store8 offset=2 (local.get $body) (i32.const 78))      ;; N
      (return (local.get $body) (i64.const 3))))
  (if (i32.eq (local.get $class) (i32.const 3))
    (then                                                          ;; a true zero has no digits
      (local.set $ndigits (i32.const 0))
      (local.set $p (i32.const 0))
      (local.set $sign (i32.const 0))))                            ;; ...and drops its sign
  (local.set $keep (i64.add (i64.sub (i64.extend_i32_s (local.get $ndigits))
                                     (i64.extend_i32_s (local.get $p)))
                            (local.get $prec)))
  (local.set $klen (local.get $keep))
  (if (i64.lt_s (local.get $klen) (i64.const 0))
    (then (local.set $klen (i64.const 0))))
  (local.set $i (i64.const 0))
  (block $ce (loop $cl                                            ;; the kept prefix, zero-extended
    (br_if $ce (i64.ge_s (local.get $i) (local.get $klen)))
    (i32.store8 (i32.add (local.get $kept) (i32.wrap_i64 (local.get $i)))
      (select
        (i32.load8_u (i32.add (local.get $digptr) (i32.wrap_i64 (local.get $i))))
        (i32.const 48)
        (i64.lt_s (local.get $i) (i64.extend_i32_s (local.get $ndigits)))))
    (local.set $i (i64.add (local.get $i) (i64.const 1)))
    (br $cl)))
  (local.set $up (i32.const 0))
  (if (i32.and (i64.ge_s (local.get $keep) (i64.const 0))
               (i64.lt_s (local.get $keep) (i64.extend_i32_s (local.get $ndigits))))
    (then
      (local.set $first (i32.sub
        (i32.load8_u (i32.add (local.get $digptr) (i32.wrap_i64 (local.get $keep))))
        (i32.const 48)))
      (local.set $rest (i64.add (local.get $keep) (i64.const 1)))
      (local.set $carry (i64.const 0))                            ;; reused: any non-zero tail?
      (block $re (loop $rl
        (br_if $re (i64.ge_s (local.get $rest) (i64.extend_i32_s (local.get $ndigits))))
        (if (i32.ne (i32.load8_u (i32.add (local.get $digptr) (i32.wrap_i64 (local.get $rest))))
                    (i32.const 48))
          (then (local.set $carry (i64.const 1)) (br $re)))
        (local.set $rest (i64.add (local.get $rest) (i64.const 1)))
        (br $rl)))
      (if (i32.gt_u (local.get $first) (i32.const 5))
        (then (local.set $up (i32.const 1))))
      (if (i32.eq (local.get $first) (i32.const 5))
        (then (if (i64.ne (local.get $carry) (i64.const 0))
          (then (local.set $up (i32.const 1)))
          (else                                                    ;; an exact tie rounds to EVEN
            (if (i64.gt_s (local.get $klen) (i64.const 0))
              (then (local.set $up (i32.and (i32.const 1)
                (i32.sub (i32.load8_u (i32.add (local.get $kept)
                  (i32.wrap_i64 (i64.sub (local.get $klen) (i64.const 1))))) (i32.const 48))))))))))))
  (if (local.get $up)
    (then
      (local.set $i (local.get $klen))
      (block $ue (loop $ul                                        ;; propagate the carry leftwards
        (br_if $ue (i64.le_s (local.get $i) (i64.const 0)))
        (local.set $i (i64.sub (local.get $i) (i64.const 1)))
        (if (i32.eq (i32.load8_u (i32.add (local.get $kept) (i32.wrap_i64 (local.get $i))))
                    (i32.const 57))                               ;; '9' wraps and carries on
          (then (i32.store8 (i32.add (local.get $kept) (i32.wrap_i64 (local.get $i))) (i32.const 48)))
          (else
            (i32.store8 (i32.add (local.get $kept) (i32.wrap_i64 (local.get $i)))
              (i32.add (i32.load8_u (i32.add (local.get $kept) (i32.wrap_i64 (local.get $i))))
                       (i32.const 1)))
            (local.set $up (i32.const 0))                         ;; absorbed
            (br $ue)))
        (br $ul)))
      (if (local.get $up)
        (then                                                     ;; every digit was 9: prepend 1
          (local.set $i (local.get $klen))
          (block $se (loop $sl
            (br_if $se (i64.le_s (local.get $i) (i64.const 0)))
            (i32.store8 (i32.add (local.get $kept) (i32.wrap_i64 (local.get $i)))
              (i32.load8_u (i32.add (local.get $kept)
                (i32.wrap_i64 (i64.sub (local.get $i) (i64.const 1))))))
            (local.set $i (i64.sub (local.get $i) (i64.const 1)))
            (br $sl)))
          (i32.store8 (local.get $kept) (i32.const 49))            ;; '1'
          (local.set $klen (i64.add (local.get $klen) (i64.const 1)))
          (local.set $keep (i64.add (local.get $keep) (i64.const 1)))))))
  (local.set $intlen (i64.sub (local.get $keep) (local.get $prec)))
  (local.set $body (call $__rt_str_alloc
    (i64.add (i64.add (local.get $klen) (local.get $prec)) (i64.const 8))))
  (local.set $w (i32.const 0))
  (if (i64.le_s (local.get $intlen) (i64.const 0))
    (then                                                          ;; no integer digits: a bare 0
      (i32.store8 (local.get $body) (i32.const 48))
      (local.set $w (i32.const 1)))
    (else
      (local.set $i (i64.const 0))
      (block $ie (loop $il
        (br_if $ie (i64.ge_s (local.get $i) (local.get $intlen)))
        (i32.store8 (i32.add (local.get $body) (local.get $w))
          (i32.load8_u (i32.add (local.get $kept) (i32.wrap_i64 (local.get $i)))))
        (local.set $w (i32.add (local.get $w) (i32.const 1)))
        (local.set $i (i64.add (local.get $i) (i64.const 1)))
        (br $il)))))
  (if (i64.gt_s (local.get $prec) (i64.const 0))
    (then
      (i32.store8 (i32.add (local.get $body) (local.get $w)) (i32.const 46))   ;; '.'
      (local.set $w (i32.add (local.get $w) (i32.const 1)))
      (local.set $i (i64.const 0))
      (block $fe (loop $fl
        (br_if $fe (i64.ge_s (local.get $i) (local.get $prec)))
        ;; the fraction starts at intlen; a negative intlen means leading zeros first
        (i32.store8 (i32.add (local.get $body) (local.get $w))
          (select (i32.const 48)
            (select
              (i32.load8_u (i32.add (local.get $kept)
                (i32.wrap_i64 (i64.add (local.get $intlen) (local.get $i)))))
              (i32.const 48)
              (i64.lt_s (i64.add (local.get $intlen) (local.get $i)) (local.get $klen)))
            (i64.lt_s (i64.add (local.get $intlen) (local.get $i)) (i64.const 0))))
        (local.set $w (i32.add (local.get $w) (i32.const 1)))
        (local.set $i (i64.add (local.get $i) (i64.const 1)))
        (br $fl)))))
  (call $__rt_heap_free (local.get $scratch))
  (local.set $signch (i32.const 0))
  (if (local.get $sign)
    (then (local.set $signch (i32.const 45)))
    (else (if (local.get $plus) (then (local.set $signch (i32.const 43))))))
  (call $__rt_fmt_pad (local.get $body) (i64.extend_i32_u (local.get $w))
    (local.get $signch) (local.get $width) (local.get $pad) (local.get $left)))
"#;

/// `__rt_array_find_str`: first index whose 16-byte (pointer, length) slot equals the needle,
/// or -1.
///
/// Lives here rather than beside the other two scans because it compares through
/// `__rt_strict_str_eq` and `__rt_str_loose_eq`, which the array runtime is emitted before.
///
/// `$strict` picks the comparison: `===` is byte equality, while `==` is php-src's
/// `zendi_smart_strcmp` — so `in_array("1e1", ["10"])` is TRUE loosely and FALSE strictly.
const RT_ARRAY_FIND_STR: &str = r#"(func $__rt_array_find_str (param $nptr i32) (param $nlen i64) (param $array i32) (param $strict i32) (result i64)
  (local $i i64) (local $len i64) (local $slot i32) (local $ep i32) (local $el i64)
  (if (i32.eqz (local.get $array))
    (then (return (i64.const -1))))
  (local.set $len (i64.load (local.get $array)))
  (block $done (loop $scan
    (br_if $done (i64.ge_s (local.get $i) (local.get $len)))
    (local.set $slot (i32.add (i32.add (local.get $array) (i32.const 24))
                              (i32.wrap_i64 (i64.mul (local.get $i) (i64.const 16)))))
    (local.set $ep (i32.wrap_i64 (i64.load (local.get $slot))))
    (local.set $el (i64.load (i32.add (local.get $slot) (i32.const 8))))
    (if (local.get $strict)
      (then
        (if (call $__rt_strict_str_eq (local.get $nptr) (local.get $nlen) (local.get $ep) (local.get $el))
          (then (return (local.get $i)))))
      (else
        (if (i64.ne (call $__rt_str_loose_eq (local.get $nptr) (local.get $nlen) (local.get $ep) (local.get $el)) (i64.const 0))
          (then (return (local.get $i))))))
    (local.set $i (i64.add (local.get $i) (i64.const 1)))
    (br $scan)))
  (i64.const -1))
"#;
/// `__rt_implode_owned`: joins an array whose elements must be CONVERTED to strings first.
///
/// `$kind` selects the conversion: 0 renders an integer slot, 1 casts a boxed `Mixed` cell. PHP
/// converts each element with the same rule as an explicit `(string)` cast — measured element by
/// element — so the Mixed arm reuses `__rt_mixed_cast_string` rather than inventing one.
///
/// Both conversions PRODUCE an owned block, so each piece is released once copied; skipping that
/// would leak one string per element. The string-element case has no conversion and no ownership
/// to manage, which is why it keeps its own simpler helper.
///
/// Two passes over a scratch table rather than repeated concatenation: converting once and
/// remembering `(pointer, length)` keeps this linear, where pairwise appending would copy the
/// accumulated prefix again for every element.
const RT_IMPLODE_OWNED: &str = r#"(func $__rt_implode_owned (param $array i32) (param $gptr i32) (param $glen i64) (param $kind i32) (result i32) (result i64)
  (local $cell i32)
  (local $n i64) (local $i i64) (local $j i64) (local $total i64)
  (local $table i32) (local $eptr i32) (local $elen i64) (local $out i32) (local $w i32)
  (local.set $n (i64.load (local.get $array)))
  (local.set $table (call $__rt_heap_alloc
    (i32.add (i32.const 8) (i32.wrap_i64 (i64.mul (local.get $n) (i64.const 16))))))
  (local.set $total (i64.const 0))
  (local.set $i (i64.const 0))
  (block $me (loop $ml                                            ;; cast once, remember the piece
    (br_if $me (i64.ge_s (local.get $i) (local.get $n)))
    (if (i32.eq (local.get $kind) (i32.const 1))
      (then
        ;; a Mixed-cell array has 16-byte slots (value_type 7) with the cell pointer at
        ;; slot+0 — NOT the 8-byte stride __rt_array_get_int walks
        (call $__rt_mixed_cast_string
          (i32.wrap_i64 (i64.load (i32.add (i32.add (local.get $array) (i32.const 24))
                                           (i32.wrap_i64 (i64.mul (local.get $i) (i64.const 16)))))))
        (local.set $elen (i64.extend_i32_u))                       ;; the cast answers an i32 length
        (local.set $eptr))
      (else (if (i32.eq (local.get $kind) (i32.const 2))
        (then
          ;; a float renders exactly as the (string) cast does, and only the cast knows that
          ;; rule — so box the slot's bits into a throwaway tag-2 cell and reuse it
          (local.set $cell (call $__rt_mixed_from_value (i64.const 2)
            (i64.load (i32.add (i32.add (local.get $array) (i32.const 24))
                               (i32.wrap_i64 (i64.mul (local.get $i) (i64.const 8)))))
            (i64.const 0)))
          (call $__rt_mixed_cast_string (local.get $cell))
          (local.set $elen (i64.extend_i32_u))
          (local.set $eptr)
          (call $__rt_decref_any (local.get $cell)))               ;; the box was ours alone
        (else
          (call $__rt_fmt_int (call $__rt_array_get_int (local.get $array) (local.get $i))
                (i32.const 0) (i64.const 0) (i32.const 32) (i32.const 0))  ;; plain decimal, no field
          (local.set $elen)
          (local.set $eptr)))))
    (i32.store (i32.add (local.get $table) (i32.wrap_i64 (i64.mul (local.get $i) (i64.const 16))))
               (local.get $eptr))
    (i64.store offset=8 (i32.add (local.get $table)
                                 (i32.wrap_i64 (i64.mul (local.get $i) (i64.const 16))))
               (local.get $elen))
    (local.set $total (i64.add (local.get $total) (local.get $elen)))
    (local.set $i (i64.add (local.get $i) (i64.const 1)))
    (br $ml)))
  (if (i64.gt_s (local.get $n) (i64.const 0))
    (then (local.set $total (i64.add (local.get $total)
      (i64.mul (i64.sub (local.get $n) (i64.const 1)) (local.get $glen))))))
  (local.set $out (call $__rt_str_alloc (local.get $total)))
  (local.set $w (i32.const 0))
  (local.set $i (i64.const 0))
  (block $we (loop $wl
    (br_if $we (i64.ge_s (local.get $i) (local.get $n)))
    (if (i64.gt_s (local.get $i) (i64.const 0))
      (then                                                       ;; glue goes BETWEEN, not after
        (local.set $j (i64.const 0))
        (block $ge (loop $gl
          (br_if $ge (i64.ge_s (local.get $j) (local.get $glen)))
          (i32.store8 (i32.add (local.get $out) (local.get $w))
            (i32.load8_u (i32.add (local.get $gptr) (i32.wrap_i64 (local.get $j)))))
          (local.set $w (i32.add (local.get $w) (i32.const 1)))
          (local.set $j (i64.add (local.get $j) (i64.const 1)))
          (br $gl)))))
    (local.set $eptr (i32.load (i32.add (local.get $table)
      (i32.wrap_i64 (i64.mul (local.get $i) (i64.const 16))))))
    (local.set $elen (i64.load offset=8 (i32.add (local.get $table)
      (i32.wrap_i64 (i64.mul (local.get $i) (i64.const 16))))))
    (local.set $j (i64.const 0))
    (block $ee (loop $el
      (br_if $ee (i64.ge_s (local.get $j) (local.get $elen)))
      (i32.store8 (i32.add (local.get $out) (local.get $w))
        (i32.load8_u (i32.add (local.get $eptr) (i32.wrap_i64 (local.get $j)))))
      (local.set $w (i32.add (local.get $w) (i32.const 1)))
      (local.set $j (i64.add (local.get $j) (i64.const 1)))
      (br $el)))
    (if (local.get $eptr)
      (then (call $__rt_heap_free (local.get $eptr))))            ;; the cast owned it; release it
    (local.set $i (i64.add (local.get $i) (i64.const 1)))
    (br $wl)))
  (call $__rt_heap_free (local.get $table))
  (local.get $out) (i64.extend_i32_u (local.get $w)))
"#;

/// Returns whether a unary string transform is lowered by this module.
///
/// Admitted: the exact same-length BYTE transforms, the re-encoders whose rules are pure byte
/// arithmetic (hex expansion, backslash escaping, line-break tagging), and base64 and url coding,
/// whose alphabets are contiguous enough to compute rather than tabulate. Still out are the html
/// entity decoders, which need a real named-entity table, and `hex2bin`, whose PHP result is
/// `string|false` rather than the `string` this family's signature promises.
pub(super) fn unary_string_is_supported(target: UnaryStringRuntime) -> bool {
    matches!(
        target,
        UnaryStringRuntime::StrToUpper
            | UnaryStringRuntime::StrToLower
            | UnaryStringRuntime::StrReverse
            | UnaryStringRuntime::BinToHex
            | UnaryStringRuntime::AddSlashes
            | UnaryStringRuntime::StripSlashes
            | UnaryStringRuntime::NlToBr
            | UnaryStringRuntime::UrlEncode
            | UnaryStringRuntime::RawUrlEncode
            | UnaryStringRuntime::UrlDecode
            | UnaryStringRuntime::RawUrlDecode
            | UnaryStringRuntime::Base64Encode
            | UnaryStringRuntime::Base64Decode
    )
}

/// Validates one unary string transform: a string in, a string out.
pub(super) fn unary_string_shape_issue(
    function: &Function,
    call: &Instruction,
    target: UnaryStringRuntime,
) -> Option<String> {
    let [operand] = call.operands.as_slice() else {
        return Some(format!(
            "expected one string operand, got {}",
            call.operands.len()
        ));
    };
    let Some(value) = function.value(*operand) else {
        return Some("string operand is missing from the value table".to_string());
    };
    if value.ir_type != IrType::Str || value.php_type.codegen_repr() != PhpType::Str {
        return Some(format!(
            "expected a string operand, got {:?}/{:?}",
            value.ir_type,
            value.php_type.codegen_repr()
        ));
    }
    if call.result.is_none()
        || call.result_type != IrType::Str
        || call.result_php_type.codegen_repr() != PhpType::Str
    {
        return Some(format!(
            "{target:?} result {:?}/{:?} is not the expected Str/Str",
            call.result_type,
            call.result_php_type.codegen_repr()
        ));
    }
    None
}

/// Lowers one unary string transform to its byte-mapping helper.
pub(super) fn lower_unary_string(
    ctx: &mut FnCtx,
    inst: &Instruction,
    target: UnaryStringRuntime,
) -> Result<()> {
    ctx.emit_load_value(operand(inst, 0)?)?;
    match target {
        UnaryStringRuntime::StrToUpper => {
            ctx.fb.ins("i32.const 1", "map towards upper case");
            ctx.fb
                .ins("call $__rt_str_map_case", "ASCII-only case mapping");
        }
        UnaryStringRuntime::StrToLower => {
            ctx.fb.ins("i32.const 0", "map towards lower case");
            ctx.fb
                .ins("call $__rt_str_map_case", "ASCII-only case mapping");
        }
        UnaryStringRuntime::StrReverse => {
            ctx.fb
                .ins("call $__rt_str_reverse", "reverse the bytes");
        }
        UnaryStringRuntime::BinToHex => {
            ctx.fb
                .ins("call $__rt_str_bin2hex", "expand each byte to two hex digits");
        }
        UnaryStringRuntime::AddSlashes => {
            ctx.fb
                .ins("call $__rt_str_addslashes", "escape quotes, backslash and NUL");
        }
        UnaryStringRuntime::StripSlashes => {
            ctx.fb
                .ins("call $__rt_str_stripslashes", "remove one level of backslash escaping");
        }
        UnaryStringRuntime::NlToBr => {
            ctx.fb
                .ins("call $__rt_str_nl2br", "insert a break tag before each line break");
        }
        UnaryStringRuntime::UrlEncode => {
            ctx.fb.ins("i32.const 0", "urlencode folds a space to a plus");
            ctx.fb
                .ins("call $__rt_str_url_encode", "percent-encode the reserved bytes");
        }
        UnaryStringRuntime::RawUrlEncode => {
            ctx.fb.ins("i32.const 1", "rawurlencode keeps a tilde and encodes a space");
            ctx.fb
                .ins("call $__rt_str_url_encode", "percent-encode the reserved bytes");
        }
        UnaryStringRuntime::UrlDecode => {
            ctx.fb.ins("i32.const 1", "urldecode reads a plus as a space");
            ctx.fb
                .ins("call $__rt_str_url_decode", "percent-decode tolerantly");
        }
        UnaryStringRuntime::RawUrlDecode => {
            ctx.fb.ins("i32.const 0", "rawurldecode keeps a plus literal");
            ctx.fb
                .ins("call $__rt_str_url_decode", "percent-decode tolerantly");
        }
        UnaryStringRuntime::Base64Encode => {
            ctx.fb
                .ins("call $__rt_str_base64_encode", "encode with padding to a quartet");
        }
        UnaryStringRuntime::Base64Decode => {
            ctx.fb
                .ins("call $__rt_str_base64_decode", "decode skipping non-alphabet bytes");
        }
        other => {
            return Err(WasmError::Unsupported(format!(
                "unary string transform {:?}",
                other
            )))
        }
    }
    store_result(ctx, inst)
}

/// `__rt_str_region_eq`: compares `nlen` bytes of a needle against a haystack at `offset`.
///
/// The caller guarantees the region is in bounds, which every user below checks by comparing
/// lengths first. An empty needle matches anywhere, which is what makes `str_contains($h, "")`
/// true in PHP.
const RT_STR_REGION_EQ: &str = r#"(func $__rt_str_region_eq (param $hptr i32) (param $nptr i32) (param $nlen i64) (param $offset i64) (result i64)
  (local $i i64)
  (local.set $i (i64.const 0))                                    ;; i = 0
  (block $end (loop $cmp
    (br_if $end (i64.ge_s (local.get $i) (local.get $nlen)))      ;; every needle byte matched
    (if (i32.ne
          (i32.load8_u (i32.add (local.get $hptr)
                                (i32.wrap_i64 (i64.add (local.get $offset) (local.get $i)))))
          (i32.load8_u (i32.add (local.get $nptr) (i32.wrap_i64 (local.get $i)))))
      (then (return (i64.const 0))))                              ;; first mismatch decides
    (local.set $i (i64.add (local.get $i) (i64.const 1)))         ;; i++
    (br $cmp)))
  (i64.const 1))                                                  ;; the whole needle matched
"#;

/// `__rt_str_contains`: whether the needle occurs anywhere in the haystack.
///
/// Scans every start offset that leaves room for the needle. A needle longer than the haystack
/// leaves none, so the answer is false without reading a byte; an empty needle matches at offset
/// zero, which PHP reports as true even for an empty haystack.
const RT_STR_CONTAINS: &str = r#"(func $__rt_str_contains (param $hptr i32) (param $hlen i64) (param $nptr i32) (param $nlen i64) (result i64)
  (local $offset i64)
  (local $last i64)
  (local.set $last (i64.sub (local.get $hlen) (local.get $nlen)))  ;; last start offset with room
  (if (i64.lt_s (local.get $last) (i64.const 0))
    (then (return (i64.const 0))))                                 ;; the needle cannot fit
  (local.set $offset (i64.const 0))                                ;; start at the beginning
  (block $end (loop $scan
    (br_if $end (i64.gt_s (local.get $offset) (local.get $last)))  ;; no room left
    (if (i64.eq (call $__rt_str_region_eq (local.get $hptr) (local.get $nptr) (local.get $nlen) (local.get $offset)) (i64.const 1))
      (then (return (i64.const 1))))                               ;; occurrence found
    (local.set $offset (i64.add (local.get $offset) (i64.const 1)))  ;; try the next offset
    (br $scan)))
  (i64.const 0))                                                   ;; no occurrence
"#;

/// The storage one direct builtin accepts and produces.
///
/// Both the audit and the emitter derive from this single description, which is what keeps a
/// newly admitted builtin from being auditable but unlowerable, or the reverse.
struct DirectSignature {
    /// EIR type every operand must carry.
    operand_ir: IrType,
    /// PHP type every operand must carry, after `codegen_repr`.
    operand_php: PhpType,
    /// EIR type the result must carry.
    result_ir: IrType,
    /// PHP type the result must carry, after `codegen_repr`.
    result_php: PhpType,
}

/// Returns the signature and WebAssembly instruction for a builtin lowered inline, or `None`
/// when the builtin needs a runtime helper.
///
/// `count` is absent from the instruction column because it is a memory load rather than an
/// arithmetic operation; it is handled separately by [`lower_count`].
fn direct_builtin(target: RuntimeFnId, operand_php: &PhpType) -> Option<(DirectSignature, &'static str)> {
    let float = |instruction| {
        Some((
            DirectSignature {
                operand_ir: IrType::F64,
                operand_php: PhpType::Float,
                result_ir: IrType::F64,
                result_php: PhpType::Float,
            },
            instruction,
        ))
    };
    match target {
        // `abs` is the one entry whose storage depends on its argument: PHP keeps an integer
        // argument integral and a float one floating.
        RuntimeFnId::Abs => match operand_php {
            PhpType::Int => Some((
                DirectSignature {
                    operand_ir: IrType::I64,
                    operand_php: PhpType::Int,
                    result_ir: IrType::I64,
                    result_php: PhpType::Int,
                },
                // WebAssembly has no i64 absolute value; the branchless form is
                // `(x ^ (x >> 63)) - (x >> 63)`, emitted by `lower_int_abs`.
                "",
            )),
            PhpType::Float => float("f64.abs"),
            _ => None,
        },
        RuntimeFnId::Floor => float("f64.floor"),
        RuntimeFnId::Round => float("call $__rt_round"),
        RuntimeFnId::Ceil => float("f64.ceil"),
        RuntimeFnId::Sqrt => float("f64.sqrt"),
        _ => None,
    }
}

/// Returns whether `target` is lowered inline by this module.
pub(super) fn is_direct_builtin(target: RuntimeFnId) -> bool {
    matches!(
        target,
        RuntimeFnId::Abs
            | RuntimeFnId::Floor
            | RuntimeFnId::Round
            | RuntimeFnId::Ceil
            | RuntimeFnId::Sqrt
            | RuntimeFnId::Count
            | RuntimeFnId::ArrayIsList
            | RuntimeFnId::ArrayKeys
            | RuntimeFnId::ArrayValues
            | RuntimeFnId::InArray
            | RuntimeFnId::ArrayReverse
            | RuntimeFnId::ArraySum
            | RuntimeFnId::ArrayProduct
            | RuntimeFnId::Max
            | RuntimeFnId::Min
            | RuntimeFnId::Intdiv
            | RuntimeFnId::ArrayFill
            | RuntimeFnId::StrContains
            | RuntimeFnId::StrStartsWith
            | RuntimeFnId::StrEndsWith
            | RuntimeFnId::Chr
            | RuntimeFnId::Ord
            | RuntimeFnId::Ucfirst
            | RuntimeFnId::Lcfirst
            | RuntimeFnId::Ucwords
            | RuntimeFnId::Strcmp
            | RuntimeFnId::Strcasecmp
            | RuntimeFnId::Trim
            | RuntimeFnId::Ltrim
            | RuntimeFnId::Rtrim
            | RuntimeFnId::Substr
            | RuntimeFnId::StrRepeat
            | RuntimeFnId::Strpos
            | RuntimeFnId::Strrpos
            | RuntimeFnId::Implode
            | RuntimeFnId::ArraySlice
            | RuntimeFnId::ArrayMerge
            | RuntimeFnId::Range
            | RuntimeFnId::ArrayKeyExists
            | RuntimeFnId::Sort
            | RuntimeFnId::Rsort
            | RuntimeFnId::ArraySearch
            | RuntimeFnId::Explode
            | RuntimeFnId::StrSplit
            | RuntimeFnId::Wordwrap
            | RuntimeFnId::Sprintf
            | RuntimeFnId::Printf
            | RuntimeFnId::Strstr
            | RuntimeFnId::StrPad
            | RuntimeFnId::StrReplace
            | RuntimeFnId::Crc32
            | RuntimeFnId::Sha1
            | RuntimeFnId::Md5
            | RuntimeFnId::Htmlspecialchars
    )
}

/// Returns whether `value` is an indexed array whose slots this module can read directly.
///
/// Everything in the array family below reads raw i64 slots, which is what an `array<int>`
/// stores. A string or mixed element array uses a different slot width and carries refcounted
/// payloads, so it is not served here rather than being read at the wrong stride.
///
/// `array<never>` is admitted alongside `array<int>`: it is the type of the empty array literal,
/// and an array that provably holds nothing is read at no stride at all — every operation here
/// answers from its length, which is zero.
fn indexed_int_array(value: &crate::ir::Value) -> bool {
    value.ir_type == IrType::Heap(IrHeapKind::Array)
        && matches!(
            value.php_type.codegen_repr(),
            PhpType::Array(element) if matches!(*element, PhpType::Int | PhpType::Never)
        )
}

/// Validates one direct builtin's operand and result storage before planning.
pub(super) fn direct_builtin_shape_issue(
    module: &Module,
    function: &Function,
    call: &Instruction,
    target: RuntimeFnId,
) -> Option<String> {
    if target == RuntimeFnId::Count {
        return count_shape_issue(function, call);
    }
    if target == RuntimeFnId::ArrayIsList {
        return array_is_list_shape_issue(function, call);
    }
    if matches!(
        target,
        RuntimeFnId::ArrayKeys | RuntimeFnId::ArrayValues | RuntimeFnId::ArrayReverse
    ) {
        return indexed_array_result_shape_issue(function, call, target);
    }
    if matches!(target, RuntimeFnId::ArraySum | RuntimeFnId::ArrayProduct) {
        return array_fold_shape_issue(function, call, target);
    }
    if matches!(target, RuntimeFnId::Max | RuntimeFnId::Min | RuntimeFnId::Intdiv) {
        return int_pair_shape_issue(function, call, target);
    }
    if target == RuntimeFnId::ArrayFill {
        return array_fill_shape_issue(function, call);
    }
    if matches!(
        target,
        RuntimeFnId::StrContains | RuntimeFnId::StrStartsWith | RuntimeFnId::StrEndsWith
    ) {
        return string_predicate_shape_issue(function, call, target);
    }
    if target == RuntimeFnId::InArray {
        return in_array_shape_issue(function, call);
    }
    if matches!(target, RuntimeFnId::Chr | RuntimeFnId::Ord) {
        return byte_conversion_shape_issue(function, call, target);
    }
    if matches!(target, RuntimeFnId::Strcmp | RuntimeFnId::Strcasecmp) {
        return string_compare_shape_issue(function, call, target);
    }
    if matches!(
        target,
        RuntimeFnId::Trim | RuntimeFnId::Ltrim | RuntimeFnId::Rtrim
    ) {
        return trim_shape_issue(function, call, target);
    }
    if target == RuntimeFnId::Substr {
        return substr_shape_issue(function, call);
    }
    if target == RuntimeFnId::StrRepeat {
        return str_repeat_shape_issue(function, call);
    }
    if matches!(target, RuntimeFnId::Strpos | RuntimeFnId::Strrpos) {
        return string_search_shape_issue(function, call, target);
    }
    if target == RuntimeFnId::Implode {
        return implode_shape_issue(function, call);
    }
    if target == RuntimeFnId::ArraySlice {
        return array_slice_shape_issue(function, call);
    }
    if target == RuntimeFnId::ArrayMerge {
        return array_merge_shape_issue(function, call);
    }
    if target == RuntimeFnId::Range {
        return range_shape_issue(function, call);
    }
    if matches!(target, RuntimeFnId::Sort | RuntimeFnId::Rsort) {
        return scalar_sort_shape_issue(module, function, call);
    }
    if target == RuntimeFnId::ArrayKeyExists {
        return array_key_exists_shape_issue(function, call);
    }
    if target == RuntimeFnId::ArraySearch {
        return in_array_shape_issue(function, call);
    }
    if target == RuntimeFnId::Explode {
        return explode_shape_issue(function, call);
    }
    if target == RuntimeFnId::StrSplit {
        return str_split_shape_issue(function, call);
    }
    if target == RuntimeFnId::Wordwrap {
        return wordwrap_shape_issue(function, call);
    }
    if matches!(target, RuntimeFnId::Sprintf | RuntimeFnId::Printf) {
        return sprintf_shape_issue(function, module, call, target);
    }
    if target == RuntimeFnId::Strstr {
        return strstr_shape_issue(function, call);
    }
    if target == RuntimeFnId::StrPad {
        return str_pad_shape_issue(function, call);
    }
    if target == RuntimeFnId::StrReplace {
        return str_replace_shape_issue(function, call);
    }
    if target == RuntimeFnId::Crc32 {
        return crc32_shape_issue(function, call);
    }
    if matches!(
        target,
        RuntimeFnId::Sha1 | RuntimeFnId::Md5 | RuntimeFnId::Htmlspecialchars
    ) {
        return trim_shape_issue(function, call, target)
            .or_else(|| (call.operands.len() != 1).then(|| {
                format!("{target:?} takes exactly one string, got {}", call.operands.len())
            }));
    }
    if matches!(
        target,
        RuntimeFnId::Ucfirst | RuntimeFnId::Lcfirst | RuntimeFnId::Ucwords
    ) {
        return trim_shape_issue(function, call, target)
            .or_else(|| (call.operands.len() != 1).then(|| {
                format!("{target:?} takes exactly one string, got {}", call.operands.len())
            }));
    }
    let [operand] = call.operands.as_slice() else {
        return Some(format!(
            "expected one operand, got {}",
            call.operands.len()
        ));
    };
    let Some(value) = function.value(*operand) else {
        return Some("operand is missing from the value table".to_string());
    };
    let operand_php = value.php_type.codegen_repr();
    let Some((signature, _)) = direct_builtin(target, &operand_php) else {
        return Some(format!(
            "no inline lowering for a {operand_php:?} argument"
        ));
    };
    if value.ir_type != signature.operand_ir || operand_php != signature.operand_php {
        return Some(format!(
            "operand {:?}/{operand_php:?} is not the expected {:?}/{:?}",
            value.ir_type, signature.operand_ir, signature.operand_php
        ));
    }
    if call.result.is_none()
        || call.result_type != signature.result_ir
        || call.result_php_type.codegen_repr() != signature.result_php
    {
        return Some(format!(
            "result {:?}/{:?} is not the expected {:?}/{:?}",
            call.result_type,
            call.result_php_type.codegen_repr(),
            signature.result_ir,
            signature.result_php
        ));
    }
    None
}

/// Validates `count($array)` against the one shape its load can serve.
///
/// The length is read straight from the container header, so the operand has to be a container
/// this backend allocated. PHP's `count()` of a non-countable value is a `TypeError`, which a
/// header load cannot raise, so any other operand type is refused rather than answering nonsense.
fn count_shape_issue(function: &Function, call: &Instruction) -> Option<String> {
    let [operand] = call.operands.as_slice() else {
        return Some(format!(
            "expected one container operand, got {}",
            call.operands.len()
        ));
    };
    let Some(value) = function.value(*operand) else {
        return Some("container operand is missing from the value table".to_string());
    };
    if !matches!(
        value.ir_type,
        IrType::Heap(IrHeapKind::Array | IrHeapKind::Hash)
    ) || !matches!(
        value.php_type.codegen_repr(),
        PhpType::Array(_) | PhpType::AssocArray { .. }
    ) {
        return Some(format!(
            "expected a statically typed array or hash, got {:?}/{:?}",
            value.ir_type,
            value.php_type.codegen_repr()
        ));
    }
    if call.result.is_none()
        || call.result_type != IrType::I64
        || call.result_php_type.codegen_repr() != PhpType::Int
    {
        return Some(format!(
            "result {:?}/{:?} is not the expected I64/Int",
            call.result_type,
            call.result_php_type.codegen_repr()
        ));
    }
    None
}

/// Lowers one direct builtin.
pub(super) fn lower_direct_builtin(
    ctx: &mut FnCtx,
    inst: &Instruction,
    target: RuntimeFnId,
) -> Result<()> {
    if target == RuntimeFnId::Count {
        return lower_count(ctx, inst);
    }
    if target == RuntimeFnId::ArrayIsList {
        return lower_array_is_list(ctx, inst);
    }
    if target == RuntimeFnId::ArrayKeys {
        return lower_array_keys(ctx, inst);
    }
    if target == RuntimeFnId::ArrayValues {
        return lower_array_values(ctx, inst);
    }
    if target == RuntimeFnId::ArrayReverse {
        return lower_array_reverse(ctx, inst);
    }
    if matches!(target, RuntimeFnId::ArraySum | RuntimeFnId::ArrayProduct) {
        return lower_array_fold(ctx, inst, target);
    }
    if matches!(target, RuntimeFnId::Max | RuntimeFnId::Min) {
        return lower_int_extremum(ctx, inst, target);
    }
    if target == RuntimeFnId::Intdiv {
        return super::inst::lower_signed_int_div(ctx, inst);
    }
    if target == RuntimeFnId::ArrayFill {
        return lower_array_fill(ctx, inst);
    }
    if matches!(
        target,
        RuntimeFnId::StrContains | RuntimeFnId::StrStartsWith | RuntimeFnId::StrEndsWith
    ) {
        return lower_string_predicate(ctx, inst, target);
    }
    if target == RuntimeFnId::InArray {
        return lower_in_array(ctx, inst);
    }
    if target == RuntimeFnId::Chr {
        ctx.emit_load_value(operand(inst, 0)?)?;
        ctx.fb
            .ins("call $__rt_str_chr", "the byte PHP's chr returns for this integer");
        return store_result(ctx, inst);
    }
    if target == RuntimeFnId::Ord {
        ctx.emit_load_value(operand(inst, 0)?)?;
        ctx.fb
            .ins("call $__rt_str_ord", "the first byte, or zero for the empty string");
        return store_result(ctx, inst);
    }
    if matches!(target, RuntimeFnId::Ucfirst | RuntimeFnId::Lcfirst) {
        ctx.emit_load_value(operand(inst, 0)?)?;
        ctx.fb.ins(
            if target == RuntimeFnId::Ucfirst {
                "i32.const 1"
            } else {
                "i32.const 0"
            },
            "map the first byte towards upper or lower case",
        );
        ctx.fb
            .ins("call $__rt_str_case_edge", "case-map the first byte only");
        return store_result(ctx, inst);
    }
    if target == RuntimeFnId::Ucwords {
        ctx.emit_load_value(operand(inst, 0)?)?;
        ctx.fb
            .ins("call $__rt_str_ucwords", "upper-case each word's first letter");
        return store_result(ctx, inst);
    }
    if matches!(target, RuntimeFnId::Strcmp | RuntimeFnId::Strcasecmp) {
        ctx.emit_load_value(operand(inst, 0)?)?;
        ctx.emit_load_value(operand(inst, 1)?)?;
        ctx.fb.ins(
            if target == RuntimeFnId::Strcasecmp {
                "i32.const 1"
            } else {
                "i32.const 0"
            },
            "fold ASCII case before comparing",
        );
        ctx.fb
            .ins("call $__rt_str_cmp", "byte distance, or +/-1 on length alone");
        return store_result(ctx, inst);
    }
    if matches!(
        target,
        RuntimeFnId::Trim | RuntimeFnId::Ltrim | RuntimeFnId::Rtrim
    ) {
        return lower_trim(ctx, inst, target);
    }
    if target == RuntimeFnId::Substr {
        return lower_substr(ctx, inst);
    }
    if target == RuntimeFnId::StrRepeat {
        return lower_str_repeat(ctx, inst);
    }
    if target == RuntimeFnId::Strpos {
        return lower_string_search(ctx, inst);
    }
    if target == RuntimeFnId::Strrpos {
        return lower_string_rsearch(ctx, inst);
    }
    if target == RuntimeFnId::Implode {
        return lower_implode(ctx, inst);
    }
    if target == RuntimeFnId::ArraySlice {
        return lower_array_slice(ctx, inst);
    }
    if matches!(target, RuntimeFnId::Sort | RuntimeFnId::Rsort) {
        return lower_scalar_sort(ctx, inst, target == RuntimeFnId::Rsort);
    }
    if target == RuntimeFnId::ArrayKeyExists {
        return super::inst_hash::lower_array_key_exists(ctx, inst);
    }
    if target == RuntimeFnId::ArraySearch {
        return lower_array_search(ctx, inst);
    }
    if target == RuntimeFnId::Range {
        ctx.emit_load_value(operand(inst, 0)?)?;
        ctx.emit_load_value(operand(inst, 1)?)?;
        ctx.fb.ins(
            "call $__rt_range_int",
            "count from start to end inclusive, in whichever direction",
        );
        return store_result(ctx, inst);
    }
    if target == RuntimeFnId::ArrayMerge {
        ctx.emit_load_value(operand(inst, 0)?)?;
        ctx.emit_load_value(operand(inst, 1)?)?;
        ctx.fb.ins(
            "call $__rt_array_merge",
            "clone the left, then append every element of the right",
        );
        return store_result(ctx, inst);
    }
    if target == RuntimeFnId::Explode {
        return lower_explode(ctx, inst);
    }
    if target == RuntimeFnId::StrSplit {
        return lower_str_split(ctx, inst);
    }
    if target == RuntimeFnId::Wordwrap {
        return lower_wordwrap(ctx, inst);
    }
    if target == RuntimeFnId::Sprintf {
        return lower_sprintf(ctx, inst);
    }
    if target == RuntimeFnId::Printf {
        return lower_printf(ctx, inst);
    }
    if target == RuntimeFnId::Strstr {
        return lower_strstr(ctx, inst);
    }
    if target == RuntimeFnId::StrPad {
        return lower_str_pad(ctx, inst);
    }
    if target == RuntimeFnId::StrReplace {
        return lower_str_replace(ctx, inst);
    }
    if target == RuntimeFnId::Crc32 {
        ctx.emit_load_value(operand(inst, 0)?)?;
        ctx.fb.ins("call $__rt_crc32", "reflected IEEE 802.3 remainder");
        return store_result(ctx, inst);
    }
    if target == RuntimeFnId::Sha1 {
        ctx.emit_load_value(operand(inst, 0)?)?;
        ctx.fb.ins("call $__rt_sha1_hex", "40-character lowercase digest");
        return store_result(ctx, inst);
    }
    if target == RuntimeFnId::Md5 {
        ctx.emit_load_value(operand(inst, 0)?)?;
        ctx.fb.ins("call $__rt_md5_hex", "32-character lowercase digest");
        return store_result(ctx, inst);
    }
    if target == RuntimeFnId::Htmlspecialchars {
        ctx.emit_load_value(operand(inst, 0)?)?;
        ctx.fb.ins("call $__rt_htmlspecialchars", "escape under PHP 8.1+ default flags");
        return store_result(ctx, inst);
    }
    let argument = operand(inst, 0)?;
    let operand_php = ctx.value_php_type(argument)?.codegen_repr();
    let Some((_, instruction)) = direct_builtin(target, &operand_php) else {
        return Err(WasmError::Unsupported(format!(
            "builtin {:?} over a {operand_php:?} argument",
            target
        )));
    };
    if target == RuntimeFnId::Abs && operand_php == PhpType::Int {
        return lower_int_abs(ctx, inst, argument);
    }
    ctx.emit_load_value(argument)?;
    ctx.fb.ins(instruction, "PHP builtin lowered inline");
    store_result(ctx, inst)
}

/// Lowers `abs($int)` branchlessly as `(x ^ (x >> 63)) - (x >> 63)`.
///
/// KNOWN DIVERGENCE, shared with the native backend and rooted in EIR rather than in either
/// emitter: PHP promotes `abs(PHP_INT_MIN)` to the float `9.2233720368548E+18`, because its
/// magnitude has no integer representation. EIR types this call `I64`/`int`, so there is no slot
/// a float could be returned in, and both backends therefore answer `PHP_INT_MIN` unchanged.
/// Every other input is exact.
fn lower_int_abs(ctx: &mut FnCtx, inst: &Instruction, argument: crate::ir::ValueId) -> Result<()> {
    let mask = ctx.fresh_temp(super::wat::ValType::I64);
    ctx.emit_load_value(argument)?;
    ctx.fb.ins("i64.const 63", "sign-bit shift distance");
    ctx.fb
        .ins("i64.shr_s", "all ones for a negative argument, zero otherwise");
    ctx.fb.ins(&format!("local.tee {}", mask), "keep the sign mask");
    ctx.emit_load_value(argument)?;
    ctx.fb.ins("i64.xor", "conditionally invert the argument");
    ctx.fb.ins(&format!("local.get {}", mask), "the sign mask again");
    ctx.fb.ins("i64.sub", "add one back for a negative argument");
    store_result(ctx, inst)
}

/// Validates `array_is_list($array)` against the one operand whose answer is known statically.
///
/// This backend's `Heap(Array)` IS the contiguous representation: its keys are `0..n-1` in order
/// by construction, which is exactly PHP's definition of a list, and an empty array qualifies.
/// A `Heap(Hash)` carries arbitrary keys and would need a real scan, so it is refused rather
/// than answered from the representation.
fn array_is_list_shape_issue(function: &Function, call: &Instruction) -> Option<String> {
    let [operand] = call.operands.as_slice() else {
        return Some(format!(
            "expected one array operand, got {}",
            call.operands.len()
        ));
    };
    let Some(value) = function.value(*operand) else {
        return Some("array operand is missing from the value table".to_string());
    };
    if value.ir_type != IrType::Heap(IrHeapKind::Array)
        || !matches!(value.php_type.codegen_repr(), PhpType::Array(_))
    {
        return Some(format!(
            "expected a statically typed indexed array, got {:?}/{:?}",
            value.ir_type,
            value.php_type.codegen_repr()
        ));
    }
    if call.result.is_none()
        || call.result_type != IrType::I64
        || call.result_php_type.codegen_repr() != PhpType::Bool
    {
        return Some(format!(
            "result {:?}/{:?} is not the expected I64/Bool",
            call.result_type,
            call.result_php_type.codegen_repr()
        ));
    }
    None
}

/// Lowers `array_is_list($indexed)` to the constant true its representation guarantees.
///
/// The operand is still evaluated and dropped: it may be a call whose side effects PHP performs
/// before answering, and discarding the expression rather than the value would skip them.
fn lower_array_is_list(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    ctx.emit_load_value(operand(inst, 0)?)?;
    ctx.fb
        .ins("drop", "the answer follows from the representation, not the contents");
    ctx.fb
        .ins("i64.const 1", "an indexed array is a list by construction");
    store_result(ctx, inst)
}

/// Validates `array_keys($list)` and `array_values($list)`, which both answer a fresh
/// `array<int>` built from an `array<int>`.
///
/// `array_keys` of a list is `[0, 1, ..., n-1]` because its keys ARE its positions, and
/// `array_values` of a list is a copy because re-indexing a list changes nothing. Both facts
/// hold only for the indexed representation, so a hash operand is refused.
fn indexed_array_result_shape_issue(
    function: &Function,
    call: &Instruction,
    target: RuntimeFnId,
) -> Option<String> {
    let [operand] = call.operands.as_slice() else {
        return Some(format!(
            "expected one array operand, got {}",
            call.operands.len()
        ));
    };
    let Some(value) = function.value(*operand) else {
        return Some("array operand is missing from the value table".to_string());
    };
    if !indexed_int_array(value) {
        return Some(format!(
            "expected a statically typed array<int>, got {:?}/{:?}",
            value.ir_type,
            value.php_type.codegen_repr()
        ));
    }
    if call.result.is_none()
        || call.result_type != IrType::Heap(IrHeapKind::Array)
        || !matches!(
            call.result_php_type.codegen_repr(),
            PhpType::Array(element) if matches!(*element, PhpType::Int | PhpType::Never)
        )
    {
        return Some(format!(
            "{target:?} result {:?}/{:?} is not the expected Heap(Array)/array<int>",
            call.result_type,
            call.result_php_type.codegen_repr()
        ));
    }
    None
}

/// Lowers `array_keys($list)` to the positional key array its representation implies.
fn lower_array_keys(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    ctx.emit_load_value(operand(inst, 0)?)?;
    ctx.fb
        .ins("call $__rt_array_index_keys", "keys of a list are its positions");
    store_result(ctx, inst)
}

/// Lowers `array_values($list)` to a shallow clone.
///
/// Re-indexing a list changes nothing, so the values are the source's in order; the clone is
/// what makes the result an independent owned array rather than an alias of the source.
fn lower_array_values(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    ctx.emit_load_value(operand(inst, 0)?)?;
    ctx.fb
        .ins("call $__rt_array_clone_shallow", "values of a list are the list itself");
    store_result(ctx, inst)
}

/// Validates `array_sum($list)` / `array_product($list)`, which fold an `array<int>` to an int.
fn array_fold_shape_issue(
    function: &Function,
    call: &Instruction,
    target: RuntimeFnId,
) -> Option<String> {
    let [operand] = call.operands.as_slice() else {
        return Some(format!(
            "expected one array operand, got {}",
            call.operands.len()
        ));
    };
    let Some(value) = function.value(*operand) else {
        return Some("array operand is missing from the value table".to_string());
    };
    if !indexed_int_array(value) {
        return Some(format!(
            "expected a statically typed array<int>, got {:?}/{:?}",
            value.ir_type,
            value.php_type.codegen_repr()
        ));
    }
    if call.result.is_none()
        || call.result_type != IrType::I64
        || call.result_php_type.codegen_repr() != PhpType::Int
    {
        return Some(format!(
            "{target:?} result {:?}/{:?} is not the expected I64/Int",
            call.result_type,
            call.result_php_type.codegen_repr()
        ));
    }
    None
}

/// Lowers `array_reverse($list)` to a reversed copy.
fn lower_array_reverse(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    ctx.emit_load_value(operand(inst, 0)?)?;
    ctx.fb.ins(
        "call $__rt_array_reverse_int",
        "reversing a list re-indexes it from zero",
    );
    store_result(ctx, inst)
}

/// Lowers `array_sum($list)` / `array_product($list)` to their accumulating scan.
///
/// KNOWN DIVERGENCE, shared with the native backend and rooted in the checker rather than in
/// either emitter: PHP promotes an overflowing sum or product to a float, so
/// `array_sum([PHP_INT_MAX, 1])` is `9.2233720368548E+18`. The checker types this call `int`,
/// leaving no slot a float could be returned in, and both backends therefore wrap. Closing it
/// means widening the declared result to `int|float`, which is an EIR-level change.
fn lower_array_fold(ctx: &mut FnCtx, inst: &Instruction, target: RuntimeFnId) -> Result<()> {
    ctx.emit_load_value(operand(inst, 0)?)?;
    let (helper, comment) = if target == RuntimeFnId::ArraySum {
        ("call $__rt_array_sum_int", "PHP sums an empty array to 0")
    } else {
        ("call $__rt_array_product_int", "PHP's empty product is 1")
    };
    ctx.fb.ins(helper, comment);
    store_result(ctx, inst)
}

/// Validates the builtins taking two ints and answering one: `max`, `min` and `intdiv`.
///
/// PHP's `max`/`min` are variadic and compare across types; only the two-integer form is served
/// here, where the comparison is a plain signed ordering with no juggling.
fn int_pair_shape_issue(
    function: &Function,
    call: &Instruction,
    target: RuntimeFnId,
) -> Option<String> {
    let [left, right] = call.operands.as_slice() else {
        return Some(format!(
            "expected two int operands, got {}",
            call.operands.len()
        ));
    };
    for operand in [left, right] {
        let Some(value) = function.value(*operand) else {
            return Some("operand is missing from the value table".to_string());
        };
        if value.ir_type != IrType::I64 || value.php_type.codegen_repr() != PhpType::Int {
            return Some(format!(
                "expected an int operand, got {:?}/{:?}",
                value.ir_type,
                value.php_type.codegen_repr()
            ));
        }
    }
    if call.result.is_none()
        || call.result_type != IrType::I64
        || call.result_php_type.codegen_repr() != PhpType::Int
    {
        return Some(format!(
            "{target:?} result {:?}/{:?} is not the expected I64/Int",
            call.result_type,
            call.result_php_type.codegen_repr()
        ));
    }
    None
}

/// Validates `array_fill(0, $count, $value)` against the one shape a list can represent.
///
/// A non-zero start index produces the keys `start..start+count-1`, which is not a list, so the
/// start must be the literal `0`. The value must be an int because the result's slots are raw
/// i64s.
fn array_fill_shape_issue(function: &Function, call: &Instruction) -> Option<String> {
    let [start, count, value] = call.operands.as_slice() else {
        return Some(format!(
            "expected start, count and value, got {} operands",
            call.operands.len()
        ));
    };
    if !literal_zero(function, *start) {
        return Some(
            "only a literal 0 start index yields a list; other starts key from the start index"
                .to_string(),
        );
    }
    for operand in [count, value] {
        let Some(operand) = function.value(*operand) else {
            return Some("operand is missing from the value table".to_string());
        };
        if operand.ir_type != IrType::I64 || operand.php_type.codegen_repr() != PhpType::Int {
            return Some(format!(
                "expected an int operand, got {:?}/{:?}",
                operand.ir_type,
                operand.php_type.codegen_repr()
            ));
        }
    }
    if call.result.is_none()
        || call.result_type != IrType::Heap(IrHeapKind::Array)
        || !matches!(
            call.result_php_type.codegen_repr(),
            PhpType::Array(element) if matches!(*element, PhpType::Int | PhpType::Never)
        )
    {
        return Some(format!(
            "result {:?}/{:?} is not the expected Heap(Array)/array<int>",
            call.result_type,
            call.result_php_type.codegen_repr()
        ));
    }
    None
}

/// Returns whether `value` is the constant integer zero.
fn literal_zero(function: &Function, value: crate::ir::ValueId) -> bool {
    let Some(defined) = function.value(value) else {
        return false;
    };
    let crate::ir::ValueDef::Instruction { inst, .. } = defined.def else {
        return false;
    };
    let Some(defining) = function.instruction(inst) else {
        return false;
    };
    defining.op == crate::ir::Op::ConstI64
        && matches!(defining.immediate, Some(crate::ir::Immediate::I64(0)))
}

/// Lowers the two-integer `max` / `min` to a signed comparison and a select.
fn lower_int_extremum(ctx: &mut FnCtx, inst: &Instruction, target: RuntimeFnId) -> Result<()> {
    let left = operand(inst, 0)?;
    let right = operand(inst, 1)?;
    ctx.emit_load_value(left)?;
    ctx.emit_load_value(right)?;
    ctx.emit_load_value(left)?;
    ctx.emit_load_value(right)?;
    if target == RuntimeFnId::Max {
        ctx.fb.ins("i64.gt_s", "is the left operand the larger?");
    } else {
        ctx.fb.ins("i64.lt_s", "is the left operand the smaller?");
    }
    ctx.fb
        .ins("select", "keep the operand the comparison chose");
    store_result(ctx, inst)
}

/// Lowers `array_fill(0, $count, $value)` to a filled list.
fn lower_array_fill(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    ctx.emit_load_value(operand(inst, 1)?)?;
    ctx.emit_load_value(operand(inst, 2)?)?;
    ctx.fb
        .ins("call $__rt_array_fill_int", "one repeated value per slot");
    store_result(ctx, inst)
}

/// Validates `str_contains`, `str_starts_with` and `str_ends_with`: two strings in, a bool out.
/// Lowers `trim`, `ltrim` and `rtrim` through the shared end-stripping helper.
///
/// The one-argument form passes a `-1` charlist length, which the helper reads as PHP's default
/// set. That keeps the default out of the data segments of a module that never asks for it, and
/// keeps an explicitly EMPTY charlist — which strips nothing — distinguishable from it.
fn lower_trim(ctx: &mut FnCtx, inst: &Instruction, target: RuntimeFnId) -> Result<()> {
    ctx.emit_load_value(operand(inst, 0)?)?;
    if inst.operands.len() == 2 {
        ctx.emit_load_value(operand(inst, 1)?)?;
    } else {
        ctx.fb.ins("i32.const 0", "no charlist pointer");
        ctx.fb
            .ins("i64.const -1", "sentinel: PHP's default character set");
    }
    let mode = match target {
        RuntimeFnId::Ltrim => 1,
        RuntimeFnId::Rtrim => 2,
        _ => 3,
    };
    ctx.fb
        .ins(&format!("i32.const {mode}"), "which ends to strip");
    ctx.fb.ins("call $__rt_str_trim", "strip the selected ends");
    store_result(ctx, inst)
}

/// Lowers `substr`, passing whether an explicit length was written.
///
/// The two- and three-argument forms differ in more than a default: without a length the slice
/// runs to the end, while a NEGATIVE length names an end offset from the right. A single flag
/// tells the helper which rule to apply rather than inventing a length that would have to encode
/// both.
fn lower_substr(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    ctx.emit_load_value(operand(inst, 0)?)?;
    ctx.emit_load_value(operand(inst, 1)?)?;
    if inst.operands.len() == 3 {
        ctx.emit_load_value(operand(inst, 2)?)?;
        ctx.fb.ins("i32.const 1", "an explicit length was written");
    } else {
        ctx.fb.ins("i64.const 0", "unused length");
        ctx.fb.ins("i32.const 0", "no length: run to the end");
    }
    ctx.fb.ins("call $__rt_str_substr", "own the selected bytes");
    store_result(ctx, inst)
}

/// Lowers `wordwrap` in its one- and two-argument forms.
///
/// The default break is a single newline and the default is NOT to cut long words, which together
/// select php-src's fast in-place path. A custom `$break` or `$cut_long_words` selects a different
/// algorithm in php-src — the general path can change the LENGTH, which this one never does — so
/// those arities are refused rather than approximated.
fn lower_wordwrap(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    ctx.emit_load_value(operand(inst, 0)?)?;
    if inst.operands.len() >= 2 {
        ctx.emit_load_value(operand(inst, 1)?)?;
    } else {
        ctx.fb.ins("i64.const 75", "PHP's default width");
    }
    if inst.operands.len() <= 2 {
        // The one-byte, no-cut form rewrites in place: a space BECOMES the break, so the length
        // never changes and no buffer has to be built.
        ctx.fb.ins("call $__rt_wordwrap", "break lines at spaces, in place");
        return store_result(ctx, inst);
    }
    ctx.emit_load_value(operand(inst, 2)?)?;
    if inst.operands.len() == 4 {
        ctx.emit_load_value(operand(inst, 3)?)?;
        ctx.fb.ins("i32.wrap_i64", "the cut flag is a PHP bool");
    } else {
        ctx.fb.ins("i32.const 0", "no cut: a long word is left whole");
    }
    ctx.fb.ins(
        "call $__rt_wordwrap_general",
        "an arbitrary break, and cutting, both lengthen the text",
    );
    store_result(ctx, inst)
}

/// Validates `wordwrap`: a subject and an optional integer width, a string out.
fn wordwrap_shape_issue(function: &Function, call: &Instruction) -> Option<String> {
    if !matches!(call.operands.len(), 1 | 2 | 3 | 4) {
        return Some(format!(
            "expected a subject, an optional width, break and cut flag, got {} operands",
            call.operands.len()
        ));
    }
    for (index, operand) in call.operands.iter().enumerate() {
        let Some(value) = function.value(*operand) else {
            return Some("operand is missing from the value table".to_string());
        };
        let (want_ir, want_php) = if index == 0 || index == 2 {
            (IrType::Str, PhpType::Str)
        } else if index == 3 {
            (IrType::I64, PhpType::Bool)
        } else {
            (IrType::I64, PhpType::Int)
        };
        if value.ir_type != want_ir || value.php_type.codegen_repr() != want_php {
            return Some(format!(
                "wordwrap operand {index} is {:?}/{:?}, expected {want_ir:?}/{want_php:?}",
                value.ir_type,
                value.php_type.codegen_repr()
            ));
        }
    }
    if call.result.is_none()
        || call.result_type != IrType::Str
        || call.result_php_type.codegen_repr() != PhpType::Str
    {
        return Some(format!(
            "wordwrap result {:?}/{:?} is not the expected Str/Str",
            call.result_type,
            call.result_php_type.codegen_repr()
        ));
    }
    None
}

/// The `__rt_fail` code PHP's non-positive-length `str_split` `ValueError` reports under.
const STR_SPLIT_LENGTH_FAILURE_CODE: i32 = 14;

/// Lowers `str_split` in both arities.
///
/// The one-argument form's default really is a chunk of one rather than a different rule, so both
/// arities share the same lowering with a literal 1 supplied. A chunk length below one raises
/// php-src's ValueError.
fn lower_str_split(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let chunk = ctx.fresh_temp(super::wat::ValType::I64);
    if inst.operands.len() == 2 {
        ctx.emit_load_value(operand(inst, 1)?)?;
    } else {
        ctx.fb.ins("i64.const 1", "the default chunk is one byte");
    }
    ctx.fb.ins(&format!("local.set {chunk}"), "spill the chunk length");
    ctx.fb.ins(&format!("local.get {chunk}"), "chunk length");
    ctx.fb.ins("i64.const 1", "PHP's lower bound");
    ctx.fb.ins("i64.lt_s", "below one?");
    ctx.fb.ins("if", "str_split() rejects a non-positive chunk length");
    super::inst::emit_runtime_failure(
        ctx,
        STR_SPLIT_LENGTH_FAILURE_CODE,
        "str_split() non-positive length",
    );
    ctx.fb.ins("end", "end chunk-length guard");
    ctx.emit_load_value(operand(inst, 0)?)?;
    ctx.fb.ins(&format!("local.get {chunk}"), "the validated chunk length");
    ctx.fb.ins("call $__rt_str_split", "cut into a fresh indexed array");
    store_result(ctx, inst)
}

/// Validates `str_split`: a subject and an optional chunk length, an indexed string array out.
fn str_split_shape_issue(function: &Function, call: &Instruction) -> Option<String> {
    if !matches!(call.operands.len(), 1 | 2) {
        return Some(format!(
            "expected a subject and an optional length, got {} operands",
            call.operands.len()
        ));
    }
    for (index, operand) in call.operands.iter().enumerate() {
        let Some(value) = function.value(*operand) else {
            return Some("operand is missing from the value table".to_string());
        };
        let (want_ir, want_php) = if index == 0 {
            (IrType::Str, PhpType::Str)
        } else {
            (IrType::I64, PhpType::Int)
        };
        if value.ir_type != want_ir || value.php_type.codegen_repr() != want_php {
            return Some(format!(
                "str_split operand {index} is {:?}/{:?}, expected {want_ir:?}/{want_php:?}",
                value.ir_type,
                value.php_type.codegen_repr()
            ));
        }
    }
    if call.result.is_none() || call.result_type != IrType::Heap(IrHeapKind::Array) {
        return Some(format!(
            "str_split result {:?} is not an indexed array",
            call.result_type
        ));
    }
    if !matches!(&call.result_php_type, PhpType::Array(element) if **element == PhpType::Str) {
        return Some(format!(
            "str_split must produce exactly array<string>, got {:?}",
            call.result_php_type
        ));
    }
    None
}

/// The `__rt_fail` code PHP's empty-separator `explode` `ValueError` reports under.
const EXPLODE_EMPTY_SEP_FAILURE_CODE: i32 = 13;

/// Lowers `explode` in its two-argument form.
///
/// PHP refuses an empty separator outright — unlike `str_pad`'s empty pad, which only raises when
/// it would be used — because there is no split it could mean and the scan would not advance. The
/// `$limit` form is refused: a positive limit caps the count with the remainder in the last
/// element, a negative one drops elements from the END, and zero behaves as one, which is three
/// different rules rather than a default.
fn lower_explode(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let pptr = ctx.fresh_temp(super::wat::ValType::I32);
    let plen = ctx.fresh_temp(super::wat::ValType::I64);
    ctx.emit_load_value(operand(inst, 0)?)?;
    ctx.fb.ins(&format!("local.set {plen}"), "spill separator length");
    ctx.fb.ins(&format!("local.set {pptr}"), "spill separator pointer");
    ctx.fb.ins(&format!("local.get {plen}"), "separator length");
    ctx.fb.ins("i64.eqz", "an empty separator?");
    ctx.fb.ins("if", "explode() has no split an empty separator could mean");
    super::inst::emit_runtime_failure(
        ctx,
        EXPLODE_EMPTY_SEP_FAILURE_CODE,
        "explode() empty separator",
    );
    ctx.fb.ins("end", "end empty-separator guard");
    ctx.emit_load_value(operand(inst, 1)?)?;
    ctx.fb.ins(&format!("local.get {pptr}"), "separator pointer");
    ctx.fb.ins(&format!("local.get {plen}"), "separator length");
    ctx.fb.ins("call $__rt_explode", "split into a fresh indexed array");
    store_result(ctx, inst)
}

/// Validates `explode`: a separator and a subject string, an indexed string array out.
fn explode_shape_issue(function: &Function, call: &Instruction) -> Option<String> {
    let [separator, subject] = call.operands.as_slice() else {
        return Some(format!(
            "expected a separator and a subject with no limit, got {} operands",
            call.operands.len()
        ));
    };
    for operand in [separator, subject] {
        let Some(value) = function.value(*operand) else {
            return Some("operand is missing from the value table".to_string());
        };
        if value.ir_type != IrType::Str || value.php_type.codegen_repr() != PhpType::Str {
            return Some(format!(
                "explode takes strings, got {:?}/{:?}",
                value.ir_type,
                value.php_type.codegen_repr()
            ));
        }
    }
    if call.result.is_none() || call.result_type != IrType::Heap(IrHeapKind::Array) {
        return Some(format!(
            "explode result {:?} is not an indexed array",
            call.result_type
        ));
    }
    if !matches!(&call.result_php_type, PhpType::Array(element) if **element == PhpType::Str) {
        return Some(format!(
            "explode must produce exactly array<string>, got {:?}",
            call.result_php_type
        ));
    }
    None
}

/// Lowers `implode` over an indexed array of strings.
///
/// PHP also accepts an array of ints or mixed values, joining their string coercions, and a
/// one-argument form whose glue is empty. Only the two-argument all-string shape is admitted:
/// coercing elements would need the per-tag conversions this backend keeps fail-closed, and
/// admitting the shorter form would mean inventing an empty glue with no literal behind it.
fn lower_implode(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let array = operand(inst, 1)?;
    // A string element needs no conversion; an int, a float or a Mixed cell does, and the
    // converted piece is owned so the shared helper releases it.
    let element_kind = match ctx.function.value(array).map(|v| &v.php_type) {
        Some(PhpType::Array(element)) => match **element {
            PhpType::Int => Some(0),
            PhpType::Mixed => Some(1),
            PhpType::Float => Some(2),
            _ => None,
        },
        _ => None,
    };
    ctx.emit_load_value(array)?;
    ctx.emit_load_value(operand(inst, 0)?)?;
    match element_kind {
        Some(kind) => {
            ctx.fb.ins(
                &format!("i32.const {kind}"),
                "0 renders an int slot, 1 casts a Mixed cell, 2 casts a float slot",
            );
            ctx.fb.ins(
                "call $__rt_implode_owned",
                "each element converts as an explicit (string) cast would",
            );
        }
        None => {
            ctx.fb.ins("call $__rt_implode", "join the elements with the glue");
        }
    }
    store_result(ctx, inst)
}

/// Lowers `array_slice` over a list, answering a fresh `array<mixed>`.
///
/// The offset/length rules are `substr`'s exactly (verified on 52 pairs against php-src), and
/// live in `__rt_array_slice` so the clamping that keeps `PHP_INT_MIN` from wrapping an i64 sits
/// next to the arithmetic it protects. The element shape decides whether a source slot is boxed
/// or, for a Mixed-cell source, shared with an incref.
fn lower_array_slice(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let array = operand(inst, 0)?;
    let element = match ctx.function.value(array).map(|v| v.php_type.codegen_repr()) {
        Some(PhpType::Array(element)) => *element,
        other => {
            return Err(WasmError::Unsupported(format!(
                "array_slice takes an indexed array, got {other:?}"
            )))
        }
    };
    let (tag, elem_size) = array_slice_element_shape(&element).ok_or_else(|| {
        WasmError::Unsupported(format!("array_slice has no element copy for {element:?}"))
    })?;
    ctx.emit_load_value(array)?;
    ctx.emit_load_value(operand(inst, 1)?)?;
    match inst.operands.len() {
        3 => {
            ctx.emit_load_value(operand(inst, 2)?)?;
            ctx.fb.ins("i32.const 1", "a length was given");
        }
        _ => {
            ctx.fb.ins("i64.const 0", "unused length");
            ctx.fb.ins("i32.const 0", "no length: slice runs to the end");
        }
    }
    ctx.fb
        .ins(&format!("i64.const {tag}"), "element cell tag (negative: share the cell)");
    ctx.fb
        .ins(&format!("i64.const {elem_size}"), "source slot stride");
    ctx.fb.ins(
        "call $__rt_array_slice",
        "copy the window into a fresh mixed-cell array",
    );
    store_result(ctx, inst)
}

/// How `in_array` scans, for one (needle, element) pair.
///
/// `blocks_strict` marks a pair whose types DIFFER: `===` compares types first, so a strict
/// request there can never match and the scan is skipped. `widen_elements` marks the one mix that
/// converts the elements rather than the needle.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) struct InArrayScan {
    kind: InArrayKind,
    blocks_strict: bool,
    widen_elements: bool,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum InArrayKind {
    /// 8-byte slots compared as integers; loose and strict agree.
    Int,
    /// 8-byte slots compared as doubles.
    Float,
    /// 16-byte (pointer, length) slots; loose uses the numeric-string rule.
    Str,
}

/// Classifies an `in_array` call, or `None` when the pair has no measured rule here.
///
/// PHP's table is wider than this: a string against a number, a bool against a number, and
/// anything boxed each need their own measured behaviour. An EMPTY haystack (`Void`, what an empty
/// literal's `Never` normalizes to) always answers false, but the scan still has to type-check —
/// the helper takes the needle BY VALUE — so its kind follows the needle.
fn in_array_scan(needle: &PhpType, element: &PhpType) -> Option<InArrayScan> {
    let scan = |kind, blocks_strict, widen_elements| {
        Some(InArrayScan { kind, blocks_strict, widen_elements })
    };
    match (needle.codegen_repr(), element.codegen_repr()) {
        // An empty haystack never matches, but the scan still has to TYPE-CHECK: the helper takes
        // the needle by value, so the kind follows the needle rather than the (absent) elements.
        (PhpType::Int | PhpType::Bool, PhpType::Void) => scan(InArrayKind::Int, false, false),
        (PhpType::Float, PhpType::Void) => scan(InArrayKind::Float, false, false),
        (PhpType::Str, PhpType::Void) => scan(InArrayKind::Str, false, false),
        (PhpType::Int, PhpType::Int) => scan(InArrayKind::Int, false, false),
        (PhpType::Bool, PhpType::Bool) => scan(InArrayKind::Int, false, false),
        (PhpType::Float, PhpType::Float) => scan(InArrayKind::Float, false, false),
        (PhpType::Str, PhpType::Str) => scan(InArrayKind::Str, false, false),
        // Mixed numbers: PHP widens and compares as doubles, and `===` can never match.
        (PhpType::Int, PhpType::Float) => scan(InArrayKind::Float, true, false),
        (PhpType::Float, PhpType::Int) => scan(InArrayKind::Float, true, true),
        _ => None,
    }
}

/// Validates `in_array`/`array_search`: a needle, an indexed haystack, and an optional strict
/// flag. `array_search` never carries one — the front-end admits exactly two operands — so the
/// same rule covers both.
fn in_array_shape_issue(function: &Function, call: &Instruction) -> Option<String> {
    let (needle, haystack, strict) = match call.operands.as_slice() {
        [needle, haystack] => (needle, haystack, None),
        [needle, haystack, strict] => (needle, haystack, Some(strict)),
        other => {
            return Some(format!(
                "the search takes a needle, an array and an optional strict flag, got {} operands",
                other.len()
            ))
        }
    };
    let Some(needle_value) = function.value(*needle) else {
        return Some("in_array needle is missing from the value table".to_string());
    };
    let Some(haystack_value) = function.value(*haystack) else {
        return Some("in_array haystack is missing from the value table".to_string());
    };
    if haystack_value.ir_type != IrType::Heap(IrHeapKind::Array) {
        return Some(format!(
            "in_array takes an indexed array, got {:?}",
            haystack_value.ir_type
        ));
    }
    let PhpType::Array(element) = haystack_value.php_type.codegen_repr() else {
        return Some(format!(
            "in_array takes an indexed array, got {:?}",
            haystack_value.php_type.codegen_repr()
        ));
    };
    if in_array_scan(&needle_value.php_type, &element).is_none() {
        return Some(format!(
            "in_array has no measured rule for {:?} against elements of {:?}",
            needle_value.php_type.codegen_repr(),
            element
        ));
    }
    if let Some(strict) = strict {
        let Some(value) = function.value(*strict) else {
            return Some("in_array strict flag is missing from the value table".to_string());
        };
        if value.ir_type != IrType::I64
            || !matches!(value.php_type.codegen_repr(), PhpType::Bool)
        {
            return Some(format!(
                "in_array strict flag is {:?}/{:?}, expected I64/Bool",
                value.ir_type,
                value.php_type.codegen_repr()
            ));
        }
    }
    None
}

/// Lowers `in_array` by scanning the haystack with the pair's measured comparison.
///
/// The scan answers the first matching INDEX, which `array_search` boxes and this reduces to a
/// bool — one scan, two builtins.
fn lower_in_array(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    emit_array_find(ctx, inst)?;
    ctx.fb.ins("i64.const 0", "a miss answers -1");
    ctx.fb.ins("i64.ge_s", "found means a non-negative index");
    ctx.fb.ins("i64.extend_i32_u", "PHP booleans are i64 here");
    store_result(ctx, inst)
}

/// Lowers `array_search`: the same scan, boxed as PHP's `int|false`.
///
/// The front-end admits exactly two operands, so only the LOOSE form exists here. `int|false`
/// travels as a Mixed cell — tag 0 carrying the index, or tag 3 carrying the value false —
/// matching what `strpos` already does for the same result type.
fn lower_array_search(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let index = ctx.fresh_temp(super::wat::ValType::I64);
    emit_array_find(ctx, inst)?;
    ctx.fb.ins(&format!("local.set {}", index), "first matching index, or -1");
    ctx.fb.ins(&format!("local.get {}", index), "index");
    ctx.fb.ins("i64.const 0", "a miss answers -1");
    ctx.fb.ins("i64.ge_s", "found?");
    ctx.fb.ins("if (result i32)", "int|false travels as a Mixed cell");
    ctx.fb.ins("i64.const 0", "int tag");
    ctx.fb.ins(&format!("local.get {}", index), "the key");
    ctx.fb.ins("i64.const 0", "no high payload");
    ctx.fb.ins("call $__rt_mixed_from_value", "box the key");
    ctx.fb.ins("else", "not found");
    ctx.fb.ins("i64.const 3", "bool tag");
    ctx.fb.ins("i64.const 0", "the value false");
    ctx.fb.ins("i64.const 0", "no high payload");
    ctx.fb.ins("call $__rt_mixed_from_value", "box false");
    ctx.fb.ins("end", "");
    store_result(ctx, inst)
}

/// Emits the scan shared by `in_array` and `array_search`, leaving the matching index on the
/// stack (-1 when absent).
fn emit_array_find(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let needle = operand(inst, 0)?;
    let haystack = operand(inst, 1)?;
    let needle_php = ctx
        .function
        .value(needle)
        .map(|v| v.php_type.codegen_repr())
        .unwrap_or(PhpType::Mixed);
    let element = match ctx.function.value(haystack).map(|v| v.php_type.codegen_repr()) {
        Some(PhpType::Array(element)) => *element,
        other => {
            return Err(WasmError::Unsupported(format!(
                "in_array takes an indexed array, got {other:?}"
            )))
        }
    };
    let scan = in_array_scan(&needle_php, &element).ok_or_else(|| {
        WasmError::Unsupported(format!(
            "in_array has no rule for {needle_php:?} against {element:?}"
        ))
    })?;

    // The strict flag is a runtime value, so park it: the helpers branch on it.
    let strict = ctx.fresh_temp(super::wat::ValType::I32);
    if inst.operands.len() == 3 {
        ctx.emit_load_value(operand(inst, 2)?)?;
        ctx.fb.ins("i32.wrap_i64", "the strict flag is a PHP bool");
        ctx.fb.ins(&format!("local.set {}", strict), "strict comparison requested");
    } else {
        ctx.fb.ins("i32.const 0", "no strict flag: loose comparison");
        ctx.fb.ins(&format!("local.set {}", strict), "strict comparison requested");
    }

    match scan.kind {
        InArrayKind::Int => {
            ctx.emit_load_value(needle)?;
            ctx.emit_load_value(haystack)?;
            emit_strict_block(ctx, &strict, scan.blocks_strict);
            ctx.fb.ins("call $__rt_array_find_int", "scan comparing as integers");
        }
        InArrayKind::Float => {
            ctx.emit_load_value(needle)?;
            if needle_php.codegen_repr() == PhpType::Int {
                ctx.fb
                    .ins("f64.convert_i64_s", "PHP widens the needle to compare");
            }
            ctx.emit_load_value(haystack)?;
            emit_strict_block(ctx, &strict, scan.blocks_strict);
            ctx.fb.ins(
                &format!("i32.const {}", i32::from(scan.widen_elements)),
                "the elements are integers to convert",
            );
            ctx.fb.ins("call $__rt_array_find_float", "scan comparing as doubles");
        }
        InArrayKind::Str => {
            ctx.emit_load_value(needle)?;
            ctx.emit_load_value(haystack)?;
            ctx.fb.ins(&format!("local.get {}", strict), "=== or ==");
            ctx.fb.ins(
                "call $__rt_array_find_str",
                "scan; loose uses the numeric-string rule",
            );
        }
    }
    Ok(())
}

/// Pushes the "a strict request cannot match" flag: the strict flag itself when the needle and the
/// elements are different types, and a constant zero otherwise.
fn emit_strict_block(ctx: &mut FnCtx, strict: &str, blocks: bool) {
    if blocks {
        ctx.fb
            .ins(&format!("local.get {}", strict), "types differ: === can never match");
    } else {
        ctx.fb.ins("i32.const 0", "same types: === and == agree");
    }
}

/// Validates `sort`/`rsort`: one indexed array of a scalar element the runtime sorts in place.
///
/// String and Mixed elements are NOT admitted: PHP orders strings with its standard comparison,
/// where two numeric strings compare NUMERICALLY — `sort(["10", "9"])` answers `9, 10`, not
/// `10, 9` — and that rule is not this helper's.
/// Validates `array_key_exists($key, $hash)`.
///
/// The haystack must be an associative array: the indexed form asks a different question
/// (an int index against a dense run) and has no lowering here. The key reuses the hash
/// key contract, so a Mixed or float key is refused for the same diagnostic reason
/// `$h[$k]` refuses it — PHP's implicit key conversions are profile-specific.
fn array_key_exists_shape_issue(function: &Function, call: &Instruction) -> Option<String> {
    let [key, hash] = call.operands.as_slice() else {
        return Some(format!(
            "array_key_exists takes a key and an array, got {} operands",
            call.operands.len()
        ));
    };
    let Some(haystack) = function.value(*hash) else {
        return Some("array_key_exists array is missing from the value table".to_string());
    };
    if haystack.ir_type != IrType::Heap(IrHeapKind::Hash) {
        return Some(format!(
            "array_key_exists has no lowering for a haystack of {:?}",
            haystack.ir_type
        ));
    }
    let Some(key_value) = function.value(*key) else {
        return Some("array_key_exists key is missing from the value table".to_string());
    };
    let key_php = key_value.php_type.codegen_repr();
    if key_value.ir_type == IrType::Heap(IrHeapKind::Mixed) || key_php == PhpType::Mixed {
        return Some("dynamic Mixed associative keys require exact per-tag PHP diagnostics".to_string());
    }
    if key_value.ir_type == IrType::F64 && key_php == PhpType::Float {
        return Some(
            "float associative keys require exact profile-specific implicit-conversion diagnostics"
                .to_string(),
        );
    }
    None
}

fn scalar_sort_shape_issue(
    module: &Module,
    function: &Function,
    call: &Instruction,
) -> Option<String> {
    let [array] = call.operands.as_slice() else {
        return Some(format!(
            "sort takes one array, got {} operands",
            call.operands.len()
        ));
    };
    let Some(value) = function.value(*array) else {
        return Some("sort array is missing from the value table".to_string());
    };
    if value.ir_type != IrType::Heap(IrHeapKind::Array) {
        return Some(format!(
            "sort takes an indexed array, got {:?}",
            value.ir_type
        ));
    }
    let PhpType::Array(element) = value.php_type.codegen_repr() else {
        return Some(format!(
            "sort takes an indexed array, got {:?}",
            value.php_type.codegen_repr()
        ));
    };
    // `Void` is what an empty literal's `Never` normalizes to: nothing to order.
    if !matches!(
        element.codegen_repr(),
        PhpType::Int | PhpType::Bool | PhpType::Float | PhpType::Str | PhpType::Void
    ) {
        return Some(format!(
            "sort has no lowered ordering for elements of {element:?}"
        ));
    }
    // The string ordering runs through `__rt_str_smart_cmp`, which lives in the command-only
    // numeric runtime because its classifier's diagnostics write to stderr.
    if element.codegen_repr() == PhpType::Str
        && !module.functions.iter().any(|candidate| candidate.flags.is_main)
    {
        return Some(
            "ordering strings requires a main-bearing command module for the numeric-string \
             classifier"
                .to_string(),
        );
    }
    None
}

/// Lowers `sort`/`rsort` over a scalar array, writing the sorted pointer back into the by-
/// reference argument.
fn lower_scalar_sort(ctx: &mut FnCtx, inst: &Instruction, descending: bool) -> Result<()> {
    let array = operand(inst, 0)?;
    let element = match ctx.function.value(array).map(|v| v.php_type.codegen_repr()) {
        Some(PhpType::Array(element)) => *element,
        other => {
            return Err(WasmError::Unsupported(format!(
                "sort takes an indexed array, got {other:?}"
            )))
        }
    };
    // A string array has 16-byte slots and its own comparison, so it takes a dedicated walk.
    if element.codegen_repr() == PhpType::Str {
        ctx.emit_load_value(array)?;
        ctx.fb
            .ins(&format!("i32.const {}", i32::from(descending)), "rsort reverses the order");
        ctx.fb.ins(
            "call $__rt_sort_string",
            "stable in-place sort by php-src's string ordering (COW)",
        );
    } else {
        let is_float = i32::from(element.codegen_repr() == PhpType::Float);
        ctx.emit_load_value(array)?;
        ctx.fb
            .ins(&format!("i32.const {}", i32::from(descending)), "rsort reverses the order");
        ctx.fb
            .ins(&format!("i32.const {is_float}"), "read the slots as doubles?");
        ctx.fb.ins(
            "call $__rt_sort_scalar",
            "stable in-place sort (COW), returns the array pointer",
        );
    }
    // `sort($a)` rebinds `$a`: the runtime may have cloned, so the pointer goes back.
    ctx.emit_store_value(array)?;
    if let Some(slot) = super::inst::value_source_slot(ctx, array) {
        let array_ref = ctx.value_repr(array)?.local_refs();
        let slot_ref = ctx.slot_repr(slot)?.local_refs();
        if array_ref.len() == 1 && slot_ref.len() == 1 {
            ctx.fb
                .ins(&format!("local.get {}", array_ref[0]), "sorted array pointer");
            ctx.fb
                .ins(&format!("local.set {}", slot_ref[0]), "write back to the array slot");
        }
    }
    Ok(())
}

/// Validates `range`: two integer bounds. The step form does not exist — the front-end rejects
/// any arity but two — and a float or string bound is a different result element type.
fn range_shape_issue(function: &Function, call: &Instruction) -> Option<String> {
    let [start, end] = call.operands.as_slice() else {
        return Some(format!(
            "range takes two bounds, got {} operands",
            call.operands.len()
        ));
    };
    for (operand, side) in [(start, "start"), (end, "end")] {
        let Some(value) = function.value(*operand) else {
            return Some(format!("range {side} is missing from the value table"));
        };
        if value.ir_type != IrType::I64 || value.php_type.codegen_repr() != PhpType::Int {
            return Some(format!(
                "range {side} is {:?}/{:?}, expected I64/Int",
                value.ir_type,
                value.php_type.codegen_repr()
            ));
        }
    }
    None
}

/// Validates `array_merge`: two indexed arrays whose element types agree with the result.
///
/// The front-end admits exactly two operands. Both reach here with the SAME element type — when
/// they differ, EIR widens each with `Op::ArrayToMixed` first — so a mismatch here means the
/// widening did not happen and the slot layouts would disagree.
fn array_merge_shape_issue(function: &Function, call: &Instruction) -> Option<String> {
    let [left, right] = call.operands.as_slice() else {
        return Some(format!(
            "array_merge takes two arrays, got {} operands",
            call.operands.len()
        ));
    };
    let mut elements = Vec::new();
    for (operand, side) in [(left, "left"), (right, "right")] {
        let Some(value) = function.value(*operand) else {
            return Some(format!("array_merge {side} is missing from the value table"));
        };
        if value.ir_type != IrType::Heap(IrHeapKind::Array) {
            return Some(format!(
                "array_merge {side} is {:?}, expected an indexed array",
                value.ir_type
            ));
        }
        let PhpType::Array(element) = value.php_type.codegen_repr() else {
            return Some(format!(
                "array_merge {side} is {:?}, expected an indexed array",
                value.php_type.codegen_repr()
            ));
        };
        // `Void` is what an empty literal's `Never` normalizes to: no element is ever read.
        if !matches!(
            element.codegen_repr(),
            PhpType::Int | PhpType::Str | PhpType::Float | PhpType::Bool | PhpType::Mixed | PhpType::Void
        ) {
            return Some(format!(
                "array_merge has no lowered element copy for {element:?}"
            ));
        }
        elements.push(element.codegen_repr());
    }
    // An empty operand carries no elements, so its type never has to agree.
    let concrete: Vec<_> = elements
        .iter()
        .filter(|element| **element != PhpType::Void)
        .collect();
    if concrete.len() == 2 && concrete[0] != concrete[1] {
        return Some(format!(
            "array_merge operands disagree on element storage: {:?} and {:?}",
            concrete[0], concrete[1]
        ));
    }
    None
}

/// Returns the cell tag and source slot stride for slicing an array, or `None` when this target
/// has no lowered layout for that element type.
///
/// A NEGATIVE tag marks a source whose slots already hold cells: `__rt_array_slice` shares those
/// with an incref instead of copying a payload.
fn array_slice_element_shape(element: &PhpType) -> Option<(i64, i64)> {
    Some(match element.codegen_repr() {
        PhpType::Int => (0, 8),
        PhpType::Str => (1, 16),
        PhpType::Float => (2, 8),
        PhpType::Bool => (3, 8),
        PhpType::Mixed => (-1, 16),
        // `Void` is what `Never` — the element type of a literal `[]` — normalizes to. There is
        // nothing to copy, so the tag and stride are never read.
        PhpType::Void => (0, 8),
        _ => return None,
    })
}

/// Validates `array_slice`'s shape: an indexed array, an int offset, and an optional int length.
///
/// The `preserve_keys` form is NOT lowered: it answers a hash whose keys are the source's
/// positions, which is a different result type from the reindexed list this produces.
fn array_slice_shape_issue(function: &Function, call: &Instruction) -> Option<String> {
    let (array, offset, length) = match call.operands.as_slice() {
        [array, offset] => (array, offset, None),
        [array, offset, length] => (array, offset, Some(length)),
        other => {
            return Some(format!(
                "array_slice takes an array, an offset and an optional length, got {} operands",
                other.len()
            ))
        }
    };
    let Some(array_value) = function.value(*array) else {
        return Some("array is missing from the value table".to_string());
    };
    if array_value.ir_type != IrType::Heap(IrHeapKind::Array) {
        return Some(format!(
            "array_slice takes an indexed array, got {:?}",
            array_value.ir_type
        ));
    }
    let PhpType::Array(element) = array_value.php_type.codegen_repr() else {
        return Some(format!(
            "array_slice takes an indexed array, got {:?}",
            array_value.php_type.codegen_repr()
        ));
    };
    if array_slice_element_shape(&element).is_none() {
        return Some(format!(
            "array_slice has no lowered element copy for {:?}",
            element
        ));
    }
    for (operand, what) in [Some(offset), length].into_iter().zip(["offset", "length"]) {
        let Some(operand) = operand else { continue };
        let Some(value) = function.value(*operand) else {
            return Some(format!("array_slice {what} is missing from the value table"));
        };
        if value.ir_type != IrType::I64
            || !matches!(value.php_type.codegen_repr(), PhpType::Int)
        {
            return Some(format!(
                "array_slice {what} is {:?}/{:?}, expected I64/Int",
                value.ir_type,
                value.php_type.codegen_repr()
            ));
        }
    }
    None
}

/// Validates `implode`: a glue string and an indexed array of strings, a string out.
fn implode_shape_issue(function: &Function, call: &Instruction) -> Option<String> {
    let [glue, array] = call.operands.as_slice() else {
        return Some(format!(
            "expected a glue and an array, got {} operands",
            call.operands.len()
        ));
    };
    let Some(glue_value) = function.value(*glue) else {
        return Some("glue is missing from the value table".to_string());
    };
    if glue_value.ir_type != IrType::Str || glue_value.php_type.codegen_repr() != PhpType::Str {
        return Some(format!(
            "implode glue is {:?}/{:?}, expected Str/Str",
            glue_value.ir_type,
            glue_value.php_type.codegen_repr()
        ));
    }
    let Some(array_value) = function.value(*array) else {
        return Some("array is missing from the value table".to_string());
    };
    if array_value.ir_type != IrType::Heap(IrHeapKind::Array) {
        return Some(format!(
            "implode takes an indexed array, got {:?}",
            array_value.ir_type
        ));
    }
    // The element type has to be exactly string: `__rt_array_get_str` reads a slot as a
    // (pointer, length) pair, and a slot holding an int or a boxed Mixed is a different layout.
    // `Never` is admitted alongside it because it is the type of an array proven to have no
    // elements — `implode(",", [])` — where the element read never happens and the answer is the
    // empty string. Refusing it would reject a shape whose result cannot be got wrong.
    //
    // `Mixed` elements are admitted through a different helper: PHP converts each with the same
    // rule as an explicit `(string)` cast, measured element by element, so the conversion is one
    // this backend already implements exactly. That is what makes `implode(",", array_slice(…))`
    // reachable, since a slice always answers `array<mixed>`.
    if !matches!(
        &array_value.php_type,
        PhpType::Array(element)
            if matches!(
                **element,
                PhpType::Str
                    | PhpType::Never
                    | PhpType::Int
                    | PhpType::Float
                    | PhpType::Mixed
            )
    ) {
        return Some(format!(
            "implode elements must be exactly string, got {:?}",
            array_value.php_type
        ));
    }
    if call.result.is_none()
        || call.result_type != IrType::Str
        || call.result_php_type.codegen_repr() != PhpType::Str
    {
        return Some(format!(
            "implode result {:?}/{:?} is not the expected Str/Str",
            call.result_type,
            call.result_php_type.codegen_repr()
        ));
    }
    None
}

/// Lowers `strrpos` in its two-argument form, boxing PHP's `int|false`.
///
/// Only the two-argument form is admitted. The three-argument form's `$offset` has a rule of its
/// own that is NOT the mirror of `strpos`'s — a negative offset there bounds where the match may
/// START, counted from the end — so it is refused rather than assumed symmetrical.
fn lower_string_rsearch(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let found = ctx.fresh_temp(super::wat::ValType::I64);
    ctx.emit_load_value(operand(inst, 0)?)?;
    ctx.emit_load_value(operand(inst, 1)?)?;
    ctx.fb
        .ins("call $__rt_str_rfind", "last offset, or -1 when absent");
    ctx.fb
        .ins(&format!("local.set {found}"), "spill the scan result");
    ctx.fb.ins(&format!("local.get {found}"), "scan result");
    ctx.fb.ins("i64.const 0", "the absent sentinel is negative");
    ctx.fb.ins("i64.lt_s", "was the needle absent?");
    ctx.fb.ins("if (result i32)", "int|false travels as a Mixed cell");
    ctx.fb.ins("i64.const 3", "mixed tag (bool)");
    ctx.fb.ins("i64.const 0", "the value false");
    ctx.fb.ins("i64.const 0", "hi unused");
    ctx.fb.ins("call $__rt_mixed_from_value", "box PHP's false");
    ctx.fb.ins("else", "the needle was found");
    ctx.fb.ins("i64.const 0", "mixed tag (int)");
    ctx.fb.ins(&format!("local.get {found}"), "the byte offset");
    ctx.fb.ins("i64.const 0", "hi unused");
    ctx.fb.ins("call $__rt_mixed_from_value", "box the offset");
    ctx.fb.ins("end", "end int|false selection");
    store_result(ctx, inst)
}

/// Lowers `strpos` in its two-argument form.
///
/// The scan helper takes a case-folding flag because `stripos`, `strrpos` and `strstr` all want
/// the same search with one knob changed; only `strpos` is registered as a distinct runtime
/// identity today, so this passes the case-sensitive setting.
///
/// PHP's result is `int|false`, which EIR carries as a `Mixed` cell, so the two outcomes are
/// boxed under different tags: an index under the int tag and a miss under the BOOL tag holding
/// zero, which is exactly `false`. Returning 0 under the int tag instead would make
/// `strpos($h, $n) === false` answer wrong for a match at the start — the classic PHP trap this
/// distinction exists to serve.
fn lower_string_search(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let found = ctx.fresh_temp(super::wat::ValType::I64);
    ctx.emit_load_value(operand(inst, 0)?)?;
    ctx.emit_load_value(operand(inst, 1)?)?;
    ctx.fb.ins(
        "i32.const 0",
        "strpos is case-sensitive",
    );
    ctx.fb
        .ins("call $__rt_str_find", "first offset, or -1 when absent");
    ctx.fb
        .ins(&format!("local.set {found}"), "spill the scan result");
    ctx.fb.ins(&format!("local.get {found}"), "scan result");
    ctx.fb.ins("i64.const 0", "the absent sentinel is negative");
    ctx.fb.ins("i64.lt_s", "was the needle absent?");
    ctx.fb.ins("if (result i32)", "int|false travels as a Mixed cell");
    ctx.fb.ins("i64.const 3", "mixed tag (bool)");
    ctx.fb.ins("i64.const 0", "the value false");
    ctx.fb.ins("i64.const 0", "hi unused");
    ctx.fb
        .ins("call $__rt_mixed_from_value", "box PHP's false");
    ctx.fb.ins("else", "the needle was found");
    ctx.fb.ins("i64.const 0", "mixed tag (int)");
    ctx.fb.ins(&format!("local.get {found}"), "the byte offset");
    ctx.fb.ins("i64.const 0", "hi unused");
    ctx.fb.ins("call $__rt_mixed_from_value", "box the offset");
    ctx.fb.ins("end", "end int|false selection");
    store_result(ctx, inst)
}

/// Validates `strpos` and `stripos`: two strings in, PHP's `int|false` Mixed out.
///
/// Only the two-argument form is admitted. The three-argument form has to validate `$offset`
/// against the haystack and raise a `ValueError` naming the called function when it does not fit,
/// which is a different contract rather than a default, so it is refused rather than guessed.
fn string_search_shape_issue(
    function: &Function,
    call: &Instruction,
    target: RuntimeFnId,
) -> Option<String> {
    let [haystack, needle] = call.operands.as_slice() else {
        return Some(format!(
            "expected a haystack and a needle with no offset, got {} operands",
            call.operands.len()
        ));
    };
    for operand in [haystack, needle] {
        let Some(value) = function.value(*operand) else {
            return Some("operand is missing from the value table".to_string());
        };
        if value.ir_type != IrType::Str || value.php_type.codegen_repr() != PhpType::Str {
            return Some(format!(
                "expected a string operand, got {:?}/{:?}",
                value.ir_type,
                value.php_type.codegen_repr()
            ));
        }
    }
    if call.result.is_none()
        || call.result_type != IrType::Heap(IrHeapKind::Mixed)
        || call.result_php_type.codegen_repr() != PhpType::Mixed
    {
        return Some(format!(
            "{target:?} result {:?}/{:?} is not the Mixed cell PHP's int|false needs",
            call.result_type,
            call.result_php_type.codegen_repr()
        ));
    }
    None
}

/// Lowers `str_replace` in its all-string, three-argument form.
///
/// PHP also accepts arrays for any of the three arguments and an optional by-reference `$count`,
/// which are different operations rather than defaults, so only the string form is admitted.
fn lower_str_replace(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    ctx.emit_load_value(operand(inst, 2)?)?;
    ctx.emit_load_value(operand(inst, 0)?)?;
    ctx.emit_load_value(operand(inst, 1)?)?;
    ctx.fb
        .ins("call $__rt_str_replace", "own the rewritten bytes");
    store_result(ctx, inst)
}

/// Validates `str_replace`: three strings in, a string out.
fn str_replace_shape_issue(function: &Function, call: &Instruction) -> Option<String> {
    let [search, replace, subject] = call.operands.as_slice() else {
        return Some(format!(
            "expected a search, a replacement and a subject, got {} operands",
            call.operands.len()
        ));
    };
    for operand in [search, replace, subject] {
        let Some(value) = function.value(*operand) else {
            return Some("operand is missing from the value table".to_string());
        };
        if value.ir_type != IrType::Str || value.php_type.codegen_repr() != PhpType::Str {
            return Some(format!(
                "str_replace takes strings, got {:?}/{:?}",
                value.ir_type,
                value.php_type.codegen_repr()
            ));
        }
    }
    if call.result.is_none()
        || call.result_type != IrType::Str
        || call.result_php_type.codegen_repr() != PhpType::Str
    {
        return Some(format!(
            "str_replace result {:?}/{:?} is not the expected Str/Str",
            call.result_type,
            call.result_php_type.codegen_repr()
        ));
    }
    None
}

/// Validates `crc32`: one string in, an integer out.
fn crc32_shape_issue(function: &Function, call: &Instruction) -> Option<String> {
    let [subject] = call.operands.as_slice() else {
        return Some(format!(
            "expected one string operand, got {}",
            call.operands.len()
        ));
    };
    let Some(value) = function.value(*subject) else {
        return Some("operand is missing from the value table".to_string());
    };
    if value.ir_type != IrType::Str || value.php_type.codegen_repr() != PhpType::Str {
        return Some(format!(
            "crc32 takes a string, got {:?}/{:?}",
            value.ir_type,
            value.php_type.codegen_repr()
        ));
    }
    if call.result.is_none()
        || call.result_type != IrType::I64
        || call.result_php_type.codegen_repr() != PhpType::Int
    {
        return Some(format!(
            "crc32 result {:?}/{:?} is not the expected I64/Int",
            call.result_type,
            call.result_php_type.codegen_repr()
        ));
    }
    None
}

/// The `__rt_fail` code PHP's empty-pad `str_pad` `ValueError` reports under.
const STR_PAD_EMPTY_FAILURE_CODE: i32 = 12;

/// Lowers `str_pad` in its two- and three-argument forms.
///
/// PHP refuses an empty `$pad_string` — but ONLY when padding is actually needed. `str_pad("abc",
/// 2, "")` answers `"abc"` rather than raising, so the guard tests the target length as well as
/// the pad, and raises through the same path the other builtin ValueErrors take.
///
/// The four-argument form stays refused: it validates `$pad_type` eagerly, raising even when no
/// padding is needed and even when the pad string is also invalid, which is a different contract
/// with its own message rather than a default for this one.
fn lower_str_pad(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let target = ctx.fresh_temp(super::wat::ValType::I64);
    let sptr = ctx.fresh_temp(super::wat::ValType::I32);
    let slen = ctx.fresh_temp(super::wat::ValType::I64);
    let pptr = ctx.fresh_temp(super::wat::ValType::I32);
    let plen = ctx.fresh_temp(super::wat::ValType::I64);

    ctx.emit_load_value(operand(inst, 0)?)?;
    ctx.fb.ins(&format!("local.set {slen}"), "spill subject length");
    ctx.fb.ins(&format!("local.set {sptr}"), "spill subject pointer");
    ctx.emit_load_value(operand(inst, 1)?)?;
    ctx.fb.ins(&format!("local.set {target}"), "spill the target length");
    if inst.operands.len() == 3 {
        ctx.emit_load_value(operand(inst, 2)?)?;
        ctx.fb.ins(&format!("local.set {plen}"), "spill pad length");
        ctx.fb.ins(&format!("local.set {pptr}"), "spill pad pointer");
    } else {
        // A null pad pointer means PHP's default single space. Synthesizing it in the helper
        // costs one branch and keeps a module that never pads from carrying a data segment.
        ctx.fb.ins("i32.const 0", "null pad pointer selects the default space");
        ctx.fb.ins(&format!("local.set {pptr}"), "spill pad pointer");
        ctx.fb.ins("i64.const 1", "the default pad is one byte");
        ctx.fb.ins(&format!("local.set {plen}"), "spill pad length");
    }

    if inst.operands.len() == 3 {
        ctx.fb.ins(&format!("local.get {plen}"), "pad length");
        ctx.fb.ins("i64.eqz", "an empty pad string?");
        ctx.fb.ins(&format!("local.get {target}"), "target length");
        ctx.fb.ins(&format!("local.get {slen}"), "subject length");
        ctx.fb.ins("i64.gt_s", "is padding actually needed?");
        ctx.fb.ins("i32.and", "PHP raises only when both hold");
        ctx.fb.ins("if", "str_pad() rejects an empty pad it would have to use");
        super::inst::emit_runtime_failure(
            ctx,
            STR_PAD_EMPTY_FAILURE_CODE,
            "str_pad() empty pad string",
        );
        ctx.fb.ins("end", "end empty-pad guard");
    }

    ctx.fb.ins(&format!("local.get {sptr}"), "subject pointer");
    ctx.fb.ins(&format!("local.get {slen}"), "subject length");
    ctx.fb.ins(&format!("local.get {target}"), "target length");
    ctx.fb.ins(&format!("local.get {pptr}"), "pad pointer, or 0 for the default space");
    ctx.fb.ins(&format!("local.get {plen}"), "pad length");
    ctx.fb.ins("i32.const 1", "STR_PAD_RIGHT");
    ctx.fb.ins("call $__rt_str_pad", "own the padded bytes");
    store_result(ctx, inst)
}

/// Validates `str_pad`: a subject, an integer length, and optionally a pad string.
fn str_pad_shape_issue(function: &Function, call: &Instruction) -> Option<String> {
    if !matches!(call.operands.len(), 2 | 3) {
        return Some(format!(
            "expected a subject, a length and an optional pad, got {} operands",
            call.operands.len()
        ));
    }
    for (index, operand) in call.operands.iter().enumerate() {
        let Some(value) = function.value(*operand) else {
            return Some("operand is missing from the value table".to_string());
        };
        let (want_ir, want_php) = if index == 1 {
            (IrType::I64, PhpType::Int)
        } else {
            (IrType::Str, PhpType::Str)
        };
        if value.ir_type != want_ir || value.php_type.codegen_repr() != want_php {
            return Some(format!(
                "str_pad operand {index} is {:?}/{:?}, expected {want_ir:?}/{want_php:?}",
                value.ir_type,
                value.php_type.codegen_repr()
            ));
        }
    }
    if call.result.is_none()
        || call.result_type != IrType::Str
        || call.result_php_type.codegen_repr() != PhpType::Str
    {
        return Some(format!(
            "str_pad result {:?}/{:?} is not the expected Str/Str",
            call.result_type,
            call.result_php_type.codegen_repr()
        ));
    }
    None
}

/// The `__rt_fail` code PHP's negative-`str_repeat` `ValueError` reports under.
const STR_REPEAT_NEGATIVE_FAILURE_CODE: i32 = 11;

/// Lowers `str_repeat`, refusing a negative count the way PHP does.
///
/// PHP does not clamp a negative `$times` to zero: it raises a `ValueError`, which an ordinary
/// `catch` receives. The guard therefore goes through the shared runtime-failure path so it is
/// RAISED where the module can catch it and reported as a fatal where it cannot — the same
/// treatment division by zero gets. The count is spilled to a local because the guard reads it
/// before the helper consumes it.
fn lower_str_repeat(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let times = ctx.fresh_temp(super::wat::ValType::I64);
    ctx.emit_load_value(operand(inst, 1)?)?;
    ctx.fb
        .ins(&format!("local.set {times}"), "spill the repeat count");
    ctx.fb
        .ins(&format!("local.get {times}"), "repeat count");
    ctx.fb.ins("i64.const 0", "PHP's lower bound");
    ctx.fb.ins("i64.lt_s", "negative count?");
    ctx.fb.ins("if", "str_repeat() rejects a negative count");
    super::inst::emit_runtime_failure(
        ctx,
        STR_REPEAT_NEGATIVE_FAILURE_CODE,
        "str_repeat() negative count",
    );
    ctx.fb.ins("end", "end negative-count guard");
    ctx.emit_load_value(operand(inst, 0)?)?;
    ctx.fb
        .ins(&format!("local.get {times}"), "the validated repeat count");
    ctx.fb
        .ins("call $__rt_str_repeat", "own the repeated bytes");
    store_result(ctx, inst)
}

/// Validates `str_repeat`: a string subject and an integer count.
fn str_repeat_shape_issue(function: &Function, call: &Instruction) -> Option<String> {
    let [subject, times] = call.operands.as_slice() else {
        return Some(format!(
            "expected a subject and a count, got {} operands",
            call.operands.len()
        ));
    };
    for (operand, want_ir, want_php) in [
        (subject, IrType::Str, PhpType::Str),
        (times, IrType::I64, PhpType::Int),
    ] {
        let Some(value) = function.value(*operand) else {
            return Some("operand is missing from the value table".to_string());
        };
        if value.ir_type != want_ir || value.php_type.codegen_repr() != want_php {
            return Some(format!(
                "str_repeat operand is {:?}/{:?}, expected {want_ir:?}/{want_php:?}",
                value.ir_type,
                value.php_type.codegen_repr()
            ));
        }
    }
    if call.result.is_none()
        || call.result_type != IrType::Str
        || call.result_php_type.codegen_repr() != PhpType::Str
    {
        return Some(format!(
            "str_repeat result {:?}/{:?} is not the expected Str/Str",
            call.result_type,
            call.result_php_type.codegen_repr()
        ));
    }
    None
}

/// Validates `trim`, `ltrim` and `rtrim`: a string, and optionally a charlist string.
fn trim_shape_issue(
    function: &Function,
    call: &Instruction,
    target: RuntimeFnId,
) -> Option<String> {
    if !matches!(call.operands.len(), 1 | 2) {
        return Some(format!(
            "expected a subject and an optional charlist, got {} operands",
            call.operands.len()
        ));
    }
    for operand in &call.operands {
        let Some(value) = function.value(*operand) else {
            return Some("operand is missing from the value table".to_string());
        };
        if value.ir_type != IrType::Str || value.php_type.codegen_repr() != PhpType::Str {
            return Some(format!(
                "expected a string operand, got {:?}/{:?}",
                value.ir_type,
                value.php_type.codegen_repr()
            ));
        }
    }
    if call.result.is_none()
        || call.result_type != IrType::Str
        || call.result_php_type.codegen_repr() != PhpType::Str
    {
        return Some(format!(
            "{target:?} result {:?}/{:?} is not the expected Str/Str",
            call.result_type,
            call.result_php_type.codegen_repr()
        ));
    }
    None
}

/// Validates `substr`: a string, an integer offset, and optionally an integer length.
fn substr_shape_issue(function: &Function, call: &Instruction) -> Option<String> {
    if !matches!(call.operands.len(), 2 | 3) {
        return Some(format!(
            "expected a subject, an offset and an optional length, got {} operands",
            call.operands.len()
        ));
    }
    for (index, operand) in call.operands.iter().enumerate() {
        let Some(value) = function.value(*operand) else {
            return Some("operand is missing from the value table".to_string());
        };
        let (want_ir, want_php) = if index == 0 {
            (IrType::Str, PhpType::Str)
        } else {
            (IrType::I64, PhpType::Int)
        };
        if value.ir_type != want_ir || value.php_type.codegen_repr() != want_php {
            return Some(format!(
                "substr operand {index} is {:?}/{:?}, expected {want_ir:?}/{want_php:?}",
                value.ir_type,
                value.php_type.codegen_repr()
            ));
        }
    }
    if call.result.is_none()
        || call.result_type != IrType::Str
        || call.result_php_type.codegen_repr() != PhpType::Str
    {
        return Some(format!(
            "substr result {:?}/{:?} is not the expected Str/Str",
            call.result_type,
            call.result_php_type.codegen_repr()
        ));
    }
    None
}

/// Validates `strcmp` and `strcasecmp`: two strings in, an integer out.
fn string_compare_shape_issue(
    function: &Function,
    call: &Instruction,
    target: RuntimeFnId,
) -> Option<String> {
    let [left, right] = call.operands.as_slice() else {
        return Some(format!(
            "expected two string operands, got {}",
            call.operands.len()
        ));
    };
    for operand in [left, right] {
        let Some(value) = function.value(*operand) else {
            return Some("operand is missing from the value table".to_string());
        };
        if value.ir_type != IrType::Str || value.php_type.codegen_repr() != PhpType::Str {
            return Some(format!(
                "expected a string operand, got {:?}/{:?}",
                value.ir_type,
                value.php_type.codegen_repr()
            ));
        }
    }
    if call.result.is_none()
        || call.result_type != IrType::I64
        || call.result_php_type.codegen_repr() != PhpType::Int
    {
        return Some(format!(
            "{target:?} result {:?}/{:?} is not the expected I64/Int",
            call.result_type,
            call.result_php_type.codegen_repr()
        ));
    }
    None
}

/// Validates `chr` and `ord`: one concrete scalar in, the opposite one out.
///
/// A `mixed` argument is refused rather than coerced. PHP would juggle it, and juggling carries
/// its own per-tag diagnostics that this backend does not reproduce yet, so admitting one here
/// would answer confidently where PHP would have complained first.
fn byte_conversion_shape_issue(
    function: &Function,
    call: &Instruction,
    target: RuntimeFnId,
) -> Option<String> {
    let (argument_ir, argument_php, result_ir, result_php) = if target == RuntimeFnId::Chr {
        (IrType::I64, PhpType::Int, IrType::Str, PhpType::Str)
    } else {
        (IrType::Str, PhpType::Str, IrType::I64, PhpType::Int)
    };
    let [operand] = call.operands.as_slice() else {
        return Some(format!(
            "expected one operand, got {}",
            call.operands.len()
        ));
    };
    let Some(value) = function.value(*operand) else {
        return Some("operand is missing from the value table".to_string());
    };
    if value.ir_type != argument_ir || value.php_type.codegen_repr() != argument_php {
        return Some(format!(
            "{target:?} operand {:?}/{:?} is not the expected {argument_ir:?}/{argument_php:?}",
            value.ir_type,
            value.php_type.codegen_repr()
        ));
    }
    if call.result.is_none()
        || call.result_type != result_ir
        || call.result_php_type.codegen_repr() != result_php
    {
        return Some(format!(
            "{target:?} result {:?}/{:?} is not the expected {result_ir:?}/{result_php:?}",
            call.result_type,
            call.result_php_type.codegen_repr()
        ));
    }
    None
}

fn string_predicate_shape_issue(
    function: &Function,
    call: &Instruction,
    target: RuntimeFnId,
) -> Option<String> {
    let [haystack, needle] = call.operands.as_slice() else {
        return Some(format!(
            "expected a haystack and a needle, got {} operands",
            call.operands.len()
        ));
    };
    for operand in [haystack, needle] {
        let Some(value) = function.value(*operand) else {
            return Some("operand is missing from the value table".to_string());
        };
        if value.ir_type != IrType::Str || value.php_type.codegen_repr() != PhpType::Str {
            return Some(format!(
                "expected a string operand, got {:?}/{:?}",
                value.ir_type,
                value.php_type.codegen_repr()
            ));
        }
    }
    if call.result.is_none()
        || call.result_type != IrType::I64
        || call.result_php_type.codegen_repr() != PhpType::Bool
    {
        return Some(format!(
            "{target:?} result {:?}/{:?} is not the expected I64/Bool",
            call.result_type,
            call.result_php_type.codegen_repr()
        ));
    }
    None
}

/// Lowers the three PHP 8 substring predicates to a byte comparison.
///
/// `str_starts_with` and `str_ends_with` compare ONE region, so they check the needle fits and
/// then compare at offset zero or at `hlen - nlen`. `str_contains` scans every offset that
/// leaves room. An empty needle matches in all three, which is PHP's answer.
fn lower_string_predicate(
    ctx: &mut FnCtx,
    inst: &Instruction,
    target: RuntimeFnId,
) -> Result<()> {
    let haystack = operand(inst, 0)?;
    let needle = operand(inst, 1)?;
    let (hptr, hlen) = match ctx.value_repr(haystack)?.clone() {
        super::values::WasmRepr::Str { ptr, len } => (ptr, len),
        other => {
            return Err(WasmError::Unsupported(format!(
                "string predicate haystack is {:?}",
                other
            )))
        }
    };
    let (nptr, nlen) = match ctx.value_repr(needle)?.clone() {
        super::values::WasmRepr::Str { ptr, len } => (ptr, len),
        other => {
            return Err(WasmError::Unsupported(format!(
                "string predicate needle is {:?}",
                other
            )))
        }
    };
    if target == RuntimeFnId::StrContains {
        ctx.fb.ins(&format!("local.get {}", hptr), "haystack pointer");
        ctx.fb.ins(&format!("local.get {}", hlen), "haystack length");
        ctx.fb.ins(&format!("local.get {}", nptr), "needle pointer");
        ctx.fb.ins(&format!("local.get {}", nlen), "needle length");
        ctx.fb
            .ins("call $__rt_str_contains", "scan every start offset");
        return store_result(ctx, inst);
    }
    // A needle longer than the haystack cannot match at any single offset, and the comparison
    // would read past the end, so the length check has to come first.
    ctx.fb.ins(&format!("local.get {}", nlen), "needle length");
    ctx.fb.ins(&format!("local.get {}", hlen), "haystack length");
    ctx.fb.ins("i64.gt_s", "does the needle overrun the haystack?");
    ctx.fb.ins("if (result i64)", "needle too long");
    ctx.fb.ins("i64.const 0", "an overrunning needle never matches");
    ctx.fb.ins("else", "the needle fits");
    ctx.fb.ins(&format!("local.get {}", hptr), "haystack pointer");
    ctx.fb.ins(&format!("local.get {}", nptr), "needle pointer");
    ctx.fb.ins(&format!("local.get {}", nlen), "needle length");
    if target == RuntimeFnId::StrStartsWith {
        ctx.fb.ins("i64.const 0", "compare at the start");
    } else {
        ctx.fb.ins(&format!("local.get {}", hlen), "haystack length");
        ctx.fb.ins(&format!("local.get {}", nlen), "needle length");
        ctx.fb.ins("i64.sub", "compare at the trailing region");
    }
    ctx.fb
        .ins("call $__rt_str_region_eq", "compare the one candidate region");
    ctx.fb.ins("end", "end needle-length guard");
    store_result(ctx, inst)
}

/// Lowers `count($array)` to the container header's element count at `[ptr + 0]`.
fn lower_count(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    ctx.emit_load_value(operand(inst, 0)?)?;
    ctx.fb.ins("i64.load", "container element count @ +0");
    store_result(ctx, inst)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{Builder, Immediate, Op, Ownership, RuntimeCallTarget};

    /// Builds a one-instruction function calling `target` with one operand of the given storage.
    fn call_with(
        target: RuntimeFnId,
        operand_ir: IrType,
        operand_php: PhpType,
        result_ir: IrType,
        result_php: PhpType,
    ) -> Function {
        let mut function = Function::new("probe".to_string(), IrType::Void, PhpType::Void);
        {
            let mut builder = Builder::new(&mut function);
            let entry = builder.create_named_block("entry", Vec::new());
            builder.set_entry(entry);
            builder.position_at_end(entry);
            let slot = builder.add_local(
                Some("v".to_string()),
                operand_ir,
                operand_php.clone(),
                crate::ir::LocalKind::PhpLocal,
            );
            let argument = builder.emit_load_local(slot, operand_ir, operand_php);
            builder.emit(
                Op::RuntimeCall,
                vec![argument],
                Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(target))),
                result_ir,
                result_php,
                Ownership::NonHeap,
            );
            builder.terminate(crate::ir::Terminator::Return { value: None });
        }
        function
    }

    /// Returns the audit verdict for the last instruction of `function`.
    fn verdict(function: &Function, target: RuntimeFnId) -> Option<String> {
        let call = function
            .instructions
            .last()
            .expect("the probe emitted a call");
        direct_builtin_shape_issue(&probe_module(), function, call, target)
    }

    /// Builds `in_array($needle, $haystack, $strict)` with the given strict operand.
    fn in_array_call(strict_is_literal_true: bool) -> Function {
        let mut function = Function::new("probe".to_string(), IrType::Void, PhpType::Void);
        {
            let mut builder = Builder::new(&mut function);
            let entry = builder.create_named_block("entry", Vec::new());
            builder.set_entry(entry);
            builder.position_at_end(entry);
            let needle_slot = builder.add_local(
                Some("n".to_string()),
                IrType::I64,
                PhpType::Int,
                crate::ir::LocalKind::PhpLocal,
            );
            let haystack_slot = builder.add_local(
                Some("a".to_string()),
                IrType::Heap(IrHeapKind::Array),
                PhpType::Array(Box::new(PhpType::Int)),
                crate::ir::LocalKind::PhpLocal,
            );
            let needle = builder.emit_load_local(needle_slot, IrType::I64, PhpType::Int);
            let haystack = builder.emit_load_local(
                haystack_slot,
                IrType::Heap(IrHeapKind::Array),
                PhpType::Array(Box::new(PhpType::Int)),
            );
            let strict = if strict_is_literal_true {
                builder.emit_const_bool(true)
            } else {
                builder.emit_const_bool(false)
            };
            builder.emit(
                Op::RuntimeCall,
                vec![needle, haystack, strict],
                Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
                    RuntimeFnId::InArray,
                ))),
                IrType::I64,
                PhpType::Bool,
                Ownership::NonHeap,
            );
            builder.terminate(crate::ir::Terminator::Return { value: None });
        }
        function
    }

    /// Builds a unary string transform call with the given operand storage.
    fn unary_string_call(
        target: UnaryStringRuntime,
        operand_ir: IrType,
        operand_php: PhpType,
    ) -> Function {
        let mut function = Function::new("probe".to_string(), IrType::Void, PhpType::Void);
        {
            let mut builder = Builder::new(&mut function);
            let entry = builder.create_named_block("entry", Vec::new());
            builder.set_entry(entry);
            builder.position_at_end(entry);
            let slot = builder.add_local(
                Some("s".to_string()),
                operand_ir,
                operand_php.clone(),
                crate::ir::LocalKind::PhpLocal,
            );
            let argument = builder.emit_load_local(slot, operand_ir, operand_php);
            builder.emit(
                Op::RuntimeCall,
                vec![argument],
                Some(Immediate::RuntimeCall(RuntimeCallTarget::UnaryString(
                    target,
                ))),
                IrType::Str,
                PhpType::Str,
                Ownership::MaybeOwned,
            );
            builder.terminate(crate::ir::Terminator::Return { value: None });
        }
        function
    }

    /// Verifies the admitted unary string transforms take a string and nothing else.
    #[test]
    fn unary_string_transforms_admit_only_strings() {
        for target in [
            UnaryStringRuntime::StrToUpper,
            UnaryStringRuntime::StrToLower,
            UnaryStringRuntime::StrReverse,
            UnaryStringRuntime::BinToHex,
            UnaryStringRuntime::AddSlashes,
            UnaryStringRuntime::StripSlashes,
            UnaryStringRuntime::NlToBr,
            UnaryStringRuntime::UrlEncode,
            UnaryStringRuntime::RawUrlEncode,
            UnaryStringRuntime::UrlDecode,
            UnaryStringRuntime::RawUrlDecode,
            UnaryStringRuntime::Base64Encode,
            UnaryStringRuntime::Base64Decode,
        ] {
            assert!(unary_string_is_supported(target), "{target:?} is lowered");
            let ok = unary_string_call(target, IrType::Str, PhpType::Str);
            let call = ok.instructions.last().expect("the probe emitted a call");
            assert_eq!(unary_string_shape_issue(&ok, call, target), None);

            let scalar = unary_string_call(target, IrType::I64, PhpType::Int);
            let call = scalar.instructions.last().expect("the probe emitted a call");
            assert!(
                unary_string_shape_issue(&scalar, call, target).is_some(),
                "{target:?} maps string bytes"
            );
        }

        // The html decoders need a real named-entity table, and `hex2bin` returns `string|false`
        // in PHP rather than the `string` this family's signature promises.
        assert!(!unary_string_is_supported(
            UnaryStringRuntime::HtmlEntityDecode
        ));
        assert!(!unary_string_is_supported(UnaryStringRuntime::HexToBin));
    }

    /// Builds a one-operand `RuntimeCall` probe with a chosen operand and result typing.
    fn scalar_conversion_call(
        target: RuntimeFnId,
        operand_ir: IrType,
        operand_php: PhpType,
        result_ir: IrType,
        result_php: PhpType,
    ) -> Function {
        let mut function = Function::new("probe".to_string(), IrType::Void, PhpType::Void);
        {
            let mut builder = Builder::new(&mut function);
            let entry = builder.create_named_block("entry", Vec::new());
            builder.set_entry(entry);
            builder.position_at_end(entry);
            let slot = builder.add_local(
                Some("v".to_string()),
                operand_ir,
                operand_php.clone(),
                crate::ir::LocalKind::PhpLocal,
            );
            let argument = builder.emit_load_local(slot, operand_ir, operand_php);
            builder.emit(
                Op::RuntimeCall,
                vec![argument],
                Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(target))),
                result_ir,
                result_php,
                Ownership::MaybeOwned,
            );
            builder.terminate(crate::ir::Terminator::Return { value: None });
        }
        function
    }

    /// Verifies `chr` and `ord` take one concrete scalar and refuse anything juggled.
    ///
    /// `RuntimeFnId::Chr` maps an int to a one-byte string and `RuntimeFnId::Ord` maps a string
    /// back to an int, so each refuses the other's typing as well as a `mixed` operand. PHP would
    /// juggle a `mixed` with its own per-tag diagnostics, which this backend does not reproduce,
    /// so admitting one would answer confidently where PHP would have complained first.
    #[test]
    fn chr_and_ord_admit_only_their_concrete_scalar() {
        let chr = scalar_conversion_call(
            RuntimeFnId::Chr,
            IrType::I64,
            PhpType::Int,
            IrType::Str,
            PhpType::Str,
        );
        let call = chr.instructions.last().expect("the probe emitted a call");
        assert_eq!(direct_builtin_shape_issue(&probe_module(), &chr, call, RuntimeFnId::Chr), None);

        let ord = scalar_conversion_call(
            RuntimeFnId::Ord,
            IrType::Str,
            PhpType::Str,
            IrType::I64,
            PhpType::Int,
        );
        let call = ord.instructions.last().expect("the probe emitted a call");
        assert_eq!(direct_builtin_shape_issue(&probe_module(), &ord, call, RuntimeFnId::Ord), None);

        // Each refuses the other's operand typing, and both refuse a juggled one.
        let swapped = scalar_conversion_call(
            RuntimeFnId::Chr,
            IrType::Str,
            PhpType::Str,
            IrType::Str,
            PhpType::Str,
        );
        let call = swapped.instructions.last().expect("the probe emitted a call");
        assert!(direct_builtin_shape_issue(&probe_module(), &swapped, call, RuntimeFnId::Chr).is_some());

        let juggled = scalar_conversion_call(
            RuntimeFnId::Ord,
            IrType::Heap(IrHeapKind::Mixed),
            PhpType::Mixed,
            IrType::I64,
            PhpType::Int,
        );
        let call = juggled.instructions.last().expect("the probe emitted a call");
        assert!(direct_builtin_shape_issue(&probe_module(), &juggled, call, RuntimeFnId::Ord).is_some());

        // A wrong RESULT typing is refused too: `chr` cannot answer an int.
        let bad_result = scalar_conversion_call(
            RuntimeFnId::Chr,
            IrType::I64,
            PhpType::Int,
            IrType::I64,
            PhpType::Int,
        );
        let call = bad_result.instructions.last().expect("the probe emitted a call");
        assert!(direct_builtin_shape_issue(&probe_module(), &bad_result, call, RuntimeFnId::Chr).is_some());
    }

    /// Verifies the PHP 8.5 deprecations are the ONLY thing the profile changes about chr/ord.
    ///
    /// PHP 8.5 diagnoses an out-of-range `chr` argument and an `ord` argument that is not exactly
    /// one byte, but both still ANSWER, and with the same value an earlier profile gives. So the
    /// two renderings must differ only by the diagnostic call: if the arithmetic differed, an
    /// earlier profile would silently compute something else.
    #[test]
    fn chr_and_ord_differ_between_profiles_only_by_the_deprecation() {
        let (diagnosing_chr, diagnosing_ord) = str_chr_ord(true);
        let (silent_chr, silent_ord) = str_chr_ord(false);

        assert!(diagnosing_chr.contains("call $__rt_deprecated_chr_range"));
        assert!(!silent_chr.contains("__rt_deprecated"));
        assert!(diagnosing_ord.contains("call $__rt_deprecated_ord_length"));
        assert!(!silent_ord.contains("__rt_deprecated"));

        // Strip the guard the diagnosing form adds; what remains must be identical.
        for (diagnosing, silent, marker) in [
            (&diagnosing_chr, &silent_chr, "__rt_deprecated_chr_range"),
            (&diagnosing_ord, &silent_ord, "__rt_deprecated_ord_length"),
        ] {
            let stripped: String = diagnosing
                .lines()
                .skip_while(|line| !line.contains("(if ("))
                .skip_while(|line| !line.contains(marker))
                .skip(1)
                .collect::<Vec<_>>()
                .join("\n");
            let expected: String = silent
                .lines()
                .skip_while(|line| !line.contains("(local.set $byte") && !line.contains("(if (i64.le_s"))
                .collect::<Vec<_>>()
                .join("\n");
            assert_eq!(
                stripped, expected,
                "the profiles must agree on the ANSWER, not just on the diagnostic"
            );
        }
    }

    /// Builds a `RuntimeCall` probe with an arbitrary operand list.
    /// `sort` and `rsort` order scalar slots in place; string and Mixed elements are refused,
    /// because PHP compares two numeric strings NUMERICALLY and that rule is not this helper's.
    #[test]
    fn scalar_sorts_admit_only_orderable_elements() {
        let array_of = |element: PhpType| {
            (
                IrType::Heap(IrHeapKind::Array),
                PhpType::Array(Box::new(element)),
            )
        };
        for target in [RuntimeFnId::Sort, RuntimeFnId::Rsort] {
            for element in [PhpType::Int, PhpType::Bool, PhpType::Float, PhpType::Never] {
                let probe = shaped_call(
                    target,
                    &[array_of(element.clone())],
                    IrType::I64,
                    PhpType::Void,
                );
                let call = probe.instructions.last().expect("the probe emitted a call");
                assert_eq!(
                    direct_builtin_shape_issue(&probe_module(), &probe, call, target),
                    None,
                    "{target:?} orders elements of {element:?}"
                );
            }
            // Strings order through the numeric-string classifier, which only a command
            // module carries — so the same call is admitted there and refused without it.
            let probe = shaped_call(
                target,
                &[array_of(PhpType::Str)],
                IrType::I64,
                PhpType::Void,
            );
            let call = probe.instructions.last().expect("the probe emitted a call");
            assert_eq!(
                direct_builtin_shape_issue(&command_probe_module(), &probe, call, target),
                None,
                "{target:?} orders strings in a command module"
            );
            assert!(
                direct_builtin_shape_issue(&probe_module(), &probe, call, target).is_some(),
                "{target:?} has no string ordering without a main"
            );
            for element in [PhpType::Mixed] {
                let probe = shaped_call(
                    target,
                    &[array_of(element.clone())],
                    IrType::I64,
                    PhpType::Void,
                );
                let call = probe.instructions.last().expect("the probe emitted a call");
                assert!(
                    direct_builtin_shape_issue(&probe_module(), &probe, call, target).is_some(),
                    "{target:?} has no measured ordering for {element:?}"
                );
            }
        }
    }

    /// `range` here is the two-bound integer form only: the front-end rejects every other arity
    /// with "range() takes exactly 2 arguments", so there is no step to validate and no float or
    /// string bound to widen.
    #[test]
    fn range_admits_only_two_integer_bounds() {
        let int_arg = (IrType::I64, PhpType::Int);
        let probe = |operands: Vec<(IrType, PhpType)>| {
            let function = shaped_call(
                RuntimeFnId::Range,
                &operands,
                IrType::Heap(IrHeapKind::Array),
                PhpType::Array(Box::new(PhpType::Int)),
            );
            let call = function
                .instructions
                .last()
                .expect("the probe emitted a call")
                .clone();
            direct_builtin_shape_issue(&probe_module(), &function, &call, RuntimeFnId::Range)
        };

        assert_eq!(probe(vec![int_arg.clone(), int_arg.clone()]), None);
        assert!(probe(vec![int_arg.clone()]).is_some(), "range needs both bounds");
        assert!(
            probe(vec![int_arg.clone(), int_arg.clone(), int_arg.clone()]).is_some(),
            "the step form does not reach this target"
        );
        assert!(
            probe(vec![(IrType::F64, PhpType::Float), int_arg.clone()]).is_some(),
            "a float bound produces float elements"
        );
        assert!(probe(vec![int_arg.clone(), (IrType::Str, PhpType::Str)]).is_some());
    }

    /// `array_merge` clones the left and appends the right, so both operands must agree on slot
    /// layout — which they do, because EIR widens each with `Op::ArrayToMixed` when they differ.
    /// An empty operand carries no elements, so its type never has to agree.
    #[test]
    fn array_merge_admits_only_agreeing_element_storage() {
        let array_of = |element: PhpType| {
            (
                IrType::Heap(IrHeapKind::Array),
                PhpType::Array(Box::new(element)),
            )
        };
        let probe = |left: PhpType, right: PhpType| {
            let function = shaped_call(
                RuntimeFnId::ArrayMerge,
                &[array_of(left.clone()), array_of(right)],
                IrType::Heap(IrHeapKind::Array),
                PhpType::Array(Box::new(left)),
            );
            let call = function
                .instructions
                .last()
                .expect("the probe emitted a call")
                .clone();
            direct_builtin_shape_issue(&probe_module(), &function, &call, RuntimeFnId::ArrayMerge)
        };

        for element in [
            PhpType::Int,
            PhpType::Str,
            PhpType::Float,
            PhpType::Bool,
            PhpType::Mixed,
        ] {
            assert_eq!(
                probe(element.clone(), element.clone()),
                None,
                "two arrays of {element:?} merge"
            );
            // `Never` is the empty literal's element type: nothing is read from that side.
            assert_eq!(probe(element.clone(), PhpType::Never), None);
            assert_eq!(probe(PhpType::Never, element.clone()), None);
        }

        assert!(
            probe(PhpType::Int, PhpType::Str).is_some(),
            "an int slot and a string slot are different widths; EIR widens before the call"
        );
        assert!(probe(PhpType::Mixed, PhpType::Float).is_some());
    }

    /// `array_slice` copies the window into a fresh `array<mixed>`, so it admits every element
    /// type this target has a slot layout for — including `Mixed`, whose cells it shares rather
    /// than copies. The `preserve_keys` form answers a hash and stays refused, as does a
    /// non-integer offset.
    #[test]
    fn array_slice_admits_its_lowered_windows_only() {
        let array_of = |element: PhpType| {
            (
                IrType::Heap(IrHeapKind::Array),
                PhpType::Array(Box::new(element)),
            )
        };
        let int_arg = (IrType::I64, PhpType::Int);

        for element in [
            PhpType::Int,
            PhpType::Str,
            PhpType::Float,
            PhpType::Bool,
            PhpType::Mixed,
            PhpType::Never,
        ] {
            for operands in [
                vec![array_of(element.clone()), int_arg.clone()],
                vec![array_of(element.clone()), int_arg.clone(), int_arg.clone()],
            ] {
                let probe = shaped_call(
                    RuntimeFnId::ArraySlice,
                    &operands,
                    IrType::Heap(IrHeapKind::Array),
                    PhpType::Array(Box::new(PhpType::Mixed)),
                );
                let call = probe.instructions.last().expect("the probe emitted a call");
                assert_eq!(
                    direct_builtin_shape_issue(
                        &probe_module(),
                        &probe,
                        call,
                        RuntimeFnId::ArraySlice
                    ),
                    None,
                    "an array of {element:?} sliced with {} bounds",
                    operands.len() - 1
                );
            }
        }

        // The four-operand `preserve_keys` form answers a hash, not a reindexed list.
        let preserve = shaped_call(
            RuntimeFnId::ArraySlice,
            &[
                array_of(PhpType::Int),
                int_arg.clone(),
                int_arg.clone(),
                (IrType::I64, PhpType::Bool),
            ],
            IrType::Heap(IrHeapKind::Array),
            PhpType::Array(Box::new(PhpType::Mixed)),
        );
        let call = preserve.instructions.last().expect("the probe emitted a call");
        assert!(
            direct_builtin_shape_issue(&probe_module(), &preserve, call, RuntimeFnId::ArraySlice)
                .is_some(),
            "preserve_keys is a different result type and is not lowered"
        );

        // A string offset is a coercion this lowerer does not perform.
        let string_offset = shaped_call(
            RuntimeFnId::ArraySlice,
            &[array_of(PhpType::Int), (IrType::Str, PhpType::Str)],
            IrType::Heap(IrHeapKind::Array),
            PhpType::Array(Box::new(PhpType::Mixed)),
        );
        let call = string_offset
            .instructions
            .last()
            .expect("the probe emitted a call");
        assert!(
            direct_builtin_shape_issue(
                &probe_module(),
                &string_offset,
                call,
                RuntimeFnId::ArraySlice
            )
            .is_some(),
            "array_slice takes an integer offset"
        );
    }

    fn shaped_call(
        target: RuntimeFnId,
        operands: &[(IrType, PhpType)],
        result_ir: IrType,
        result_php: PhpType,
    ) -> Function {
        let mut function = Function::new("probe".to_string(), IrType::Void, PhpType::Void);
        {
            let mut builder = Builder::new(&mut function);
            let entry = builder.create_named_block("entry", Vec::new());
            builder.set_entry(entry);
            builder.position_at_end(entry);
            let arguments: Vec<_> = operands
                .iter()
                .enumerate()
                .map(|(index, (ir, php))| {
                    let slot = builder.add_local(
                        Some(format!("a{index}")),
                        *ir,
                        php.clone(),
                        crate::ir::LocalKind::PhpLocal,
                    );
                    builder.emit_load_local(slot, *ir, php.clone())
                })
                .collect();
            builder.emit(
                Op::RuntimeCall,
                arguments,
                Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(target))),
                result_ir,
                result_php,
                Ownership::MaybeOwned,
            );
            builder.terminate(crate::ir::Terminator::Return { value: None });
        }
        function
    }

    /// Verifies the string-shaping builtins accept exactly the arities they lower.
    ///
    /// `trim` and `substr` each have an optional trailing argument that changes the RULE, not
    /// just a default — an absent charlist means PHP's built-in set rather than the empty one,
    /// and an absent length runs to the end rather than meaning zero — so both arities have to be
    /// admitted and a third refused. The rest take exactly what they take.
    #[test]
    fn string_shaping_builtins_admit_only_their_arities() {
        let str_arg = (IrType::Str, PhpType::Str);
        let int_arg = (IrType::I64, PhpType::Int);

        for target in [
            RuntimeFnId::Ucfirst,
            RuntimeFnId::Lcfirst,
            RuntimeFnId::Ucwords,
        ] {
            let ok = shaped_call(target, &[str_arg.clone()], IrType::Str, PhpType::Str);
            let call = ok.instructions.last().expect("the probe emitted a call");
            assert_eq!(direct_builtin_shape_issue(&probe_module(), &ok, call, target), None);

            let two = shaped_call(
                target,
                &[str_arg.clone(), str_arg.clone()],
                IrType::Str,
                PhpType::Str,
            );
            let call = two.instructions.last().expect("the probe emitted a call");
            assert!(
                direct_builtin_shape_issue(&probe_module(), &two, call, target).is_some(),
                "{target:?} takes one string"
            );
        }

        for target in [RuntimeFnId::Trim, RuntimeFnId::Ltrim, RuntimeFnId::Rtrim] {
            for arity in 1..=2 {
                let operands = vec![str_arg.clone(); arity];
                let ok = shaped_call(target, &operands, IrType::Str, PhpType::Str);
                let call = ok.instructions.last().expect("the probe emitted a call");
                assert_eq!(
                    direct_builtin_shape_issue(&probe_module(), &ok, call, target),
                    None,
                    "{target:?} accepts {arity} operand(s)"
                );
            }
            let three = shaped_call(target, &vec![str_arg.clone(); 3], IrType::Str, PhpType::Str);
            let call = three.instructions.last().expect("the probe emitted a call");
            assert!(direct_builtin_shape_issue(&probe_module(), &three, call, target).is_some());
        }

        for arity in 2..=3 {
            let mut operands = vec![str_arg.clone()];
            operands.extend(vec![int_arg.clone(); arity - 1]);
            let ok = shaped_call(RuntimeFnId::Substr, &operands, IrType::Str, PhpType::Str);
            let call = ok.instructions.last().expect("the probe emitted a call");
            assert_eq!(
                direct_builtin_shape_issue(&probe_module(), &ok, call, RuntimeFnId::Substr),
                None,
                "substr accepts {arity} operands"
            );
        }
        // A string where the offset belongs is refused rather than coerced.
        let bad_offset = shaped_call(
            RuntimeFnId::Substr,
            &[str_arg.clone(), str_arg.clone()],
            IrType::Str,
            PhpType::Str,
        );
        let call = bad_offset.instructions.last().expect("the probe emitted a call");
        assert!(direct_builtin_shape_issue(&probe_module(), &bad_offset, call, RuntimeFnId::Substr).is_some());

        // `str_repeat` takes a subject and a count, and refuses a string where the count goes.
        let ok = shaped_call(
            RuntimeFnId::StrRepeat,
            &[str_arg.clone(), int_arg.clone()],
            IrType::Str,
            PhpType::Str,
        );
        let call = ok.instructions.last().expect("the probe emitted a call");
        assert_eq!(
            direct_builtin_shape_issue(&probe_module(), &ok, call, RuntimeFnId::StrRepeat),
            None
        );
        let bad_count = shaped_call(
            RuntimeFnId::StrRepeat,
            &[str_arg.clone(), str_arg.clone()],
            IrType::Str,
            PhpType::Str,
        );
        let call = bad_count.instructions.last().expect("the probe emitted a call");
        assert!(direct_builtin_shape_issue(&probe_module(), &bad_count, call, RuntimeFnId::StrRepeat).is_some());

        // `strpos` answers PHP's `int|false`, which only a runtime-tagged Mixed cell can carry.
        let searched = shaped_call(
            RuntimeFnId::Strpos,
            &[str_arg.clone(), str_arg.clone()],
            IrType::Heap(IrHeapKind::Mixed),
            PhpType::Mixed,
        );
        let call = searched.instructions.last().expect("the probe emitted a call");
        assert_eq!(
            direct_builtin_shape_issue(&probe_module(), &searched, call, RuntimeFnId::Strpos),
            None
        );
        // An Int result would lose the difference between a match at offset 0 and a miss.
        let as_int = shaped_call(
            RuntimeFnId::Strpos,
            &[str_arg.clone(), str_arg.clone()],
            IrType::I64,
            PhpType::Int,
        );
        let call = as_int.instructions.last().expect("the probe emitted a call");
        assert!(direct_builtin_shape_issue(&probe_module(), &as_int, call, RuntimeFnId::Strpos).is_some());
        // `str_replace` takes three strings; an array form is a different operation.
        let ok = shaped_call(
            RuntimeFnId::StrReplace,
            &[str_arg.clone(), str_arg.clone(), str_arg.clone()],
            IrType::Str,
            PhpType::Str,
        );
        let call = ok.instructions.last().expect("the probe emitted a call");
        assert_eq!(
            direct_builtin_shape_issue(&probe_module(), &ok, call, RuntimeFnId::StrReplace),
            None
        );
        let with_count = shaped_call(
            RuntimeFnId::StrReplace,
            &[str_arg.clone(), str_arg.clone(), str_arg.clone(), int_arg.clone()],
            IrType::Str,
            PhpType::Str,
        );
        let call = with_count.instructions.last().expect("the probe emitted a call");
        assert!(direct_builtin_shape_issue(&probe_module(), &with_count, call, RuntimeFnId::StrReplace).is_some());

        // `sha1` answers a 40-character hex string from exactly one string.
        let digest = shaped_call(
            RuntimeFnId::Sha1,
            &[str_arg.clone()],
            IrType::Str,
            PhpType::Str,
        );
        let call = digest.instructions.last().expect("the probe emitted a call");
        assert_eq!(direct_builtin_shape_issue(&probe_module(), &digest, call, RuntimeFnId::Sha1), None);
        // The digest is fixed-width, and every SHA-1 word is big-endian.
        assert!(RT_SHA1_HEX.contains("(local.get $out) (i64.const 40)"));
        assert!(
            RT_SHA1_HEX.contains("(i32.const 0x5A827999)")
                && RT_SHA1_HEX.contains("(i32.const 0xCA62C1D6)"),
            "the four round constants must all be present"
        );

        // `htmlspecialchars` escapes BOTH quote styles under PHP 8.1+ defaults.
        let escaped = shaped_call(
            RuntimeFnId::Htmlspecialchars,
            &[str_arg.clone()],
            IrType::Str,
            PhpType::Str,
        );
        let call = escaped.instructions.last().expect("the probe emitted a call");
        assert_eq!(
            direct_builtin_shape_issue(&probe_module(), &escaped, call, RuntimeFnId::Htmlspecialchars),
            None
        );
        // `&#039;` is the ENT_QUOTES default since 8.1; leaving `'` alone would be the 8.0 rule.
        assert!(RT_HTML_PUT.contains("(i32.const 35)"), "the numeric entity for an apostrophe");
        // The bad-span rule stops at a byte that could START a sequence, not at a non-continuation:
        // a plain continuation test gets 102 byte pairs wrong.
        assert!(
            RT_UTF8_BAD_SPAN.contains("(i32.ge_u (local.get $next) (i32.const 0xC2))")
                && RT_UTF8_BAD_SPAN.contains("(i32.le_u (local.get $next) (i32.const 0xF4))"),
            "a valid lead terminates the bad span; a never-valid byte is absorbed"
        );
        // Surrogates and overlong forms are rejected, not just structurally short sequences.
        assert!(RT_UTF8_SEQ_LEN.contains("(i32.const 0xED)"), "surrogate exclusion");
        assert!(RT_UTF8_SEQ_LEN.contains("(i32.const 0xC2)"), "C0/C1 are overlong");

        // `md5` answers a 32-character digest, half sha1's width.
        let md5 = shaped_call(
            RuntimeFnId::Md5,
            &[str_arg.clone()],
            IrType::Str,
            PhpType::Str,
        );
        let call = md5.instructions.last().expect("the probe emitted a call");
        assert_eq!(direct_builtin_shape_issue(&probe_module(), &md5, call, RuntimeFnId::Md5), None);
        assert!(RT_MD5_HEX.contains("(local.get $out) (i64.const 32)"));
        // The K table is what the algorithm cannot compute: WebAssembly has no `sin`.
        assert!(RT_MD5_K.contains("(i32.const 0xd76aa478)"), "K[0]");
        assert!(RT_MD5_K.contains("(i32.const 0xeb86d391)"), "K[63]");
        assert_eq!(
            RT_MD5_K.matches("(i32.eq (local.get $i) (i32.const").count(),
            64,
            "all 64 round constants must be present"
        );
        assert_eq!(
            RT_MD5_S.matches("(i32.eq (local.get $i) (i32.const").count(),
            64,
            "all 64 rotation amounts must be present"
        );

        // `crc32` answers an int, and PHP's is the UNSIGNED 32-bit value.
        let hashed = shaped_call(
            RuntimeFnId::Crc32,
            &[str_arg.clone()],
            IrType::I64,
            PhpType::Int,
        );
        let call = hashed.instructions.last().expect("the probe emitted a call");
        assert_eq!(direct_builtin_shape_issue(&probe_module(), &hashed, call, RuntimeFnId::Crc32), None);
        assert!(
            RT_CRC32.contains("(i64.const 0xFFFFFFFF)"),
            "the result is masked to 32 bits, not sign-extended"
        );
        assert!(
            RT_CRC32.contains("(i32.const 0xEDB88320)"),
            "PHP uses the reflected IEEE 802.3 polynomial"
        );

        // `strstr` answers `string|false` through the same tagged cell, in BOTH arities.
        for arity in 2..=3 {
            let mut operands = vec![str_arg.clone(), str_arg.clone()];
            if arity == 3 {
                operands.push((IrType::I64, PhpType::Bool));
            }
            let ok = shaped_call(
                RuntimeFnId::Strstr,
                &operands,
                IrType::Heap(IrHeapKind::Mixed),
                PhpType::Mixed,
            );
            let call = ok.instructions.last().expect("the probe emitted a call");
            assert_eq!(
                direct_builtin_shape_issue(&probe_module(), &ok, call, RuntimeFnId::Strstr),
                None,
                "strstr accepts {arity} operands"
            );
        }
        // The flag is a bool, not an int offset, and the result is never a bare string.
        let bad_flag = shaped_call(
            RuntimeFnId::Strstr,
            &[str_arg.clone(), str_arg.clone(), int_arg.clone()],
            IrType::Heap(IrHeapKind::Mixed),
            PhpType::Mixed,
        );
        let call = bad_flag.instructions.last().expect("the probe emitted a call");
        assert!(direct_builtin_shape_issue(&probe_module(), &bad_flag, call, RuntimeFnId::Strstr).is_some());

        // `wordwrap` takes an optional width; a custom break or cut flag is a different path.
        for arity in 1..=2 {
            let mut operands = vec![str_arg.clone()];
            if arity == 2 {
                operands.push(int_arg.clone());
            }
            let wrapped = shaped_call(RuntimeFnId::Wordwrap, &operands, IrType::Str, PhpType::Str);
            let call = wrapped.instructions.last().expect("the probe emitted a call");
            assert_eq!(
                direct_builtin_shape_issue(&probe_module(), &wrapped, call, RuntimeFnId::Wordwrap),
                None,
                "wordwrap accepts {arity} operand(s)"
            );
        }
        // The break-string and cut forms are lowered too, through the building helper rather
        // than the in-place one.
        for operands in [
            vec![str_arg.clone(), int_arg.clone(), str_arg.clone()],
            vec![
                str_arg.clone(),
                int_arg.clone(),
                str_arg.clone(),
                (IrType::I64, PhpType::Bool),
            ],
        ] {
            let wrapped = shaped_call(RuntimeFnId::Wordwrap, &operands, IrType::Str, PhpType::Str);
            let call = wrapped.instructions.last().expect("the probe emitted a call");
            assert_eq!(
                direct_builtin_shape_issue(&probe_module(), &wrapped, call, RuntimeFnId::Wordwrap),
                None,
                "wordwrap accepts {} operands",
                operands.len()
            );
        }
        // A break that is not a string, and a cut flag that is not a bool, still refuse.
        for operands in [
            vec![str_arg.clone(), int_arg.clone(), int_arg.clone()],
            vec![
                str_arg.clone(),
                int_arg.clone(),
                str_arg.clone(),
                str_arg.clone(),
            ],
        ] {
            let wrapped = shaped_call(RuntimeFnId::Wordwrap, &operands, IrType::Str, PhpType::Str);
            let call = wrapped.instructions.last().expect("the probe emitted a call");
            assert!(
                direct_builtin_shape_issue(&probe_module(), &wrapped, call, RuntimeFnId::Wordwrap)
                    .is_some(),
                "wordwrap checks its break and cut storage"
            );
        }
        // The fast path never changes the length: it replaces a space, it does not insert.
        assert!(
            RT_WORDWRAP.contains("(local.get $out) (local.get $olen)"),
            "wordwrap returns the persisted length unchanged"
        );

        // `str_split` accepts both arities, since the default really is a chunk of one.
        for arity in 1..=2 {
            let mut operands = vec![str_arg.clone()];
            if arity == 2 {
                operands.push(int_arg.clone());
            }
            let cut = shaped_call(
                RuntimeFnId::StrSplit,
                &operands,
                IrType::Heap(IrHeapKind::Array),
                PhpType::Array(Box::new(PhpType::Str)),
            );
            let call = cut.instructions.last().expect("the probe emitted a call");
            assert_eq!(
                direct_builtin_shape_issue(&probe_module(), &cut, call, RuntimeFnId::StrSplit),
                None,
                "str_split accepts {arity} operand(s)"
            );
        }

        // `explode` produces exactly array<string>; a limit argument is a different contract.
        let split = shaped_call(
            RuntimeFnId::Explode,
            &[str_arg.clone(), str_arg.clone()],
            IrType::Heap(IrHeapKind::Array),
            PhpType::Array(Box::new(PhpType::Str)),
        );
        let call = split.instructions.last().expect("the probe emitted a call");
        assert_eq!(direct_builtin_shape_issue(&probe_module(), &split, call, RuntimeFnId::Explode), None);
        let with_limit = shaped_call(
            RuntimeFnId::Explode,
            &[str_arg.clone(), str_arg.clone(), int_arg.clone()],
            IrType::Heap(IrHeapKind::Array),
            PhpType::Array(Box::new(PhpType::Str)),
        );
        let call = with_limit.instructions.last().expect("the probe emitted a call");
        assert!(direct_builtin_shape_issue(&probe_module(), &with_limit, call, RuntimeFnId::Explode).is_some());

        // `implode` reads an array, so its element type is part of the contract.
        let str_array = (IrType::Heap(IrHeapKind::Array), PhpType::Array(Box::new(PhpType::Str)));
        let joined = shaped_call(
            RuntimeFnId::Implode,
            &[str_arg.clone(), str_array.clone()],
            IrType::Str,
            PhpType::Str,
        );
        let call = joined.instructions.last().expect("the probe emitted a call");
        assert_eq!(direct_builtin_shape_issue(&probe_module(), &joined, call, RuntimeFnId::Implode), None);
        // A provably empty array is fine: the element read never happens.
        let empty = shaped_call(
            RuntimeFnId::Implode,
            &[
                str_arg.clone(),
                (IrType::Heap(IrHeapKind::Array), PhpType::Array(Box::new(PhpType::Never))),
            ],
            IrType::Str,
            PhpType::Str,
        );
        let call = empty.instructions.last().expect("the probe emitted a call");
        assert_eq!(direct_builtin_shape_issue(&probe_module(), &empty, call, RuntimeFnId::Implode), None);
        // Int, float and Mixed elements are CONVERTED — PHP applies the same rule as an
        // explicit `(string)` cast, so they go through the owning helper rather than being
        // refused.
        for element in [PhpType::Int, PhpType::Float, PhpType::Mixed] {
            let converted = shaped_call(
                RuntimeFnId::Implode,
                &[
                    str_arg.clone(),
                    (IrType::Heap(IrHeapKind::Array), PhpType::Array(Box::new(element.clone()))),
                ],
                IrType::Str,
                PhpType::Str,
            );
            let call = converted.instructions.last().expect("the probe emitted a call");
            assert_eq!(
                direct_builtin_shape_issue(&probe_module(), &converted, call, RuntimeFnId::Implode),
                None,
                "an array of {element:?} converts element by element"
            );
        }
        // A bool slot has neither a (pointer, length) layout nor a conversion here.
        for element in [PhpType::Bool] {
            let wrong = shaped_call(
                RuntimeFnId::Implode,
                &[
                    str_arg.clone(),
                    (IrType::Heap(IrHeapKind::Array), PhpType::Array(Box::new(element.clone()))),
                ],
                IrType::Str,
                PhpType::Str,
            );
            let call = wrong.instructions.last().expect("the probe emitted a call");
            assert!(
                direct_builtin_shape_issue(&probe_module(), &wrong, call, RuntimeFnId::Implode).is_some(),
                "an array of {element:?} has no lowered element conversion"
            );
        }

        // `strrpos` answers the same tagged cell, and scans from the right.
        let last = shaped_call(
            RuntimeFnId::Strrpos,
            &[str_arg.clone(), str_arg.clone()],
            IrType::Heap(IrHeapKind::Mixed),
            PhpType::Mixed,
        );
        let call = last.instructions.last().expect("the probe emitted a call");
        assert_eq!(direct_builtin_shape_issue(&probe_module(), &last, call, RuntimeFnId::Strrpos), None);
        assert!(
            RT_STR_RFIND.contains("(local.set $at (i64.sub (local.get $hlen) (local.get $nlen)))"),
            "the scan starts at the rightmost offset that still fits"
        );

        // Only the two-argument form is lowered; the offset form has its own ValueError contract.
        let with_offset = shaped_call(
            RuntimeFnId::Strpos,
            &[str_arg.clone(), str_arg.clone(), int_arg.clone()],
            IrType::Heap(IrHeapKind::Mixed),
            PhpType::Mixed,
        );
        let call = with_offset.instructions.last().expect("the probe emitted a call");
        assert!(direct_builtin_shape_issue(&probe_module(), &with_offset, call, RuntimeFnId::Strpos).is_some());

        for target in [RuntimeFnId::Strcmp, RuntimeFnId::Strcasecmp] {
            let ok = shaped_call(
                target,
                &[str_arg.clone(), str_arg.clone()],
                IrType::I64,
                PhpType::Int,
            );
            let call = ok.instructions.last().expect("the probe emitted a call");
            assert_eq!(direct_builtin_shape_issue(&probe_module(), &ok, call, target), None);

            // A comparison answers an int, never a bool: PHP's result is a byte distance.
            let as_bool = shaped_call(
                target,
                &[str_arg.clone(), str_arg.clone()],
                IrType::I64,
                PhpType::Bool,
            );
            let call = as_bool.instructions.last().expect("the probe emitted a call");
            assert!(direct_builtin_shape_issue(&probe_module(), &as_bool, call, target).is_some());
        }
    }

    /// Verifies `strcmp` reports a byte DISTANCE but normalizes a pure length difference.
    ///
    /// These are two different rules and php-src applies both: `strcmp("ABC", "abc")` is -32
    /// because `A` and `a` are 32 apart, while `strcmp("abcd", "a")` is 1 rather than 3 because
    /// nothing mismatched — only the lengths differ. A helper that returned the length delta, or
    /// that clamped the byte distance to a sign, would pass a naive test and fail php-src.
    #[test]
    fn strcmp_returns_a_byte_distance_but_a_normalized_length_difference() {
        assert!(
            RT_STR_CMP.contains("(return (i64.extend_i32_s (i32.sub (local.get $x) (local.get $y))))"),
            "a mismatched byte yields its raw distance, not a sign"
        );
        for normalized in ["(then (return (i64.const -1)))", "(then (return (i64.const 1)))"] {
            assert!(
                RT_STR_CMP.contains(normalized),
                "a pure length difference is normalized to +/-1"
            );
        }
        // The bytes are read UNSIGNED, which is what makes strcmp("\xff", "\x01") 254 and not -2.
        assert!(!RT_STR_CMP.contains("i32.load8_s"));
    }

    /// Verifies every builtin that RAISES is also declared as raising.
    ///
    /// `function::raises_runtime_error` is what makes the module lay out an error's message, and
    /// `emit_runtime_failure` silently falls back to the deterministic fatal when the message is
    /// absent. So a builtin added to the catchable catalogue but not to that predicate compiles,
    /// runs, and reports the right text — while no `catch` ever receives it. That failure is
    /// invisible unless the test catches the error rather than just reading stderr, which is
    /// exactly how it was found.
    #[test]
    fn builtins_that_raise_are_declared_as_raising() {
        let str_arg = (IrType::Str, PhpType::Str);
        let int_arg = (IrType::I64, PhpType::Int);
        for (target, operands) in [
            (RuntimeFnId::StrRepeat, vec![str_arg.clone(), int_arg.clone()]),
            (RuntimeFnId::Explode, vec![str_arg.clone(), str_arg.clone()]),
            (RuntimeFnId::StrSplit, vec![str_arg.clone(), int_arg.clone()]),
            (
                RuntimeFnId::StrPad,
                vec![str_arg.clone(), int_arg.clone(), str_arg.clone()],
            ),
        ] {
            let probe = shaped_call(target, &operands, IrType::Str, PhpType::Str);
            assert!(
                crate::codegen_wasm::function::raises_runtime_error(&probe),
                "{target:?} raises a PHP error, so its message must be laid out"
            );
        }
        // A builtin with no failure mode must NOT drag the error messages into every module.
        let quiet = shaped_call(
            RuntimeFnId::Ucfirst,
            &[(IrType::Str, PhpType::Str)],
            IrType::Str,
            PhpType::Str,
        );
        assert!(!crate::codegen_wasm::function::raises_runtime_error(&quiet));
    }

    /// An empty module for shape checks that do not depend on module data.
    ///
    /// Only `sprintf` reads the module — it recovers its literal format from the interned
    /// strings — and no probe here builds one, so an empty module is the honest stand-in.
    fn probe_module() -> Module {
        Module::new(crate::codegen_support::platform::Target::wasm())
    }

    /// A module carrying a `main`, for the shapes whose runtime is command-only.
    fn command_probe_module() -> Module {
        let mut module = probe_module();
        let mut main = Function::new("main".to_string(), IrType::Void, PhpType::Void);
        main.flags.is_main = true;
        module.add_function(main);
        module
    }

    /// Verifies the format parser applies php-src's flag rules, not C's.
    ///
    /// Every expectation here was measured against php-src 8.5.6 before the parser was written,
    /// and each is a place C and PHP disagree — so a parser transcribed from the C manual would
    /// pass a naive test and fail these.
    #[test]
    fn sprintf_format_parser_follows_php_flag_rules() {
        use FormatPiece::{Conversion, Literal};
        let conv = |p: &FormatPiece| match p {
            Conversion { left, plus, pad, width, precision, argument, conversion } => {
                (*left, *plus, *pad, *width, *precision, *argument, *conversion)
            }
            other => panic!("expected a conversion, got {other:?}"),
        };

        // The LAST padding flag wins: `%'x03d` pads with zeros, `%0'x3d` pads with x.
        assert_eq!(conv(&parse_sprintf_format(b"%'x03d", 1).unwrap()[0]).2, b'0');
        assert_eq!(conv(&parse_sprintf_format(b"%0'x3d", 1).unwrap()[0]).2, b'x');

        // `-` cancels a ZERO pad on the numeric conversions but NOT on %s, and never cancels
        // an explicit `'X`. Measured: `%-08d` is spaces, `%-03s` is zeros, `%'x-3d` is x.
        assert_eq!(conv(&parse_sprintf_format(b"%-08d", 1).unwrap()[0]).2, b' ');
        assert_eq!(conv(&parse_sprintf_format(b"%-03s", 1).unwrap()[0]).2, b'0');
        assert_eq!(conv(&parse_sprintf_format(b"%'x-3d", 1).unwrap()[0]).2, b'x');

        // `%%` becomes a one-byte run pointing at the FIRST `%`, so every run stays a
        // contiguous slice of the format and needs no segment of its own.
        assert_eq!(
            parse_sprintf_format(b"a%%b", 0).unwrap(),
            vec![
                Literal { offset: 0, length: 1 },
                Literal { offset: 1, length: 1 },
                Literal { offset: 3, length: 1 },
            ]
        );
        assert_eq!(
            parse_sprintf_format(b"%%", 0).unwrap(),
            vec![Literal { offset: 0, length: 1 }]
        );

        // An explicit argument number is zero-based here and may repeat.
        let repeated = parse_sprintf_format(b"%1$s%1$s", 1).unwrap();
        assert_eq!(conv(&repeated[0]).5, 0);
        assert_eq!(conv(&repeated[1]).5, 0);
        // Positional arguments advance independently of explicit ones.
        let mixed = parse_sprintf_format(b"%s%d", 2).unwrap();
        assert_eq!(conv(&mixed[0]).5, 0);
        assert_eq!(conv(&mixed[1]).5, 1);

        // Width and precision, including an empty precision meaning zero.
        let sized = conv(&parse_sprintf_format(b"%8.2f", 1).unwrap()[0]);
        assert_eq!((sized.3, sized.4), (8, Some(2)));
        assert_eq!(conv(&parse_sprintf_format(b"%.s", 1).unwrap()[0]).4, Some(0));

        // `%f` accepts the same flags; `-` with `0` is the one combination refused.
        assert!(parse_sprintf_format(b"%08.2f", 1).is_ok());
        assert_eq!(conv(&parse_sprintf_format(b"%.3f", 1).unwrap()[0]).4, Some(3));
        // An absent precision on %f is SIX, which the lowering supplies, not zero.
        assert_eq!(conv(&parse_sprintf_format(b"%f", 1).unwrap()[0]).4, None);

        // The radix conversions read the argument as UNSIGNED and carry no sign.
        for radix in [&b"%x"[..], b"%X", b"%b", b"%o"] {
            assert!(
                parse_sprintf_format(radix, 1).is_ok(),
                "{} is lowered",
                String::from_utf8_lossy(radix)
            );
        }

        // Refused rather than guessed.
        for (format, why) in [
            (&b"%e"[..], "a conversion outside the subset"),
            (&b"%-08.2f"[..], "php-src loses the precision for '-' with '0' on %f"),
            (&b"%"[..], "a lone trailing percent"),
            (&b"%d"[..], "more conversions than arguments"),
        ] {
            let count = if format == b"%d" { 0 } else { 1 };
            assert!(
                parse_sprintf_format(format, count).is_err(),
                "{why} must be refused: {}",
                String::from_utf8_lossy(format)
            );
        }
    }

    /// Verifies the two url codecs differ exactly where php-src says they do.
    ///
    /// `urlencode` and `rawurlencode` share one helper and are told apart by a flag, so the
    /// difference is one branch rather than two implementations. Those two branches are the whole
    /// contract between them — a space folding to `+`, and `~` counting as unreserved — and this
    /// reads both back out of the emitted helper rather than trusting the call sites.
    #[test]
    fn url_codecs_differ_only_in_space_and_tilde() {
        assert!(
            RT_STR_URL_ENCODE.contains("(i32.eq (local.get $byte) (i32.const 32))"),
            "urlencode folds a space to a plus"
        );
        assert!(
            RT_STR_URL_ENCODE.contains("(i32.eq (local.get $byte) (i32.const 126))"),
            "rawurlencode alone leaves a tilde unreserved"
        );
        assert!(
            RT_STR_URL_DECODE.contains("(i32.eq (local.get $byte) (i32.const 43))"),
            "urldecode alone reads a plus as a space"
        );
        // Percent-encoding is uppercase: 55 is 'A' - 10, where a lowercase table would use 87.
        assert!(RT_STR_URL_ENCODE.contains("(i32.const 55)"));
        assert!(!RT_STR_URL_ENCODE.contains("(i32.const 87)"));
        // ...and hex DECODING accepts both cases, so it needs the lowercase offset as well.
        assert!(RT_HEX_DIGIT_VALUE.contains("(i32.const 55)"));
        assert!(RT_HEX_DIGIT_VALUE.contains("(i32.const 87)"));
    }

    /// Verifies every lowered unary string transform reserves enough room for its own output.
    ///
    /// A re-encoder allocates a worst case up front and reports the length it actually wrote, so
    /// a reservation smaller than the expansion would corrupt the heap block after it rather
    /// than fail visibly. Each factor is read back out of the emitted helper so a helper whose
    /// escaping grows without its reservation growing cannot pass.
    #[test]
    fn re_encoding_helpers_reserve_their_worst_case_expansion() {
        for (helper, factor, transform) in [
            (RT_STR_BIN2HEX, 2, "bin2hex writes two hex digits per byte"),
            (
                RT_STR_ADDSLASHES,
                2,
                "addslashes writes a backslash before an escaped byte",
            ),
            (
                RT_STR_NL2BR,
                7,
                "nl2br writes a six-byte tag before a kept break",
            ),
        ] {
            assert!(
                helper.contains(&format!(
                    "(call $__rt_str_alloc (i64.mul (local.get $len) (i64.const {factor})))"
                )),
                "{transform}, so it must reserve {factor} bytes per input byte:\n{helper}"
            );
        }
        // stripslashes only ever removes bytes, so the source length is already its worst case.
        assert!(RT_STR_STRIPSLASHES.contains("(call $__rt_str_alloc (local.get $len))"));
        for helper in [
            RT_STR_BIN2HEX,
            RT_STR_ADDSLASHES,
            RT_STR_STRIPSLASHES,
            RT_STR_NL2BR,
        ] {
            assert!(
                helper.contains("(i64.extend_i32_u (local.get $w))"),
                "a re-encoder returns what it WROTE, not what it reserved:\n{helper}"
            );
        }
    }

    /// Verifies `in_array` admits every (needle, element) pair whose rule was MEASURED, in both
    /// the loose and the strict form, and refuses the rest.
    ///
    /// The loose form is no longer an identity scan: it reuses the same comparison `==` lowers, so
    /// `in_array("1e1", ["10"])` is true loosely and false strictly. What is still refused is the
    /// pairs PHP's table handles differently — a string against a number, a bool against a number,
    /// and anything boxed — because guessing there answers the wrong question silently.
    #[test]
    fn in_array_admits_only_the_pairs_whose_rule_was_measured() {
        assert_eq!(verdict(&in_array_call(true), RuntimeFnId::InArray), None);
        assert_eq!(
            verdict(&in_array_call(false), RuntimeFnId::InArray),
            None,
            "the loose form now goes through the measured comparison"
        );

        for (needle, element) in [
            (PhpType::Int, PhpType::Int),
            (PhpType::Bool, PhpType::Bool),
            (PhpType::Float, PhpType::Float),
            (PhpType::Str, PhpType::Str),
            (PhpType::Int, PhpType::Float),
            (PhpType::Float, PhpType::Int),
            // An empty haystack never matches, so any needle is safe.
            (PhpType::Str, PhpType::Never),
            (PhpType::Float, PhpType::Never),
        ] {
            assert!(
                in_array_scan(&needle, &element).is_some(),
                "{needle:?} against elements of {element:?} was measured"
            );
        }
        for (needle, element) in [
            (PhpType::Str, PhpType::Int),
            (PhpType::Int, PhpType::Str),
            (PhpType::Bool, PhpType::Int),
            (PhpType::Int, PhpType::Bool),
            (PhpType::Mixed, PhpType::Int),
            (PhpType::Int, PhpType::Mixed),
        ] {
            assert!(
                in_array_scan(&needle, &element).is_none(),
                "{needle:?} against elements of {element:?} still needs its measured table"
            );
        }
    }

    /// Verifies each inline builtin admits exactly the storage its lowering can emit.
    ///
    /// `RuntimeFnId::Floor`, `RuntimeFnId::Ceil` and `RuntimeFnId::Sqrt` are float-only;
    /// `RuntimeFnId::Abs` accepts both widths and must keep an integral argument integral; and
    /// `RuntimeFnId::Count` reads a container header, so a scalar operand has to be refused
    /// rather than loading whatever lies at that address.
    #[test]
    fn direct_builtins_admit_only_the_storage_they_lower() {
        for target in [RuntimeFnId::Floor, RuntimeFnId::Ceil, RuntimeFnId::Sqrt] {
            let ok = call_with(target, IrType::F64, PhpType::Float, IrType::F64, PhpType::Float);
            assert_eq!(verdict(&ok, target), None, "{target:?} over a float");

            let bad = call_with(target, IrType::I64, PhpType::Int, IrType::I64, PhpType::Int);
            assert!(
                verdict(&bad, target).is_some(),
                "{target:?} has no integral lowering"
            );
        }

        let int_abs = call_with(
            RuntimeFnId::Abs,
            IrType::I64,
            PhpType::Int,
            IrType::I64,
            PhpType::Int,
        );
        assert_eq!(verdict(&int_abs, RuntimeFnId::Abs), None);
        let float_abs = call_with(
            RuntimeFnId::Abs,
            IrType::F64,
            PhpType::Float,
            IrType::F64,
            PhpType::Float,
        );
        assert_eq!(verdict(&float_abs, RuntimeFnId::Abs), None);
        let widened_abs = call_with(
            RuntimeFnId::Abs,
            IrType::I64,
            PhpType::Int,
            IrType::F64,
            PhpType::Float,
        );
        assert!(
            verdict(&widened_abs, RuntimeFnId::Abs).is_some(),
            "an integral argument must not claim a float result"
        );

        let counted = call_with(
            RuntimeFnId::Count,
            IrType::Heap(IrHeapKind::Array),
            PhpType::Array(Box::new(PhpType::Int)),
            IrType::I64,
            PhpType::Int,
        );
        assert_eq!(verdict(&counted, RuntimeFnId::Count), None);
        let listed = call_with(
            RuntimeFnId::ArrayIsList,
            IrType::Heap(IrHeapKind::Array),
            PhpType::Array(Box::new(PhpType::Int)),
            IrType::I64,
            PhpType::Bool,
        );
        assert_eq!(verdict(&listed, RuntimeFnId::ArrayIsList), None);
        let hashed = call_with(
            RuntimeFnId::ArrayIsList,
            IrType::Heap(IrHeapKind::Hash),
            PhpType::AssocArray {
                key: Box::new(PhpType::Str),
                value: Box::new(PhpType::Int),
            },
            IrType::I64,
            PhpType::Bool,
        );
        assert!(
            verdict(&hashed, RuntimeFnId::ArrayIsList).is_some(),
            "a hash needs a real scan, not an answer from the representation"
        );

        // The array family reads raw i64 slots, so it accepts `array<int>` and the empty
        // `array<never>`, and nothing else.
        for target in [
            RuntimeFnId::ArrayKeys,
            RuntimeFnId::ArrayValues,
            RuntimeFnId::ArrayReverse,
        ] {
            let ok = call_with(
                target,
                IrType::Heap(IrHeapKind::Array),
                PhpType::Array(Box::new(PhpType::Int)),
                IrType::Heap(IrHeapKind::Array),
                PhpType::Array(Box::new(PhpType::Int)),
            );
            assert_eq!(verdict(&ok, target), None, "{target:?} over array<int>");

            let stringly = call_with(
                target,
                IrType::Heap(IrHeapKind::Array),
                PhpType::Array(Box::new(PhpType::Str)),
                IrType::Heap(IrHeapKind::Array),
                PhpType::Array(Box::new(PhpType::Str)),
            );
            assert!(
                verdict(&stringly, target).is_some(),
                "{target:?} must not read string slots at the integer stride"
            );
        }

        // `RuntimeFnId::ArraySum` and `RuntimeFnId::ArrayProduct` fold to an int, so they take
        // an array<int> and answer I64/Int.
        for target in [RuntimeFnId::ArraySum, RuntimeFnId::ArrayProduct] {
            let ok = call_with(
                target,
                IrType::Heap(IrHeapKind::Array),
                PhpType::Array(Box::new(PhpType::Int)),
                IrType::I64,
                PhpType::Int,
            );
            assert_eq!(verdict(&ok, target), None, "{target:?} over array<int>");

            let floaty = call_with(
                target,
                IrType::Heap(IrHeapKind::Array),
                PhpType::Array(Box::new(PhpType::Float)),
                IrType::F64,
                PhpType::Float,
            );
            assert!(
                verdict(&floaty, target).is_some(),
                "{target:?} folds integer slots only"
            );
        }

        // `RuntimeFnId::Max`, `RuntimeFnId::Min` and `RuntimeFnId::Intdiv` take two ints; the
        // variadic and cross-type forms of max/min are not served.
        for target in [RuntimeFnId::Max, RuntimeFnId::Min, RuntimeFnId::Intdiv] {
            let single = call_with(
                target,
                IrType::I64,
                PhpType::Int,
                IrType::I64,
                PhpType::Int,
            );
            assert!(
                verdict(&single, target).is_some(),
                "{target:?} needs exactly two operands"
            );
        }

        // `RuntimeFnId::ArrayFill` needs its start index, count and value; one operand is not it.
        let short_fill = call_with(
            RuntimeFnId::ArrayFill,
            IrType::I64,
            PhpType::Int,
            IrType::Heap(IrHeapKind::Array),
            PhpType::Array(Box::new(PhpType::Int)),
        );
        assert!(
            verdict(&short_fill, RuntimeFnId::ArrayFill).is_some(),
            "array_fill takes three operands"
        );

        // `RuntimeFnId::StrContains`, `RuntimeFnId::StrStartsWith` and `RuntimeFnId::StrEndsWith`
        // take two strings; a scalar operand has no bytes to compare.
        for target in [
            RuntimeFnId::StrContains,
            RuntimeFnId::StrStartsWith,
            RuntimeFnId::StrEndsWith,
        ] {
            let scalar = call_with(target, IrType::I64, PhpType::Int, IrType::I64, PhpType::Bool);
            assert!(
                verdict(&scalar, target).is_some(),
                "{target:?} compares string bytes"
            );
        }

        let scalar_count = call_with(
            RuntimeFnId::Count,
            IrType::I64,
            PhpType::Int,
            IrType::I64,
            PhpType::Int,
        );
        assert!(
            verdict(&scalar_count, RuntimeFnId::Count).is_some(),
            "count() of a scalar is a PHP TypeError a header load cannot raise"
        );
    }
}
