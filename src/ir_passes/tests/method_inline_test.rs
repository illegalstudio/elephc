//! Purpose:
//! EIR-shape tests for the inliner's statically bound instance-method splices.
//!
//! Called from:
//! - `cargo test` through Rust's test harness (ir_passes).
//!
//! Key details:
//! - Fixtures lower real PHP through the `--emit-ir` frontend ordering for every supported
//!   target and then run the full EIR pipeline, so the inliner sees the bodies lowering emits.
//! - Each guard (dynamic dispatch, frame-dependent ops, backtraces, literal parameter guards)
//!   is checked by asserting that the `method_call` survives optimization.
//! - The positive fixture also emits assembly to prove the spliced private-property reads
//!   still resolve in the method's class instead of raising an access error.

use std::path::Path;

use crate::codegen::platform::Target;
use crate::ir::{Function, Immediate, Module, Op};

/// Every supported target; method splicing must behave the same on all of them.
const TARGETS: [&str; 5] = [
    "macos-aarch64",
    "ios-arm64",
    "ios-sim-arm64",
    "linux-aarch64",
    "linux-x86_64",
];

/// An Ivory-style entity filter: a final class with a private buffer of packed slots.
const ECS_SOURCE: &str = r#"<?php
packed class Slot { public bool $alive; public int $mask; }
final class Table
{
    private buffer<Slot> $slots;
    private int $high = 0;
    public function __construct(int $n) { $this->slots = buffer_new<Slot>($n); $this->high = $n; }
    public function highWater(): int { return $this->high; }
    public function matches(int $index, int $components): bool
    {
        return $this->slots[$index]->alive
            && ($this->slots[$index]->mask & $components) === $components;
    }
    public function next(): int { return $this->highWater() + 1; }
}
function scan(Table $t): int
{
    $n = $t->highWater();
    $hits = 0;
    for ($i = 0; $i < $n; $i++) {
        if (!$t->matches($i, 3)) { continue; }
        $hits = $hits + 1;
    }
    return $hits;
}
echo scan(new Table($argc + 9)), " ", (new Table($argc))->next(), "\n";
"#;

/// Lowers and optimizes `source` for one target, as `--emit-ir` does with `--ir-opt=on`.
fn optimized(source: &str, target: &str) -> Module {
    let mut module = crate::ir_lower::tests::lower_source_at_for_target(
        source,
        Path::new("main.php"),
        Path::new("."),
        Target::parse(target).expect("supported target"),
    );
    crate::ir_passes::optimize_module(&mut module);
    module
}

/// Returns the free function named `name`.
fn function<'a>(module: &'a Module, name: &str) -> &'a Function {
    module
        .functions
        .iter()
        .find(|function| function.name == name)
        .unwrap_or_else(|| panic!("missing function {name}"))
}

/// Returns the class method body named `name` (`Class::method`).
fn method<'a>(module: &'a Module, name: &str) -> &'a Function {
    module
        .class_methods
        .iter()
        .find(|function| function.name == name)
        .unwrap_or_else(|| panic!("missing method {name}"))
}

/// Returns the instructions placed in the function's blocks.
fn placed(function: &Function) -> impl Iterator<Item = &crate::ir::Instruction> {
    function
        .blocks
        .iter()
        .flat_map(|block| block.instructions.iter())
        .filter_map(|&id| function.instruction(id))
}

/// Returns the method names `function` still calls through `method_call`.
fn called_methods(module: &Module, function: &Function) -> Vec<String> {
    placed(function)
        .filter(|inst| inst.op == Op::MethodCall)
        .filter_map(|inst| match inst.immediate {
            Some(Immediate::Data(id)) => module.data.strings.get(id.as_raw() as usize).cloned(),
            _ => None,
        })
        .collect()
}

/// Returns the messages of the `throw_error` instructions placed in `function`.
fn thrown_errors(module: &Module, function: &Function) -> Vec<String> {
    placed(function)
        .filter(|inst| inst.op == Op::ThrowError)
        .filter_map(|inst| match inst.immediate {
            Some(Immediate::Data(id)) => module.data.strings.get(id.as_raw() as usize).cloned(),
            _ => None,
        })
        .collect()
}

/// A final-class accessor and predicate are spliced into a free function on every target:
/// the calls disappear, the receiver null check stays, the spliced instructions keep the
/// method's class scope, the method's locals lose their PHP names, and codegen still reads
/// the private properties instead of raising an access error.
#[test]
fn final_class_methods_are_spliced_with_their_scope_on_every_target() {
    for target in TARGETS {
        let module = optimized(ECS_SOURCE, target);
        let scan = function(&module, "scan");
        let calls = called_methods(&module, scan);
        assert!(
            calls.is_empty(),
            "{target}: scan still calls {calls:?}:\n{}",
            crate::ir::print_function(scan)
        );
        let errors = thrown_errors(&module, scan);
        for method in ["highWater", "matches"] {
            let message = format!("Call to a member function {method}() on null");
            assert!(errors.contains(&message), "{target}: missing receiver check {message:?}: {errors:?}");
        }
        assert!(
            placed(scan).any(|inst| inst.op == Op::IsNull),
            "{target}: the receiver check must test the receiver"
        );
        let property_reads: Vec<_> = scan
            .blocks
            .iter()
            .flat_map(|block| block.instructions.iter().copied())
            .filter(|&id| {
                scan.instruction(id).is_some_and(|inst| {
                    inst.op == Op::PropGet && !scan.is_packed_field_access(inst)
                })
            })
            .collect();
        assert!(!property_reads.is_empty(), "{target}: the spliced property reads are missing");
        for id in property_reads {
            assert_eq!(
                scan.inlined_scopes.get(&id).map(String::as_str),
                Some("Table"),
                "{target}: a spliced private-property read must keep the method's class scope"
            );
        }
        assert!(
            scan.locals.iter().all(|local| !matches!(
                local.name.as_deref(),
                Some("this" | "index" | "components")
            )),
            "{target}: spliced method locals must not keep their PHP names"
        );
        let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{target}: {error:?}"));
        assert!(
            !assembly.contains("Cannot access private property"),
            "{target}: a spliced private-property read was resolved from the caller's scope"
        );
    }
}

