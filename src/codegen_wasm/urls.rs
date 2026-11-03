//! Purpose:
//! The wasm32-wasi `parse_url` scanner: PHP 8.4 `php_url_parse_ex2`'s decision tree in WAT,
//! plus the component materialization that turns its result into PHP's `array|string|int|false`.
//!
//! Called from:
//! - `crate::codegen_wasm::builtins::emit_builtin_runtime()` to register the helpers.
//! - `crate::codegen_wasm::builtins` lowering for `RuntimeFnId::ParseUrl`.
//!
//! Key details:
//! - The parse writes into a fixed 72-byte table in runtime scratch — eight `(start, len)`
//!   pairs, a presence MASK and the port — rather than allocating per component, so a failed
//!   parse costs no heap traffic at all and the caller decides what to materialize.
//! - Missing and present-but-empty are DIFFERENT: `http://h/p?` has a `query` key holding the
//!   empty string, while `http://h/p` has no `query` key. The presence mask is what keeps them
//!   apart; a length of zero cannot.
//! - Components are copied with ASCII control bytes replaced by `_`, php-src's substitution,
//!   which is why a component cannot simply borrow a slice of the input.
//! - The port is validated, not merely scanned: `parse_url` answers `false` outright for a port
//!   outside `0..=65535`, and php's own scanner tolerates leading whitespace and a sign inside
//!   the authority (`http://host: 80` parses, port 80).

use super::runtime::FLOAT_SCRATCH_BASE;
use super::wat::{DataSegment, WatModule};

/// Offset of the parts table inside the float-scratch region.
///
/// The region is addressed through the immutable `$__float_scratch` global, which every module
/// carries, so the WAT never hardcodes a base address.
pub(super) const PARTS: u32 = 0x5000;

/// Offset of the read-only component-key blob, above the 72-byte table.
const KEYS: u32 = 0x5050;

/// The eight component keys, concatenated. PHP's array uses this fixed insertion ORDER.
const KEY_BLOB: &[u8] = b"schemehostportuserpasspathqueryfragment";

/// `(offset, length)` of each component key inside `KEY_BLOB`, indexed by component number.
const KEY_SPANS: [(u32, u32); 8] = [
    (0, 6),   // scheme
    (6, 4),   // host
    (10, 4),  // port
    (14, 4),  // user
    (18, 4),  // pass
    (22, 4),  // path
    (26, 5),  // query
    (31, 8),  // fragment
];

/// Registers the `parse_url` scanner, its key blob, and the materialization helpers.
pub(super) fn emit_url_runtime(wm: &mut WatModule) {
    wm.add_data(DataSegment {
        offset: FLOAT_SCRATCH_BASE + KEYS,
        bytes: KEY_BLOB.to_vec(),
    });
    wm.add_raw_func(&rt_parse_url_table());
    wm.add_raw_func(RT_PARSE_URL_PUT);
    wm.add_raw_func(RT_PARSE_URL_DELIM);
    wm.add_raw_func(RT_PARSE_URL_FIND);
    wm.add_raw_func(RT_PARSE_URL_RFIND);
    wm.add_raw_func(RT_PARSE_URL_SCHEME_BYTE);
    wm.add_raw_func(RT_PARSE_URL_SLASHES);
    wm.add_raw_func(RT_PARSE_URL_IS_FILE);
    wm.add_raw_func(RT_PARSE_URL_PORT);
    wm.add_raw_func(RT_PARSE_URL_PARTS);
    wm.add_raw_func(&rt_parse_url_key());
    wm.add_raw_func(RT_PARSE_URL_COPY);
    wm.add_raw_func(RT_PARSE_URL_CELL);
    wm.add_raw_func(RT_PARSE_URL_ARRAY);
    wm.add_raw_func(RT_PARSE_URL);
}

/// `__rt_parse_url_table`: the parts table's absolute address.
fn rt_parse_url_table() -> String {
    format!(
        r#"(func $__rt_parse_url_table (result i32)
  (i32.add (global.get $__float_scratch) (i32.const {PARTS})))
"#
    )
}

/// `__rt_parse_url_key`: the `(pointer, length)` of one component's array key.
fn rt_parse_url_key() -> String {
    let mut body = String::new();
    for (component, (offset, length)) in KEY_SPANS.iter().enumerate().take(7) {
        body.push_str(&format!(
            "  (if (i32.eq (local.get $idx) (i32.const {component}))\n    \
             (then (return (i32.add (global.get $__float_scratch) (i32.const {})) \
             (i32.const {length}))))\n",
            KEYS + offset
        ));
    }
    let (offset, length) = KEY_SPANS[7];
    format!(
        r#"(func $__rt_parse_url_key (param $idx i32) (result i32) (result i32)
{body}  (i32.add (global.get $__float_scratch) (i32.const {}))
  (i32.const {length}))
"#,
        KEYS + offset
    )
}

