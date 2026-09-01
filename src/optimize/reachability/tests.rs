//! Purpose:
//! Unit tests for declaration usage scanning, graph reachability, AST pruning, and checker reconcile.
//!
//! Called from:
//! - `cargo test --lib optimize::reachability::tests` through Rust's test harness.
//!
//! Key details:
//! - Fixtures parse directly and use the real type checker for metadata reconciliation coverage.

use std::collections::HashSet;

use crate::names::{php_symbol_key, property_hook_get_method, property_hook_set_method};
use crate::parser::ast::{Program, StmtKind};

use super::usage::scan_program;
use super::{prune_unreachable_declarations, PreludeInventory, PruneOptions};

/// Parses one PHP fixture without running resolution or optimization passes.
fn parse(source: &str) -> Program {
    let tokens = crate::lexer::tokenize(source).expect("fixture must tokenize");
    crate::parser::parse(&tokens).expect("fixture must parse")
}

/// Prunes one self-contained fixture using the real checker and no compiler-owned roots.
fn prune(source: &str) -> (Program, crate::types::CheckResult) {
    let program = parse(source);
    let mut check = crate::types::check(&program).expect("fixture must type check");
    let inventory = PreludeInventory::new();
    let roots = HashSet::new();
    let program = prune_unreachable_declarations(
        program,
        &mut check,
        PruneOptions {
            inventory: &inventory,
            forced_groups: &roots,
            structural_groups: &roots,
            exported_functions: &roots,
            eval_forced: false,
        },
    );
    (program, check)
}

/// Returns whether a top-level function declaration survives.
fn has_function(program: &[crate::parser::ast::Stmt], name: &str) -> bool {
    program.iter().any(|statement| {
        matches!(&statement.kind, StmtKind::FunctionDecl { name: candidate, .. } if php_symbol_key(candidate) == php_symbol_key(name))
    })
}

/// Returns whether a top-level class declaration survives.
fn has_class(program: &[crate::parser::ast::Stmt], name: &str) -> bool {
    program.iter().any(|statement| {
        matches!(&statement.kind, StmtKind::ClassDecl { name: candidate, .. } if php_symbol_key(candidate) == php_symbol_key(name))
    })
}

/// Returns whether one method remains on a top-level class declaration.
fn has_method(program: &[crate::parser::ast::Stmt], class: &str, method: &str) -> bool {
    program.iter().any(|statement| match &statement.kind {
        StmtKind::ClassDecl { name, methods, .. }
            if php_symbol_key(name) == php_symbol_key(class) => methods
                .iter()
                .any(|candidate| php_symbol_key(&candidate.name) == php_symbol_key(method)),
        _ => false,
    })
}

/// Verifies direct function, class, instance-method, and static-method references are recorded.
#[test]
fn scan_records_function_and_method_and_class() {
    let program = parse(
        "<?php $pdo = new PDO('sqlite::memory:'); $pdo->query('select 1'); PDO::getAvailableDrivers(); pdo_drivers();",
    );
    let usage = scan_program(&program);
    assert!(usage.classes.contains(&php_symbol_key("PDO")));
    assert!(usage.methods.contains(&(
        php_symbol_key("PDO"),
        php_symbol_key("query"),
        false
    )));
    assert!(usage.methods.contains(&(
        php_symbol_key("PDO"),
        php_symbol_key("getAvailableDrivers"),
        true
    )));
    assert!(usage.functions.contains(&php_symbol_key("pdo_drivers")));
    assert!(!usage.hazards.dynamic_function);
}

/// Verifies conditional assignments accumulate every statically known receiver class.
#[test]
fn scan_unions_receiver_classes_across_assignments() {
    let usage = scan_program(&parse(
        "<?php class A { public function foo(): string { return 'A'; } } class B { public function foo(): string { return 'B'; } } $x = new A(); if ($argc) { $x = new B(); } $x->foo();",
    ));
    for class in ["A", "B"] {
        assert!(usage.methods.contains(&(
            php_symbol_key(class),
            php_symbol_key("foo"),
            false,
        )));
    }
}

/// Verifies a nullable typed local keeps its declared class and refines nullsafe dispatch.
#[test]
fn scan_uses_typed_local_hint_without_new_expression() {
    let usage = scan_program(&parse(
        "<?php class Box { public function label($value): string { return $value; } } ?Box $box = null; echo $box?->label('ok') ?? 'none';",
    ));
    assert!(usage.classes.contains(&php_symbol_key("Box")));
    assert!(usage.methods.contains(&(
        php_symbol_key("Box"),
        php_symbol_key("label"),
        false,
    )));
}

/// Verifies opaque writes and binding forms turn later receiver dispatch into a wildcard.
#[test]
fn scan_invalidates_receiver_classes_after_opaque_bindings() {
    for source in [
        "<?php class A { public function foo(): int { return 1; } } class B { public function foo(): int { return 2; } } $x = new A(); if ($argc) { $x = $candidate; } else { $x = new B(); } $x->foo();",
        "<?php class A { public function foo(): int { return 1; } } $x = new A(); foreach ($items as $key => $x) {} $x->foo();",
        "<?php class A { public function foo(): int { return 1; } } $x = new A(); [$x] = $items; $x->foo();",
        "<?php class A { public function foo(): int { return 1; } } $x = new A(); $x =& $other; $x->foo();",
        "<?php class A { public function foo(): int { return 1; } } class B {} $source = new A(); $alias =& $source; $alias = new B(); $source->foo();",
        "<?php class A { public function foo(): int { return 1; } } $x = new A(); try {} catch (Exception $x) {} $x->foo();",
        "<?php class A { public function foo(): int { return 1; } } function read_global(): void { $x = new A(); global $x; $x->foo(); } read_global();",
        "<?php class A { public function foo(): int { return 1; } } function read_static(): void { $x = new A(); static $x = null; $x->foo(); } read_static();",
        "<?php class A { public function foo(): int { return 1; } } $x = new A(); $x++; $x->foo();",
    ] {
        let usage = scan_program(&parse(source));
        assert!(
            usage
                .wildcard_methods
                .contains(&(php_symbol_key("foo"), false)),
            "opaque receiver binding must widen method dispatch for: {source}"
        );
    }
}

