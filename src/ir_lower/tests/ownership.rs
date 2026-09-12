//! Purpose:
//! Ownership-operation tests for AST-to-EIR local assignment lowering.
//!
//! Called from:
//! - `crate::ir_lower::tests`.
//!
//! Key details:
//! - Verifies explicit acquire/release markers and exception-safe frame cleanup
//!   for refcounted locals across every supported native target.

use crate::ir::{print_module, Op, Ownership, ValueDef};

/// A boxed PHP array result cannot take ownership of an unrelated raw object argument.
#[test]
fn php_array_results_release_object_argument_temporaries_on_all_targets() {
    let source = r#"<?php
class ArrayResultProducer {
    public function values(): array { return [42]; }
}
function readProducedArray(ArrayResultProducer $producer): array { return $producer->values(); }
function makeProducedArray(): array { return readProducedArray(new ArrayResultProducer()); }
echo makeProducedArray()[0];
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        let function = module.functions.iter().find(|function| function.name == "makeProducedArray").unwrap();
        let call = function.instructions.iter().position(|inst| inst.op == Op::Call).unwrap();
        let argument = function.instructions[call].operands[0];
        assert!(function.instructions[call].result_php_type.is_php_array(), "{target}");
        let owner_slot = function.instructions[..call].iter().find_map(|inst| {
            if inst.op != Op::StoreLocal || inst.operands != [argument] {
                return None;
            }
            let Some(crate::ir::Immediate::LocalSlot(slot)) = inst.immediate else {
                return None;
            };
            function.instructions[..call].iter().any(|publish| {
                publish.op == Op::PushCallOperandOwner
                    && publish.immediate == Some(crate::ir::Immediate::LocalSlot(slot))
            }).then_some(slot)
        }).expect("object argument must be published in an unwind-visible owner slot");
        assert!(function.instructions[call + 1..].iter().any(|inst| {
            inst.op == Op::ReleaseLocalSlot
                && inst.immediate == Some(crate::ir::Immediate::LocalSlot(owner_slot))
        }), "{target}: {}", print_module(&module));
        assert!(!function.instructions[call + 1..].iter().any(|inst| {
            inst.op == Op::Release && inst.operands == [argument]
        }), "{target}: the published slot, not its call operand SSA, owns retirement");
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Mixed reference stores consume the explicit string acquire on every supported ABI.
#[test]
fn mixed_reference_stores_adopt_acquired_strings_on_all_targets() {
    let source = r#"<?php
function replace_mixed_reference(mixed &$value): void { $value = "replaced"; }
function exercise_mixed_reference(mixed $value): void {
    replace_mixed_reference($value);
    echo $value;
}
exercise_mixed_reference(null);
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        let function = module.functions.iter().find(|function| function.name == "replace_mixed_reference").unwrap();
        let store = function.instructions.iter().find(|inst| inst.op == Op::StoreRefCell).unwrap();
        let ValueDef::Instruction { inst, .. } = function.value(store.operands[0]).unwrap().def else {
            panic!("{target}: reference store must receive an acquired string");
        };
        assert_eq!(function.instruction(inst).unwrap().op, Op::Acquire, "{target}");
        let asm = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        assert!(asm.contains("transfer acquired ref-cell payload into Mixed storage"), "{target}");
    }
}

