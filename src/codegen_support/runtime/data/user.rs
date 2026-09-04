//! Purpose:
//! Builds user-program runtime metadata as assembly text.
//! This owns class, interface, vtable, enum, static-property, and source-location tables generated from analysis.
//!
//! Called from:
//! - `crate::codegen_support::runtime::data::emit_runtime_data_user()`.
//!
//! Key details:
//! - User data is program-specific and must match class ids, static property slots, and generated call sites.

use std::collections::{HashMap, HashSet};

use crate::codegen_support::data_section::comm_directive;
use crate::codegen_support::platform::Target;
use crate::names::{
    enum_case_symbol, function_variant_active_symbol, interface_method_wrapper_symbol, mangle_fqn,
    method_symbol, php_symbol_key, static_method_symbol, static_property_symbol,
};
use crate::parser::ast::Visibility;
use crate::types::{ClassInfo, EnumInfo, FunctionSig, InterfaceInfo, PhpType};

use super::instanceof::{escaped_ascii, escaped_bytes};

const EVAL_REFLECTION_CLASS_FLAG_FINAL: u64 = 1;
const EVAL_REFLECTION_CLASS_FLAG_ABSTRACT: u64 = 2;
const EVAL_REFLECTION_CLASS_FLAG_READONLY: u64 = 32;
const EVAL_REFLECTION_CLASS_SOURCE_LINE_MASK: u64 = 0x00ff_ffff;
const EVAL_REFLECTION_CLASS_SOURCE_START_SHIFT: u64 = 16;
const EVAL_REFLECTION_CLASS_SOURCE_END_SHIFT: u64 = 40;
const EVAL_REFLECTION_PROPERTY_FLAG_STATIC: u64 = 1;
const EVAL_REFLECTION_PROPERTY_FLAG_PUBLIC: u64 = 2;
const EVAL_REFLECTION_PROPERTY_FLAG_PROTECTED: u64 = 4;
const EVAL_REFLECTION_PROPERTY_FLAG_PRIVATE: u64 = 8;
const EVAL_REFLECTION_PROPERTY_FLAG_FINAL: u64 = 16;
const EVAL_REFLECTION_PROPERTY_FLAG_ABSTRACT: u64 = 32;
const EVAL_REFLECTION_PROPERTY_FLAG_READONLY: u64 = 64;
const EVAL_REFLECTION_PROPERTY_FLAG_HAS_DEFAULT_VALUE: u64 = 256;
const EVAL_REFLECTION_PROPERTY_FLAG_PROMOTED: u64 = 512;
const EVAL_REFLECTION_PROPERTY_FLAG_PROTECTED_SET: u64 = 2048;
const EVAL_REFLECTION_PROPERTY_FLAG_PRIVATE_SET: u64 = 4096;
const EVAL_REFLECTION_METHOD_FLAG_STATIC: u64 = 1;
const EVAL_REFLECTION_METHOD_FLAG_PUBLIC: u64 = 2;
const EVAL_REFLECTION_METHOD_FLAG_PROTECTED: u64 = 4;
const EVAL_REFLECTION_METHOD_FLAG_PRIVATE: u64 = 8;
const EVAL_REFLECTION_METHOD_FLAG_FINAL: u64 = 16;
const EVAL_REFLECTION_METHOD_FLAG_ABSTRACT: u64 = 32;
const EVAL_REFLECTION_METHOD_SOURCE_LINE_MASK: u64 = 0x00ff_ffff;
const EVAL_REFLECTION_METHOD_SOURCE_START_SHIFT: u64 = 16;
const EVAL_REFLECTION_METHOD_SOURCE_END_SHIFT: u64 = 40;