/// Verifies a reachable global alias widens same-named receiver dispatch across scopes.
#[test]
fn prune_reachable_global_alias_keeps_runtime_receiver_method() {
    let (program, _) = prune(
        "<?php class A { public function foo(): string { return 'A'; } } class B extends A { public function foo(): string { return 'B'; } } function later(): void { global $x; $x = new B(); } $x = new A(); later(); echo $x->foo();",
    );
    assert!(has_method(&program, "A", "foo"));
    assert!(has_method(&program, "B", "foo"));
}

/// Verifies a literal `$GLOBALS` slot aliases the same top-level receiver storage.
#[test]
fn prune_reachable_globals_array_alias_keeps_runtime_receiver_method() {
    let (program, _) = prune(
        "<?php class A { public function foo(): string { return 'A'; } } class B extends A { public function foo(): string { return 'B'; } } function later(): void { $GLOBALS['x'] = new B(); } $x = new A(); later(); echo $x->foo();",
    );
    assert!(has_method(&program, "A", "foo"));
    assert!(has_method(&program, "B", "foo"));
}

/// Verifies a dynamic `$GLOBALS` key widens receiver dispatch for every variable name.
#[test]
fn prune_dynamic_globals_array_alias_widens_all_receiver_names() {
    let (program, _) = prune(
        "<?php class A { public function foo(): string { return 'A'; } } class B extends A { public function foo(): string { return 'B'; } } function later(string $name): void { $GLOBALS[$name] = new B(); } $receiver = new A(); later('receiver'); echo $receiver->foo();",
    );
    assert!(has_method(&program, "A", "foo"));
    assert!(has_method(&program, "B", "foo"));
}

/// Verifies a direct by-ref call invalidates the caller variable even under a different name.
#[test]
fn prune_by_ref_argument_keeps_runtime_receiver_method() {
    let (program, _) = prune(
        "<?php class A { public function foo(): string { return 'A'; } } class B extends A { public function foo(): string { return 'B'; } } function replace(&$slot): void { $slot = new B(); } $receiver = new A(); replace(slot: $receiver); echo $receiver->foo();",
    );
    assert!(has_method(&program, "A", "foo"));
    assert!(has_method(&program, "B", "foo"));
}

/// Verifies aliases in unreachable function bodies do not widen executable receiver dispatch.
#[test]
fn prune_ignores_global_alias_in_unreachable_function() {
    let (program, _) = prune(
        "<?php class A { public function foo(): string { return 'A'; } } class B extends A { public function foo(): string { return 'B'; } } function never_called(): void { global $x; $x = new B(); } $x = new A(); echo $x->foo();",
    );
    assert!(has_method(&program, "A", "foo"));
    assert!(!has_class(&program, "B"));
}

/// Verifies expression-call conservatism deliberately retains methods on every live class.
#[test]
fn prune_expr_call_keeps_methods_on_live_classes() {
    let (program, _) = prune(
        "<?php class A { public function alpha(): int { return 1; } } class B { public function beta(): int { return 2; } } new A(); new B(); $callback = function(): void {}; ($callback)();",
    );
    assert!(has_method(&program, "A", "alpha"));
    assert!(has_method(&program, "B", "beta"));
}

/// Verifies PDO's internal initializer builtin retains the private method called by its backend lowering.
#[test]
fn scan_records_pdo_initializer_backend_method_edge() {
    let usage = scan_program(&parse(
        "<?php __elephc_initialize_pdo_statement($statement, 1, 2, 2, 'select 1');",
    ));
    assert!(usage.classes.contains(&php_symbol_key("PDOStatement")));
    assert!(usage.methods.contains(&(
        php_symbol_key("PDOStatement"),
        php_symbol_key("__elephcInitialize"),
        false,
    )));
}

/// Verifies registry-declared callback parameters retain literal function targets.
#[test]
fn scan_records_builtin_callback_parameters() {
    for (source, expected) in [
        ("<?php usort($values, 'compare_values');", "compare_values"),
        ("<?php array_map('map_value', $values);", "map_value"),
        (
            "<?php array_map(array: $values, callback: 'named_map_value');",
            "named_map_value",
        ),
        (
            "<?php preg_replace_callback('/x/', 'replace_value', 'x');",
            "replace_value",
        ),
        ("<?php array_walk($values, 'walk_value');", "walk_value"),
        ("<?php ob_start('output_handler');", "output_handler"),
    ] {
        let usage = scan_program(&parse(source));
        assert!(
            usage.functions.contains(&php_symbol_key(expected)),
            "builtin callback target {expected} must be recorded"
        );
    }
}

/// Verifies a positive `is_array()` guard suppresses impossible Iterator dispatch.
#[test]
fn scan_array_guard_avoids_iterator_protocol_wildcard() {
    let usage = scan_program(&parse(
        "<?php function copy_values(mixed $source): array { $copy = []; if (is_array($source)) { foreach ($source as $value) { $copy[] = $value; } } return $copy; } copy_values([]);",
    ));
    assert!(!usage
        .wildcard_methods
        .contains(&(php_symbol_key("rewind"), false)));
}

/// Verifies an unguarded opaque foreach receiver still widens Iterator dispatch.
#[test]
fn scan_opaque_foreach_keeps_iterator_protocol_wildcard() {
    let usage = scan_program(&parse(
        "<?php function copy_values(mixed $source): array { $copy = []; foreach ($source as $value) { $copy[] = $value; } return $copy; }",
    ));
    assert!(usage
        .wildcard_methods
        .contains(&(php_symbol_key("rewind"), false)));
}

/// Verifies a reachable function keeps classes referenced only by parameter attributes.
#[test]
fn prune_scans_function_parameter_attributes() {
    let (program, _) = prune(
        "<?php #[Attribute] class FunctionParamTag { public function __construct(public string $name = '') {} } function tagged(#[FunctionParamTag('value')] int $value): int { return $value; } echo tagged(1);",
    );
    assert!(has_class(&program, "FunctionParamTag"));
    assert!(has_method(&program, "FunctionParamTag", "__construct"));
}