/// `__rt_parse_url_put`: records component `$idx` as PRESENT, spanning `[$start, $start+$len)`.
///
/// Presence is a bit in the mask rather than a non-zero length, because a present-but-empty
/// component is a real PHP answer and would otherwise be indistinguishable from a missing one.
const RT_PARSE_URL_PUT: &str = r#"(func $__rt_parse_url_put (param $idx i32) (param $start i32) (param $len i32)
  (local $slot i32)
  (local $t i32)
  (local.set $t (call $__rt_parse_url_table))
  (local.set $slot (i32.add (local.get $t) (i32.shl (local.get $idx) (i32.const 3))))  ;; 8 bytes per component
  (i32.store (local.get $slot) (local.get $start))                ;; start @ +0
  (i32.store offset=4 (local.get $slot) (local.get $len))         ;; length @ +4
  (i32.store offset=64 (local.get $t)
    (i32.or (i32.load offset=64 (local.get $t)) (i32.shl (i32.const 1) (local.get $idx)))))  ;; presence bit
"#;

/// `__rt_parse_url_delim`: first offset in `[$start, $len)` holding one of up to three
/// delimiters, or `$len` when none does. Pass `-1` for an unused delimiter slot.
const RT_PARSE_URL_DELIM: &str = r#"(func $__rt_parse_url_delim (param $p i32) (param $len i32) (param $start i32) (param $d0 i32) (param $d1 i32) (param $d2 i32) (result i32)
  (local $i i32)
  (local $b i32)
  (local.set $i (local.get $start))
  (block $done (loop $scan
    (br_if $done (i32.ge_s (local.get $i) (local.get $len)))
    (local.set $b (i32.load8_u (i32.add (local.get $p) (local.get $i))))
    (if (i32.or (i32.or (i32.eq (local.get $b) (local.get $d0))
                        (i32.eq (local.get $b) (local.get $d1)))
                (i32.eq (local.get $b) (local.get $d2)))
      (then (return (local.get $i))))
    (local.set $i (i32.add (local.get $i) (i32.const 1)))
    (br $scan)))
  (local.get $len))
"#;

/// `__rt_parse_url_find`: first offset of `$byte` in `[$start, $end)`, or `-1`.
const RT_PARSE_URL_FIND: &str = r#"(func $__rt_parse_url_find (param $p i32) (param $start i32) (param $end i32) (param $byte i32) (result i32)
  (local $i i32)
  (local.set $i (local.get $start))
  (block $done (loop $scan
    (br_if $done (i32.ge_s (local.get $i) (local.get $end)))
    (if (i32.eq (i32.load8_u (i32.add (local.get $p) (local.get $i))) (local.get $byte))
      (then (return (local.get $i))))
    (local.set $i (i32.add (local.get $i) (i32.const 1)))
    (br $scan)))
  (i32.const -1))
"#;

/// `__rt_parse_url_rfind`: LAST offset of `$byte` in `[$start, $end)`, or `-1`.
///
/// The authority scan needs the last `@` and the last `:`, not the first: `a:b@c:d@host`
/// puts the userinfo boundary at the final `@`, php-src's rule.
const RT_PARSE_URL_RFIND: &str = r#"(func $__rt_parse_url_rfind (param $p i32) (param $start i32) (param $end i32) (param $byte i32) (result i32)
  (local $i i32)
  (local.set $i (local.get $end))
  (block $done (loop $scan
    (br_if $done (i32.le_s (local.get $i) (local.get $start)))
    (local.set $i (i32.sub (local.get $i) (i32.const 1)))
    (if (i32.eq (i32.load8_u (i32.add (local.get $p) (local.get $i))) (local.get $byte))
      (then (return (local.get $i))))
    (br $scan)))
  (i32.const -1))
"#;

