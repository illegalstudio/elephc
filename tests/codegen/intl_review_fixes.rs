//! Purpose:
//! Regression coverage for the confirmed findings of the ext/intl compliance review.
//!
//! Called from:
//! - `cargo test` through the codegen integration harness.
//!
//! Key details:
//! - Non-Intl fixes (literal `str_replace` order, `strtotime` base, `range` folding,
//!   `defined()` membership, `ReflectionClass` constructor visibility, `html_entity_decode`
//!   flags) run without the managed ICU package.
//! - ICU-backed fixtures skip locally when the managed `icu` archive set is missing, matching
//!   the existing Intl corpus harness.
//! - The large-result fixture raises the heap ceiling because the payload is intentionally
//!   above the bridge's 1 MiB fixed buffer.

use crate::support::*;

/// Skips one ICU-backed fixture when the managed package is unavailable on this host.
fn skip_without_intl(name: &str) -> bool {
    if !intl_native::host_supported() {
        return true;
    }
    intl_native::skip(name)
}

/// Verifies a literal `str_replace()` search array applies its entries in source order.
///
/// The optimizer lowers a scalar replacement into nested calls; building them from the back
/// of the array made the last entry run first, so `["a", "ab"]` replaced `ab` before `a`.
#[test]
fn test_review_str_replace_literal_search_array_keeps_source_order() {
    let out = compile_and_run(
        r#"<?php
var_dump(str_replace(["a", "ab"], "x", "ab"));
var_dump(str_replace(["ab", "a"], "x", "ab"));
"#,
    );
    assert_eq!(out, "string(2) \"xb\"\nstring(1) \"x\"\n");
}

/// Verifies `strtotime()` honours the optional base timestamp through both paths.
///
/// `$argc - 1` keeps the base runtime-unknown, while the second call exercises the literal
/// fold that rewrites a trailing zone token into the synthetic `DateTime` helper.
#[test]
fn test_review_strtotime_keeps_base_timestamp() {
    let out = compile_and_run(
        r#"<?php
$base = $argc - 1;
var_dump(strtotime("+1 day UTC", $base));
var_dump(strtotime("+1 day UTC", 0));
var_dump(strtotime("2025-01-01 00:00:00 UTC", $base));
"#,
    );
    assert_eq!(out, "int(86400)\nint(86400)\nint(1735689600)\n");
}

/// Verifies large literal `range()` calls fall back to the runtime instead of inflating the AST.
#[test]
fn test_review_range_literal_cap_preserves_result() {
    let out = compile_and_run(
        r#"<?php
var_dump(count(range(1, 300)));
var_dump(count(range(1, 256)));
var_dump(implode(",", range(1, 3)));
"#,
    );
    assert_eq!(out, "int(300)\nint(256)\nstring(5) \"1,2,3\"\n");
}

/// Verifies `defined()` reports only real Intl constants, including the `LC_*` family.
///
/// The previous prefix check answered true for any `INTL_*`/`IDNA_*`/`LC_*` spelling.
#[test]
fn test_review_defined_uses_exact_intl_constant_membership() {
    let out = compile_and_run(
        r#"<?php
var_dump(defined("INTL_MADE_UP"));
var_dump(defined("INTL_MAX_LOCALE_LEN"));
var_dump(defined("LC_ALL"));
var_dump(defined("LC_MADE_UP"));
"#,
    );
    assert_eq!(out, "bool(false)\nbool(true)\nbool(true)\nbool(false)\n");
}

/// Verifies an `ini_set("intl.*")` literal alone pulls in the bridge-backed declarations.
#[test]
fn test_review_intl_ini_set_alone_links_the_bridge() {
    let out = compile_and_run(
        r#"<?php
ini_set("intl.use_exceptions", "1");
echo "ok\n";
"#,
    );
    assert_eq!(out, "ok\n");
}

/// Verifies `ini_set("intl.default_locale")` updates the locale Intl objects observe.
#[test]
fn test_review_intl_ini_set_default_locale() {
    if skip_without_intl("test_review_intl_ini_set_default_locale") { return; }
    let out = compile_and_run(
        r#"<?php
var_dump(ini_set("intl.default_locale", "fr_FR"));
var_dump(locale_get_default());
"#,
    );
    assert_eq!(out, "string(0) \"\"\nstring(5) \"fr_FR\"\n");
}