/// Emit the user-dependent data section — globals, statics, class metadata.
/// This changes per program and cannot be cached.
pub(crate) fn emit_runtime_data_user(
    global_var_names: &HashSet<String>,
    static_vars: &HashMap<(String, String), PhpType>,
    functions: &HashMap<String, FunctionSig>,
    function_variant_groups: &HashSet<String>,
    interfaces: &HashMap<String, InterfaceInfo>,
    interface_names: &[String],
    trait_names: &[String],
    declared_trait_uses: &HashMap<String, Vec<String>>,
    declared_trait_source_lines: &HashMap<String, u32>,
    classes: &HashMap<String, ClassInfo>,
    enums: &HashMap<String, EnumInfo>,
    allowed_class_names: Option<&HashSet<String>>,
    emit_eval_reflection_metadata: bool,
    source_path: Option<&str>,
    has_user_frames: bool,
    target: Target,
) -> String {
    let mut out = String::new();

    let mut sorted_globals: Vec<&String> = global_var_names.iter().collect();
    sorted_globals.sort();
    for name in sorted_globals {
        out.push_str(&comm_directive(&format!("_gvar_{}", name), 16, target));
    }

    let mut sorted_statics: Vec<&(String, String)> = static_vars.keys().collect();
    sorted_statics.sort();
    for (func_name, var_name) in sorted_statics {
        out.push_str(&comm_directive(
            &format!("_static_{}_{}", mangle_fqn(func_name), var_name),
            16,
            target,
        ));
        out.push_str(&comm_directive(
            &format!("_static_{}_{}_init", mangle_fqn(func_name), var_name),
            8,
            target,
        ));
    }

    let mut static_property_symbols = HashSet::new();
    for (class_name, class_info) in classes {
        if allowed_class_names.is_some_and(|allowed| !allowed.contains(class_name)) {
            continue;
        }
        for (property_name, _) in &class_info.static_properties {
            let declaring_class = class_info
                .static_property_declaring_classes
                .get(property_name)
                .map(String::as_str)
                .unwrap_or(class_name.as_str());
            static_property_symbols.insert(static_property_symbol(declaring_class, property_name));
        }
    }
    let mut static_property_symbols: Vec<String> = static_property_symbols.into_iter().collect();
    static_property_symbols.sort();
    for symbol in static_property_symbols {
        out.push_str(&comm_directive(&symbol, 16, target));
    }

    let mut sorted_enum_names: Vec<&String> = enums.keys().collect();
    sorted_enum_names.sort();
    for enum_name in &sorted_enum_names {
        let Some(enum_info) = enums.get(*enum_name) else {
            continue;
        };
        for case in &enum_info.cases {
            out.push_str(&comm_directive(
                &enum_case_symbol(*enum_name, &case.name),
                8,
                target,
            ));
        }
    }

    let mut sorted_interfaces: Vec<(&String, &InterfaceInfo)> = interfaces.iter().collect();
    sorted_interfaces.sort_by_key(|(_, interface_info)| interface_info.interface_id);
    let all_class_id_by_name: HashMap<String, u64> = classes
        .iter()
        .map(|(name, class_info)| (name.clone(), class_info.class_id))
        .collect();
    let mut sorted_classes: Vec<(&String, &ClassInfo)> = classes.iter().collect();
    if let Some(allowed_class_names) = allowed_class_names {
        sorted_classes.retain(|(class_name, _)| allowed_class_names.contains(*class_name));
    }
    sorted_classes.sort_by_key(|(_, class_info)| class_info.class_id);
    let class_id_by_name: HashMap<String, u64> = sorted_classes
        .iter()
        .map(|(name, class_info)| ((*name).clone(), class_info.class_id))
        .collect();
    let class_info_by_id: HashMap<u64, &ClassInfo> = sorted_classes
        .iter()
        .map(|(_, class_info)| (class_info.class_id, *class_info))
        .collect();
    let class_name_by_id: HashMap<u64, &String> = sorted_classes
        .iter()
        .map(|(name, class_info)| (class_info.class_id, *name))
        .collect();
    let max_class_id = sorted_classes.iter().map(|(_, class_info)| class_info.class_id).max();

    out.push_str(".data\n");
    out.push_str(".p2align 3\n");
    emit_callable_function_data(&mut out, functions, function_variant_groups);
    out.push_str(".p2align 3\n");
    super::instanceof::emit_instanceof_target_lookup_data(&mut out, &sorted_interfaces, &sorted_classes);
    emit_class_name_lookup_data(&mut out, max_class_id, &class_name_by_id);
    emit_name_lookup_data(
        &mut out,
        "_interface_names_count",
        "_interface_names",
        "_interface_name",
        interface_names,
    );
    emit_name_lookup_data(
        &mut out,
        "_trait_names_count",
        "_trait_names",
        "_trait_name",
        trait_names,
    );
    let enum_names: Vec<String> = sorted_enum_names.iter().map(|name| (*name).clone()).collect();
    emit_name_lookup_data(
        &mut out,
        "_enum_names_count",
        "_enum_names",
        "_enum_name",
        &enum_names,
    );

    // Per-program class id of the built-in `Fiber` class. The fiber runtime
    // checks this against the receiver's class_id in __rt_object_free_deep so
    // that a Fiber being garbage-collected releases its 256 KB stack instead
    // of leaking it. Defaults to u64::MAX when Fiber is not in scope (which
    // never happens in practice — Fiber is always injected as a built-in).
    let fiber_class_id = all_class_id_by_name
        .get("Fiber")
        .copied()
        .unwrap_or(u64::MAX);
    out.push_str(".globl _fiber_class_id\n_fiber_class_id:\n");
    out.push_str(&format!("    .quad {}\n", fiber_class_id));

    let fiber_error_class_id = all_class_id_by_name
        .get("FiberError")
        .copied()
        .unwrap_or(u64::MAX);
    out.push_str(".globl _fiber_error_class_id\n_fiber_error_class_id:\n");
    out.push_str(&format!("    .quad {}\n", fiber_error_class_id));

    let generator_class_id = all_class_id_by_name
        .get("Generator")
        .copied()
        .unwrap_or(u64::MAX);
    out.push_str(".globl _generator_class_id\n_generator_class_id:\n");
    out.push_str(&format!("    .quad {}\n", generator_class_id));

    for (symbol, class_name) in [
        ("_spl_dll_class_id", "SplDoublyLinkedList"),
        ("_spl_stack_class_id", "SplStack"),
        ("_spl_queue_class_id", "SplQueue"),
        ("_spl_fixed_array_class_id", "SplFixedArray"),
        ("_spl_error_class_id", "Error"),
        ("_spl_logic_exception_class_id", "LogicException"),
        ("_spl_runtime_exception_class_id", "RuntimeException"),
        ("_spl_out_of_range_exception_class_id", "OutOfRangeException"),
        ("_spl_out_of_bounds_exception_class_id", "OutOfBoundsException"),
        ("_spl_invalid_argument_exception_class_id", "InvalidArgumentException"),
        ("_spl_type_error_class_id", "TypeError"),
        ("_spl_value_error_class_id", "ValueError"),
        ("_spl_arithmetic_error_class_id", "ArithmeticError"),
        // Emitted for the `intdiv($a, 0)` / `$a % 0` zero-divisor guards, which
        // raise reference PHP's catchable DivisionByZeroError from codegen with
        // no EIR class reference to hang the id off.
        ("_spl_division_by_zero_error_class_id", "DivisionByZeroError"),
        // Emitted for the `new $c(...)` arity refusals. The checker rejects a
        // static `new C()` that passes too few arguments, but `new $c()` names
        // its class in a VALUE, so the refusal has to be raised at run time and
        // has no EIR class reference to hang the id off either.
        ("_spl_argument_count_error_class_id", "ArgumentCountError"),
    ] {
        let class_id = all_class_id_by_name
            .get(class_name)
            .copied()
            .unwrap_or(u64::MAX);
        out.push_str(&format!(".globl {}\n{}:\n", symbol, symbol));
        out.push_str(&format!("    .quad {}\n", class_id));
    }

    out.push_str(".globl _interface_count\n_interface_count:\n");
    out.push_str(&format!("    .quad {}\n", sorted_interfaces.len()));
    out.push_str(".globl _interface_method_ptrs\n_interface_method_ptrs:\n");
    for (_, interface_info) in &sorted_interfaces {
        out.push_str(&format!(
            "    .quad _interface_methods_{}\n",
            interface_info.interface_id
        ));
    }

    out.push_str(".globl _class_interface_ptrs\n_class_interface_ptrs:\n");
    if let Some(max_class_id) = max_class_id {
        for class_id in 0..=max_class_id {
            if class_info_by_id.contains_key(&class_id) {
                out.push_str(&format!("    .quad _class_interfaces_{}\n", class_id));
            } else {
                out.push_str("    .quad _class_interfaces_missing\n");
            }
        }
    }

    // Per-class JSON descriptor pointer table — used by __rt_json_encode_object
    // to walk public properties and dispatch JsonSerializable when present.
    out.push_str(".globl _class_json_desc_ptrs\n_class_json_desc_ptrs:\n");
    if let Some(max_class_id) = max_class_id {
        for class_id in 0..=max_class_id {
            if class_info_by_id.contains_key(&class_id) {
                out.push_str(&format!("    .quad _class_json_desc_{}\n", class_id));
            } else {
                out.push_str("    .quad _class_json_desc_missing\n");
            }
        }
    }

    // Per-class var_dump descriptor pointer table — used by
    // `__rt_var_dump_object` to walk EVERY declared property, not just the
    // public subset the JSON descriptor carries, and with PHP's rendered
    // `["p":protected]` key text instead of serialize's NUL-mangled key.
    out.push_str(".globl _class_vd_desc_ptrs\n_class_vd_desc_ptrs:\n");
    if let Some(max_class_id) = max_class_id {
        for class_id in 0..=max_class_id {
            if class_info_by_id.contains_key(&class_id) {
                out.push_str(&format!("    .quad _class_vd_desc_{}\n", class_id));
            } else {
                out.push_str("    .quad _class_vd_desc_missing\n");
            }
        }
    }

    // Per-class print_r / var_export descriptor pointer table — read by
    // `__rt_print_r_object` and by the `__elephc_object_prop_*` prelude helpers.
    // Same rows as `_class_vd_desc_ptrs`, different key spellings.
    out.push_str(".globl _class_prop_desc_ptrs\n_class_prop_desc_ptrs:\n");
    if let Some(max_class_id) = max_class_id {
        for class_id in 0..=max_class_id {
            if class_info_by_id.contains_key(&class_id) {
                out.push_str(&format!("    .quad _class_prop_desc_{}\n", class_id));
            } else {
                out.push_str("    .quad _class_prop_desc_missing\n");
            }
        }
    }

    // Per-class enum tables. `_class_enum_kinds` is 0 for an ordinary class and
    // 1/2/3 for a pure / int-backed / string-backed enum — `print_r` prints
    // `E Enum`, `E Enum:int` and `E Enum:string` respectively, and `var_dump` /
    // `var_export` only need the non-zero test. `_class_enum_name_offsets` is the
    // byte offset of the enum's `name` property slot inside the instance (`-1`
    // for a non-enum), which is where the case name every renderer prints lives.
    // Both are indexed by the same class id as every other per-class table, so an
    // enum instance is recognized from its object header alone.
    out.push_str(".globl _class_enum_kinds\n_class_enum_kinds:\n");
    if let Some(max_class_id) = max_class_id {
        for class_id in 0..=max_class_id {
            let kind = class_name_by_id
                .get(&class_id)
                .and_then(|class_name| enums.get(*class_name))
                .map(|enum_info| match &enum_info.backing_type {
                    Some(PhpType::Int) => 2u64,
                    Some(PhpType::Str) => 3u64,
                    _ => 1u64,
                })
                .unwrap_or(0);
            out.push_str(&format!("    .quad {}\n", kind));
        }
    }

    out.push_str(".globl _class_enum_name_offsets\n_class_enum_name_offsets:\n");
    if let Some(max_class_id) = max_class_id {
        for class_id in 0..=max_class_id {
            let offset = match (
                class_name_by_id.get(&class_id),
                class_info_by_id.get(&class_id),
            ) {
                (Some(class_name), Some(class_info)) if enums.contains_key(*class_name) => {
                    enum_case_name_property_offset(class_info)
                }
                _ => -1,
            };
            out.push_str(&format!("    .quad {}\n", offset));
        }
    }

    // JsonException's class_id is consulted by __rt_json_throw_error when
    // JSON_THROW_ON_ERROR is set — it allocates an instance of this class
    // and routes it through the normal exception machinery.
    let json_exception_class_id = classes
        .get("JsonException")
        .map(|info| info.class_id as i64)
        .unwrap_or(-1);
    out.push_str(&format!(
        ".globl _json_exception_class_id\n_json_exception_class_id:\n    .quad {}\n",
        json_exception_class_id,
    ));

    // TWO SENTINELS, BECAUSE THEY MEAN OPPOSITE THINGS. `__rt_exception_matches` walks this table
    // from the thrown class upwards, and a slot that is not a parent id ends the walk. Until this
    // distinction existed, one value ended it three ways: a class with no parent (a genuine root,
    // where stopping is the right answer), a hole in the id space, and a class whose PARENT was
    // never emitted. The last two are broken chains, and collapsing them onto the first is how a
    // `catch (Exception $e)` silently fails to match a thrown JsonException — the walk reaches
    // for an ancestor that is not there and reports, in good faith, no match.
    //
    // `-1` still means "root, stop and report no match". `-2` means "the metadata this walk needs
    // was never emitted", and the helper aborts on it. Nothing should ever produce a `-2` walk
    // today: `crate::codegen_support::emitted_classes` seeds the whole throwable hierarchy for
    // exactly this reason. The sentinel is what lets a future gate there fail loudly instead of
    // quietly, which is the precondition for narrowing that seeding at all.
    const CLASS_PARENT_ROOT: i64 = -1;
    const CLASS_PARENT_ABSENT: i64 = -2;
    out.push_str(".globl _class_parent_ids\n_class_parent_ids:\n");
    if let Some(max_class_id) = max_class_id {
        for class_id in 0..=max_class_id {
            let parent_id = match class_info_by_id.get(&class_id) {
                // A hole in the id space: nothing was emitted under this id at all.
                None => CLASS_PARENT_ABSENT.to_string(),
                Some(class_info) => match class_info.parent.as_ref() {
                    None => CLASS_PARENT_ROOT.to_string(),
                    Some(parent_name) => match class_id_by_name.get(parent_name) {
                        Some(id) => id.to_string(),
                        // The chain is broken: this class has a parent, but it was not emitted.
                        None => CLASS_PARENT_ABSENT.to_string(),
                    },
                },
            };
            out.push_str(&format!("    .quad {}\n", parent_id));
        }
    }

    out.push_str(".globl _class_object_payload_sizes\n_class_object_payload_sizes:\n");
    if let Some(max_class_id) = max_class_id {
        for class_id in 0..=max_class_id {
            let payload_size =
                match (class_info_by_id.get(&class_id), class_name_by_id.get(&class_id)) {
                    (Some(class_info), Some(class_name)) => {
                        class_object_payload_size(class_name, class_info)
                    }
                    _ => 0,
                };
            out.push_str(&format!("    .quad {}\n", payload_size));
        }
    }

    out.push_str(".globl _class_object_dynamic_prop_flags\n_class_object_dynamic_prop_flags:\n");
    if let Some(max_class_id) = max_class_id {
        for class_id in 0..=max_class_id {
            let flag = match (class_info_by_id.get(&class_id), class_name_by_id.get(&class_id)) {
                (Some(class_info), Some(class_name)) => {
                    u8::from(class_uses_dynamic_property_tail(class_name, class_info))
                }
                _ => 0,
            };
            out.push_str(&format!("    .quad {}\n", flag));
        }
    }

    out.push_str(".globl _class_gc_desc_count\n_class_gc_desc_count:\n");
    out.push_str(&format!(
        "    .quad {}\n",
        max_class_id.map_or(0, |class_id| class_id + 1)
    ));
    out.push_str(".globl _class_gc_desc_ptrs\n_class_gc_desc_ptrs:\n");
    if let Some(max_class_id) = max_class_id {
        for class_id in 0..=max_class_id {
            if class_info_by_id.contains_key(&class_id) {
                out.push_str(&format!("    .quad _class_gc_desc_{}\n", class_id));
            } else {
                out.push_str("    .quad _class_gc_desc_missing\n");
            }
        }
    }

    out.push_str(".globl _class_vtable_ptrs\n_class_vtable_ptrs:\n");
    if let Some(max_class_id) = max_class_id {
        for class_id in 0..=max_class_id {
            if class_info_by_id.contains_key(&class_id) {
                out.push_str(&format!("    .quad _class_vtable_{}\n", class_id));
            } else {
                out.push_str("    .quad _class_vtable_missing\n");
            }
        }
    }

    // Per-class destructor symbol table — consulted by __rt_call_object_destructor
    // (invoked at the top of __rt_object_free_deep) to run a class's PHP
    // __destruct before its storage is freed. Each entry resolves through the
    // implementing class so an inherited destructor dispatches to the ancestor's
    // emitted method symbol; `0` means the class and its ancestors declare no
    // __destruct, so no destructor call is made.
    out.push_str(".globl _class_destruct_count\n_class_destruct_count:\n");
    out.push_str(&format!(
        "    .quad {}\n",
        max_class_id.map_or(0, |class_id| class_id + 1)
    ));
    out.push_str(".globl _class_destruct_ptrs\n_class_destruct_ptrs:\n");
    if let Some(max_class_id) = max_class_id {
        let destruct_key = php_symbol_key("__destruct");
        for class_id in 0..=max_class_id {
            let entry = class_info_by_id
                .get(&class_id)
                .and_then(|class_info| class_info.method_impl_classes.get(&destruct_key))
                .map(|impl_class| method_symbol(impl_class, &destruct_key))
                .unwrap_or_else(|| "0".to_string());
            out.push_str(&format!("    .quad {}\n", entry));
        }
    }

    // Dense class-id-indexed __toString table for runtime string coercions that
    // cannot know the concrete class during EIR lowering (notably
    // unserialize()'s allowed_classes values).
    out.push_str(".globl _class_tostring_count\n_class_tostring_count:\n");
    out.push_str(&format!(
        "    .quad {}\n",
        max_class_id.map_or(0, |class_id| class_id + 1)
    ));
    out.push_str(".globl _class_tostring_ptrs\n_class_tostring_ptrs:\n");
    if let Some(max_class_id) = max_class_id {
        let tostring_key = php_symbol_key("__toString");
        for class_id in 0..=max_class_id {
            let entry = class_info_by_id
                .get(&class_id)
                .and_then(|class_info| class_info.method_impl_classes.get(&tostring_key))
                .map(|impl_class| method_symbol(impl_class, &tostring_key))
                .unwrap_or_else(|| "0".to_string());
            out.push_str(&format!("    .quad {}\n", entry));
        }
    }

    // Dense class-id-indexed interface-method tables for the runtime helpers that meet an object
    // only as a boxed `Mixed` and so cannot know its class during EIR lowering: `count($m)` reaches
    // `__rt_mixed_count` and `$m[$k]` reaches `__rt_mixed_array_get`. Both answered for a short
    // hard-coded ladder of RUNTIME-NATIVE SPL classes and returned 0 / null for everything else, so
    // a `Countable`/`ArrayAccess` written in PHP — the synthetic builtins like `ArrayObject`
    // included — was skipped in silence.
    //
    // An entry is filled only when all three hold, and is `0` otherwise so the helper keeps its
    // previous answer:
    //   - the class DECLARES the interface. A class that merely owns a `count` method must keep
    //     PHP's refusal rather than start answering.
    //   - the method resolves through its implementing class, so an inherited one dispatches to the
    //     ancestor's emitted symbol.
    //   - the method's return REPRESENTATION is the one the helper returns. `count` hands back a
    //     bare integer and `offsetGet` an owned `Mixed*`; a method typed otherwise would have its
    //     pointer read as an integer, or its payload read as a cell.
    out.push_str(".globl _class_iface_method_count\n_class_iface_method_count:\n");
    out.push_str(&format!(
        "    .quad {}\n",
        max_class_id.map_or(0, |class_id| class_id + 1)
    ));
    for (table, interface, method, returns) in [
        ("_class_count_ptrs", "Countable", "count", PhpType::Int),
        ("_class_offsetget_ptrs", "ArrayAccess", "offsetGet", PhpType::Mixed),
        ("_class_offsetset_ptrs", "ArrayAccess", "offsetSet", PhpType::Void),
    ] {
        out.push_str(&format!(".globl {table}\n{table}:\n"));
        if let Some(max_class_id) = max_class_id {
            let method_key = php_symbol_key(method);
            for class_id in 0..=max_class_id {
                let entry = class_info_by_id
                    .get(&class_id)
                    .filter(|class_info| {
                        class_info
                            .interfaces
                            .iter()
                            .any(|name| name.eq_ignore_ascii_case(interface))
                            && class_info
                                .methods
                                .get(&method_key)
                                .is_some_and(|sig| sig.return_type.codegen_repr() == returns)
                    })
                    .and_then(|class_info| class_info.method_impl_classes.get(&method_key))
                    .map(|impl_class| method_symbol(impl_class, &method_key))
                    .unwrap_or_else(|| "0".to_string());
                out.push_str(&format!("    .quad {}\n", entry));
            }
        }
    }

    // Per-class serialize-magic symbol tables — consulted by __rt_serialize_object
    // and __rt_unser_at_object. Each is a dense class_id-indexed table whose entry
    // resolves through the implementing class (so an inherited magic method
    // dispatches to the ancestor's emitted symbol); `0` means the class and its
    // ancestors declare no such method. `__serialize`/`__sleep` customise how an
    // object is written; `__unserialize`/`__wakeup` customise how it is restored.
    for (table, method) in [
        ("_class_serialize_ptrs", "__serialize"),
        ("_class_unserialize_ptrs", "__unserialize"),
        ("_class_sleep_ptrs", "__sleep"),
        ("_class_wakeup_ptrs", "__wakeup"),
    ] {
        out.push_str(&format!(".globl {table}\n{table}:\n"));
        if let Some(max_class_id) = max_class_id {
            let method_key = php_symbol_key(method);
            for class_id in 0..=max_class_id {
                let entry = class_info_by_id
                    .get(&class_id)
                    .and_then(|class_info| class_info.method_impl_classes.get(&method_key))
                    .map(|impl_class| method_symbol(impl_class, &method_key))
                    .unwrap_or_else(|| "0".to_string());
                out.push_str(&format!("    .quad {}\n", entry));
            }
        }
    }

    // _class_propinit_ptrs: dense class_id-indexed table of property-default
    // init thunks. Entry = _class_propinit_<id> when the class has any property
    // default, else 0 (null = nothing to init). __rt_new_by_name indexes this
    // by class_id and calls the thunk (when non-zero) after zeroing the object.
    // The has-default predicate MUST match EIR property-init thunk generation.
    out.push_str(".globl _class_propinit_ptrs\n_class_propinit_ptrs:\n");
    if let Some(max_class_id) = max_class_id {
        for class_id in 0..=max_class_id {
            match class_info_by_id.get(&class_id) {
                Some(class_info) if class_info.defaults.iter().any(|d| d.is_some()) => {
                    out.push_str(&format!("    .quad _class_propinit_{}\n", class_id));
                }
                _ => out.push_str("    .quad 0\n"),
            }
        }
    }

    // _class_serprop_ptrs: dense class_id-indexed table of serialize property-info
    // tables. Entry = _class_serprop_<id> for an existing class, else
    // _class_serprop_missing. __rt_serialize_object / __rt_unserialize_object index
    // this by class_id to walk an object's properties (PHP-mangled key bytes, byte
    // offset within the object, runtime value tag).
    out.push_str(".globl _class_serprop_ptrs\n_class_serprop_ptrs:\n");
    if let Some(max_class_id) = max_class_id {
        for class_id in 0..=max_class_id {
            if class_info_by_id.contains_key(&class_id) {
                out.push_str(&format!("    .quad _class_serprop_{}\n", class_id));
            } else {
                out.push_str("    .quad _class_serprop_missing\n");
            }
        }
    }

    // Parallel class-id-indexed property declaring-class tables used by
    // get_object_vars() to evaluate protected visibility against each property's
    // declaration scope instead of the runtime object's concrete class.
    out.push_str(".globl _class_serprop_declaring_ptrs\n_class_serprop_declaring_ptrs:\n");
    if let Some(max_class_id) = max_class_id {
        for class_id in 0..=max_class_id {
            if class_info_by_id.contains_key(&class_id) {
                out.push_str(&format!("    .quad _class_serprop_declaring_{}\n", class_id));
            } else {
                out.push_str("    .quad _class_serprop_declaring_missing\n");
            }
        }
    }

    out.push_str(".globl _class_static_vtable_ptrs\n_class_static_vtable_ptrs:\n");
    if let Some(max_class_id) = max_class_id {
        for class_id in 0..=max_class_id {
            if class_info_by_id.contains_key(&class_id) {
                out.push_str(&format!("    .quad _class_static_vtable_{}\n", class_id));
            } else {
                out.push_str("    .quad _class_static_vtable_missing\n");
            }
        }
    }

    out.push_str(".globl _class_callable_method_ptrs\n_class_callable_method_ptrs:\n");
    if let Some(max_class_id) = max_class_id {
        for class_id in 0..=max_class_id {
            if class_info_by_id.contains_key(&class_id) {
                out.push_str(&format!("    .quad _class_callable_methods_{}\n", class_id));
            } else {
                out.push_str("    .quad _class_callable_methods_missing\n");
            }
        }
    }

    out.push_str(".p2align 3\n");
    out.push_str(".globl _user_wrapper_vtable_ptrs\n_user_wrapper_vtable_ptrs:\n");
    if let Some(max_class_id) = max_class_id {
        for class_id in 0..=max_class_id {
            let class_publishes_wrapper_method = class_info_by_id
                .get(&class_id)
                .is_some_and(|class_info| class_has_user_wrapper_method(class_info));
            if class_publishes_wrapper_method {
                out.push_str(&format!("    .quad _user_wrapper_vtable_{}\n", class_id));
            } else {
                out.push_str("    .quad _user_wrapper_vtable_missing\n");
            }
        }
    }

    out.push_str(".p2align 3\n");
    out.push_str(".globl _user_filter_vtable_ptrs\n_user_filter_vtable_ptrs:\n");
    if let Some(max_class_id) = max_class_id {
        for class_id in 0..=max_class_id {
            let class_publishes_filter_method = class_info_by_id
                .get(&class_id)
                .is_some_and(|class_info| class_has_user_filter_method(class_info));
            if class_publishes_filter_method {
                out.push_str(&format!("    .quad _user_filter_vtable_{}\n", class_id));
            } else {
                out.push_str("    .quad _user_filter_vtable_missing\n");
            }
        }
    }

    out.push_str(".globl _class_interfaces_missing\n_class_interfaces_missing:\n");
    out.push_str("    .quad 0\n");
    out.push_str(".globl _class_gc_desc_missing\n_class_gc_desc_missing:\n");
    out.push_str("    .byte 0\n");
    // _class_serprop_missing: zero properties (a class with no serialize metadata).
    out.push_str("    .p2align 3\n");
    out.push_str(".globl _class_serprop_missing\n_class_serprop_missing:\n");
    out.push_str("    .quad 0\n"); // property count = 0
    out.push_str(".globl _class_serprop_declaring_missing\n_class_serprop_declaring_missing:\n");
    out.push_str("    .quad -1\n"); // no declaring class for a missing descriptor
    // _class_json_desc_missing: zero flags, zero properties, no jsonSerialize.
    out.push_str("    .p2align 3\n");
    out.push_str(".globl _class_json_desc_missing\n_class_json_desc_missing:\n");
    out.push_str("    .quad 0\n"); // flags
    out.push_str("    .quad 0\n"); // jsonSerialize target
    out.push_str("    .quad 0\n"); // public property count
    // _class_vd_desc_missing: zero properties (a class id with no var_dump metadata).
    out.push_str("    .p2align 3\n");
    out.push_str(".globl _class_vd_desc_missing\n_class_vd_desc_missing:\n");
    out.push_str("    .quad 0\n"); // property count = 0
    // _class_prop_desc_missing: zero properties (a class id with no print_r /
    // var_export metadata), so an unknown class renders an empty body instead of
    // reading past the table.
    out.push_str("    .p2align 3\n");
    out.push_str(".globl _class_prop_desc_missing\n_class_prop_desc_missing:\n");
    out.push_str("    .quad 0\n"); // property count = 0
    out.push_str("    .p2align 3\n");
    out.push_str(".globl _class_vtable_missing\n_class_vtable_missing:\n");
    out.push_str("    .quad 0\n");
    out.push_str("    .p2align 3\n");
    out.push_str(
        ".globl _class_static_vtable_missing\n_class_static_vtable_missing:\n",
    );
    out.push_str("    .quad 0\n");
    out.push_str("    .p2align 3\n");
    out.push_str(".globl _class_callable_methods_missing\n_class_callable_methods_missing:\n");
    out.push_str("    .quad 0\n");
    out.push_str("    .p2align 3\n");
    out.push_str(".globl _user_wrapper_vtable_missing\n_user_wrapper_vtable_missing:\n");
    // The method pointers plus BOTH trailing quads — the boxed-result mask and the `$context`
    // offset — so a class with no wrapper method shares a table the helpers can read to the same
    // extent. A short table here would let a helper read past its end.
    // +3: the boxed-result mask, the `$context` offset, and the constructor pointer.
    for _ in 0..USER_WRAPPER_VTABLE_SLOTS + 3 {
        out.push_str("    .quad 0\n");
    }
    out.push_str("    .p2align 3\n");
    out.push_str(".globl _user_filter_vtable_missing\n_user_filter_vtable_missing:\n");
    for _ in 0..USER_FILTER_VTABLE_SLOTS {
        out.push_str("    .quad 0\n");
    }
    out.push_str(".p2align 3\n");
    emit_static_callable_method_data(&mut out, &sorted_classes);
    out.push_str(".p2align 3\n");
    emit_script_source_file_data(&mut out, source_path);
    out.push_str(".p2align 3\n");
    emit_trace_exactness_flag(&mut out, has_user_frames);
    if emit_eval_reflection_metadata {
        out.push_str(".p2align 3\n");
        emit_eval_reflection_source_file_data(&mut out, source_path);
    }
    out.push_str(".p2align 3\n");
    emit_classes_by_name_table(&mut out, &sorted_classes);
    if emit_eval_reflection_metadata {
        out.push_str(".p2align 3\n");
        emit_eval_reflection_method_lookup_data(&mut out, &sorted_classes, &sorted_interfaces);
        out.push_str(".p2align 3\n");
        emit_eval_reflection_property_lookup_data(&mut out, &sorted_classes);
        out.push_str(".p2align 3\n");
        emit_eval_reflection_class_lookup_data(
            &mut out,
            &sorted_classes,
            &sorted_interfaces,
            declared_trait_source_lines,
        );
        out.push_str(".p2align 3\n");
        emit_eval_reflection_class_interface_lookup_data(&mut out, &sorted_classes, interfaces);
        out.push_str(".p2align 3\n");
        emit_eval_reflection_class_trait_lookup_data(
            &mut out,
            &sorted_classes,
            declared_trait_uses,
        );
        out.push_str(".p2align 3\n");
        emit_eval_reflection_class_trait_alias_lookup_data(&mut out, &sorted_classes);
    }

    // -- class-level PHP 8 attribute metadata table --
    // Per-class layout: count followed by (name_ptr, name_len) pairs.
    // Top-level pointer table indexes by class_id.
    out.push_str(".p2align 3\n");
    out.push_str(".globl _class_attribute_count\n_class_attribute_count:\n");
    out.push_str(&format!(
        "    .quad {}\n",
        max_class_id.map_or(0, |class_id| class_id + 1)
    ));
    out.push_str(".globl _class_attribute_ptrs\n_class_attribute_ptrs:\n");
    if let Some(max_class_id) = max_class_id {
        for class_id in 0..=max_class_id {
            let has_attrs = class_info_by_id
                .get(&class_id)
                .is_some_and(|info| !info.attribute_names.is_empty());
            if has_attrs {
                out.push_str(&format!("    .quad _class_attributes_{}\n", class_id));
            } else {
                out.push_str("    .quad _class_attributes_missing\n");
            }
        }
    }
    out.push_str(".globl _class_attributes_missing\n_class_attributes_missing:\n");
    out.push_str("    .quad 0\n"); // count = 0

    // Per-class attribute payloads. The per-class table holds 32-byte
    // entries: `(name_ptr, name_len, args_count, args_ptr)`. The args_ptr
    // points to a block of 24-byte tagged-arg entries — one per literal
    // argument captured at parse time. Each entry is laid out as
    // `(tag, lo, hi)` matching the runtime mixed-cell ABI:
    //
    //   tag 0 = int   (lo = i64 value,         hi = 0)
    //   tag 1 = str   (lo = .ascii label addr, hi = byte length)
    //   tag 3 = bool  (lo = 0 or 1,            hi = 0)
    //   tag 8 = null  (lo = 0,                 hi = 0)
    //
    // Unsupported args are represented as absent metadata by
    // `collect_attribute_args`; reflection helpers reject queries that would
    // need those payloads before codegen reaches this table. Float and other
    // mixed-cell payloads are reserved for future iterations.
    if let Some(max_class_id) = max_class_id {
        let mut name_id = 0u64;
        let mut arg_str_id = 0u64;
        let mut args_block_id = 0u64;
        for class_id in 0..=max_class_id {
            let Some(info) = class_info_by_id.get(&class_id) else {
                continue;
            };
            if info.attribute_names.is_empty() {
                continue;
            }
            let mut entries = Vec::with_capacity(info.attribute_names.len());
            for (idx, name) in info.attribute_names.iter().enumerate() {
                let name_label = format!("_attr_name_{}", name_id);
                name_id += 1;
                out.push_str(&format!(".globl {0}\n{0}:\n", name_label));
                out.push_str(&format!("    .ascii \"{}\"\n", escaped_ascii(name)));

                let empty_fallback = Vec::new();
                let args = info
                    .attribute_args
                    .get(idx)
                    .and_then(Option::as_ref)
                    .unwrap_or(&empty_fallback);
                let args_label = if args.is_empty() {
                    None
                } else {
                    // Intern any string-arg payload first so the per-arg
                    // table can reference it by label, then emit the tagged
                    // (tag, lo, hi) rows in source order.
                    let mut arg_rows = Vec::with_capacity(args.len());
                    for entry in args {
                        match &entry.value {
                            crate::types::AttrArgValue::Str(value) => {
                                let label = format!("_attr_arg_str_{}", arg_str_id);
                                arg_str_id += 1;
                                let bytes = crate::string_bytes::literal_bytes(value);
                                out.push_str(&format!(".globl {0}\n{0}:\n", label));
                                out.push_str(&format!(
                                    "    .ascii \"{}\"\n",
                                    escaped_bytes(&bytes)
                                ));
                                arg_rows.push((1u64, label, bytes.len() as u64));
                            }
                            crate::types::AttrArgValue::Int(value) => {
                                arg_rows.push((0u64, format!("{}", *value as u64), 0u64));
                            }
                            crate::types::AttrArgValue::Float(bits) => {
                                arg_rows.push((2u64, format!("{}", *bits), 0u64));
                            }
                            crate::types::AttrArgValue::Bool(value) => {
                                arg_rows.push((3u64, format!("{}", *value as u64), 0u64));
                            }
                            crate::types::AttrArgValue::Null => {
                                arg_rows.push((8u64, "0".to_string(), 0u64));
                            }
                            crate::types::AttrArgValue::Array(_)
                            | crate::types::AttrArgValue::ConstRef(_)
                            | crate::types::AttrArgValue::ScopedConst(..) => {
                                // This legacy flat (tag, lo, hi) table cannot
                                // represent a nested array or a deferred symbolic
                                // reference (global/class constant, enum case),
                                // and no runtime routine reads it; emit a null
                                // placeholder. The active EIR path materializes
                                // the real value from class metadata instead.
                                arg_rows.push((8u64, "0".to_string(), 0u64));
                            }
                        }
                    }
                    out.push_str("    .p2align 3\n");
                    let block_label = format!("_attr_args_{}", args_block_id);
                    args_block_id += 1;
                    out.push_str(&format!(".globl {0}\n{0}:\n", block_label));
                    for (tag, lo, hi) in arg_rows {
                        out.push_str(&format!("    .quad {}\n", tag));
                        out.push_str(&format!("    .quad {}\n", lo));
                        out.push_str(&format!("    .quad {}\n", hi));
                    }
                    Some(block_label)
                };
                entries.push((name_label, name.len(), args.len(), args_label));
            }
            out.push_str("    .p2align 3\n");
            out.push_str(&format!(
                ".globl _class_attributes_{0}\n_class_attributes_{0}:\n",
                class_id
            ));
            out.push_str(&format!("    .quad {}\n", info.attribute_names.len()));
            for (name_label, name_len, args_count, args_label) in entries {
                out.push_str(&format!("    .quad {}\n", name_label));
                out.push_str(&format!("    .quad {}\n", name_len));
                out.push_str(&format!("    .quad {}\n", args_count));
                out.push_str(&format!(
                    "    .quad {}\n",
                    args_label.as_deref().unwrap_or("0")
                ));
            }
        }
    }

    for (_, interface_info) in &sorted_interfaces {
        out.push_str(&format!(
            ".globl _interface_methods_{}\n_interface_methods_{}:\n",
            interface_info.interface_id, interface_info.interface_id
        ));
        out.push_str(&format!("    .quad {}\n", interface_info.method_order.len()));
        for method_name in &interface_info.method_order {
            let slot = interface_info
                .method_slots
                .get(method_name)
                .expect("codegen bug: missing interface method slot");
            out.push_str(&format!("    .quad {}\n", slot));
        }
    }

    for (class_name, class_info) in sorted_classes {
        out.push_str(&format!(".globl _class_interfaces_{}\n_class_interfaces_{}:\n", class_info.class_id, class_info.class_id));
        out.push_str(&format!("    .quad {}\n", class_info.interfaces.len()));
        for interface_name in &class_info.interfaces {
            let interface_info = interfaces
                .get(interface_name)
                .expect("codegen bug: missing interface metadata for class");
            out.push_str(&format!("    .quad {}\n", interface_info.interface_id));
            out.push_str(&format!(
                "    .quad _class_interface_impl_{}_{}\n",
                class_info.class_id, interface_info.interface_id
            ));
        }

        for interface_name in &class_info.interfaces {
            let interface_info = interfaces
                .get(interface_name)
                .expect("codegen bug: missing interface metadata for class");
            out.push_str(&format!(
                ".globl _class_interface_impl_{}_{}\n_class_interface_impl_{}_{}:\n",
                class_info.class_id, interface_info.interface_id,
                class_info.class_id, interface_info.interface_id
            ));
            if interface_info.method_order.is_empty() {
                out.push_str("    .quad 0\n");
                continue;
            }
            for method_name in &interface_info.method_order {
                if let Some(impl_class) = class_info.method_impl_classes.get(method_name) {
                    let symbol = interface_method_table_symbol(
                        class_info,
                        interface_info,
                        method_name,
                        impl_class,
                        classes,
                    );
                    out.push_str(&format!("    .quad {}\n", symbol));
                } else {
                    out.push_str("    .quad 0\n");
                }
            }
        }

        // Per-property name strings used by the JSON descriptor below. We
        // emit them as labels before the descriptor so the descriptor
        // table holds plain (ptr, len) pairs.
        let public_props: Vec<(usize, &(String, PhpType))> = class_info
            .properties
            .iter()
            .enumerate()
            .filter(|(prop_index, (name, _))| {
                class_info
                    .visible_property_index(name)
                    .is_some_and(|visible_index| visible_index == *prop_index)
            })
            .filter(|(_, (name, _))| {
                class_info
                    .property_visibilities
                    .get(name)
                    .map_or(true, |v| matches!(v, Visibility::Public))
            })
            .collect();
        for (prop_index, (prop_name, _)) in &public_props {
            out.push_str(&format!(
                ".globl _class_json_pname_{}_{}\n_class_json_pname_{}_{}:\n    .ascii {:?}\n",
                class_info.class_id, prop_index, class_info.class_id, prop_index, prop_name,
            ));
        }
        out.push_str("    .p2align 3\n");
        out.push_str(&format!(
            ".globl _class_json_desc_{}\n_class_json_desc_{}:\n",
            class_info.class_id, class_info.class_id,
        ));
        let implements_jsonserializable = class_info
            .interfaces
            .iter()
            .any(|i| i == "JsonSerializable");
        let flags: u64 = if implements_jsonserializable { 1 } else { 0 };
        out.push_str(&format!("    .quad {}\n", flags));
        if implements_jsonserializable {
            let key = php_symbol_key("jsonSerialize");
            if let Some(impl_class) = class_info.method_impl_classes.get(&key) {
                out.push_str(&format!(
                    "    .quad {}\n",
                    method_symbol(impl_class, &key),
                ));
            } else {
                out.push_str("    .quad 0\n");
            }
        } else {
            out.push_str("    .quad 0\n");
        }
        out.push_str(&format!("    .quad {}\n", public_props.len()));
        for (prop_index, (prop_name, prop_ty)) in &public_props {
            let tag = if class_info.property_slot_is_reference(*prop_index, prop_name) {
                0
            } else {
                match prop_ty {
                    PhpType::Int => 0,
                    PhpType::Str => 1,
                    PhpType::Float => 2,
                    PhpType::Bool | PhpType::False => 3,
                    PhpType::Array(_) => 4,
                    PhpType::AssocArray { .. } => 5,
                    PhpType::Object(_) => 6,
                    PhpType::Mixed | PhpType::Union(_) | PhpType::Iterable => 7,
                    PhpType::Resource(_) => 9,
                    PhpType::TaggedScalar => {
                        unreachable!("nullable scalar properties use the boxed Mixed representation")
                    }
                    PhpType::Callable
                    | PhpType::Pointer(_)
                    | PhpType::Buffer(_)
                    | PhpType::Packed(_)
                    | PhpType::Never
                    | PhpType::Void => 0,
                }
            };
            out.push_str(&format!(
                "    .quad _class_json_pname_{}_{}\n",
                class_info.class_id, prop_index,
            ));
            out.push_str(&format!("    .quad {}\n", prop_name.len()));
            out.push_str(&format!("    .quad {}\n", prop_index));
            out.push_str(&format!("    .quad {}\n", tag));
        }

        out.push_str("    .p2align 3\n");
        out.push_str(&format!(".globl _class_gc_desc_{}\n_class_gc_desc_{}:\n", class_info.class_id, class_info.class_id));
        if class_info.properties.is_empty() {
            out.push_str("    .byte 0\n");
        } else {
            out.push_str("    .byte ");
            for (i, (_, prop_ty)) in class_info.properties.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                let prop_name = &class_info.properties[i].0;
                let tag = if class_info.property_slot_is_reference(i, prop_name) {
                    0
                } else {
                    match prop_ty {
                        PhpType::Int => 0,
                        PhpType::Str => 1,
                        PhpType::Float => 2,
                        PhpType::Bool | PhpType::False => 3,
                        PhpType::Array(_) => 4,
                        PhpType::AssocArray { .. } => 5,
                        PhpType::Object(_) => 6,
                        PhpType::Mixed => 7,
                        PhpType::Union(_) => 7,
                        PhpType::Iterable => 7,
                        PhpType::Resource(_) => 9,
                        PhpType::TaggedScalar => {
                            unreachable!("nullable scalar properties use the boxed Mixed representation")
                        }
                        PhpType::Callable => 10,
                        PhpType::Pointer(_)
                        | PhpType::Buffer(_)
                        | PhpType::Packed(_)
                        | PhpType::Never
                        | PhpType::Void => 0,
                    }
                };
                out.push_str(&tag.to_string());
            }
            out.push('\n');
        }

        // Serialize property-info table: one row per declared property in
        // declaration order with the PHP-mangled serialize key bytes, the
        // property's byte offset within the object, and its runtime value tag.
        // __rt_serialize_object / __rt_unserialize_object walk this by class id.
        for (prop_index, (prop_name, _)) in class_info.properties.iter().enumerate() {
            let mangled = mangled_property_name(class_info, class_name, prop_name);
            out.push_str(&format!(
                ".globl _class_serpname_{}_{}\n_class_serpname_{}_{}:\n",
                class_info.class_id, prop_index, class_info.class_id, prop_index,
            ));
            out.push_str("    .byte ");
            for (i, byte) in mangled.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                out.push_str(&byte.to_string());
            }
            out.push('\n');
        }
        out.push_str("    .p2align 3\n");
        out.push_str(&format!(
            ".globl _class_serprop_{}\n_class_serprop_{}:\n",
            class_info.class_id, class_info.class_id,
        ));
        out.push_str(&format!("    .quad {}\n", class_info.properties.len()));
        for (prop_index, (prop_name, prop_ty)) in class_info.properties.iter().enumerate() {
            let mangled_len = mangled_property_name(class_info, class_name, prop_name).len();
            let offset = class_info
                .property_offsets
                .get(prop_name)
                .copied()
                .unwrap_or(8 + prop_index * 16);
            let tag = prop_value_tag(class_info, prop_name, prop_ty);
            out.push_str(&format!(
                "    .quad _class_serpname_{}_{}\n",
                class_info.class_id, prop_index
            ));
            out.push_str(&format!("    .quad {}\n", mangled_len)); // mangled key byte length
            out.push_str(&format!("    .quad {}\n", offset)); // byte offset within the object
            out.push_str(&format!("    .quad {}\n", tag)); // runtime value tag
        }
        out.push_str("    .p2align 3\n");
        out.push_str(&format!(
            ".globl _class_serprop_declaring_{}\n_class_serprop_declaring_{}:\n",
            class_info.class_id, class_info.class_id,
        ));
        for (prop_name, _) in &class_info.properties {
            let declaring_class = class_info
                .property_declaring_classes
                .get(prop_name)
                .map(String::as_str)
                .unwrap_or(class_name);
            let declaring_class_id = all_class_id_by_name
                .get(declaring_class)
                .copied()
                .unwrap_or(class_info.class_id);
            out.push_str(&format!("    .quad {}\n", declaring_class_id));
        }

        // var_dump property-info table: one row per RENDERED property, carrying the
        // text PHP renders BETWEEN the `[` and `]` of the key line (`"p"`,
        // `"p":protected`, `"p":"C":private`), the property's byte offset within the
        // object, its runtime value tag, and the declared type name
        // `uninitialized(...)` needs. `__rt_var_dump_object` walks this by class id.
        // Kept separate from `_class_serprop_*` because serialize's key is NUL-mangled
        // and separate from `_class_json_desc_*` because JSON only carries public
        // properties — and, unlike both of those, this table honours `__debugInfo()`
        // (see `var_dump_debug_info_projection`), because `var_dump` is the only PHP
        // renderer that consults `__debugInfo` AND the only elephc renderer that
        // enumerates object properties at all.
        let mut vd_rows = var_dump_descriptor_rows(class_info, class_name);
        if enums.contains_key(class_name.as_str()) {
            hoist_enum_name_row(&mut vd_rows);
        }
        for (row_index, row) in vd_rows.iter().enumerate() {
            out.push_str(&format!(
                ".globl _class_vd_pkey_{}_{}\n_class_vd_pkey_{}_{}:\n    .ascii \"{}\"\n",
                class_info.class_id, row_index, class_info.class_id, row_index,
                escaped_ascii(&row.key),
            ));
            out.push_str(&format!(
                ".globl _class_vd_ptype_{}_{}\n_class_vd_ptype_{}_{}:\n    .ascii \"{}\"\n",
                class_info.class_id, row_index, class_info.class_id, row_index,
                escaped_ascii(&row.type_name),
            ));
        }
        out.push_str("    .p2align 3\n");
        out.push_str(&format!(
            ".globl _class_vd_desc_{}\n_class_vd_desc_{}:\n",
            class_info.class_id, class_info.class_id,
        ));
        out.push_str(&format!("    .quad {}\n", vd_rows.len()));
        for (row_index, row) in vd_rows.iter().enumerate() {
            out.push_str(&format!(
                "    .quad _class_vd_pkey_{}_{}\n",
                class_info.class_id, row_index
            ));
            out.push_str(&format!("    .quad {}\n", row.key.len())); // rendered key byte length
            out.push_str(&format!("    .quad {}\n", row.offset)); // byte offset within the object
            out.push_str(&format!("    .quad {}\n", row.tag)); // runtime value tag
            out.push_str(&format!(
                "    .quad _class_vd_ptype_{}_{}\n",
                class_info.class_id, row_index
            ));
            out.push_str(&format!("    .quad {}\n", row.type_name.len())); // declared type-name byte length
        }

        // print_r / var_export property table: the SAME rows (and therefore the
        // same `__debugInfo()` projection, offsets and value tags) as the
        // var_dump descriptor above, but carrying the two other key spellings PHP
        // uses for the same property — `print_r`'s unquoted `x` / `y:protected` /
        // `z:C:private`, and `var_export`'s bare `x`. Sharing one row list is what
        // keeps the three renderers from ever disagreeing about which properties
        // an object has or where they live.
        for (row_index, row) in vd_rows.iter().enumerate() {
            out.push_str(&format!(
                ".globl _class_prop_pkey_{}_{}\n_class_prop_pkey_{}_{}:\n    .ascii \"{}\"\n",
                class_info.class_id, row_index, class_info.class_id, row_index,
                escaped_ascii(&row.print_r_key),
            ));
            out.push_str(&format!(
                ".globl _class_prop_nkey_{}_{}\n_class_prop_nkey_{}_{}:\n    .ascii \"{}\"\n",
                class_info.class_id, row_index, class_info.class_id, row_index,
                escaped_ascii(&row.plain_key),
            ));
        }
        out.push_str("    .p2align 3\n");
        out.push_str(&format!(
            ".globl _class_prop_desc_{}\n_class_prop_desc_{}:\n",
            class_info.class_id, class_info.class_id,
        ));
        out.push_str(&format!("    .quad {}\n", vd_rows.len()));
        for (row_index, row) in vd_rows.iter().enumerate() {
            out.push_str(&format!(
                "    .quad _class_prop_pkey_{}_{}\n",
                class_info.class_id, row_index
            ));
            out.push_str(&format!("    .quad {}\n", row.print_r_key.len())); // print_r key byte length
            out.push_str(&format!("    .quad {}\n", row.offset)); // byte offset within the object
            out.push_str(&format!("    .quad {}\n", row.tag)); // runtime value tag
            out.push_str(&format!(
                "    .quad _class_prop_nkey_{}_{}\n",
                class_info.class_id, row_index
            ));
            out.push_str(&format!("    .quad {}\n", row.plain_key.len())); // bare property-name byte length
        }

        out.push_str("    .p2align 3\n");
        out.push_str(&format!(".globl _class_vtable_{}\n_class_vtable_{}:\n", class_info.class_id, class_info.class_id));
        if class_info.vtable_methods.is_empty() {
            out.push_str("    .quad 0\n");
        } else {
            for method_name in &class_info.vtable_methods {
                if let Some(impl_class) = class_info.method_impl_classes.get(method_name) {
                    out.push_str(&format!("    .quad {}\n", method_symbol(impl_class, method_name)));
                } else {
                    out.push_str("    .quad 0\n");
                }
            }
        }

        out.push_str("    .p2align 3\n");
        out.push_str(&format!(".globl _class_static_vtable_{}\n_class_static_vtable_{}:\n", class_info.class_id, class_info.class_id));
        if class_info.static_vtable_methods.is_empty() {
            out.push_str("    .quad 0\n");
        } else {
            for method_name in &class_info.static_vtable_methods {
                if let Some(impl_class) = class_info.static_method_impl_classes.get(method_name) {
                    out.push_str(&format!("    .quad {}\n", static_method_symbol(impl_class, method_name)));
                } else {
                    out.push_str("    .quad 0\n");
                }
            }
        }

        emit_class_callable_methods(&mut out, class_info);
        emit_user_wrapper_vtable(&mut out, class_info);
        emit_user_filter_vtable(&mut out, class_info);
    }

    let stdclass_id = classes
        .get("stdClass")
        .map(|class_info| class_info.class_id as i64)
        .unwrap_or(-1);
    out.push_str(".p2align 3\n");
    out.push_str(".globl _stdclass_class_id\n_stdclass_class_id:\n");
    out.push_str(&format!("    .quad {}\n", stdclass_id));

    out
}