/// `__rt_parse_url_scheme_byte`: whether one byte may appear in a URL scheme.
const RT_PARSE_URL_SCHEME_BYTE: &str = r#"(func $__rt_parse_url_scheme_byte (param $b i32) (result i32)
  (if (i32.and (i32.ge_u (local.get $b) (i32.const 48)) (i32.le_u (local.get $b) (i32.const 57)))
    (then (return (i32.const 1))))                                ;; 0-9
  (if (i32.and (i32.ge_u (local.get $b) (i32.const 65)) (i32.le_u (local.get $b) (i32.const 90)))
    (then (return (i32.const 1))))                                ;; A-Z
  (if (i32.and (i32.ge_u (local.get $b) (i32.const 97)) (i32.le_u (local.get $b) (i32.const 122)))
    (then (return (i32.const 1))))                                ;; a-z
  (if (i32.or (i32.or (i32.eq (local.get $b) (i32.const 43)) (i32.eq (local.get $b) (i32.const 45)))
              (i32.eq (local.get $b) (i32.const 46)))
    (then (return (i32.const 1))))                                ;; + - .
  (i32.const 0))
"#;

/// `__rt_parse_url_slashes`: whether the URL begins with `//`.
const RT_PARSE_URL_SLASHES: &str = r#"(func $__rt_parse_url_slashes (param $p i32) (param $len i32) (result i32)
  (if (i32.lt_s (local.get $len) (i32.const 2)) (then (return (i32.const 0))))
  (if (i32.ne (i32.load8_u (local.get $p)) (i32.const 47)) (then (return (i32.const 0))))
  (i32.eq (i32.load8_u offset=1 (local.get $p)) (i32.const 47)))
"#;

/// `__rt_parse_url_is_file`: whether `url[0..$colon]` is `file`, case-INSENSITIVELY.
///
/// php-src special-cases the `file` scheme so `file:///etc/hosts` keeps `/etc/hosts` as a PATH
/// with no host, and `file:///c:/x` keeps the drive letter.
const RT_PARSE_URL_IS_FILE: &str = r#"(func $__rt_parse_url_is_file (param $p i32) (param $colon i32) (result i32)
  (local $i i32)
  (local $b i32)
  (local $want i32)
  (if (i32.ne (local.get $colon) (i32.const 4)) (then (return (i32.const 0))))
  (block $done (loop $scan
    (br_if $done (i32.ge_s (local.get $i) (i32.const 4)))
    (local.set $b (i32.load8_u (i32.add (local.get $p) (local.get $i))))
    (if (i32.and (i32.ge_u (local.get $b) (i32.const 65)) (i32.le_u (local.get $b) (i32.const 90)))
      (then (local.set $b (i32.add (local.get $b) (i32.const 32)))))     ;; A-Z -> a-z
    ;; "file", one byte at a time: the scheme is four bytes, so a table would cost more than it saves
    (local.set $want (i32.const 102))                                    ;; 'f'
    (if (i32.eq (local.get $i) (i32.const 1)) (then (local.set $want (i32.const 105))))  ;; 'i'
    (if (i32.eq (local.get $i) (i32.const 2)) (then (local.set $want (i32.const 108))))  ;; 'l'
    (if (i32.eq (local.get $i) (i32.const 3)) (then (local.set $want (i32.const 101))))  ;; 'e'
    (if (i32.ne (local.get $b) (local.get $want)) (then (return (i32.const 0))))
    (local.set $i (i32.add (local.get $i) (i32.const 1)))
    (br $scan)))
  (i32.const 1))
"#;