/// Verifies reflected non-public constructors throw `ReflectionException` instead of running.
#[test]
fn test_review_reflection_class_rejects_non_public_constructors() {
    let out = compile_and_run(
        r#"<?php
class ProtectedCtor { protected function __construct() {} }
class PrivateCtor { private function __construct() {} }
class ProtectedBase { protected function __construct(public int $n = 1) {} }
class ProtectedChild extends ProtectedBase {}
class PrivateBase { private function __construct(public int $n = 2) {} }
class PrivateChild extends PrivateBase {}
foreach (["ProtectedCtor", "PrivateCtor", "ProtectedChild"] as $name) {
    try {
        (new ReflectionClass($name))->newInstanceArgs([]);
        echo "$name constructed\n";
    } catch (ReflectionException $error) {
        echo get_class($error), ": ", $error->getMessage(), "\n";
    }
}
// An inherited private constructor is not callable from the descendant; the AOT
// constructor-owner walk keeps constructing the descendant without calling it.
echo (new ReflectionClass("PrivateChild"))->newInstance()->n, "\n";
"#,
    );
    assert_eq!(
        out,
        "ReflectionException: Access to non-public constructor of class ProtectedCtor\n\
         ReflectionException: Access to non-public constructor of class PrivateCtor\n\
         ReflectionException: Access to non-public constructor of class ProtectedChild\n\
         2\n"
    );
}

/// Verifies the decoder applies the default quote policy for its supported entities.
#[test]
fn test_review_html_entity_decode_default_and_ent_quotes() {
    let out = compile_and_run(
        r#"<?php
var_dump(html_entity_decode("&quot;x&quot;"));
var_dump(html_entity_decode("&quot;x&quot;", ENT_QUOTES));
var_dump(html_entity_decode("&lt;a&gt;&amp;", ENT_QUOTES, "UTF-8"));
var_dump(html_entity_decode("&#65;&#x42;", ENT_QUOTES));
var_dump(html_entity_decode("&#67;", ENT_QUOTES | ENT_SUBSTITUTE));
var_dump(bin2hex(html_entity_decode("&#x200D;", ENT_NOQUOTES, "UTF-8")));
"#,
    );
    assert_eq!(
        out,
        "string(3) \"\"x\"\"\nstring(3) \"\"x\"\"\nstring(4) \"<a>&\"\nstring(2) \"AB\"\nstring(1) \"C\"\nstring(6) \"e2808d\"\n"
    );
}

/// Verifies `IntlDateFormatter::localtime()` reports the formatter zone's DST state.
#[test]
fn test_review_intl_localtime_reports_dst() {
    if skip_without_intl("test_review_intl_localtime_reports_dst") { return; }
    let out = compile_and_run(
        r#"<?php
$formatter = new IntlDateFormatter(
    "en_US",
    IntlDateFormatter::NONE,
    IntlDateFormatter::NONE,
    "America/Los_Angeles",
    IntlDateFormatter::GREGORIAN,
    "yyyy-MM-dd HH:mm:ss"
);
var_dump($formatter->localtime("2025-07-15 12:00:00")["tm_isdst"]);
var_dump($formatter->localtime("2025-01-15 12:00:00")["tm_isdst"]);
"#,
    );
    assert_eq!(out, "int(1)\nint(0)\n");
}

/// Verifies `UConverter` leaves legitimate `0x1A` bytes intact in multi-byte targets.
///
/// The shim used to rewrite every `0x1A` byte with the configured substitution text, which
/// corrupted UTF-16 output such as `1A 01`.
#[test]
fn test_review_uconverter_keeps_utf16_substitution_byte() {
    if skip_without_intl("test_review_uconverter_keeps_utf16_substitution_byte") { return; }
    let out = compile_and_run(
        r#"<?php
$with_bom = new UConverter("UTF-16", "UTF-8");
echo bin2hex($with_bom->convert("\xC4\x9A")), "\n";
$little_endian = new UConverter("UTF-16LE", "UTF-8");
echo bin2hex($little_endian->convert("a\x1Ab")), "\n";
"#,
    );
    assert_eq!(out, "fffe1a01\n61001a006200\n");
}

