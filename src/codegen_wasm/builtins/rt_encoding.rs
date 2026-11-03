//! Purpose:
//! WAT source for the byte-encoding helpers: hex, URL, base64, UTF-8 validation and HTML escaping.
//!
//! Called from:
//! - `super::emit_builtin_runtime` registers every constant; `encoding` and `strings` admit the calls.

/// `__rt_hex_digit_value`: the value of one ASCII hex digit, or -1 for any other byte.
///
/// Both cases are accepted, which is what makes `urldecode("%aB")` and `urldecode("%Ab")` agree
/// with php-src. The -1 sentinel is what lets a caller distinguish "not hex" from the digit zero.
pub(super) const RT_HEX_DIGIT_VALUE: &str = r#"(func $__rt_hex_digit_value (param $c i32) (result i32)
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
pub(super) const RT_STR_URL_ENCODE: &str = r#"(func $__rt_str_url_encode (param $ptr i32) (param $len i64) (param $raw i32) (result i32) (result i64)
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
pub(super) const RT_STR_URL_DECODE: &str = r#"(func $__rt_str_url_decode (param $ptr i32) (param $len i64) (param $plus i32) (result i32) (result i64)
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
pub(super) const RT_B64_CHAR: &str = r#"(func $__rt_b64_char (param $v i32) (result i32)
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
pub(super) const RT_B64_VALUE: &str = r#"(func $__rt_b64_value (param $c i32) (result i32)
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
pub(super) const RT_STR_BASE64_ENCODE: &str = r#"(func $__rt_str_base64_encode (param $ptr i32) (param $len i64) (result i32) (result i64)
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
pub(super) const RT_STR_BASE64_DECODE: &str = r#"(func $__rt_str_base64_decode (param $ptr i32) (param $len i64) (result i32) (result i64)
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

/// `__rt_base64_decode`: the `base64_decode` BUILTIN, boxed as PHP's `string|false`.
///
/// The declared return type is a union for every arity because the two-argument STRICT form can
/// answer `false`; the one-argument form lowered here never can, so this always boxes tag 1.
/// Wrapping the decode rather than inlining it at each call site keeps one copy of the boxing in
/// the module. `__rt_mixed_from_value` persists its own copy of a tag-1 payload, so the
/// transient decode block is released right after — the same ownership handoff `__rt_getenv`
/// makes.
pub(super) const RT_BASE64_DECODE: &str = r#"(func $__rt_base64_decode (param $ptr i32) (param $len i64) (result i32)
  (local $out i32)                                                ;; transient decoded block
  (local $olen i64)                                               ;; decoded length
  (local $cell i32)                                               ;; the boxed answer
  (call $__rt_str_base64_decode (local.get $ptr) (local.get $len))
  (local.set $olen)                                               ;; decoded length
  (local.set $out)                                                ;; decoded pointer
  (local.set $cell (call $__rt_mixed_from_value
    (i64.const 1)                                                 ;; mixed tag (string)
    (i64.extend_i32_u (local.get $out))
    (local.get $olen)))                                           ;; boxing persists a copy
  (call $__rt_decref_any (local.get $out))                        ;; release the transient block
  (local.get $cell))                                              ;; owned Mixed cell
"#;

/// `__rt_utf8_seq_len`: the length of a VALID UTF-8 sequence at `$p`, or 0 when there is none.
///
/// Rejects what the encoding forbids rather than only what is structurally short: overlong forms
/// (`C0`/`C1`, `E0 80`, `F0 80`), UTF-16 surrogates (`ED A0`..`ED BF`), and anything above
/// U+10FFFF (`F4 90`.., `F5`..`FF`). A caller that gets 0 is looking at an invalid sequence.
pub(super) const RT_UTF8_SEQ_LEN: &str = r#"(func $__rt_utf8_seq_len (param $ptr i32) (param $len i64) (param $p i64) (result i32)
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
pub(super) const RT_UTF8_BAD_SPAN: &str = r#"(func $__rt_utf8_bad_span (param $ptr i32) (param $len i64) (param $p i64) (result i64)
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
pub(super) const RT_HTMLSPECIALCHARS: &str = r#"(func $__rt_htmlspecialchars (param $ptr i32) (param $len i64) (result i32) (result i64)
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
pub(super) const RT_HTML_PUT: &str = r#"(func $__rt_html_put (param $out i32) (param $w i32) (param $which i32) (result i32)
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