/// `__rt_parse_url_port`: php's port scanner over `[$start, $end)`, as `port + 1`, or `0`
/// when the bytes are not a port PHP accepts.
///
/// Returning `port + 1` is what lets port ZERO — which `parse_url` genuinely reports for
/// `http://h:0/` — be distinguished from a rejection without a second result. Leading
/// whitespace and an explicit sign are tolerated because php's own `ZEND_STRTOL` path is;
/// `-0` is the one negative that survives, since it converts back to the unsigned zero.
const RT_PARSE_URL_PORT: &str = r#"(func $__rt_parse_url_port (param $p i32) (param $start i32) (param $end i32) (result i32)
  (local $i i32)
  (local $b i32)
  (local $neg i32)
  (local $digits i32)
  (local $value i64)
  (local.set $i (local.get $start))
  (block $ws (loop $skip
    (br_if $ws (i32.ge_s (local.get $i) (local.get $end)))
    (local.set $b (i32.load8_u (i32.add (local.get $p) (local.get $i))))
    (br_if $ws (i32.eqz (i32.or
      (i32.eq (local.get $b) (i32.const 32))
      (i32.and (i32.ge_u (local.get $b) (i32.const 9)) (i32.le_u (local.get $b) (i32.const 13))))))
    (local.set $i (i32.add (local.get $i) (i32.const 1)))
    (br $skip)))
  (if (i32.lt_s (local.get $i) (local.get $end))
    (then
      (local.set $b (i32.load8_u (i32.add (local.get $p) (local.get $i))))
      (if (i32.eq (local.get $b) (i32.const 45))                   ;; '-'
        (then
          (local.set $neg (i32.const 1))
          (local.set $i (i32.add (local.get $i) (i32.const 1))))
        (else (if (i32.eq (local.get $b) (i32.const 43))           ;; '+'
          (then (local.set $i (i32.add (local.get $i) (i32.const 1)))))))))
  (block $stop (loop $digit
    (br_if $stop (i32.ge_s (local.get $i) (local.get $end)))
    (local.set $b (i32.load8_u (i32.add (local.get $p) (local.get $i))))
    (br_if $stop (i32.or (i32.lt_u (local.get $b) (i32.const 48)) (i32.gt_u (local.get $b) (i32.const 57))))
    (local.set $value (i64.add (i64.mul (local.get $value) (i64.const 10))
                               (i64.extend_i32_u (i32.sub (local.get $b) (i32.const 48)))))
    (if (i64.gt_u (local.get $value) (i64.const 16777216))
      (then (local.set $value (i64.const 16777216))))              ;; clamp: already out of range, cannot overflow
    (local.set $digits (i32.add (local.get $digits) (i32.const 1)))
    (local.set $i (i32.add (local.get $i) (i32.const 1)))
    (br $digit)))
  (if (i32.eqz (local.get $digits)) (then (return (i32.const 0)))) ;; no digits at all
  (if (local.get $neg)
    (then (if (i64.ne (local.get $value) (i64.const 0))
      (then (return (i32.const 0))))))                             ;; only `-0` survives a sign
  (if (i64.gt_u (local.get $value) (i64.const 65535)) (then (return (i32.const 0))))
  (i32.add (i32.wrap_i64 (local.get $value)) (i32.const 1)))
"#;

