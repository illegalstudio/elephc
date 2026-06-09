//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of strings interpolation and hashes, including interpolation simple, interpolation multiple, and interpolation at start.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

/// Verifies simple double-quoted string interpolation with one variable.
/// Fixture: assign a string to `$name`, then echo `"Hello $name"`.
#[test]
fn test_string_interpolation_simple() {
    let out = compile_and_run(r#"<?php $name = "World"; echo "Hello $name";"#);
    assert_eq!(out, "Hello World");
}

/// Verifies double-quoted string interpolation with two variables adjacent in the string.
/// Fixture: `$a = "foo"`, `$b = "bar"`, then echo `"$a and $b"`.
#[test]
fn test_string_interpolation_multiple() {
    let out = compile_and_run(r#"<?php $a = "foo"; $b = "bar"; echo "$a and $b";"#);
    assert_eq!(out, "foo and bar");
}

/// Verifies double-quoted string interpolation when the variable appears at the start of the string.
/// Fixture: `$x = "hi"`, then echo `"$x there"`.
#[test]
fn test_string_interpolation_at_start() {
    let out = compile_and_run(r#"<?php $x = "hi"; echo "$x there";"#);
    assert_eq!(out, "hi there");
}

/// Verifies double-quoted string interpolation when the variable appears at the end of the string.
/// Fixture: `$x = "world"`, then echo `"hello $x"`.
#[test]
fn test_string_interpolation_at_end() {
    let out = compile_and_run(r#"<?php $x = "world"; echo "hello $x";"#);
    assert_eq!(out, "hello world");
}

/// Verifies that single-quoted strings do NOT perform variable interpolation.
/// Fixture: `$x = 42`, then echo `'$x'`; expects literal "$x" in output.
#[test]
fn test_string_no_interpolation() {
    // Single-quoted strings should NOT interpolate
    let out = compile_and_run("<?php $x = 42; echo '$x';");
    assert_eq!(out, "$x");
}

/// Verifies complex `{$var}` interpolation: the braces delimit the variable and are not
/// emitted literally.
#[test]
fn test_string_interpolation_complex_simple_var() {
    let out = compile_and_run(r#"<?php $b = "B"; echo "a{$b}c";"#);
    assert_eq!(out, "aBc");
}

/// Verifies complex `{$arr[idx]}` interpolation evaluates the array access inside braces.
#[test]
fn test_string_interpolation_complex_array_access() {
    let out = compile_and_run(r#"<?php $a = [1, 2, 3]; echo "x{$a[1]}y";"#);
    assert_eq!(out, "x2y");
}

/// Verifies complex `{$obj->prop}` interpolation evaluates the property access inside braces.
#[test]
fn test_string_interpolation_complex_property() {
    let out = compile_and_run(r#"<?php class C { public $x = 5; } $o = new C(); echo "{$o->x}";"#);
    assert_eq!(out, "5");
}

/// Verifies simple `$arr[key]` interpolation with a bareword key (treated as a string key).
#[test]
fn test_string_interpolation_simple_array_bareword() {
    let out = compile_and_run(r#"<?php $a = ["k" => "V"]; echo "X $a[k] Y";"#);
    assert_eq!(out, "X V Y");
}

/// Verifies simple `$arr[int]` interpolation with an integer key.
#[test]
fn test_string_interpolation_simple_array_int() {
    let out = compile_and_run(r#"<?php $a = [10, 20]; echo "$a[1]";"#);
    assert_eq!(out, "20");
}

/// Verifies simple `$obj->prop` interpolation reads a single property.
#[test]
fn test_string_interpolation_simple_property() {
    let out =
        compile_and_run(r#"<?php class C { public $x = 5; } $o = new C(); echo "v=$o->x";"#);
    assert_eq!(out, "v=5");
}

/// Verifies a `{` not followed by `$` stays a literal brace (PHP only treats `{$` as the
/// start of complex interpolation).
#[test]
fn test_string_literal_brace_not_interpolation() {
    let out = compile_and_run(r#"<?php echo "a{b}c";"#);
    assert_eq!(out, "a{b}c");
}

/// Verifies `md5()` produces the correct hash for an empty string input.
#[test]
fn test_md5_empty() {
    let out = compile_and_run(r#"<?php echo md5("");"#);
    assert_eq!(out, "d41d8cd98f00b204e9800998ecf8427e");
}

/// Verifies `md5()` produces the correct hash for "Hello".
#[test]
fn test_md5_hello() {
    let out = compile_and_run(r#"<?php echo md5("Hello");"#);
    assert_eq!(out, "8b1a9953c4611296a827abf8c47804d7");
}

/// Verifies `sha1()` produces the correct hash for an empty string input.
#[test]
fn test_sha1_empty() {
    let out = compile_and_run(r#"<?php echo sha1("");"#);
    assert_eq!(out, "da39a3ee5e6b4b0d3255bfef95601890afd80709");
}

/// Verifies `sha1()` produces the correct hash for "Hello".
#[test]
fn test_sha1_hello() {
    let out = compile_and_run(r#"<?php echo sha1("Hello");"#);
    assert_eq!(out, "f7ff9e8b7bb2e09b70935a5d785e0cc5d9d0abf0");
}

// --- crc32() ---

// Verifies crc32() against PHP reference vectors, including the empty string (0)
// and the canonical "123456789" CRC-32 test vector. The result is a non-negative
// 64-bit int (the unsigned 32-bit checksum), matching 64-bit PHP.
/// Verifies compiled PHP output for crc32 known vectors.
#[test]
fn test_crc32_known_vectors() {
    let out = compile_and_run(
        r#"<?php echo crc32("") . "|" . crc32("123456789") . "|" . crc32("The quick brown fox");"#,
    );
    assert_eq!(out, "0|3421780262|3074782430");
}

// Verifies crc32() resolves through PHP's case-insensitive builtin lookup and
// that its result feeds arithmetic as a plain int.
/// Verifies compiled PHP output for crc32 case insensitive and int.
#[test]
fn test_crc32_case_insensitive_and_int() {
    let out = compile_and_run(r#"<?php echo CRC32("abc") + 1;"#);
    assert_eq!(out, "891568579"); // crc32("abc") = 891568578
}

// --- hash() ---

/// Verifies `hash("md5", ...)` produces the correct hash for "Hello".
#[test]
fn test_hash_md5() {
    let out = compile_and_run(r#"<?php echo hash("md5", "Hello");"#);
    assert_eq!(out, "8b1a9953c4611296a827abf8c47804d7");
}

/// Verifies `hash("sha1", ...)` produces the correct hash for "Hello".
#[test]
fn test_hash_sha1() {
    let out = compile_and_run(r#"<?php echo hash("sha1", "Hello");"#);
    assert_eq!(out, "f7ff9e8b7bb2e09b70935a5d785e0cc5d9d0abf0");
}

/// Verifies `hash("sha256", ...)` produces the correct hash for "Hello".
#[test]
fn test_hash_sha256() {
    let out = compile_and_run(r#"<?php echo hash("sha256", "Hello");"#);
    assert_eq!(
        out,
        "185f8db32271fe25f561a6fc938b2e264306ec304eda518007d1764826381969"
    );
}

/// Verifies `hash()` now reaches the full elephc-crypto algorithm set
/// (sha512, sha3-256, ripemd160, crc32b) beyond the legacy md5/sha1/sha256.
#[test]
fn hash_supports_full_algorithm_set() {
    assert_eq!(compile_and_run(r#"<?php echo hash("sha256","hello");"#),
        "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824");
    assert_eq!(compile_and_run(r#"<?php echo hash("sha512","hello");"#),
        "9b71d224bd62f3785d96d46ad3ea3d73319bfbc2890caadae2dff72519673ca72323c3d99ba5c11d7c7acc6e14b8c5da0c4663475c2e5c3adef46f73bcdec043");
    assert_eq!(compile_and_run(r#"<?php echo hash("sha3-256","hello");"#),
        "3338be694f50c5f338814986cdf0686453a888b84f424d792af4b9202398f392");
    assert_eq!(compile_and_run(r#"<?php echo hash("ripemd160","hello");"#),
        "108f07b8382412612c048d07d13f814118445acd");
    assert_eq!(compile_and_run(r#"<?php echo hash("crc32b","hello");"#), "3610a686");
}

/// Verifies md5/sha256 keep byte-for-byte parity through the new crate path,
/// guarding against a marshalling regression for the previously CommonCrypto-backed algos.
#[test]
fn hash_md5_sha256_parity_regression() {
    assert_eq!(compile_and_run(r#"<?php echo hash("md5","abc");"#),
        "900150983cd24fb0d6963f7d28e17f72");
    assert_eq!(compile_and_run(r#"<?php echo hash("sha256","abc");"#),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
}

/// Verifies the `$binary=true` flag returns the raw digest bytes (not hex),
/// so bin2hex round-trips to the hex form and strlen reports the raw digest size.
#[test]
fn hash_binary_flag_returns_raw_bytes() {
    assert_eq!(compile_and_run(r#"<?php echo bin2hex(hash("sha256","abc",true));"#),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    assert_eq!(compile_and_run(r#"<?php echo strlen(hash("sha256","abc",true));"#), "32");
}

/// Verifies an unknown algorithm throws a catchable `\ValueError` with PHP's message.
#[test]
fn hash_unknown_algorithm_throws_value_error() {
    assert_eq!(
        compile_and_run(r#"<?php try { hash("nope","x"); } catch (\ValueError $e) { echo $e->getMessage(); }"#),
        "hash(): Argument #1 ($algo) must be a valid hashing algorithm"
    );
}

/// Verifies `hash_hmac()` matches PHP's golden HMAC vectors for hex output,
/// that the `$binary=true` raw form bin2hex round-trips to the hex form, and
/// that the raw digest length matches the algorithm's digest size.
#[test]
fn hash_hmac_matches_php() {
    assert_eq!(compile_and_run(r#"<?php echo hash_hmac("sha256","what do ya want for nothing?","Jefe");"#),
        "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843");
    assert_eq!(compile_and_run(r#"<?php echo hash_hmac("sha1","abc","key");"#),
        "4fd0b215276ef12f2b3e4c8ecac2811498b656fc");
    // sha512 has a 128-byte block (vs 64 for sha1/sha256), exercising that HMAC key-schedule path.
    assert_eq!(compile_and_run(r#"<?php echo hash_hmac("sha512","abc","key");"#),
        "3926a207c8c42b0c41792cbd3e1a1aaaf5f7a25704f62dfc939c4987dd7ce060009c5bb1c2447355b3216f10b537e9afa7b64a4e5391b0d631172d07939e087a");
    assert_eq!(compile_and_run(r#"<?php echo bin2hex(hash_hmac("sha256","abc","key",true)) === hash_hmac("sha256","abc","key") ? "1" : "0";"#), "1");
    assert_eq!(compile_and_run(r#"<?php echo strlen(hash_hmac("sha256","abc","key",true));"#), "32");
}

/// Verifies a non-cryptographic checksum algorithm (crc32b) throws a catchable
/// `\ValueError` with PHP's cryptographic-algorithm message — HMAC rejects
/// checksums like crc32/adler/fnv/joaat.
#[test]
fn hash_hmac_rejects_non_crypto_with_value_error() {
    assert_eq!(
        compile_and_run(r#"<?php try { hash_hmac("crc32b","d","k"); } catch (\ValueError $e) { echo $e->getMessage(); }"#),
        "hash_hmac(): Argument #1 ($algo) must be a valid cryptographic hashing algorithm"
    );
}

/// Verifies hash_equals() does a timing-safe byte comparison: equal strings are
/// true, any byte difference is false, and a length mismatch is false.
#[test]
fn hash_equals_timing_safe_compare() {
    assert_eq!(compile_and_run(r#"<?php echo hash_equals("abc","abc") ? "T" : "F";"#), "T");
    assert_eq!(compile_and_run(r#"<?php echo hash_equals("abc","abd") ? "T" : "F";"#), "F");
    assert_eq!(compile_and_run(r#"<?php echo hash_equals("abc","abcd") ? "T" : "F";"#), "F");
    assert_eq!(compile_and_run(r#"<?php echo hash_equals("","") ? "T" : "F";"#), "T");
    assert_eq!(compile_and_run(r#"<?php echo hash_equals("x","") ? "T" : "F";"#), "F");
}

/// Verifies hash_algos() returns the supported-algorithm set: representative
/// members are present, an unsupported PHP algo is absent, and — the key drift
/// guard — every advertised name is actually hashable by hash() (would throw a
/// ValueError if a name were not in the crate's make() table).
#[test]
fn hash_algos_lists_supported_and_each_is_hashable() {
    assert_eq!(compile_and_run(r#"<?php echo in_array("sha256", hash_algos()) ? "1" : "0";"#), "1");
    assert_eq!(compile_and_run(r#"<?php echo in_array("crc32c", hash_algos()) ? "1" : "0";"#), "1");
    assert_eq!(compile_and_run(r#"<?php echo in_array("whirlpool", hash_algos()) ? "1" : "0";"#), "1");
    // tiger is a documented gap — must NOT be advertised
    assert_eq!(compile_and_run(r#"<?php echo in_array("tiger128,3", hash_algos()) ? "1" : "0";"#), "0");
    // Every advertised algorithm must hash without throwing.
    assert_eq!(
        compile_and_run(r#"<?php $ok = 1; foreach (hash_algos() as $a) { if (hash($a, "x") === "") $ok = 0; } echo $ok;"#),
        "1"
    );
}

/// Verifies hash_file() hashes a file's contents (equal to hash() of the same
/// bytes), honors $binary, and returns PHP false for a file that cannot be read.
#[test]
fn hash_file_hashes_contents_and_false_on_missing() {
    assert_eq!(
        compile_and_run(r#"<?php file_put_contents("hf.txt", "hello"); echo hash_file("sha256", "hf.txt");"#),
        "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
    );
    assert_eq!(
        compile_and_run(r#"<?php file_put_contents("hf2.txt", "hello"); echo bin2hex(hash_file("sha256", "hf2.txt", true));"#),
        "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
    );
    assert_eq!(
        compile_and_run(r#"<?php echo hash_file("sha256", "/no/such/elephc/file") === false ? "FALSE" : "STR";"#),
        "FALSE"
    );
}

/// Verifies incremental hashing: hash_init/update/final equals the one-shot
/// digest, the $binary flag is honored, and hash_copy() produces an independent
/// context that diverges from the original after the clone point.
#[test]
fn hash_context_incremental_copy_and_final() {
    assert_eq!(
        compile_and_run(r#"<?php $c = hash_init("sha256"); hash_update($c, "ab"); hash_update($c, "c"); echo hash_final($c);"#),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(
        compile_and_run(r#"<?php $c = hash_init("sha256"); hash_update($c, "abc"); echo strlen(hash_final($c, true));"#),
        "32"
    );
    // hash_copy independence: original continues to "abc", the clone to "aXY".
    assert_eq!(
        compile_and_run(r#"<?php $h = hash_init("sha256"); hash_update($h, "a"); $h2 = hash_copy($h); hash_update($h, "bc"); hash_update($h2, "XY"); echo hash_final($h), "|", hash_final($h2);"#),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad|8411259f736c55dc19cfc1728693503c8e571d2d9ac272bb674636e956f2e49d"
    );
}

/// Verifies hash_init() throws a catchable \ValueError (with PHP's hash_init
/// message) for an unknown algorithm.
#[test]
fn hash_init_unknown_algorithm_throws_value_error() {
    assert_eq!(
        compile_and_run(r#"<?php try { hash_init("definitely-not-an-algo"); } catch (\ValueError $e) { echo $e->getMessage(); }"#),
        "hash_init(): Argument #1 ($algo) must be a valid hashing algorithm"
    );
}

/// Regression: hash()/hash_init() must coerce a Mixed string argument through
/// __rt_mixed_cast_string (like md5()/sha1() always did) instead of pushing the
/// stale string registers left behind by a Mixed-returning call — that garbage
/// (ptr,len) used to reach elephc-crypto and abort in slice::from_raw_parts.
/// `m()` is untyped and receives a bool once, so its inferred return is Mixed.
#[test]
fn hash_family_coerces_mixed_string_args() {
    assert_eq!(
        compile_and_run(r#"<?php function m($s, $r) { echo $s; return $r; } if (m("", false)) { echo "?"; } echo hash("md5", m("b", "x"));"#),
        "b9dd4e461268c8034f5c8564e155c67a6"
    );
    assert_eq!(
        compile_and_run(r#"<?php function m($s, $r) { echo $s; return $r; } if (m("", false)) { echo "?"; } $c = hash_init(m("", "sha256")); hash_update($c, "abc"); echo hash_final($c);"#),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

/// Regression: hash()/hash_hmac()/hash_file() evaluate arguments in PHP source
/// order (the echoed labels) even when every argument is a side-effecting call,
/// and Mixed values land correctly in each slot (algo/data/key strings cast via
/// __rt_mixed_cast_string, the $binary flag via Mixed truthiness).
#[test]
fn hash_family_evaluates_args_in_php_source_order() {
    assert_eq!(
        compile_and_run(r#"<?php function m($s, $r) { echo $s; return $r; } echo hash(m("A", "md5"), m("B", "x"), m("C", false));"#),
        "ABC9dd4e461268c8034f5c8564e155c67a6"
    );
    assert_eq!(
        compile_and_run(r#"<?php function m($s, $r) { echo $s; return $r; } echo hash_hmac(m("A", "sha256"), m("B", "data"), m("C", "key"), m("D", false));"#),
        "ABCD5031fe3d989c6d1537a013fa6e739da23463fdaec3b70137d828e36ace221bd0"
    );
    assert_eq!(
        compile_and_run(r#"<?php function m($s, $r) { echo $s; return $r; } file_put_contents("hfm.txt", "x"); echo hash_file(m("A", "md5"), m("B", "hfm.txt"), m("C", false));"#),
        "ABC9dd4e461268c8034f5c8564e155c67a6"
    );
}

/// Verifies hash() coerces non-string scalar data like PHP does: an int data
/// argument hashes its decimal string form (hash("md5", 123) == md5("123")).
#[test]
fn hash_coerces_int_data_to_string() {
    assert_eq!(
        compile_and_run(r#"<?php echo hash("md5", 123);"#),
        "202cb962ac59075b964b07152d234b70"
    );
}

/// Verifies `hash()` resolves through PHP's case-insensitive and namespaced builtin lookup.
#[test]
fn hash_is_case_insensitive_and_namespaced() {
    assert_eq!(compile_and_run(r#"<?php echo HASH("md5","abc");"#),
        "900150983cd24fb0d6963f7d28e17f72");
    assert_eq!(compile_and_run(r#"<?php echo \hash("md5","abc");"#),
        "900150983cd24fb0d6963f7d28e17f72");
}

/// Verifies `md5()` and `sha1()` keep byte-for-byte hex parity after routing
/// through the elephc-crypto path, and that the optional `$binary` flag now
/// returns the raw digest bytes (16 for md5, 20 for sha1) instead of being
/// silently ignored.
#[test]
fn md5_sha1_parity_and_binary() {
    assert_eq!(compile_and_run(r#"<?php echo md5("abc");"#), "900150983cd24fb0d6963f7d28e17f72");
    assert_eq!(compile_and_run(r#"<?php echo sha1("abc");"#), "a9993e364706816aba3e25717850c26c9cd0d89d");
    assert_eq!(compile_and_run(r#"<?php echo md5("");"#), "d41d8cd98f00b204e9800998ecf8427e");
    assert_eq!(compile_and_run(r#"<?php echo bin2hex(md5("abc",true));"#), "900150983cd24fb0d6963f7d28e17f72");
    assert_eq!(compile_and_run(r#"<?php echo strlen(md5("abc",true));"#), "16");
    assert_eq!(compile_and_run(r#"<?php echo bin2hex(sha1("abc",true));"#), "a9993e364706816aba3e25717850c26c9cd0d89d");
    assert_eq!(compile_and_run(r#"<?php echo strlen(sha1("abc",true));"#), "20");
}

// --- sscanf() ---
