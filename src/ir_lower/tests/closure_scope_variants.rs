//! Purpose:
//! Structural and all-target emission coverage for runtime closure scope specialization.
//!
//! Called from:
//! - The AST-to-EIR library test suite.
//!
//! Key details:
//! - Scope selection remains typed EIR metadata after closure identity becomes runtime-shaped.
//! - Original and rebound entries share capture layouts and can be rebound across scopes again.

use std::path::Path;

use crate::codegen::platform::Target;
use crate::ir::{Immediate, Op};

/// Unknown native closure identities retain explicit class/global scope across every backend.
#[test]
fn runtime_closure_scope_variants_compile_on_all_targets() {
    let source = r#"<?php
class ScopeOne { private string $value = "one"; }
class ScopeTwo { private string $value = "two"; }
class ScopePublic { public string $value = "public"; }
function bind_one(callable $closure): callable {
    return Closure::bind($closure, new ScopeOne(), ScopeOne::class);
}
function bind_two(callable $closure): callable {
    return $closure->bindTo(new ScopeTwo(), ScopeTwo::class);
}
function bind_object(callable $closure, ScopeOne $object): callable {
    return Closure::bind($closure, $object, $object);
}
function bind_boxed_scope(callable $closure, mixed $scope): callable {
    return Closure::bind($closure, new ScopePublic(), $scope);
}
$reader = function () { return $this->value; };
$one = bind_one($reader);
$two = bind_two($one);
$global = Closure::bind($two, new ScopePublic(), null);
$object = bind_object($reader, new ScopeOne());
$boxed = bind_boxed_scope($reader, new ScopePublic());
$null_scope = bind_boxed_scope($reader, null);
echo $one(), $two(), $global(), $object(), $boxed(), $null_scope();
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let mut module = super::lower_source_at_for_target(
            source, Path::new("main.php"), Path::new("."), Target::parse(name).unwrap(),
        );
        assert_eq!(module.closure_scope_variants.len(), 3, "{name}");
        for variant in &module.closure_scope_variants {
            assert_eq!(variant.source_names.len(), 4, "{name}");
            // `$this` plus the hidden runtime-scope capture a closure written outside any class
            // gets for its `$this->value` read; a variant keeps both slots.
            assert_eq!(variant.capture_count, 2, "{name}");
            let function = module.closures.iter().find(|function| {
                function.name == variant.function_name
            }).expect("scope variant function");
            assert_eq!(
                function.params.last().map(|param| param.name.as_str()),
                Some(crate::names::CLOSURE_SCOPE_LOCAL),
                "{name}"
            );
            let function = module.closures.iter().find(|function| {
                function.name == variant.function_name
            }).expect("scope variant function");
            let scope = function.lexical_class.as_ref().map(|name| {
                module.class_infos[name].class_id as u32
            });
            assert_eq!(scope, variant.scope_class_id, "{name}");
        }
        assert!(module.functions.iter().flat_map(|function| &function.instructions)
            .any(|instruction| instruction.op == Op::ClosureBind
                && matches!(instruction.immediate, Some(Immediate::ClosureBindScope(Some(_))))), "{name}");
        assert!(module.functions.iter().flat_map(|function| &function.instructions)
            .any(|instruction| instruction.op == Op::ClosureBind && instruction.operands.len() == 3
                && matches!(instruction.immediate, Some(Immediate::ClosureBindObjectScope(_)))), "{name}");
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
        crate::ir_passes::optimize_module(&mut module);
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{name} optimized: {error:?}"));
    }
}

/// A class-name string scope resolved at run time lowers and emits on every supported target,
/// like php, which accepts any string naming a class there.
#[test]
fn runtime_closure_bind_string_scope_compiles_on_all_targets() {
    let source = r#"<?php
class RuntimeScopeClass {}
function runtime_scope_name(int $n): string { return $n > 1 ? "RuntimeScopeClass" : "stdClass"; }
$reader = function () { return $this->value; };
$bound = Closure::bind($reader, new RuntimeScopeClass(), runtime_scope_name($argc));
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, Path::new("main.php"), Path::new("."), Target::parse(name).unwrap(),
        );
        assert!(module.functions.iter().flat_map(|function| &function.instructions)
            .any(|instruction| instruction.op == Op::ClosureBind
                && matches!(instruction.immediate, Some(Immediate::ClosureBindObjectScope(None)))), "{name}");
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
    }
}

/// Invalid source storage produces a diagnostic instead of allocating an impossible owner slot.
#[test]
fn native_closure_bind_rejects_non_closure_source_on_all_targets() {
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let error = super::try_lower_source_at_for_target(
            "<?php Closure::bind(42, null, null);", Path::new("main.php"), Path::new("."),
            Target::parse(name).unwrap(),
        ).expect_err("a scalar cannot be rebound as a closure");
        let crate::ir_lower::LoweringError::Unsupported(error) = error else {
            panic!("{name}: unexpected lowering error {error:?}");
        };
        assert!(error.message.contains("Native closure binding requires a closure value"),
            "{name}: {}", error.message);
    }
}

/// A scope known only at run time asks for no compile-time variant at all: the bind falls back to
/// the plain bind plus the run-time scope stamp. Cloning every `$this` closure for every class
/// cost closures x classes bodies (71,280 for Symfony) and ran codegen out of memory.
#[test]
fn runtime_scope_bind_adds_no_class_variants() {
    let source = r#"<?php
class ScopeA { private string $value = "a"; }
class ScopeB { private string $value = "b"; }
class ScopeC { private string $value = "c"; }
function bind_local(mixed $scope): callable {
    $reader = function () { return $this->value; };
    return Closure::bind($reader, new ScopeA(), $scope);
}
function bind_any(callable $closure, mixed $scope): callable {
    return Closure::bind($closure, new ScopeB(), $scope);
}
function never_bound(): callable {
    return function () { return $this->value; };
}
echo bind_local(ScopeB::class)(), bind_any(never_bound(), ScopeC::class)();
"#;
    let module = super::lower_source_at_for_target(
        source, Path::new("main.php"), Path::new("."), Target::parse("macos-aarch64").unwrap(),
    );
    let variants = &module.closure_scope_variants;
    // Counter assertion: no per-class clone, however many classes the program declares.
    assert!(
        variants.iter().all(|variant| variant.scope_class_id.is_none()),
        "runtime scopes must not be specialized per class: {:?}",
        variants.iter().map(|variant| variant.scope_class_id).collect::<Vec<_>>()
    );
    assert!(variants.len() <= 2, "{} variants", variants.len());
    crate::codegen::generate_user_asm_from_ir(&module, false, false).expect("codegen");
}

/// A string literal naming a class the program never declares is a run-time scope, not a refusal:
/// php resolves the name when the bind executes (Symfony binds `Request::class` spelled as text in
/// code whose class the closed world does not include).
#[test]
fn literal_scope_naming_an_undeclared_class_lowers_as_a_runtime_scope() {
    let source = r#"<?php
$reader = function () { return 1; };
$bound = Closure::bind($reader, null, "Undeclared\\ScopeClass");
echo isset($bound) ? "bound" : "none";
"#;
    let module = super::lower_source_at_for_target(
        source, Path::new("main.php"), Path::new("."), Target::parse("macos-aarch64").unwrap(),
    );
    assert!(module.functions.iter().flat_map(|function| &function.instructions)
        .any(|instruction| instruction.op == Op::ClosureBind
            && matches!(instruction.immediate, Some(Immediate::ClosureBindObjectScope(None)))));
    assert!(module.closure_scope_variants.iter().all(|variant| variant.scope_class_id.is_none()));
}