/// `__rt_parse_url_parts`: fills the parts table; `1` on success, `0` for PHP's `false`.
///
/// This is `php_url_parse_ex2`'s decision tree, state for state: the leading colon decides
/// between a scheme, a bare `host:port`, and a plain path, and only then do the authority and
/// path scans run. The three states are exactly php-src's `parse_port`, `parse_host` and
/// `just_path` labels, which is why the control flow looks like a goto rather than a descent.
const RT_PARSE_URL_PARTS: &str = r#"(func $__rt_parse_url_parts (param $p i32) (param $len i32) (result i32)
  (local $t i32)
  (local $cursor i32)
  (local $state i32)
  (local $sc i32)
  (local $colon i32)
  (local $i i32)
  (local $b i32)
  (local $ok i32)
  (local $after i32)
  (local $fqf i32)
  (local $ps i32)
  (local $pe i32)
  (local $dl i32)
  (local $pv i32)
  (local $ae i32)
  (local $at i32)
  (local $c2 i32)
  (local $pc i32)
  (local $he i32)
  (local $brk i32)
  (local.set $t (call $__rt_parse_url_table))
  (block $cleared (loop $clear
    (br_if $cleared (i32.ge_s (local.get $i) (i32.const 72)))
    (i32.store (i32.add (local.get $t) (local.get $i)) (i32.const 0))
    (local.set $i (i32.add (local.get $i) (i32.const 4)))
    (br $clear)))
  (local.set $colon (call $__rt_parse_url_find (local.get $p) (i32.const 0) (local.get $len) (i32.const 58)))
  (if (i32.eq (local.get $colon) (i32.const -1))
    (then
      (if (call $__rt_parse_url_slashes (local.get $p) (local.get $len))
        (then (local.set $cursor (i32.const 2)) (local.set $state (i32.const 1)))
        (else (local.set $state (i32.const 2)))))
    (else (if (i32.eqz (local.get $colon))
      (then (local.set $state (i32.const 0)) (local.set $sc (i32.const 0)))   ;; a leading ':' is a port marker
      (else
        (local.set $ok (i32.const 1))
        (local.set $i (i32.const 0))
        (block $checked (loop $scheme
          (br_if $checked (i32.ge_s (local.get $i) (local.get $colon)))
          (local.set $b (i32.load8_u (i32.add (local.get $p) (local.get $i))))
          (if (i32.eqz (call $__rt_parse_url_scheme_byte (local.get $b)))
            (then (local.set $ok (i32.const 0)) (br $checked)))
          (local.set $i (i32.add (local.get $i) (i32.const 1)))
          (br $scheme)))
        (if (i32.eqz (local.get $ok))
          (then
            ;; Not a scheme. A colon before any '?' or '#' still reads as a port marker.
            (local.set $fqf (call $__rt_parse_url_delim (local.get $p) (local.get $len) (i32.const 0)
              (i32.const 63) (i32.const 35) (i32.const -1)))
            (if (i32.and (i32.lt_s (i32.add (local.get $colon) (i32.const 1)) (local.get $len))
                         (i32.lt_s (local.get $colon) (local.get $fqf)))
              (then (local.set $state (i32.const 0)) (local.set $sc (local.get $colon)))
              (else (if (call $__rt_parse_url_slashes (local.get $p) (local.get $len))
                (then (local.set $cursor (i32.const 2)) (local.set $state (i32.const 1)))
                (else (local.set $state (i32.const 2)))))))
          (else (if (i32.eq (i32.add (local.get $colon) (i32.const 1)) (local.get $len))
            (then
              (call $__rt_parse_url_put (i32.const 0) (i32.const 0) (local.get $colon))
              (return (i32.const 1)))                                  ;; "mailto:" is a scheme and nothing else
            (else (if (i32.ne (i32.load8_u (i32.add (local.get $p) (i32.add (local.get $colon) (i32.const 1)))) (i32.const 47))
              (then
                ;; `x:80` and `x:80/p` are host:port; `x:anything-else` is scheme + path.
                (local.set $after (i32.add (local.get $colon) (i32.const 1)))
                (block $counted (loop $digits
                  (br_if $counted (i32.ge_s (local.get $after) (local.get $len)))
                  (local.set $b (i32.load8_u (i32.add (local.get $p) (local.get $after))))
                  (br_if $counted (i32.or (i32.lt_u (local.get $b) (i32.const 48)) (i32.gt_u (local.get $b) (i32.const 57))))
                  (local.set $after (i32.add (local.get $after) (i32.const 1)))
                  (br $digits)))
                (local.set $ok (i32.const 0))
                (if (i32.eq (local.get $after) (local.get $len))
                  (then (local.set $ok (i32.const 1)))
                  (else (if (i32.eq (i32.load8_u (i32.add (local.get $p) (local.get $after))) (i32.const 47))
                    (then (local.set $ok (i32.const 1))))))
                (if (i32.and (local.get $ok)
                             (i32.lt_s (i32.sub (local.get $after) (local.get $colon)) (i32.const 7)))
                  (then (local.set $state (i32.const 0)) (local.set $sc (local.get $colon)))
                  (else
                    (call $__rt_parse_url_put (i32.const 0) (i32.const 0) (local.get $colon))
                    (local.set $cursor (i32.add (local.get $colon) (i32.const 1)))
                    (local.set $state (i32.const 2)))))
              (else
                (call $__rt_parse_url_put (i32.const 0) (i32.const 0) (local.get $colon))
                (local.set $ok (i32.const 0))
                (if (i32.lt_s (i32.add (local.get $colon) (i32.const 2)) (local.get $len))
                  (then (if (i32.eq (i32.load8_u (i32.add (local.get $p) (i32.add (local.get $colon) (i32.const 2)))) (i32.const 47))
                    (then (local.set $ok (i32.const 1))))))
                (if (local.get $ok)
                  (then
                    (local.set $cursor (i32.add (local.get $colon) (i32.const 3)))
                    (local.set $brk (i32.const 0))
                    (if (call $__rt_parse_url_is_file (local.get $p) (local.get $colon))
                      (then (if (i32.lt_s (i32.add (local.get $colon) (i32.const 3)) (local.get $len))
                        (then (if (i32.eq (i32.load8_u (i32.add (local.get $p) (i32.add (local.get $colon) (i32.const 3)))) (i32.const 47))
                          (then (local.set $brk (i32.const 1))))))))
                    (if (local.get $brk)
                      (then
                        (if (i32.lt_s (i32.add (local.get $colon) (i32.const 5)) (local.get $len))
                          (then (if (i32.eq (i32.load8_u (i32.add (local.get $p) (i32.add (local.get $colon) (i32.const 5)))) (i32.const 58))
                            (then (local.set $cursor (i32.add (local.get $colon) (i32.const 4)))))))
                        (local.set $state (i32.const 2)))
                      (else (local.set $state (i32.const 1)))))
                  (else
                    (local.set $cursor (i32.add (local.get $colon) (i32.const 1)))
                    (local.set $state (i32.const 2))))))))))))))
  (block $finished (loop $states
    (block $dispatch
      (if (i32.eqz (local.get $state))
        (then
          ;; --- php-src `parse_port` ---
          (local.set $ps (i32.add (local.get $sc) (i32.const 1)))
          (local.set $pe (local.get $ps))
          (block $counted (loop $digits
            (br_if $counted (i32.ge_s (local.get $pe) (local.get $len)))
            (br_if $counted (i32.ge_s (i32.sub (local.get $pe) (local.get $ps)) (i32.const 6)))
            (local.set $b (i32.load8_u (i32.add (local.get $p) (local.get $pe))))
            (br_if $counted (i32.or (i32.lt_u (local.get $b) (i32.const 48)) (i32.gt_u (local.get $b) (i32.const 57))))
            (local.set $pe (i32.add (local.get $pe) (i32.const 1)))
            (br $digits)))
          (local.set $dl (i32.sub (local.get $pe) (local.get $ps)))
          (local.set $ok (i32.const 0))
          (if (i32.eq (local.get $pe) (local.get $len))
            (then (local.set $ok (i32.const 1)))
            (else (if (i32.eq (i32.load8_u (i32.add (local.get $p) (local.get $pe))) (i32.const 47))
              (then (local.set $ok (i32.const 1))))))
          (if (i32.and (i32.and (i32.gt_s (local.get $dl) (i32.const 0))
                                (i32.lt_s (local.get $dl) (i32.const 6)))
                       (local.get $ok))
            (then
              (local.set $pv (call $__rt_parse_url_port (local.get $p) (local.get $ps) (local.get $pe)))
              (if (i32.eqz (local.get $pv)) (then (return (i32.const 0))))
              (i32.store offset=68 (local.get $t) (i32.sub (local.get $pv) (i32.const 1)))
              (i32.store offset=64 (local.get $t) (i32.or (i32.load offset=64 (local.get $t)) (i32.const 4)))
              (if (call $__rt_parse_url_slashes (local.get $p) (local.get $len))
                (then (local.set $cursor (i32.const 2))))
              (local.set $state (i32.const 1)))
            (else (if (i32.and (i32.eqz (local.get $dl)) (i32.eq (local.get $pe) (local.get $len)))
              (then (return (i32.const 0)))                            ;; a bare trailing ':' is not a URL
              (else (if (call $__rt_parse_url_slashes (local.get $p) (local.get $len))
                (then (local.set $cursor (i32.const 2)) (local.set $state (i32.const 1)))
                (else (local.set $state (i32.const 2))))))))
          (br $dispatch)))
      (if (i32.eq (local.get $state) (i32.const 1))
        (then
          ;; --- php-src `parse_host`: userinfo, host, port ---
          (local.set $ae (call $__rt_parse_url_delim (local.get $p) (local.get $len) (local.get $cursor)
            (i32.const 47) (i32.const 63) (i32.const 35)))
          (local.set $at (call $__rt_parse_url_rfind (local.get $p) (local.get $cursor) (local.get $ae) (i32.const 64)))
          (if (i32.ne (local.get $at) (i32.const -1))
            (then
              (local.set $c2 (call $__rt_parse_url_find (local.get $p) (local.get $cursor) (local.get $at) (i32.const 58)))
              (if (i32.ne (local.get $c2) (i32.const -1))
                (then
                  (call $__rt_parse_url_put (i32.const 3) (local.get $cursor) (i32.sub (local.get $c2) (local.get $cursor)))
                  (call $__rt_parse_url_put (i32.const 4) (i32.add (local.get $c2) (i32.const 1))
                    (i32.sub (local.get $at) (i32.add (local.get $c2) (i32.const 1)))))
                (else
                  (call $__rt_parse_url_put (i32.const 3) (local.get $cursor) (i32.sub (local.get $at) (local.get $cursor)))))
              (local.set $cursor (i32.add (local.get $at) (i32.const 1)))))
          (local.set $brk (i32.const 0))
          (if (i32.and (i32.lt_s (local.get $cursor) (local.get $len))
                       (i32.lt_s (local.get $cursor) (local.get $ae)))
            (then (if (i32.eq (i32.load8_u (i32.add (local.get $p) (local.get $cursor))) (i32.const 91))
              (then (if (i32.eq (i32.load8_u (i32.add (local.get $p) (i32.sub (local.get $ae) (i32.const 1)))) (i32.const 93))
                (then (local.set $brk (i32.const 1))))))))            ;; [::1] keeps its brackets AND its colons
          (local.set $pc (i32.const -1))
          (if (i32.eqz (local.get $brk))
            (then (local.set $pc (call $__rt_parse_url_rfind (local.get $p) (local.get $cursor) (local.get $ae) (i32.const 58)))))
          (if (i32.ne (local.get $pc) (i32.const -1))
            (then
              (if (i32.eqz (i32.and (i32.load offset=64 (local.get $t)) (i32.const 4)))
                (then
                  (local.set $dl (i32.sub (local.get $ae) (i32.add (local.get $pc) (i32.const 1))))
                  (if (i32.gt_s (local.get $dl) (i32.const 5)) (then (return (i32.const 0))))
                  (if (i32.gt_s (local.get $dl) (i32.const 0))
                    (then
                      (local.set $pv (call $__rt_parse_url_port (local.get $p) (i32.add (local.get $pc) (i32.const 1)) (local.get $ae)))
                      (if (i32.eqz (local.get $pv)) (then (return (i32.const 0))))
                      (i32.store offset=68 (local.get $t) (i32.sub (local.get $pv) (i32.const 1)))
                      (i32.store offset=64 (local.get $t) (i32.or (i32.load offset=64 (local.get $t)) (i32.const 4)))))))
              (local.set $he (local.get $pc)))
            (else (local.set $he (local.get $ae))))
          (if (i32.le_s (local.get $he) (local.get $cursor)) (then (return (i32.const 0))))  ;; an empty host is not a URL
          (call $__rt_parse_url_put (i32.const 1) (local.get $cursor) (i32.sub (local.get $he) (local.get $cursor)))
          (if (i32.eq (local.get $ae) (local.get $len)) (then (return (i32.const 1))))
          (local.set $cursor (local.get $ae))
          (local.set $state (i32.const 2))
          (br $dispatch)))
      ;; --- php-src `just_path`: fragment, then query, then whatever is left ---
      (local.set $pe (local.get $len))
      (local.set $i (call $__rt_parse_url_find (local.get $p) (local.get $cursor) (local.get $pe) (i32.const 35)))
      (if (i32.ne (local.get $i) (i32.const -1))
        (then
          (call $__rt_parse_url_put (i32.const 7) (i32.add (local.get $i) (i32.const 1))
            (i32.sub (local.get $pe) (i32.add (local.get $i) (i32.const 1))))
          (local.set $pe (local.get $i))))
      (local.set $i (call $__rt_parse_url_find (local.get $p) (local.get $cursor) (local.get $pe) (i32.const 63)))
      (if (i32.ne (local.get $i) (i32.const -1))
        (then
          (call $__rt_parse_url_put (i32.const 6) (i32.add (local.get $i) (i32.const 1))
            (i32.sub (local.get $pe) (i32.add (local.get $i) (i32.const 1))))
          (local.set $pe (local.get $i))))
      (if (i32.or (i32.lt_s (local.get $cursor) (local.get $pe))
                  (i32.eq (local.get $cursor) (local.get $len)))
        (then (call $__rt_parse_url_put (i32.const 5) (local.get $cursor) (i32.sub (local.get $pe) (local.get $cursor)))))
      (br $finished))
    (br $states)))
  (i32.const 1))