/// An accessor a final class inherits splices the PARENT's body: its private-property read
/// keeps the parent's class scope although the subclass shadows that property name, while the
/// subclass's own accessor keeps the subclass scope.
#[test]
fn inherited_accessor_on_a_final_subclass_keeps_the_parent_scope() {
    let source = r#"<?php
class Base
{
    private int $secret;
    public function __construct(int $secret) { $this->secret = $secret; }
    public function secret(): int { return $this->secret; }
}
final class Leaf extends Base
{
    private int $secret = 99;
    public function own(): int { return $this->secret; }
}
function read(Leaf $leaf): int { return $leaf->secret() * 1000 + $leaf->own(); }
echo read(new Leaf($argc + 6)), "\n";
"#;
    for target in TARGETS {
        let module = optimized(source, target);
        let read = function(&module, "read");
        let calls = called_methods(&module, read);
        assert!(calls.is_empty(), "{target}: read() still calls {calls:?}");
        let mut scopes: Vec<&str> = read
            .blocks
            .iter()
            .flat_map(|block| block.instructions.iter().copied())
            .filter(|&id| {
                read.instruction(id).is_some_and(|inst| {
                    inst.op == Op::PropGet && !read.is_packed_field_access(inst)
                })
            })
            .filter_map(|id| read.inlined_scopes.get(&id).map(String::as_str))
            .collect();
        scopes.sort_unstable();
        assert_eq!(
            scopes,
            vec!["Base", "Leaf"],
            "{target}: each spliced read must keep the scope of the body it came from"
        );
        let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{target}: {error:?}"));
        assert!(
            !assembly.contains("Cannot access private property"),
            "{target}: an inherited private-property read was resolved from the wrong scope"
        );
    }
}

/// A method called on the host method's own `$this` needs no receiver check: PHP never binds
/// `$this` to null, so the splice adds no `is_null` and no `throw_error`.
#[test]
fn host_this_receiver_is_spliced_without_a_null_check() {
    for target in TARGETS {
        let module = optimized(ECS_SOURCE, target);
        let next = method(&module, "Table::next");
        assert!(called_methods(&module, next).is_empty(), "{target}: next() still calls highWater()");
        assert!(thrown_errors(&module, next).is_empty(), "{target}: $this needs no null check");
        assert!(
            next.inlined_scopes.is_empty(),
            "{target}: a same-class splice runs in the host's own scope"
        );
    }
}

/// Calls the inliner must not bind statically keep their `method_call`: an overridable method
/// on a non-final class, and bodies whose lowering reads the enclosing frame (`static::`, a
/// static variable) or carries a literal `false` parameter guard.
#[test]
fn dynamic_and_frame_dependent_methods_keep_their_calls() {
    let source = r#"<?php
class Shape { public function sides(): int { return 0; } }
class Square extends Shape { public function sides(): int { return 4; } }
class Base { public function name(): string { return static::class; } }
final class Child extends Base {}
final class Ticker
{
    public function tick(): int { static $n = 0; $n = $n + 1; return $n; }
    public function strict(false $flag): int { return 1; }
}
function probe(Shape $shape, Child $child, Ticker $ticker): int
{
    return $shape->sides() + strlen($child->name()) + $ticker->tick() + $ticker->strict(false);
}
echo probe(new Square(), new Child(), new Ticker()), "\n";
"#;
    for target in TARGETS {
        let module = optimized(source, target);
        let calls = called_methods(&module, function(&module, "probe"));
        for method in ["sides", "name", "tick", "strict"] {
            assert!(
                calls.iter().any(|call| call == method),
                "{target}: {method}() must stay a real call: {calls:?}"
            );
        }
    }
}

/// A module that can observe PHP frames keeps every method call, so `debug_backtrace()`
/// frames never lose a method.
#[test]
fn backtrace_modules_keep_method_calls() {
    let source = r#"<?php
final class Probe { public function value(int $x): int { return $x + 1; } }
function trace(): int { return count(debug_backtrace()); }
function use_probe(Probe $p): int { return $p->value(1) + trace(); }
echo use_probe(new Probe()), "\n";
"#;
    for target in TARGETS {
        let module = optimized(source, target);
        assert!(module.calls_php_backtrace(), "{target}: the fixture must request a backtrace");
        let calls = called_methods(&module, function(&module, "use_probe"));
        assert!(
            calls.iter().any(|call| call == "value"),
            "{target}: backtrace-visible method frames must stay real calls: {calls:?}"
        );
    }
}