/// Static properties retain their own object owner before a caller binding is retyped or retired.
#[test]
fn static_property_stores_preserve_concrete_and_retyped_local_owners_on_all_targets() {
    let source = r#"<?php
        class StaticPublishedValue { public int $number = 17; }
        class StaticPublishedHolder { public static StaticPublishedValue $value; }
        function publish_concrete_owner(): void {
            $value = new StaticPublishedValue();
            StaticPublishedHolder::$value = $value;
        }
        function publish_widened_owner(): void {
            $value = new StaticPublishedValue();
            StaticPublishedHolder::$value = $value;
            $value = 42;
            echo $value;
        }
        publish_concrete_owner();
        publish_widened_owner();
    "#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        for (name, retyped) in [("publish_concrete_owner", false), ("publish_widened_owner", true)] {
            let function = module.functions.iter().find(|function| function.name == name).unwrap();
            let store = function.instructions.iter().find(|inst| inst.op == Op::StoreStaticProperty).unwrap();
            let ValueDef::Instruction { inst, .. } = function.value(store.operands[0]).unwrap().def else {
                panic!("{target}: static publication needs an acquired value");
            };
            let acquired = function.instruction(inst).unwrap();
            assert_eq!(acquired.op, Op::Acquire, "{target}: {name}");
            assert!(!function.instructions.iter().any(|inst| {
                inst.op == Op::Release && inst.operands == acquired.operands
            }), "{target}: {name} must not release the borrowed publication source");
            let ValueDef::Instruction { inst, .. } = function.value(acquired.operands[0]).unwrap().def else {
                panic!("{target}: publication must load the caller's object binding");
            };
            let source = function.instruction(inst).unwrap();
            let Some(crate::ir::Immediate::LocalSlot(slot)) = source.immediate else {
                panic!("{target}: publication source must identify its local slot");
            };
            assert_eq!(source.op, Op::LoadLocal, "{target}: {name}");
            assert!(matches!(function.locals[slot.as_raw() as usize].php_type,
                crate::types::PhpType::Object(_)), "{target}: keep the original object slot concrete");
            assert_eq!(function.instructions.iter().any(|inst| {
                inst.op == Op::ZeroLocalSlot
                    && inst.immediate == Some(crate::ir::Immediate::LocalSlot(slot))
            }), retyped, "{target}: a retyped binding retires its old owner slot");
        }
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Dynamic property stores retire concrete object temporaries after their boxes retain them.
#[test]
fn dynamic_property_stores_release_temporary_object_sources_on_all_targets() {
    let source = r#"<?php
        class DynamicStoredOwner { public function __destruct() { echo "released"; } }
        function store_dynamic_owner(stdClass $holder, string $name): void {
            $holder->named = new DynamicStoredOwner();
            $holder->{$name} = new DynamicStoredOwner();
        }
        store_dynamic_owner(new stdClass(), "runtime");
    "#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        let function = module.functions.iter().find(|function| function.name == "store_dynamic_owner").unwrap();
        for op in [Op::PropSet, Op::DynamicPropSet] {
            let index = function.instructions.iter().position(|inst| inst.op == op).unwrap();
            let source = *function.instructions[index].operands.last().unwrap();
            assert!(function.instructions[index + 1..].iter().any(|inst| {
                inst.op == Op::Release && inst.operands == [source]
            }), "{target}: {op:?} must retire the boxed object's original owner");
        }
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Static scalar stores release fresh boxes but do not acquire borrowed conversion sources on any ABI.
#[test]
fn static_scalar_conversions_preserve_source_box_ownership_on_all_targets() {
    let source = r#"<?php
        class StaticConversionOwner {
            public static int $integer = 0;
            public static bool $flag = false;
            public static float $fraction = 0.0;
            public static string $text = "";
        }
        function store_static_borrow(mixed $value): void {
            StaticConversionOwner::$integer = $value;
            StaticConversionOwner::$flag = $value;
            StaticConversionOwner::$fraction = $value;
            StaticConversionOwner::$text = $value;
        }
        function store_static_fresh(int $value): void {
            StaticConversionOwner::$integer = $value + 1;
        }
        store_static_borrow($argc);
        store_static_fresh($argc);
    "#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        let borrowed = module.functions.iter().find(|function| function.name == "store_static_borrow").unwrap();
        let stores = borrowed.instructions.iter().filter(|inst| inst.op == Op::StoreStaticProperty)
            .collect::<Vec<_>>();
        assert_eq!(stores.len(), 4, "{target}");
        for store in stores {
            let value = store.operands[0];
            let ValueDef::Instruction { inst, .. } = borrowed.value(value).unwrap().def else {
                panic!("{target}: the borrowed conversion source must be a local load");
            };
            assert_eq!(borrowed.instruction(inst).unwrap().op, Op::LoadLocal, "{target}");
            assert!(!borrowed.instructions.iter().any(|inst| {
                inst.op == Op::Release && inst.operands == [value]
            }), "{target}: a conversion must not consume its borrowed box");
        }
        let fresh = module.functions.iter().find(|function| function.name == "store_static_fresh").unwrap();
        let store = fresh.instructions.iter().position(|inst| inst.op == Op::StoreStaticProperty).unwrap();
        let value = fresh.instructions[store].operands[0];
        assert_eq!(fresh.value(value).unwrap().php_type.codegen_repr(), crate::types::PhpType::Mixed, "{target}");
        assert!(fresh.instructions[store + 1..].iter().any(|inst| {
            inst.op == Op::Release && inst.operands == [value]
        }), "{target}: a scalar slot does not own the checked arithmetic box");
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Unsetting a plain heap local uses atomic slot retirement instead of releasing a still-rooted load.
#[test]
fn unset_heap_local_retires_its_slot_before_destructor_escape() {
    let module = super::lower_source(r#"<?php
        class RetiredSlotValue {
            public function __destruct() { throw new RuntimeException("retired"); }
        }
        function retire_heap_slot(): void {
            $value = new RetiredSlotValue();
            unset($value);
        }
        try { retire_heap_slot(); } catch (RuntimeException $error) { echo "caught"; }
    "#);
    let function = module.functions.iter().find(|function| function.name == "retire_heap_slot").unwrap();
    let slot = function.locals.iter().find(|local| local.name.as_deref() == Some("value")).unwrap().id;
    let retirement = function.instructions.iter().find(|inst| {
        inst.op == Op::ReleaseLocalSlot && inst.immediate == Some(crate::ir::Immediate::LocalSlot(slot))
    }).expect("unset must clear the owning slot before invoking a throwing destructor");
    assert!(retirement.effects.contains(crate::ir::Effects::WRITES_LOCAL));
    assert!(retirement.effects.contains(crate::ir::Effects::MAY_THROW));
}

/// Every PHP catch snapshots the live activation before setjmp, including nested handlers.
#[test]
fn exception_handlers_preserve_their_surviving_activation_on_all_targets() {
    let source = r#"<?php
        function preserve_catch_frame(Exception $error): void {
            try {
                try { throw $error; } catch (Exception $inner) { echo "inner"; }
                throw $error;
            } catch (Exception $outer) { echo "outer"; }
        }
        preserve_catch_frame(new Exception("stop"));
    "#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(name).unwrap(),
        );
        let asm = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        let handlers = asm.split("push EIR exception handler").skip(1).collect::<Vec<_>>();
        assert!(handlers.len() >= 2, "{name}: the nested PHP catches must survive lowering");
        for handler in handlers {
            let setup = handler.split("setjmp").next().unwrap();
            assert!(setup.contains("_exc_call_frame_top"), "{name}: catch must preserve its activation");
        }
    }
}

/// Executable PHP frames publish callbacks that contain destructor throws on every supported ABI.
#[test]
fn executable_frames_publish_non_escaping_local_cleanup_on_all_targets() {
    let source = r#"<?php
        class UnwindOwner { public int $value = 7; }
        function abort_owned_frame(Exception $error): void {
            $value = new UnwindOwner();
            echo $value->value;
            throw $error;
        }
        try { abort_owned_frame(new Exception("stop")); } catch (Exception $error) {}
    "#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(name).unwrap(),
        );
        let asm = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        let callback = format!("{}__cdylib_exception_cleanup", crate::names::function_symbol("abort_owned_frame"));
        assert!(asm.matches(&callback).count() >= 2, "{name}: publish and define the executable frame callback");
        let body = asm.split(&format!("{callback}:")).nth(1).unwrap();
        let body = body.split("\n.globl ").next().unwrap();
        assert!(body.contains("__rt_cleanup_preserve_exception"), "{name}: destructor throws cannot skip later locals");
    }
}