"#;

/// `__rt_parse_url_copy`: an OWNED copy of one string component, control bytes substituted.
///
/// The substitution is why a component can never simply borrow a slice of the input, even
/// where the consumer would persist its own copy anyway.
const RT_PARSE_URL_COPY: &str = r#"(func $__rt_parse_url_copy (param $p i32) (param $idx i32) (result i32) (result i32)
  (local $slot i32)
  (local $s i32)
  (local $n i32)
  (local $out i32)
  (local $i i32)
  (local $b i32)
  (local.set $slot (i32.add (call $__rt_parse_url_table) (i32.shl (local.get $idx) (i32.const 3))))
  (local.set $s (i32.load (local.get $slot)))
  (local.set $n (i32.load offset=4 (local.get $slot)))
  (local.set $out (call $__rt_str_alloc (i64.extend_i32_u (local.get $n))))
  (block $copied (loop $copy
    (br_if $copied (i32.ge_u (local.get $i) (local.get $n)))
    (local.set $b (i32.load8_u (i32.add (local.get $p) (i32.add (local.get $s) (local.get $i)))))
    (if (i32.or (i32.lt_u (local.get $b) (i32.const 32)) (i32.eq (local.get $b) (i32.const 127)))
      (then (local.set $b (i32.const 95))))                         ;; ASCII control -> '_'
    (i32.store8 (i32.add (local.get $out) (local.get $i)) (local.get $b))
    (local.set $i (i32.add (local.get $i) (i32.const 1)))
    (br $copy)))
  (local.get $out) (local.get $n))