/// Verifies a reachable method keeps classes referenced only by parameter attributes.
#[test]
fn prune_scans_method_parameter_attributes() {
    let (program, _) = prune(
        "<?php #[Attribute] class MethodParamTag { public function __construct(public string $name = '') {} } class TaggedTarget { public function run(#[MethodParamTag('value')] int $value): int { return $value; } } echo (new TaggedTarget())->run(1);",
    );
    assert!(has_class(&program, "MethodParamTag"));
    assert!(has_method(&program, "MethodParamTag", "__construct"));
}

/// Verifies a user function's typed callable parameter retains a literal free-function target.
#[test]
fn prune_keeps_callable_bound_to_typed_user_parameter() {
    let (program, _) = prune(
        "<?php function format_value(): string { return 'ok'; } function invoke(callable $handler): string { return $handler(); } echo invoke(handler: 'format_value');",
    );
    assert!(has_function(&program, "invoke"));
    assert!(has_function(&program, "format_value"));
}

/// Verifies property reads and writes retain the parser-generated hook method bodies.
#[test]
fn prune_keeps_property_hook_accessors() {
    let (program, _) = prune(
        "<?php class Name { public string $value { get => $this->value; set => trim($value); } } $name = new Name(); $name->value = ' Ada '; echo $name->value;",
    );
    assert!(has_method(
        &program,
        "Name",
        &property_hook_get_method("value")
    ));
    assert!(has_method(
        &program,
        "Name",
        &property_hook_set_method("value")
    ));
}

/// Verifies reflection-style class-string builtins keep the inspected source class metadata.
#[test]
fn prune_keeps_class_named_by_attribute_introspection() {
    let (program, _) = prune(
        "<?php class Author { public function __construct(public string $name) {} } #[Author('Ada')] class Greeter {} foreach (class_attribute_names(class_name: 'Greeter') as $name) { echo $name; }",
    );
    assert!(has_class(&program, "Greeter"));
    assert!(has_class(&program, "Author"));
    assert!(has_method(&program, "Author", "__construct"));
}

/// Verifies a class-string size query keeps its extern-class layout metadata.
#[test]
fn prune_keeps_extern_class_named_by_ptr_sizeof() {
    let (program, check) = prune(
        "<?php extern class Point { public int $x; public int $y; } echo ptr_sizeof('Point');",
    );
    assert!(program.iter().any(|statement| {
        matches!(&statement.kind, StmtKind::ExternClassDecl { name, .. } if php_symbol_key(name) == php_symbol_key("Point"))
    }));
    assert!(check
        .extern_classes
        .keys()
        .any(|name| php_symbol_key(name) == php_symbol_key("Point")));
}

/// Verifies a literal stream protocol registration retains every runtime-dispatched method.
#[test]
fn prune_keeps_registered_stream_wrapper_protocol() {
    let (program, _) = prune(
        "<?php class Wrapper { public function stream_open($path, $mode, $options, &$opened): bool { return true; } public function stream_read($count): string { return ''; } public function stream_close(): void {} public function ordinary(): void {} } stream_wrapper_register('test', 'Wrapper'); fopen('test://value', 'r');",
    );
    for method in ["stream_open", "stream_read", "stream_close", "ordinary"] {
        assert!(
            has_method(&program, "Wrapper", method),
            "dynamic runtime protocol registration must retain {method}"
        );
    }
}

/// Verifies PDO's validated dynamic statement allocation keeps only compatible subclasses.
#[test]
fn prune_constrains_pdo_prepare_dynamic_allocation_to_statement_subclasses() {
    let (program, _) = prune(
        "<?php class PDOStatement {} class CustomStatement extends PDOStatement { public function __construct() {} } class Unrelated { public function __construct() {} } class PDO { public function prepare(string $class): mixed { return __elephc_new_without_constructor($class); } } echo (new PDO())->prepare('CustomStatement');",
    );
    assert!(has_class(&program, "PDOStatement"));
    assert!(has_class(&program, "CustomStatement"));
    assert!(has_method(&program, "CustomStatement", "__construct"));
    assert!(!has_class(&program, "Unrelated"));
}

/// Verifies a literal `function_exists` probe adds one edge without widening dynamic calls.
#[test]
fn scan_literal_function_exists_is_a_reference_not_a_hazard() {
    let usage = scan_program(&parse(
        "<?php echo function_exists('pdo_drivers') ? 'y' : 'n';",
    ));
    assert!(usage.functions.contains(&php_symbol_key("pdo_drivers")));
    assert!(!usage.hazards.dynamic_function);
}

/// Verifies eval widens declaration reachability because runtime source can name declarations.
#[test]
fn scan_eval_is_dynamic_function() {
    let usage = scan_program(&parse("<?php eval('echo 1;');"));
    assert!(usage.hazards.dynamic_function);
    assert!(usage.hazards.dynamic_method);
    assert!(usage.hazards.dynamic_class);
}

/// Verifies a variable callable can dispatch to either a free function or an object method.
#[test]
fn scan_variable_callable_widens_function_and_method_hazards() {
    let usage = scan_program(&parse("<?php $callback = $argv[1]; $callback();"));
    assert!(usage.hazards.dynamic_function);
    assert!(usage.hazards.dynamic_method);
}

/// Verifies a computed expression callable may resolve to a function or an array method callable.
#[test]
fn scan_computed_expr_callable_widens_function_and_method_hazards() {
    let usage = scan_program(&parse(
        "<?php ([make_prefixer(), choose_method('wrap')])(suffix: '?');",
    ));
    assert!(usage.hazards.dynamic_function);
    assert!(usage.hazards.dynamic_method);
}

/// Verifies dynamic method dispatch and unserialize produce their respective hazards.
#[test]
fn scan_dynamic_method_and_unserialize_hazards() {
    let usage = scan_program(&parse(
        "<?php $m = 'query'; $pdo->$m(); unserialize($argv[1]);",
    ));
    assert!(usage.hazards.dynamic_method);
    assert!(usage.hazards.dynamic_class);
}

/// Verifies dynamic function and class names widen their independent declaration domains.
#[test]
fn scan_dynamic_function_and_class_hazards() {
    let usage = scan_program(&parse("<?php $fn = $argv[1]; $fn(); $class = $argv[2]; new $class();"));
    assert!(usage.hazards.dynamic_function);
    assert!(usage.hazards.dynamic_class);
}

