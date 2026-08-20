//! Purpose:
//! End-to-end assembler and runtime tests for whole-program declaration reachability.
//!
//! Called from:
//! - `cargo test` through Rust's integration-test harness.
//!
//! Key details:
//! - Symbol assertions prove declarations disappear instead of relying only on preserved stdout.
//! - PDO fixtures cover prelude classes, method pruning, and bridge requirements together.

use super::*;

/// Verifies an unreferenced user function is absent while its reachable sibling is emitted.
#[test]
fn test_unused_user_function_is_absent_from_assembly() {
    let dir = make_cli_test_dir("elephc_decl_reach_unused_fn");
    let (user_asm, _, _) = compile_source_to_asm_with_options(
        "<?php
        function used(): int { return 1; }
        function unused(): int { return 2; }
        echo used();
        ",
        &dir,
        8_388_608,
        false,
        false,
    );
    let used = elephc::names::function_symbol("used");
    let unused = elephc::names::function_symbol("unused");
    assert!(user_asm.contains(&used), "reachable function must be emitted");
    assert!(
        !user_asm.contains(&format!(".globl {unused}\n")),
        "unused function must not be emitted: {user_asm}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies pruning an unused function preserves the executable program result.
#[test]
fn test_unused_user_function_program_still_runs() {
    let out = compile_and_run(
        "<?php
        function used(): int { return 1; }
        function unused(): int { return 2; }
        echo used();
        ",
    );
    assert_eq!(out, "1");
}

/// Verifies exception-aware DCE removes a disjoint catch before declaration reachability scans it.
#[test]
fn test_exception_dce_exposes_catch_only_function_to_reachability() {
    let dir = make_cli_test_dir("elephc_decl_reach_exception_dce");
    let (user_asm, _, _) = compile_source_to_asm_with_options(
        "<?php
        class A extends Exception {}
        class B extends Exception {}
        function catchOnly(): int { return 9; }
        try {
            throw new A('a');
        } catch (B $error) {
            echo catchOnly();
        } catch (A $error) {
            echo 'ok';
        }
        ",
        &dir,
        8_388_608,
        false,
        false,
    );
    let catch_only = elephc::names::function_symbol("catchOnly");
    assert!(
        !user_asm.contains(&format!(".globl {catch_only}\n")),
        "a function referenced only by a disjoint catch must be pruned: {user_asm}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies an entirely unused class does not leave a method body in user assembly.
#[test]
fn test_unused_user_class_is_absent_from_assembly() {
    let dir = make_cli_test_dir("elephc_decl_reach_unused_class");
    let (user_asm, _, _) = compile_source_to_asm_with_options(
        "<?php class NeverUsed { public function hidden(): int { return 2; } } echo 1;",
        &dir,
        8_388_608,
        false,
        false,
    );
    assert!(
        !user_asm.contains(&elephc::names::method_symbol("NeverUsed", "hidden")),
        "unused class method must not be emitted"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies a literal method probe retains the named method on its live class.
#[test]
fn test_method_exists_literal_keeps_method() {
    let dir = make_cli_test_dir("elephc_decl_reach_method_exists");
    let source = "<?php
        class T { public function hidden(): int { return 1; } }
        $t = new T();
        echo method_exists($t, 'hidden') ? 'y' : 'n';
    ";
    let (user_asm, _, _) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    assert!(
        user_asm.contains(&elephc::names::method_symbol("T", "hidden")),
        "literal method_exists must retain the probed method"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies a retained literal method probe remains observable at runtime.
#[test]
fn test_method_exists_literal_still_runs() {
    let out = compile_and_run(
        "<?php
        class T { public function hidden(): int { return 1; } }
        $t = new T();
        echo method_exists($t, 'hidden') ? 'y' : 'n';
        ",
    );
    assert_eq!(out, "y");
}

/// Verifies literal method probing retains an otherwise uncalled static method.
#[test]
fn test_method_exists_literal_keeps_static_method() {
    let out = compile_and_run(
        "<?php
        class StaticProbe { public static function hidden(): int { return 1; } }
        echo method_exists('StaticProbe', 'hidden') ? 'y' : 'n';
        ",
    );
    assert_eq!(out, "y");
}

/// Verifies reordered named arguments retain the class and method probed by `method_exists`.
#[test]
fn test_method_exists_named_arguments_keep_static_method() {
    let out = compile_and_run(
        "<?php
        class NamedMethodProbe { public static function hidden(): int { return 1; } }
        echo method_exists(method: 'hidden', object_or_class: 'NamedMethodProbe') ? 'y' : 'n';
        ",
    );
    assert_eq!(out, "y");
}

/// Verifies a class-string property probe retains otherwise unreachable class metadata.
#[test]
fn test_property_exists_class_string_keeps_class_metadata() {
    let out = compile_and_run(
        "<?php
        class PropertyProbe { public int $hidden = 1; }
        echo property_exists('PropertyProbe', 'hidden') ? 'y' : 'n';
        ",
    );
    assert_eq!(out, "y");
}

/// Verifies a literal function probe retains an otherwise uncalled user declaration.
#[test]
fn test_function_exists_literal_keeps_user_function() {
    let out = compile_and_run(
        "<?php
        function hidden_by_probe(): int { return 1; }
        echo function_exists('hidden_by_probe') ? 'y' : 'n';
        ",
    );
    assert_eq!(out, "y");
}

/// Verifies runtime method-name dispatch retains every candidate method on a live class.
#[test]
fn test_dynamic_method_name_keeps_live_class_methods() {
    let out = compile_and_run(
        "<?php
        class DynamicTarget {
            public function first(): string { return 'first'; }
            public function second(): string { return 'second'; }
        }
        $target = new DynamicTarget();
        $method = 'second';
        echo $target->$method();
        ",
    );
    assert_eq!(out, "second");
}

/// Verifies conditional receiver assignments retain every class that can reach the call site.
#[test]
fn test_conditional_receiver_assignments_keep_both_method_bodies() {
    let out = compile_and_run(
        "<?php
        class A { public function foo(): string { return 'A'; } }
        class B { public function foo(): string { return 'B'; } }
        function dispatch(bool $useB): string {
            A|B $receiver = new A();
            if ($useB) { $receiver = new B(); }
            return $receiver->foo();
        }
        echo dispatch(false), dispatch(true);
        ",
    );
    assert_eq!(out, "AB");
}

/// Verifies a foreach value binding invalidates an earlier receiver-class refinement.
#[test]
fn test_foreach_receiver_binding_keeps_runtime_method_body() {
    let out = compile_and_run(
        "<?php
        class A { public function foo(): string { return 'A'; } }
        class B { public function foo(): string { return 'B'; } }
        function dispatch(bool $useB): string {
            A|B $receiver = new A();
            $items = $useB ? [new B()] : [];
            foreach ($items as $receiver) {}
            return $receiver->foo();
        }
        echo dispatch(false), dispatch(true);
        ",
    );
    assert_eq!(out, "AB");
}

/// Verifies a reachable function can replace a global receiver without losing its method body.
#[test]
fn test_global_alias_keeps_runtime_receiver_method_body() {
    let out = compile_and_run(
        "<?php
        class A { public function foo(): string { return 'A'; } }
        class B extends A { public function foo(): string { return 'B'; } }
        function later(): void {
            global $receiver;
            $receiver = new B();
        }
        $receiver = new A();
        later();
        echo $receiver->foo();
        ",
    );
    assert_eq!(out, "B");
}

/// Verifies `$GLOBALS['name']` conservatively retains the aliased receiver method body.
#[test]
fn test_globals_array_alias_keeps_receiver_method_body() {
    let dir = make_cli_test_dir("elephc_decl_reach_globals_alias");
    let (user_asm, _, _) = compile_source_to_asm_with_options(
        "<?php
        class A { public function foo(): string { return 'A'; } }
        class B extends A { public function foo(): string { return 'B'; } }
        function later(): void {
            $GLOBALS['receiver'] = new B();
        }
        $receiver = new A();
        later();
        echo $receiver->foo();
        ",
        &dir,
        8_388_608,
        false,
        false,
    );
    assert!(
        user_asm.contains(&elephc::names::method_symbol("B", "foo")),
        "the `$GLOBALS` alias must retain B::foo even before runtime alias semantics are available"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies a by-ref callee can replace a differently named caller receiver safely.
#[test]
fn test_by_ref_parameter_keeps_runtime_receiver_method_body() {
    let out = compile_and_run(
        "<?php
        class A { public function foo(): string { return 'A'; } }
        class B extends A { public function foo(): string { return 'B'; } }
        function replace(&$slot): void {
            $slot = new B();
        }
        $receiver = new A();
        replace(slot: $receiver);
        echo $receiver->foo();
        ",
    );
    assert_eq!(out, "B");
}

/// Verifies direct method signatures invalidate caller storage passed by reference.
#[test]
fn test_by_ref_method_parameter_keeps_runtime_receiver_method_body() {
    let out = compile_and_run(
        "<?php
        class A { public function foo(): string { return 'A'; } }
        class B extends A { public function foo(): string { return 'B'; } }
        class Replacer {
            public function replace(&$slot): void {
                $slot = new B();
            }
        }
        $receiver = new A();
        (new Replacer())->replace(slot: $receiver);
        echo $receiver->foo();
        ",
    );
    assert_eq!(out, "B");
}

/// Verifies constructor by-ref parameters invalidate caller receiver facts too.
#[test]
fn test_by_ref_constructor_parameter_keeps_runtime_receiver_method_body() {
    let out = compile_and_run(
        "<?php
        class A { public function foo(): string { return 'A'; } }
        class B extends A { public function foo(): string { return 'B'; } }
        class Replacer {
            public function __construct(A &$slot) {
                $slot = new B();
            }
        }
        $receiver = new A();
        new Replacer(slot: $receiver);
        echo $receiver->foo();
        ",
    );
    assert_eq!(out, "B");
}

/// Verifies late-static construction retains and invokes an overriding child constructor.
#[test]
fn test_new_static_keeps_runtime_subclass_constructor() {
    let out = compile_and_run(
        "<?php
        class BaseFactory {
            public static function make(): static { return new static(); }
        }
        class ChildFactory extends BaseFactory {
            public function __construct() { echo 'child'; }
        }
        ChildFactory::make();
        ",
    );
    assert_eq!(out, "child");
}

/// Verifies descendant constructor signatures invalidate late-static by-ref arguments.
#[test]
fn test_new_static_uses_runtime_subclass_constructor_signature() {
    let out = compile_and_run(
        "<?php
        class A { public function value(): string { return 'A'; } }
        class B extends A { public function value(): string { return 'B'; } }
        class BaseFactory {
            public function __construct(A $slot) {}
            public static function make(A $slot): string {
                new static(slot: $slot);
                return $slot->value();
            }
        }
        class ChildFactory extends BaseFactory {
            public function __construct(A &$slot) { $slot = new B(); }
        }
        echo ChildFactory::make(new A());
        ",
    );
    assert_eq!(out, "B");
}

/// Verifies interface contracts and `parent::` dispatch retain the inherited implementation.
#[test]
fn test_parent_dispatch_and_interface_contract_still_run() {
    let out = compile_and_run(
        "<?php
        interface ValueSource { public function value(): int; }
        class ParentValue implements ValueSource {
            public function value(): int { return 42; }
            public function unused(): int { return 0; }
        }
        class ChildValue extends ParentValue {
            public function read(): int { return parent::value(); }
        }
        echo (new ChildValue())->read();
        ",
    );
    assert_eq!(out, "42");
}

/// Verifies `parent::` retains the immediate parent's body when the child overrides the method.
#[test]
fn test_parent_dispatch_bypasses_child_override_after_pruning() {
    let out = compile_and_run(
        "<?php
        class ParentValue {
            protected function value(): int { return 42; }
        }
        class ChildValue extends ParentValue {
            protected function value(): int { return 99; }
            public function read(): int { return parent::value(); }
        }
        echo (new ChildValue())->read();
        ",
    );
    assert_eq!(out, "42");
}

/// Verifies a `parent::` call cannot desynchronize parent and child instance-vtable slots.
#[test]
fn test_parent_scoped_call_keeps_instance_slots_aligned_across_the_chain() {
    let out = compile_and_run(
        "<?php
        class SlotBase {
            public function shadowed(): string { return 'base'; }
            public function later(): string { return 'later'; }
        }
        class SlotChild extends SlotBase {
            public function shadowed(): string { return 'child'; }
            public function boot(): string { return parent::shadowed(); }
        }
        function through_base(SlotBase $value): string { return $value->later(); }
        $child = new SlotChild();
        $child->boot();
        echo through_base($child);
        ",
    );
    assert_eq!(out, "later");
}

/// Verifies scoped parent retention cannot desynchronize late-bound static-vtable slots.
#[test]
fn test_parent_scoped_call_keeps_static_slots_aligned_across_the_chain() {
    let out = compile_and_run(
        "<?php
        class StaticSlotBase {
            public static function shadowed(): string { return 'base'; }
            public static function dispatch(): string { return static::later(); }
            public static function later(): string { return 'later'; }
        }
        class StaticSlotChild extends StaticSlotBase {
            public static function shadowed(): string { return 'child'; }
            public static function boot(): string { return parent::shadowed(); }
        }
        StaticSlotChild::boot();
        echo StaticSlotChild::dispatch();
        ",
    );
    assert_eq!(out, "later");
}

/// Verifies `count()` still executes a dynamic method inside Countable::count after pruning.
#[test]
fn test_countable_protocol_body_keeps_dynamic_method_target() {
    let out = compile_and_run(
        "<?php
        class Items implements Countable {
            public function count(): int {
                $name = 'target';
                $this->$name();
                return 1;
            }
            public function target(): void { echo 'hit'; }
        }
        echo count(new Items());
        ",
    );
    assert_eq!(out, "hit1");
}

/// Verifies foreach still executes a dynamic method inside Iterator::rewind after pruning.
#[test]
fn test_iterator_protocol_body_keeps_dynamic_method_target() {
    let out = compile_and_run(
        "<?php
        class Items implements Iterator {
            public function rewind(): void {
                $name = 'target';
                $this->$name();
            }
            public function current(): mixed { return 1; }
            public function key(): mixed { return 0; }
            public function next(): void {}
            public function valid(): bool { return false; }
            public function target(): void { echo 'hit'; }
        }
        foreach (new Items() as $value) {}
        ",
    );
    assert_eq!(out, "hit");
}

/// Verifies a mid-chain `parent::` call cannot desynchronize the grandparent instance vtable.
#[test]
fn test_parent_scoped_call_keeps_grandparent_instance_slots_aligned() {
    let out = compile_and_run(
        "<?php
        class SlotRoot {
            public function shadowed(): string { return 'root'; }
            public function later(): string { return 'later'; }
        }
        class SlotMid extends SlotRoot {
            public function shadowed(): string { return 'mid'; }
        }
        class SlotLeaf extends SlotMid {
            public function boot(): string { return parent::shadowed(); }
        }
        function through_root(SlotRoot $value): string { return $value->later(); }
        $leaf = new SlotLeaf();
        $leaf->boot();
        echo through_root($leaf);
        ",
    );
    assert_eq!(out, "later");
}

/// Verifies a mid-chain `parent::` call cannot desynchronize an abstract grandparent vtable.
#[test]
fn test_parent_scoped_call_keeps_abstract_grandparent_slots_aligned() {
    let out = compile_and_run(
        "<?php
        abstract class SlotRoot {
            abstract public function shadowed(): string;
            public function later(): string { return 'later'; }
        }
        class SlotMid extends SlotRoot {
            public function shadowed(): string { return 'mid'; }
        }
        class SlotLeaf extends SlotMid {
            public function boot(): string { return parent::shadowed(); }
        }
        function through_root(SlotRoot $value): string { return $value->later(); }
        $leaf = new SlotLeaf();
        $leaf->boot();
        echo through_root($leaf);
        ",
    );
    assert_eq!(out, "later");
}

/// Verifies a mid-chain static `parent::` call cannot desynchronize late-bound static slots.
#[test]
fn test_parent_scoped_call_keeps_grandparent_static_slots_aligned() {
    let out = compile_and_run(
        "<?php
        class StaticSlotRoot {
            public static function shadowed(): string { return 'root'; }
            public static function dispatch(): string { return static::later(); }
            public static function later(): string { return 'later'; }
        }
        class StaticSlotMid extends StaticSlotRoot {
            public static function shadowed(): string { return 'mid'; }
        }
        class StaticSlotLeaf extends StaticSlotMid {
            public static function boot(): string { return parent::shadowed(); }
        }
        StaticSlotLeaf::boot();
        echo StaticSlotLeaf::dispatch();
        ",
    );
    assert_eq!(out, "later");
}

/// Verifies a runtime-generated TypeError retains its checker-injected class metadata after pruning.
#[test]
fn test_synthetic_type_error_metadata_survives_declaration_pruning() {
    let out = compile_and_run(
        "<?php
        function unused_source_declaration(): int { return 0; }
        enum Level: int { case Low = 1; }
        try {
            Level::from('not-an-int');
        } catch (Throwable $error) {
            echo get_class($error), ':', $error->getMessage();
        }
        ",
    );
    assert_eq!(
        out,
        "TypeError:Level::from(): Argument #1 ($value) must be of type int, string given"
    );
}

/// Verifies registry-backed builtin callbacks retain their literal user function target.
#[test]
fn test_builtin_callback_function_still_runs() {
    let out = compile_and_run(
        "<?php
        function double_value(int $value): int { return $value * 2; }
        echo implode(',', array_map('double_value', [1, 2, 3]));
        ",
    );
    assert_eq!(out, "2,4,6");
}

/// Verifies trait methods are retained from the checker's flattened method declarations.
#[test]
fn test_flattened_trait_method_still_runs() {
    let out = compile_and_run(
        "<?php
        function trait_value(): int { return 42; }
        trait ValueTrait {
            public function value(): int { return trait_value(); }
            public function unused(): int { return 0; }
        }
        class UsesValue { use ValueTrait; }
        echo (new UsesValue())->value();
        ",
    );
    assert_eq!(out, "42");
}

/// Verifies a trait alias is retained from the consuming class's flattened method surface.
#[test]
fn test_flattened_trait_alias_still_runs() {
    let out = compile_and_run(
        "<?php
        function aliased_trait_value(): int { return 43; }
        trait AliasTrait {
            public function original(): int { return aliased_trait_value(); }
        }
        class UsesAlias { use AliasTrait { original as aliased; } }
        echo (new UsesAlias())->aliased();
        ",
    );
    assert_eq!(out, "43");
}

/// Verifies an inherited instance call retains the checker-selected parent implementation.
#[test]
fn test_inherited_instance_method_still_runs() {
    let out = compile_and_run(
        "<?php
        class ParentValue {
            public function value(): int { return 44; }
            public function unused(): int { return 0; }
        }
        class ChildValue extends ParentValue {}
        echo (new ChildValue())->value();
        ",
    );
    assert_eq!(out, "44");
}

/// Verifies eval can still resolve a user function that has no static call edge.
#[test]
fn test_eval_keeps_user_function_observable() {
    let out = compile_and_run(
        "<?php
        function hidden_from_static_graph(): string { return 'eval'; }
        eval('echo hidden_from_static_graph();');
        ",
    );
    assert_eq!(out, "eval");
}

/// Verifies unserialize retains class metadata and the class's implicit wakeup hook.
#[test]
fn test_unserialize_keeps_user_class_and_magic_method() {
    let out = compile_and_run(
        "<?php
        class HiddenPayload {
            public function __wakeup(): void { echo 'awake'; }
            public function unused(): void { echo 'unused'; }
        }
        $class = 'HiddenPayload';
        $payload = new $class();
        unserialize(serialize($payload));
        ",
    );
    assert_eq!(out, "awake");
}

/// Verifies query-only PDO use omits an unrelated driver class and keeps its bridge.
#[test]
fn test_pdo_query_only_drops_unused_sibling_classes() {
    let dir = make_cli_test_dir("elephc_decl_reach_pdo_siblings");
    let (user_asm, _, libraries) = compile_source_to_asm_with_options(
        "<?php
        $pdo = new PDO('sqlite::memory:');
        echo $pdo->query('select 1')->fetchColumn();
        ",
        &dir,
        8_388_608,
        false,
        false,
    );
    assert!(
        !user_asm.contains(&format!(
            ".globl {}\n",
            elephc::names::method_symbol("Pdo\\Mysql", "__construct")
        )),
        "unused PDO driver class must not emit its constructor"
    );
    assert!(
        libraries.iter().any(|library| library == "elephc_pdo"),
        "used PDO program must still link the PDO bridge"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies query-only PDO use omits an unrelated method on the live base class.
#[test]
fn test_pdo_query_only_omits_begin_transaction_method() {
    let dir = make_cli_test_dir("elephc_decl_reach_pdo_methods");
    let (user_asm, _, _) = compile_source_to_asm_with_options(
        "<?php
        $pdo = new PDO('sqlite::memory:');
        echo $pdo->query('select 1')->fetchColumn();
        ",
        &dir,
        8_388_608,
        false,
        false,
    );
    assert!(
        !user_asm.contains(&elephc::names::method_symbol("PDO", "begintransaction")),
        "unused PDO::beginTransaction must not be emitted"
    );
    assert!(
        user_asm.contains(&elephc::names::method_symbol("PDO", "__construct"))
            || user_asm.contains(&elephc::names::method_symbol("PDO", "query")),
        "used PDO methods must remain"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies the public CLI applies the same PDO method pruning as the in-process fixture pipeline.
#[test]
fn test_cli_pdo_query_only_omits_begin_transaction_method() {
    let dir = make_cli_test_dir("elephc_decl_reach_cli_pdo_methods");
    let php_path = dir.join("main.php");
    fs::write(
        &php_path,
        "<?php
        $pdo = new PDO('sqlite::memory:');
        echo $pdo->query('select 1')->fetchColumn();
        ",
    )
    .expect("failed to write PDO CLI fixture");

    let output = elephc_cli_command(&dir)
        .arg("--emit-asm")
        .arg(&php_path)
        .output()
        .expect("failed to compile PDO fixture through the CLI");
    assert!(
        output.status.success(),
        "elephc --emit-asm failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let user_asm = fs::read_to_string(dir.join("main.s")).expect("failed to read PDO assembly");
    let begin_transaction = elephc::names::method_symbol("PDO", "begintransaction");
    assert!(
        !user_asm.contains(&format!(".globl {begin_transaction}\n")),
        "unused PDO::beginTransaction must not be emitted by the public CLI"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies the public CLI dead-strips, links, and executes the pruned query-only PDO surface.
#[test]
fn test_pdo_query_only_still_runs() {
    let out = compile_cli_file_and_run(
        "<?php
        $pdo = new PDO('sqlite::memory:');
        echo $pdo->query('select 1')->fetchColumn();
        ",
        &[],
    );
    assert_eq!(out, "1");
}

/// Verifies unrelated programs do not acquire the PDO bridge through dormant prelude metadata.
#[test]
fn test_program_without_pdo_still_does_not_link_bridge() {
    let dir = make_cli_test_dir("elephc_decl_reach_no_pdo");
    let (_, _, libraries) =
        compile_source_to_asm_with_options("<?php echo 1;", &dir, 8_388_608, false, false);
    assert!(
        libraries.iter().all(|library| library != "elephc_pdo"),
        "an unrelated program must not link the PDO bridge"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies executable user assembly respects the safe target-specific dead-strip boundary.
#[test]
fn test_user_assembly_respects_linker_dead_strip_boundaries() {
    let dir = make_cli_test_dir("elephc_user_dead_strip_shape");
    let (user_asm, _, _) =
        compile_source_to_asm_with_options("<?php echo 1;", &dir, 8_388_608, false, false);
    if cfg!(target_os = "macos") {
        assert!(
            !user_asm.contains(".subsections_via_symbols"),
            "macOS user assembly must remain intact for address-taken callable labels"
        );
    } else {
        assert!(
            user_asm.contains(".section .text."),
            "Linux user assembly must use per-function sections for --gc-sections"
        );
    }
    let _ = fs::remove_dir_all(&dir);
}
