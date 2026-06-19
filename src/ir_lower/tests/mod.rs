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
use crate::ir::print_module;

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
    let ast = crate::pdo_prelude::inject_if_used(ast);
    let ast = crate::tz_prelude::inject_if_used(ast);
    let ast = crate::list_id_prelude::inject_if_used(ast);
    let ast = crate::var_export_prelude::inject_if_used(ast);
    let ast = crate::name_resolver::resolve(ast).expect("name resolution failed");
    let ast = crate::autoload::run(ast, parent, &autoload_registry).expect("autoload failed");
    let ast = crate::optimize::fold_constants(ast);
    let check_result = crate::types::check_with_target(&ast, target).expect("type check failed");
    let ast = crate::optimize::propagate_constants(ast);
    let ast = crate::optimize::prune_constant_control_flow(ast);
    let ast = crate::optimize::normalize_control_flow(ast);
    let ast = crate::optimize::eliminate_dead_code(ast);
    crate::ir_lower::lower_program(&ast, &check_result, target).expect("EIR lowering failed")
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

/// Verifies mixed float/integer comparisons coerce both operands before `fcmp`.
#[test]
fn float_comparison_coerces_integer_operand() {
    let module = lower_source("<?php $t = microtime(true); if ($t > 1000000000) { echo \"ok\"; }");
    let text = print_module(&module);
    assert!(text.contains("i_to_f"), "missing integer-to-float coercion in {text}");
    assert!(text.contains("fcmp"), "missing float comparison in {text}");
}