/// Verifies declared-symbol enumeration widens the complete class-like declaration domain.
#[test]
fn scan_declared_class_queries_are_dynamic_class_hazards() {
    for source in [
        "<?php get_declared_classes();",
        "<?php get_declared_interfaces();",
        "<?php get_declared_traits();",
    ] {
        assert!(scan_program(&parse(source)).hazards.dynamic_class);
    }
}

/// Verifies eval-only introspection names do not invent hazards in the AOT declaration graph.
#[test]
fn scan_eval_only_introspection_names_are_not_aot_hazards() {
    for source in [
        "<?php get_defined_functions();",
        "<?php get_class_methods($value);",
    ] {
        let hazards = scan_program(&parse(source)).hazards;
        assert!(!hazards.dynamic_function);
        assert!(!hazards.dynamic_method);
        assert!(!hazards.dynamic_class);
    }
}

/// Verifies an unpacked builtin callback that cannot be identified widens callable reachability.
#[test]
fn scan_dynamic_spread_builtin_callback_is_a_hazard() {
    let usage = scan_program(&parse("<?php usort(...$args);"));
    assert!(usage.hazards.dynamic_function);
    assert!(usage.hazards.dynamic_method);
}

/// Verifies ordinary two-element data arrays do not become dynamic-method hazards.
#[test]
fn scan_two_element_data_array_is_not_a_callable_hazard() {
    let usage = scan_program(&parse("<?php $error = ['HY000', 0]; echo $error[0];"));
    assert!(!usage.hazards.dynamic_method);
}

/// Verifies literal class probes retain only the named class without a dynamic-class hazard.
#[test]
fn scan_literal_class_exists_is_a_reference_not_a_hazard() {
    let usage = scan_program(&parse("<?php echo class_exists('Visible') ? 'y' : 'n';"));
    assert!(usage.classes.contains(&php_symbol_key("Visible")));
    assert!(!usage.hazards.dynamic_class);
}

/// Verifies reordered named class-probe arguments are normalized instead of widening hazards.
#[test]
fn scan_normalizes_named_class_exists_arguments() {
    let usage = scan_program(&parse(
        "<?php echo class_exists(autoload: true, class: 'Visible') ? 'y' : 'n';",
    ));
    assert!(usage.classes.contains(&php_symbol_key("Visible")));
    assert!(!usage.hazards.dynamic_class);
}

/// Verifies a literal `property_exists` class target retains exact class metadata.
#[test]
fn scan_literal_property_exists_class_is_a_reference_not_a_hazard() {
    let usage = scan_program(&parse(
        "<?php echo property_exists('Visible', 'property') ? 'y' : 'n';",
    ));
    assert!(usage.classes.contains(&php_symbol_key("Visible")));
    assert!(!usage.hazards.dynamic_class);
}

/// Verifies named `method_exists` arguments are mapped by signature rather than source order.
#[test]
fn scan_normalizes_named_method_exists_arguments() {
    let usage = scan_program(&parse(
        "<?php echo method_exists(method: 'hidden', object_or_class: 'Visible') ? 'y' : 'n';",
    ));
    assert!(usage.classes.contains(&php_symbol_key("Visible")));
    assert!(usage.methods.contains(&(
        php_symbol_key("Visible"),
        php_symbol_key("hidden"),
        false,
    )));
    assert!(!usage.hazards.dynamic_class);
    assert!(!usage.hazards.dynamic_method);
}

/// Verifies a dynamic method probe still roots its literal target before widening methods.
#[test]
fn prune_dynamic_method_exists_keeps_literal_target_methods() {
    let (program, _) = prune(
        "<?php class Visible { public static function hidden(): int { return 1; } } $method = $argc > 0 ? 'hidden' : 'missing'; echo method_exists('Visible', $method) ? 'y' : 'n';",
    );
    assert!(has_class(&program, "Visible"));
    assert!(has_method(&program, "Visible", "hidden"));
}

/// Verifies Reflection references conservatively widen functions, methods, and classes.
#[test]
fn scan_reflection_reference_widens_all_declaration_hazards() {
    let usage = scan_program(&parse("<?php $reflection = new ReflectionClass('Visible');"));
    assert!(usage.hazards.dynamic_function);
    assert!(usage.hazards.dynamic_method);
    assert!(usage.hazards.dynamic_class);
}

/// Verifies Reflection retains checker-synthesized enum methods absent from the source AST.
#[test]
fn prune_keeps_synthetic_enum_methods_for_reflection() {
    let (_, check) = prune(
        "<?php enum Pure { case Ready; } enum Backed: string { case Ready = 'ready'; } new ReflectionClass(Pure::class); new ReflectionClass(Backed::class);",
    );
    let pure = check.classes.get("Pure").expect("Pure metadata must survive");
    assert!(pure.static_methods.contains_key("cases"));
    let backed = check
        .classes
        .get("Backed")
        .expect("Backed metadata must survive");
    for method in ["cases", "from", "tryfrom"] {
        assert!(
            backed.static_methods.contains_key(method),
            "synthetic enum method {method} must survive Reflection"
        );
    }
}

/// Verifies fixed-point pruning keeps a called function and removes an unused sibling.
#[test]
fn prune_drops_unused_function_and_keeps_called_one() {
    let (program, check) = prune(
        "<?php function used(): int { return 1; } function unused(): int { return 2; } echo used();",
    );
    assert!(has_function(&program, "used"));
    assert!(!has_function(&program, "unused"));
    assert!(check.functions.contains_key("used"));
    assert!(!check.functions.contains_key("unused"));
}

