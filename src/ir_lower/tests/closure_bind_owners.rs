//! Purpose:
//! Structural coverage for Closure rebinding operands and bound descriptors in lowered EIR.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Static `Closure::bind()` extracts invalidated closure locals from boxed Mixed storage.
//! - `closure_bind` allocates a fresh descriptor, so its value must be an owned temporary the
//!   call site publishes in the unwind chain and retires on every supported target.

use std::path::Path;

use crate::codegen::platform::Target;
use crate::ir::{Immediate, LocalKind, Op, Ownership};
use crate::types::PhpType;

/// Static `Closure::bind()` passes an extracted descriptor when a global write invalidated the
/// closure local's compile-time identity.
#[test]
fn static_closure_bind_unboxes_invalidated_closure_storage_on_every_target() {
    let source = r#"<?php
function replaceGlobalClosure(): void {
    global $closure;
    $closure = function () { return $this->value; };
}
class Holder {
    public int $value = 7;
}
$closure = function () { return $this->value; };
try {
    replaceGlobalClosure();
} catch (Error $error) {}
$bound = Closure::bind($closure, new Holder());
echo $bound();
"#;
    for name in [
        "macos-aarch64",
        "ios-arm64",
        "ios-sim-arm64",
        "linux-aarch64",
        "linux-x86_64",
    ] {
        let module = super::lower_source_at_for_target(
            source,
            Path::new("main.php"),
            Path::new("."),
            Target::parse(name).unwrap(),
        );
        let main = module
            .functions
            .iter()
            .find(|function| function.flags.is_main)
            .unwrap();
        let extraction = main
            .instructions
            .iter()
            .find(|inst| inst.op == Op::MixedUnbox && inst.result_php_type == PhpType::Callable)
            .unwrap_or_else(|| panic!("{name}: invalidated closure storage was not extracted"));
        let extracted = extraction.result.expect("MixedUnbox produces a descriptor");
        assert_eq!(
            main.value(extracted).unwrap().ownership,
            Ownership::Owned,
            "{name}: the extracted descriptor owns its retained lease",
        );
        let bind = main
            .instructions
            .iter()
            .find(|inst| inst.op == Op::ClosureBind)
            .expect("the extracted descriptor is rebound");
        let mut bind_source = bind.operands[0];
        while let Some(producer) = main.instructions.iter().find(|inst| {
            inst.result == Some(bind_source) && matches!(inst.op, Op::Acquire | Op::Borrow)
        }) {
            bind_source = producer.operands[0];
        }
        assert_eq!(
            bind_source, extracted,
            "{name}: ClosureBind must consume the extracted descriptor, not its Mixed box",
        );
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
    }
}

/// A Closure value written by dynamic eval is rebound by Magician rather than interpreted as an
/// AOT closure descriptor by the native binder.
#[test]
fn dynamic_eval_closure_bind_uses_eval_static_dispatch_on_every_target() {
    let source = r#"<?php
class Vault {
    private string $code = "old";
    public string $label = "new";
}
$peek = function() { return $this->code; };
$source = '$peek = function() { return $this->label; };';
try { eval($source); } catch (Error $error) {}
$bound = Closure::bind($peek, new Vault(), Vault::class);
echo $bound();
"#;
    for name in [
        "macos-aarch64",
        "ios-arm64",
        "ios-sim-arm64",
        "linux-aarch64",
        "linux-x86_64",
    ] {
        let module = super::lower_source_at_for_target(
            source,
            Path::new("main.php"),
            Path::new("."),
            Target::parse(name).unwrap(),
        );
        let main = module
            .functions
            .iter()
            .find(|function| function.flags.is_main)
            .unwrap();
        let bind = main
            .instructions
            .iter()
            .find(|instruction| instruction.op == Op::EvalStaticMethodCall)
            .unwrap_or_else(|| panic!("{name}: dynamic Closure::bind must use eval dispatch"));
        let Some(Immediate::Data(target)) = bind.immediate else {
            panic!("{name}: eval static bind must name its target");
        };
        assert_eq!(
            module.data.strings[target.as_raw() as usize],
            "Closure::bind",
            "{name}: eval dispatch target",
        );
        assert!(
            main.instructions
                .iter()
                .all(|instruction| instruction.op != Op::ClosureBind),
            "{name}: the native binder must not consume a Magician Closure object",
        );
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
    }
}