/// Emits a dense class-id to class-name lookup table for runtime `get_class()`.
///
/// Each `_class_name_entries` row is two words: `(name_ptr, name_len)`. Missing
/// class ids point at `_class_name_missing` with length zero so runtime lookups
/// can fail to an empty string without branching into undefined labels.
fn emit_class_name_lookup_data(
    out: &mut String,
    max_class_id: Option<u64>,
    class_name_by_id: &HashMap<u64, &String>,
) {
    out.push_str(".p2align 3\n");
    out.push_str(".globl _class_name_count\n_class_name_count:\n");
    out.push_str(&format!(
        "    .quad {}\n",
        max_class_id.map_or(0, |class_id| class_id + 1)
    ));
    out.push_str(".globl _class_name_entries\n_class_name_entries:\n");
    if let Some(max_class_id) = max_class_id {
        for class_id in 0..=max_class_id {
            if let Some(class_name) = class_name_by_id.get(&class_id) {
                out.push_str(&format!("    .quad _class_name_{}\n", class_id));
                out.push_str(&format!("    .quad {}\n", class_name.len()));
            } else {
                out.push_str("    .quad _class_name_missing\n");
                out.push_str("    .quad 0\n");
            }
        }
    }
    out.push_str(".globl _class_name_missing\n_class_name_missing:\n");
    out.push_str("    .byte 0\n");
    if let Some(max_class_id) = max_class_id {
        for class_id in 0..=max_class_id {
            let Some(class_name) = class_name_by_id.get(&class_id) else {
                continue;
            };
            out.push_str(&format!(
                ".globl _class_name_{0}\n_class_name_{0}:\n",
                class_id
            ));
            out.push_str(&format!("    .ascii \"{}\"\n", escaped_ascii(class_name)));
        }
    }
    out.push_str("    .p2align 3\n");
}