/// Object aliases allocate a fallback cell whose epilogue uses bounded retirement on every ABI.
#[test]
fn promoted_object_local_uses_exception_safe_cell_retirement_on_all_targets() {
    let source = r#"<?php
        class CellOwnerValue { public int $value = 7; }
        function retire_cell_owner(): void {
            $value = new CellOwnerValue();
            $alias =& $value;
            echo $alias->value;
        }
        retire_cell_owner();
    "#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(name).unwrap(),
        );
        let function = module.functions.iter().find(|function| function.name == "retire_cell_owner").unwrap();
        assert!(function.locals.iter().any(|local| local.kind == crate::ir::LocalKind::RefCell), "{name}");
        let asm = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        assert!(asm.contains("__rt_local_ref_cell_release"), "{name}");
    }
}

/// Nested Mixed writes detach local/reference roots and borrow separated property cells on every target.
#[test]
fn nested_mixed_write_roots_preserve_storage_ownership_on_all_targets() {
    let source = r#"<?php
        class NestedWriteRoot { public mixed $tree = [[1]]; }
        function change_nested_root(mixed &$tree): void { $tree[0][0] = 2; }
        $object = new NestedWriteRoot();
        $copy = $object->tree;
        $copy[0][0] = 3;
        $object->tree[0][0] = 4;
        change_nested_root($copy);
        echo $copy[0][0], $object->tree[0][0];
    "#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        let function = module.functions.iter().find(|function| function.name == "change_nested_root").unwrap();
        let clone = function.instructions.iter().find(|inst| inst.op == Op::MixedClone)
            .expect("a by-reference root needs a detached value before nested mutation");
        assert!(function.instructions.iter().any(|inst| inst.op == Op::StoreRefCell), "{target}");
        assert_eq!(clone.result_php_type.codegen_repr(), crate::types::PhpType::Mixed, "{target}");
        let mut property_fetches = 0;
        for function in &module.functions {
            for inst in &function.instructions {
                if inst.op == Op::PropGetForWrite {
                    assert_eq!(inst.result_php_type.codegen_repr(), crate::types::PhpType::Mixed, "{target}");
                    assert_eq!(function.value(inst.result.unwrap()).unwrap().ownership, Ownership::Borrowed, "{target}");
                    property_fetches += 1;
                }
            }
        }
        assert_eq!(property_fetches, 1, "{target}");
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{target}: {error:?}"));
    }
}