/// `Closure::call()` roots its freshly bound descriptor and retires it after the invocation.
#[test]
fn closure_call_roots_and_retires_its_bound_descriptor_on_every_target() {
    let source = r#"<?php
class Holder {
    public int $value = 7;
}
function callBound($extra) {
    $closure = function ($bonus) { return $this->value + $bonus; };
    return $closure->call(new Holder(), $extra);
}
echo callBound(1);
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, Path::new("main.php"), Path::new("."), Target::parse(name).unwrap(),
        );
        let caller = module.functions.iter()
            .find(|function| function.name == "callBound").unwrap();
        let bind = caller.instructions.iter()
            .find(|inst| inst.op == Op::ClosureBind)
            .expect("the closure is rebound before the call");
        let bound = bind.result.expect("closure_bind produces the bound descriptor");
        assert_eq!(
            caller.value(bound).unwrap().ownership,
            Ownership::Owned,
            "{name}: a fresh bound descriptor is an owned temporary",
        );
        // The root stores either the bind result or its explicit acquire into a frame slot.
        let mut owners = vec![bound];
        owners.extend(caller.instructions.iter().filter_map(|inst| {
            (inst.op == Op::Acquire && inst.operands == [bound]).then_some(inst.result).flatten()
        }));
        let store = caller.instructions.iter()
            .find(|inst| inst.op == Op::StoreLocal
                && inst.operands.iter().all(|operand| owners.contains(operand)))
            .expect("the bound descriptor is stored into an operand-owner slot");
        let Some(Immediate::LocalSlot(slot)) = store.immediate else { unreachable!() };
        let local = &caller.locals[slot.as_raw() as usize];
        assert_eq!(local.php_type.codegen_repr(), PhpType::Callable, "{name}");
        assert_eq!(local.kind, LocalKind::HiddenTemp, "{name}");
        let at = |op: Op| caller.instructions.iter().position(|inst| {
            inst.op == op && inst.immediate == Some(Immediate::LocalSlot(slot))
        });
        let published = at(Op::PushCallOperandOwner)
            .unwrap_or_else(|| panic!("{name}: the bound descriptor needs an unwind owner"));
        let retired = at(Op::ReleaseLocalSlot)
            .unwrap_or_else(|| panic!("{name}: the bound descriptor is never released"));
        assert!(published < retired, "{name}: publication precedes retirement");
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
    }
}

/// A restored top-level closure keeps its runtime-shaped return when explicit scope is cloned.
#[test]
fn scoped_bind_after_try_keeps_mixed_property_return_on_every_target() {
    let source = r#"<?php
function expose_callable_global(): void { global $peek; }
class Vault {
    private string $code = "open";
    public string $label = "abc";
}
$indexed = [1];
$hash = ["key" => 1];
$peek = function() { return $this->code; };
try {
    $indexed[0] = 2;
    $hash["key"] = 2;
    unset($hash["key"]);
} catch (Error $error) {}
$bound = Closure::bind($peek, new Vault(), Vault::class);
echo $bound();
"#;
    for name in [
        "macos-aarch64",
        "ios-arm64",
        "ios-sim-arm64",
        "linux-aarch64",
        "linux-x86_64",
    ] {
        let module = super::lower_source_at_for_target(
            source,
            Path::new("main.php"),
            Path::new("."),
            Target::parse(name).unwrap(),
        );
        let scoped = module
            .closures
            .iter()
            .find(|function| function.lexical_class.as_deref() == Some("Vault"))
            .unwrap_or_else(|| panic!("{name}: explicit scope must clone the closure"));
        assert_eq!(
            scoped.return_php_type,
            PhpType::Mixed,
            "{name}: the cloned body must transport its runtime-shaped property value",
        );
        assert_eq!(
            scoped
                .params
                .iter()
                .find(|param| param.name == "this")
                .map(|param| param.php_type.clone()),
            Some(PhpType::Mixed),
            "{name}: the rebound receiver remains boxed for runtime property dispatch",
        );
    }
}