/// Emits a compact `(name_ptr, name_len)` table for runtime class-like name probes.
fn emit_name_lookup_data(
    out: &mut String,
    count_symbol: &str,
    table_symbol: &str,
    label_prefix: &str,
    names: &[String],
) {
    let mut sorted_names: Vec<&String> = names.iter().collect();
    sorted_names.sort();
    for (idx, name) in sorted_names.iter().enumerate() {
        out.push_str(&format!(
            ".globl {0}_{1}\n{0}_{1}:\n    .ascii \"{2}\"\n",
            label_prefix,
            idx,
            escaped_ascii(name)
        ));
    }
    out.push_str(".p2align 3\n");
    out.push_str(&format!(".globl {0}\n{0}:\n", count_symbol));
    out.push_str(&format!("    .quad {}\n", sorted_names.len()));
    out.push_str(&format!(".globl {0}\n{0}:\n", table_symbol));
    for (idx, name) in sorted_names.iter().enumerate() {
        out.push_str(&format!("    .quad {}_{}\n", label_prefix, idx));
        out.push_str(&format!("    .quad {}\n", name.len()));
    }
}

/// Emits the compiled script's canonical path, read by `Throwable::getFile()` and by the
/// ` in <file>:<line>` suffix of `__rt_report_uncaught_exception`.
///
/// Emitted UNCONDITIONALLY, unlike [`emit_eval_reflection_source_file_data`], because an
/// uncaught exception can end any program whether or not it uses eval. The bytes are the same
/// canonicalized string `crate::magic_constants::file_pass` bakes for `__FILE__`, so a program
/// that mentions `__FILE__` already carries them; a length of zero means the module had no source
/// path (a synthesized or in-memory module) and the readers fall back to omitting the location
/// rather than printing an empty filename.
fn emit_script_source_file_data(out: &mut String, source_path: Option<&str>) {
    let source_path = source_path.unwrap_or("");
    out.push_str(".globl _script_source_file\n_script_source_file:\n");
    out.push_str(&format!("    .ascii \"{}\"\n", escaped_ascii(source_path)));
    out.push_str(".p2align 3\n");
    out.push_str(".globl _script_source_file_len\n_script_source_file_len:\n");
    out.push_str(&format!("    .quad {}\n", source_path.len()));
}

/// Emits whether a recorded stack trace can be trusted to be COMPLETE for this module.
///
/// php's trace names every frame. elephc records the builtin frame that raised, and nothing else,
/// so the moment a module can put a user frame on the stack the recorded list may be short — and a
/// short trace is not an approximation. `#0 {main}` where php names a function asserts the stack
/// was empty, which is a wrong answer rather than a missing one, so the report prints nothing at
/// all in that case, exactly as it did before.
///
/// The condition is therefore "this module declares no user function and no user class". It is
/// deliberately coarse: it is easy to state, impossible to get subtly wrong, and it holds for the
/// scripts whose whole trace IS a builtin frame plus `{main}`.
fn emit_trace_exactness_flag(out: &mut String, has_user_frames: bool) {
    let exact = usize::from(!has_user_frames);
    out.push_str(".globl _rt_trace_exact\n_rt_trace_exact:\n");
    out.push_str(&format!("    .quad {}\n", exact));
}

/// Emits the source filename used by eval Reflection source-location hooks.
fn emit_eval_reflection_source_file_data(out: &mut String, source_path: Option<&str>) {
    let source_path = source_path.unwrap_or("");
    out.push_str(".globl _eval_reflection_source_file\n_eval_reflection_source_file:\n");
    out.push_str(&format!("    .ascii \"{}\"\n", escaped_ascii(source_path)));
    out.push_str(".p2align 3\n");
    out.push_str(".globl _eval_reflection_source_file_len\n_eval_reflection_source_file_len:\n");
    out.push_str(&format!("    .quad {}\n", source_path.len()));
}

/// Emits AOT method flag rows consumed by eval ReflectionMethod metadata probes.
fn emit_eval_reflection_method_lookup_data(
    out: &mut String,
    sorted_classes: &[(&String, &ClassInfo)],
    sorted_interfaces: &[(&String, &InterfaceInfo)],
) {
    let mut entries = Vec::new();
    let class_infos = sorted_classes
        .iter()
        .map(|(name, info)| (name.as_str(), *info))
        .collect::<HashMap<_, _>>();
    let mut index = 0usize;
    for (class_name, class_info) in sorted_classes {
        let mut methods = class_info.methods.keys().collect::<Vec<_>>();
        methods.sort();
        for method_name in methods {
            let declaring_class = eval_reflection_instance_method_declaring_class(
                class_name,
                class_info,
                method_name,
            );
            let declaring_info = class_infos.get(declaring_class).copied().unwrap_or(class_info);
            let flags = eval_reflection_method_flags_with_source_lines(
                eval_reflection_instance_method_flags(class_info, method_name),
                declaring_info,
                method_name,
                false,
            );
            push_eval_reflection_method_lookup_row(
                out,
                &mut entries,
                &mut index,
                class_name,
                method_name,
                flags,
                declaring_class,
            );
        }

        let mut static_methods = class_info.static_methods.keys().collect::<Vec<_>>();
        static_methods.sort();
        for method_name in static_methods {
            let declaring_class =
                eval_reflection_static_method_declaring_class(class_name, class_info, method_name);
            let declaring_info = class_infos.get(declaring_class).copied().unwrap_or(class_info);
            let flags = eval_reflection_method_flags_with_source_lines(
                eval_reflection_static_method_flags(class_info, method_name),
                declaring_info,
                method_name,
                true,
            );
            push_eval_reflection_method_lookup_row(
                out,
                &mut entries,
                &mut index,
                class_name,
                method_name,
                flags,
                declaring_class,
            );
        }
    }

    for (interface_name, interface_info) in sorted_interfaces {
        let mut methods = interface_info.methods.keys().collect::<Vec<_>>();
        methods.sort();
        for method_name in methods {
            let declaring_interface = eval_reflection_interface_method_declaring_interface(
                interface_name,
                interface_info,
                method_name,
            );
            push_eval_reflection_method_lookup_row(
                out,
                &mut entries,
                &mut index,
                interface_name,
                method_name,
                eval_reflection_interface_method_flags(false),
                declaring_interface,
            );
        }

        let mut static_methods = interface_info.static_methods.keys().collect::<Vec<_>>();
        static_methods.sort();
        for method_name in static_methods {
            let declaring_interface = eval_reflection_interface_static_method_declaring_interface(
                interface_name,
                interface_info,
                method_name,
            );
            push_eval_reflection_method_lookup_row(
                out,
                &mut entries,
                &mut index,
                interface_name,
                method_name,
                eval_reflection_interface_method_flags(true),
                declaring_interface,
            );
        }
    }

    out.push_str(".p2align 3\n");
    out.push_str(".globl _eval_reflection_method_count\n_eval_reflection_method_count:\n");
    out.push_str(&format!("    .quad {}\n", entries.len()));
    out.push_str(".globl _eval_reflection_methods\n_eval_reflection_methods:\n");
    for (class_label, class_len, method_label, method_len, flags, declaring_label, declaring_len) in
        entries
    {
        out.push_str(&format!("    .quad {}\n", class_label));
        out.push_str(&format!("    .quad {}\n", class_len));
        out.push_str(&format!("    .quad {}\n", method_label));
        out.push_str(&format!("    .quad {}\n", method_len));
        out.push_str(&format!("    .quad {}\n", flags));
        out.push_str(&format!("    .quad {}\n", declaring_label));
        out.push_str(&format!("    .quad {}\n", declaring_len));
    }
}

/// Adds one eval ReflectionMethod lookup row and its backing string labels.
fn push_eval_reflection_method_lookup_row(
    out: &mut String,
    entries: &mut Vec<(String, usize, String, usize, u64, String, usize)>,
    index: &mut usize,
    class_name: &str,
    method_name: &str,
    flags: u64,
    declaring_class: &str,
) {
    let class_label = format!("_eval_reflection_method_class_{}", *index);
    let method_label = format!("_eval_reflection_method_name_{}", *index);
    let declaring_label = format!("_eval_reflection_method_declaring_class_{}", *index);
    out.push_str(&format!(
        ".globl {0}\n{0}:\n    .ascii \"{1}\"\n",
        class_label,
        escaped_ascii(class_name)
    ));
    out.push_str(&format!(
        ".globl {0}\n{0}:\n    .ascii \"{1}\"\n",
        method_label,
        escaped_ascii(method_name)
    ));
    out.push_str(&format!(
        ".globl {0}\n{0}:\n    .ascii \"{1}\"\n",
        declaring_label,
        escaped_ascii(declaring_class)
    ));
    entries.push((
        class_label,
        class_name.len(),
        method_label,
        method_name.len(),
        flags,
        declaring_label,
        declaring_class.len(),
    ));
    *index += 1;
}

/// Adds source start/end line bits to an AOT ReflectionMethod flag word when available.
fn eval_reflection_method_flags_with_source_lines(
    flags: u64,
    class_info: &ClassInfo,
    method_name: &str,
    is_static: bool,
) -> u64 {
    let Some(method) = class_info
        .method_decls
        .iter()
        .find(|method| method.is_static == is_static && php_symbol_key(&method.name) == method_name)
    else {
        return flags;
    };
    let start_line = u64::from(method.span.line);
    if start_line == 0 || start_line > EVAL_REFLECTION_METHOD_SOURCE_LINE_MASK {
        return flags;
    }
    flags
        | (start_line << EVAL_REFLECTION_METHOD_SOURCE_START_SHIFT)
        | (start_line << EVAL_REFLECTION_METHOD_SOURCE_END_SHIFT)
}

/// Returns the class name that declares one visible instance method.
fn eval_reflection_instance_method_declaring_class<'a>(
    reflected_class: &'a str,
    class_info: &'a ClassInfo,
    method_name: &str,
) -> &'a str {
    class_info
        .method_impl_classes
        .get(method_name)
        .or_else(|| class_info.method_declaring_classes.get(method_name))
        .map(String::as_str)
        .unwrap_or(reflected_class)
}

/// Returns the class name that declares one visible static method.
fn eval_reflection_static_method_declaring_class<'a>(
    reflected_class: &'a str,
    class_info: &'a ClassInfo,
    method_name: &str,
) -> &'a str {
    class_info
        .static_method_impl_classes
        .get(method_name)
        .or_else(|| class_info.static_method_declaring_classes.get(method_name))
        .map(String::as_str)
        .unwrap_or(reflected_class)
}

/// Returns the interface name that declares one visible instance method.
fn eval_reflection_interface_method_declaring_interface<'a>(
    reflected_interface: &'a str,
    interface_info: &'a InterfaceInfo,
    method_name: &str,
) -> &'a str {
    interface_info
        .method_declaring_interfaces
        .get(method_name)
        .map(String::as_str)
        .unwrap_or(reflected_interface)
}

/// Returns the interface name that declares one visible static method.
fn eval_reflection_interface_static_method_declaring_interface<'a>(
    reflected_interface: &'a str,
    interface_info: &'a InterfaceInfo,
    method_name: &str,
) -> &'a str {
    interface_info
        .static_method_declaring_interfaces
        .get(method_name)
        .map(String::as_str)
        .unwrap_or(reflected_interface)
}

/// Returns eval ReflectionMethod bitflags for one instance method entry.
fn eval_reflection_instance_method_flags(class_info: &ClassInfo, method_name: &str) -> u64 {
    let visibility = class_info
        .method_visibilities
        .get(method_name)
        .unwrap_or(&Visibility::Public);
    let mut flags = eval_reflection_method_visibility_flags(visibility);
    if class_info.final_methods.contains(method_name) {
        flags |= EVAL_REFLECTION_METHOD_FLAG_FINAL;
    }
    if !class_info.method_impl_classes.contains_key(method_name) {
        flags |= EVAL_REFLECTION_METHOD_FLAG_ABSTRACT;
    }
    flags
}

/// Returns eval ReflectionMethod bitflags for one static method entry.
fn eval_reflection_static_method_flags(class_info: &ClassInfo, method_name: &str) -> u64 {
    let visibility = class_info
        .static_method_visibilities
        .get(method_name)
        .unwrap_or(&Visibility::Public);
    let mut flags =
        EVAL_REFLECTION_METHOD_FLAG_STATIC | eval_reflection_method_visibility_flags(visibility);
    if class_info.final_static_methods.contains(method_name) {
        flags |= EVAL_REFLECTION_METHOD_FLAG_FINAL;
    }
    if !class_info.static_method_impl_classes.contains_key(method_name) {
        flags |= EVAL_REFLECTION_METHOD_FLAG_ABSTRACT;
    }
    flags
}

/// Returns eval ReflectionMethod bitflags for one interface method entry.
fn eval_reflection_interface_method_flags(is_static: bool) -> u64 {
    let mut flags = EVAL_REFLECTION_METHOD_FLAG_PUBLIC | EVAL_REFLECTION_METHOD_FLAG_ABSTRACT;
    if is_static {
        flags |= EVAL_REFLECTION_METHOD_FLAG_STATIC;
    }
    flags
}

/// Converts method visibility metadata into eval ReflectionMethod flag bits.
fn eval_reflection_method_visibility_flags(visibility: &Visibility) -> u64 {
    match visibility {
        Visibility::Public => EVAL_REFLECTION_METHOD_FLAG_PUBLIC,
        Visibility::Protected => EVAL_REFLECTION_METHOD_FLAG_PROTECTED,
        Visibility::Private => EVAL_REFLECTION_METHOD_FLAG_PRIVATE,
    }
}