/// Verifies pruning source declarations leaves checker-injected runtime class metadata intact.
///
/// Split in two because registration is now PAY-FOR-USE. The throwables below are seeded
/// unconditionally — `ALWAYS_REGISTERED_THROWABLES` mirrors the codegen seeder, since a runtime
/// helper can raise `DivisionByZeroError` or `JsonException` with no class reference in the
/// source to hang an id off. Everything else, `ReflectionClass` and `DateTime` included, arrives
/// only when the program can reach it, so a program that never mentions them has nothing to
/// preserve and asserting otherwise would pin the absence of that gate rather than this pass.
///
/// What this test is about is unchanged: pruning SOURCE declarations must not take
/// checker-injected metadata with it.
#[test]
fn prune_preserves_synthetic_checker_classes() {
    let (program, check) = prune(
        "<?php class UnusedSourceClass { public function unused(): int { return 1; } } echo 1;",
    );
    assert!(!has_class(&program, "UnusedSourceClass"));
    for class in ["Exception", "Error", "TypeError"] {
        assert!(
            check
                .classes
                .keys()
                .any(|candidate| php_symbol_key(candidate) == php_symbol_key(class)),
            "unconditional throwable {class} must survive source declaration pruning"
        );
    }

    // The gated half: mentioned by the program, so registered.
    //
    // `UnusedSourceClass` SURVIVES here, and that is not a weaker assertion — it is the pruner
    // being right. `new ReflectionClass($name)` can name any class at run time, so a program
    // holding one gives the pass nothing it may drop. Pinning the survival is what would catch a
    // future pruner that got clever about reflection.
    let (program, check) = prune(
        "<?php class UnusedSourceClass { public function unused(): int { return 1; } } \
         $r = new ReflectionClass(\"Exception\"); $d = new DateTime(); $f = new SplFixedArray(1); echo 1;",
    );
    assert!(
        has_class(&program, "UnusedSourceClass"),
        "a reflection construct must keep source classes reachable"
    );
    for class in ["ReflectionClass", "DateTime", "SplFixedArray"] {
        assert!(
            check
                .classes
                .keys()
                .any(|candidate| php_symbol_key(candidate) == php_symbol_key(class)),
            "synthetic checker class {class} must survive source declaration pruning"
        );
    }
}

/// Verifies the fixed point follows calls made from a newly live function body.
#[test]
fn prune_follows_callee_body_edges() {
    let (program, _) = prune(
        "<?php function inner(): int { return 3; } function outer(): int { return inner(); } function unused(): int { return inner(); } echo outer();",
    );
    assert!(has_function(&program, "outer"));
    assert!(has_function(&program, "inner"));
    assert!(!has_function(&program, "unused"));
}

/// Verifies class reachability removes an unused sibling class.
#[test]
fn prune_drops_unused_class_keeps_used_class() {
    let (program, _) = prune(
        "<?php class Keep { public function f(): int { return 1; } } class Drop { public function g(): int { return 2; } } echo (new Keep())->f();",
    );
    assert!(has_class(&program, "Keep"));
    assert!(!has_class(&program, "Drop"));
}

/// Verifies eval conservatively retains declarations absent from static edges.
#[test]
fn eval_keeps_otherwise_unused_function() {
    let (program, _) = prune(
        "<?php function hidden(): int { return 1; } eval('echo hidden();');",
    );
    assert!(has_function(&program, "hidden"));
}

/// Verifies a live class drops an unreferenced ordinary method while retaining a called one.
#[test]
fn prune_drops_unused_method() {
    let (program, check) = prune(
        "<?php class T { public function keep(): int { return 1; } public function drop(): int { return 2; } } $t = new T(); echo $t->keep();",
    );
    assert!(has_method(&program, "T", "keep"));
    assert!(!has_method(&program, "T", "drop"));
    let class = check.classes.get("T").expect("T metadata must survive");
    assert!(class.methods.contains_key("keep"));
    assert!(!class.methods.contains_key("drop"));
    assert_eq!(
        class.vtable_slots.len(),
        class.vtable_methods.len(),
        "vtable slots must be rebuilt for exactly the surviving methods"
    );
    for (slot, method) in class.vtable_methods.iter().enumerate() {
        assert_eq!(class.vtable_slots.get(method), Some(&slot));
    }
}

/// Verifies a live interface contract retains its concrete implementation but not a sibling method.
#[test]
fn prune_keeps_interface_method() {
    let (program, _) = prune(
        "<?php interface I { public function need(): int; } class C implements I { public function need(): int { return 1; } public function extra(): int { return 2; } } function take(I $x): int { return $x->need(); } echo take(new C());",
    );
    assert!(has_method(&program, "C", "need"));
    assert!(!has_method(&program, "C", "extra"));
}

/// Verifies instantiating a child retains inherited constructor and destructor implementations.
#[test]
fn prune_keeps_inherited_magic_methods() {
    let (program, _) = prune(
        "<?php class ParentType { public function __construct() {} public function __destruct() {} public function unused(): int { return 1; } } class ChildType extends ParentType {} $child = new ChildType();",
    );
    assert!(has_method(&program, "ParentType", "__construct"));
    assert!(has_method(&program, "ParentType", "__destruct"));
    assert!(!has_method(&program, "ParentType", "unused"));
}

/// Verifies a private override remains as a descendant vtable hole after its metadata stops inheriting.
#[test]
fn prune_keeps_private_override_vtable_hole_on_descendant() {
    let (_, check) = prune(
        "<?php class SlotBase { public function __construct() {} public function later(): string { return 'later'; } } class SlotParent extends SlotBase { private function __construct() { parent::__construct(); } public static function build(): self { return new self(); } } class SlotChild extends SlotParent {} function through_base(SlotBase $value): string { return $value->later(); } class_exists('SlotChild'); echo through_base(SlotParent::build());",
    );
    let parent = check
        .classes
        .get("SlotParent")
        .expect("private override metadata must survive");
    let child = check
        .classes
        .get("SlotChild")
        .expect("descendant metadata must survive");

    assert!(!child.methods.contains_key("__construct"));
    assert_eq!(
        parent.vtable_slots.get("__construct"),
        child.vtable_slots.get("__construct")
    );
    assert_eq!(
        parent.vtable_slots.get("later"),
        child.vtable_slots.get("later")
    );
}

/// Verifies `parent::method()` retains a non-static parent implementation.
#[test]
fn prune_keeps_parent_scoped_instance_method() {
    let (program, _) = prune(
        "<?php class ParentType { public function value(): int { return 1; } } class ChildType extends ParentType { public function read(): int { return parent::value(); } } echo (new ChildType())->read();",
    );
    assert!(has_method(&program, "ChildType", "read"));
    assert!(has_method(&program, "ParentType", "value"));
}

