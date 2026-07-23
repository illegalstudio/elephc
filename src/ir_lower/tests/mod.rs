//! Purpose:
//! Unit coverage for AST-to-EIR lowering and module validation.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Tests run the same frontend/optimizer ordering used by `--emit-ir` and
//!   assert that `lower_program` returns validated printable modules.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::codegen::platform::Target;
use crate::ir::{print_module, Terminator};

mod arrays;
mod corpus;
mod exhaustive;
mod ownership;

/// Runs frontend, type checking, optimization, and EIR lowering for a source string.
fn lower_source(source: &str) -> crate::ir::Module {
    lower_source_at(source, Path::new("main.php"), Path::new("."))
}

/// Runs frontend, type checking, optimization, and EIR lowering for a file.
fn lower_file(path: &Path) -> crate::ir::Module {
    let source = std::fs::read_to_string(path).expect("failed to read PHP fixture");
    let parent = path.parent().unwrap_or(Path::new("."));
    lower_source_at(&source, path, parent)
}

/// Runs the `--emit-ir` frontend ordering for a source string and base path.
fn lower_source_at(source: &str, main_file_path: &Path, parent: &Path) -> crate::ir::Module {
    let target = Target::detect_host();
    let tokens = crate::lexer::tokenize(source).expect("tokenize failed");
    let parsed = crate::parser::parse(&tokens).expect("parse failed");
    let main_file_path = PathBuf::from(main_file_path);
    let parsed = crate::magic_constants::substitute_file_and_scope_constants(parsed, &main_file_path);
    let parsed = crate::conditional::apply(parsed, &HashSet::new());
    let (autoload_registry, parsed) = crate::autoload::Registry::build(parent, parsed);
    let ast = crate::resolver::resolve(parsed, parent).expect("resolver failed");
    let ast = crate::autoload::collect_aliases(ast);
    let ast = crate::pdo_prelude::inject_if_used(ast, false);
    let ast = crate::tz_prelude::inject_if_used(ast, false);
    let ast = crate::list_id_prelude::inject_if_used(ast);
    let ast = crate::var_export_prelude::inject_if_used(ast);
    let ast = crate::image_prelude::inject_if_used(ast, false);
    let ast = crate::name_resolver::resolve(ast).expect("name resolution failed");
    let ast = crate::autoload::run(ast, parent, &autoload_registry).expect("autoload failed");
    let ast = crate::optimize::fold_constants(ast);
    let check_result = crate::types::check_with_target(&ast, target).expect("type check failed");
    let ast = crate::optimize::propagate_constants(ast);
    let ast = crate::optimize::prune_constant_control_flow(ast);
    let ast = crate::optimize::normalize_control_flow(ast);
    let ast = crate::optimize::eliminate_dead_code(ast);
    crate::ir_lower::lower_program(&ast, &check_result, target, false).unwrap_or_else(|error| {
        panic!(
            "EIR lowering failed for {}: {error:?}",
            main_file_path.display()
        )
    })
}

/// Verifies lowering emits valid EIR for functions, arrays, foreach, and loops.
#[test]
fn lowers_control_flow_arrays_and_functions() {
    let module = lower_source(
        r#"<?php
function inc(int $x): int {
    return $x + 1;
}
$items = [1, 2];
$items[] = inc(2);
foreach ($items as $k => $v) {
    echo $v;
}
while (time()) {
    break;
}
"#,
    );
    let text = print_module(&module);
    assert!(text.contains("function inc"), "missing lowered function: {text}");
    assert!(text.contains("function main"), "missing lowered main: {text}");
    assert!(text.contains("array_new"), "missing array construction: {text}");
    assert!(text.contains("iter_start"), "missing foreach iterator: {text}");
}

/// Verifies dead `try.after` joins are terminated as unreachable for heap-returning functions,
/// with and without a `finally` body.
#[test]
fn dead_try_after_joins_are_unreachable() {
    let module = lower_source(
        r#"<?php
final class Conn { public function __construct(public string $dsn) {} }
final class Factory {
    public function create(string $dsn): Conn {
        try { return new Conn($dsn); }
        catch (\Throwable $e) { throw new \RuntimeException('fail'); }
    }
}
final class ArrayFactory {
    public function values(): array {
        try { return [1, 2]; }
        catch (\Throwable $e) { throw new \RuntimeException('fail'); }
        finally { $cleanup = true; }
    }
}
echo (new Factory())->create('pg')->dsn;
echo (new ArrayFactory())->values()[0];
"#,
    );

    let create = module
        .class_methods
        .iter()
        .find(|function| function.name == "Factory::create")
        .expect("missing Factory::create EIR");
    let create_after = create
        .blocks
        .iter()
        .find(|block| block.name == "try.after")
        .expect("missing Factory::create try.after block");
    assert_eq!(create_after.terminator, Some(Terminator::Unreachable));

    let values = module
        .class_methods
        .iter()
        .find(|function| function.name == "ArrayFactory::values")
        .expect("missing ArrayFactory::values EIR");
    let values_after = values
        .blocks
        .iter()
        .find(|block| block.name == "try.after")
        .expect("missing ArrayFactory::values try.after block");
    assert_eq!(values_after.terminator, Some(Terminator::Unreachable));
}