/// Verifies dropping Intl wrapper objects releases their bridge handles.
#[test]
fn test_review_intl_handles_released_on_unset_and_clone() {
    if skip_without_intl("test_review_intl_handles_released_on_unset_and_clone") { return; }
    let out = compile_and_run(
        r#"<?php
$before = elephc_intl_live_count();
$formatter = new IntlDateFormatter("en_US", IntlDateFormatter::FULL, IntlDateFormatter::FULL);
var_dump(elephc_intl_live_count() - $before);
$clone = clone $formatter;
var_dump(elephc_intl_live_count() - $before);
unset($formatter);
var_dump(elephc_intl_live_count() - $before);
unset($clone);
var_dump(elephc_intl_live_count() - $before);
"#,
    );
    assert_eq!(out, "int(1)\nint(2)\nint(1)\nint(0)\n");
}

/// Verifies a bridge result above the fixed 1 MiB buffer is fetched intact.
#[test]
fn test_review_intl_large_bridge_result_is_not_truncated() {
    if skip_without_intl("test_review_intl_large_bridge_result_is_not_truncated") { return; }
    let out = compile_and_run_with_heap_size(
        r#"<?php
$transliterator = Transliterator::create("Any-Latin");
$input = str_repeat("a", 1100000);
$output = $transliterator->transliterate($input);
var_dump(strlen($output));
var_dump(substr($output, 0, 3));
"#,
        64 * 1024 * 1024,
    );
    assert_eq!(out, "int(1100000)\nstring(3) \"aaa\"\n");
}

/// Verifies `IntlIterator::current()`/`key()` stay in bounds after iteration ends.
#[test]
fn test_review_intl_iterator_reads_stay_in_bounds() {
    if skip_without_intl("test_review_intl_iterator_reads_stay_in_bounds") { return; }
    let out = compile_and_run(
        r#"<?php
$iterator = IntlTimeZone::createEnumeration();
$iterator->rewind();
while ($iterator->valid()) {
    $iterator->next();
}
var_dump($iterator->valid());
var_dump($iterator->current());
var_dump($iterator->key());
"#,
    );
    assert_eq!(out, "bool(false)\nNULL\nNULL\n");
}

/// Verifies `locale_set_default("")` restores the captured system default.
#[test]
fn test_review_intl_locale_set_default_empty_resets_to_system() {
    if skip_without_intl("test_review_intl_locale_set_default_empty_resets_to_system") { return; }
    let out = compile_and_run(
        r#"<?php
$system = locale_get_default();
locale_set_default("fr_FR");
var_dump(locale_get_default());
locale_set_default("");
var_dump(locale_get_default() === $system);
"#,
    );
    assert_eq!(out, "string(5) \"fr_FR\"\nbool(true)\n");
}

/// Verifies a static callable arity guard evaluates arguments before raising PHP's error.
///
/// PHP runs the argument list first, then validates arity; the direct-callable path used to
/// throw before any argument was lowered, dropping side effects. A caught throw clears the
/// compiler's flow-sensitive callable facts at the try join, so each case is its own program.
#[test]
fn test_review_static_callable_arity_guard_keeps_argument_side_effects() {
    let out = compile_and_run(
        r#"<?php
function side(): int { echo "side|"; return 1; }
$callable = "strlen";
try {
    $callable(side(), side());
} catch (ArgumentCountError $error) {
    echo $error->getMessage(), "\n";
}
"#,
    );
    assert_eq!(out, "side|side|strlen() expects exactly 1 argument, 2 given\n");
}

/// Verifies a static callable type guard says what PHP says after evaluating the argument.
#[test]
fn test_review_static_callable_type_guard_uses_php_message() {
    let out = compile_and_run(
        r#"<?php
$object = new stdClass();
$callable = "strlen";
try {
    $callable($object);
} catch (TypeError $error) {
    echo $error->getMessage(), "\n";
}
"#,
    );
    assert_eq!(
        out,
        "strlen(): Argument #1 ($string) must be of type string, stdClass given\n"
    );
}

