//! Purpose:
//! Builds the complete, identifier-checked WebAssembly text for one EIR module.
//! The returned plan owns the exact WAT consumed by public artifact generation.
//!
//! Called from:
//! - `crate::codegen_wasm::capability::validate_module()` after its aggregate
//!   static capability audit succeeds.
//!
//! Key details:
//! - Planning performs the only EIR-to-WAT lowering pass; consumers never lower
//!   the module again or substitute placeholder runtime/data state.
//! - Duplicate or malformed function identifiers are rejected before a plan is
//!   constructed; artifact publication performs WAT assembly and binary validation.
//! - This module must not call back into `capability`, which would recurse.

use super::{
    arrays, classes, closures, float, function, hashes, heap, methods, mixed, objects, refcount,
    runtime, strict, symbols, wat, WasmError,
};
use crate::codegen::Emit;
use crate::ir::Module;

/// Fully lowered, identifier-checked WebAssembly text ready for artifact encoding.
#[derive(Debug)]
pub(super) struct LoweredWasmPlan {
    wat: String,
}

impl LoweredWasmPlan {
    /// Consumes the plan and returns its already lowered WebAssembly text.
    pub(super) fn into_wat(self) -> String {
        self.wat
    }
}

/// Lowers one EIR module exactly once and returns its identifier-checked plan.
///
/// All literal addresses, runtime tables, heap bounds, functions, methods, and
/// closure dispatch entries are derived from the real module. Any lowering or
/// structural error prevents construction of `LoweredWasmPlan`.
/// php-src's own suffix for the `ValueError` a failed `Enum::from()` raises.
///
/// Duplicated from the emitter deliberately rather than shared through a `pub` — the planner
/// lays the bytes out and the emitter asks for them by CONTENT, so a mismatch would fail the
/// build with "has no data segment" rather than emit a wrong message. Keeping the two spellings
/// beside their own use makes that failure the only way they can disagree.
const ENUM_FROM_MESSAGE_SUFFIX: &str = " is not a valid backing value for enum ";

/// The enums some `Enum::from()` call in this module names.
///
/// The label an EIR static call interns is `Enum::method`, so the receiver is read off the
/// label rather than from the instruction's type — a `from()` on a non-enum class is an
/// ordinary method with a body and needs none of this.
pub(super) fn enum_names_reached_by_from(module: &Module) -> Vec<String> {
    let mut reached: Vec<String> = Vec::new();
    for function in module
        .functions
        .iter()
        .chain(module.class_methods.iter())
        .chain(module.closures.iter())
    {
        for inst in &function.instructions {
            if inst.op != crate::ir::Op::StaticMethodCall {
                continue;
            }
            let Some(crate::ir::Immediate::Data(data)) = inst.immediate else {
                continue;
            };
            let Some(label) = module.data.strings.get(data.as_raw() as usize) else {
                continue;
            };
            let Some((receiver, method)) = label.split_once("::") else {
                continue;
            };
            if !method.eq_ignore_ascii_case("from") {
                continue;
            }
            let receiver = receiver.trim_start_matches('\\');
            if module.enum_infos.contains_key(receiver) && !reached.iter().any(|seen| seen == receiver)
            {
                reached.push(receiver.to_string());
            }
        }
    }
    reached
}

/// php's own sentence for reading a typed property that has never been written, or was `unset`.
pub(super) const UNSET_PROPERTY_MESSAGE_PREFIX: &str = "Typed property ";

/// The rest of it, after `Class::$property`.
pub(super) const UNSET_PROPERTY_MESSAGE_SUFFIX: &str =
    " must not be accessed before initialization";

/// The full message for one slot, which is a CONSTANT: both halves are compile-time names.
pub(super) fn unset_property_message(class_name: &str, property: &str) -> String {
    format!("{UNSET_PROPERTY_MESSAGE_PREFIX}{class_name}::${property}{UNSET_PROPERTY_MESSAGE_SUFFIX}")
}

/// The `(declaring class, property)` pairs some `unset($obj->$p)` in this module can reach.
///
/// Three things read this and they must agree: the slot's marker write, the `isset` probe that
/// stops folding to a constant, and the read that has to raise. A pair is keyed by the DECLARING
/// class so `unset($child->p)` and `isset($parent->p)` name the same slot — which is the same
/// normalization the layout itself uses, since a child shares the parent's slot.
///
/// A module with no `unset` of a property keeps every read folded and every probe constant, which
/// is why this is a pair list rather than a blanket flag.
pub(super) fn properties_reached_by_unset(module: &Module) -> Vec<(String, String)> {
    let mut reached: Vec<(String, String)> = Vec::new();
    for function in module
        .functions
        .iter()
        .chain(module.class_methods.iter())
        .chain(module.closures.iter())
    {
        for inst in &function.instructions {
            if inst.op != crate::ir::Op::PropUnset {
                continue;
            }
            let Some(crate::ir::Immediate::Data(data)) = inst.immediate else {
                continue;
            };
            let Some(property) = module.data.strings.get(data.as_raw() as usize) else {
                continue;
            };
            let Some(receiver) = inst
                .operands
                .first()
                .and_then(|id| function.value(*id))
                .map(|value| value.php_type.codegen_repr())
            else {
                continue;
            };
            let crate::types::PhpType::Object(class_name) = receiver else {
                continue;
            };
            let Some(declaring) = declaring_class_of_property(module, &class_name, property) else {
                continue;
            };
            let pair = (declaring, property.clone());
            if !reached.contains(&pair) {
                reached.push(pair);
            }
        }
    }
    reached
}