/// Declared array defaults use the property's boxed contract before later hash-key writes.
#[test]
fn property_initializers_contextualize_array_defaults_on_all_targets() {
    let source = r#"<?php
        class HashDefaults {
            public array $empty = [];
            public array $seeded = [1];
            public function fill(): void {
                $this->empty["key"] = "value";
                $this->seeded["key"] = "value";
            }
        }
        $object = new HashDefaults();
        $object->fill();
        echo count($object->empty), count($object->seeded);
    "#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        let class = &module.class_infos["HashDefaults"];
        let init = module.functions.iter().find(|function| {
            function.name == format!("_class_propinit_{}", class.class_id)
        }).unwrap();
        for (index, (_, ty)) in class.properties.iter().enumerate() {
            assert!(ty.is_php_array(), "{target}: later key writes must not specialize the declaration");
            assert_eq!(ty.codegen_repr(), crate::types::PhpType::Mixed, "{target}");
            let store = init.instructions.iter().find(|inst| {
                inst.op == Op::PropSet && inst.immediate == Some(crate::ir::Immediate::PropertyRef {
                    class: class.class_id as u32, property: index as u32,
                })
            }).unwrap();
            assert_eq!(init.value(store.operands[1]).unwrap().php_type.codegen_repr(), ty.codegen_repr(), "{target}");
        }
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{target}: {error:?}"));
    }
}

/// By-name initializers preserve private shadow slots and default-less typed markers on every target.
#[test]
fn property_initializers_address_physical_slots_on_all_targets() {
    let source = r#"<?php
        class InitRoot { private int $value = 3; public int $pending; }
        class InitChild extends InitRoot { private int $value = 5; }
        class InitOnlyTyped { public string $pending; }
        $child = new InitChild();
        $onlyTyped = new InitOnlyTyped();
    "#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        for name in ["InitChild", "InitOnlyTyped"] {
            let class = &module.class_infos[name];
            let init = module.functions.iter().find(|function| {
                function.name == format!("_class_propinit_{}", class.class_id)
            }).expect("default-less typed classes also need an initializer");
            for (index, (property, _)) in class.properties.iter().enumerate() {
                let operation = if class.defaults.get(index).is_some_and(Option::is_some) {
                    Op::PropSet
                } else {
                    assert!(class.property_slot_is_declared(index, property));
                    Op::PropUnset
                };
                assert_eq!(init.instructions.iter().filter(|inst| {
                    inst.op == operation && inst.immediate == Some(crate::ir::Immediate::PropertyRef {
                        class: class.class_id as u32, property: index as u32,
                    })
                }).count(), 1, "{target}: {name}::{property} at slot {index}");
            }
        }
    }
}