/// Verifies class method declarations are lowered into the class-method table.
#[test]
fn lowers_class_method_bodies() {
    let module = lower_source(
        r#"<?php
class Counter {
    public function value(): int {
        return 7;
    }
}
$counter = new Counter();
echo $counter->value();
"#,
    );
    let text = print_module(&module);
    assert!(
        text.contains("function Counter::value"),
        "missing lowered method body: {text}"
    );
    assert!(text.contains("flags(method)"), "missing method flag: {text}");
}

/// Verifies a native program without Reflection references does not lower the synthetic surface.
#[test]
fn plain_program_omits_unreferenced_builtin_reflection_methods() {
    let module = lower_source("<?php echo 1;");
    assert!(
        module
            .class_methods
            .iter()
            .all(|function| !function.name.starts_with("Reflection")),
        "plain EIR unexpectedly contains builtin Reflection methods"
    );
}

/// Verifies a native ReflectionClass use retains its constructor and called method body.
#[test]
fn native_reflection_program_lowers_reachable_builtin_methods() {
    let module = lower_source(
        r#"<?php
class Plain {}
$reflection = new ReflectionClass('Plain');
echo $reflection->getName();
"#,
    );
    let method_names = module
        .class_methods
        .iter()
        .map(|function| function.name.as_str())
        .collect::<HashSet<_>>();
    assert!(method_names.contains("ReflectionClass::__construct"));
    assert!(method_names.contains("ReflectionClass::getName"));
}

/// Verifies mixed float/integer comparisons coerce both operands before `fcmp`.
#[test]
fn float_comparison_coerces_integer_operand() {
    let module = lower_source("<?php $t = microtime(true); if ($t > 1000000000) { echo \"ok\"; }");
    let text = print_module(&module);
    assert!(text.contains("i_to_f"), "missing integer-to-float coercion in {text}");
    assert!(text.contains("fcmp"), "missing float comparison in {text}");
}

/// Verifies registry-backed `strlen` becomes ordinary EIR instead of a name-based builtin call.
#[test]
fn strlen_uses_backend_neutral_eir_graph() {
    let module = lower_source(
        "<?php function length_of(string $value): int { return strlen($value); } echo length_of('abc');",
    );
    let text = print_module(&module);
    assert!(text.contains("str_len"), "missing string-length EIR operation: {text}");
    assert!(
        !text.contains("builtin_call @strlen"),
        "strlen leaked through the legacy name-based backend boundary: {text}"
    );
}

/// Verifies nested autovivification carries a typed fetch-for-write runtime identity.
#[test]
fn nested_autovivify_uses_typed_fetch_for_write_runtime_call() {
    let module = lower_source(
        "<?php $items = [['x', 'y'], 7]; $items[7][1] = 'patched'; echo $items[7][1];",
    );
    let text = print_module(&module);
    assert!(
        text.contains("runtime.array.fetch_for_write"),
        "missing typed fetch-for-write runtime call: {text}"
    );
}

/// Verifies unary string transforms carry typed runtime identities rather than PHP names.
#[test]
fn unary_string_builtins_use_typed_runtime_calls() {
    let module = lower_source(
        "<?php function transform(string $value): string { return strtolower(urlencode($value)); } echo transform('A B');",
    );
    let text = print_module(&module);
    assert!(
        text.contains("runtime.string.url_encode"),
        "missing typed URL-encode runtime call: {text}"
    );
    assert!(
        text.contains("runtime.string.to_lower"),
        "missing typed lower-case runtime call: {text}"
    );
    assert!(
        !text.contains("builtin_call @urlencode") && !text.contains("builtin_call @strtolower"),
        "unary transform leaked through the legacy PHP-name backend boundary: {text}"
    );
}

