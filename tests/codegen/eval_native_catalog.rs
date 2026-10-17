//! Purpose:
//! Checks repeated and reentrant native eval activations using a shared catalog.
//!
//! Called from:
//! - The codegen integration test harness.
//!
//! Key details:
//! - Dynamic fragments retain fresh scope and defaults in both IR optimizer modes.
//! - Heap diagnostics cover runtime ownership while PHP supplies the output oracle.

use crate::support::*;

/// Checks dynamic eval semantics and runtime ownership in both optimizer modes.
fn check_catalog_fixture(source: &str, expected: &str, check_heap: bool) {
    for optimized in [false, true] {
        let output = if optimized {
            compile_and_run_with_heap_debug(source)
        } else {
            without_ir_opt(|| compile_and_run_with_heap_debug(source))
        };
        assert!(output.success, "{}", output.stderr);
        assert_eq!(output.stdout, expected);
        if check_heap {
            assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
        }
    }
    if std::env::var_os("ELEPHC_PHP_CHECK").is_some() {
        let php = Command::new("php").args(["-n", "-r", &source[6..]])
            .output().expect("run PHP catalog oracle");
        assert!(php.status.success(), "{}", String::from_utf8_lossy(&php.stderr));
        assert_eq!(php.stdout, expected.as_bytes());
    }
}

/// Scope writes stay activation-local, including partial writes before a throw.
#[test]
fn test_eval_native_catalog_fresh_scope_and_exception_writeback() {
    let source = r#"<?php
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
"#;
    check_catalog_fixture(source, "caught:3:1|caught:3:1|caught:3:1|", true);
}

/// Reentrant native callbacks create independent contexts while sharing the module catalog.
#[test]
fn test_eval_native_catalog_reentrant_native_callbacks() {
    let source = r#"<?php
function catalogInner(string $code): int { $value = 10; eval($code); return $value; }
function catalogOuter(string $code): int { $value = 1; eval($code); return $value; }
$code = $argc > 0 ? '$value += catalogInner("\$value += 2;");' : '';
echo catalogOuter($code), '|', catalogOuter($code), '|', catalogInner('$value += 3;');
"#;
    check_catalog_fixture(source, "13|13|13", true);
}

/// Shared signature descriptions never cache materialized arrays or constructor defaults.
#[test]
fn test_eval_native_catalog_defaults_and_reflection() {
    let source = r#"<?php
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
"#;
    // This reflection/default fixture already retains runtime cells on the baseline.
    // Dedicated scope tests and catalog unit tests cover ownership without freezing cells.
    check_catalog_fixture(source, "2:8:box!:0|2:8:box!:0|", false);
}

/// Cached member descriptions do not carry the access scope of their first activation.
#[test]
fn test_eval_native_catalog_visibility_uses_the_current_activation() {
    let source = r#"<?php
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
"#;
    check_catalog_fixture(source, "allowed|denied|allowed|denied|", true);
}

/// Local aliases stay fresh while native function statics retain PHP's process lifetime.
#[test]
fn test_eval_native_catalog_local_aliases_and_native_statics() {
    let source = r#"<?php
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
"#;
    check_catalog_fixture(source, "1:1|2:2|3:3|", true);
}