/// Emits AOT property flag rows consumed by eval ReflectionProperty metadata probes.
fn emit_eval_reflection_property_lookup_data(
    out: &mut String,
    sorted_classes: &[(&String, &ClassInfo)],
) {
    let mut entries = Vec::new();
    let mut index = 0usize;
    for (class_name, class_info) in sorted_classes {
        for (slot, (property_name, _)) in class_info.properties.iter().enumerate() {
            let flags = eval_reflection_instance_property_flags(class_info, slot, property_name);
            let class_label = format!("_eval_reflection_property_class_{}", index);
            let property_label = format!("_eval_reflection_property_name_{}", index);
            let declaring_class = eval_reflection_instance_property_declaring_class(
                class_name,
                class_info,
                property_name,
            );
            let declaring_label = format!("_eval_reflection_property_declaring_class_{}", index);
            out.push_str(&format!(
                ".globl {0}\n{0}:\n    .ascii \"{1}\"\n",
                class_label,
                escaped_ascii(class_name)
            ));
            out.push_str(&format!(
                ".globl {0}\n{0}:\n    .ascii \"{1}\"\n",
                property_label,
                escaped_ascii(property_name)
            ));
            out.push_str(&format!(
                ".globl {0}\n{0}:\n    .ascii \"{1}\"\n",
                declaring_label,
                escaped_ascii(declaring_class)
            ));
            entries.push((
                class_label,
                class_name.len(),
                property_label,
                property_name.len(),
                flags,
                declaring_label,
                declaring_class.len(),
            ));
            index += 1;
        }
        for (slot, (property_name, _)) in class_info.static_properties.iter().enumerate() {
            let flags = eval_reflection_static_property_flags(class_info, slot, property_name);
            let class_label = format!("_eval_reflection_property_class_{}", index);
            let property_label = format!("_eval_reflection_property_name_{}", index);
            let declaring_class = eval_reflection_static_property_declaring_class(
                class_name,
                class_info,
                property_name,
            );
            let declaring_label = format!("_eval_reflection_property_declaring_class_{}", index);
            out.push_str(&format!(
                ".globl {0}\n{0}:\n    .ascii \"{1}\"\n",
                class_label,
                escaped_ascii(class_name)
            ));
            out.push_str(&format!(
                ".globl {0}\n{0}:\n    .ascii \"{1}\"\n",
                property_label,
                escaped_ascii(property_name)
            ));
            out.push_str(&format!(
                ".globl {0}\n{0}:\n    .ascii \"{1}\"\n",
                declaring_label,
                escaped_ascii(declaring_class)
            ));
            entries.push((
                class_label,
                class_name.len(),
                property_label,
                property_name.len(),
                flags,
                declaring_label,
                declaring_class.len(),
            ));
            index += 1;
        }
    }

    out.push_str(".p2align 3\n");
    out.push_str(".globl _eval_reflection_property_count\n_eval_reflection_property_count:\n");
    out.push_str(&format!("    .quad {}\n", entries.len()));
    out.push_str(".globl _eval_reflection_properties\n_eval_reflection_properties:\n");
    for (class_label, class_len, property_label, property_len, flags, declaring_label, declaring_len) in
        entries
    {
        out.push_str(&format!("    .quad {}\n", class_label));
        out.push_str(&format!("    .quad {}\n", class_len));
        out.push_str(&format!("    .quad {}\n", property_label));
        out.push_str(&format!("    .quad {}\n", property_len));
        out.push_str(&format!("    .quad {}\n", flags));
        out.push_str(&format!("    .quad {}\n", declaring_label));
        out.push_str(&format!("    .quad {}\n", declaring_len));
    }
}

/// Returns the class name that declares one visible instance property.
fn eval_reflection_instance_property_declaring_class<'a>(
    reflected_class: &'a str,
    class_info: &'a ClassInfo,
    property_name: &str,
) -> &'a str {
    class_info
        .property_declaring_classes
        .get(property_name)
        .map(String::as_str)
        .unwrap_or(reflected_class)
}

/// Returns the class name that declares one visible static property.
fn eval_reflection_static_property_declaring_class<'a>(
    reflected_class: &'a str,
    class_info: &'a ClassInfo,
    property_name: &str,
) -> &'a str {
    class_info
        .static_property_declaring_classes
        .get(property_name)
        .map(String::as_str)
        .unwrap_or(reflected_class)
}

/// Emits AOT class flag rows consumed by eval ReflectionClass metadata probes.
fn emit_eval_reflection_class_lookup_data(
    out: &mut String,
    sorted_classes: &[(&String, &ClassInfo)],
    sorted_interfaces: &[(&String, &InterfaceInfo)],
    declared_trait_source_lines: &HashMap<String, u32>,
) {
    let mut entries = Vec::new();
    let mut index = 0usize;
    for (class_name, class_info) in sorted_classes {
        let flags = eval_reflection_class_flags(class_info);
        if flags == 0 {
            continue;
        }
        let class_label = format!("_eval_reflection_class_name_{}", index);
        out.push_str(&format!(
            ".globl {0}\n{0}:\n    .ascii \"{1}\"\n",
            class_label,
            escaped_ascii(class_name)
        ));
        entries.push((class_label, class_name.len(), flags));
        index += 1;
    }
    for (interface_name, interface_info) in sorted_interfaces {
        let flags = eval_reflection_interface_flags(interface_info);
        if flags == 0 {
            continue;
        }
        let class_label = format!("_eval_reflection_class_name_{}", index);
        out.push_str(&format!(
            ".globl {0}\n{0}:\n    .ascii \"{1}\"\n",
            class_label,
            escaped_ascii(interface_name)
        ));
        entries.push((class_label, interface_name.len(), flags));
        index += 1;
    }
    let mut sorted_trait_lines = declared_trait_source_lines.iter().collect::<Vec<_>>();
    sorted_trait_lines
        .sort_by(|(left_name, _), (right_name, _)| left_name.cmp(right_name));
    for (trait_name, line) in sorted_trait_lines {
        let flags = eval_reflection_trait_flags(*line);
        if flags == 0 {
            continue;
        }
        let class_label = format!("_eval_reflection_class_name_{}", index);
        out.push_str(&format!(
            ".globl {0}\n{0}:\n    .ascii \"{1}\"\n",
            class_label,
            escaped_ascii(trait_name)
        ));
        entries.push((class_label, trait_name.len(), flags));
        index += 1;
    }

    out.push_str(".p2align 3\n");
    out.push_str(".globl _eval_reflection_class_count\n_eval_reflection_class_count:\n");
    out.push_str(&format!("    .quad {}\n", entries.len()));
    out.push_str(".globl _eval_reflection_classes\n_eval_reflection_classes:\n");
    for (class_label, class_len, flags) in entries {
        out.push_str(&format!("    .quad {}\n", class_label));
        out.push_str(&format!("    .quad {}\n", class_len));
        out.push_str(&format!("    .quad {}\n", flags));
    }
}

/// Returns eval ReflectionClass flag bits retained for one generated/AOT class.
fn eval_reflection_class_flags(class_info: &ClassInfo) -> u64 {
    let mut flags = 0;
    if class_info.is_final {
        flags |= EVAL_REFLECTION_CLASS_FLAG_FINAL;
    }
    if class_info.is_abstract {
        flags |= EVAL_REFLECTION_CLASS_FLAG_ABSTRACT;
    }
    if class_info.is_readonly_class {
        flags |= EVAL_REFLECTION_CLASS_FLAG_READONLY;
    }
    flags |= eval_reflection_source_line_flags(class_info.declaration_span.line);
    flags
}

/// Returns eval ReflectionClass source-location bits retained for one generated/AOT interface.
fn eval_reflection_interface_flags(interface_info: &InterfaceInfo) -> u64 {
    eval_reflection_source_line_flags(interface_info.declaration_span.line)
}

/// Returns eval ReflectionClass source-location bits retained for one generated/AOT trait.
fn eval_reflection_trait_flags(line: u32) -> u64 {
    eval_reflection_source_line_flags(line)
}

/// Encodes declaration line metadata into high ReflectionClass flag bits.
fn eval_reflection_source_line_flags(line: u32) -> u64 {
    let start_line = u64::from(line);
    if start_line == 0 || start_line > EVAL_REFLECTION_CLASS_SOURCE_LINE_MASK {
        return 0;
    }
    (start_line << EVAL_REFLECTION_CLASS_SOURCE_START_SHIFT)
        | (start_line << EVAL_REFLECTION_CLASS_SOURCE_END_SHIFT)
}

/// Emits class-like/interface-name rows consumed by eval ReflectionClass metadata probes.
fn emit_eval_reflection_class_interface_lookup_data(
    out: &mut String,
    sorted_classes: &[(&String, &ClassInfo)],
    interfaces: &HashMap<String, InterfaceInfo>,
) {
    let mut entries = Vec::new();
    let mut index = 0usize;
    for (class_name, class_info) in sorted_classes {
        for interface_name in &class_info.interfaces {
            push_eval_reflection_class_interface_row(
                out,
                &mut entries,
                &mut index,
                class_name,
                interface_name,
            );
        }
    }

    let mut sorted_interfaces: Vec<&String> = interfaces.keys().collect();
    sorted_interfaces.sort();
    for interface_name in sorted_interfaces {
        for parent_name in eval_reflection_interface_parent_names(interface_name, interfaces) {
            push_eval_reflection_class_interface_row(
                out,
                &mut entries,
                &mut index,
                interface_name,
                &parent_name,
            );
        }
    }

    out.push_str(".p2align 3\n");
    out.push_str(
        ".globl _eval_reflection_class_interface_count\n_eval_reflection_class_interface_count:\n",
    );
    out.push_str(&format!("    .quad {}\n", entries.len()));
    out.push_str(".globl _eval_reflection_class_interfaces\n_eval_reflection_class_interfaces:\n");
    for (class_label, class_len, interface_label, interface_len) in entries {
        out.push_str(&format!("    .quad {}\n", class_label));
        out.push_str(&format!("    .quad {}\n", class_len));
        out.push_str(&format!("    .quad {}\n", interface_label));
        out.push_str(&format!("    .quad {}\n", interface_len));
    }
}

/// Adds one class-like/interface-name row and its backing string labels.
fn push_eval_reflection_class_interface_row(
    out: &mut String,
    entries: &mut Vec<(String, usize, String, usize)>,
    index: &mut usize,
    class_name: &str,
    interface_name: &str,
) {
    let class_label = format!("_eval_reflection_class_interface_class_{}", *index);
    let interface_label = format!("_eval_reflection_class_interface_name_{}", *index);
    out.push_str(&format!(
        ".globl {0}\n{0}:\n    .ascii \"{1}\"\n",
        class_label,
        escaped_ascii(class_name)
    ));
    out.push_str(&format!(
        ".globl {0}\n{0}:\n    .ascii \"{1}\"\n",
        interface_label,
        escaped_ascii(interface_name)
    ));
    entries.push((
        class_label,
        class_name.len(),
        interface_label,
        interface_name.len(),
    ));
    *index += 1;
}

/// Emits class-like/trait-name rows consumed by eval `class_uses()` metadata probes.
fn emit_eval_reflection_class_trait_lookup_data(
    out: &mut String,
    sorted_classes: &[(&String, &ClassInfo)],
    declared_trait_uses: &HashMap<String, Vec<String>>,
) {
    let mut entries = Vec::new();
    let mut index = 0usize;
    for (class_name, class_info) in sorted_classes {
        for trait_name in &class_info.used_traits {
            push_eval_reflection_class_trait_row(
                out,
                &mut entries,
                &mut index,
                class_name,
                trait_name,
            );
        }
    }

    let mut sorted_traits: Vec<&String> = declared_trait_uses.keys().collect();
    sorted_traits.sort();
    for trait_name in sorted_traits {
        if let Some(used_traits) = declared_trait_uses.get(trait_name) {
            for used_trait in used_traits {
                push_eval_reflection_class_trait_row(
                    out,
                    &mut entries,
                    &mut index,
                    trait_name,
                    used_trait,
                );
            }
        }
    }

    out.push_str(".p2align 3\n");
    out.push_str(".globl _eval_reflection_class_trait_count\n_eval_reflection_class_trait_count:\n");
    out.push_str(&format!("    .quad {}\n", entries.len()));
    out.push_str(".globl _eval_reflection_class_traits\n_eval_reflection_class_traits:\n");
    for (class_label, class_len, trait_label, trait_len) in entries {
        out.push_str(&format!("    .quad {}\n", class_label));
        out.push_str(&format!("    .quad {}\n", class_len));
        out.push_str(&format!("    .quad {}\n", trait_label));
        out.push_str(&format!("    .quad {}\n", trait_len));
    }
}

/// Adds one class-like/trait-name row and its backing string labels.
fn push_eval_reflection_class_trait_row(
    out: &mut String,
    entries: &mut Vec<(String, usize, String, usize)>,
    index: &mut usize,
    class_name: &str,
    trait_name: &str,
) {
    let class_label = format!("_eval_reflection_class_trait_class_{}", *index);
    let trait_label = format!("_eval_reflection_class_trait_name_{}", *index);
    out.push_str(&format!(
        ".globl {0}\n{0}:\n    .ascii \"{1}\"\n",
        class_label,
        escaped_ascii(class_name)
    ));
    out.push_str(&format!(
        ".globl {0}\n{0}:\n    .ascii \"{1}\"\n",
        trait_label,
        escaped_ascii(trait_name)
    ));
    entries.push((class_label, class_name.len(), trait_label, trait_name.len()));
    *index += 1;
}

/// Emits class/alias and class/source rows consumed by eval `getTraitAliases()`.
fn emit_eval_reflection_class_trait_alias_lookup_data(
    out: &mut String,
    sorted_classes: &[(&String, &ClassInfo)],
) {
    let mut alias_entries = Vec::new();
    let mut source_entries = Vec::new();
    let mut index = 0usize;
    for (class_name, class_info) in sorted_classes {
        for (alias, source) in &class_info.trait_aliases {
            push_eval_reflection_class_trait_alias_row(
                out,
                &mut alias_entries,
                &mut source_entries,
                &mut index,
                class_name,
                alias,
                source,
            );
        }
    }

    out.push_str(".p2align 3\n");
    out.push_str(
        ".globl _eval_reflection_class_trait_alias_count\n_eval_reflection_class_trait_alias_count:\n",
    );
    out.push_str(&format!("    .quad {}\n", alias_entries.len()));
    out.push_str(".globl _eval_reflection_class_trait_aliases\n_eval_reflection_class_trait_aliases:\n");
    for (class_label, class_len, alias_label, alias_len) in alias_entries {
        out.push_str(&format!("    .quad {}\n", class_label));
        out.push_str(&format!("    .quad {}\n", class_len));
        out.push_str(&format!("    .quad {}\n", alias_label));
        out.push_str(&format!("    .quad {}\n", alias_len));
    }
    out.push_str(
        ".globl _eval_reflection_class_trait_alias_sources\n_eval_reflection_class_trait_alias_sources:\n",
    );
    for (class_label, class_len, source_label, source_len) in source_entries {
        out.push_str(&format!("    .quad {}\n", class_label));
        out.push_str(&format!("    .quad {}\n", class_len));
        out.push_str(&format!("    .quad {}\n", source_label));
        out.push_str(&format!("    .quad {}\n", source_len));
    }
}

/// Adds one class/trait-alias row and its backing string labels.
fn push_eval_reflection_class_trait_alias_row(
    out: &mut String,
    alias_entries: &mut Vec<(String, usize, String, usize)>,
    source_entries: &mut Vec<(String, usize, String, usize)>,
    index: &mut usize,
    class_name: &str,
    alias: &str,
    source: &str,
) {
    let class_label = format!("_eval_reflection_class_trait_alias_class_{}", *index);
    let alias_label = format!("_eval_reflection_class_trait_alias_name_{}", *index);
    let source_label = format!("_eval_reflection_class_trait_alias_source_{}", *index);
    out.push_str(&format!(
        ".globl {0}\n{0}:\n    .ascii \"{1}\"\n",
        class_label,
        escaped_ascii(class_name)
    ));
    out.push_str(&format!(
        ".globl {0}\n{0}:\n    .ascii \"{1}\"\n",
        alias_label,
        escaped_ascii(alias)
    ));
    out.push_str(&format!(
        ".globl {0}\n{0}:\n    .ascii \"{1}\"\n",
        source_label,
        escaped_ascii(source)
    ));
    alias_entries.push((class_label.clone(), class_name.len(), alias_label, alias.len()));
    source_entries.push((class_label, class_name.len(), source_label, source.len()));
    *index += 1;
}

/// Returns direct and inherited parent interface names for one generated interface.
fn eval_reflection_interface_parent_names(
    interface_name: &str,
    interfaces: &HashMap<String, InterfaceInfo>,
) -> Vec<String> {
    let mut names = Vec::new();
    collect_eval_reflection_interface_parent_names(interface_name, interfaces, &mut names);
    names
}

/// Recursively appends interface parents without duplicating case-insensitive names.
fn collect_eval_reflection_interface_parent_names(
    interface_name: &str,
    interfaces: &HashMap<String, InterfaceInfo>,
    names: &mut Vec<String>,
) {
    let Some((_, interface_info)) = eval_reflection_interface_entry(interface_name, interfaces)
    else {
        return;
    };
    for parent in &interface_info.parents {
        let parent_name = eval_reflection_interface_entry(parent, interfaces)
            .map(|(name, _)| name.to_string())
            .unwrap_or_else(|| parent.clone());
        if names
            .iter()
            .any(|name| php_symbol_key(name) == php_symbol_key(&parent_name))
        {
            continue;
        }
        names.push(parent_name.clone());
        collect_eval_reflection_interface_parent_names(&parent_name, interfaces, names);
    }
}

/// Returns the canonical generated interface entry for a possibly case-varied name.
fn eval_reflection_interface_entry<'a>(
    interface_name: &str,
    interfaces: &'a HashMap<String, InterfaceInfo>,
) -> Option<(&'a str, &'a InterfaceInfo)> {
    if let Some((name, info)) = interfaces.get_key_value(interface_name) {
        return Some((name.as_str(), info));
    }
    interfaces
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(interface_name))
        .map(|(name, info)| (name.as_str(), info))
}

