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
        assert_eq!(module.closure_scope_variants.len(), 4, "{name}");
        for variant in &module.closure_scope_variants {
            assert_eq!(variant.source_names.len(), 5, "{name}");
            assert_eq!(variant.capture_count, 1, "{name}");
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

/// Runtime string scopes fail native lowering on every supported target.
#[test]
fn runtime_closure_bind_string_scope_is_refused_on_all_targets() {
    let source = r#"<?php
class RuntimeScopeClass {}
function runtime_scope_name(int $n): string { return $n > 1 ? "RuntimeScopeClass" : "stdClass"; }
$reader = function () { return $this->value; };
$bound = Closure::bind($reader, new RuntimeScopeClass(), runtime_scope_name($argc));
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let error = super::try_lower_source_at_for_target(
            source, Path::new("main.php"), Path::new("."), Target::parse(name).unwrap(),
        ).expect_err("native scope strings must be refused");
        let crate::ir_lower::LoweringError::Unsupported(error) = error else {
            panic!("{name}: unexpected lowering error {error:?}");
        };
        assert!(error.message.contains("Native closure binding needs a literal user class scope"),
            "{name}: {}", error.message);
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
