//! Purpose:
//! Executes strict concatenation and intentional formatting through the native backend.
//!
//! Called from:
//! - Focused staged strict-string integration tests.
//!
//! Key details:
//! - Every optimizer combination preserves kind failures, source effects and operand owners.

use crate::support::*;
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Interpolated Stringable failures retain their catch path with or without strict strings.
#[test]
fn test_strict_string_native_interpolation_stringable_throw_controls() {
    let source = r#"<?php
class InterpolationThrowText {
    public function __toString(): string { throw new DomainException('format'); }
    public function __destruct() { echo 'D'; }
}
$value = new InterpolationThrowText();
try { echo "[$value]"; echo 'X'; }
catch (DomainException $error) { echo 'T'; unset($error); }
unset($value);
gc_collect_cycles();
"#;
    for selected in [false, true] {
        for (ast, ir) in [(false, false), (true, false), (false, true), (true, true)] {
            let output = StrictFixture::new(source).capture_staged_optimizers(
                SemanticProfile::PERMISSIVE.with(StrictFeature::Strings, selected), ast, ir);
            assert!(output.success, "strict={selected} ast={ast} ir={ir}: {} {} {}", output.status, output.stdout, output.stderr);
            assert_eq!(output.stdout, "TD", "strict={selected} ast={ast} ir={ir}: {}", output.stderr);
            assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
        }
    }
}

/// Print retires temporary Stringable objects and owned strings without a semantic profile.
#[test]
fn test_strict_string_native_print_permissive_owners() {
    let source = r#"<?php
class PrintOwnerText {
    public function __toString(): string { return 'v'; }
    public function __destruct() { echo 'D'; }
}
function printOwnedText(int $count): string { return 'owned-' . strval($count); }
print new PrintOwnerText();
print printOwnedText($argc);
gc_collect_cycles();
"#;
    for optimized in [false, true] {
        let output = StrictFixture::new(source).capture_staged_lowering(SemanticProfile::PERMISSIVE, optimized);
        assert!(output.success, "opt={optimized}: {} {} {}", output.status, output.stdout, output.stderr);
        assert_eq!(output.stdout, "vDowned-1");
        assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "opt={optimized}: {}", output.stderr);
    }
}

/// The row label example formats numbers explicitly and only concatenates actual strings.
#[test]
fn test_strict_string_native_row_label_example() {
    let source = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("examples/strict-strings/main.php")).unwrap();
    for optimized in [false, true] {
        let output = StrictFixture::new(&source).capture_staged_lowering(
            SemanticProfile::PERMISSIVE.with(StrictFeature::Strings, true), optimized);
        assert!(output.success, "opt={optimized}: {} {} {}", output.status, output.stdout, output.stderr);
        assert_eq!(output.stdout, "Row 3: notebook\nTotal: 7\nNext row: 4\n");
        assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "opt={optimized}: {}", output.stderr);
    }
}

/// Original operands fail before conversion hooks, while output keeps intentional formatting.
#[test]
fn test_strict_string_native_boundaries_and_owners_bundle() {
    let source = r#"<?php
function stringJoin(mixed $left, mixed $right): string { return $left . $right; }
function stringDiscard(mixed $left, mixed $right): void { $left . $right; echo 'X'; }
function stringOutput(mixed $value): void { echo $value; }
function stringPrint(mixed $value): int { return print $value; }
function stringInterpolate(mixed $value): string { return "[$value]"; }
function stringLeft(): mixed { echo 'A'; return 1; }
function stringRight(): mixed { echo 'B'; return 'x'; }
function stringReplace(mixed &$value): string { echo 'R'; $value = 'new'; return 'x'; }
function stringSnapshot(mixed $value): void {
    try { $value . stringReplace($value); echo 'X'; }
    catch (TypeError $error) { echo 'T'; unset($error); }
    echo $value;
}
function stringAppend(mixed &$value, mixed $tail): void { $value .= $tail; }
function stringMaybeArray(int $count): array|int { if ($count > 0) { return [1]; } return 7; }
class StrictStringText {
    public function __toString(): string { echo 'H'; return 'v'; }
    public function __destruct() { echo 'D'; }
}
try { stringJoin(1, 'x'); echo 'X'; } catch (TypeError $error) { echo 'T'; unset($error); }
try { stringJoin('x', false); echo 'X'; } catch (TypeError $error) { echo 'T'; unset($error); }
try { stringJoin(stringLeft(), stringRight()); echo 'X'; } catch (TypeError $error) { echo 'T'; unset($error); }
try { stringJoin(new StrictStringText(), 'x'); echo 'X'; } catch (TypeError $error) { echo 'T'; unset($error); }
try { stringDiscard(1, 'x'); } catch (TypeError $error) { echo 'T'; unset($error); }
try { stringOutput([1]); echo 'X'; } catch (TypeError $error) { echo 'T'; unset($error); }
try { stringPrint([1]); echo 'X'; } catch (TypeError $error) { echo 'T'; unset($error); }
try { stringInterpolate([1]); echo 'X'; } catch (TypeError $error) { echo 'T'; unset($error); }
stringSnapshot(7);
$append = 7;
try { stringAppend($append, 'x'); echo 'X'; } catch (TypeError $error) { echo 'T'; unset($error); }
echo $append;
$append = 'a'; stringAppend($append, 'b'); echo $append;
echo '|', stringJoin('a', 'b'); stringOutput(7); echo stringPrint(false), stringInterpolate(2);
stringOutput(new StrictStringText());
echo '|'; print new StrictStringText();
try { echo stringMaybeArray($argc); echo 'X'; } catch (TypeError $error) { echo 'T'; unset($error); }
gc_collect_cycles();
"#;
    for (ast, ir) in [(false, false), (true, false), (false, true), (true, true)] {
        let output = StrictFixture::new(source).capture_staged_optimizers(
            SemanticProfile::PERMISSIVE.with(StrictFeature::Strings, true), ast, ir);
        assert!(output.success, "ast={ast} ir={ir}: {} {} {}", output.status, output.stdout, output.stderr);
        assert_eq!(output.stdout, "TTABTDTTTTTRTnewT7ab|ab71[2]HvD|HvDT", "ast={ast} ir={ir}: {}", output.stderr);
        assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "ast={ast} ir={ir}: {}", output.stderr);
    }
}