/// Returns eval ReflectionProperty bitflags for one instance property slot.
fn eval_reflection_instance_property_flags(
    class_info: &ClassInfo,
    slot: usize,
    property_name: &str,
) -> u64 {
    let visibility = class_info
        .property_visibilities
        .get(property_name)
        .unwrap_or(&Visibility::Public);
    let mut flags = eval_reflection_visibility_flags(visibility);
    if class_info.final_properties.contains(property_name) {
        flags |= EVAL_REFLECTION_PROPERTY_FLAG_FINAL;
    }
    if class_info.abstract_properties.contains(property_name) {
        flags |= EVAL_REFLECTION_PROPERTY_FLAG_ABSTRACT;
    }
    if class_info.readonly_properties.contains(property_name) {
        flags |= EVAL_REFLECTION_PROPERTY_FLAG_READONLY;
    }
    if class_info.promoted_properties.contains(property_name) {
        flags |= EVAL_REFLECTION_PROPERTY_FLAG_PROMOTED;
    }
    match class_info.property_set_visibilities.get(property_name) {
        Some(Visibility::Protected) => flags |= EVAL_REFLECTION_PROPERTY_FLAG_PROTECTED_SET,
        Some(Visibility::Private) => flags |= EVAL_REFLECTION_PROPERTY_FLAG_PRIVATE_SET,
        Some(Visibility::Public) | None => {}
    }
    if class_info.defaults.get(slot).is_some_and(Option::is_some) {
        flags |= EVAL_REFLECTION_PROPERTY_FLAG_HAS_DEFAULT_VALUE;
    }
    flags
}

/// Returns eval ReflectionProperty bitflags for one static property slot.
fn eval_reflection_static_property_flags(
    class_info: &ClassInfo,
    slot: usize,
    property_name: &str,
) -> u64 {
    let visibility = class_info
        .static_property_visibilities
        .get(property_name)
        .unwrap_or(&Visibility::Public);
    let mut flags =
        EVAL_REFLECTION_PROPERTY_FLAG_STATIC | eval_reflection_visibility_flags(visibility);
    if class_info.final_static_properties.contains(property_name) {
        flags |= EVAL_REFLECTION_PROPERTY_FLAG_FINAL;
    }
    if class_info
        .static_defaults
        .get(slot)
        .is_some_and(Option::is_some)
    {
        flags |= EVAL_REFLECTION_PROPERTY_FLAG_HAS_DEFAULT_VALUE;
    }
    flags
}

/// Converts a property visibility into eval ReflectionProperty bitflags.
fn eval_reflection_visibility_flags(visibility: &Visibility) -> u64 {
    match visibility {
        Visibility::Public => EVAL_REFLECTION_PROPERTY_FLAG_PUBLIC,
        Visibility::Protected => EVAL_REFLECTION_PROPERTY_FLAG_PROTECTED,
        Visibility::Private => EVAL_REFLECTION_PROPERTY_FLAG_PRIVATE,
    }
}

/// Emits the callable-function name table and pointer table for user-defined functions.
/// Each function name is emitted as an ASCII label; the pointer table references
/// either the active variant symbol for polymorphic functions or zero.
fn emit_callable_function_data(
    out: &mut String,
    functions: &HashMap<String, FunctionSig>,
    function_variant_groups: &HashSet<String>,
) {
    let mut sorted_functions: Vec<&String> = functions.keys().collect();
    sorted_functions.sort();
    for (idx, name) in sorted_functions.iter().enumerate() {
        out.push_str(&format!(
            ".globl _callable_user_fn_name_{0}\n_callable_user_fn_name_{0}:\n    .ascii \"{1}\"\n",
            idx,
            escaped_ascii(name)
        ));
    }
    out.push_str(".p2align 3\n");
    out.push_str(".globl _callable_user_function_count\n_callable_user_function_count:\n");
    out.push_str(&format!("    .quad {}\n", sorted_functions.len()));
    out.push_str(".globl _callable_user_function_table\n_callable_user_function_table:\n");
    for (idx, name) in sorted_functions.iter().enumerate() {
        out.push_str(&format!("    .quad _callable_user_fn_name_{}\n", idx));
        out.push_str(&format!("    .quad {}\n", name.len()));
        if function_variant_groups.contains(name.as_str()) {
            out.push_str(&format!(
                "    .quad {}\n",
                function_variant_active_symbol(name)
            ));
        } else {
            out.push_str("    .quad 0\n");
        }
    }
}

/// Emits the `_classes_by_name` lookup table used by `__rt_new_by_name`
/// for `new $variable()` dynamic instantiation (Phase 10 user-wrapper
/// dispatch). Each registered class contributes a 32-byte entry:
///
///   [0..8)   name_ptr   — pointer to the class-name ASCII bytes
///   [8..16)  name_len   — count of name bytes
///   [16..24) class_id   — runtime class id (matches the static
///                         `class_info.class_id` stamped by
///                         `__rt_heap_alloc` callers)
///   [24..32) obj_size   — `8 + num_props*16 + dyn_props_slot`, the same
///                         allocation size emit_new_object_core uses
///
/// The accompanying `_classes_by_name_count` symbol holds the entry count
/// so the runtime helper can bound its linear scan.
fn emit_classes_by_name_table(
    out: &mut String,
    sorted_classes: &[(&String, &ClassInfo)],
) {
    for (class_name, class_info) in sorted_classes {
        out.push_str(&format!(
            ".globl _class_by_name_str_{0}\n_class_by_name_str_{0}:\n    .ascii \"{1}\"\n",
            class_info.class_id,
            escaped_ascii(class_name)
        ));
    }
    out.push_str(".p2align 3\n");
    out.push_str(".globl _classes_by_name_count\n_classes_by_name_count:\n");
    out.push_str(&format!("    .quad {}\n", sorted_classes.len()));
    out.push_str(".globl _classes_by_name\n_classes_by_name:\n");
    for (class_name, class_info) in sorted_classes {
        let obj_size = class_object_payload_size(class_name, class_info);
        out.push_str(&format!(
            "    .quad _class_by_name_str_{}\n",
            class_info.class_id
        ));
        out.push_str(&format!("    .quad {}\n", class_name.len()));
        out.push_str(&format!("    .quad {}\n", class_info.class_id));
        out.push_str(&format!("    .quad {}\n", obj_size));
    }
}

/// Returns the PHP object payload bytes required by one class layout.
fn class_object_payload_size(class_name: &str, class_info: &ClassInfo) -> usize {
    let dyn_props_slot = if class_uses_dynamic_property_tail(class_name, class_info) {
        8
    } else {
        0
    };
    8 + class_info.properties.len() * 16 + dyn_props_slot
}

/// Returns whether this class layout stores a dynamic-property hash tail.
fn class_uses_dynamic_property_tail(class_name: &str, class_info: &ClassInfo) -> bool {
    class_name == "stdClass" || class_info.allow_dynamic_properties
}

/// The number of fixed-slot stream-wrapper methods recorded per class in
/// `_user_wrapper_vtable_<class_id>`. Slot order matches the runtime fopen
/// dispatch (Phase 10): 0 stream_open, 1 stream_close, 2 stream_read,
/// 3 stream_write, 4 stream_eof, 5 stream_tell, 6 stream_seek, 7 stream_flush,
/// 8 stream_stat (fd-based `fstat()` on an open wrapper stream), 9 url_stat
/// (path-based `file_exists()`/`is_file()`/`filesize()` on a `scheme://` URL).
/// G1 reserves the full PHP `StreamWrapper` surface so slot indices stay stable
/// as the dispatch is filled in: 10 stream_cast, 11 stream_lock (`flock()`),
/// 12 stream_truncate (`ftruncate()`), 13 stream_set_option, 14 stream_metadata,
/// 15 unlink, 16 rename, 17 mkdir, 18 rmdir, 19 dir_opendir, 20 dir_readdir,
/// 21 dir_closedir, 22 dir_rewinddir. Slots whose dispatch is not yet wired are
/// still emitted (zero when the class does not declare the method); the runtime
/// only reaches a slot when the corresponding builtin routes to it.
/// Each method slot is either a method-symbol pointer (when the class declares
/// the method publicly) or zero. Slot 23 stores the byte offset of a `mixed`
/// `context` property for PHP's user-wrapper context injection. The stat
/// methods must be declared WITHOUT a return type (or `: mixed`) so their
/// associative stat array round-trips as a boxed Mixed cell — a `: array`
/// return is integer-keyed and rejects the string keys (`size`, `mode`, ...)
/// PHP stat arrays use.
pub(crate) const USER_WRAPPER_VTABLE_SLOTS: usize = 23;

/// Byte offset of the boxed-result mask that follows the method pointers in every
/// `_user_wrapper_vtable_<class_id>`. Single authority for the layout: the emitter
/// writes the quad at this position and the runtime helpers read it from here.
pub(crate) const USER_WRAPPER_VTABLE_BOXED_MASK_OFFSET: usize = USER_WRAPPER_VTABLE_SLOTS * 8;

/// Byte offset of the `$context` property quad, which follows the boxed-result mask.
///
/// Its own quad, not a second reading of the mask's. Both branches of the origin/main merge
/// appended one quad here — this one the context offset, upstream the mask — and the merge kept
/// the mask while `fopen` went on reading the address as an offset. A zero mask then read as
/// "undeclared" and deprecated a property the class declares; a non-zero one read as an offset
/// and stored the boxed context that many bytes into the object.
///
/// The value is the offset PLUS ONE, with zero meaning undeclared, because a declared
/// `$context` may legitimately live at offset zero.
pub(crate) const USER_WRAPPER_VTABLE_CONTEXT_OFFSET: usize = USER_WRAPPER_VTABLE_SLOTS * 8 + 8;

/// Byte offset of the class's `__construct` pointer, which follows the `$context` quad. Zero
/// when the class declares no constructor.
///
/// php constructs a wrapper BEFORE it assigns `$context` and before `stream_open()` — measured,
/// `construct: context=NULL` then `open: tag=built`. A wrapper that prepares its state in the
/// constructor is therefore ready by the time php uses it, and elephc started it empty.
///
/// It is a trailing quad rather than a 24th method slot because `USER_WRAPPER_METHOD_NAMES`
/// doubles as the "does this class look like a stream wrapper" test: adding `__construct` there
/// would make every class with a constructor look like one.
pub(crate) const USER_WRAPPER_VTABLE_CTOR_OFFSET: usize = USER_WRAPPER_VTABLE_SLOTS * 8 + 16;

/// The number of fixed-slot stream-filter methods recorded per class in
/// `_user_filter_vtable_<class_id>` (Phase 10 tier 3). Slot order:
/// 0 filter, 1 onCreate, 2 onClose. Slot 3 is a non-method "arity" flag:
/// 0 = elephc-simplified `filter(string $data): string`, 1 = PHP-canonical
/// `filter($in, $out, &$consumed, $closing): int` with bucket brigades.
/// Slot 4 is a non-method byte offset for `php_user_filter::$params`, or zero
/// when the class has no statically declared params property.
/// The flag is read by the runtime dispatcher to choose which code path
/// to invoke. Adding the flag inline in the vtable lets the dispatcher
/// branch with a single load + cmp.
pub(crate) const USER_FILTER_VTABLE_SLOTS: usize = 7;

/// Returns true when a method key belongs to the fixed-ABI stream-wrapper
/// vtable surface dispatched by the runtime with raw arguments.
pub(crate) fn is_user_wrapper_contract_method(method_key: &str) -> bool {
    USER_WRAPPER_METHOD_NAMES.contains(&method_key)
}

/// Returns true when a method key belongs to the fixed-ABI user-filter
/// vtable surface dispatched by the runtime with raw arguments.
pub(crate) fn is_user_filter_contract_method(method_key: &str) -> bool {
    USER_FILTER_METHOD_NAMES.contains(&method_key)
}

/// Returns true when declaring `method_key` is by itself enough to identify a class as a wrapper.
///
/// This is the marker deciding whether the fixed raw-argument ABI applies to a class at all, and
/// every gate asking "is this a wrapper?" must ask it here — the checker's contract seeding and the
/// EIR normalizer have to agree, or one hands the body a boxed Mixed while the other hands the
/// dispatcher a (ptr,len) pair.
///
/// The split follows how php-src can REACH each hook:
///
/// - PATH hooks (below) are dispatched straight off a `scheme://` URL — `chmod()`/`touch()` reach
///   `stream_metadata()`, `stat()` reaches `url_stat()`, `opendir()` reaches `dir_opendir()` — with
///   no stream ever opened. A class declaring one is a wrapper on that evidence alone, and must be,
///   because nothing else about it says so.
/// - STREAM-INSTANCE hooks (`stream_read`, `stream_write`, `stream_eof`, …) are reachable only
///   through an OPEN stream, and `php_stream_open_wrapper` refuses a wrapper without `stream_open`
///   (measured: `fopen()` on such a class returns false without calling anything). So they mark a
///   wrapper only alongside `stream_open` — otherwise an unrelated `Codec::stream_write($d)` would
///   be forced onto an ABI PHP could never invoke.
/// - GENERIC names (`unlink`/`rename`/`mkdir`/`rmdir`) are ordinary method names on ordinary
///   classes (`Filesystem::mkdir($path, $mode)`), so they never mark; they take the wrapper
///   contract only when the class also declares `stream_open` or a path hook.
pub(crate) fn is_user_wrapper_marker_method(method_key: &str) -> bool {
    method_key == "stream_open" || USER_WRAPPER_PATH_METHOD_NAMES.contains(&method_key)
}

/// The wrapper hooks php-src dispatches from a URL alone, with no stream opened.
///
/// Their names are reserved by the protocol, so declaring one identifies a wrapper by itself.
const USER_WRAPPER_PATH_METHOD_NAMES: [&str; 6] = [
    "stream_metadata",
    "url_stat",
    "dir_opendir",
    "dir_readdir",
    "dir_closedir",
    "dir_rewinddir",
];

const USER_FILTER_METHOD_NAMES: [&str; 3] = [
    "filter",
    "oncreate",
    "onclose",
];

/// Vtable slots whose runtime helper reads the method's result as a RAW STRING PAIR
/// (pointer + length in the string-result registers) rather than as a boxed Mixed.
///
/// A wrapper method is called through its own ABI, and that ABI follows its return
/// type: `: string` returns the pair, while the union the PHP manual declares —
/// `stream_read(): string|false`, `dir_readdir(): string|false` — has codegen
/// representation `Mixed` and returns a single boxed pointer instead. The helper read
/// the pair registers either way, so the manual's signature yielded the right length
/// and the wrong bytes, silently. `user_wrapper_boxed_result_mask` records which slots
/// need the conversion so the helper can do it.
///
/// The stat slots are deliberately absent: they already expect a boxed Mixed (see the
/// note on `USER_WRAPPER_VTABLE_SLOTS`), so a union return is the shape they want.
const USER_WRAPPER_STRING_RESULT_SLOTS: [usize; 2] = [2, 20];

/// Slots whose helper reads a raw integer or boolean out of the result register.
///
/// These need the same treatment as the string slots for the opposite reason: real
/// wrapper code does NOT annotate `stream_tell(): int`, so the undeclared form —
/// which returns a boxed cell — is the common one, and reading that register as a
/// raw integer answered a pointer.
///
/// `dir_closedir` and `dir_rewinddir` are absent because nothing reads their result;
/// `stream_stat`/`url_stat` already expect a boxed Mixed and `stream_cast` normalizes
/// both shapes itself, so converting any of those would break a correct result.
const USER_WRAPPER_SCALAR_RESULT_SLOTS: [usize; 13] =
    [0, 3, 4, 5, 6, 7, 11, 12, 15, 16, 17, 18, 19];


/// Returns the bitmask stored after the method pointers, where bit `i` marks a slot
/// whose method returns a boxed Mixed although its helper expects the raw string pair.
///
/// The mask is APPENDED to the vtable rather than kept in a table of its own: every
/// existing slot offset stays where it is, no second dense per-class-id pointer table
/// is emitted, and a helper that does not care never loads it.
fn user_wrapper_boxed_result_mask(class_info: &ClassInfo) -> u64 {
    let mut mask = 0u64;
    let slots = USER_WRAPPER_STRING_RESULT_SLOTS
        .iter()
        .chain(USER_WRAPPER_SCALAR_RESULT_SLOTS.iter())
        .copied();
    for slot in slots {
        let method_name = USER_WRAPPER_METHOD_NAMES[slot];
        let returns_boxed = class_info
            .methods
            .get(method_name)
            .is_some_and(|sig| matches!(sig.return_type.codegen_repr(), PhpType::Mixed));
        if returns_boxed {
            mask |= 1 << slot;
        }
    }
    mask
}

const USER_WRAPPER_METHOD_NAMES: [&str; USER_WRAPPER_VTABLE_SLOTS] = [
    "stream_open",
    "stream_close",
    "stream_read",
    "stream_write",
    "stream_eof",
    "stream_tell",
    "stream_seek",
    "stream_flush",
    "stream_stat",
    "url_stat",
    "stream_cast",
    "stream_lock",
    "stream_truncate",
    "stream_set_option",
    "stream_metadata",
    "unlink",
    "rename",
    "mkdir",
    "rmdir",
    "dir_opendir",
    "dir_readdir",
    "dir_closedir",
    "dir_rewinddir",
];

/// Returns true when a class publishes at least one of the eight
/// stream-wrapper methods publicly — i.e. when it is plausibly a stream
/// wrapper. Classes that miss this filter share `_user_wrapper_vtable_missing`
/// (all zeros) instead of emitting their own all-zero table.
fn class_has_user_wrapper_method(class_info: &ClassInfo) -> bool {
    USER_WRAPPER_METHOD_NAMES.iter().any(|method_name| {
        let is_public = class_info
            .method_visibilities
            .get(*method_name)
            .is_some_and(|visibility| matches!(visibility, Visibility::Public));
        let has_impl = class_info.method_impl_classes.contains_key(*method_name);
        is_public && has_impl
    })
}

