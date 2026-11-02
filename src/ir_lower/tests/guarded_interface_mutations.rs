//! Purpose:
//! Verify guarded property mutation receiver views on all supported targets.
//!
//! Called from:
//! - `crate::ir_lower::tests`.
//!
//! Key details:
//! - The receiver load changes its class view while its local keeps interface storage.

use crate::codegen::platform::Target;
use crate::ir::Op;
use crate::types::PhpType;
use std::path::Path;

/// Guarded named and runtime-name mutations emit valid assembly with unchanged local storage.
#[test]
fn guarded_interface_mutations_preserve_storage_on_all_targets() {
    let source = r#"<?php
interface Root {}
class Label implements Root { public int $value = 1; }
class MagicChild extends Label {
    public function __unset(string $name): void { throw new Error("stop"); }
}
class Holder {
    public Root $item;
    public function __construct(Root $item) { $this->item = $item; }
}
class StaticHolder { public static Root $item; }
function set_static_holder(Root $item): void { StaticHolder::$item = $item; }
class MagicLabel implements Root {
    public function __set(string $name, mixed $value): void { echo $name; }
    public function __unset(string $name): void { echo $name; }
}
class LockedLabel implements Root {
    public readonly int $value;
    public function __construct() { $this->value = 1; }
}
function write(Root $root): void {
    if ($root instanceof Label) { $root->value = 9; }
}
function clear(Root $root): void {
    if ($root instanceof Label) { unset($root->value); }
}
function clear_holder(Holder $holder): void {
    if ($holder->item instanceof Label) { unset($holder->item->value); }
}
function clear_static_holder(): void {
    if (StaticHolder::$item instanceof Label) { unset(StaticHolder::$item->value); }
}
function write_static_holder(mixed $value): void {
    if (StaticHolder::$item instanceof Label) { StaticHolder::$item->value = $value; }
}
function write_static_dynamic(string $name, mixed $value): void {
    if (StaticHolder::$item instanceof Label) { StaticHolder::$item->$name = $value; }
}
function clear_missing(Holder $holder): void {
    if ($holder->item instanceof Label) { unset($holder->item->missing); }
}
function dynamic(Root $root, string $name): void {
    if ($root instanceof Label) {
        $root->$name = 9;
        unset($root->$name);
    }
}
function magic(Root $root): void {
    if ($root instanceof MagicLabel) { $root->value = 9; unset($root->value); }
}
function locked(Root $root): void {
    if ($root instanceof LockedLabel) {
        $root->value ??= 9;
        try { echo ($root->value = 9); } catch (Error $error) { echo "locked"; }
    }
}
function locked_static(): void {
    if (StaticHolder::$item instanceof LockedLabel) { StaticHolder::$item->value = 9; }
}
$root = new Label();
write($root); clear($root); dynamic($root, "value");
clear_holder(new Holder(new Label()));
set_static_holder(new Label());
write_static_holder(9); write_static_dynamic("value", 9);
clear_static_holder();
clear_missing(new Holder(new Label()));
try { clear_missing(new Holder(new MagicChild())); } catch (Error $error) { echo "caught"; }
magic(new MagicLabel()); locked(new LockedLabel());
set_static_holder(new LockedLabel());
try { locked_static(); } catch (Error $error) { echo "locked"; }
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let mut module = super::lower_source_at_for_target(
            source, Path::new("main.php"), Path::new("."), Target::parse(target).unwrap(),
        );
        for (name, op) in [
            ("write", Op::PropSet), ("clear", Op::PropUnset),
            ("dynamic", Op::DynamicPropSet), ("dynamic", Op::DynamicPropUnset),
        ] {
            let function = module.functions.iter().find(|function| function.name == name).unwrap();
            let member = function.instructions.iter().find(|inst| inst.op == op).unwrap();
            let receiver = function.value(member.operands[0]).unwrap();
            assert_eq!(receiver.php_type, PhpType::Object("Label".into()), "{target}: {name}");
            let load = function.instructions.iter().find(|inst| inst.result == Some(member.operands[0])).unwrap();
            assert_eq!(load.op, Op::LoadLocal, "{target}: {name}: receiver producer");
            assert_eq!(load.result_php_type, receiver.php_type);
            assert_eq!(load.result_ownership, receiver.ownership);
            let slot = function.locals.iter().find(|slot| slot.name.as_deref() == Some("root")).unwrap();
            assert_eq!(slot.php_type, PhpType::Object("Root".into()), "{target}: {name}");
        }
        for (name, producer) in [
            ("clear_holder", Op::PropGet),
            ("clear_static_holder", Op::LoadStaticProperty),
        ] {
            let function = module.functions.iter().find(|function| function.name == name).unwrap();
            let unset = function.instructions.iter().find(|inst| inst.op == Op::PropUnset).unwrap();
            let receiver = function.value(unset.operands[0]).unwrap();
            assert_eq!(receiver.php_type, PhpType::Object("Label".into()), "{target}: {name}");
            let view = function.instructions.iter().find(|inst| inst.result == Some(unset.operands[0])).unwrap();
            assert_eq!(view.result_php_type, receiver.php_type);
            assert_eq!(view.result_ownership, receiver.ownership);
            let load = if view.op == Op::Acquire {
                assert_eq!(receiver.ownership, crate::ir::Ownership::Owned);
                let load = function.instructions.iter().find(|inst| inst.result == Some(view.operands[0])).unwrap();
                assert_eq!(load.result_php_type, PhpType::Object("Root".into()));
                load
            } else { view };
            assert_eq!(load.op, producer, "{target}: {name}: receiver producer");
        }
        for (name, op) in [
            ("write_static_holder", Op::PropSet),
            ("write_static_dynamic", Op::DynamicPropSet),
        ] {
            let function = module.functions.iter().find(|function| function.name == name).unwrap();
            let write = function.instructions.iter().find(|inst| inst.op == op).unwrap();
            let receiver = function.value(write.operands[0]).unwrap();
            assert_eq!(receiver.php_type, PhpType::Mixed, "{target}: {name}");
            let boxed = function.instructions.iter().find(|inst| inst.result == Some(write.operands[0])).unwrap();
            assert_eq!(boxed.op, Op::MixedBox, "{target}: {name}: delayed runtime view");
            assert_eq!(function.value(boxed.operands[0]).unwrap().php_type, PhpType::Object("Root".into()));
        }
        for optimized in [false, true] {
            if optimized { crate::ir_passes::optimize_module(&mut module); }
            crate::ir::validate_module(&module).expect("valid guarded mutation EIR");
            crate::codegen::generate_user_asm_from_ir(&module, false, false)
                .unwrap_or_else(|error| panic!("{target}, optimized={optimized}: {error:?}"));
        }
    }
}