/// Returns the class whose layout OWNS `property`, which a child shares rather than copies.
pub(super) fn declaring_class_of_property(
    module: &Module,
    class_name: &str,
    property: &str,
) -> Option<String> {
    let class_name = class_name.trim_start_matches('\\');
    let info = module.class_infos.get(class_name)?;
    if !info.properties.iter().any(|(name, _)| name == property) {
        return None;
    }
    Some(
        info.property_declaring_classes
            .get(property)
            .cloned()
            .unwrap_or_else(|| class_name.to_string()),
    )
}

/// Plans a whole module: lays out the data segments, class tables and runtime helpers and
/// lowers every function, answering the plan the artifact writer consumes.
pub(super) fn plan_module(module: &Module, emit: Emit) -> Result<LoweredWasmPlan, WasmError> {
    let _ = emit;
    let mut wm = wat::WatModule::new();
    // The WASI imports + `__rt_*` runtime are only added for command (main-bearing)
    // modules. Importing WASI makes a runtime treat the module as a command
    // (requiring `_start`), so a reactor/library module with no main must not.
    // Import-free runtime (concat buffer + cursor) is needed by every module.
    runtime::emit_common_runtime(&mut wm);
    let has_main = module.functions.iter().any(|function| function.flags.is_main);
    if has_main {
        runtime::emit_command_runtime(&mut wm);
    }

    // PHP needs exactly one exception tag: `throw` carries the exception object's
    // heap pointer and the catching frame decides which clause matches. Declared
    // only when some function actually throws or catches, so a module without
    // exceptions carries neither the tag nor the current-exception slot.
    if super::function::module_uses_exceptions(module) {
        wm.add_tag(wat::Tag {
            name: super::function::EXCEPTION_TAG.to_string(),
            params: vec![wat::ValType::I32],
        });
        wm.add_global(wat::Global {
            name: super::function::EXCEPTION_VALUE_GLOBAL.to_string(),
            ty: wat::ValType::I32,
            mutable: true,
            init: 0,
        });
        // Set by every raise site immediately before `throw`, read by `main`'s landing pad when
        // nothing caught the exception. The initial value is the class-agnostic diagnostic, so a
        // path that somehow reaches the pad without a raise still prints a PHP fatal.
        wm.add_global(wat::Global {
            name: super::function::EXCEPTION_FATAL_CODE_GLOBAL.to_string(),
            ty: wat::ValType::I32,
            mutable: true,
            init: i64::from(super::function::UNCAUGHT_EXCEPTION_FAILURE_CODE),
        });
    }

    // Lay out every interned string literal as a data segment above the runtime
    // scratch region, recording (offset, byte_len) per DataId for ConstStr. The
    // float<->string scratch region sits between the concat buffer and the string
    // literals so a strtod/ftoa never runs through an in-flight concatenation.
    let mut str_literals: Vec<(u32, u32)> = vec![(0, 0); module.data.strings.len()];
    let mut cursor = if has_main {
        runtime::COMMAND_DATA_END
    } else {
        runtime::RT_SCRATCH_END + runtime::FLOAT_SCRATCH_SIZE
    };
    let mut ordered_strings: Vec<(usize, &String)> =
        module.data.strings.iter().enumerate().collect();
    ordered_strings.sort_by(|(left_id, left), (right_id, right)| {
        left.as_bytes()
            .cmp(right.as_bytes())
            .then_with(|| left_id.cmp(right_id))
    });
    // Content -> segment, so a class property default whose bytes are already interned shares
    // that segment instead of laying out a second copy. First `DataId` wins, which is stable
    // because `ordered_strings` is sorted by (bytes, id).
    let mut interned_by_content: std::collections::HashMap<&str, (u32, u32)> =
        std::collections::HashMap::new();
    for (data_id, string) in ordered_strings {
        // A PHP string is BYTES; a Rust `String` must be UTF-8. The lexer bridges that by
        // carrying every non-ASCII escaped byte as a private-use marker char, so the segment has
        // to be decoded rather than copied — otherwise `"\xff"` reaches the module as the three
        // UTF-8 bytes of U+E0FF and `strlen` answers 3. The native backend decodes through the
        // same `string_bytes::literal_bytes`.
        let bytes = crate::string_bytes::literal_bytes(string);
        let len = bytes.len() as u32;
        wm.add_data(wat::DataSegment {
            offset: cursor,
            bytes,
        });
        str_literals[data_id] = (cursor, len);
        interned_by_content
            .entry(string.as_str())
            .or_insert((cursor, len));
        // 4-align the next literal.
        cursor = (cursor + len + 3) & !3;
    }

    // Object construction writes property defaults inline, so a string default has no `DataId`
    // to address — see `objects::literal_default_strings`. Lay out the ones that are not
    // already interned and key the whole set by content for `emit_scalar_default`.
    let mut default_strings: std::collections::HashMap<String, (u32, u32)> =
        std::collections::HashMap::new();
    // A runtime-error raise site writes its `$message` from the same content-keyed map, and its
    // text is a backend constant that no PHP literal need mention. Laid out only for a module
    // that can actually raise one — which needs both a failing operation and a `try` able to
    // receive it — so every other module's data segments stay byte-identical.
    let mut layout_values = objects::literal_default_strings(&module.class_infos);
    // An enum case's `name` — and a string-backed case's `value` — are written by the
    // materializer as string defaults, so they need addresses for the same reason.
    for (enum_name, info) in &module.enum_infos {
        let _ = enum_name;
        for case in &info.cases {
            if !layout_values.contains(&case.name) {
                layout_values.push(case.name.clone());
            }
            if let Some(crate::types::EnumCaseValue::Str(text)) = &case.value {
                if !layout_values.contains(text) {
                    layout_values.push(text.clone());
                }
            }
        }
    }
    // `Enum::from()` raises php's `ValueError` naming the value that did not match, so its
    // message is a CONCATENATION rather than a constant: a quote, the rendered value, then the
    // rest of php's sentence carrying the enum's own name. Only the constant pieces need
    // addresses, and only for an enum some call actually reaches — a module that never calls
    // `from()` keeps its data segments byte-identical.
    for enum_name in enum_names_reached_by_from(module) {
        for piece in [
            "\"".to_string(),
            format!("\"{ENUM_FROM_MESSAGE_SUFFIX}{enum_name}"),
            format!("{ENUM_FROM_MESSAGE_SUFFIX}{enum_name}"),
        ] {
            if !layout_values.contains(&piece) {
                layout_values.push(piece);
            }
        }
    }
    // Reading a slot some `unset` can have cleared raises php's own `Error`, whose message names
    // the class and the property — both compile-time names, so the whole sentence is a constant
    // that only needs an ADDRESS. Laid out per pair an unset reaches, so a module without one
    // keeps its data segments byte-identical.
    // ONE walk of the module for the whole compilation: the same list the lowering reads, so the
    // messages laid out here and the marker checks emitted there cannot describe different sets.
    let unset_properties = properties_reached_by_unset(module);
    for (class_name, property) in &unset_properties {
        let message = unset_property_message(class_name, property);
        if !layout_values.contains(&message) {
            layout_values.push(message);
        }
    }
    // A `count($obj)` on a Countable names the method in its null-receiver diagnostic, and
    // "count" is a backend constant there rather than a PHP literal the program wrote.
    if !layout_values.iter().any(|value| value == "count") {
        layout_values.push("count".to_string());
    }
    // `ob_get_status` answers an ordered map whose KEY NAMES are backend constants, so they
    // need addresses for the same reason the enum case names do.
    if super::output_buffer::module_uses_output_buffering(module) {
        for name in super::output_buffer::STATUS_KEYS
            .iter()
            .copied()
            .chain(std::iter::once(super::output_buffer::DEFAULT_HANDLER_NAME))
        {
            if !layout_values.iter().any(|value| value == name) {
                layout_values.push(name.to_string());
            }
        }
    }
    // The names a class-relation builtin answers with. They are computed from the module's
    // declarations rather than written in the program, so they have no `DataId` of their own.
    for name in super::builtins::class_relation_answer_strings(module) {
        if !layout_values.contains(&name) {
            layout_values.push(name);
        }
    }
    // The extension names `get_loaded_extensions()` answers with. They are this target's own
    // answer rather than anything the program wrote, so they have no `DataId` either.
    for name in super::builtins::loaded_extension_answer_strings(module) {
        if !layout_values.contains(&name) {
            layout_values.push(name);
        }
    }
    // The class names `get_declared_classes()` and `spl_classes()` answer with. Same reason:
    // they come from the module's own declarations and from the shared contract, never from a
    // string the program wrote, so nothing else gives them an address.
    for name in super::builtins::declared_name_answer_strings(module) {
        if !layout_values.contains(&name) {
            layout_values.push(name);
        }
    }
    // `gettype()` answers one of a fixed set of php-src spellings. Any module that calls it needs
    // those bytes addressable, whether the answer is settled at compile time or picked by a tag.
    if super::function::all_functions(module)
        .any(|function| {
            function.instructions.iter().any(|inst| {
                matches!(
                    inst.immediate.as_ref(),
                    Some(crate::ir::Immediate::RuntimeCall(
                        crate::ir::RuntimeCallTarget::Function(crate::ir::RuntimeFnId::Gettype)
                            | crate::ir::RuntimeCallTarget::ProfiledFunction {
                                target: crate::ir::RuntimeFnId::Gettype,
                                ..
                            }
                    ))
                )
            })
        })
    {
        for name in [
            "integer", "double", "boolean", "string", "array", "object", "NULL", "resource",
            "resource (closed)",
        ] {
            if !layout_values.iter().any(|value| value == name) {
                layout_values.push(name.to_string());
            }
        }
    }
    // `get_resource_type()` answers one of two php-src spellings — `stream` while the handle is
    // open, `Unknown` once `fclose` has run — and picks between them at runtime, so a module
    // that calls it needs both addressable for the same reason `gettype` does.
    if super::function::all_functions(module)
        .any(|function| {
            function.instructions.iter().any(|inst| {
                matches!(
                    inst.immediate.as_ref(),
                    Some(crate::ir::Immediate::RuntimeCall(
                        crate::ir::RuntimeCallTarget::Function(
                            crate::ir::RuntimeFnId::GetResourceType
                        ) | crate::ir::RuntimeCallTarget::ProfiledFunction {
                            target: crate::ir::RuntimeFnId::GetResourceType,
                            ..
                        }
                    ))
                )
            })
        })
    {
        for name in ["stream", "Unknown"] {
            if !layout_values.iter().any(|value| value == name) {
                layout_values.push(name.to_string());
            }
        }
    }
    // A read of an UNDECLARED property dispatches to `__get`, and the null-receiver check names
    // the method it was about to call. The name is a backend constant no PHP source mentions.
    if super::function::all_functions(module)
        .any(|function| {
            function.instructions.iter().any(|inst| {
                inst.op == crate::ir::Op::PropGet
                    && super::capability::magic_get_dispatch_is_supported(module, function, inst)
            })
        })
    {
        if !layout_values.iter().any(|value| value == "__get") {
            layout_values.push("__get".to_string());
        }
    }

    // `define("NAME", …)` answers false and warns for a duplicate, so each distinct name needs a
    // flag global and its bytes addressable for the warning that names it. Collected in one pass
    // over the module so the planner and the lowering agree on both.
    let mut define_names: Vec<String> = Vec::new();
    for function in super::function::all_functions(module) {
        for inst in &function.instructions {
            if !matches!(
                inst.immediate.as_ref(),
                Some(crate::ir::Immediate::RuntimeCall(
                    crate::ir::RuntimeCallTarget::Function(crate::ir::RuntimeFnId::Define)
                        | crate::ir::RuntimeCallTarget::ProfiledFunction {
                            target: crate::ir::RuntimeFnId::Define,
                            ..
                        }
                ))
            ) {
                continue;
            }
            let Some(name) = super::capability::define_constant_name(module, function, inst) else {
                continue;
            };
            if !define_names.iter().any(|seen| seen == name) {
                define_names.push(name.to_string());
            }
        }
    }
    for name in &define_names {
        if !layout_values.iter().any(|value| value == name) {
            layout_values.push(name.clone());
        }
        wm.add_global(wat::Global {
            name: super::symbols::define_flag_symbol(name),
            ty: wat::ValType::I32,
            mutable: true,
            init: 0,
        });
    }

    // `$obj[$key]` on an `ArrayAccess` implementor dispatches to `offsetGet`, but a program that
    // only ever writes the subscript never MENTIONS that name, so it is absent from the module
    // string table. The null-receiver check names the method it was about to call, so lay the
    // bytes out for any module carrying the untyped runtime call that shape uses.
    if super::function::all_functions(module)
        .any(|function| {
            function
                .instructions
                .iter()
                .any(|inst| {
                    inst.op == crate::ir::Op::RuntimeCall
                        && inst.immediate.is_none()
                        && (super::capability::array_access_read_is_supported(function, inst)
                            || super::capability::array_access_write_is_supported(function, inst))
                })
        })
    {
        for name in ["offsetGet", "offsetSet"] {
            if !layout_values.iter().any(|value| value == name) {
                layout_values.push(name.to_string());
            }
        }
    }
    // `Foo::class` answers a compile-time string, so every class name an EIR `ConstClassName`
    // names needs its bytes addressable in static data.
    for function in super::function::all_functions(module) {
        for inst in &function.instructions {
            if inst.op != crate::ir::Op::ConstClassName {
                continue;
            }
            let Some(crate::ir::Immediate::Data(data)) = inst.immediate else {
                continue;
            };
            let Some(name) = module.data.class_names.get(data.as_raw() as usize) else {
                continue;
            };
            let name = name.trim_start_matches('\\').to_string();
            if name != "static" && !layout_values.contains(&name) {
                layout_values.push(name);
            }
        }
    }
    // A variant dispatcher's fatal names the function at RUNTIME, so its bytes must be
    // addressable even when no PHP literal in the program mentions them.
    for name in super::includes::dispatch_group_names(module) {
        if !layout_values.iter().any(|value| value == &name) {
            layout_values.push(name);
        }
    }
    layout_values.sort();
    if has_main
        && super::function::module_uses_exceptions(module)
        && (super::function::module_raises_runtime_errors(module)
            || super::transfer::module_may_unbox_mixed_into_tagged(module))
    {
        for (_, _, message) in objects::CATCHABLE_RUNTIME_ERRORS {
            if !layout_values.iter().any(|value| value == message) {
                layout_values.push(message.to_string());
            }
        }
    }
    // The stream builtins' argument errors (a closed handle is a `TypeError`, a bad length a
    // `ValueError`) name the function, so each message and its uncaught-fatal line is laid out.
    if has_main {
        for text in super::streamguard::layout_strings(module) {
            if !layout_values.contains(&text) {
                layout_values.push(text);
            }
        }
    }
    // The implicit coercion at a declared scalar return composes its diagnostic at RUNTIME
    // from the function's own name — `f()` or `C::m()` — so every function that can raise it
    // needs those bytes addressable in static data.
    //
    // Closures included. `$f = function (int $a, int $b): int { return $a + $b; };` reaches the
    // boundary like any other declared `int` return — checked addition widens, because an
    // overflow promotes the sum to a float — and leaving them out failed the whole compilation
    // with "string property default \"__eir_closure_main_0\" has no data segment".
    if has_main {
        for function in module
            .functions
            .iter()
            .chain(module.class_methods.iter())
            .chain(module.closures.iter())
        {
            let coerces = function.instructions.iter().any(|inst| {
                // Two spellings reach the same runtime rule: the dedicated opcode EIR emits at
                // a declared `int` return, and the `Cast` the other three scalar returns still
                // arrive as.
                inst.op == crate::ir::Op::ReturnBoundaryMixedToInt
                    || (inst.op == crate::ir::Op::Cast
                        && super::capability::declared_return_coercion_target(
                            module, function, inst,
                        )
                        .is_some())
            });
            if coerces && !layout_values.iter().any(|value| value == &function.name) {
                layout_values.push(function.name.clone());
            }
        }
    }
    // The coercion at a builtin's declared `string` parameter composes its diagnostic from the
    // PHP FUNCTION's name and the PARAMETER's, neither of which any PHP literal need mention.
    if has_main {
        for function in module
            .functions
            .iter()
            .chain(module.class_methods.iter())
            .chain(module.closures.iter())
        {
            for inst in &function.instructions {
                // The coercion reaches an argument two ways: through a `Cast` the frontend
                // materialised, and — commonly — through a boxed operand it left in place, where
                // there is no cast anywhere to find. Both name the same two strings.
                let mut named: Vec<(&'static str, String)> = Vec::new();
                if inst.op == crate::ir::Op::Cast {
                    named.extend(
                        super::capability::mixed_string_argument_coercion(function, inst)
                            .or_else(|| {
                                super::capability::mixed_int_argument_coercion(function, inst)
                            })
                            .map(|(name, parameter, _)| (name, parameter)),
                    );
                }
                if inst.op == crate::ir::Op::Cast
                    && super::capability::cast_feeds_integer_arithmetic(function, inst).is_some()
                {
                    // The operator and the right operand's type word are backend constants no
                    // PHP literal need mention, so they are laid out here like every other
                    // diagnostic fragment composed at runtime.
                    // EVERY symbol the lowering can pick, not just the fallback: it chooses
                    // from the CONSUMER's opcode, so a program whose cast feeds a shift asks
                    // for a spelling this list has to already carry.
                    for value in ["%", "<<", ">>", "&", "|", "^", "int"] {
                        if !layout_values.iter().any(|already| already == value) {
                            layout_values.push(value.to_string());
                        }
                    }
                }
                if inst.op == crate::ir::Op::RuntimeCall {
                    for index in 0..inst.operands.len() {
                        named.extend(
                            super::capability::runtime_call_int_operand_coercion(
                                function, inst, index,
                            )
                            .map(|(name, parameter, _)| (name, parameter)),
                        );
                    }
                }
                for (name, parameter) in named {
                    for value in [name.to_string(), parameter] {
                        if !layout_values.iter().any(|already| already == &value) {
                            layout_values.push(value);
                        }
                    }
                }
            }
        }
    }
    for value in layout_values {
        if let Some(&placed) = interned_by_content.get(value.as_str()) {
            default_strings.insert(value, placed);
            continue;
        }
        let bytes = crate::string_bytes::literal_bytes(&value);
        let len = bytes.len() as u32;
        wm.add_data(wat::DataSegment {
            offset: cursor,
            bytes,
        });
        default_strings.insert(value, (cursor, len));
        // 4-align the next literal.
        cursor = (cursor + len + 3) & !3;
    }

    // The compiled script's own path, which `Throwable::getFile()` answers with — the native
    // backend's `_script_source_file` / `_script_source_file_len` pair, spelled as wasm globals.
    // Laid out only for a module that can actually produce a Throwable, so a program with no
    // exception in it keeps the data segments it had.
    if objects::module_can_throw(module) {
        if let Some(path) = objects::script_source_file(module) {
            let bytes = path.as_bytes().to_vec();
            let len = bytes.len() as u32;
            wm.add_data(wat::DataSegment {
                offset: cursor,
                bytes,
            });
            wm.add_global(wat::Global {
                name: objects::SCRIPT_SOURCE_FILE_GLOBAL.to_string(),
                ty: wat::ValType::I32,
                mutable: false,
                init: cursor as i64,
            });
            wm.add_global(wat::Global {
                name: objects::SCRIPT_SOURCE_FILE_LEN_GLOBAL.to_string(),
                ty: wat::ValType::I64,
                mutable: false,
                init: i64::from(len),
            });
            // 4-align the data that follows.
            cursor = (cursor + len + 3) & !3;
        }
    }

    // Emit the per-class gc_desc data (one runtime tag byte per property) plus the
    // class-indexed pointer table and the `$__gc_desc_ptrs` / `$__gc_desc_count` globals,
    // advancing the static-data cursor. This must land before `heap_base` is computed so
    // the descriptor data sits in static memory below the heap and is never overwritten by
    // allocation. `__rt_decref_object` walks these descriptors to release refcounted
    // property values before freeing an object at refcount zero.
    cursor = objects::emit_gc_desc_table(&mut wm, &module.class_infos, cursor);
    // The per-class PUBLIC property metadata `json_encode`'s object walk reads. Laid out beside
    // the gc descriptors because it answers the same question about the same layout, from the
    // same class-id index — only about a different subset of the properties.
    if super::json::module_uses_json_encode(module) {
        cursor = super::json::emit_json_property_table(&mut wm, module, cursor);
    }
    // `get_object_vars()` reads the same question about the same layout from the same class-id
    // index, but about a different subset: every public property, floats included, where the json
    // table drops what it has no serialization precision for.
    let projects_object_vars = super::objvars::module_uses_get_object_vars(module);
    let reads_protocols = super::protoent::module_uses_protocols(module);
    let tracks_object_handles = super::handles::module_uses_object_handles(module);
    cursor = super::protoent::emit_protocol_tables(&mut wm, module, cursor);
    // RFC 1951's length and distance tables. They are 500 bytes and every program that
    // compresses or decompresses reads them, so they are laid out unconditionally rather
    // than gated like the class-indexed ones.
    cursor = super::zlib::emit_zlib_tables(&mut wm, cursor);
    // Each `preg_match` pattern's compiled program. The pattern is a literal, so the whole
    // bytecode is settled here and the VM is handed an address rather than a pattern.
    let (regex_programs, next) = super::regex::emit_pattern_programs(&mut wm, module, cursor);
    cursor = next;
    if projects_object_vars {
        cursor = super::objvars::emit_object_vars_table(&mut wm, module, cursor);
    }

    // P6f class-metadata tables (`__class_parent_ids`, `__class_interface_ptrs`,
    // `__class_name_entries`, `__class_name_missing`), advancing the static-data
    // cursor. Must land immediately after `emit_gc_desc_table` and before
    // `heap_base` is computed so the tables sit in static memory below the heap,
    // indexed by runtime class_id. Reuses `$__gc_desc_count` as the shared bounds.
    cursor = classes::emit_class_metadata_tables(&mut wm, module, cursor);

    // P6g: dynamic-string instanceof target lookup table
    // (`__instanceof_target_entries` + `__instanceof_target_count`), advancing the
    // static-data cursor. Must land immediately after `emit_class_metadata_tables` and
    // before `heap_base` is computed so the table sits in static memory below the heap,
    // scanned case-insensitively by `__rt_instanceof_lookup` (registered in
    // `emit_class_runtime`).
    cursor = classes::emit_instanceof_target_table(&mut wm, module, cursor);

    // P7b: per-closure capture-tag byte arrays (one byte per by-value capture, in
    // source order), laid out in static memory below the heap. The recorded base
    // address per closure (indexed by its canonical symbol rank = `entry_index`)
    // is stamped as the descriptor's `capture_tags_ptr` by `ClosureNew`, so the
    // release runtime can walk refcounted captures. Must land before `heap_base`
    // is computed so the arrays sit below the bump allocator. No-capture closures
    // get a `0` sentinel (no segment emitted).
    let (cursor, closure_tag_ptrs) =
        closures::emit_closure_capture_tag_tables(&mut wm, module, cursor)?;

    // P7d2a: build the first-class-callable entry registry once, before any function
    // is lowered. It collects the distinct user-free-function targets of every
    // `Op::FirstClassCallableNew` in the module and assigns them unified callable-ladder
    // indices AFTER the closures (`module.closures.len() + position`), so closures keep
    // `0..N` and FCC entries take `N..N+M`. `FirstClassCallableNew` lowering reads it
    // (via `FnCtx::fcc_entry_index`) to stamp descriptors; `emit_closure_dispatch` reads
    // it to emit one FCC wrapper + ladder arm per entry. A builtin/extern/method FCC
    // target is excluded here and rejected at lowering time (deferred slice).
    // One 16-byte slot per distinct static property, laid out under its DECLARING class so
    // an inherited static shares one storage. Must land before `heap_base` is computed so
    // the region sits in static memory below the bump allocator and its addresses are
    // compile-time constants. The initial bytes come from the literal defaults, which is
    // why no initializer has to run: a string default carries its LITERAL's address, and
    // the refcount helpers already no-op below the heap.
    let (mut static_slots, cursor) = super::statics::plan_static_slots(&mut wm, module, &default_strings, cursor);
    // PHP globals share that map — a `const` at top level and a `global $x` both lower to
    // `store_global`, and their storage has the same 16-byte shape a static property uses.
    let cursor = super::statics::plan_global_slots(module, &mut static_slots, cursor);
    // A function's `static $x` is one cell for the whole program, like a static property —
    // same map, a key that carries the declaring function, and eight more bytes for the flag
    // that says whether the initializer has already run.
    let cursor = super::statics::plan_static_local_slots(module, &mut static_slots, cursor);

    // One mutable flag per include-once site, and one active-variant slot per include-loaded
    // function group. Both are plain module globals, like the `define` flags above.
    super::includes::plan_include_globals(&mut wm, module);

    let fcc_entries = closures::collect_fcc_free_function_entries(module);

    // The heap begins 16-aligned just above the string/data region; reserve two
    // pages of initial headroom above it. The bump allocator grows beyond
    // `heap_end` with `memory.grow` when this region is exhausted.
    const PAGE: u32 = 65536;
    let heap_base = (cursor + 15) & !15;
    let pages = (heap_base / PAGE) + 2;
    let heap_end = pages * PAGE;
    wm.set_memory(pages, Some("memory"));
    if has_main {
        heap::emit_command_heap_runtime(&mut wm, heap_base, heap_end);
    } else {
        heap::emit_heap_runtime(&mut wm, heap_base, heap_end);
    }
    // The object-handle pool, for a module that asks an object for its identity. It must
    // precede the object runtime: `__rt_decref_object` calls `__rt_handle_forget` on the
    // way to the free, and WAT requires the target to be defined.
    if tracks_object_handles {
        super::handles::emit_handle_runtime(&mut wm);
    }
    // The kind dispatch gets its generator-frame arm only when a frame can exist, and the
    // release it names is emitted right here so the two cannot come apart.
    let has_generators = !super::generators::generator_functions(module).is_empty();
    if has_generators {
        refcount::emit_refcount_runtime_with_generator_frames(&mut wm);
        super::generators::emit_frame_release(&mut wm, module);
    } else {
        refcount::emit_refcount_runtime(&mut wm);
    }
    // Callable-descriptor refcount runtime: `__rt_callable_descriptor_release`, called
    // from `__rt_decref_any` kind-6 (P7a0). References only `__rt_decref_any` and
    // `__rt_heap_free`, so it needs no extra globals; every module emitting the refcount
    // runtime must emit this too, since `__rt_decref_any`'s kind-6 branch calls it and
    // WAT requires the call target to be defined.
    closures::emit_closure_runtime(&mut wm);
    // Object refcount runtime: `__rt_decref_object`, called from `__rt_decref_any`
    // kind-4. P6b performs the full gc_desc-driven property walk + `__rt_heap_free`.
    objects::emit_object_runtime(&mut wm, tracks_object_handles);
    // The `get_object_vars()` projection, for a module that calls it. It references the tables
    // laid out above plus the hash runtime, so it is registered after both.
    if projects_object_vars {
        super::objvars::emit_object_vars_runtime(&mut wm);
    }
    // The protocol database, for a module that consults it. It reads only the tables laid out
    // above and `__rt_mixed_from_value`, which the mixed runtime below provides.
    if reads_protocols {
        super::protoent::emit_protocol_runtime(&mut wm);
    }
    // P6e destructor dispatch: `__rt_call_object_destructor`, called from the free path
    // above to run `__destruct` before the property walk. One if-ladder arm per class
    // whose hierarchy declares `__destruct` (resolved via `method_impl_classes`).
    objects::emit_destructor_dispatch(&mut wm, &module.class_infos)?;
    // P6f class runtime helpers: `__rt_instanceof`, `__rt_mixed_instanceof`,
    // `__rt_class_name_by_cid`, `__rt_class_name_by_obj`. They reference the
    // class-metadata globals emitted above, so they must be registered after
    // `emit_class_metadata_tables`. They safely return false/empty when
    // `__gc_desc_count == 0` (no classes).
    classes::emit_class_runtime(&mut wm);
    arrays::emit_array_runtime(&mut wm);
    mixed::emit_mixed_runtime(&mut wm, has_main, Some(module));
    hashes::emit_hash_runtime(&mut wm, has_main);
    // Float<->string runtime (ftoa + strtod). Published with the `$__float_scratch`
    // global set to `FLOAT_SCRATCH_BASE` so cast/echo/mixed-stdout callers pass
    // `(global.get $__float_scratch)` as the bignum scratch base.
    float::emit_float_runtime(&mut wm, runtime::FLOAT_SCRATCH_BASE as i32);
    strict::emit_strict_runtime(&mut wm);
    // Cycle collector: `__rt_gc_collect_cycles` and its helpers. Emitted after every
    // runtime it walks or releases through — the array/hash/mixed/object layouts and
    // `__rt_decref_any` — because WAT requires each call target to be defined.
    super::gc::emit_gc_runtime(&mut wm);
    super::builtins::emit_builtin_runtime(&mut wm, has_main);

    // Generators: the if-ladder that resumes one, then the wrapper each `g(...)` becomes.
    // `lower_function` emits the BODY of each under its `__resume` symbol, so the wrapper
    // takes the ordinary one — a call site needs no knowledge that it is calling a
    // generator, which is what lets `Op::Call` stay an ordinary direct call.
    super::generators::emit_resume_dispatcher(&mut wm, module);
    for function in super::generators::generator_functions(module) {
        let class_id = module
            .class_infos
            .get("Generator")
            .map(|class_info| class_info.class_id)
            .ok_or_else(|| {
                WasmError::Unsupported(
                    "a module with a generator needs the Generator class registered".to_string(),
                )
            })?;
        super::generators::emit_wrapper(&mut wm, module, function, class_id)?;
    }

    // Lower every user function; `main` becomes the WASI `_start` command entry.
    let mut functions: Vec<_> = module.functions.iter().collect();
    functions.sort_by_key(|function| symbols::function_symbol(function));
    for function in functions {
        let function = function::lower_function(
            module,
            function,
            &str_literals,
            &default_strings,
            &closure_tag_ptrs,
            &fcc_entries,
            &static_slots,
            &regex_programs,
            &unset_properties,
        )?;
        wm.add_func(function);
    }

    // Lower every class method (instance + static), so `__construct` and other
    // methods become callable WAT functions. Reuses the same lowering as user
    // functions: a non-static method's hidden leading `this` param is just param 0
    // (`IrType::Heap(Object)` -> `WasmRepr::Ptr` / i32), and the body uses the
    // already-supported `PropGet`/`PropSet`/`LoadLocal("this")`/`EchoValue` ops. WAT
    // `call $<name>` resolves a module-local function regardless of definition
    // order, so a `module.functions` entry calling `__construct` (via `ObjectNew`)
    // sees the method defined here even though methods are lowered after it.
    let mut class_methods: Vec<_> = module.class_methods.iter().collect();
    class_methods.sort_by_key(|function| symbols::function_symbol(function));
    for function in class_methods {
        let function = function::lower_function(
            module,
            function,
            &str_literals,
            &default_strings,
            &closure_tag_ptrs,
            &fcc_entries,
            &static_slots,
            &regex_programs,
            &unset_properties,
        )?;
        wm.add_func(function);
    }

    // Lower every closure body (P7a0). A closure is a module-level EIR function with a
    // synthetic `__eir_closure_<owner>_<n>` name and `FunctionFlags::is_closure`; its
    // params are the visible user params ++ capture params (captures appended at the
    // tail). `lower_function` handles the body as-is. WAT `call $<name>` resolves across
    // the whole module regardless of definition order, so the P7a1 wrapper that calls a
    // closure body sees it defined here.
    for function in closures::ordered_closures(module) {
        let function = function::lower_function(
            module,
            function,
            &str_literals,
            &default_strings,
            &closure_tag_ptrs,
            &fcc_entries,
            &static_slots,
            &regex_programs,
            &unset_properties,
        )?;
        wm.add_func(function);
    }

    // Emit per-(introducer, method) dispatch stubs for virtual instance methods,
    // so every `call $<stub>` emitted by `MethodCall` lowering resolves to a
    // defined function. Must run after class methods are lowered (stub signatures
    // are read from the class-method `Function`s) but before `wm.render()`.
    methods::emit_method_dispatch_stubs(&mut wm, module)?;

    // One dispatcher per include-variant group, reading the active-variant global. Must run
    // after user functions are lowered — the signature comes from a variant's own `Function`.
    super::includes::emit_variant_dispatchers(&mut wm, module, &default_strings);

    // Emit one wrapper per closure body plus the `__rt_closure_call` if-ladder that
    // `ClosureCall` lowering dispatches through (P7a1). Must run after closure bodies are
    // lowered (wrappers call `fn___eir_closure_<owner>_<n>`) but before `wm.render()`.
    // `json_encode`'s per-class public-property metadata, and the encoder that reads it. Both
    // only for a module that calls it, so every other module's data segments are unchanged.
    if super::json::module_uses_json_encode(module) {
        let arms = super::json::jsonserialize_ladder(module);
        super::json::emit_json_runtime(&mut wm, &arms);
    }

    closures::emit_closure_dispatch(&mut wm, module, &fcc_entries)?;

    // Output buffering intercepts every stdout write, so its `__rt_stdout_write` replaces the
    // passthrough one for a module that can actually enter a buffer. The handler path needs the
    // callable ladder, which exists exactly when this module has a closure or a first-class
    // callable — and a module with neither cannot have produced a handler either.
    // Both variants call `__rt_wasi_write_or_fail`, which only a COMMAND module has: the
    // five sites that reach `__rt_stdout_write` all live in the command runtime, so a
    // reactor module references neither and needs neither.
    if !has_main {
        // no stdout write exists in this module
    } else if super::output_buffer::module_uses_output_buffering(module) {
        let has_callable_ladder = !module.closures.is_empty() || !fcc_entries.is_empty();
        super::output_buffer::emit_output_buffer_runtime(
            &mut wm,
            has_callable_ladder,
            super::function::module_uses_exceptions(module),
        );
        wm.add_raw_func(super::output_buffer::RT_STDOUT_WRITE);
        let status_keys: Vec<(u32, u32)> = super::output_buffer::STATUS_KEYS
            .iter()
            .map(|name| {
                default_strings
                    .get(*name)
                    .copied()
                    .expect("ob_get_status key laid out above")
            })
            .collect();
        let handler = default_strings
            .get(super::output_buffer::DEFAULT_HANDLER_NAME)
            .copied()
            .expect("default handler name laid out above");
        super::output_buffer::emit_ob_get_status(&mut wm, &status_keys, handler);
    } else {
        wm.add_raw_func(super::output_buffer::RT_STDOUT_WRITE_PASSTHROUGH);
    }

    // The `[$object, "method"]` / `["Class", "method"]` ladders and their per-method wrappers.
    // Runs after class methods are lowered (a wrapper calls the method body by symbol) and
    // after `emit_closure_dispatch`, which defines the shared `__rt_fail_callable_dispatch`
    // boundary every rejected arm falls into.
    super::callable_arrays::emit_callable_array_dispatch(&mut wm, module)?;

    let wat = wm.render_checked().map_err(WasmError::InvalidModule)?;
    Ok(LoweredWasmPlan { wat })
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Regression tests for exact, owned, and deterministic WASM planning.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.
    //!
    //! Key details:
    //! - Tests consume the plan exactly as public generation does.
    //! - Sentinel literals prove planning uses the real module data layout.

    use super::plan_module;
    use crate::codegen::platform::Target;
    use crate::codegen::Emit;
    use crate::ir::Module;

    /// Verifies a successful plan owns assemblable WAT with exact literal data.
    #[test]
    fn plan_owns_checked_wat_with_exact_module_data() {
        let mut module = Module::new(Target::wasm());
        module.data.intern_string("wasm-plan-sentinel");

        let wat = plan_module(&module, Emit::Executable)
            .expect("exact module planning should succeed")
            .into_wat();

        assert!(wat.contains("\"wasm-plan-sentinel\""), "{wat}");
        let bytes =
            ::wat::parse_str(&wat).unwrap_or_else(|error| panic!("WAT did not assemble: {error}"));
        wasmparser::validate(&bytes)
            .unwrap_or_else(|error| panic!("WASM did not validate: {error}"));
    }

    /// Verifies repeated independent planning of one module is byte deterministic.
    #[test]
    fn planning_the_same_module_is_byte_deterministic() {
        let mut module = Module::new(Target::wasm());
        module.data.intern_string("zeta");
        module.data.intern_string("alpha");

        let first = plan_module(&module, Emit::Executable)
            .expect("first exact module plan should succeed")
            .into_wat();
        let second = plan_module(&module, Emit::Executable)
            .expect("second exact module plan should succeed")
            .into_wat();

        assert_eq!(first, second);
    }
}