/// Returns true when a class publishes at least one of the three
/// stream-filter methods publicly (filter / onCreate / onClose). Classes
/// that miss this filter share `_user_filter_vtable_missing` instead of
/// emitting their own all-zero table.
fn class_has_user_filter_method(class_info: &ClassInfo) -> bool {
    USER_FILTER_METHOD_NAMES.iter().any(|method_name| {
        let is_public = class_info
            .method_visibilities
            .get(*method_name)
            .is_some_and(|visibility| matches!(visibility, Visibility::Public));
        let has_impl = class_info.method_impl_classes.contains_key(*method_name);
        is_public && has_impl
    })
}

/// Emits runtime metadata for user filter vtable.
fn emit_user_filter_vtable(out: &mut String, class_info: &ClassInfo) {
    if !class_has_user_filter_method(class_info) {
        return;
    }
    out.push_str("    .p2align 3\n");
    out.push_str(&format!(
        ".globl _user_filter_vtable_{0}\n_user_filter_vtable_{0}:\n",
        class_info.class_id
    ));
    for method_name in &USER_FILTER_METHOD_NAMES {
        let is_public = class_info
            .method_visibilities
            .get(*method_name)
            .is_some_and(|visibility| matches!(visibility, Visibility::Public));
        let impl_class = class_info.method_impl_classes.get(*method_name);
        if is_public && impl_class.is_some() {
            out.push_str(&format!(
                "    .quad {}\n",
                method_symbol(impl_class.unwrap(), method_name)
            ));
        } else {
            out.push_str("    .quad 0\n");
        }
    }
    // -- slot 3: filter()-arity flag (0 = 1-arg string contract, 1 = 4-arg brigade)
    // The arity is detected by counting the visible parameters of filter() when
    // it lives on this class. 4 params → PHP-canonical
    // filter($in, $out, &$consumed, $closing): int. Anything else → 1-arg.
    let brigade_arity = class_info
        .methods
        .get("filter")
        .map(|sig| sig.params.len() == 4)
        .unwrap_or(false);
    out.push_str(&format!("    .quad {}\n", if brigade_arity { 1 } else { 0 }));
    let params_offset = class_info
        .property_offsets
        .get("params")
        .copied()
        .unwrap_or(0);
    out.push_str(&format!("    .quad {}\n", params_offset));
    // -- slot 5: the `filtername` property's byte offset.
    // php seeds `$this->filtername` with the name the filter was ATTACHED under, before
    // `onCreate()` runs — the same class registered twice reports each name in turn. Without the
    // offset here the property stayed null, so a filter that branches on its own name could not.
    let filtername_offset = class_info
        .property_offsets
        .get("filtername")
        .copied()
        .unwrap_or(0);
    out.push_str(&format!("    .quad {}\n", filtername_offset));
    // -- slot 6: the `stream` property's byte offset.
    // php publishes `$this->stream` for the DURATION of each `filter()` call and nowhere else:
    // measured on `php -n` 8.5.6 it is UNSET inside `onCreate()`, a live resource inside
    // `filter()`, and NULL again inside `onClose()`. Without the offset here it stayed null
    // throughout, so a filter could not reach the stream it was filtering.
    let stream_offset = class_info
        .property_offsets
        .get("stream")
        .copied()
        .unwrap_or(0);
    out.push_str(&format!("    .quad {}\n", stream_offset));
}

/// Emits runtime metadata for user wrapper vtable.
fn emit_user_wrapper_vtable(out: &mut String, class_info: &ClassInfo) {
    if !class_has_user_wrapper_method(class_info) {
        return;
    }
    out.push_str("    .p2align 3\n");
    out.push_str(&format!(
        ".globl _user_wrapper_vtable_{0}\n_user_wrapper_vtable_{0}:\n",
        class_info.class_id
    ));
    for method_name in &USER_WRAPPER_METHOD_NAMES {
        let is_public = class_info
            .method_visibilities
            .get(*method_name)
            .is_some_and(|visibility| matches!(visibility, Visibility::Public));
        let impl_class = class_info.method_impl_classes.get(*method_name);
        if is_public && impl_class.is_some() {
            out.push_str(&format!(
                "    .quad {}\n",
                method_symbol(impl_class.unwrap(), method_name)
            ));
        } else {
            out.push_str("    .quad 0\n");
        }
    }
    // Two trailing quads after the method pointers: the boxed-result mask, then the byte offset
    // of a declared `public $context;` PLUS ONE — zero meaning the class declares none, which is
    // the case php deprecates as an invented property.
    out.push_str(&format!(
        "    .quad {}\n",
        user_wrapper_boxed_result_mask(class_info)
    ));
    let context_offset = class_info
        .property_offsets
        .get("context")
        .copied()
        .map_or(0, |offset| offset + 1);
    out.push_str(&format!("    .quad {}\n", context_offset));
    // A third: the constructor php runs before it asks the wrapper anything, or 0.
    match class_info.method_impl_classes.get("__construct") {
        Some(impl_class) => out.push_str(&format!(
            "    .quad {}\n",
            method_symbol(impl_class, "__construct")
        )),
        None => out.push_str("    .quad 0\n"),
    }
}

/// Emits the per-class callable-method name table and count for __invoke support.
/// Only public instance methods are included. Each method name is emitted as an
/// ASCII label; the table is indexed by class_id at runtime.
fn emit_class_callable_methods(out: &mut String, class_info: &ClassInfo) {
    let mut public_methods: Vec<&String> = class_info
        .methods
        .keys()
        .filter(|method_name| {
            class_info
                .method_visibilities
                .get(*method_name)
                .is_some_and(|visibility| matches!(visibility, Visibility::Public))
        })
        .collect();
    public_methods.sort();
    for method_name in &public_methods {
        out.push_str(&format!(
            ".globl _class_callable_method_name_{0}_{1}\n_class_callable_method_name_{0}_{1}:\n    .ascii \"{2}\"\n",
            class_info.class_id,
            mangle_fqn(method_name),
            escaped_ascii(method_name)
        ));
    }
    out.push_str(".p2align 3\n");
    out.push_str(&format!(
        ".globl _class_callable_methods_{0}\n_class_callable_methods_{0}:\n",
        class_info.class_id
    ));
    out.push_str(&format!("    .quad {}\n", public_methods.len()));
    for method_name in public_methods {
        out.push_str(&format!(
            "    .quad _class_callable_method_name_{}_{}\n",
            class_info.class_id,
            mangle_fqn(method_name)
        ));
        out.push_str(&format!("    .quad {}\n", method_name.len()));
    }
}

/// Emits the static-callable method table for ReflectionMethod support on static methods.
/// For each class with public static methods, emits class-name and method-name labels,
/// then builds an entries table of (class_name_ptr, class_name_len, method_name_ptr, method_name_len).
fn emit_static_callable_method_data(out: &mut String, sorted_classes: &[(&String, &ClassInfo)]) {
    let mut entries = Vec::new();
    for (class_name, class_info) in sorted_classes {
        let mut public_static_methods: Vec<&String> = class_info
            .static_methods
            .keys()
            .filter(|method_name| {
                class_info
                    .static_method_visibilities
                    .get(*method_name)
                    .is_some_and(|visibility| matches!(visibility, Visibility::Public))
            })
            .collect();
        public_static_methods.sort();
        if public_static_methods.is_empty() {
            continue;
        }

        out.push_str(&format!(
            ".globl _class_callable_static_class_name_{0}\n_class_callable_static_class_name_{0}:\n    .ascii \"{1}\"\n",
            class_info.class_id,
            escaped_ascii(class_name)
        ));
        for method_name in public_static_methods {
            out.push_str(&format!(
                ".globl _class_callable_static_method_name_{0}_{1}\n_class_callable_static_method_name_{0}_{1}:\n    .ascii \"{2}\"\n",
                class_info.class_id,
                mangle_fqn(method_name),
                escaped_ascii(method_name)
            ));
            entries.push((class_info.class_id, class_name.as_str(), method_name.as_str()));
        }
    }

    out.push_str(".p2align 3\n");
    out.push_str(".globl _class_callable_static_method_count\n_class_callable_static_method_count:\n");
    out.push_str(&format!("    .quad {}\n", entries.len()));
    out.push_str(".globl _class_callable_static_method_table\n_class_callable_static_method_table:\n");
    for (class_id, class_name, method_name) in entries {
        out.push_str(&format!(
            "    .quad _class_callable_static_class_name_{}\n",
            class_id
        ));
        out.push_str(&format!("    .quad {}\n", class_name.len()));
        out.push_str(&format!(
            "    .quad _class_callable_static_method_name_{}_{}\n",
            class_id,
            mangle_fqn(method_name)
        ));
        out.push_str(&format!("    .quad {}\n", method_name.len()));
    }
}

/// Returns the symbol name to use for an interface method table entry.
/// Returns a wrapper symbol when the interface declares a Mixed return type but the
/// implementing class uses a narrower type (the wrapper bridges the type mismatch).
fn interface_method_table_symbol(
    class_info: &ClassInfo,
    interface_info: &InterfaceInfo,
    method_name: &str,
    impl_class: &str,
    classes: &HashMap<String, ClassInfo>,
) -> String {
    if interface_method_needs_return_wrapper(interface_info, method_name, impl_class, classes) {
        interface_method_wrapper_symbol(
            class_info.class_id,
            interface_info.interface_id,
            method_name,
        )
    } else {
        method_symbol(impl_class, method_name)
    }
}

/// Returns true when an interface method requires a return-type wrapper at call sites.
/// A wrapper is needed when the interface declares a Mixed return type but the
/// implementing class uses a narrower type — without the wrapper, a Mixed would be
/// written where a typed value is expected.
fn interface_method_needs_return_wrapper(
    interface_info: &InterfaceInfo,
    method_name: &str,
    impl_class: &str,
    classes: &HashMap<String, ClassInfo>,
) -> bool {
    let Some(interface_sig) = interface_info.methods.get(method_name) else {
        return false;
    };
    let Some(actual_sig) = classes
        .get(impl_class)
        .and_then(|class_info| class_info.methods.get(method_name))
    else {
        return false;
    };

    matches!(interface_sig.return_type.codegen_repr(), PhpType::Mixed)
        && !matches!(actual_sig.return_type.codegen_repr(), PhpType::Mixed)
}

/// Returns a property's PHP-mangled `serialize()` key bytes: `name` for a public
/// property, `\0*\0name` for protected, and `\0DeclaringClass\0name` for private
/// (matching the keys the PHP interpreter emits inside `O:...{...}`).
fn mangled_property_name(class_info: &ClassInfo, class_name: &str, prop_name: &str) -> Vec<u8> {
    match class_info.property_visibilities.get(prop_name) {
        Some(Visibility::Protected) => {
            let mut out = vec![0u8, b'*', 0u8];
            out.extend_from_slice(prop_name.as_bytes());
            out
        }
        Some(Visibility::Private) => {
            let declaring = class_info
                .property_declaring_classes
                .get(prop_name)
                .map(String::as_str)
                .unwrap_or(class_name);
            let mut out = vec![0u8];
            out.extend_from_slice(declaring.as_bytes());
            out.push(0u8);
            out.extend_from_slice(prop_name.as_bytes());
            out
        }
        _ => prop_name.as_bytes().to_vec(),
    }
}

/// Renders the text PHP's `var_dump` places BETWEEN the `[` and `]` of a
/// property key line.
///
/// PHP annotates the key with the property's visibility: `"p"` for public,
/// `"p":protected`, and `"p":"C":private` where `C` is the DECLARING class (a
/// private property inherited into a subclass keeps the parent's name). The
/// declaring class comes from `property_declaring_classes`, the same source
/// serialize's NUL-mangling uses, so the two renderings can never disagree.
/// One rendered row of a `_class_vd_desc_*` table: everything
/// `__rt_var_dump_object` needs to print a single `["key"]=> value` pair.
///
/// A row is NOT necessarily a declared property — when the class declares a
/// foldable `__debugInfo()`, rows come from that projection instead (different
/// key text, different order, possibly fewer rows), but each row still points at
/// a real property slot so the walker reads a real value.
struct VarDumpRow {
    /// Text PHP renders between the `[` and `]`, quotes included.
    key: String,
    /// Text PHP's `print_r` renders between the `[` and `]`: the same visibility
    /// annotation as `key` but WITHOUT the double quotes (`x`, `y:protected`,
    /// `z:C:private`). Consumed by `__rt_print_r_object`.
    print_r_key: String,
    /// Bare property name, which `var_export` prints with no visibility suffix
    /// at all. Consumed by `__elephc_object_prop_name`.
    plain_key: String,
    /// Byte offset of the backing property within the object.
    offset: usize,
    /// Runtime value tag of the backing property.
    tag: u64,
    /// Declared type name `uninitialized(...)` prints.
    type_name: String,
}

/// Builds the `var_dump` rows for a class: the `__debugInfo()` projection when the
/// class declares a foldable one, otherwise every declared property in layout order.
fn var_dump_descriptor_rows(class_info: &ClassInfo, class_name: &str) -> Vec<VarDumpRow> {
    if let Some(projection) = var_dump_debug_info_projection(class_info) {
        return projection
            .into_iter()
            .filter_map(|(key, prop_name)| {
                let (layout_index, (_, prop_ty)) = class_info
                    .properties
                    .iter()
                    .enumerate()
                    .find(|(_, (name, _))| *name == prop_name)?;
                Some(VarDumpRow {
                    key: format!("\"{}\"", key),
                    // A `__debugInfo()` projection key is a plain array key, so PHP
                    // never annotates it with a visibility: print_r and var_export
                    // both print the projected key verbatim.
                    print_r_key: key.clone(),
                    plain_key: key,
                    offset: class_info
                        .property_offsets
                        .get(&prop_name)
                        .copied()
                        .unwrap_or(8 + layout_index * 16),
                    tag: prop_value_tag(class_info, &prop_name, prop_ty),
                    type_name: var_dump_property_type_name(prop_ty),
                })
            })
            .collect();
    }
    class_info
        .properties
        .iter()
        .enumerate()
        .map(|(layout_index, (prop_name, prop_ty))| VarDumpRow {
            key: var_dump_property_key(class_info, class_name, prop_name),
            print_r_key: print_r_property_key(class_info, class_name, prop_name),
            plain_key: prop_name.clone(),
            offset: class_info
                .property_offsets
                .get(prop_name)
                .copied()
                .unwrap_or(8 + layout_index * 16),
            tag: prop_value_tag(class_info, prop_name, prop_ty),
            type_name: var_dump_property_type_name(prop_ty),
        })
        .collect()
}

/// Folds a class's `__debugInfo()` into the `(array key, property name)` pairs
/// `var_dump` should print, or `None` when the method is absent or its body is not
/// a shape this compiler can resolve statically.
///
/// WHY STATICALLY. PHP calls `__debugInfo()` at dump time and prints the returned
/// array in place of the declared properties. elephc's `var_dump` object walker is
/// hand-written assembly driven entirely by `_class_vd_desc_*`, so honouring
/// `__debugInfo` dynamically would mean emitting a PHP method call, an array walk,
/// and the matching refcount handling inside that walker on every architecture.
/// The overwhelmingly common body — PHP's own `HashContext::__debugInfo()` included
/// — is a pure projection of properties, which the descriptor can express exactly:
/// a row already names a key and points at a property slot, so a projection is just
/// a different row list. That keeps the change to this table and costs no assembly.
///
/// SUPPORTED SHAPE (must match completely, else `None`):
/// `public function __debugInfo() { return ['k1' => $this->p1, 'k2' => $this->p2]; }`
/// — a single `return` of an associative array literal whose every key is a string
/// literal and whose every value is `$this-><declared property>`. `return [];` is
/// supported and yields zero rows. Reordering and renaming come free.
///
/// DELIBERATELY UNSUPPORTED, and why `None` means "fall back" rather than "error":
/// bodies that compute values (`'n' => count($this->items)`), read another object,
/// use non-string or NUL-mangled keys (the synthetic SPL container bodies do), or
/// span several statements cannot be reduced to a property slot. Before this
/// function existed elephc ignored `__debugInfo()` for every class, so falling back
/// to the declared-property list preserves that behaviour exactly and turns no
/// currently-compiling program into an error. It is a KNOWN DIVERGENCE from PHP,
/// not parity — `tests/var_dump_object_tests.rs` pins it as such.
fn var_dump_debug_info_projection(class_info: &ClassInfo) -> Option<Vec<(String, String)>> {
    use crate::parser::ast::{ExprKind, StmtKind};

    let method = class_info
        .method_decls
        .iter()
        .find(|m| m.name.eq_ignore_ascii_case("__debugInfo"))?;
    if method.is_static || !method.has_body {
        return None;
    }
    let [only_stmt] = method.body.as_slice() else {
        return None;
    };
    let StmtKind::Return(Some(returned)) = &only_stmt.kind else {
        return None;
    };
    let pairs = match &returned.kind {
        ExprKind::ArrayLiteralAssoc(pairs) => pairs,
        // `return [];` parses as a positional literal; an empty one is a valid
        // (and exact) zero-row projection. A non-empty positional literal has
        // integer keys pointing at non-property values, so it is not foldable.
        ExprKind::ArrayLiteral(items) if items.is_empty() => return Some(Vec::new()),
        _ => return None,
    };

    let mut projection = Vec::with_capacity(pairs.len());
    for (key_expr, value_expr) in pairs {
        let ExprKind::StringLiteral(key) = &key_expr.kind else {
            return None;
        };
        // A NUL byte marks php-src's visibility mangling, which this projection
        // does not reproduce; leave such classes on the declared-property path.
        if key.contains('\0') {
            return None;
        }
        let ExprKind::PropertyAccess { object, property } = &value_expr.kind else {
            return None;
        };
        // `$this` normally parses to `ExprKind::This`; the `Variable("this")`
        // spelling is accepted too because synthetic method bodies build it that way.
        let reads_this = match &object.kind {
            ExprKind::This => true,
            ExprKind::Variable(receiver) => receiver == "this",
            _ => false,
        };
        if !reads_this {
            return None;
        }
        if !class_info
            .properties
            .iter()
            .any(|(name, _)| name == property)
        {
            return None;
        }
        projection.push((key.clone(), property.clone()));
    }
    Some(projection)
}