/// Directory-only wrappers retain the raw runtime ABI on every target without changing ordinary methods.
#[test]
fn directory_wrapper_parameters_keep_the_runtime_abi_on_all_targets() {
    let source = r#"<?php
        class DirectoryOnly {
            public function dir_opendir($path, $options): bool { return true; }
            public function dir_readdir(): string { return ""; }
            public function identity($value) { return $value; }
        }
        stream_wrapper_register("directoryonly", "DirectoryOnly");
        $directory = opendir("directoryonly://root");
        echo (new DirectoryOnly())->identity("ok");
    "#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        let open = module.class_methods.iter().find(|method| method.name == "DirectoryOnly::dir_opendir").unwrap();
        assert!(!open.params.iter().any(|param| param.php_type.codegen_repr() == crate::types::PhpType::Mixed), "{target}");
        assert!(!open.instructions.iter().any(|inst| inst.op == Op::MixedClone), "{target}");
        let identity = module.class_methods.iter().find(|method| method.name == "DirectoryOnly::identity").unwrap();
        assert!(identity.instructions.iter().any(|inst| inst.op == Op::MixedClone), "{target}");
    }
}

/// Every target gives a by-value Mixed parameter an owned shadow while preserving ref parameters.
#[test]
fn mixed_parameters_own_detached_entry_cells_on_all_targets() {
    let source = "<?php
        function mixed_identity(mixed $value): mixed { return $value; }
        function mixed_reference(mixed &$value): mixed { return $value; }
        function mixed_store(mixed $input): mixed {
            $output = mixed_identity($input);
            return $output;
        }
        function mixed_reference_store(mixed $input): mixed {
            $output = mixed_reference($input);
            return $output;
        }
    ";
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        let identity = module.functions.iter().find(|function| function.name == "mixed_identity").unwrap();
        let clones = identity.instructions.iter().filter(|inst| inst.op == Op::MixedClone).collect::<Vec<_>>();
        assert_eq!(clones.len(), 1, "{target}");
        assert_eq!(identity.value(clones[0].result.unwrap()).unwrap().ownership, Ownership::Owned);
        let clone = clones[0].result.unwrap();
        assert_eq!(identity.instructions.iter().filter(|inst| {
            inst.op == Op::Release && inst.operands == [clone]
        }).count(), 1, "{target}: the shadow store must release its producer exactly once");
        assert!(identity.locals.iter().any(|local| local.name.as_deref() == Some("value#cow")), "{target}");
        let reference = module.functions.iter().find(|function| function.name == "mixed_reference").unwrap();
        let returned_clones = reference.instructions.iter().filter(|inst| inst.op == Op::MixedClone).collect::<Vec<_>>();
        assert_eq!(returned_clones.len(), 1, "{target}: by-value return must detach from the caller's reference");
        let clone = returned_clones[0];
        let ValueDef::Instruction { inst, .. } = reference.value(clone.operands[0]).unwrap().def else {
            panic!("{target}: expected a load from reference storage");
        };
        assert_eq!(reference.instruction(inst).unwrap().op, Op::LoadRefCell, "{target}");
        assert_eq!(reference.value(clone.result.unwrap()).unwrap().ownership, Ownership::Owned, "{target}");
        assert!(!reference.locals.iter().any(|local| local.name.as_deref() == Some("value#cow")), "{target}");
        for name in ["mixed_store", "mixed_reference_store"] {
            let store = module.functions.iter().find(|function| function.name == name).unwrap();
            let call = store.instructions.iter().find(|inst| inst.op == Op::Call).unwrap();
            let result = call.result.unwrap();
            assert_eq!(store.value(result).unwrap().ownership, Ownership::Owned, "{target}: {name}");
            assert_eq!(store.instructions.iter().filter(|inst| {
                inst.op == Op::Release && inst.operands == [result]
            }).count(), 1, "{target}: {name} must release the stored call result's producer");
        }
    }
}

/// Returns the printed EIR for `main`, excluding built-in helper and property-init functions.
fn main_function_text(text: &str) -> &str {
    let start = text.find("function main()").expect("expected lowered main function");
    let tail = &text[start..];
    match tail[1..].find("\n  function ") {
        Some(next_function) => &tail[..1 + next_function],
        None => tail,
    }
}

/// Returns the printed EIR slice for one named function.
fn named_function_text<'a>(text: &'a str, name: &str) -> &'a str {
    let needle = format!("function {name}(");
    let start = text.find(&needle).expect("expected named lowered function");
    let tail = &text[start..];
    match tail[1..].find("\n  function ") {
        Some(next_function) => &tail[..1 + next_function],
        None => tail,
    }
}