/// Verifies positional builtin operands use registry types for explicit EIR coercions.
#[test]
fn unary_string_builtin_coerces_mixed_operand_before_runtime_call() {
    let module = lower_source(
        "<?php $value = json_decode('\"abc\"'); echo strtoupper($value);",
    );
    let text = print_module(&module);
    assert!(
        text.contains("cast") && text.contains("runtime.string.to_upper"),
        "missing Mixed-to-Str coercion before typed runtime call: {text}"
    );
}

/// Verifies descriptor result contracts override checker precision when runtime layouts differ.
#[test]
fn builtin_runtime_calls_use_descriptor_result_representations() {
    let module = lower_source(
        "<?php $encoded = json_encode(INF); $environment = getenv('HOME'); echo $encoded === false; echo strlen($environment);",
    );
    let text = print_module(&module);
    assert!(
        text.lines().any(|line| {
            line.contains("Heap(Mixed) php=mixed") && line.contains("runtime.json_encode")
        }),
        "json_encode must retain its boxed string-or-false EIR result: {text}"
    );
    assert!(
        text.lines().any(|line| {
            line.contains("Str php=string") && line.contains("runtime.getenv")
        }),
        "getenv must retain the backend's concrete string EIR result: {text}"
    );
}

/// Verifies `count` uses its typed runtime operation for concrete and dynamic values.
#[test]
fn count_uses_typed_runtime_lowering() {
    let module = lower_source(
        "<?php function sized(array $value): int { return count($value); } function dynamic($value): int { return count($value); } echo sized([1]); echo dynamic([1]);",
    );
    let text = print_module(&module);
    assert_eq!(
        text.matches("runtime.count").count(),
        2,
        "concrete and dynamic count calls must retain typed runtime semantics: {text}"
    );
    assert!(
        !text.contains("builtin_call @count"),
        "count leaked through the PHP-name backend boundary: {text}"
    );
}

/// Verifies `is_null` is represented by the general EIR predicate rather than a runtime ID.
#[test]
fn is_null_uses_general_eir_predicate() {
    let module = lower_source(
        "<?php function missing($value): bool { return is_null($value); } echo missing(null);",
    );
    let text = print_module(&module);
    assert!(text.contains("is_null"), "missing EIR null predicate: {text}");
    assert!(
        !text.contains("runtime.is_null") && !text.contains("builtin_call @is_null"),
        "is_null leaked through a builtin-specific backend operation: {text}"
    );
}

/// Verifies scalar conversion builtins reuse general EIR casts and truthiness.
#[test]
fn scalar_conversion_builtins_use_general_eir_primitives() {
    let module = lower_source(
        r#"<?php
function as_bool($value): bool { return boolval($value); }
function as_float(string $value): float { return floatval($value); }
function as_int(string $value): int { return intval($value); }
function as_string(int $value): string { return strval($value); }
echo as_bool($argc), as_float("1.5"), as_int("2"), as_string(3);
"#,
    );
    let text = print_module(&module);
    assert!(text.contains("is_truthy"), "missing EIR truthiness predicate: {text}");
    assert!(text.contains("php=float = cast"), "missing EIR float cast: {text}");
    assert!(text.contains("php=int = cast"), "missing EIR integer cast: {text}");
    assert!(
        text.contains("php=string own=maybe_owned = cast"),
        "missing EIR string cast: {text}"
    );
    for builtin in ["boolval", "floatval", "intval", "strval"] {
        assert!(
            !text.contains(&format!("runtime.{builtin}"))
                && !text.contains(&format!("builtin_call @{builtin}")),
            "{builtin} leaked through a builtin-specific backend operation: {text}"
        );
    }
}

/// Verifies scalar and container type checks share one typed EIR predicate operation.
#[test]
fn php_type_builtins_share_the_type_predicate_primitive() {
    let module = lower_source(
        r#"<?php
function checks($value): bool {
    return is_array($value)
        || is_bool($value)
        || is_float($value)
        || is_int($value)
        || is_iterable($value)
        || is_object($value)
        || is_resource($value)
        || is_scalar($value)
        || is_string($value);
}
echo checks($argc);
"#,
    );
    let text = print_module(&module);
    for predicate in [
        "array", "bool", "float", "int", "iterable", "object", "resource", "scalar", "string",
    ] {
        assert!(
            text.lines().any(|line| {
                line.contains("type_predicate") && line.contains(&format!(" {predicate} ;"))
            }),
            "missing {predicate} EIR type predicate: {text}"
        );
        assert!(
            !text.contains(&format!("runtime.is_{predicate}")),
            "{predicate} predicate leaked through a runtime function: {text}"
        );
    }
}
