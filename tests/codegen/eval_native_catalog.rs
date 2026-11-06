//! Purpose:
//! Checks repeated and reentrant native eval activations using a shared catalog.
//!
//! Called from:
//! - The codegen integration test harness.
//!
//! Key details:
//! - Dynamic fragments retain fresh scope and defaults in both IR optimizer modes.
//! - Heap diagnostics cover runtime ownership while PHP supplies the output oracle.
//! - The fixtures are opaque-eval programs, each of which lowers the whole builtin catalog, so
//!   they run as two bundles (one heap-checked, one not) instead of five separate compiles per
//!   optimizer mode; every case keeps its own name and expected output.

use crate::support::*;

/// Runs one bundle of dynamic-eval fixtures in both optimizer modes with heap diagnostics,
/// then checks each case against PHP when the oracle is enabled.
fn check_catalog_bundle(cases: &[BundleCase<'_>], check_heap: bool) {
    for optimized in [false, true] {
        assert_bundle_with(cases, |source| {
            let output = if optimized {
                compile_and_run_with_heap_debug(source)
            } else {
                without_ir_opt(|| compile_and_run_with_heap_debug(source))
            };
            assert!(output.success, "{}", output.stderr);
            if check_heap {
                assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
            }
            output.stdout
        });
    }
    if std::env::var_os("ELEPHC_PHP_CHECK").is_some() {
        for case in cases {
            let php = Command::new("php").args(["-n", "-r", &case.source[6..]])
                .output().expect("run PHP catalog oracle");
            assert!(php.status.success(), "{}: {}", case.name, String::from_utf8_lossy(&php.stderr));
            assert_eq!(php.stdout, case.expected.as_bytes(), "{}", case.name);
        }
    }
}

/// Scope writes stay activation-local (including partial writes before a throw), reentrant
/// native callbacks get independent contexts over the shared catalog, cached member
/// descriptions carry no access scope, and local aliases stay fresh while function statics
/// keep PHP's process lifetime. Every case leaves the heap clean.
#[test]
fn test_eval_native_catalog_activations_bundle() {
    check_catalog_bundle(
        &[
            BundleCase {
                name: "fresh_scope_and_exception_writeback",
                source: r#"<?php
function catalogActivation(string $code): void {
    $local = 1;
    $number = 0;
    try { eval($code); }
    catch (RuntimeException $e) { echo $e->getMessage(), ':'; }
    echo $local, ':', $number, '|';
}
$code = $argc > 0 ? '$local += 2; $number += 1; throw new RuntimeException("caught");' : '';
catalogActivation($code);
catalogActivation($code);
catalogActivation($code);
"#,
                expected: "caught:3:1|caught:3:1|caught:3:1|",
            },
            BundleCase {
                name: "reentrant_native_callbacks",
                source: r#"<?php
function catalogInner(string $code): int { $value = 10; eval($code); return $value; }
function catalogOuter(string $code): int { $value = 1; eval($code); return $value; }
$code = $argc > 0 ? '$value += catalogInner("\$value += 2;");' : '';
echo catalogOuter($code), '|', catalogOuter($code), '|', catalogInner('$value += 3;');
"#,
                expected: "13|13|13",
            },
            BundleCase {
                name: "visibility_uses_the_current_activation",
                source: r#"<?php
class CatalogAccess {
    private function secret(): string { return "allowed"; }
    public function probe(string $code): void { eval($code); }
}
function catalogGlobalProbe(string $code): void { eval($code); }
$inside = $argc > 0 ? 'echo $this->secret(), "|";' : '';
$outside = $argc > 0 ? 'try { echo (new CatalogAccess())->secret(); } catch (Error $e) { echo "denied|"; }' : '';
$box = new CatalogAccess();
$box->probe($inside);
catalogGlobalProbe($outside);
$box->probe($inside);
catalogGlobalProbe($outside);
"#,
                expected: "allowed|denied|allowed|denied|",
            },
            BundleCase {
                name: "local_aliases_and_native_statics",
                source: r#"<?php
function catalogNext(): int { static $next = 0; return ++$next; }
function catalogAlias(string $code): void {
    $value = 0;
    $alias =& $value;
    eval($code);
    echo $value, ':', $alias, '|';
}
$code = $argc > 0 ? '$alias += catalogNext();' : '';
catalogAlias($code);
catalogAlias($code);
catalogAlias($code);
"#,
                expected: "1:1|2:2|3:3|",
            },
        ],
        true,
    );
}

/// Shared signature descriptions never cache materialized arrays or constructor defaults.
///
/// Kept out of the heap-checked bundle: this reflection/default fixture already retains runtime
/// cells on the baseline, and a bundle's leak summary covers every case it contains. Dedicated
/// scope tests and catalog unit tests cover ownership without freezing cells.
#[test]
fn test_eval_native_catalog_defaults_and_reflection() {
    check_catalog_bundle(
        &[BundleCase {
            name: "defaults_and_reflection",
            source: r#"<?php
class CatalogBox {
    public int $value;
    public function __construct(int $value = 7) { $this->value = $value; }
    public function label(string $suffix = "!"): string { return "box" . $suffix; }
}
function catalogArray(array $items = [1]): int { $items[] = 2; return count($items); }
function catalogObject(CatalogBox $box = new CatalogBox(7)): int { $box->value++; return $box->value; }
function catalogDefaults(string $code): void { eval($code); }
$code = $argc > 0 ? 'echo catalogArray(), ":", catalogObject(), ":", (new CatalogBox())->label(), ":", (new ReflectionFunction("catalogArray"))->getNumberOfRequiredParameters(), "|";' : '';
catalogDefaults($code);
catalogDefaults($code);
"#,
            expected: "2:8:box!:0|2:8:box!:0|",
        }],
        false,
    );
}