"#;

/// `__rt_parse_url_cell`: one component as an owned Mixed cell — string, int port, or null.
const RT_PARSE_URL_CELL: &str = r#"(func $__rt_parse_url_cell (param $p i32) (param $idx i32) (result i32)
  (local $t i32)
  (local $out i32)
  (local $n i32)
  (local $cell i32)
  (local.set $t (call $__rt_parse_url_table))
  (if (i32.eqz (i32.and (i32.load offset=64 (local.get $t)) (i32.shl (i32.const 1) (local.get $idx))))
    (then (return (call $__rt_mixed_from_value (i64.const 8) (i64.const 0) (i64.const 0)))))  ;; absent -> null
  (if (i32.eq (local.get $idx) (i32.const 2))
    (then (return (call $__rt_mixed_from_value (i64.const 0)
      (i64.extend_i32_u (i32.load offset=68 (local.get $t))) (i64.const 0)))))                ;; port is an int
  (call $__rt_parse_url_copy (local.get $p) (local.get $idx))
  (local.set $n)
  (local.set $out)
  (local.set $cell (call $__rt_mixed_from_value (i64.const 1)
    (i64.extend_i32_u (local.get $out)) (i64.extend_i32_u (local.get $n))))                   ;; boxing persists its own copy
  (call $__rt_decref_any (local.get $out))
  (local.get $cell))
