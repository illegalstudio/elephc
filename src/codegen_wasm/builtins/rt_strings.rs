//! Purpose:
//! WAT source for the string helpers (`__rt_str_*`, trim, pad, replace, split, wordwrap, etc.).
//! Each `RT_*` constant is one `(func ...)` that `emit_builtin_runtime` registers.
//!
//! Called from:
//! - `super::emit_builtin_runtime` registers every constant; `strings` lowers the calls.
//!
//! Key details:
//! - Pure move of text: edit a helper here and its lowerer/shape predicate together.

/// `__rt_str_map_case`: owns a copy of a string with its ASCII letters case-mapped.
///
/// `$upper` selects the direction. Since PHP 8.2 `strtoupper` and `strtolower` are
/// LOCALE-INDEPENDENT and touch `A-Z` / `a-z` only — byte `0xE9` comes back unchanged, which is
/// what makes a pure byte map correct here rather than an approximation.
pub(super) const RT_STR_MAP_CASE: &str = r#"(func $__rt_str_map_case (param $ptr i32) (param $len i64) (param $upper i32) (result i32) (result i64)
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
pub(super) const RT_STR_REVERSE: &str = r#"(func $__rt_str_reverse (param $ptr i32) (param $len i64) (result i32) (result i64)
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

/// `__rt_str_alloc`: reserves an owned kind-1 string block of `bytes` capacity.
///
/// A re-encoding transform cannot size its result before it runs, so it reserves the worst case
/// here and returns the length it actually produced. The block stays at its reserved size, which
/// costs slack rather than correctness: a PHP string is the `(ptr, len)` pair, and the header the
/// release path reads is the reservation. `__rt_checked_layout` is what rejects a negative or
/// wasm32-overflowing size before the allocation rather than after.
pub(super) const RT_STR_ALLOC: &str = r#"(func $__rt_str_alloc (param $bytes i64) (result i32)
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
pub(super) const RT_STR_BIN2HEX: &str = r#"(func $__rt_str_bin2hex (param $ptr i32) (param $len i64) (result i32) (result i64)
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
pub(super) const RT_STR_ADDSLASHES: &str = r#"(func $__rt_str_addslashes (param $ptr i32) (param $len i64) (result i32) (result i64)
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
pub(super) const RT_STR_STRIPSLASHES: &str = r#"(func $__rt_str_stripslashes (param $ptr i32) (param $len i64) (result i32) (result i64)
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
pub(super) const RT_STR_NL2BR: &str = r#"(func $__rt_str_nl2br (param $ptr i32) (param $len i64) (result i32) (result i64)
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

/// `__rt_str_chr`: owns the one-byte string PHP's `chr` returns for any integer.
///
/// PHP does not reject an out-of-range codepoint: it constrains it with `% 256`, and a NEGATIVE
/// remainder is brought back up by adding 256, so `chr(-1)` is `\xff` and `chr(1000000)` is
/// `\x40` — measured. `$deprecate` carries whether this profile diagnoses the out-of-range
/// argument; the RESULT is the same either way, which is why the flag only gates the message.
pub(super) const RT_STR_CHR_TEMPLATE: &str = r#"(func $__rt_str_chr (param $n i64) (result i32) (result i64)
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
pub(super) const RT_STR_ORD_TEMPLATE: &str = r#"(func $__rt_str_ord (param $ptr i32) (param $len i64) (result i64)
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
pub(super) fn str_chr_ord(diagnoses: bool) -> (String, String) {
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
pub(super) const RT_STR_CASE_EDGE: &str = r#"(func $__rt_str_case_edge (param $ptr i32) (param $len i64) (param $upper i32) (result i32) (result i64)
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
pub(super) const RT_STR_UCWORDS: &str = r#"(func $__rt_str_ucwords (param $ptr i32) (param $len i64) (result i32) (result i64)
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
pub(super) const RT_STR_CMP: &str = r#"(func $__rt_str_cmp (param $aptr i32) (param $alen i64) (param $bptr i32) (param $blen i64) (param $fold i32) (result i64)
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
pub(super) const RT_STR_TRIM: &str = r#"(func $__rt_str_trim (param $ptr i32) (param $len i64) (param $cl_ptr i32) (param $cl_len i64) (param $mode i32) (result i32) (result i64)
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
pub(super) const RT_TRIM_MATCHES: &str = r#"(func $__rt_trim_matches (param $byte i32) (param $cl_ptr i32) (param $cl_len i64) (result i32)
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
pub(super) const RT_STR_SUBSTR: &str = r#"(func $__rt_str_substr (param $ptr i32) (param $len i64) (param $offset i64) (param $count i64) (param $has_len i32) (result i32) (result i64)
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
pub(super) const RT_STR_REPEAT: &str = r#"(func $__rt_str_repeat (param $ptr i32) (param $len i64) (param $times i64) (result i32) (result i64)
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
pub(super) const RT_STR_FIND: &str = r#"(func $__rt_str_find (param $hptr i32) (param $hlen i64) (param $nptr i32) (param $nlen i64) (param $fold i32) (result i64)
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
pub(super) const RT_PAD_BYTE: &str = r#"(func $__rt_pad_byte (param $pptr i32) (param $plen i64) (param $i i64) (result i32)
  (if (i32.eqz (local.get $pptr))
    (then (return (i32.const 32))))                               ;; the default pad is a space
  (i32.load8_u (i32.add (local.get $pptr)
    (i32.wrap_i64 (i64.rem_s (local.get $i) (local.get $plen))))))  ;; cycle through the pad