/// Verifies a scoped parent call does not retain an unrelated live class's same-named method.
#[test]
fn prune_parent_scoped_method_edge_is_class_specific() {
    let (program, _) = prune(
        "<?php class ParentType { protected function value(): int { return 1; } } class ChildType extends ParentType { public function read(): int { return parent::value(); } } class OtherType { public function value(): int { return 2; } public function keep(): int { return 3; } } echo (new ChildType())->read(); echo (new OtherType())->keep();",
    );
    assert!(has_method(&program, "ParentType", "value"));
    assert!(has_method(&program, "ChildType", "read"));
    assert!(has_method(&program, "OtherType", "keep"));
    assert!(!has_method(&program, "OtherType", "value"));
}

/// Verifies a scoped parent edge keeps overriding descendant slots without widening to siblings.
#[test]
fn prune_parent_scoped_method_keeps_descendant_vtable_slots_aligned() {
    let (program, check) = prune(
        "<?php class SlotBase { public function shadowed(): string { return 'base'; } public function later(): string { return 'later'; } } class SlotChild extends SlotBase { public function shadowed(): string { return 'child'; } public function boot(): string { return parent::shadowed(); } } class SlotSibling { public function shadowed(): string { return 'sibling'; } public function keep(): string { return 'keep'; } } function through_base(SlotBase $value): string { return $value->later(); } $child = new SlotChild(); $child->boot(); echo through_base($child); echo (new SlotSibling())->keep();",
    );
    assert!(has_method(&program, "SlotBase", "shadowed"));
    assert!(has_method(&program, "SlotChild", "shadowed"));
    assert!(!has_method(&program, "SlotSibling", "shadowed"));

    let parent = check
        .classes
        .get("SlotBase")
        .expect("parent metadata must survive");
    let child = check
        .classes
        .get("SlotChild")
        .expect("child metadata must survive");
    assert_eq!(
        parent.vtable_slots.get("shadowed"),
        child.vtable_slots.get("shadowed")
    );
    assert_eq!(
        parent.vtable_slots.get("later"),
        child.vtable_slots.get("later")
    );
}

/// Verifies a mid-chain `parent::` edge cannot desynchronize the grandparent vtable.
#[test]
fn prune_parent_scoped_method_keeps_grandparent_vtable_slots_aligned() {
    let (program, check) = prune(
        "<?php class SlotRoot { public function shadowed(): string { return 'root'; } public function later(): string { return 'later'; } } class SlotMid extends SlotRoot { public function shadowed(): string { return 'mid'; } } class SlotLeaf extends SlotMid { public function boot(): string { return parent::shadowed(); } } function through_root(SlotRoot $value): string { return $value->later(); } $leaf = new SlotLeaf(); $leaf->boot(); echo through_root($leaf);",
    );
    assert!(has_method(&program, "SlotRoot", "shadowed"));
    assert!(has_method(&program, "SlotMid", "shadowed"));
    assert!(has_method(&program, "SlotLeaf", "boot"));

    let root = check
        .classes
        .get("SlotRoot")
        .expect("grandparent metadata must survive");
    let mid = check
        .classes
        .get("SlotMid")
        .expect("mid metadata must survive");
    let leaf = check
        .classes
        .get("SlotLeaf")
        .expect("leaf metadata must survive");
    assert_eq!(
        root.vtable_slots.get("shadowed"),
        mid.vtable_slots.get("shadowed")
    );
    assert_eq!(
        root.vtable_slots.get("later"),
        mid.vtable_slots.get("later")
    );
    assert_eq!(
        mid.vtable_slots.get("later"),
        leaf.vtable_slots.get("later")
    );
}

/// Verifies a mid-chain `parent::` edge cannot desynchronize an abstract grandparent vtable.
#[test]
fn prune_parent_scoped_method_keeps_abstract_grandparent_vtable_slots_aligned() {
    let (program, check) = prune(
        "<?php abstract class SlotRoot { abstract public function shadowed(): string; public function later(): string { return 'later'; } } class SlotMid extends SlotRoot { public function shadowed(): string { return 'mid'; } } class SlotLeaf extends SlotMid { public function boot(): string { return parent::shadowed(); } } function through_root(SlotRoot $value): string { return $value->later(); } $leaf = new SlotLeaf(); $leaf->boot(); echo through_root($leaf);",
    );
    assert!(has_method(&program, "SlotMid", "shadowed"));
    assert!(has_method(&program, "SlotLeaf", "boot"));

    let root = check
        .classes
        .get("SlotRoot")
        .expect("abstract grandparent metadata must survive");
    let mid = check
        .classes
        .get("SlotMid")
        .expect("mid metadata must survive");
    assert_eq!(
        root.vtable_slots.get("shadowed"),
        mid.vtable_slots.get("shadowed")
    );
    assert_eq!(
        root.vtable_slots.get("later"),
        mid.vtable_slots.get("later")
    );
}

/// Verifies a mid-chain static `parent::` edge cannot desynchronize late-bound static slots.
#[test]
fn prune_parent_scoped_method_keeps_grandparent_static_vtable_slots_aligned() {
    let (program, check) = prune(
        "<?php class StaticSlotRoot { public static function shadowed(): string { return 'root'; } public static function dispatch(): string { return static::later(); } public static function later(): string { return 'later'; } } class StaticSlotMid extends StaticSlotRoot { public static function shadowed(): string { return 'mid'; } } class StaticSlotLeaf extends StaticSlotMid { public static function boot(): string { return parent::shadowed(); } } StaticSlotLeaf::boot(); echo StaticSlotLeaf::dispatch();",
    );
    assert!(has_method(&program, "StaticSlotRoot", "shadowed"));
    assert!(has_method(&program, "StaticSlotMid", "shadowed"));
    assert!(has_method(&program, "StaticSlotLeaf", "boot"));

    let root = check
        .classes
        .get("StaticSlotRoot")
        .expect("static grandparent metadata must survive");
    let mid = check
        .classes
        .get("StaticSlotMid")
        .expect("static mid metadata must survive");
    let leaf = check
        .classes
        .get("StaticSlotLeaf")
        .expect("static leaf metadata must survive");
    assert_eq!(
        root.static_vtable_slots.get("shadowed"),
        mid.static_vtable_slots.get("shadowed")
    );
    assert_eq!(
        root.static_vtable_slots.get("later"),
        mid.static_vtable_slots.get("later")
    );
    assert_eq!(
        mid.static_vtable_slots.get("later"),
        leaf.static_vtable_slots.get("later")
    );
}