"#;

/// `__rt_parse_url_array`: the full result as an ordered hash of present components.
///
/// A hash slot holds its value INLINE as a `(lo, hi, tag)` triple, not a pointer to a boxed
/// cell — so the port goes in under tag 0 and every other component under tag 1, and the
/// heterogeneity lives in the per-entry tags rather than in a layer of Mixed cells. The hash's
/// own `value_tag` is the DECLARED element type (Mixed), which is a different question.
/// `__rt_hash_set` persists a tag-1 payload, so the transient copy is released right after.
const RT_PARSE_URL_ARRAY: &str = r#"(func $__rt_parse_url_array (param $p i32) (result i32)
  (local $t i32)
  (local $h i32)
  (local $i i32)
  (local $kp i32)
  (local $kl i32)
  (local $out i32)
  (local $n i32)
  (local.set $t (call $__rt_parse_url_table))
  (local.set $h (call $__rt_hash_new (i64.const 16) (i64.const 7)))   ;; 16 slots: eight entries stay under the 75% load factor
  (block $done (loop $each
    (br_if $done (i32.ge_s (local.get $i) (i32.const 8)))
    (if (i32.and (i32.load offset=64 (local.get $t)) (i32.shl (i32.const 1) (local.get $i)))
      (then
        (call $__rt_parse_url_key (local.get $i))
        (local.set $kl)
        (local.set $kp)
        (if (i32.eq (local.get $i) (i32.const 2))
          (then
            (local.set $h (call $__rt_hash_set (local.get $h)
              (i64.extend_i32_u (local.get $kp)) (i64.extend_i32_u (local.get $kl))
              (i64.extend_i32_u (i32.load offset=68 (local.get $t))) (i64.const 0) (i64.const 0))))
          (else
            (call $__rt_parse_url_copy (local.get $p) (local.get $i))
            (local.set $n)
            (local.set $out)
            (local.set $h (call $__rt_hash_set (local.get $h)
              (i64.extend_i32_u (local.get $kp)) (i64.extend_i32_u (local.get $kl))
              (i64.extend_i32_u (local.get $out)) (i64.extend_i32_u (local.get $n)) (i64.const 1)))
            (call $__rt_decref_any (local.get $out))))))
    (local.set $i (i32.add (local.get $i) (i32.const 1)))
    (br $each)))
  (local.get $h))
"#;

/// `__rt_parse_url`: the builtin. `$component` below zero selects the whole array.
///
/// A component ABOVE seven is php's catchable `ValueError` and never reaches here — the
/// capability audit refuses that call at compile time rather than guessing a value, since the
/// selector has to be a compile-time constant for this target to prove which shape comes back.
const RT_PARSE_URL: &str = r#"(func $__rt_parse_url (param $p i32) (param $len i64) (param $component i64) (result i32)
  (local $hash i32)
  (local $cell i32)
  (if (i32.eqz (call $__rt_parse_url_parts (local.get $p) (i32.wrap_i64 (local.get $len))))
    (then (return (call $__rt_mixed_from_value (i64.const 3) (i64.const 0) (i64.const 0)))))  ;; php's false
  (if (i64.lt_s (local.get $component) (i64.const 0))
    (then
      (local.set $hash (call $__rt_parse_url_array (local.get $p)))
      (local.set $cell (call $__rt_mixed_from_value (i64.const 5)
        (i64.extend_i32_u (local.get $hash)) (i64.const 0)))                                  ;; boxing increfs the hash
      (call $__rt_decref_any (local.get $hash))
      (return (local.get $cell))))
  (call $__rt_parse_url_cell (local.get $p) (i32.wrap_i64 (local.get $component))))
"#;
