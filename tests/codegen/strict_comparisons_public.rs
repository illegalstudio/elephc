//! Purpose:
//! Verifies noncoercive comparisons through public compiler, callable and runtime entry points.
//!
//! Called from:
//! - Comparisons admission checks after native return and optimizer integration.
//!
//! Key details:
//! - Both optimizer modes retain original kinds, catchable failures and independent flags.

use crate::support::*;

/// Source-known violations are rejected before folding without enabling unrelated strict features.
#[test]
fn test_strict_comparisons_public_source_evidence_and_independence() {
    for source in ["<?php echo 1 == '1';", "<?php if (false) { echo 1 != 2; }",
        "<?php echo 1 <> 2;", "<?php echo 1 < 2.0;", "<?php echo '1' < 2;",
        "<?php $items = [1]; sort($items);", "<?php in_array(1, [1]);",
        "<?php max(1, 2);", "<?php array_unique([1, 1]);",
        "<?php function compareGmp(GMP $left, GMP $right): bool { return $left < $right; }",
        "<?php function compareDates(DateTimeImmutable $left, DateTimeImmutable $right): bool { return $left < $right; }",
        "<?php function compareXml(SimpleXMLElement $left, SimpleXMLElement $right): bool { return $left < $right; }"]
    {
        StrictFixture::new(source).assert_error(&["--strict-comparisons"], "strict-comparisons");
    }
    assert_eq!(StrictFixture::new("<?php function comparisonWeak(float $value): float { return $value; } \
        echo is_int(6 / 2) ? 'I' : 'X', '|', (int)'12tail', '|', comparisonWeak(1), '|'; \
        if (1) { echo 'B'; }").run(&["--strict-comparisons"]), "I|12|1|B");
    assert_eq!(StrictFixture::new("<?php echo 1 == '1' ? 'E' : 'X';")
        .run(&["--strict-comparisons", "--no-strict-comparisons"]), "E");
}

/// Public native and opaque consumers agree on original scalar kinds and comparator ownership.
#[test]
fn test_strict_comparisons_public_native_and_eval_domains_bundle() {
    let native = include_str!("../fixtures/strict/comparator_native_results.php");
    let opaque = include_str!("../fixtures/strict/comparator_checked_results.php")
        .trim_start_matches("<?php").replace("$items", "$opaqueCmpItems");
    let scalar = r#"
function publicOrder(mixed $left, mixed $right): void { echo $left <=> $right; }
class PublicOrderText { public function __toString(): string { echo 'X'; return '10'; } }
function publicSwitch(mixed $value): void {
    switch ($value) { case '1': echo 'S'; break; case 1: echo 'I'; break; default: echo 'D'; }
}
echo '|'; publicOrder('10', '9'); echo ';';
try { publicOrder(1, 1.0); } catch (TypeError $orderA) { echo 'T'; unset($orderA); }
try { publicOrder('1', 1); } catch (TypeError $orderB) { echo 'S'; unset($orderB); }
try { publicOrder(new PublicOrderText(), new PublicOrderText()); }
catch (TypeError $orderC) { echo 'O'; unset($orderC); }
echo '|'; publicSwitch(1); publicSwitch('1');
echo in_array(1, ['1'], true) ? 'X' : 'N'; echo '|';
"#;
    let source = format!("{native}\n{scalar}\n{opaque}\ngc_collect_cycles();");
    let expected = concat!("I1,2;M1,2;C1,2;N1,2;A11;BFSDOCDRNUDV", "|-1;TSO|ISN|",
        "1,2;1,2;1,2;B;DV;DK;2,1");
    for mode in ["--ir-opt=off", "--ir-opt=on"] {
        let output = StrictFixture::new(&source).capture(&["--strict-comparisons", mode, "--heap-debug"]);
        assert_eq!(String::from_utf8_lossy(&output.stdout), expected, "{mode}: {}",
            String::from_utf8_lossy(&output.stderr));
        assert!(String::from_utf8_lossy(&output.stderr).contains("HEAP DEBUG: leak summary: clean"),
            "{mode}: {}", String::from_utf8_lossy(&output.stderr));
    }
}

/// Original-kind guards reach user callables, builtin aliases, Reflection and eval re-entry.
#[test]
fn test_strict_comparisons_public_callable_hosts_bundle() {
    let source = include_str!("../fixtures/strict/comparison_hosts.php");
    for mode in ["--ir-opt=off", "--ir-opt=on"] {
        let output = StrictFixture::new(source).capture(&["--strict-comparisons", mode, "--heap-debug"]);
        assert_eq!(String::from_utf8_lossy(&output.stdout), "ABCDEFGHIJ|NECLRU|LL", "{mode}: {}",
            String::from_utf8_lossy(&output.stderr));
        assert!(String::from_utf8_lossy(&output.stderr).contains("HEAP DEBUG: leak summary: clean"),
            "{mode}: {}", String::from_utf8_lossy(&output.stderr));
    }
}

/// Public comparison and callback contracts emit for all supported standalone-library targets.
#[test]
fn test_strict_comparisons_public_library_emission_all_targets() {
    let fixture = StrictFixture::new("<?php \
        function libraryComparator(mixed $left, mixed $right): mixed { return $left <=> $right; } \
        function libraryOrder(array $values): array { usort($values, libraryComparator(...)); return $values; } \
        function rawLibraryOrder(): array { $values = [2, 1]; $sort = usort(...); \
            $sort($values, libraryComparator(...)); return $values; } \
        function boxedLibraryOrder(): array { $sort = usort(...); $values = ['b', 'a']; \
            try { throw new Exception('flow'); } catch (Exception $error) { unset($error); } \
            $sort($values, libraryComparator(...)); return $values; } \
        class LibraryMethodOrder { \
            public function compare(mixed $left, mixed $right): mixed { return $left <=> $right; } \
            public static function staticCompare(mixed $left, mixed $right): mixed { return $left <=> $right; } } \
        function methodLibraryOrder(): array { $values = [2, 1]; $sorter = new LibraryMethodOrder(); \
            usort($values, $sorter->compare(...)); return $values; } \
        function staticLibraryOrder(): array { $values = ['b', 'a']; \
            usort($values, LibraryMethodOrder::staticCompare(...)); return $values; }");
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        fixture.assert_compiles(&["--strict-comparisons", "--emit-asm", "--emit", "staticlib", "--target", name]);
    }
}

/// The runnable label example uses bytewise ordering and explicit identity membership.
#[test]
fn test_strict_comparisons_public_label_example() {
    let output = StrictFixture::new(include_str!("../../examples/strict-comparisons/main.php"))
        .capture(&["--strict-comparisons", "--heap-debug"]);
    assert_eq!(String::from_utf8_lossy(&output.stdout), concat!(
        "Label with a preserved leading zero: 02\nLabel: 10\nLabel: 9\nExact label found: 02\n"));
    assert!(String::from_utf8_lossy(&output.stderr).contains("HEAP DEBUG: leak summary: clean"),
        "{}", String::from_utf8_lossy(&output.stderr));
}