"#;

pub(super) const RT_STR_PAD: &str = r#"(func $__rt_str_pad (param $ptr i32) (param $len i64) (param $target i64) (param $pptr i32) (param $plen i64) (param $mode i32) (result i32) (result i64)
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
pub(super) const RT_STR_REPLACE: &str = r#"(func $__rt_str_replace (param $sptr i32) (param $slen i64) (param $nptr i32) (param $nlen i64) (param $rptr i32) (param $rlen i64) (result i32) (result i64)
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

/// `__rt_str_rfind`: the LAST index of a needle in a haystack, or -1 when it is absent.
///
/// Scanning runs from the last offset that could still fit the needle down to zero, so overlapping
/// matches resolve to the rightmost one — `strrpos("aaa", "aa")` is 1, not 0. An empty needle
/// starts the scan at `hlen` and matches immediately, which is why `strrpos("abcabc", "")` is 6:
/// the position just past the end.
pub(super) const RT_STR_RFIND: &str = r#"(func $__rt_str_rfind (param $hptr i32) (param $hlen i64) (param $nptr i32) (param $nlen i64) (result i64)
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

/// `__rt_str_split`: cuts a string into fixed-size chunks as a fresh indexed string array.
///
/// The final chunk is SHORT when the length does not divide evenly, and an EMPTY subject yields
/// the EMPTY array rather than one empty element — which is PHP 8.2's behaviour and the opposite
/// of `explode`, where the tail is always pushed. The caller has already refused a non-positive
/// chunk length, which is what would otherwise make this loop forever.
pub(super) const RT_STR_SPLIT: &str = r#"(func $__rt_str_split (param $ptr i32) (param $len i64) (param $chunk i64) (result i32)
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
pub(super) const RT_WORDWRAP_GENERAL: &str = r#"(func $__rt_wordwrap_general (param $ptr i32) (param $len i64) (param $width i64) (param $bptr i32) (param $blen i64) (param $cut i32) (result i32) (result i64)
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
pub(super) const RT_WORDWRAP: &str = r#"(func $__rt_wordwrap (param $ptr i32) (param $len i64) (param $width i64) (result i32) (result i64)
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

/// `__rt_str_region_eq`: compares `nlen` bytes of a needle against a haystack at `offset`.
///
/// The caller guarantees the region is in bounds, which every user below checks by comparing
/// lengths first. An empty needle matches anywhere, which is what makes `str_contains($h, "")`
/// true in PHP.
pub(super) const RT_STR_REGION_EQ: &str = r#"(func $__rt_str_region_eq (param $hptr i32) (param $nptr i32) (param $nlen i64) (param $offset i64) (result i64)
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
pub(super) const RT_STR_CONTAINS: &str = r#"(func $__rt_str_contains (param $hptr i32) (param $hlen i64) (param $nptr i32) (param $nlen i64) (result i64)
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