/// Verifies `NumberFormatter::parse()` defaults to TYPE_DOUBLE and keeps float attributes.
#[test]
fn test_review_number_formatter_parse_defaults_and_float_attributes() {
    if skip_without_intl("test_review_number_formatter_parse_defaults_and_float_attributes") { return; }
    let out = compile_and_run(
        r#"<?php
$formatter = new NumberFormatter("en_US", NumberFormatter::DECIMAL);
var_dump($formatter->parse("123"));
var_dump($formatter->parse("1.5"));
var_dump($formatter->parse("123", NumberFormatter::TYPE_INT64));
try {
    $formatter->parse("123", 0);
} catch (ValueError $error) {
    echo $error->getMessage(), "\n";
}
var_dump($formatter->setAttribute(NumberFormatter::ROUNDING_INCREMENT, 0.5));
var_dump($formatter->getAttribute(NumberFormatter::ROUNDING_INCREMENT));
var_dump($formatter->getAttribute(NumberFormatter::MULTIPLIER));
"#,
    );
    assert_eq!(
        out,
        "float(123)\nfloat(1.5)\nint(123)\n\
         NumberFormatter::parse(): Argument #2 ($type) must be a NumberFormatter::TYPE_* constant\n\
         bool(true)\nfloat(0.5)\nint(1)\n"
    );
}

/// Verifies unpublished normalization forms throw instead of silently becoming NFC.
#[test]
fn test_review_normalizer_rejects_invalid_forms() {
    if skip_without_intl("test_review_normalizer_rejects_invalid_forms") { return; }
    let out = compile_and_run(
        r#"<?php
try {
    Normalizer::normalize("a", 99);
} catch (ValueError $error) {
    echo $error->getMessage(), "\n";
}
try {
    normalizer_normalize("a", 99);
} catch (ValueError $error) {
    echo $error->getMessage(), "\n";
}
var_dump(Normalizer::getRawDecomposition("a", 99));
var_dump(Normalizer::isNormalized("ABC"));
var_dump(normalizer_is_normalized("ABC"));
var_dump(Normalizer::isNormalized("A\xCC\x8A"));
var_dump(normalizer_is_normalized("A\xCC\x8A"));
"#,
    );
    assert_eq!(
        out,
        "Normalizer::normalize(): Argument #2 ($form) must be a a valid normalization form\n\
         normalizer_normalize(): Argument #2 ($form) must be a a valid normalization form\n\
         string(0) \"\"\n\
         bool(true)\nbool(true)\nbool(false)\nbool(false)\n"
    );
}

/// Verifies `IntlPartsIterator::getRuleStatus()` reports the current part's rule status.
#[test]
fn test_review_parts_iterator_rule_status() {
    if skip_without_intl("test_review_parts_iterator_rule_status") { return; }
    let out = compile_and_run(
        r#"<?php
$parts = IntlBreakIterator::createWordInstance("en")->getPartsIterator();
$parts->getBreakIterator()->setText("Hello world 42");
foreach ($parts as $part) {
    echo "[", $part, "]=", $parts->getRuleStatus(), "\n";
}
"#,
    );
    assert_eq!(out, "[Hello]=200\n[ ]=0\n[world]=200\n[ ]=0\n[42]=100\n");
}

/// Verifies the procedural wall-time setters reject invalid options with PHP's ValueError.
#[test]
fn test_review_procedural_wall_time_option_validation() {
    if skip_without_intl("test_review_procedural_wall_time_option_validation") { return; }
    let out = compile_and_run(
        r#"<?php
$calendar = IntlCalendar::createInstance("UTC");
try {
    intlcal_set_repeated_wall_time_option($calendar, 5);
} catch (ValueError $error) {
    echo $error->getMessage(), "\n";
}
try {
    intlcal_set_skipped_wall_time_option($calendar, -1);
} catch (ValueError $error) {
    echo $error->getMessage(), "\n";
}
"#,
    );
    assert_eq!(
        out,
        "intlcal_set_repeated_wall_time_option(): Argument #2 ($option) must be either \
         IntlCalendar::WALLTIME_FIRST or IntlCalendar::WALLTIME_LAST\n\
         intlcal_set_skipped_wall_time_option(): Argument #2 ($option) must be one of \
         IntlCalendar::WALLTIME_FIRST, IntlCalendar::WALLTIME_LAST, or IntlCalendar::WALLTIME_NEXT_VALID\n"
    );
}

/// Verifies a dynamic Intl callable string still requests the Intl bridge surface.
#[test]
fn test_review_dynamic_intl_callable_links_the_bridge() {
    if skip_without_intl("test_review_dynamic_intl_callable_links_the_bridge") { return; }
    let out = compile_and_run(
        r#"<?php
$callable = "grapheme_strlen";
var_dump($callable("abc"));
"#,
    );
    assert_eq!(out, "int(3)\n");
}