/// Verifies storing a freshly allocated array releases the temporary producer after the store.
#[test]
fn fresh_array_local_assignment_releases_source_after_store() {
    let module = super::lower_source("<?php $a = [1];");
    let text = print_module(&module);
    let main = main_function_text(&text);
    let store = main.find("store_local").expect("expected local store in lowered IR");
    let release = main.find("release").expect("expected release in lowered IR");
    assert!(main.contains("acquire"), "expected acquire in {text}");
    assert!(store < release, "expected release after store in {text}");
    assert_eq!(main.matches("release").count(), 1, "expected one release in {text}");
}

/// Verifies storing a freshly returned `array_column()` result releases the producer.
#[test]
fn array_column_assignment_releases_source_after_store() {
    let module = super::lower_source(
        r#"<?php
$users = [["name" => "Ada"], ["name" => "Linus"]];
$names = array_column($users, "name");
"#,
    );
    let function = module.functions.iter().find(|function| function.name == "main").unwrap();
    let call = function.instructions.iter().position(|inst| {
        inst.op == Op::RuntimeCall
            && inst.immediate == Some(crate::ir::Immediate::RuntimeCall(
                crate::ir::RuntimeCallTarget::Function(crate::ir::RuntimeFnId::ArrayColumn),
            ))
    }).expect("expected typed array_column runtime call in lowered IR");
    let result = function.instructions[call].result.unwrap();
    let acquired = function.instructions[call + 1..].iter().find(|inst| {
        inst.op == Op::Acquire && inst.operands == [result]
    }).expect("local assignment must acquire the array_column result").result.unwrap();
    let store = function.instructions.iter().position(|inst| {
        inst.op == Op::StoreLocal && inst.operands == [acquired]
    }).expect("expected local store of the acquired array_column result");
    let releases = function.instructions.iter().enumerate().filter(|(_, inst)| {
        inst.op == Op::Release && inst.operands == [result]
    }).map(|(index, _)| index).collect::<Vec<_>>();
    assert_eq!(releases.len(), 1, "the result producer must retire exactly once");
    assert!(store < releases[0], "expected result release after its local store");
}