/// Verifies scoped parent retention also preserves matching static-vtable entries on descendants.
#[test]
fn prune_parent_scoped_method_keeps_descendant_static_vtable_slots_aligned() {
    let (program, check) = prune(
        "<?php class StaticSlotBase { public static function shadowed(): string { return 'base'; } public static function dispatch(): string { return static::later(); } public static function later(): string { return 'later'; } } class StaticSlotChild extends StaticSlotBase { public static function shadowed(): string { return 'child'; } public static function boot(): string { return parent::shadowed(); } } StaticSlotChild::boot(); echo StaticSlotChild::dispatch();",
    );
    assert!(has_method(&program, "StaticSlotBase", "shadowed"));
    assert!(has_method(&program, "StaticSlotChild", "shadowed"));

    let parent = check
        .classes
        .get("StaticSlotBase")
        .expect("static parent metadata must survive");
    let child = check
        .classes
        .get("StaticSlotChild")
        .expect("static child metadata must survive");
    assert_eq!(
        parent.static_vtable_slots.get("shadowed"),
        child.static_vtable_slots.get("shadowed")
    );
    assert_eq!(
        parent.static_vtable_slots.get("later"),
        child.static_vtable_slots.get("later")
    );
}

/// Verifies inherited dispatch follows the implementation owner recorded by the checker.
#[test]
fn prune_keeps_checker_resolved_inherited_method() {
    let (program, check) = prune(
        "<?php class ParentType { public function value(): int { return 7; } public function unused(): int { return 0; } } class ChildType extends ParentType {} echo (new ChildType())->value();",
    );
    assert!(has_method(&program, "ParentType", "value"));
    assert!(!has_method(&program, "ParentType", "unused"));
    let child = check
        .classes
        .get("ChildType")
        .expect("ChildType metadata must survive");
    assert_eq!(
        child.method_impl_classes.get("value").map(String::as_str),
        Some("ParentType")
    );
}

/// Verifies flattened trait bodies are scanned in the consuming class context.
#[test]
fn prune_keeps_flattened_trait_body_dependencies() {
    let (program, check) = prune(
        "<?php function helper(): int { return 42; } trait ValueTrait { public function value(): int { return helper(); } public function unused(): int { return 0; } } class UsesValue { use ValueTrait; } echo (new UsesValue())->value();",
    );
    assert!(has_function(&program, "helper"));
    let class = check
        .classes
        .get("UsesValue")
        .expect("UsesValue metadata must survive");
    assert!(class.method_decls.iter().any(|method| method.name == "value"));
    assert!(class.method_decls.iter().all(|method| method.name != "unused"));
}

/// Verifies `$this`, `self`, and `parent` references retain their concrete method owners.
#[test]
fn prune_resolves_scoped_method_context() {
    let (program, _) = prune(
        "<?php class Base { protected static function baseValue(): int { return 40; } } class Child extends Base { private static function increment(): int { return 1; } private function one(): int { return 1; } public function value(): int { return parent::baseValue() + self::increment() + $this->one(); } public function unused(): int { return 0; } } echo (new Child())->value();",
    );
    assert!(has_method(&program, "Base", "baseValue"));
    assert!(has_method(&program, "Child", "increment"));
    assert!(has_method(&program, "Child", "one"));
    assert!(has_method(&program, "Child", "value"));
    assert!(!has_method(&program, "Child", "unused"));
}

/// Verifies bridge requirements owned only by a pruned builtin call disappear from checker metadata.
#[test]
fn prune_recomputes_builtin_required_libraries() {
    let (_, check) = prune(
        "<?php function unused(): string { return hash('sha256', 'value'); } echo 1;",
    );
    assert!(
        check
            .required_libraries
            .iter()
            .all(|library| library != "elephc_crypto")
    );
}

/// Verifies named builtin arguments preserve a live conditional bridge after dead-call pruning.
#[test]
fn prune_normalizes_builtin_arguments_before_recomputing_libraries() {
    let (_, check) = prune(
        "<?php function unused(): void { fopen('https://example.com/dead', 'rb'); } fopen(mode: 'rb', filename: 'https://example.com/live');",
    );
    assert!(
        check
            .required_libraries
            .iter()
            .any(|library| library == "elephc_tls"),
        "the surviving named fopen call still requires the TLS bridge"
    );
}

/// Verifies `new static` retains an overriding constructor on a runtime-selected subclass.
#[test]
fn prune_new_static_keeps_descendant_constructor() {
    let (program, _) = prune(
        "<?php class BaseFactory { public static function make(): static { return new static(); } } class ChildFactory extends BaseFactory { public function __construct() { echo 'child'; } } ChildFactory::make();",
    );
    assert!(has_class(&program, "ChildFactory"));
    assert!(has_method(&program, "ChildFactory", "__construct"));
}

/// Verifies a dynamic method name retains every method on each live class.
#[test]
fn prune_keeps_all_methods_when_dynamic_method_hazard() {
    let (program, _) = prune(
        "<?php class T { public function keep(): int { return 1; } public function drop(): int { return 2; } } $t = new T(); $m = $argv[1]; echo $t->$m();",
    );
    assert!(has_method(&program, "T", "keep"));
    assert!(has_method(&program, "T", "drop"));
}

/// Verifies a dynamic dispatch hidden in a dead method does not retain live-class siblings.
#[test]
fn prune_ignores_dynamic_method_hazard_in_unreachable_method() {
    let (program, _) = prune(
        "<?php class T { public function keep(): int { return 1; } public function deadDispatch(string $name) { return $this->$name(); } public function drop(): int { return 2; } } $t = new T(); echo $t->keep();",
    );
    assert!(has_method(&program, "T", "keep"));
    assert!(!has_method(&program, "T", "deadDispatch"));
    assert!(!has_method(&program, "T", "drop"));
}

/// Verifies a dynamic dispatch in a reachable method still retains every live-class method.
#[test]
fn prune_propagates_dynamic_method_hazard_from_reachable_method() {
    let (program, _) = prune(
        "<?php class T { public function dispatch(string $name) { return $this->$name(); } public function target(): int { return 1; } public function sibling(): int { return 2; } } $t = new T(); echo $t->dispatch('target');",
    );
    assert!(has_method(&program, "T", "dispatch"));
    assert!(has_method(&program, "T", "target"));
    assert!(has_method(&program, "T", "sibling"));
}