/// Deferred global writers and a method-nested rebound receiver stay runtime-shaped.
#[test]
fn deferred_global_closure_rebinding_stays_mixed_on_every_target() {
    let source = r#"<?php
class DeferredClosureMutator {
    public function __destruct() {
        global $peek;
        $peek = function() { return $this->label; };
    }
}
class Vault {
    private string $code = "old";
    public string $label = "new";
}
function suspended_writer(): Generator {
    global $peek;
    $peek = function() { return $this->label; };
    yield 1;
}
set_error_handler(function(int $level, string $message): bool {
    global $peek;
    $peek = function() { return $this->label; };
    return true;
});
$peek = function() { return $this->code; };
$victim = new DeferredClosureMutator();
$generator = suspended_writer();
$bound = Closure::bind($peek, new Vault(), Vault::class);
echo $bound();
"#;
    for name in [
        "macos-aarch64",
        "ios-arm64",
        "ios-sim-arm64",
        "linux-aarch64",
        "linux-x86_64",
    ] {
        let module = super::lower_source_at_for_target(
            source,
            Path::new("main.php"),
            Path::new("."),
            Target::parse(name).unwrap(),
        );
        let rebound = module
            .closures
            .iter()
            .find(|function| {
                function.lexical_class.as_deref() == Some("DeferredClosureMutator")
            })
            .unwrap_or_else(|| panic!("{name}: destructor closure was not lowered"));
        assert_eq!(
            rebound.return_php_type,
            PhpType::Mixed,
            "{name}: an absent enclosing-class property is selected by the rebound receiver",
        );
        assert_eq!(
            rebound
                .params
                .iter()
                .find(|param| param.name == "this")
                .map(|param| param.php_type.clone()),
            Some(PhpType::Mixed),
            "{name}: the rebound receiver must remain boxed",
        );
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
    }
}

/// A receiver loaded from global Mixed storage is detached before the runtime binder sees it.
#[test]
fn suspended_generator_bind_unboxes_global_receiver_on_every_target() {
    let source = r#"<?php
class Vault {
    private string $code = "old";
    public string $label = "new";
}
function suspended_bind(): Generator {
    global $peek, $vault;
    $peek = function() { return $this->code; };
    try { yield 1; } catch (Error $error) {}
    $bound = Closure::bind($peek, $vault, Vault::class);
    echo $bound();
}
$vault = new Vault();
$generator = suspended_bind();
$generator->current();
$peek = function() { return $this->label; };
$generator->next();
"#;
    for name in [
        "macos-aarch64",
        "ios-arm64",
        "ios-sim-arm64",
        "linux-aarch64",
        "linux-x86_64",
    ] {
        let module = super::lower_source_at_for_target(
            source,
            Path::new("main.php"),
            Path::new("."),
            Target::parse(name).unwrap(),
        );
        let generator = module
            .functions
            .iter()
            .find(|function| function.name == "suspended_bind")
            .unwrap_or_else(|| panic!("{name}: generator body was lowered"));
        let bind = generator
            .instructions
            .iter()
            .find(|instruction| instruction.op == Op::ClosureBind)
            .unwrap_or_else(|| panic!("{name}: runtime closure bind was emitted"));
        let mut receiver = bind.operands[1];
        while let Some(producer) = generator.instructions.iter().find(|instruction| {
            instruction.result == Some(receiver)
                && matches!(instruction.op, Op::Acquire | Op::Borrow)
        }) {
            receiver = producer.operands[0];
        }
        let extraction = generator
            .instructions
            .iter()
            .find(|instruction| instruction.result == Some(receiver))
            .unwrap_or_else(|| panic!("{name}: bound receiver has a producer"));
        assert_eq!(
            extraction.op,
            Op::MixedUnbox,
            "{name}: the binder receives the object payload, not its Mixed cell",
        );
        assert_eq!(
            extraction.result_php_type,
            PhpType::Object("Vault".to_string()),
            "{name}: explicit scope supplies the detached receiver representation",
        );
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
    }
}