/// Verifies nested array literals release refcounted row temporaries after insertion.
#[test]
fn nested_array_literal_releases_pushed_hash_temporary() {
    let module = super::lower_source(r#"<?php $users = [["name" => "Ada"]];"#);
    let text = print_module(&module);
    let push = text.find("array_push").expect("expected row append in lowered IR");
    let tail = &text[push..];
    let release = tail.find("release").expect("expected row release after append");
    assert!(release > 0, "expected release after array_push in {text}");
}

/// Boxed property mutation borrows the cell already separated and published by PropGetForWrite.
#[test]
fn property_array_push_borrows_the_published_write_cell() {
    let module = super::lower_source(
        r#"<?php
class C { public array $a; }
$x = new C();
$x->a = [];
$x->a[] = 1;
"#,
    );
    let mut writes = 0;
    for function in &module.functions {
        for (index, inst) in function.instructions.iter().enumerate() {
            if inst.op != Op::PropGetForWrite { continue; }
            writes += 1;
            let value = inst.result.unwrap();
            assert_eq!(function.value(value).unwrap().ownership, Ownership::Borrowed);
            assert!(inst.result_php_type.is_php_array());
            assert!(function.instructions[index + 1..].iter().any(|next| {
                next.op == Op::MixedArrayAppend && next.operands[0] == value
            }), "append must mutate the published property cell");
            assert!(!function.instructions.iter().any(|next| {
                next.op == Op::Release && next.operands == [value]
            }), "a borrowed property cell must not be retired by the append");
        }
    }
    assert_eq!(writes, 1);
}

/// Verifies overwriting a refcounted array local releases the previous value.
#[test]
fn overwriting_array_local_emits_release() {
    let module = super::lower_source("<?php $a = [1]; $a = [2];");
    let text = print_module(&module);
    let main = main_function_text(&text);
    assert!(main.contains("acquire"), "expected acquire in {text}");
    assert!(main.contains("release"), "expected release in {text}");
    assert_eq!(main.matches("array_new").count(), 2, "expected two arrays in {text}");
}

/// Verifies string locals participate in explicit ownership operations.
#[test]
fn overwriting_string_local_emits_release() {
    let module = super::lower_source(r#"<?php $s = "a"; $s = "b";"#);
    let text = print_module(&module);
    assert!(text.contains("acquire"), "expected acquire in {text}");
    assert!(text.contains("release"), "expected release in {text}");
}

/// Verifies a borrowed string result is retained before its aliased source slot is released.
#[test]
fn self_reassignment_acquires_borrowed_string_before_releasing_slot() {
    let module = super::lower_source(
        r#"<?php
function normalize(string $value): string {
    $value = trim($value);
    return $value;
}
echo normalize("  hi  ");
"#,
    );
    let text = print_module(&module);
    let function = named_function_text(&text, "normalize");
    let builtin = function
        .find("runtime.trim")
        .expect("expected typed trim runtime call");
    let assignment = &function[builtin..];
    let acquire = assignment.find("acquire").expect("expected retained trim result");
    let release = assignment
        .find("release")
        .expect("expected previous slot release");
    let store = assignment
        .find("store_local")
        .expect("expected replacement local store");
    assert!(
        acquire < release && release < store,
        "expected acquire before old-slot release and store in {function}"
    );
}

/// Verifies appends into mixed function parameters use an explicit append opcode.
#[test]
fn mixed_parameter_array_push_uses_explicit_opcode() {
    let module = super::lower_source(
        r#"<?php
function add($arr, $value) {
    $arr[] = $value;
    return $arr;
}
"#,
    );
    let text = print_module(&module);
    assert!(
        text.contains("mixed_array_append"),
        "expected mixed_array_append for mixed parameter array push in {text}"
    );
}

/// Stringifying a Mixed local read must not release its slot-backed source.
#[test]
fn mixed_string_cast_does_not_release_local_source() {
    let module = super::lower_source(
        r#"<?php
function render_mixed(mixed $value): string {
    $first = (string) $value;
    return $first . "|" . (string) $value;
}
echo render_mixed(str_repeat("alive", 1));
"#,
    );
    let function = module
        .functions
        .iter()
        .find(|function| function.name == "render_mixed")
        .expect("expected render_mixed EIR function");
    let cast_sources = function
        .instructions
        .iter()
        .filter(|inst| inst.op == Op::Cast)
        .filter_map(|inst| inst.operands.first().copied())
        .collect::<Vec<_>>();
    assert_eq!(cast_sources.len(), 2, "expected two Mixed string casts");
    for source in cast_sources {
        assert!(
            function
                .instructions
                .iter()
                .all(|inst| inst.op != Op::Release || inst.operands.first().copied() != Some(source)),
            "a Mixed local read must survive stringification"
        );
    }
}

/// Stringifying an owned Mixed container read must release that exact source value.
#[test]
fn mixed_string_cast_releases_owned_container_read() {
    let module = super::lower_source(
        r#"<?php
$values = ["s" => str_repeat("x", 1), "n" => 1];
echo (string) $values["s"];
"#,
    );
    let function = module
        .functions
        .iter()
        .find(|function| function.name == "main")
        .expect("expected main EIR function");
    let source = function
        .instructions
        .iter()
        .filter(|inst| inst.op == Op::Cast)
        .filter_map(|inst| inst.operands.first().copied())
        .find(|source| {
            let Some(value) = function.value(*source) else {
                return false;
            };
            let ValueDef::Instruction { inst, .. } = value.def else {
                return false;
            };
            function
                .instruction(inst)
                .is_some_and(|inst| matches!(inst.op, Op::ArrayGet | Op::HashGet))
        })
        .expect("expected a Mixed string cast sourced from a container read");
    assert!(
        function
            .instructions
            .iter()
            .any(|inst| inst.op == Op::Release && inst.operands.first().copied() == Some(source)),
        "the owned Mixed container read must be released after stringification"
    );
}

/// A by-value Mixed identity call returns an owned detached cell without consuming its borrowed input.
#[test]
fn mixed_identity_call_result_is_owned_independently_of_borrowed_input() {
    let module = super::lower_source(
        r#"<?php
function identity(mixed $value): mixed { return $value; }
$values = [1];
$value = array_pop($values);
echo identity($value);
echo $value;
"#,
    );
    let function = module
        .functions
        .iter()
        .find(|function| function.name == "main")
        .expect("expected main EIR function");
    let call = function
        .instructions
        .iter()
        .find(|inst| inst.op == Op::Call)
        .expect("expected the identity user call");
    let result = call.result.expect("expected the identity user-call result");
    assert_eq!(
        function
            .instructions
            .iter()
            .filter(|inst| inst.op == Op::Release && inst.operands == [result])
            .count(),
        1,
        "the detached result must be released exactly once after echo"
    );
    assert_eq!(
        function.value(result).expect("call result metadata").ownership,
        Ownership::Owned,
        "the callee clones its by-value Mixed input before returning it"
    );
    let argument = call.operands[0];
    assert!(function.instructions.iter().all(|inst| {
        inst.op != Op::Release || inst.operands != [argument]
    }), "the caller's borrowed argument must stay alive for the following echo");
}

/// Verifies fresh boxed producers publish `Owned` instead of requiring codegen inference.
#[test]
fn fresh_boxed_producers_publish_owned_eir_metadata() {
    let module = super::lower_source(
        r#"<?php
function checked_add(int $value): mixed { return $value + 1; }
function boxed_scalar(int $value): mixed { return $value; }
function scratch_string(int $value): string { return "v" . $value; }
echo checked_add(1);
echo boxed_scalar(2);
echo scratch_string(3);
"#,
    );

    let mut observed = Vec::new();
    for function in &module.functions {
        for inst in &function.instructions {
            if !matches!(inst.op, Op::ICheckedAdd | Op::MixedBox) {
                continue;
            }
            let result = inst.result.expect("owning producer must have a result");
            let ownership = function
                .value(result)
                .expect("owning producer result metadata")
                .ownership;
            observed.push((inst.op, ownership));
            assert_eq!(
                ownership,
                Ownership::Owned,
                "{} must publish owned EIR metadata",
                inst.op.name()
            );
            assert_eq!(
                inst.result_ownership,
                Ownership::Owned,
                "{} instruction metadata must match its result value",
                inst.op.name()
            );
        }
    }

    assert!(
        observed.iter().any(|(op, _)| *op == Op::ICheckedAdd),
        "expected a checked-add producer"
    );
    assert!(
        observed.iter().any(|(op, _)| *op == Op::MixedBox),
        "expected a MixedBox producer"
    );

    let scratch_function = module
        .functions
        .iter()
        .find(|function| function.name == "scratch_string")
        .expect("expected scratch_string EIR function");
    let scratch_result = scratch_function
        .instructions
        .iter()
        .find(|inst| inst.op == Op::StrConcat)
        .and_then(|inst| inst.result)
        .expect("expected a scratch string concat result");
    assert_ne!(
        scratch_function
            .value(scratch_result)
            .expect("scratch string metadata")
            .ownership,
        Ownership::Owned,
        "concat scratch storage must retain its string-specific ownership contract"
    );
}

/// A fresh Mixed argument and the callee's detached return each require their own release.
#[test]
fn owned_mixed_argument_and_detached_return_are_released_independently() {
    let module = super::lower_source(
        r#"<?php
function idv(mixed $value): mixed { return $value; }
function run(int $i): void {
    $r = idv($i + 1);
    echo $r;
}
run(5);
"#,
    );
    let function = module
        .functions
        .iter()
        .find(|function| function.name == "run")
        .expect("expected the run EIR function");
    let call_index = function
        .instructions
        .iter()
        .position(|inst| inst.op == Op::Call)
        .expect("expected the idv call");
    let call = &function.instructions[call_index];
    let argument = call.operands[0];
    let root = function.instructions[..call_index].iter().find(|inst| {
        inst.op == Op::StoreLocal && inst.operands == [argument]
    }).expect("the independent argument owner must be rooted before invocation");
    assert_eq!(function.instructions[..call_index].iter().filter(|inst| {
        inst.op == Op::PushCallOperandOwner && inst.immediate == root.immediate
    }).count(), 1, "the argument has exactly one unwind root");
    for op in [Op::PopCallOperandOwner, Op::ReleaseLocalSlot] {
        assert_eq!(function.instructions[call_index + 1..].iter().filter(|inst| {
            inst.op == op && inst.immediate == root.immediate
        }).count(), 1, "the rooted argument retires exactly once: {op:?}");
    }
    assert!(!function.instructions.iter().any(|inst| {
        inst.op == Op::Release && inst.operands == [argument]
    }), "the argument's transferred root owner must not also be released as SSA");
    let result = call.result.expect("expected the detached result");
    assert_ne!(argument, result);
    assert_eq!(function.instructions[call_index + 1..].iter().filter(|inst| {
        inst.op == Op::Release && inst.operands == [result]
    }).count(), 1, "the detached result still releases its independent SSA owner");
}
