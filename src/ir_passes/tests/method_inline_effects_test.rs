//! Purpose:
//! Covers method splices containing nested calls, property writes and string operations.
//!
//! Called from:
//! - The parent method-inlining EIR tests on every supported target.
//!
//! Key details:
//! - Asserts actual call removal, lexical scopes, nested diagnostic spans and recursion guards.

use super::*;

/// Methods combining explicit static calls, real free calls and private writes can be spliced.
#[test]
fn method_effects_splice_nested_calls_and_writes_on_every_target() {
    let source = r#"<?php
function outside(int $n): int { for ($i = 0; $i < 2; $i++) { $n = $n + 1; } return $n; }
final class Entity { public static function index(int $id): int { return $id & 255; } }
final class Counter {
    private int $count = 0;
    public function add(int $id): int { $this->count = outside(Entity::index($id)); return $this->count; }
    public function twice(int $id): int { return $this->add($id) + $this->count; }
}
function exercise(Counter $counter, int $id): int { return $counter->twice($id); }
echo exercise(new Counter(), $argc), "\n";
"#;
    let expected_line = source.lines().position(|line| line.contains("outside(Entity::index")).unwrap() + 1;
    for target in TARGETS {
        let module = optimized(source, target);
        let exercise = function(&module, "exercise");
        assert!(called_methods(&module, exercise).is_empty(),
            "{target}: nested method calls survived: {}", crate::ir::print_function(exercise));
        let nested: Vec<_> = placed(exercise)
            .filter(|inst| matches!(inst.op, Op::Call | Op::StaticMethodCall)).collect();
        assert!(nested.iter().any(|inst| inst.op == Op::StaticMethodCall),
            "{target}: the explicit static call must remain in the spliced body");
        assert!(nested.iter().any(|inst| inst.op == Op::Call),
            "{target}: the non-inlineable nested free call must remain");
        for inst in nested {
            assert_eq!(inst.span.map(|span| span.line as usize), Some(expected_line),
                "{target}: nested calls must publish the original method-body line");
        }
        let writes: Vec<_> = exercise.blocks.iter().flat_map(|block| block.instructions.iter())
            .filter(|&&id| exercise.instruction(id).is_some_and(|inst| inst.op == Op::PropSet)).collect();
        assert!(!writes.is_empty(), "{target}: no spliced property write");
        for id in writes {
            assert_eq!(exercise.inlined_scopes.get(id).map(String::as_str), Some("Counter"),
                "{target}: the private write lost its method scope");
        }
        let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{target}: {error:?}"));
        assert!(!assembly.contains("Cannot access private property"), "{target}");
    }
}

/// String-building methods splice while preserving the host's surrounding concatenation.
#[test]
fn method_effects_splice_string_building_on_every_target() {
    let source = r#"<?php
final class Label { public function wrap(string $name): string { return '[' . $name . ']'; } }
function decorate(Label $label, string $name): string { return 'before:' . $label->wrap($name) . ':after'; }
echo decorate(new Label(), 'item'), "\n";
"#;
    for target in TARGETS {
        let module = optimized(source, target);
        let decorate = function(&module, "decorate");
        assert!(called_methods(&module, decorate).is_empty(),
            "{target}: string-building method stayed a real call");
        assert!(placed(decorate).any(|inst| matches!(inst.op, Op::StrConcat | Op::StrAppend)),
            "{target}: expected concatenation after the splice");
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{target}: {error:?}"));
    }
}

/// A recursive cycle through a static body stays real, and lexical static calls remain excluded.
#[test]
fn method_effects_keep_static_cycles_and_lexical_calls_real() {
    let source = r#"<?php
final class Recursive {
    public function step(int $n): int { return Bridge::step($n); }
    public static function identity(int $n): int { return $n; }
    public function lexical(int $n): int { return self::identity($n); }
}
final class Bridge {
    public static function step(int $n): int {
        if ($n <= 0) { return 0; }
        return (new Recursive())->step($n - 1);
    }
}
function probe(Recursive $recursive, int $n): int { return $recursive->step($n) + $recursive->lexical($n); }
echo probe(new Recursive(), $argc), "\n";
"#;
    for target in TARGETS {
        let module = optimized(source, target);
        let calls = called_methods(&module, function(&module, "probe"));
        for name in ["step", "lexical"] {
            assert!(calls.iter().any(|call| call == name), "{target}: {name} must stay real: {calls:?}");
        }
    }
}

/// A setter keeps its real boundary in an initializer even when the lexical class matches.
#[test]
fn method_effects_keep_setters_out_of_initializer_frames() {
    let source = r#"<?php
final class Setter {
    private int $value = 0;
    public function set(int $value): void { $this->value = $value; }
}
function initialize(Setter $setter, int $value): void { $setter->set($value); }
initialize(new Setter(), $argc);
"#;
    for target in TARGETS {
        let mut module = crate::ir_lower::tests::lower_source_at_for_target(
            source, Path::new("main.php"), Path::new("."), Target::parse(target).unwrap(),
        );
        let host = module.functions.iter_mut().find(|function| function.name == "initialize").unwrap();
        host.name = "_class_propinit_999".to_string();
        host.flags.is_synthetic = true;
        host.lexical_class = Some("Setter".to_string());
        for name in &mut module.data.function_names {
            if name == "initialize" { *name = "_class_propinit_999".to_string(); }
        }
        crate::ir_passes::inline::inline_small_functions(&mut module);
        let calls = called_methods(&module, function(&module, "_class_propinit_999"));
        assert!(calls.iter().any(|call| call == "set"),
            "{target}: a setter must not inherit initializer-only store compatibility");
    }
}

/// Variance keeps real calls for ordinary and final methods on instantiated final-class receivers.
#[test]
fn method_effects_keep_instantiated_final_class_calls_real() {
    let source = r#"<?php
class InlineAnimal {}
class InlineDog extends InlineAnimal {}
final class VarianceBox<+T> {
    public function __construct(private T $value) {}
    public function kind(): string { return 'kind'; }
    final public function finalKind(): string { return 'final'; }
}
function inspect(VarianceBox<InlineAnimal> $box): string { return $box->kind() . $box->finalKind(); }
echo inspect(new VarianceBox<InlineDog>(new InlineDog())), "\n";
"#;
    for target in TARGETS {
        let module = optimized(source, target);
        let calls = called_methods(&module, function(&module, "inspect"));
        for name in ["kind", "finalKind"] {
            assert!(calls.iter().any(|call| call == name),
                "{target}: {name} must stay a real call: {calls:?}");
        }
    }
}

/// A source concat assignment keeps its frame because its store also resets that frame's scratch.
#[test]
fn method_effects_keep_implicit_concat_store_resets_in_the_callee() {
    let source = r#"<?php
final class LocalLabel {
    public function wrap(string $name): string { $local = '[' . $name; return $local . ']'; }
}
function decorate(LocalLabel $label, string $name): string { return $label->wrap($name); }
echo decorate(new LocalLabel(), 'item'), "\n";
"#;
    for target in TARGETS {
        let module = optimized(source, target);
        let calls = called_methods(&module, function(&module, "decorate"));
        assert!(calls.iter().any(|call| call == "wrap"),
            "{target}: the implicit local-store reset must stay in a real frame");
    }
}