/// Renders the text PHP prints between the `[` and `]` of a declared property's
/// `var_dump` key line, including its visibility suffix.
fn var_dump_property_key(class_info: &ClassInfo, class_name: &str, prop_name: &str) -> String {
    match class_info.property_visibilities.get(prop_name) {
        Some(Visibility::Protected) => format!("\"{}\":protected", prop_name),
        Some(Visibility::Private) => {
            let declaring = class_info
                .property_declaring_classes
                .get(prop_name)
                .map(String::as_str)
                .unwrap_or(class_name);
            format!("\"{}\":\"{}\":private", prop_name, declaring)
        }
        _ => format!("\"{}\"", prop_name),
    }
}

/// Moves an enum's `name` row to the front of its rendered property list.
///
/// PHP prints a backed enum case as `[name] => Hearts` then `[value] => H`
/// (`print_r`), and `var_export` follows the same order. elephc lays a backed
/// enum's storage out with `value` first, so the two disagree unless the DISPLAY
/// order is fixed here. Rows carry an explicit byte offset, so reordering them is
/// purely cosmetic — every row still points at the same slot. Applied only to
/// enum classes, and a no-op when `name` is already first or absent.
fn hoist_enum_name_row(rows: &mut Vec<VarDumpRow>) {
    if let Some(index) = rows.iter().position(|row| row.plain_key == "name") {
        let row = rows.remove(index);
        rows.insert(0, row);
    }
}

/// Returns the byte offset of an enum class's `name` property slot, or `-1` when
/// the class does not declare one.
///
/// Every PHP enum case exposes a readonly `name` holding the case identifier, and
/// elephc materializes it as an ordinary declared string property — so the case
/// name `enum(E::C)`, `E Enum` bodies and `\E::C` all print is just that slot's
/// 16-byte `(ptr, len)` pair. A `-1` result makes the runtime treat the class as a
/// plain object rather than reading a slot that may not exist.
fn enum_case_name_property_offset(class_info: &ClassInfo) -> i64 {
    class_info
        .properties
        .iter()
        .enumerate()
        .find(|(_, (prop_name, _))| prop_name == "name")
        .map(|(layout_index, (prop_name, _))| {
            class_info
                .property_offsets
                .get(prop_name)
                .copied()
                .unwrap_or(8 + layout_index * 16) as i64
        })
        .unwrap_or(-1)
}

/// Renders the text PHP prints between the `[` and `]` of a declared property's
/// `print_r` key line.
///
/// `print_r` annotates visibility like `var_dump` does but WITHOUT quoting either
/// the property name or the declaring class: `x`, `y:protected`, `z:C:private`
/// (verified against PHP 8.4). The declaring class comes from the same
/// `property_declaring_classes` map `var_dump_property_key` reads, so the two
/// renderings can never name a different class for one property.
fn print_r_property_key(class_info: &ClassInfo, class_name: &str, prop_name: &str) -> String {
    match class_info.property_visibilities.get(prop_name) {
        Some(Visibility::Protected) => format!("{}:protected", prop_name),
        Some(Visibility::Private) => {
            let declaring = class_info
                .property_declaring_classes
                .get(prop_name)
                .map(String::as_str)
                .unwrap_or(class_name);
            format!("{}:{}:private", prop_name, declaring)
        }
        _ => prop_name.to_string(),
    }
}

/// Renders the declared type name PHP prints inside `uninitialized(...)` for a
/// typed property read before its first write.
///
/// PHP echoes the SOURCE type text; `ClassInfo` only retains the resolved
/// `PhpType`, so this reconstructs the canonical spelling for the shapes a
/// property declaration can actually take. Unions and intersections collapse to
/// `mixed` — a property can only be uninitialized when it is typed and
/// default-less, and the walker's `uninitialized(...)` line is the only consumer.
fn var_dump_property_type_name(prop_ty: &PhpType) -> String {
    match prop_ty {
        PhpType::Int => "int".to_string(),
        PhpType::Float => "float".to_string(),
        PhpType::Str => "string".to_string(),
        PhpType::Bool => "bool".to_string(),
        PhpType::False => "false".to_string(),
        PhpType::Array(_) | PhpType::AssocArray { .. } => "array".to_string(),
        PhpType::Iterable => "iterable".to_string(),
        PhpType::Callable => "callable".to_string(),
        PhpType::Object(class_name) if !class_name.is_empty() => class_name.clone(),
        PhpType::Object(_) => "object".to_string(),
        _ => "mixed".to_string(),
    }
}

/// Maps a declared property's static type to the runtime value tag consumed by
/// `__rt_serialize_value` when serializing that property's 16-byte object slot.
/// Mirrors the gc-descriptor tag mapping; reference and untyped/nullable
/// properties are stored as boxed `Mixed` cells (tag 7).
fn prop_value_tag(class_info: &ClassInfo, prop_name: &str, prop_ty: &PhpType) -> u64 {
    if class_info.reference_properties.contains(prop_name) {
        return 7;
    }
    match prop_ty {
        PhpType::Int => 0,
        PhpType::Str => 1,
        PhpType::Float => 2,
        PhpType::Bool | PhpType::False => 3,
        PhpType::Array(_) => 4,
        PhpType::AssocArray { .. } => 5,
        PhpType::Object(_) => 6,
        _ => 7,
    }
}

#[cfg(test)]
mod boxed_result_mask_tests {
    use std::collections::HashMap;

    use crate::types::{FunctionSig, PhpType};

    use super::{
        user_wrapper_boxed_result_mask, USER_WRAPPER_SCALAR_RESULT_SLOTS,
        USER_WRAPPER_STRING_RESULT_SLOTS,
    };

    /// Builds a signature carrying only the return type, which is all the mask reads.
    fn returning(return_type: PhpType) -> FunctionSig {
        FunctionSig {
            params: Vec::new(),
            param_type_exprs: Vec::new(),
            param_attributes: Vec::new(),
            defaults: Vec::new(),
            return_type,
            declared_return: true,
            by_ref_return: false,
            ref_params: Vec::new(),
            declared_params: Vec::new(),
            variadic: None,
            deprecation: None,
        }
    }

    /// Builds a class info whose named methods carry the given return types.
    fn class_with(methods: &[(&str, PhpType)]) -> crate::types::ClassInfo {
        let mut class_info = super::tests::empty_class_info(1, "stream_open");
        class_info.methods = HashMap::new();
        for (name, return_type) in methods {
            class_info
                .methods
                .insert((*name).to_string(), returning(return_type.clone()));
        }
        class_info
    }

    /// The mask marks exactly the slots whose method returns the boxed representation.
    ///
    /// Both directions matter and the second is the one that bites: a mask that was always
    /// set would satisfy every union test while breaking every wrapper declared `: string`,
    /// which is how all the older tests and examples are written.
    #[test]
    fn the_mask_marks_a_union_return_and_leaves_a_string_return_alone() {
        let union = PhpType::Union(vec![PhpType::Str, PhpType::False]);
        assert_eq!(
            user_wrapper_boxed_result_mask(&class_with(&[("stream_read", union.clone())])),
            1 << 2,
            "a `string|false` stream_read must select the boxed conversion"
        );
        assert_eq!(
            user_wrapper_boxed_result_mask(&class_with(&[("dir_readdir", union.clone())])),
            1 << 20,
            "a `string|false` dir_readdir must select the boxed conversion"
        );
        assert_eq!(
            user_wrapper_boxed_result_mask(&class_with(&[
                ("stream_read", PhpType::Str),
                ("dir_readdir", PhpType::Str),
            ])),
            0,
            "`: string` methods return the raw pair and must not be converted"
        );
        assert_eq!(
            user_wrapper_boxed_result_mask(&class_with(&[])),
            0,
            "a class implementing neither slot marks nothing"
        );
    }

    /// Only slots whose helper reads a raw string pair may appear in the mask.
    ///
    /// The stat slots already expect a boxed Mixed and `stream_cast` normalizes both shapes
    /// itself, so adding either here would convert a result that is already correct.
    #[test]
    fn only_the_string_pair_slots_are_convertible() {
        assert_eq!(USER_WRAPPER_STRING_RESULT_SLOTS, [2, 20]);
        for slot in USER_WRAPPER_STRING_RESULT_SLOTS {
            assert!(
                matches!(super::USER_WRAPPER_METHOD_NAMES[slot], "stream_read" | "dir_readdir"),
                "slot {slot} is not one of the string-pair methods"
            );
        }
    }

    /// The scalar slot numbers are anchored to method NAMES, not left as bare integers.
    ///
    /// They are a second authority: the runtime helpers test the mask bit for their own
    /// `VTABLE_SLOT_*` constant, so a slot list that drifts from the vtable order would
    /// convert one method's result on another method's bit — a silent miscompile rather
    /// than a build failure. Anchoring to the name makes any reordering fail here.
    #[test]
    fn every_scalar_slot_names_the_method_whose_helper_reads_a_raw_scalar() {
        const EXPECTED: [(usize, &str); 13] = [
            (0, "stream_open"),
            (3, "stream_write"),
            (4, "stream_eof"),
            (5, "stream_tell"),
            (6, "stream_seek"),
            (7, "stream_flush"),
            (11, "stream_lock"),
            (12, "stream_truncate"),
            (15, "unlink"),
            (16, "rename"),
            (17, "mkdir"),
            (18, "rmdir"),
            (19, "dir_opendir"),
        ];
        assert_eq!(
            USER_WRAPPER_SCALAR_RESULT_SLOTS.len(),
            EXPECTED.len(),
            "a slot was added or removed without updating this guard"
        );
        for (slot, name) in EXPECTED {
            assert!(
                USER_WRAPPER_SCALAR_RESULT_SLOTS.contains(&slot),
                "{name} (slot {slot}) is missing from the scalar-result mask"
            );
            assert_eq!(
                super::USER_WRAPPER_METHOD_NAMES[slot],
                name,
                "slot {slot} no longer holds {name}: the vtable order moved under the mask"
            );
        }
        for slot in USER_WRAPPER_SCALAR_RESULT_SLOTS {
            assert!(
                !USER_WRAPPER_STRING_RESULT_SLOTS.contains(&slot),
                "slot {slot} cannot be both a string-pair and a scalar result"
            );
        }
        // Reached through `__rt_user_wrapper_path_op`, whose vtable slot is a runtime
        // argument rather than a constant: the helper selects the mask bit with a
        // variable shift, so all four must be marked or one of them silently keeps
        // reading a boxed cell as a boolean.
        for slot in [15, 16, 17, 18] {
            assert!(
                USER_WRAPPER_SCALAR_RESULT_SLOTS.contains(&slot),
                "path-op slot {slot} must also be marked as a scalar result"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};

    use crate::codegen_support::platform::{Arch, Platform, Target};

    use crate::parser::ast::Visibility;
    use crate::types::{ClassInfo, PhpType};

    use super::emit_runtime_data_user;

    /// Provides the Empty class info helper used by the user module.
    pub(super) fn empty_class_info(class_id: u64, method_name: &str) -> ClassInfo {
        let mut method_impl_classes = HashMap::new();
        method_impl_classes.insert(method_name.to_string(), "Exception".to_string());

        let mut vtable_slots = HashMap::new();
        vtable_slots.insert(method_name.to_string(), 0);

        ClassInfo {
            class_id,
            declaration_span: crate::span::Span::dummy(),
            parent: None,
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            allow_dynamic_properties: false,
            constants: HashMap::new(),
    constant_deprecations: HashMap::new(),
    constant_types: HashMap::new(),
    constant_visibilities: HashMap::new(),
    final_constants: HashSet::new(),
            attribute_names: Vec::new(),
            attribute_args: Vec::new(),
            method_attribute_names: HashMap::new(),
            method_attribute_args: HashMap::new(),
            property_attribute_names: HashMap::new(),
            property_attribute_args: HashMap::new(),
            constant_attribute_names: HashMap::new(),
            constant_attribute_args: HashMap::new(),
            used_traits: Vec::new(),
            trait_aliases: Vec::new(),
            properties: Vec::new(),
            property_offsets: HashMap::new(),
            property_declaring_classes: HashMap::new(),
            defaults: Vec::new(),
            property_visibilities: HashMap::new(),
            property_set_visibilities: HashMap::new(),
            declared_properties: HashSet::new(),
            property_declared_slots: Vec::new(),
            final_properties: HashSet::new(),
            readonly_properties: HashSet::new(),
            reference_properties: HashSet::new(),
            owned_reference_properties: HashSet::new(),
            promoted_properties: HashSet::new(),
            property_reference_slots: Vec::new(),
            abstract_properties: HashSet::new(),
            abstract_property_hooks: HashMap::new(),
            static_properties: Vec::new(),
            static_defaults: Vec::new(),
            static_property_declaring_classes: HashMap::new(),
            static_property_visibilities: HashMap::new(),
            declared_static_properties: HashSet::new(),
            final_static_properties: HashSet::new(),
            method_decls: Vec::new(),
            methods: HashMap::new(),
            static_methods: HashMap::new(),
            late_static_method_returns: HashMap::new(),
            late_static_static_method_returns: HashMap::new(),
            callable_method_return_sigs: HashMap::new(),
            callable_array_method_return_sigs: HashMap::new(),
            method_visibilities: HashMap::<String, Visibility>::new(),
            final_methods: HashSet::new(),
            method_declaring_classes: HashMap::new(),
            method_impl_classes,
            vtable_methods: vec![method_name.to_string()],
            vtable_slots,
            static_method_visibilities: HashMap::new(),
            final_static_methods: HashSet::new(),
            static_method_declaring_classes: HashMap::new(),
            static_method_impl_classes: HashMap::new(),
            static_vtable_methods: Vec::new(),
            static_vtable_slots: HashMap::new(),
            interfaces: Vec::new(),
            constructor_param_to_prop: Vec::new(),
        }
    }

    /// Verifies that emit runtime data user can filter built in classes.
    #[test]
    fn test_emit_runtime_data_user_can_filter_built_in_classes() {
        let mut classes = HashMap::new();
        classes.insert(
            "Exception".to_string(),
            empty_class_info(0, "__construct"),
        );
        classes.insert(
            "UserVisible".to_string(),
            empty_class_info(1, "run"),
        );

        let mut allowed_class_names = HashSet::new();
        allowed_class_names.insert("UserVisible".to_string());

        let asm = emit_runtime_data_user(
            &HashSet::new(),
            &HashMap::new(),
            &HashMap::new(),
            &HashSet::new(),
            &HashMap::new(),
            &[],
            &[],
            &HashMap::new(),
            &HashMap::new(),
            &classes,
            &HashMap::new(),
            Some(&allowed_class_names),
            false,
            None,
            true,                                   // these fixtures declare user methods
            Target::new(Platform::MacOS, Arch::AArch64),
        );

        assert!(asm.contains("_class_vtable_1"));
        assert!(asm.contains("_method_Exception_run"));
        assert!(!asm.contains("_class_vtable_0"));
        assert!(!asm.contains("_method_Exception__construct"));
    }

    /// Verifies that emit runtime data user keeps dense class tables when ids start at one.
    ///
    /// The fixture declares ids 1..3 and leaves 0 empty, so it also pins the two parent-id
    /// sentinels apart: slot 0 is a HOLE and gets `-2` ("no metadata was emitted here", which
    /// makes `__rt_exception_matches` abort), while 1..3 are genuine roots and get `-1` ("stop
    /// walking and report no match"). Before they were distinguished, all four read `-1` and a
    /// broken ancestor chain was indistinguishable from a class that simply had no parent.
    #[test]
    fn test_emit_runtime_data_user_keeps_dense_class_tables_when_ids_start_at_one() {
        let mut classes = HashMap::new();
        classes.insert("Animal".to_string(), empty_class_info(1, "label"));
        classes.insert("Dog".to_string(), empty_class_info(2, "label"));
        classes.insert("Cat".to_string(), empty_class_info(3, "label"));

        let asm = emit_runtime_data_user(
            &HashSet::new(),
            &HashMap::new(),
            &HashMap::new(),
            &HashSet::new(),
            &HashMap::new(),
            &[],
            &[],
            &HashMap::new(),
            &HashMap::new(),
            &classes,
            &HashMap::new(),
            None,
            false,
            None,
            true,                                   // these fixtures declare user methods
            Target::new(Platform::MacOS, Arch::AArch64),
        );

        assert!(asm.contains("_class_gc_desc_count:\n    .quad 4\n"));
        assert!(asm.contains("_class_parent_ids:\n    .quad -2\n    .quad -1\n    .quad -1\n    .quad -1\n"));
        assert!(asm.contains("_class_vtable_ptrs:\n    .quad _class_vtable_missing\n    .quad _class_vtable_1\n    .quad _class_vtable_2\n    .quad _class_vtable_3\n"));
        assert!(asm.contains("_class_static_vtable_ptrs:\n    .quad _class_static_vtable_missing\n    .quad _class_static_vtable_1\n    .quad _class_static_vtable_2\n    .quad _class_static_vtable_3\n"));
    }

    /// Verifies that callable properties carry the descriptor tag required for
    /// capture-aware release during object destruction.
    #[test]
    fn test_emit_runtime_data_user_tags_callable_properties_for_gc() {
        let mut class_info = empty_class_info(1, "run");
        class_info
            .properties
            .push(("callback".to_string(), PhpType::Callable));
        class_info
            .property_offsets
            .insert("callback".to_string(), 8);

        let mut classes = HashMap::new();
        classes.insert("CallableOwner".to_string(), class_info);

        let asm = emit_runtime_data_user(
            &HashSet::new(),
            &HashMap::new(),
            &HashMap::new(),
            &HashSet::new(),
            &HashMap::new(),
            &[],
            &[],
            &HashMap::new(),
            &HashMap::new(),
            &classes,
            &HashMap::new(),
            None,
            false,
            None,
            true,                                   // these fixtures declare user methods
            Target::new(Platform::MacOS, Arch::AArch64),
        );

        assert!(asm.contains("_class_gc_desc_1:\n    .byte 10\n"));
    }
}