/// Verifies a compiler-invoked protocol body still applies its dynamic-method hazard.
#[test]
fn prune_countable_protocol_body_propagates_dynamic_method_hazard() {
    let (program, _) = prune(
        "<?php class Items implements Countable { public function count(): int { $name = 'target'; $this->$name(); return 1; } public function target(): int { return 1; } public function unused(): int { return 2; } } echo count(new Items());",
    );
    assert!(has_method(&program, "Items", "count"));
    assert!(
        has_method(&program, "Items", "target"),
        "count() lowering executes count(), so $this->$name() must keep target"
    );
}

/// Verifies foreach-invoked Iterator methods still apply dynamic lookup in their bodies.
#[test]
fn prune_iterator_protocol_body_propagates_dynamic_method_hazard() {
    let (program, _) = prune(
        "<?php class Items implements Iterator { public function rewind(): void { $name = 'target'; $this->$name(); } public function current(): mixed { return 1; } public function key(): mixed { return 0; } public function next(): void {} public function valid(): bool { return false; } public function target(): void {} public function unused(): int { return 2; } } foreach (new Items() as $value) {}",
    );
    assert!(has_method(&program, "Items", "rewind"));
    assert!(
        has_method(&program, "Items", "target"),
        "foreach lowering executes rewind(), so $this->$name() must keep target"
    );
}

/// Verifies a live class retains methods required by compiler-injected interface contracts.
#[test]
fn prune_keeps_builtin_interface_method_on_live_class() {
    let (program, _) = prune(
        "<?php class Items implements Countable { public function count(): int { return 1; } public function unused(): int { return 2; } } new Items();",
    );
    assert!(has_method(&program, "Items", "count"));
    assert!(!has_method(&program, "Items", "unused"));
}

/// Verifies structural interface retention follows static edges without activating dynamic hazards.
#[test]
fn prune_structural_interface_method_does_not_widen_hazards() {
    let (program, _) = prune(
        "<?php function helper(): int { return 1; } interface Runner { public function run(string $name): int; } class T implements Runner { public function run(string $name): int { helper(); return $this->$name(); } public function unused(): int { return 2; } } new T();",
    );
    assert!(has_function(&program, "helper"));
    assert!(has_method(&program, "T", "run"));
    assert!(!has_method(&program, "T", "unused"));
}

/// Verifies explicitly marked compiler-owned closure dispatch does not pin sibling methods.
#[test]
fn prune_ignores_internal_prelude_closure_dispatch_hazard() {
    let program = parse(
        "<?php class T { public function dispatch(callable $callback) { return $callback(); } public function sibling(): int { return 2; } } $t = new T(); echo $t->dispatch(fn() => 1);",
    );
    let mut check = crate::types::check(&program).expect("fixture must type check");
    let mut inventory = PreludeInventory::new();
    inventory.record_program("test", &program);
    inventory.record_internal_callable_method("test", "T", "dispatch", false);
    let roots = HashSet::new();
    let program = prune_unreachable_declarations(
        program,
        &mut check,
        PruneOptions {
            inventory: &inventory,
            forced_groups: &roots,
            structural_groups: &roots,
            exported_functions: &roots,
            eval_forced: false,
        },
    );
    assert!(has_method(&program, "T", "dispatch"));
    assert!(!has_method(&program, "T", "sibling"));
}

/// Verifies structural compiler roots retain dependencies without global dynamic widening.
#[test]
fn structural_prelude_group_does_not_promote_callable_hazards() {
    let program = parse(
        "<?php function helper(callable $callback): int { return $callback(); } function sibling(): int { return 2; } echo 1;",
    );
    let mut check = crate::types::check(&program).expect("fixture must type check");
    let mut inventory = PreludeInventory::new();
    inventory
        .group_mut("structural")
        .functions
        .insert("helper".to_string());
    let empty = HashSet::new();
    let structural = HashSet::from(["structural".to_string()]);
    let program = prune_unreachable_declarations(
        program,
        &mut check,
        PruneOptions {
            inventory: &inventory,
            forced_groups: &empty,
            structural_groups: &structural,
            exported_functions: &empty,
            eval_forced: false,
        },
    );

    assert!(has_function(&program, "helper"));
    assert!(!has_function(&program, "sibling"));
}

/// Verifies a declared object property narrows method reachability to its stored class.
#[test]
fn typed_property_receiver_does_not_keep_same_named_methods_on_other_live_classes() {
    let (program, _) = prune(
        "<?php
         class Item { public function target(): int { return 1; } }
         class Other { public function target(): int { return 9; } public function live(): int { return 2; } }
         class Holder {
             private Item $item;
             public function __construct() { $this->item = new Item(); }
             public function run(): int { return $this->item->target(); }
         }
         $holder = new Holder(); $other = new Other(); echo $holder->run(), $other->live();",
    );

    assert!(has_method(&program, "Item", "target"));
    assert!(has_method(&program, "Other", "live"));
    assert!(!has_method(&program, "Other", "target"));
}

/// Verifies `method_exists` observes static declarations as well as instance methods.
#[test]
fn prune_keeps_static_method_for_literal_method_exists() {
    let (program, _) = prune(
        "<?php class T { public static function hidden(): int { return 1; } } echo method_exists('T', 'hidden') ? 'y' : 'n';",
    );
    assert!(has_method(&program, "T", "hidden"));
}

/// Verifies instantiated classes retain static magic hooks such as `__set_state`.
#[test]
fn prune_keeps_static_magic_method_on_instantiated_class() {
    let (program, _) = prune(
        "<?php class T { public static function __set_state(array $values): T { return new T(); } public static function unused(): int { return 1; } } new T();",
    );
    assert!(has_method(&program, "T", "__set_state"));
    assert!(!has_method(&program, "T", "unused"));
}

/// Verifies any live class keeps its dynamic instance and static call hooks.
#[test]
fn prune_keeps_dynamic_call_magic_on_live_class() {
    let (program, _) = prune(
        "<?php class T { public function __call(string $name, array $args): string { return $name; } public static function __callStatic(string $name, array $args): string { return $name; } } echo T::missing();",
    );
    assert!(has_method(&program, "T", "__call"));
    assert!(has_method(&program, "T", "__callStatic"));
}
