//! Purpose:
//! Lowers EIR object instructions (`ObjectNew`, `PropGet`, `PropSet`) for the
//! wasm32-wasi backend and emits the kind-4 (object) refcount runtime
//! `__rt_decref_object` referenced by `__rt_decref_any`.
//!
//! Called from:
//! - `crate::codegen_wasm::inst::lower_instruction` dispatches the three object ops here.
//! - `crate::codegen_wasm::generate()` emits `emit_object_runtime` after the refcount runtime
//!   and `emit_gc_desc_table` before the heap base is computed.
//!
//! Key details:
//! - P6b scope: declared properties of scalar, string, container (array/hash/object), and
//!   mixed/union/iterable type. Constructors (P6c), method dispatch (P6d), and destructors
//!   (P6e) are supported. P6g adds dynamic properties (`#[\AllowDynamicProperties]` and
//!   `stdClass`) via an 8-byte Mixed-cell hash tail after the declared payload, plus
//!   `NullsafePropGet` for nullable concrete receivers (short-circuits to a null Mixed cell
//!   on a null unbox).
//! - Objects are heap blocks whose 16-byte header (`__rt_heap_alloc`) is stamped with
//!   heap-kind 4 at `[ptr-8]`; the payload holds `class_id` at `+0` and one 16-byte
//!   `(value_lo i64, value_hi i64)` slot per declared property (parent-first). The hi word is
//!   the runtime value tag for refcounted slots (4/5/6/7) or the string length for Str, and 0
//!   for scalars.
//! - `__rt_decref_object` performs the full release: a refcount==0 re-entrancy guard, mark-zero,
//!   a call to `__rt_call_object_destructor` (runs `__destruct` with the properties still
//!   intact, before the walk), then a gc_desc-driven property walk that releases each refcounted
//!   slot value (desc tag in {1,4,5,6,7}) before freeing the block via `__rt_heap_free` (unsafe;
//!   refcount is already 0). The property count and the dynamic-property tail come from the
//!   CLASS metadata table, never from the block's size header: the allocator reuses a free
//!   block without splitting it, so the header can be far larger than the object's payload.
//!   An `AllowDynamicProperties`/stdClass object carries a Mixed-cell hash right after its
//!   declared slots, released via `__rt_decref_any` before `__rt_heap_free`.
//!   The gc_desc table is emitted by `emit_gc_desc_table` (one tag byte per property, indexed by
//!   `class_id`); `emit_gc_desc_stub` declares empty-table globals for unit-test harnesses that
//!   register no classes (the `cid < count` check is then false for every cid and the walk is
//!   skipped, which is correct for harness blocks holding no refcounted property values).
//! - PropGet returns an OWNED value (persist/incref) so the MaybeOwned read result is
//!   independent of the object; PropSet releases the previous slot value (null-safe), retains or
//!   persists the incoming value, and stores lo + hi. Mixed slots transfer owned cells, retain
//!   borrowed/local-loaded cells, and box concrete values through `emit_box_value_into_mixed`.

use super::context::{FnCtx, Result};
use super::symbols::method_symbol;
use super::inst::{data_immediate, operand, store_result};
use super::values::WasmRepr;
use super::classes::MixedPropertySource;
use super::wat::{DataSegment, Global, ValType, WatModule};
use super::WasmError;
use crate::codegen::{literal_default_value, LiteralDefaultValue};
use crate::ir::{
    DataId, Function, Immediate, Instruction, IrHeapKind, IrType, Module, Op, Ownership, ValueDef,
    ValueId,
};
use crate::names::php_symbol_key;
use crate::types::{ClassInfo, PhpType};
use std::collections::HashMap;

/// The hidden first parameter every static method carries: the class the call NAMED,
/// which is what `static` resolves to and what `self` deliberately does not.
pub(super) const CALLED_CLASS_ID_PARAM: &str = "__elephc_called_class_id";

/// Whether a local is the hidden called-class id every static method receives as parameter 0.
///
/// The lowering names it with the generated-local suffix (`names::CALLED_CLASS_ID_LOCAL`), which
/// no PHP variable can spell; hand-built test bodies use the bare name.
pub(super) fn is_called_class_id_local(name: Option<&str>) -> bool {
    matches!(name, Some(name) if name == crate::names::CALLED_CLASS_ID_LOCAL || name == CALLED_CLASS_ID_PARAM)
}

/// Registers the object refcount runtime (`__rt_decref_object`) on `wm`.
///
/// Must be emitted alongside `refcount::emit_refcount_runtime`, whose `__rt_decref_any` calls
/// `__rt_decref_object` from its kind-4 branch. The function references the `$__gc_desc_ptrs`
/// and `$__gc_desc_count` globals, so every module emitting this runtime must also emit either
/// `emit_gc_desc_table` (real programs, via `generate()`) or `emit_gc_desc_stub` (unit-test
/// harnesses) so the globals exist and the WAT validates. WAT resolves `(call $name)` across
/// the whole module regardless of definition order, so the relative placement of this emitter
/// and `emit_refcount_runtime` is cosmetic; what matters is that every harness emitting
/// `__rt_decref_any` also emits this plus a gc_desc global pair.
pub(super) fn emit_object_runtime(wm: &mut WatModule, tracks_handles: bool) {
    wm.add_raw_func(&rt_decref_object(tracks_handles));
    wm.add_raw_func(RT_OBJECT_CLONE_SHALLOW);
    wm.add_raw_func(RT_THROWABLE_LINE);
    wm.add_raw_func(RT_THROWABLE_PREVIOUS_AT);
    wm.add_raw_func(RT_EXCEPTION_CHAIN);
}

/// `__rt_throwable_previous_at`: the object a Throwable's `previous` slot at `$off` holds, or 0.
///
/// The slot is `?Throwable`, a boxed cell: tag 6 carries the object, anything else is null.
const RT_THROWABLE_PREVIOUS_AT: &str = r#"(func $__rt_throwable_previous_at (param $obj i32) (param $off i32) (result i32)
  (local $cell i32) (local $tag i64) (local $lo i64) (local $hi i64)
  (local.set $cell (i32.wrap_i64 (i64.load (i32.add (local.get $obj) (local.get $off)))))
  (if (i32.eqz (local.get $cell)) (then (return (i32.const 0))))
  (call $__rt_mixed_unbox (local.get $cell))
  (local.set $hi)
  (local.set $lo)
  (local.set $tag)
  (if (result i32) (i64.eq (local.get $tag) (i64.const 6))
    (then (i32.wrap_i64 (local.get $lo)))
    (else (i32.const 0))))
"#;

/// `__rt_exception_chain`: appends the OWNED `$old` exception at the end of `$new`'s chain.
///
/// php-src's `zend_exception_set_previous`, which runs when an exception leaves a `finally`
/// that was entered with another one pending, and the native `__rt_exception_chain` it mirrors:
/// a link user code already set is kept, the old exception goes at the TAIL, and when the two
/// chains share an object the old owner is released instead of creating a cycle. `$new` is
/// borrowed; `$old`'s reference is consumed either way.
const RT_EXCEPTION_CHAIN: &str = r#"(func $__rt_exception_chain (param $new i32) (param $old i32) (param $off i32)
  (local $node i32) (local $ancestor i32) (local $cell i32) (local $box i32)
  (if (i32.eqz (local.get $old)) (then (return)))
  (local.set $node (local.get $new))
  (block $append (loop $scan
    (local.set $ancestor (local.get $old))
    (block $clear (loop $ancestors
      (if (i32.eq (local.get $ancestor) (local.get $node))
        (then
          (call $__rt_decref_any (local.get $old))                     ;; already linked: no cycle
          (return)))
      (local.set $ancestor (call $__rt_throwable_previous_at (local.get $ancestor) (local.get $off)))
      (br_if $ancestors (local.get $ancestor))))
    (local.set $ancestor (call $__rt_throwable_previous_at (local.get $node) (local.get $off)))
    (br_if $append (i32.eqz (local.get $ancestor)))                    ;; the tail
    (local.set $node (local.get $ancestor))
    (br $scan)))
  (local.set $cell (i32.wrap_i64 (i64.load (i32.add (local.get $node) (local.get $off)))))
  (local.set $box (call $__rt_mixed_from_value (i64.const 6)
    (i64.extend_i32_u (local.get $old)) (i64.const 0)))                ;; retains the object
  (i64.store (i32.add (local.get $node) (local.get $off)) (i64.extend_i32_u (local.get $box)))
  (i64.store offset=8 (i32.add (local.get $node) (local.get $off)) (i64.const 7))  ;; a boxed slot
  (if (local.get $cell) (then (call $__rt_decref_any (local.get $cell)))) ;; the null cell it replaces
  (call $__rt_decref_any (local.get $old)))                              ;; the box holds its own
"#;

/// The byte offset of `previous` in every Throwable class of the module, when it is one offset.
///
/// `__rt_exception_chain` walks objects of any Throwable class through one offset, so the
/// classes must agree — they do when `previous` is laid out by `Exception`/`Error` and inherited
/// — and the slot must be the boxed `?Throwable` cell the helper reads.
pub(super) fn throwable_previous_offset(module: &Module) -> std::result::Result<i32, String> {
    let mut offset = None;
    for (class_name, ci) in &module.class_infos {
        let mut ancestor = Some(class_name.clone());
        let mut throwable = false;
        while let Some(name) = ancestor {
            if name == "Exception" || name == "Error" {
                throwable = true;
                break;
            }
            ancestor = module.class_infos.get(&name).and_then(|info| info.parent.clone());
        }
        if !throwable {
            continue;
        }
        let (_, slot, php_type) = resolve_property_slot(ci, "previous")
            .map_err(|_| format!("Throwable {class_name} has no $previous slot"))?;
        if php_type.codegen_repr() != PhpType::Mixed {
            return Err(format!(
                "Throwable {class_name}'s $previous is {php_type:?}, not a boxed cell"
            ));
        }
        match offset {
            None => offset = Some(slot),
            Some(seen) if seen != slot => {
                return Err(format!(
                    "Throwable {class_name}'s $previous sits at {slot}, another class's at {seen}"
                ))
            }
            Some(_) => {}
        }
    }
    let offset = offset.ok_or_else(|| "the module declares no Throwable class".to_string())?;
    i32::try_from(offset).map_err(|_| format!("$previous offset {offset} exceeds an i32"))
}

/// Lowers `RuntimeCallTarget::ExceptionChain`: `(new, old)`, consuming `old`.
pub(super) fn lower_exception_chain(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let offset = throwable_previous_offset(ctx.module).map_err(WasmError::Unsupported)?;
    ctx.emit_load_value(operand(inst, 0)?)?;
    ctx.emit_load_value(operand(inst, 1)?)?;
    ctx.fb.ins(&format!("i32.const {offset}"), "where every Throwable keeps $previous");
    ctx.fb.ins("call $__rt_exception_chain", "append the pending exception at the tail");
    Ok(())
}

/// Declares empty-table `$__gc_desc_ptrs` / `$__gc_desc_count` globals for unit-test harnesses
/// that register no classes.
///
/// With `count = 0` the `cid < count` range check in `__rt_decref_object` is false for every
/// class id, so the property walk is skipped and the object is freed shallowly. That is correct
/// for harness blocks that hold no refcounted property values (the P6a object tests). Real
/// programs use `emit_gc_desc_table` instead, which emits the actual per-class descriptors.
#[cfg(test)]
pub(super) fn emit_gc_desc_stub(wm: &mut WatModule) {
    wm.add_global(Global {
        name: "__gc_desc_ptrs".to_string(),
        ty: ValType::I32,
        mutable: false,
        init: 0,
    });
    wm.add_global(Global {
        name: "__gc_desc_count".to_string(),
        ty: ValType::I32,
        mutable: false,
        init: 0,
    });
    wm.add_global(Global {
        name: "__gc_desc_meta".to_string(),
        ty: ValType::I32,
        mutable: false,
        init: 0,
    });
}

/// The properties an inherited Throwable constructor initializes, in constructor-argument order.
///
/// `Exception::__construct(string $message = "", int $code = 0, ?Throwable $previous = null)` is
/// the signature every built-in throwable shares, and these are the three slots it writes.
pub(super) const THROWABLE_CONSTRUCTOR_PROPERTIES: [&str; 3] = ["message", "code", "previous"];

/// Returns true when constructing `class_name` must open-code the inherited Throwable
/// constructor instead of calling one.
///
/// Built-in throwables carry a constructor in the checker's signature table but have NO EIR
/// method body — the native backend open-codes the same field writes in its
/// `lower_builtin_throwable_new`, and a user subclass declaring no constructor of its own
/// inherits exactly that situation. Requiring the class to actually BE a Throwable is what keeps
/// this from papering over a genuinely missing body: an ordinary class whose constructor failed
/// to be emitted still fails loudly rather than being constructed with its arguments quietly
/// dropped into whichever properties happen to be declared first.
pub(super) fn needs_open_coded_throwable_constructor(
    module: &Module,
    class_name: &str,
    impl_class: &str,
) -> bool {
    bodyless_throwable_method(module, class_name, impl_class, &php_symbol_key("__construct"))
}

/// Returns true when `impl_class` supplies no EIR body for `method_key` and `class_name` is a
/// Throwable, so the call must be open-coded rather than dispatched.
fn bodyless_throwable_method(
    module: &Module,
    class_name: &str,
    impl_class: &str,
    method_key: &str,
) -> bool {
    if super::methods::find_method_function(&module.class_methods, impl_class, method_key).is_some()
    {
        return false;
    }
    is_throwable_class(module, class_name)
}

/// The module global holding the address of the compiled script's own path.
///
/// Named after the native backend's `_script_source_file` symbol, which holds the same bytes for
/// the same reason: `Throwable::getFile()` and php's `__FILE__` both report the compiled
/// script's path, and the compiler knows exactly one — EIR spans carry a line and a column but
/// no filename.
pub(super) const SCRIPT_SOURCE_FILE_GLOBAL: &str = "__script_source_file";

/// The module global holding the byte length of [`SCRIPT_SOURCE_FILE_GLOBAL`]'s string.
pub(super) const SCRIPT_SOURCE_FILE_LEN_GLOBAL: &str = "__script_source_file_len";

/// Returns the compiled script's path, or `None` when the module has none to report.
///
/// A fragment compiled from stdin or from memory has no path, and an empty one is the same
/// situation spelled differently — both answer php-src's empty `getFile()`. The planner lays the
/// bytes out and the accessor reads them back through exactly this predicate, so neither can
/// emit a reference the other did not provide for.
///
/// A module that never names `getFile` answers `None` too: the path is the one input that
/// differs between two checkouts of the same program, so laying it out unread would make the
/// emitted module depend on the directory it was compiled in.
pub(super) fn script_source_file(module: &Module) -> Option<&str> {
    module
        .source_path
        .as_deref()
        .filter(|path| !path.is_empty())
        .filter(|_| module_names_get_file(module))
}

/// Returns true when any instruction of any body carries `getFile` as its name immediate.
///
/// Deliberately broader than the calls the accessor open-codes: a reader that this scan missed
/// would answer an empty path, while a match that reads nothing only costs the data segment.
fn module_names_get_file(module: &Module) -> bool {
    module.functions.iter().any(|function| {
        function.instructions.iter().any(|instruction| {
            let data = match instruction.immediate {
                Some(Immediate::Data(data)) | Some(Immediate::ProfiledData { data, .. }) => data,
                _ => return false,
            };
            module
                .data
                .strings
                .get(data.as_raw() as usize)
                .is_some_and(|name| name.eq_ignore_ascii_case("getfile"))
        })
    })
}

/// Returns true when this module can produce a `Throwable` at all.
///
/// The gate for laying the script path out: a module that declares no throwable class and names
/// no `Throwable` interface has no accessor to read it, so its data segments stay as they were.
pub(super) fn module_can_throw(module: &Module) -> bool {
    module.interface_infos.contains_key("Throwable")
        || module
            .class_infos
            .keys()
            .any(|class_name| is_throwable_class(module, class_name))
}

/// Byte offset of the creation-line word inside an instance of `class_name`.
///
/// The word sits after EVERYTHING the object's class declares — the property slots and the
/// dynamic-property tail alike — so adding it disturbs no existing offset, and both the release
/// walk and the cycle collector skip it without being told to: each derives its cursor from the
/// class's declared property count, which the word is not part of.
///
/// The offset therefore MOVES with the subclass, which is why the accessor reads it through
/// `__rt_throwable_line` rather than from the receiver's static class: `catch (Exception $e)`
/// can hold a subclass that declares properties of its own, and its line word is further along.
pub(super) fn throwable_creation_line_offset(ci: &ClassInfo, class_name: &str) -> usize {
    8 + ci.properties.len() * 16 + if has_dynamic_properties(ci, class_name) { 8 } else { 0 }
}

/// One `Throwable` accessor that has no EIR body and is open-coded at the call site.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum ThrowableIntrinsic {
    /// `getMessage()` and `__toString()`: the `$message` slot.
    Message,
    /// `getCode()`: the `$code` slot.
    Code,
    /// `getPrevious()`: the `$previous` slot.
    Previous,
    /// `getFile()`: the module's own source path.
    File,
    /// `getTraceAsString()`: the synthetic empty string.
    EmptyString,
    /// `getLine()`: the line the Throwable was CONSTRUCTED on.
    Line,
    /// `getTrace()`: the synthetic empty array.
    EmptyTrace,
}

/// Maps a `Throwable` accessor's method key to what it produces.
///
/// `getFile()` and `getLine()` answer the same origin the NATIVE backend reports — the module's
/// own source path, and the line the Throwable was constructed on — because the two backends
/// answering an exception differently is the failure this target is trying to avoid, and that
/// origin is also what php-src reports. `getTrace()` and `getTraceAsString()` stay synthetic on
/// both: elephc keeps no call stack to render, so there is nothing truer than an empty backtrace
/// to answer with.
fn throwable_intrinsic_for_key(method_key: &str) -> Option<ThrowableIntrinsic> {
    match method_key {
        "getmessage" | "__tostring" => Some(ThrowableIntrinsic::Message),
        "getcode" => Some(ThrowableIntrinsic::Code),
        "getprevious" => Some(ThrowableIntrinsic::Previous),
        "getfile" => Some(ThrowableIntrinsic::File),
        "gettraceasstring" => Some(ThrowableIntrinsic::EmptyString),
        "getline" => Some(ThrowableIntrinsic::Line),
        "gettrace" => Some(ThrowableIntrinsic::EmptyTrace),
        _ => None,
    }
}

/// Returns the accessor to open-code for a method call, or `None` to dispatch normally.
///
/// `candidates` is every concrete class the receiver can be at run time, paired with the class
/// that implements the method for it. The accessor is open-coded only when NONE of them has a
/// body: a subclass that overrides `getMessage()` must win, and a call that could reach both an
/// overriding and a built-in receiver is left to fail on the built-in rather than silently
/// answering the property for every receiver. That is stricter than the native backend, which
/// intercepts on the class name alone and would ignore such an override.
pub(super) fn throwable_intrinsic(
    module: &Module,
    class_name: &str,
    method_key: &str,
    candidates: &[(String, String)],
) -> Option<ThrowableIntrinsic> {
    let intrinsic = throwable_intrinsic_for_key(method_key)?;
    if !candidates
        .iter()
        .all(|(candidate, implementation)| {
            bodyless_throwable_method(module, candidate, implementation, method_key)
        })
    {
        return None;
    }
    is_throwable_class(module, class_name).then_some(intrinsic)
}

/// Returns the accessor to open-code for an INTERFACE-typed call, or `None` to dispatch.
///
/// The class-typed selector above asks whether the RECEIVER is a `Throwable`; an interface is
/// not a class, so that question has no answer here. The right one is about the candidates:
/// `catch (Throwable $e) { $e->getMessage(); }` can only land on classes that implement the
/// interface, and if every one of them is a bodyless throwable then there is no body to
/// dispatch to for any receiver the call can have. One implementor that overrides the accessor
/// puts a real body back in play and the whole call goes back to ordinary dispatch, exactly as
/// the class-typed rule does.
pub(super) fn interface_throwable_intrinsic(
    module: &Module,
    method_key: &str,
    candidates: &[(String, String)],
) -> Option<ThrowableIntrinsic> {
    let intrinsic = throwable_intrinsic_for_key(method_key)?;
    if candidates.is_empty() {
        return None;
    }
    candidates
        .iter()
        .all(|(candidate, implementation)| {
            bodyless_throwable_method(module, candidate, implementation, method_key)
        })
        .then_some(intrinsic)
}

/// Returns the storage an open-coded `Throwable` accessor pushes.
///
/// The audit checks the destination against this, and the Mixed-receiver path decides from it
/// whether the result needs boxing — so both read the accessor's real shape rather than the
/// signature's declared return type, which for `getPrevious()` is the wider `?Throwable`.
pub(super) fn throwable_intrinsic_storage(
    class_info: &ClassInfo,
    intrinsic: ThrowableIntrinsic,
) -> Result<(IrType, PhpType)> {
    let property = match intrinsic {
        ThrowableIntrinsic::Message => "message",
        ThrowableIntrinsic::Code => "code",
        ThrowableIntrinsic::Previous => "previous",
        ThrowableIntrinsic::File | ThrowableIntrinsic::EmptyString => {
            return Ok((IrType::Str, PhpType::Str))
        }
        ThrowableIntrinsic::Line => return Ok((IrType::I64, PhpType::Int)),
        ThrowableIntrinsic::EmptyTrace => {
            return Ok((
                IrType::Heap(IrHeapKind::Array),
                PhpType::Array(Box::new(PhpType::Mixed)),
            ))
        }
    };
    let (_, _, property_type) = resolve_property_slot(class_info, property)?;
    let property_type = property_type.codegen_repr();
    let ir = match &property_type {
        PhpType::Str => IrType::Str,
        PhpType::Int | PhpType::Bool => IrType::I64,
        PhpType::Float => IrType::F64,
        PhpType::Mixed | PhpType::Union(_) | PhpType::Iterable => IrType::Heap(IrHeapKind::Mixed),
        PhpType::Object(_) => IrType::Heap(IrHeapKind::Object),
        PhpType::Array(_) => IrType::Heap(IrHeapKind::Array),
        PhpType::AssocArray { .. } => IrType::Heap(IrHeapKind::Hash),
        other => {
            return Err(WasmError::Unsupported(format!(
                "${property} of type {other:?} has no wasm32-wasi accessor storage"
            )))
        }
    };
    Ok((ir, property_type))
}

/// Pushes one open-coded `Throwable` accessor's result, leaving an OWNED value on the stack.
///
/// The property-backed accessors go through `emit_declared_property_load`, so a message is
/// persisted and a `$previous` chain is increfed exactly as a `PropGet` of the same slot would
/// be — which is what keeps the caller's release correct.
pub(super) fn emit_throwable_intrinsic(
    ctx: &mut FnCtx,
    obj_ref: &str,
    class_info: &ClassInfo,
    intrinsic: ThrowableIntrinsic,
) -> Result<()> {
    let property = match intrinsic {
        ThrowableIntrinsic::Message => Some("message"),
        ThrowableIntrinsic::Code => Some("code"),
        ThrowableIntrinsic::Previous => Some("previous"),
        ThrowableIntrinsic::File
        | ThrowableIntrinsic::EmptyString
        | ThrowableIntrinsic::Line
        | ThrowableIntrinsic::EmptyTrace => None,
    };
    if let Some(property) = property {
        let (_, offset, property_type) = resolve_property_slot(class_info, property)?;
        let property_type = property_type.codegen_repr();
        // No `acquire` follows a Throwable accessor, and its result outlives the object it read
        // from, so this one owns its copy.
        return emit_declared_property_load(
            ctx,
            obj_ref,
            offset,
            &property_type,
            property,
            PropertyLoad::Owned,
        );
    }
    match intrinsic {
        ThrowableIntrinsic::File => {
            if script_source_file(ctx.module).is_some() {
                ctx.fb.ins(
                    &format!("global.get ${}", SCRIPT_SOURCE_FILE_GLOBAL),
                    "the compiled script's own path",
                );
                ctx.fb.ins(
                    &format!("global.get ${}", SCRIPT_SOURCE_FILE_LEN_GLOBAL),
                    "its length",
                );
                ctx.fb.ins(
                    "call $__rt_str_persist",
                    "hand the caller an owned copy, as getMessage() does",
                );
            } else {
                // Compiled from stdin or an in-memory fragment: php-src answers the empty
                // string for a script with no path of its own, and so does the native backend.
                ctx.fb.ins("i32.const 0", "no source path: empty string pointer");
                ctx.fb.ins("i64.const 0", "no source path: empty string length");
            }
        }
        ThrowableIntrinsic::EmptyString => {
            // A zero-length string needs no data segment: only the length is ever read, and a
            // null pointer is never dereferenced for it.
            ctx.fb.ins("i32.const 0", "empty string pointer");
            ctx.fb.ins("i64.const 0", "empty string length");
        }
        ThrowableIntrinsic::Line => {
            ctx.fb.ins(&format!("local.get {}", obj_ref), "the Throwable being asked");
            ctx.fb.ins(
                "call $__rt_throwable_line",
                "the line this Throwable was constructed on",
            );
        }
        ThrowableIntrinsic::EmptyTrace => {
            ctx.fb.ins("i64.const 0", "empty trace capacity");
            ctx.fb
                .ins("i64.const 16", "default elem_size (specialized on first push)");
            ctx.fb
                .ins("call $__rt_array_new", "allocate the empty backtrace array");
        }
        ThrowableIntrinsic::Message
        | ThrowableIntrinsic::Code
        | ThrowableIntrinsic::Previous => unreachable!("property-backed accessors returned above"),
    }
    Ok(())
}

/// Builds an interface-dispatch stub whose arms OPEN-CODE a bodyless `Throwable` accessor.
///
/// The ordinary stub forwards to one body per class id; there is no body here, so each arm
/// reads the receiver's own slot instead. Per-arm rather than one shared load because the slot
/// OFFSET belongs to the class: two implementors of the same interface can lay `$message` out
/// differently, and a single load would then read one of them at the other's offset.
///
/// One stub, not an inlined ladder per call site — the same reason the ordinary dispatch is a
/// stub. Ownership matches `emit_declared_property_load(.., Owned)` exactly: a string is
/// persisted and a refcounted child is increfed, because the accessor's result outlives the
/// object it was read from and the caller will release it.
pub(super) fn throwable_intrinsic_dispatch_stub(
    module: &Module,
    stub_symbol: &str,
    intrinsic: ThrowableIntrinsic,
    candidates: &[(String, String)],
) -> Result<String> {
    let first = candidates
        .first()
        .ok_or_else(|| WasmError::Unsupported("throwable accessor with no implementor".into()))?;
    let first_info = module
        .class_infos
        .get(&first.0)
        .ok_or_else(|| WasmError::Unsupported(format!("unknown implementor {}", first.0)))?;
    let (result_ir, _) = throwable_intrinsic_storage(first_info, intrinsic)?;
    let results = WasmRepr::val_types(result_ir);

    let mut wat = format!("(func ${stub_symbol} (param $this i32)");
    for ty in &results {
        wat.push_str(&format!(" (result {})", ty.as_str()));
    }
    wat.push_str("\n  (local $cid i64) (local $p i32)\n");

    // Only the three PROPERTY-backed accessors need a ladder. The rest answer from the module
    // or from the receiver's own class metadata, neither of which the static type can change.
    let property = match intrinsic {
        ThrowableIntrinsic::Message => "message",
        ThrowableIntrinsic::Code => "code",
        ThrowableIntrinsic::Previous => "previous",
        ThrowableIntrinsic::File => {
            match script_source_file(module) {
                Some(_) => wat.push_str(&format!(
                    "  (return (call $__rt_str_persist (global.get ${SCRIPT_SOURCE_FILE_GLOBAL}) (global.get ${SCRIPT_SOURCE_FILE_LEN_GLOBAL}))))\n"
                )), // the compiled script's own path, owned by the caller
                None => wat.push_str("  (return (i32.const 0) (i64.const 0)))\n"), // no path to report
            }
            return Ok(wat);
        }
        ThrowableIntrinsic::EmptyString => {
            wat.push_str("  (return (i32.const 0) (i64.const 0)))\n"); // elephc keeps no backtrace
            return Ok(wat);
        }
        ThrowableIntrinsic::Line => {
            wat.push_str("  (return (call $__rt_throwable_line (local.get $this))))\n"); // the construction line
            return Ok(wat);
        }
        ThrowableIntrinsic::EmptyTrace => {
            wat.push_str(
                "  (return (call $__rt_array_new (i64.const 0) (i64.const 16))))\n", // empty backtrace
            );
            return Ok(wat);
        }
    };

    wat.push_str("  (local.set $cid (i64.load (local.get $this)))   ;; runtime class id at +0\n");
    let mut arms: Vec<(u64, String)> = Vec::new();
    for (class_name, _) in candidates {
        let class_info = module
            .class_infos
            .get(class_name)
            .ok_or_else(|| WasmError::Unsupported(format!("unknown implementor {class_name}")))?;
        let (arm_ir, _) = throwable_intrinsic_storage(class_info, intrinsic)?;
        if arm_ir != result_ir {
            return Err(WasmError::Unsupported(format!(
                "implementor {class_name} stores ${property} as {arm_ir:?}, not {result_ir:?}"
            )));
        }
        let (_, offset, property_type) = resolve_property_slot(class_info, property)?;
        let body = match property_type.codegen_repr() {
            PhpType::Int | PhpType::Bool => {
                format!("    (return (i64.load offset={offset} (local.get $this)))")
            }
            PhpType::Float => format!("    (return (f64.load offset={offset} (local.get $this)))"),
            PhpType::Str => format!(
                "    (return (call $__rt_str_persist\n      \
                 (i32.load offset={offset} (local.get $this))\n      \
                 (i64.load offset={} (local.get $this))))",
                offset + 8
            ),
            PhpType::Array(_)
            | PhpType::AssocArray { .. }
            | PhpType::Object(_)
            | PhpType::Mixed
            | PhpType::Union(_)
            | PhpType::Iterable => format!(
                "    (local.set $p (i32.load offset={offset} (local.get $this)))\n    \
                 (call $__rt_incref (local.get $p))\n    (return (local.get $p))"
            ),
            other => {
                return Err(WasmError::Unsupported(format!(
                    "${property} of type {other:?} has no wasm32-wasi accessor storage"
                )))
            }
        };
        arms.push((class_info.class_id, body));
    }
    arms.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));
    for (class_id, body) in &arms {
        wat.push_str(&format!(
            "  (if (i64.eq (local.get $cid) (i64.const {})) (then\n{body}))\n",
            *class_id as i64
        ));
    }
        wat.push_str(
        "  call $__rt_fail_callable_dispatch\n  unreachable ;; elephc-trap:post-noreturn:closed-method-dispatch-failure\n)\n",
    );
    Ok(wat)
}

/// Returns true when `class_name` implements `Throwable`, directly or through its parents.
///
/// The walk is depth-bounded rather than trusting the hierarchy to be acyclic: this runs inside
/// a backend audit, where a malformed module must produce a rejection, not a hang.
pub(super) fn is_throwable_class(module: &Module, class_name: &str) -> bool {
    let mut current = Some(class_name.to_string());
    for _ in 0..64 {
        let Some(name) = current else {
            return false;
        };
        let Some(class_info) = module.class_infos.get(&name) else {
            return false;
        };
        if class_info
            .interfaces
            .iter()
            .any(|interface| interface == "Throwable")
        {
            return true;
        }
        current = class_info.parent.clone();
    }
    false
}

/// Returns every distinct string a class property default will materialize, sorted by content.
///
/// Object construction writes property defaults INLINE rather than calling the class's
/// `_class_propinit_*` function, so a string default reaches `emit_scalar_default` as literal
/// text with no `DataId` behind it — the address it needs cannot come from `str_literals`.
/// `plan_module` lays these out as their own data segments (sharing the segment of an already
/// interned literal with the same bytes) and hands the resulting content-keyed map to every
/// lowered function, which is what lets `emit_scalar_default` resolve a default's address.
///
/// A default that `literal_default_value` rejects is skipped rather than reported: the
/// capability audit refuses that module before planning, so its data would never be addressed.
pub(super) fn literal_default_strings(class_infos: &HashMap<String, ClassInfo>) -> Vec<String> {
    let mut strings: Vec<String> = Vec::new();
    for class_info in class_infos.values() {
        for (index, default) in class_info.defaults.iter().enumerate() {
            let Some(default) = default else {
                continue;
            };
            let Some((property, property_type)) = class_info.properties.get(index) else {
                continue;
            };
            let Ok(literal) = literal_default_value(
                &format!("property ${property}"),
                property_type,
                &default.kind,
                "ObjectNew",
            ) else {
                continue;
            };
            if let LiteralDefaultValue::Str(value) | LiteralDefaultValue::BoxedStr(value) = literal {
                if !strings.contains(&value) {
                    strings.push(value);
                }
            }
        }
        // A static property's string default needs an address for the same reason: its slot
        // bytes carry the literal's pointer, so the literal must be laid out in static data.
        for (index, default) in class_info.static_defaults.iter().enumerate() {
            let Some(default) = default else {
                continue;
            };
            let Some((property, property_type)) = class_info.static_properties.get(index) else {
                continue;
            };
            let Ok(literal) = literal_default_value(
                &format!("static property ${property}"),
                property_type,
                &default.kind,
                "StaticInit",
            ) else {
                continue;
            };
            if let LiteralDefaultValue::Str(value) | LiteralDefaultValue::BoxedStr(value) = literal {
                if !strings.contains(&value) {
                    strings.push(value);
                }
            }
        }
    }
    strings.sort();
    strings
}

/// Emits the per-class gc_desc data (one runtime tag byte per property), the class-indexed
/// pointer table, and the `$__gc_desc_ptrs` / `$__gc_desc_count` globals, then returns the
/// advanced static-data cursor.
///
/// Mirrors the native `_class_gc_desc_ptrs` / `_class_gc_desc_<id>` tables. Each known class id
/// gets a descriptor of exactly `n_properties` tag bytes (a single `0x00` for an empty class,
/// never read since `n = 0`); gap ids in `0..=max_id` point at a generous zero-filled "missing"
/// descriptor so a freed object whose class id lands on a gap reads tag 0 (skip) for every slot
/// without an out-of-bounds load. The pointer table is 4-aligned so its i32 entries load cleanly.
/// `generate()` calls this after the string-literal data and before computing `heap_base`, so
/// the descriptor data lives in static memory below the heap and is never overwritten by
/// allocation.
pub(super) fn emit_gc_desc_table(
    wm: &mut WatModule,
    class_infos: &HashMap<String, ClassInfo>,
    mut cursor: u32,
) -> u32 {
    // 4-align the cursor for the descriptor and pointer-table data that follows.
    cursor = (cursor + 3) & !3;
    if class_infos.is_empty() {
        // No classes: declare empty-table globals so the walk's `cid < count` check is false
        // for every cid and the property walk is skipped.
        wm.add_global(Global {
            name: "__gc_desc_ptrs".to_string(),
            ty: ValType::I32,
            mutable: false,
            init: 0,
        });
        wm.add_global(Global {
            name: "__gc_desc_count".to_string(),
            ty: ValType::I32,
            mutable: false,
            init: 0,
        });
        wm.add_global(Global {
            name: "__gc_desc_meta".to_string(),
            ty: ValType::I32,
            mutable: false,
            init: 0,
        });
        return cursor;
    }
    let mut ordered_classes: Vec<_> = class_infos.iter().collect();
    ordered_classes.sort_by(|(left, left_ci), (right, right_ci)| {
        left_ci
            .class_id
            .cmp(&right_ci.class_id)
            .then_with(|| left.cmp(right))
    });
    let mut id_to_ci: HashMap<u64, &ClassInfo> = HashMap::new();
    for (_, class_info) in &ordered_classes {
        id_to_ci.entry(class_info.class_id).or_insert(class_info);
    }
    let max_id = ordered_classes
        .last()
        .map(|(_, class_info)| class_info.class_id)
        .unwrap_or(0);
    let count = max_id + 1;
    // Missing/gap descriptor: a zero run so a gap class id reads tag 0 (skip) for every slot.
    let missing_off = cursor;
    wm.add_data(DataSegment {
        offset: missing_off,
        bytes: vec![0u8; 64],
    });
    cursor += 64;
    // Per-class descriptor bytes (one runtime tag per property, in declared/parent-first order).
    let mut desc_off: HashMap<u64, u32> = HashMap::new();
    for cid in 0..=max_id {
        match id_to_ci.get(&cid) {
            Some(ci) => {
                let mut bytes = Vec::with_capacity(ci.properties.len().max(1));
                for (name, ty) in &ci.properties {
                    bytes.push(if ci.reference_properties.contains(name) {
                        0
                    } else {
                        // The STORAGE type, not the declared one: `?int` is a `Union` whose
                        // slot is the tagged pair, and reading the declared type sent it to
                        // the boxed arm. The release walk then read the pair's PAYLOAD as a
                        // pointer, and a payload that happened to land inside the live heap
                        // window was freed — which corrupted the free list into a cycle and
                        // hung the next allocation's first-fit search, at 60000 objects.
                        gc_desc_tag(&ty.codegen_repr())
                    });
                }
                if bytes.is_empty() {
                    // Empty class: a single 0x00 byte (n = 0, so the walk never reads it).
                    bytes.push(0);
                }
                wm.add_data(DataSegment {
                    offset: cursor,
                    bytes: bytes.clone(),
                });
                desc_off.insert(cid, cursor);
                cursor += bytes.len() as u32;
            }
            None => {
                desc_off.insert(cid, missing_off);
            }
        }
    }
    // 4-align for the i32 pointer table, then emit one entry per class id in 0..=max_id.
    cursor = (cursor + 3) & !3;
    let ptrs_off = cursor;
    let mut ptrs_bytes = Vec::with_capacity(count as usize * 4);
    for cid in 0..=max_id {
        let off = desc_off.get(&cid).copied().unwrap_or(missing_off);
        ptrs_bytes.extend_from_slice(&off.to_le_bytes());
    }
    wm.add_data(DataSegment {
        offset: ptrs_off,
        bytes: ptrs_bytes,
    });
    cursor += (count * 4) as u32;
    wm.add_global(Global {
        name: "__gc_desc_ptrs".to_string(),
        ty: ValType::I32,
        mutable: false,
        init: ptrs_off as i64,
    });
    wm.add_global(Global {
        name: "__gc_desc_count".to_string(),
        ty: ValType::I32,
        mutable: false,
        init: count as i64,
    });
    // Per-class LAYOUT metadata: the declared property count in the low 16 bits, and whether
    // the class carries a dynamic-property hash tail in bit 16.
    //
    // The release walk used to derive both from the heap block's size. That is wrong, because
    // the allocator reuses a free block WITHOUT splitting it: an object asking for 24 bytes can
    // be served a 152-byte block, and the header keeps 152. The walk then read nine phantom
    // property slots of the previous occupant's bytes and released whatever pointers it found —
    // freeing live blocks. An object's layout is a property of its CLASS, so it is read from
    // here now.
    let meta_off = cursor;
    let mut meta_bytes = Vec::with_capacity(count as usize * 4);
    for cid in 0..=max_id {
        let value = match id_to_ci.get(&cid) {
            Some(ci) => {
                let name = ordered_classes
                    .iter()
                    .find(|(_, candidate)| candidate.class_id == cid)
                    .map(|(name, _)| name.as_str())
                    .unwrap_or("");
                let mut value = ci.properties.len() as u32 & 0xffff;
                if has_dynamic_properties(ci, name) {
                    value |= 1 << 16;
                }
                value
            }
            // A gap class id has no layout; zero properties and no tail is the safe reading.
            None => 0,
        };
        meta_bytes.extend_from_slice(&value.to_le_bytes());
    }
    wm.add_data(DataSegment {
        offset: meta_off,
        bytes: meta_bytes,
    });
    cursor += (count * 4) as u32;
    wm.add_global(Global {
        name: "__gc_desc_meta".to_string(),
        ty: ValType::I32,
        mutable: false,
        init: meta_off as i64,
    });
    cursor
}

/// Maps a property's PHP type to the single-byte runtime tag stored in its gc_desc entry.
///
/// Matches the native `_class_gc_desc_<id>` tag mapping exactly: Int=0, Str=1, Float=2, Bool=3,
/// Array=4, AssocArray=5, Object=6, Mixed/Union/Iterable=7, Resource=9, and 0 for the remaining
/// non-refcounted kinds (Callable/Pointer/Buffer/Packed/Never/Void/Null). `TaggedScalar` maps to
/// 7 (nullable scalars are boxed as Mixed on the wasm target) rather than panicking like the
/// native emitter, which never observes `TaggedScalar` as a declared property type.
/// The descriptor byte for one property's STORAGE type, for the test that pins the set.
///
/// The release walk's rule and the clone's retain rule are both read off these bytes, and a
/// slot mis-tagged as holding a pointer is silent until a payload lands inside the heap
/// window — so the mapping is asserted rather than left to the two readers to agree on.
#[cfg(test)]
pub(super) fn gc_desc_tag_for_test(ty: &PhpType) -> u8 {
    gc_desc_tag(&ty.codegen_repr())
}

/// The descriptor byte for one property's storage type, which the release walk and the clone read
/// to decide whether a slot holds a pointer.
fn gc_desc_tag(ty: &PhpType) -> u8 {
    match ty {
        PhpType::Int => 0,
        PhpType::Str => 1,
        PhpType::Float => 2,
        PhpType::Bool => 3,
        PhpType::Array(_) => 4,
        PhpType::AssocArray { .. } => 5,
        PhpType::Object(_) => 6,
        PhpType::Mixed | PhpType::Union(_) | PhpType::Iterable => 7,
        // A TAGGED slot holds no pointer — its first word is the PAYLOAD, an integer — so it
        // gets a tag of its own rather than sharing the boxed one. Sharing it would have the
        // release walk hand that integer to `__rt_decref_any`, which guards the obviously
        // out-of-range ones and would free a block for any payload that happened to land
        // inside the live heap window.
        PhpType::TaggedScalar => 8,
        PhpType::Resource(_) => 9,
        _ => 0,
    }
}

/// `__rt_decref_object`: the full object release with a gc_desc-driven property walk.
///
/// Mirrors `__rt_object_free_deep` from the native backend (minus the Fiber/Generator/SPL special
/// cases). Guards null / below-first-payload / at-or-after-cursor like `__rt_decref_any`, then a
/// refcount==0 re-entrancy guard. On reaching zero it marks the refcount 0 (so any nested release
/// during the walk is a no-op), calls `__rt_call_object_destructor` to run `__destruct` (built by
/// `emit_destructor_dispatch`, one if-ladder arm per class whose hierarchy declares it) while the
/// properties are still intact, derives the property count `n = (size[ptr-16] - 8) >> 4` from the
/// object's own size header, and — when `class_id < $__gc_desc_count` — walks `i in 0..n` releasing
/// each slot whose desc tag is in {1,4,5,6,7} (str/array/hash/object/mixed) via the null-safe,
/// kind-dispatched `__rt_decref_any`. Resource slots (tag 9) and scalars (tag 0/2/3) are deliberately
/// skipped. Finally the block is freed with `__rt_heap_free` (unsafe, no refcount guard) since the
/// refcount is already 0.
fn rt_decref_object(tracks_handles: bool) -> String {
    // The pool only exists in a module that asks an object for its identity, and WAT requires
    // every call target to be defined, so the release is spliced in rather than always called.
    let forget = if tracks_handles {
        "  (call $__rt_handle_forget (local.get $ptr))  ;; the slot is reusable once the storage is\n"
    } else {
        ""
    };
    format!(r#"(func $__rt_decref_object (param $ptr i32)
  (local $rc i32) (local $n i32) (local $cid i32) (local $desc i32) (local $i i32) (local $tag i32) (local $slot i32) (local $tail_off i32) (local $meta i32) (local $dead i32)
  (if (i32.eqz (local.get $ptr)) (then (return)))                    ;; guard: null pointer
  (if (i32.lt_u (local.get $ptr) (i32.add (global.get $__heap_base) (i32.const 16)))
    (then (return)))                                                  ;; guard: below first payload (borrowed/literal)
  (if (i32.ge_u (local.get $ptr) (global.get $__heap_ptr))
    (then (return)))                                                  ;; guard: at/after bump cursor (not live)
  (local.set $rc (i32.load (i32.sub (local.get $ptr) (i32.const 12))))  ;; refcount = [ptr-12]
  (if (i32.eqz (local.get $rc)) (then (return)))                    ;; guard: refcount == 0 (re-entrancy)
  ;; Bit 30 marks an object whose destructor already ran and then kept it alive (it stored
  ;; $this somewhere). It is counted on the low bits and must not run __destruct a second time.
  (local.set $dead (i32.and (local.get $rc) (i32.const 1073741824)))
  (if (local.get $dead) (then (local.set $rc (i32.and (local.get $rc) (i32.const 1073741823)))))
  (local.set $rc (i32.add (local.get $rc) (i32.const -1)))          ;; rc = rc - 1
  (if (i32.ne (local.get $rc) (i32.const 0)) (then                  ;; keep path: rc > 0, store and return
    (i32.store (i32.sub (local.get $ptr) (i32.const 12)) (i32.or (local.get $rc) (local.get $dead)))  ;; store decremented refcount
    (return)))                                                       ;; keep live and return
  (i32.store (i32.sub (local.get $ptr) (i32.const 12))
             (select (i32.const -2147483648) (i32.const 0) (local.get $dead)))  ;; mark refcount 0 (re-entrancy guard); flag bit 31 = destructor already ran
  (call $__rt_call_object_destructor (local.get $ptr))          ;; run __destruct (if any) before the property walk
  ;; Resurrection: the destructor stored $this somewhere, so references remain on the low bits
  ;; (bit 31 is the in-destructor flag). The object lives on; freeing it here left that reference
  ;; dangling. Keep it, remembering the destructor has run.
  (local.set $rc (i32.and (i32.load (i32.sub (local.get $ptr) (i32.const 12))) (i32.const 2147483647)))
  (if (local.get $rc) (then
    (i32.store (i32.sub (local.get $ptr) (i32.const 12)) (i32.or (local.get $rc) (i32.const 1073741824)))
    (return)))
  (local.set $cid (i32.wrap_i64 (i64.load (local.get $ptr))))    ;; class_id = [ptr+0] (i64 -> i32)
  ;; The layout comes from the CLASS, never from the heap block: the allocator reuses a free
  ;; block WITHOUT splitting it, so an object asking for 24 bytes can be served a 152-byte one
  ;; and the header keeps 152. A size-derived count then walked nine phantom slots of the
  ;; previous occupant and released whatever pointers it found, freeing live blocks.
  (local.set $meta (i32.const 0))                                ;; no class table -> no slots
  (if (i32.lt_u (local.get $cid) (global.get $__gc_desc_count)) (then
    (local.set $meta (i32.load (i32.add (global.get $__gc_desc_meta) (i32.mul (local.get $cid) (i32.const 4)))))))
  (local.set $n (i32.and (local.get $meta) (i32.const 65535)))   ;; declared property count
  (if (i32.lt_u (local.get $cid) (global.get $__gc_desc_count)) (then  ;; class_id within the descriptor table?
    (local.set $desc (i32.load (i32.add (global.get $__gc_desc_ptrs) (i32.mul (local.get $cid) (i32.const 4)))))  ;; desc = ptrs[cid]
    (local.set $i (i32.const 0))                                ;; property index = 0
    (block $walk_end
      (loop $walk
        (br_if $walk_end (i32.ge_u (local.get $i) (local.get $n)))  ;; i >= n -> end walk
        (local.set $tag (i32.load8_u (i32.add (local.get $desc) (local.get $i))))  ;; tag = desc[i]
        (if (i32.or (i32.eq (local.get $tag) (i32.const 1)) (i32.and (i32.ge_u (local.get $tag) (i32.const 4)) (i32.le_u (local.get $tag) (i32.const 7)))) (then  ;; tag in {{1,4,5,6,7}} -> release the slot
          (local.set $slot (i32.load offset=8 (i32.add (local.get $ptr) (i32.mul (local.get $i) (i32.const 16)))))  ;; slot ptr = [ptr + 8 + i*16]
          (call $__rt_decref_any (local.get $slot))            ;; release the child cell (null-safe, kind-dispatched)
        )                                                      ;; close then
        )                                                      ;; close if (tag check)
        (local.set $i (i32.add (local.get $i) (i32.const 1)))   ;; i++
        (br $walk)                                             ;; loop back
      )                                                        ;; close loop $walk
    )                                                          ;; close block $walk_end
  )                                                            ;; close then (cid < count)
  )                                                            ;; close if (cid < count)
  ;; -- P6g: release the dynamic-property hash tail (AllowDynamicProperties / stdClass) --
  ;; The dyn hash lives at [ptr + (size-8)]. When the declared-slot payload is a whole
  ;; number of 16-byte slots, (size-8) & 15 == 0 (no tail); an ADP/stdClass object adds an
  ;; 8-byte tail so (size-8) & 15 == 8. The declared-slot walk above ignores it ((n = (size-8)
  ;; >> 4) truncates the +8), so release the hash here before freeing the storage.
  (local.set $tail_off (i32.add (i32.const 8) (i32.mul (local.get $n) (i32.const 16))))  ;; the tail sits right after the declared slots
  (if (i32.ne (i32.and (local.get $meta) (i32.const 65536)) (i32.const 0)) (then  ;; the CLASS declares a dyn-prop tail
    (call $__rt_decref_any (i32.load (i32.add (local.get $ptr) (local.get $tail_off))))  ;; release the dyn hash at [ptr + (size-8)]
  )                                                              ;; close then (dyn tail)
  )                                                              ;; close if (dyn tail)
{forget}  (call $__rt_heap_free (local.get $ptr))                         ;; free the object storage (unsafe: refcount already 0)
  (return)                                                    ;; top-level return
)                                                              ;; close func
"#)
}

/// `__rt_object_clone_shallow`: php's `clone`, one new block holding the same property values.
///
/// Reads the layout from the same `$__gc_desc` tables the RELEASE walk reads, and that is the
/// whole point of writing it as one helper instead of per-class inline code: the rule "which
/// slots hold a reference" is stated once. A clone that retained a different set from the one
/// `__rt_decref_object` releases would either leak or free a child both objects still point
/// at, and the two cannot drift apart when they read the same descriptor byte.
///
/// `size` comes from the CALLER because the class knows it and the heap block does not: the
/// allocator serves a free block without splitting it, so an object asking for 24 bytes can
/// carry a header saying 152. The declared payload is a whole number of 8-byte words, so the
/// copy is a word loop.
///
/// The dynamic-property tail is the one thing NOT shared. php's clone is shallow in its
/// VALUES — an object property is the same object in both — but writing an undeclared property
/// on the copy must not appear on the original, so the tail gets a table of its own.
///
/// The php HANDLE is not assigned here. A module that never asks for object identity has no
/// pool, and a helper that called into one would reference a function it does not have; the
/// call site registers it instead, under the same gate the allocator uses.
const RT_OBJECT_CLONE_SHALLOW: &str = r#"(func $__rt_object_clone_shallow (param $src i32) (param $size i32) (result i32)
  (local $dst i32) (local $cid i32) (local $meta i32) (local $desc i32)
  (local $n i32) (local $i i32) (local $tag i32) (local $off i32) (local $tail i32)
  (local $slot i32) (local $sptr i32) (local $slen i64)
  (if (i32.eqz (local.get $src))                                 ;; cloning nothing answers nothing
    (then (return (i32.const 0))))
  ;; The caller passes only the EXTRA bytes it knows (the Throwable line word). The rest of the
  ;; size comes from the RUNTIME class: `clone $x` through a base-class-typed receiver copies a
  ;; subclass, whose declared properties extend past the static class's, and sizing the block
  ;; from the static type overran it.
  (local.set $cid (i32.wrap_i64 (i64.load (local.get $src))))
  (local.set $size (i32.add (local.get $size) (i32.const 8)))
  (if (i32.lt_u (local.get $cid) (global.get $__gc_desc_count)) (then
    (local.set $meta
      (i32.load (i32.add (global.get $__gc_desc_meta) (i32.mul (local.get $cid) (i32.const 4)))))
    (local.set $size
      (i32.add (local.get $size)
        (i32.add (i32.mul (i32.and (local.get $meta) (i32.const 65535)) (i32.const 16))
                 (select (i32.const 8) (i32.const 0)
                         (i32.and (local.get $meta) (i32.const 65536))))))))
  (local.set $dst (call $__rt_heap_alloc (local.get $size)))
  (i64.store (i32.sub (local.get $dst) (i32.const 8)) (i64.const 4))  ;; heap kind 4 = object
  (local.set $off (i32.const 0))
  (block $copied (loop $copy                                     ;; the payload, class id included
    (br_if $copied (i32.ge_u (local.get $off) (local.get $size)))
    (i64.store (i32.add (local.get $dst) (local.get $off))
               (i64.load (i32.add (local.get $src) (local.get $off))))
    (local.set $off (i32.add (local.get $off) (i32.const 8)))
    (br $copy)))
  (local.set $cid (i32.wrap_i64 (i64.load (local.get $dst))))    ;; the class came with the copy
  (if (i32.ge_u (local.get $cid) (global.get $__gc_desc_count))  ;; no layout -> nothing to retain
    (then (return (local.get $dst))))
  (local.set $meta
    (i32.load (i32.add (global.get $__gc_desc_meta) (i32.mul (local.get $cid) (i32.const 4)))))
  (local.set $n (i32.and (local.get $meta) (i32.const 65535)))   ;; declared property count
  (local.set $desc
    (i32.load (i32.add (global.get $__gc_desc_ptrs) (i32.mul (local.get $cid) (i32.const 4)))))
  (local.set $i (i32.const 0))
  (block $walked (loop $walk
    (br_if $walked (i32.ge_u (local.get $i) (local.get $n)))
    (local.set $tag (i32.load8_u (i32.add (local.get $desc) (local.get $i))))
    (local.set $slot (i32.add (i32.add (local.get $dst) (i32.const 8))
                              (i32.mul (local.get $i) (i32.const 16))))
    ;; A STRING is copy-on-acquire on this target, not refcounted: `__rt_decref_any` frees a
    ;; kind-1 block outright, so two objects may never name one. The copy takes its own bytes.
    ;; Sharing them instead made `$q->name = "second"` free what `$p->name` still pointed at,
    ;; and the original then read five bytes of the replacement.
    (if (i32.eq (local.get $tag) (i32.const 1)) (then
      (local.set $sptr (i32.wrap_i64 (i64.load (local.get $slot))))
      (if (local.get $sptr) (then
        (call $__rt_str_persist (local.get $sptr) (i64.load offset=8 (local.get $slot)))
        (local.set $slen)                                        ;; the copy's own length
        (local.set $sptr)                                        ;; the copy's own bytes
        (i64.store (local.get $slot) (i64.extend_i32_u (local.get $sptr)))
        (i64.store offset=8 (local.get $slot) (local.get $slen))))))
    ;; Everything else the release walk frees IS refcounted, so the copy takes a share: an
    ;; object property is the same object in both, which is what shallow means.
    (if (i32.and (i32.ge_u (local.get $tag) (i32.const 4))
                 (i32.le_u (local.get $tag) (i32.const 7)))
      (then (call $__rt_incref (i32.wrap_i64 (i64.load (local.get $slot))))))
    (local.set $i (i32.add (local.get $i) (i32.const 1)))
    (br $walk)))
  (if (i32.ne (i32.and (local.get $meta) (i32.const 65536)) (i32.const 0))  ;; a dyn-prop tail
    (then
      (local.set $tail (i32.add (i32.const 8) (i32.mul (local.get $n) (i32.const 16))))
      (i32.store (i32.add (local.get $dst) (local.get $tail))
        (call $__rt_hash_clone_shallow
          (i32.load (i32.add (local.get $src) (local.get $tail)))))))
  (local.get $dst))
"#;

/// `__rt_throwable_line`: reads a Throwable's creation line from its own class's layout.
///
/// The word lives after everything the class declares, so its offset depends on the RUNTIME
/// class, not on whatever type the call site knew the receiver by. `$__gc_desc_meta` already
/// answers that question for the release walk — the declared property count in the low 16 bits,
/// the dynamic-property tail in bit 16 — so this reuses it rather than growing a second table
/// that could disagree with the first.
///
/// A class id outside the table answers zero rather than loading: only a Throwable carries the
/// word, and an id with no class behind it has no layout to trust.
const RT_THROWABLE_LINE: &str = r#"(func $__rt_throwable_line (param $ptr i32) (result i64)
  (local $cid i32) (local $meta i32) (local $off i32)
  (if (i32.eqz (local.get $ptr)) (then (return (i64.const 0))))   ;; guard: null receiver
  (local.set $cid (i32.wrap_i64 (i64.load (local.get $ptr))))     ;; class_id @ O+0
  (if (i32.ge_u (local.get $cid) (global.get $__gc_desc_count))
    (then (return (i64.const 0))))                                ;; no layout for this id
  (local.set $meta
    (i32.load (i32.add (global.get $__gc_desc_meta) (i32.mul (local.get $cid) (i32.const 4)))))
  (local.set $off
    (i32.add (i32.const 8) (i32.mul (i32.and (local.get $meta) (i32.const 65535)) (i32.const 16))))  ;; past the declared slots
  (if (i32.ne (i32.and (local.get $meta) (i32.const 65536)) (i32.const 0))
    (then (local.set $off (i32.add (local.get $off) (i32.const 8)))))  ;; past the dyn-prop tail
  (i64.load (i32.add (local.get $ptr) (local.get $off)))          ;; the creation line, or 0 when unknown
)
"#;

/// Emits `__rt_call_object_destructor`, the per-class `__destruct` dispatch for the
/// wasm32-wasi backend.
///
/// WASM has no `call_reg`, so the closed AOT class set is branched at compile time: the
/// routine is an if-ladder over the runtime `class_id` (read from `[obj+0]`), with one arm
/// per class whose hierarchy declares `__destruct`. The impl class for each arm is
/// resolved via `method_impl_classes.get("__destruct")` — exactly the lookup native
/// `_class_destruct_ptrs` emission uses — so an inherited destructor points at the
/// ancestor's lowered symbol and an override points at the overriding class (most-derived,
/// matching PHP). A class with no `__destruct` in its hierarchy has the key absent and
/// emits no arm, falling through with the refcount untouched (matching native's `fn==0`
/// early return). Arms are sorted by `class_id` for deterministic emission; since each id
/// is unique, order does not affect dispatch (at most one arm matches).
///
/// Reentrancy guard: bit 31 of the refcount (`0x8000_0000`) is the in-destructor flag. It
/// is set INSIDE each matched arm (not in the preamble), so a class with no `__destruct`
/// falls through without mutating its refcount — the 0-arm test stub is therefore a true
/// no-op. The flag is required for a destructor body that creates a new strong reference to
/// `$this` (e.g. `$this->self = $this`, or `$tmp = $this; unset($tmp)`): without it the new
/// ref raises the refcount from 0 to 1, the post-destructor property walk decrefs it back
/// to 0, re-enters the free path, and re-runs the destructor (infinite recursion / stack
/// trap). With the flag the refcount stays in `0x8000_0000+` so the decref lands on the
/// keep path and returns. The existing mark-zero + top `rc==0` guard in
/// `__rt_decref_object` already handle a pre-existing self-cycle; bit 31 handles the
/// new-ref-during-destructor case. `$rc` is captured once before the ladder and reused
/// inside each arm (single-threaded; no intervening store), and `0x8000_0000` is emitted
/// as the signed `i32.const -2147483648`.
///
/// Trade-off: the ladder is O(N) in code size and per-free dispatch time. For a typical
/// PHP module the class count is small; for very large closed sets a `br_table` (dense ids)
/// or a `funcref` table + `call_indirect` would shrink the routine, at the cost of a data
/// segment and an indirection. Kept as a ladder here for parity with the P6d method
/// dispatch stubs.
///
/// `generate()` calls this right after `emit_object_runtime`; unit-test harnesses that
/// emit `emit_object_runtime` must emit `emit_destructor_dispatch_stub` (or this with an
/// empty map) so the `(call $__rt_call_object_destructor ...)` in `RT_DECREF_OBJECT`
/// resolves. Both the arm symbol and the lowered `__destruct` definition are produced by
/// the same category-specific `method_symbol` helper, so they match by construction.
pub(super) fn emit_destructor_dispatch(
    wm: &mut WatModule,
    class_infos: &HashMap<String, ClassInfo>,
) -> Result<()> {
    let destruct_key = php_symbol_key("__destruct");
    let mut arms: Vec<(u64, String)> = Vec::new();
    let mut classes: Vec<_> = class_infos.iter().collect();
    classes.sort_by(|(left, left_ci), (right, right_ci)| {
        left_ci
            .class_id
            .cmp(&right_ci.class_id)
            .then_with(|| left.cmp(right))
    });
    for (_, ci) in classes {
        let Some(impl_class) = ci.method_impl_classes.get(&destruct_key) else {
            continue;
        };
        // The checker guarantees the resolved impl class declares __destruct (self at
        // classes/methods.rs:267-271, ancestor at classes/state.rs:359-360). Validate it
        // anyway so a stale `method_impl_classes` entry surfaces a clean error instead of
        // an undefined-symbol WAT validation failure.
        let declares = class_infos
            .get(impl_class)
            .map(|c| c.methods.contains_key(&destruct_key))
            .unwrap_or(false);
        if !declares {
            return Err(WasmError::Unsupported(format!(
                "class {impl_class} resolved as __destruct impl does not declare it"
            )));
        }
        arms.push((
            ci.class_id,
            method_symbol(&format!("{}::__destruct", impl_class)),
        ));
    }
    arms.sort_by(|left, right| {
        left.0
            .cmp(&right.0)
            .then_with(|| left.1.cmp(&right.1))
    });

    let mut wat = String::new();
    wat.push_str("(func $__rt_call_object_destructor (param $obj i32)\n");
    wat.push_str("  (local $rc i32) (local $cid i64)\n");
    wat.push_str("  ;; null receiver -> nothing to destruct\n");
    wat.push_str("  (if (i32.eqz (local.get $obj)) (then (return)))\n");
    wat.push_str("  ;; reentrancy guard: bit 31 set means already destructing this object\n");
    wat.push_str("  (local.set $rc (i32.load (i32.sub (local.get $obj) (i32.const 12))))\n");
    wat.push_str("  (if (i32.lt_s (local.get $rc) (i32.const 0)) (then (return)))\n");
    wat.push_str("  ;; read the runtime class id from the object payload at +0\n");
    wat.push_str("  (local.set $cid (i64.load (local.get $obj)))\n");
    for (class_id, fn_symbol) in &arms {
        wat.push_str(&format!(
            "  ;; dispatch arm for class id {} -> {}::destructor\n",
            *class_id, fn_symbol
        ));
        wat.push_str(&format!(
            "  (if (i64.eq (local.get $cid) (i64.const {})) (then\n",
            *class_id as i64
        ));
        wat.push_str("    ;; set the in-destructor flag (bit 31) before running the body\n");
        wat.push_str("    (i32.store (i32.sub (local.get $obj) (i32.const 12)) (i32.or (local.get $rc) (i32.const -2147483648)))\n");
        wat.push_str(&format!("    (call ${} (local.get $obj))\n", fn_symbol));
        wat.push_str("    (return)))\n");
    }
    wat.push_str("  ;; no destructor for this class -> return without touching the refcount\n");
    wat.push_str(")\n");
    wm.add_raw_func(&wat);
    Ok(())
}

/// The classes whose `__toString` this target can call, paired with their runtime class id.
///
/// A class qualifies when its hierarchy declares `__toString` AND the resolved implementation
/// has a real body in this module. That last condition is what excludes most of the prelude:
/// `Exception`, `SplFileInfo` and the `Reflection*` family all DECLARE the method, and treating
/// a declaration as evidence would emit a call to a function that was never defined.
pub(super) fn stringable_classes(module: &Module) -> Vec<(u64, String)> {
    let to_string_key = php_symbol_key("__toString");
    let mut arms: Vec<(u64, String)> = Vec::new();
    for (class_name, class_info) in &module.class_infos {
        if class_info.is_abstract {
            continue;
        }
        let Some(impl_class) = class_info
            .method_impl_classes
            .get(&to_string_key)
            .cloned()
            .or_else(|| {
                class_info
                    .methods
                    .contains_key(&to_string_key)
                    .then(|| class_name.clone())
            })
        else {
            continue;
        };
        let has_body = module.class_methods.iter().any(|body| {
            body.name
                .rsplit_once("::")
                .is_some_and(|(owner, method)| {
                    owner == impl_class && php_symbol_key(method) == to_string_key
                })
        });
        if !has_body {
            continue;
        }
        arms.push((
            class_info.class_id,
            method_symbol(&format!("{impl_class}::__toString")),
        ));
    }
    arms.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));
    arms
}

/// Emits `__rt_object_to_string`: PHP's `__toString` dispatch behind every string conversion.
///
/// Returns `(ptr, len, stringable)`. A runtime class with no callable `__toString` answers
/// `(0, 0, 0)` and leaves the diagnostic to the caller, because the two callers raise DIFFERENT
/// errors for it: `(string)$o` is `Error: Object of class C could not be converted to string`,
/// while the same object at a declared `string` parameter is a `TypeError` naming the parameter.
///
/// Without this dispatch the object arm of a boxed string conversion fatally refused EVERY
/// object, including one whose class defines the method — measured against php-src 8.5.6,
/// `(string)$tag` printed `<em>` there and raised here.
pub(super) fn emit_object_to_string_dispatch(wm: &mut WatModule, module: Option<&Module>) {
    let arms = module.map(stringable_classes).unwrap_or_default();
    let mut wat = String::new();
    wat.push_str(
        "(func $__rt_object_to_string (param $obj i32) (result i32) (result i32) (result i32)\n",
    );
    wat.push_str("  (local $cid i64) (local $ptr i32) (local $len i64)\n");
    wat.push_str("  ;; a null receiver is not stringable\n");
    wat.push_str(
        "  (if (i32.eqz (local.get $obj)) (then (return (i32.const 0) (i32.const 0) (i32.const 0))))\n",
    );
    wat.push_str("  (local.set $cid (i64.load (local.get $obj)))\n");
    for (class_id, fn_symbol) in &arms {
        wat.push_str(&format!(
            "  ;; class id {} -> {}\n",
            *class_id, fn_symbol
        ));
        wat.push_str(&format!(
            "  (if (i64.eq (local.get $cid) (i64.const {})) (then\n",
            *class_id as i64
        ));
        wat.push_str(&format!("    (call ${} (local.get $obj))\n", fn_symbol));
        wat.push_str("    (local.set $len)\n");
        wat.push_str("    (local.set $ptr)\n");
        wat.push_str(
            "    (return (local.get $ptr) (i32.wrap_i64 (local.get $len)) (i32.const 1))))\n",
        );
    }
    wat.push_str("  ;; no callable __toString for this runtime class\n");
    wat.push_str("  (i32.const 0) (i32.const 0) (i32.const 0))\n");
    wm.add_raw_func(&wat);
}

/// Declares an empty `__rt_call_object_destructor` (no arms) for unit-test harnesses that
/// register no classes with a destructor.
///
/// With zero arms the routine reads the refcount, returns on the bit-31 reentrancy guard,
/// reads the class id, and falls through without mutating the refcount — a true no-op that
/// lets `RT_DECREF_OBJECT`'s `(call $__rt_call_object_destructor ...)` resolve. Mirrors
/// `emit_gc_desc_stub`: every harness emitting `emit_object_runtime` must emit this (or the
/// real `emit_destructor_dispatch`) alongside `emit_gc_desc_stub`.
#[cfg(test)]
pub(super) fn emit_destructor_dispatch_stub(wm: &mut WatModule) {
    let _ = emit_destructor_dispatch(wm, &HashMap::new());
}

/// Returns the PHP type attached to an SSA value, read from the function's value table.
///
/// The receiver class of a `PropGet`/`PropSet` is resolved from the object operand's
/// `PhpType::Object(name)` here (not from `WasmRepr::Ptr`, which carries only the WAT local
/// name). Clones the type so no borrow on `ctx.function` is held across later `&mut self`
/// lowering calls.
fn value_php_type(ctx: &FnCtx, v: ValueId) -> Result<PhpType> {
    Ok(ctx
        .function
        .value(v)
        .ok_or_else(|| WasmError::Unsupported(format!("no SSA value {:?}", v)))?
        .php_type
        .clone())
}

/// Resolves the `(class_name, ClassInfo)` for a receiver's PHP object type, rejecting
/// non-object receivers with a clean `Unsupported` error. The name is returned alongside the
/// info so callers can run dynamic-property probes that depend on the canonical class name
/// (`stdClass` detection).
pub(super) fn receiver_class_info(ctx: &FnCtx, object: ValueId) -> Result<(String, ClassInfo)> {
    let php_type = value_php_type(ctx, object)?;
    let class_name = match php_type {
        PhpType::Object(name) => name,
        other => {
            return Err(WasmError::Unsupported(format!(
                "property access on non-object receiver {:?}",
                other
            )))
        }
    };
    let ci = ctx
        .module
        .class_infos
        .get(&class_name)
        .cloned()
        .ok_or_else(|| WasmError::Unsupported(format!("unknown class {}", class_name)))?;
    Ok((class_name, ci))
}

/// Looks up a declared property's `(index, byte_offset, php_type)` on `ci`, rejecting
/// undeclared properties (dynamic/`__get` are later sub-phases).
pub(super) fn resolve_property_slot(ci: &ClassInfo, property: &str) -> Result<(usize, usize, PhpType)> {
    let index = ci
        .properties
        .iter()
        .position(|(p, _)| p == property)
        .ok_or_else(|| WasmError::Unsupported(format!("property ${} not declared", property)))?;
    let offset = ci
        .property_offsets
        .get(property)
        .copied()
        .unwrap_or(8 + index * 16);
    Ok((index, offset, ci.properties[index].1.clone()))
}

/// True if `name` is the canonical `stdClass` (bare or backslashed). stdClass is the PHP
/// built-in bag-of-dynamic-properties: it has no declared slots and every property is dynamic.
fn is_stdclass(name: &str) -> bool {
    name == "stdClass" || name == "\\stdClass"
}

/// True if the class carries an 8-byte dynamic-property hash tail: either it is annotated
/// `#[\AllowDynamicProperties]` (`ci.allow_dynamic_properties`) or it is `stdClass`, which is
/// dynamic by definition regardless of the attribute.
fn has_dynamic_properties(ci: &ClassInfo, class_name: &str) -> bool {
    ci.allow_dynamic_properties || is_stdclass(class_name)
}

/// Returns the byte offset of the dynamic-property hash tail (`[obj + off]`) when a property
/// read/write must go through the dyn hash instead of a declared slot.
///
/// `Some(off)` only when the property is NOT one of the declared slots AND the class has a
/// dynamic-property tail. The dyn hash lives immediately after the declared payload:
/// `8 + n*16` (for stdClass `n == 0`, so the whole 8-byte payload is the hash slot). Declared
/// properties and classes without a dyn tail return `None` so the caller falls back to the
/// direct-slot path.
pub(super) fn dynamic_property_hash_offset_for_class(
    ci: &ClassInfo,
    class_name: &str,
    property: &str,
) -> Option<usize> {
    if !has_dynamic_properties(ci, class_name) {
        return None;
    }
    let declared = ci.properties.iter().any(|(p, _)| p == property);
    if declared {
        return None;
    }
    Some(8 + ci.properties.len() * 16)
}

/// Loads the object pointer local ref for `object`, rejecting a non-pointer repr.
pub(super) fn object_ptr_ref(ctx: &FnCtx, object: ValueId) -> Result<String> {
    let repr = ctx.value_repr(object)?.clone();
    match repr {
        WasmRepr::Ptr(name) => Ok(name),
        other => Err(WasmError::Unsupported(format!(
            "object value is not a pointer: {:?}",
            other
        ))),
    }
}

/// Lowers `Op::ObjectNew` to an inline heap allocation followed by a constructor call.
///
/// Allocates `8 + n*16` payload bytes via `__rt_heap_alloc`, stamps heap-kind 4 at `[obj-8]`,
/// writes the compile-time `class_id` at `[obj+0]`, zeroes every property slot, then emits
/// scalar (int/float/bool/null) property defaults. If the class declares `__construct`
/// (`ci.methods[php_symbol_key("__construct")]`), the ctor is resolved (inherited ctors via
/// `ci.method_impl_classes`, defaulting to the class itself) and called AFTER the defaults with
/// `[$this, ...user_args]` — the fresh object pointer is the first arg, matching the native
/// `emit_constructor_call` and the hidden leading `this` param convention. `Op::ObjectNew`
/// operands are the ctor USER args only (the receiver is not in operands; the backend prepends
/// it), mirroring the native `lower_new_object`. Rejects args without a `__construct`, ctor
/// arg-count mismatch, and variadic / by-ref ctor params with `Unsupported`. Classes with a
/// dynamic-property tail (`#[\AllowDynamicProperties]` or `stdClass`) get an extra 8-byte slot
/// after the declared payload holding a `Mixed`-cell hash (`__rt_hash_new(4, 7)`) so undeclared
/// property reads/writes go through the hash. Non-scalar property defaults (string/container/mixed)
/// are a follow-up sub-phase and surface as `Unsupported` from `emit_scalar_default`.
pub(super) fn lower_object_new(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let class_data = data_immediate(inst)?;
    let class_name = ctx
        .module
        .data
        .class_names
        .get(class_data.as_raw() as usize)
        .cloned()
        .ok_or_else(|| WasmError::Unsupported(format!("invalid class data id {:?}", class_data)))?;
    let args = inst.operands.clone();
    emit_object_new_of_class(ctx, inst, &class_name, &args)
}

/// Constructs one instance of a NAMED class and stores it into `inst`'s result.
///
/// Split out of `lower_object_new` so late static binding can reuse it: `new static()` knows the
/// class only from the called-class id its caller forwarded, so it picks the name at run time and
/// then needs exactly this — the allocation, the defaults, and the constructor call — per
/// candidate. The constructor arguments are passed in rather than read off `inst`, because the
/// dynamic form carries the class as its FIRST operand and the arguments after it.
fn emit_object_new_of_class(
    ctx: &mut FnCtx,
    inst: &Instruction,
    class_name: &str,
    args: &[ValueId],
) -> Result<()> {
    let class_name = class_name.to_string();
    let ci = ctx
        .module
        .class_infos
        .get(&class_name)
        .cloned()
        .ok_or_else(|| WasmError::Unsupported(format!("unknown class {}", class_name)))?;
    // Resolve the constructor (case-insensitive key) and the declaring class for an
    // inherited ctor (defaults to self). The call is emitted after the defaults loop;
    // the gate logic moves there so a 0-arg ctor is still called with just `$this`.
    let ctor_key = php_symbol_key("__construct");
    let ctor_sig = ci.methods.get(&ctor_key).cloned();
    let impl_class = ci
        .method_impl_classes
        .get(&ctor_key)
        .cloned()
        .unwrap_or_else(|| class_name.clone());

    let obj = emit_object_allocation(
        ctx,
        &class_name,
        &ci,
        inst.span.map_or(0, |span| span.line),
    )?;

    // -- call __construct (if declared) with [$this, ...user_args]; defaults run first --
    match &ctor_sig {
        None => {
            // No constructor: operands MUST be empty (matches native's args-without-ctor reject).
            if !args.is_empty() {
                return Err(WasmError::Unsupported(format!(
                    "constructor arguments for class {} with no __construct on wasm32-wasi",
                    class_name
                )));
            }
        }
        Some(sig) => {
            if args.len() != sig.params.len() {
                return Err(WasmError::Unsupported(format!(
                    "constructor argument count mismatch for class {} on wasm32-wasi (got {}, expected {})",
                    class_name, args.len(), sig.params.len()
                )));
            }
            if sig.variadic.is_some() {
                return Err(WasmError::Unsupported(format!(
                    "variadic constructor not yet supported on wasm32-wasi for class {}",
                    class_name
                )));
            }
            if sig.ref_params.iter().any(|r| *r) {
                return Err(WasmError::Unsupported(format!(
                    "by-ref constructor parameters not yet supported on wasm32-wasi for class {}",
                    class_name
                )));
            }
            // The ctor symbol is the same `method_symbol("<Class>::__construct")` that
            // `function::lower_function` assigns to the class-method function, so a WAT
            // `call $<symbol>` resolves it. Push the fresh object ptr (`$this`) first
            // (deepest = first param), then each user arg in source order — the stack
            // bottom->top `[obj, arg0, ...]` matches params `[this, ...]`.
            //
            // Arguments go through the typed transfer layer rather than a raw
            // `emit_load_value`, for the same reason a direct call's do: the parameter's
            // representation is the callee's, not the argument's, so a concrete value bound to
            // a `mixed` parameter has to be boxed here. The body's parameters are read for
            // their `IrType`, which the checker-owned signature does not carry.
            if needs_open_coded_throwable_constructor(ctx.module, &class_name, &impl_class) {
                emit_open_coded_throwable_constructor(ctx, &obj, &ci, &class_name, args)?;
                ctx.fb.ins(&format!("local.get {}", obj), "reload object pointer for result store");
                store_result(ctx, inst)?;
                return Ok(());
            }
            let ctor_symbol = method_symbol(&format!("{}::{}", impl_class, "__construct"));
            let ctor_params: Vec<(IrType, PhpType)> = {
                let body = super::methods::find_method_function(
                    &ctx.module.class_methods,
                    &impl_class,
                    &ctor_key,
                )
                .ok_or_else(|| {
                    WasmError::Unsupported(format!(
                        "constructor body {}::__construct is missing on wasm32-wasi",
                        impl_class
                    ))
                })?;
                body.params
                    .iter()
                    .skip(1)
                    .map(|param| (param.ir_type, param.php_type.codegen_repr()))
                    .collect()
            };
            if ctor_params.len() != args.len() {
                return Err(WasmError::Unsupported(format!(
                    "constructor body {}::__construct takes {} arguments, got {}",
                    impl_class,
                    ctor_params.len(),
                    args.len()
                )));
            }
            ctx.fb.ins(&format!("local.get {}", obj), "push $this (fresh object) as first ctor arg");
            let mut boxed_args: Vec<(String, IrType)> = Vec::new();
            for (&arg, (param_ir, param_php)) in args.iter().zip(&ctor_params) {
                if let Some(cell) = super::transfer::emit_push_call_argument(
                    ctx,
                    arg,
                    *param_ir,
                    param_php.clone(),
                )? {
                    boxed_args.push(cell);
                }
            }
            ctx.fb.ins(&format!("call ${}", ctor_symbol), "call ClassName::__construct($this, ...args)");
            // The constructor borrows its parameters and returns nothing, so the values
            // synthesized for them have no other owner and cannot be handed back (see
            // `release_boxed_arguments` in `inst`).
            for (temp, _) in &boxed_args {
                ctx.fb.ins(
                    &format!("(call $__rt_decref_any (local.get {}))", temp),
                    "free the heap value synthesized for this constructor argument",
                );
            }
        }
    }

    // -- store the object pointer into the result value's local --
    ctx.fb.ins(&format!("local.get {}", obj), "reload object pointer for result store");
    store_result(ctx, inst)?;
    Ok(())
}

/// Lowers `new static()` — the only `dynamic_object_new` shape this target admits.
///
/// PHP resolves `static` from the CALLED class, and every static call on this target already
/// forwards that class's id as hidden parameter 0. So there is nothing to look up: the id is a
/// local, and the construction is an if-ladder over the classes that could have been called —
/// the method's own class and its descendants. The native backend goes the long way round
/// (`static::class` -> a class-name string -> `__rt_instanceof_lookup` -> the same ladder)
/// because it also serves `new $name`; this target serves only the late-bound form, where the
/// string is a detour with a table at each end.
///
/// A called-class id no arm matches is a FATAL rather than a silent null: the ladder is built
/// from the module's own class table, so a miss means the table and the call disagree.
pub(super) fn lower_dynamic_object_new(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let class_operand = operand(inst, 0)?;
    if !value_is_late_static_class_name(ctx, class_operand) {
        return Err(WasmError::Unsupported(
            "dynamic object construction from a runtime class-string on wasm32-wasi".to_string(),
        ));
    }
    let required_parent = dynamic_new_required_parent(ctx, inst)?;
    let args = inst.operands[1..].to_vec();
    let candidates = late_static_new_candidates(ctx, &required_parent);
    let [first, rest @ ..] = candidates.as_slice() else {
        return Err(WasmError::Unsupported(format!(
            "new static() in a {required_parent} hierarchy with no constructible class"
        )));
    };

    let cid = ctx.fresh_temp(ValType::I64);
    let slot = ctx
        .function
        .locals
        .iter()
        .find(|local| is_called_class_id_local(local.name.as_deref()))
        .map(|local| local.id)
        .ok_or_else(|| {
            WasmError::Unsupported(
                "new static() outside a function carrying the called-class id".to_string(),
            )
        })?;
    ctx.emit_load_slot(slot)?;
    ctx.fb.ins(
        &format!("local.set {cid}"),
        "the called class, forwarded by every static call",
    );

    // Guard FIRST, arms after: each arm stores the result and falls through to the next test,
    // so a trailing fatal would run on the way out of the matching arm too.
    let mut matches_any = format!("(i64.eq (local.get {cid}) (i64.const {}))", first.0 as i64);
    for (class_id, _) in rest {
        matches_any = format!(
            "(i32.or {matches_any} (i64.eq (local.get {cid}) (i64.const {})))",
            *class_id as i64
        );
    }
    ctx.fb.ins(
        &format!("(if (i32.eqz {matches_any}) (then (call $__rt_fail (i32.const 16)) unreachable))"),
        "elephc-trap:post-noreturn:late-static-new-unresolved",
    );

    for (class_id, class_name) in &candidates {
        ctx.fb.ins(
            &format!("(i64.eq (local.get {cid}) (i64.const {}))", *class_id as i64),
            &format!("was {class_name} the called class?"),
        );
        ctx.fb.ins("if", "construct the called class");
        emit_object_new_of_class(ctx, inst, class_name, &args)?;
        ctx.fb.ins("end", "end of this candidate's arm");
    }
    Ok(())
}

/// Whether a class-name value is the late-bound `static`, rather than a name known here.
fn value_is_late_static_class_name(ctx: &FnCtx, value: ValueId) -> bool {
    let Some(ValueDef::Instruction { inst, .. }) = ctx.function.value(value).map(|v| v.def) else {
        return false;
    };
    let Some(instruction) = ctx.function.instruction(inst) else {
        return false;
    };
    if instruction.op != Op::ConstClassName {
        return false;
    }
    let Some(Immediate::Data(data)) = instruction.immediate else {
        return false;
    };
    ctx.module
        .data
        .class_names
        .get(data.as_raw() as usize)
        .is_some_and(|name| name == "static")
}

/// The class a `dynamic_object_new` result must descend from, from its `"fallback|parent"` data.
fn dynamic_new_required_parent(ctx: &FnCtx, inst: &Instruction) -> Result<String> {
    let Some(Immediate::Data(data)) = inst.immediate else {
        return Err(WasmError::Unsupported(
            "dynamic_object_new without its class-constraint immediate".to_string(),
        ));
    };
    ctx.module
        .data
        .class_names
        .get(data.as_raw() as usize)
        .and_then(|names| names.split_once('|'))
        .map(|(_, required_parent)| required_parent.to_string())
        .ok_or_else(|| {
            WasmError::Unsupported(
                "dynamic_object_new constraint is not a `fallback|parent` pair".to_string(),
            )
        })
}

/// The `(class_id, name)` pairs a `new static()` under `required_parent` can produce.
///
/// Sorted by class id so the emitted ladder is stable across runs, the same ordering every other
/// dispatch here uses.
fn late_static_new_candidates(ctx: &FnCtx, required_parent: &str) -> Vec<(u64, String)> {
    let mut candidates: Vec<(u64, String)> = ctx
        .module
        .class_infos
        .iter()
        .filter(|(class_name, _)| class_descends_from(ctx, class_name, required_parent))
        .map(|(class_name, ci)| (ci.class_id, class_name.clone()))
        .collect();
    candidates.sort();
    candidates
}

/// Whether `class_name` IS `ancestor` or inherits from it, matching PHP's case-insensitive names.
fn class_descends_from(ctx: &FnCtx, class_name: &str, ancestor: &str) -> bool {
    let ancestor_key = php_symbol_key(ancestor.trim_start_matches('\\'));
    let mut current = Some(class_name.to_string());
    while let Some(name) = current {
        if php_symbol_key(name.trim_start_matches('\\')) == ancestor_key {
            return true;
        }
        current = ctx
            .module
            .class_infos
            .get(&name)
            .and_then(|ci| ci.parent.clone());
    }
    false
}

/// Resolves the ADDRESS of a declared property slot on a receiver whose class only run time
/// knows, and returns the local holding it.
///
/// This backend has no `funcref` table, so every dynamic dispatch here is an if-ladder over the
/// runtime `class_id`, and this is that ladder with a slot address as its answer. Two classes may
/// declare the same property at different offsets, which is exactly why the ladder produces an
/// ADDRESS rather than a constant offset the caller could fold.
///
/// They must agree on the declared TYPE, though: the caller reads or writes the slot at one
/// width, and a ladder whose arms disagreed would need a per-arm access rather than a per-arm
/// offset. That case is refused rather than picking one.
///
/// A receiver that is not an object raises php's own `Error`, worded by `on_null_helper` — the
/// verb differs between assigning to a property and aliasing one, and php distinguishes them.
pub(super) fn emit_mixed_receiver_slot_address(
    ctx: &mut FnCtx,
    object: ValueId,
    prop_data: crate::ir::DataId,
    property: &str,
    on_null_helper: &str,
) -> Result<(String, PhpType)> {
    let mut arms: Vec<(u64, String, usize, PhpType)> = Vec::new();
    let mut classes: Vec<_> = ctx.module.class_infos.iter().collect();
    classes.sort_by(|left, right| {
        left.1
            .class_id
            .cmp(&right.1.class_id)
            .then_with(|| left.0.cmp(right.0))
    });
    for (class_name, class_info) in classes {
        if let Ok((_, offset, declared)) = resolve_property_slot(class_info, property) {
            arms.push((
                class_info.class_id,
                class_name.clone(),
                offset,
                declared.codegen_repr(),
            ));
        }
    }
    let Some((_, _, _, declared)) = arms.first().cloned() else {
        return Err(WasmError::Unsupported(format!(
            "property ${property} through a runtime receiver is declared by no compiled class"
        )));
    };
    if let Some((_, class_name, _, other)) = arms
        .iter()
        .find(|(_, _, _, candidate)| *candidate != declared)
    {
        return Err(WasmError::Unsupported(format!(
            "property ${property} is {declared:?} on one class and {other:?} on {class_name}, \
             which a runtime receiver cannot read at one width"
        )));
    }

    let tag = ctx.fresh_temp(ValType::I64);
    let lo = ctx.fresh_temp(ValType::I64);
    let object_ptr = ctx.fresh_temp(ValType::I32);
    let address = ctx.fresh_temp(ValType::I32);
    ctx.emit_load_value(object)?;
    ctx.fb
        .ins("call $__rt_mixed_unbox", "open the receiver -> (tag, lo, hi)");
    ctx.fb.ins("drop", "hi is unused for an object receiver");
    ctx.fb
        .ins(&format!("local.set {}", lo), "object pointer when the tag says object");
    ctx.fb
        .ins(&format!("local.set {}", tag), "receiver runtime tag");
    let (name_ptr, name_len) = ctx.str_literal(prop_data)?;
    ctx.fb.ins(
        &format!(
            "(if (i64.ne (local.get {tag}) (i64.const 6)) (then (call ${on_null_helper} (i32.const {name_ptr}) (i32.const {name_len})) unreachable))"
        ),
        "elephc-trap:post-noreturn:property-on-non-object php names the property it could not reach",
    );
    ctx.fb.ins(
        &format!("(local.set {object_ptr} (i32.wrap_i64 (local.get {lo})))"),
        "the receiver object",
    );
    ctx.fb.ins(
        &format!("(local.set {address} (i32.const 0))"),
        "no arm has matched yet",
    );
    for (class_id, class_name, offset, _) in &arms {
        ctx.fb.ins(
            &format!(
                "(if (i64.eq (i64.load (local.get {object_ptr})) (i64.const {})) (then (local.set {address} (i32.add (local.get {object_ptr}) (i32.const {offset})))))",
                *class_id as i64
            ),
            &format!("{class_name} keeps ${property} at +{offset}"),
        );
    }
    ctx.fb.ins(
        &format!(
            "(if (i32.eqz (local.get {address})) (then (call ${on_null_helper} (i32.const {name_ptr}) (i32.const {name_len})) unreachable))"
        ),
        "elephc-trap:post-noreturn:property-on-non-object an object of a class that does not declare it",
    );
    Ok((address, declared))
}

/// Allocates one instance of `class_name` and returns the local holding its pointer.
///
/// Covers everything a fresh instance needs before any constructor runs: the heap block, the
/// kind-4 header word, the compile-time class id, a zeroed slot per declared property, the
/// property defaults, and the dynamic-property hash tail. Shared by `ObjectNew` and by the
/// runtime-error raise sites, so an object built by a division-by-zero guard is byte-identical
/// to one built by `new` — including the gc_desc contract its release path depends on.
///
/// `creation_line` is the ONE-BASED source line of the `new` expression, and `0` when there is
/// no user `new` behind the object — a `DivisionByZeroError` raised by a guard, say. It is
/// recorded only for a Throwable, which is the only class whose construction site PHP lets a
/// program read back, and it is written UNCONDITIONALLY even when zero: `__rt_heap_alloc`
/// recycles blocks without zeroing them, so an unwritten word would hand `getLine()` whatever
/// the previous occupant left there.
/// Lowers `Op::ObjectCloneShallow` — php's `clone $obj`.
///
/// The runtime helper does the work, because the rule it needs — which slots hold a reference
/// — is the one `__rt_decref_object` already reads out of the class descriptor. What the call
/// site adds is the two things only the compiler knows: the class's exact payload size, and
/// whether this module tracks object identity, in which case the copy is a NEW object and php
/// gives it its own `spl_object_id`.
///
/// `__clone()` is not called here. php runs it after the copy exists, and the EIR emits that
/// method call as its own instruction — the same split the native backend uses.
pub(super) fn lower_object_clone_shallow(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let source = super::inst::operand(inst, 0)?;
    let class_data = data_immediate(inst)?;
    let class_name = ctx
        .module
        .data
        .class_names
        .get(class_data.as_raw() as usize)
        .cloned()
        .ok_or_else(|| WasmError::Unsupported(format!("invalid class data id {:?}", class_data)))?;
    if !ctx.module.class_infos.contains_key(&class_name) {
        return Err(WasmError::Unsupported(format!("clone of unknown class {class_name}")));
    }
    // Only the Throwable line word is a static fact; the declared slots are sized at run time
    // from the receiver's own class, because the static class may be a base of it.
    let size = if is_throwable_class(ctx.module, &class_name) { 8 } else { 0 };

    // `__clone()` is a METHOD CALL in the EIR, and a method call's receiver is a borrow the
    // caller releases afterwards — so the EIR emits that release against this very result and
    // then re-acquires it for the assignment. A value that arrived with one reference is
    // therefore momentarily at zero, and on this target zero means FREED: `__rt_heap_free`
    // writes the free-list link over the payload's first word, so
    // `class A { public string $note = "orig"; public function __clone(): void {
    // $this->note = "copied"; } }` read back four bytes of link and "ed". The native backend
    // survives the same EIR shape by accident, its free not poisoning the block.
    //
    // So the call site hands over the reference that call will consume, the way a returned
    // parameter slot does. Only when such a call exists: an ordinary `clone` with no
    // `__clone()` gets no release to pay for, and an unconditional retain would leak the copy.
    let result = inst
        .result
        .ok_or_else(|| WasmError::Unsupported("object_clone_shallow without a result".into()))?;
    let feeds_a_method_call = ctx.function.instructions.iter().any(|candidate| {
        matches!(
            candidate.op,
            Op::MethodCall | Op::NullsafeMethodCall
        ) && candidate.operands.first() == Some(&result)
    });

    ctx.emit_load_value(source)?;
    ctx.fb
        .ins(&format!("i32.const {size}"), "extra bytes beyond the runtime class's slots");
    ctx.fb.ins(
        "call $__rt_object_clone_shallow",
        "php's shallow clone: same values, its own block",
    );
    if feeds_a_method_call {
        let copy = ctx.fresh_temp(ValType::I32);
        ctx.fb.ins(&format!("local.tee {copy}"), "keep the copy on the stack");
        ctx.fb.ins(
            "call $__rt_incref",
            "the reference __clone()'s receiver release will consume",
        );
        ctx.fb.ins(&format!("local.get {copy}"), "the copy");
    }
    if super::handles::module_uses_object_handles(ctx.module) {
        let copy = ctx.fresh_temp(ValType::I32);
        ctx.fb.ins(&format!("local.tee {copy}"), "keep the copy on the stack");
        ctx.fb.ins(
            "call $__rt_handle_register",
            "the copy is a new object and gets its own php handle",
        );
        ctx.fb.ins("drop", "the pool holds it; the object carries nothing");
        ctx.fb.ins(&format!("local.get {copy}"), "the copy");
    }
    super::inst::store_result(ctx, inst)
}

/// Emits the inline allocation of an object of `class_name`: sizes the payload from its
/// properties (plus a dynamic-property tail where allowed), stamps the heap kind and class id,
/// zeroes the slots and records the creation line for throwables. Answers the local holding the
/// new pointer.
pub(super) fn emit_object_allocation(
    ctx: &mut FnCtx,
    class_name: &str,
    ci: &ClassInfo,
    creation_line: u32,
) -> Result<String> {
    let n = ci.properties.len();
    let has_dyn = has_dynamic_properties(ci, class_name);
    let dyn_off = if has_dyn { Some(8 + (n as i32) * 16) } else { None };
    let line_off = is_throwable_class(ctx.module, class_name)
        .then(|| throwable_creation_line_offset(ci, class_name));
    let payload_size: i32 = 8
        + (n as i32) * 16
        + if has_dyn { 8 } else { 0 }
        + if line_off.is_some() { 8 } else { 0 };
    let class_id = ci.class_id as i64;

    // -- allocate the object block --
    ctx.fb.ins(&format!("i32.const {}", payload_size), "object payload size in bytes");
    ctx.fb.ins("call $__rt_heap_alloc", "allocate object block -> ptr (i32)");
    let obj = ctx.fresh_temp(ValType::I32);
    ctx.fb.ins(&format!("local.set {}", obj), "capture fresh object pointer");

    // -- stamp the heap header kind word (kind 4 = object) at [obj-8] --
    ctx.fb.ins(&format!("local.get {}", obj), "object base address");
    ctx.fb.ins("i32.const 8", "header kind word offset (ptr - 8)");
    ctx.fb.ins("i32.sub", "address of the kind word");
    ctx.fb.ins("i64.const 4", "heap kind 4 = object instance");
    ctx.fb.ins("i64.store", "stamp the heap header kind word");

    // -- write the compile-time class id at [obj+0] --
    ctx.fb.ins(&format!("local.get {}", obj), "object base address");
    ctx.fb.ins(&format!("i64.const {}", class_id), "compile-time class id");
    ctx.fb.ins("i64.store", "store class id at object payload offset zero");

    // -- give the object its php handle, for a module that asks one for its identity --
    // At ALLOCATION rather than at the first `spl_object_id()`, because php assigns it here:
    // `$a = new A(); $b = new A(); spl_object_id($b)` is 2, and a pool filled on demand would
    // answer 1 for whichever object asked first. The handle itself is discarded — the pool is
    // what remembers it, keyed by this pointer.
    if super::handles::module_uses_object_handles(ctx.module) {
        ctx.fb.ins(&format!("local.get {}", obj), "object base address");
        ctx.fb.ins(
            "call $__rt_handle_register",
            "assign this object's php handle",
        );
        ctx.fb.ins("drop", "the pool holds it; the object carries nothing");
    }

    // -- zero every property slot (value_lo and the tag value_hi) --
    for i in 0..n {
        let off = 8 + i * 16;
        ctx.fb.ins(&format!("local.get {}", obj), "object base address");
        ctx.fb.ins("i64.const 0", "zero");
        ctx.fb.ins(&format!("i64.store offset={}", off), "zero property slot value_lo");
        ctx.fb.ins(&format!("local.get {}", obj), "object base address");
        ctx.fb.ins("i64.const 0", "zero");
        ctx.fb.ins(&format!("i64.store offset={}", off + 8), "zero property slot value_hi (tag)");
    }

    // -- emit concrete or boxed scalar property defaults; non-scalars remain unsupported --
    for i in 0..n {
        let Some(Some(expr)) = ci.defaults.get(i) else {
            continue;
        };
        let prop_name = ci.properties[i].0.clone();
        let prop_type = ci.properties[i].1.clone();
        // PHP's implicit `= null` on an untyped property is unobservable when the constructor
        // always overwrites it, and it has no representation once the checker has narrowed the
        // slot to what the constructor stores. The audit skips it on the same terms; the two
        // must agree, or planning fails on a module the audit passed.
        if super::capability::null_default_is_overwritten_by_the_constructor(
            ctx.module,
            &class_name,
            &prop_name,
            &expr.kind,
        ) {
            continue;
        }
        let off = ci
            .property_offsets
            .get(&prop_name)
            .copied()
            .unwrap_or(8 + i * 16);
        let lit = literal_default_value(
            &format!("property ${}", prop_name),
            &prop_type,
            &expr.kind,
            "ObjectNew",
        )
        .map_err(|e| WasmError::Unsupported(e.to_string()))?;
        emit_scalar_default(ctx, &obj, off, &lit, &prop_name)?;
    }

    // -- initialise the dynamic-property hash tail (ADP / stdClass) --
    // An extra 8-byte slot after the declared payload holds an i32 ptr to a Mixed-cell
    // hash (capacity 4, value_tag 7). Undeclared reads/writes go through it; the tail is
    // released by __rt_decref_object when (size-8) & 15 == 8. stdClass has no declared
    // properties, so dyn_off == 8 and the whole payload is the 8-byte hash slot.
    if let Some(off) = dyn_off {
        ctx.fb.ins("i64.const 4", "dynamic-property hash capacity (4 entries)");
        ctx.fb.ins("i64.const 7", "dynamic-property hash value tag (7 = mixed cell)");
        ctx.fb.ins("call $__rt_hash_new", "allocate the dyn-prop mixed hash -> ptr (i32)");
        let dyn_hash = ctx.fresh_temp(ValType::I32);
        ctx.fb.ins(&format!("local.set {}", dyn_hash), "capture dyn-prop hash pointer");
        ctx.fb.ins(&format!("local.get {}", obj), "object base address (store addr)");
        ctx.fb.ins(&format!("local.get {}", dyn_hash), "dyn-prop hash ptr (value)");
        ctx.fb.ins("i64.extend_i32_u", "widen the hash ptr to i64 for storage");
        ctx.fb.ins(&format!("i64.store offset={}", off), "store the dyn-prop hash ptr at [obj + dyn_off]");
    }

    // -- stamp the Throwable creation line past everything the class declares --
    // `__rt_throwable_line` recomputes this same offset from the class's gc_desc metadata, so
    // the two agree by construction: both read the declared property count and the dyn-prop
    // flag off the one `ClassInfo`.
    if let Some(off) = line_off {
        ctx.fb.ins(&format!("local.get {}", obj), "object base address");
        ctx.fb.ins(
            &format!("i64.const {}", creation_line),
            "the source line this Throwable was constructed on (0 = unknown)",
        );
        ctx.fb.ins(
            &format!("i64.store offset={}", off),
            "store the creation line past the declared payload",
        );
    }

    Ok(obj)
}

/// PHP's Error class and message for each runtime failure reference PHP makes CATCHABLE.
///
/// Indexed by the `__rt_fail` diagnostic code the same failure carries when it reaches the top
/// of `main` uncaught, so one table pins both halves of the behaviour together: the class a
/// `catch` clause matches, and the fatal text printed when no clause matches. `runtime.rs`'s
/// `ERR_*` strings are the second half and must keep naming these same classes and messages.
pub(super) const CATCHABLE_RUNTIME_ERRORS: [(i32, &str, &str); 25] = [
    (1, "DivisionByZeroError", "Division by zero"),
    (2, "DivisionByZeroError", "Modulo by zero"),
    (3, "ArithmeticError", "Bit shift by negative number"),
    (
        4,
        "ArithmeticError",
        "Division of PHP_INT_MIN by -1 is not an integer",
    ),
    // Code 9 is the generic "Mixed cell does not fit its destination" failure. Most of its raise
    // sites still report it directly; the `?int` unbox raises it through
    // `emit_runtime_failure`, so a `catch (TypeError)` receives it.
    (9, "TypeError", "Value does not match the required heap type"),
    (
        11,
        "ValueError",
        "str_repeat(): Argument #2 ($times) must be greater than or equal to 0",
    ),
    (
        12,
        "ValueError",
        "str_pad(): Argument #3 ($pad_string) must not be empty",
    ),
    (
        13,
        "ValueError",
        "explode(): Argument #1 ($separator) must not be empty",
    ),
    (
        14,
        "ValueError",
        "str_split(): Argument #2 ($length) must be greater than 0",
    ),
    (
        17,
        "ValueError",
        "stream_set_chunk_size(): Argument #2 ($size) must be greater than 0",
    ),
    (18, "ValueError", "bcmath: an argument is not well-formed"),
    (
        20,
        "Exception",
        "Cannot rewind a generator that was already run",
    ),
    (
        21,
        "TypeError",
        "fclose(): Argument #1 ($stream) must be an open stream resource",
    ),
    (
        22,
        "ValueError",
        "stream_set_chunk_size(): Argument #2 ($size) is too large",
    ),
    (
        23,
        "ValueError",
        "bcscale(): Argument #1 ($scale) must be between 0 and 2147483647",
    ),
    (
        24,
        "ValueError",
        "bcmath: scale must be between 0 and 2147483647",
    ),
    (
        25,
        "Exception",
        "Cannot get return value of a generator that hasn't returned",
    ),
    (
        27,
        "Exception",
        "Cannot traverse an already closed generator",
    ),
    (
        32,
        "ValueError",
        "array_pad(): Argument #2 ($length) must not exceed the maximum allowed array size",
    ),
    (33, "TypeError", "Unsupported operand types: null + array"),
    (34, "TypeError", "Unsupported operand types: array + null"),
    (
        35,
        "TypeError",
        "array_merge(): Argument #1 must be of type array, null given",
    ),
    (
        36,
        "TypeError",
        "array_merge(): Argument #2 must be of type array, null given",
    ),
    (
        37,
        "TypeError",
        "array_replace(): Argument #1 ($array) must be of type array, null given",
    ),
    (
        38,
        "TypeError",
        "array_replace(): Argument #2 must be of type array, null given",
    ),
];

/// Returns the Error class and message a runtime failure code raises, when PHP makes it catchable.
pub(super) fn catchable_runtime_error(failure_code: i32) -> Option<(&'static str, &'static str)> {
    CATCHABLE_RUNTIME_ERRORS
        .iter()
        .find(|(code, _, _)| *code == failure_code)
        .map(|(_, class_name, message)| (*class_name, *message))
}

/// Returns whether this module can build `class_name` inline at a runtime-error raise site.
///
/// A raise site has no EIR instruction behind it, so an `Unsupported` there would reject a
/// program that compiles today. The construction is therefore proven possible BEFORE any WAT is
/// written, and a class this backend cannot build inline falls back to the deterministic fatal —
/// less faithful to PHP, but never a new compile failure and never a half-emitted object.
pub(super) fn runtime_error_is_constructible(module: &Module, class_name: &str) -> bool {
    let Some(class_info) = module.class_infos.get(class_name) else {
        return false;
    };
    if !is_throwable_class(module, class_name) {
        return false;
    }
    if has_dynamic_properties(class_info, class_name) {
        return false;
    }
    if !matches!(
        resolve_property_slot(class_info, "message"),
        Ok((_, _, PhpType::Str))
    ) {
        return false;
    }
    class_info
        .defaults
        .iter()
        .enumerate()
        .all(|(index, default)| {
            let Some(default) = default else {
                return true;
            };
            let Some((property, property_type)) = class_info.properties.get(index) else {
                return false;
            };
            literal_default_value(
                &format!("property ${property}"),
                property_type,
                &default.kind,
                "ObjectNew",
            )
            .is_ok()
        })
}

/// Builds the PHP Error object for a runtime failure and raises it as an ordinary exception.
///
/// This is what makes `try { intdiv(1, 0); } catch (\DivisionByZeroError $e)` behave as PHP
/// specifies rather than killing the process: the guard raises the same tag a `throw` does, so
/// every landing pad already in place receives it and picks its matching clause. The failure
/// code travels alongside in `EXCEPTION_FATAL_CODE_GLOBAL`, so an error that nobody catches
/// still prints the class-and-message fatal it printed when the guard exited directly.
///
/// The caller must have proven `runtime_error_is_constructible` for this class and that the
/// module declares the exception tag; `inst::emit_runtime_failure` is the only such caller.
pub(super) fn emit_runtime_error_throw(
    ctx: &mut FnCtx,
    class_name: &str,
    message: &str,
    failure_code: i32,
) -> Result<()> {
    let class_info = ctx
        .module
        .class_infos
        .get(class_name)
        .cloned()
        .ok_or_else(|| {
            WasmError::Unsupported(format!("runtime error class {} is unknown", class_name))
        })?;
    // No user `new` stands behind a runtime failure, and php-src would report its own internal
    // origin rather than anything this emitter knows, so the creation line is left unknown.
    let obj = emit_object_allocation(ctx, class_name, &class_info, 0)?;

    // The defaults loop already wrote an owned empty string into `$message`, so the slot is
    // released before the real message replaces it — the same order `PropSet` uses, and the
    // reason a raise site does not leak the default it overwrites.
    let (_, offset, _) = resolve_property_slot(&class_info, "message")?;
    ctx.fb.ins(&format!("local.get {}", obj), "object base address");
    ctx.fb.ins(&format!("i32.load offset={}", offset), "load the default message pointer");
    ctx.fb.ins("call $__rt_decref_any", "release the default message (null-safe)");
    emit_scalar_default(
        ctx,
        &obj,
        offset,
        &LiteralDefaultValue::Str(message.to_string()),
        "message",
    )?;
    emit_raise_prepared_error(ctx, &obj, failure_code);
    Ok(())
}

/// Builds a PHP Error whose message is only known at RUN time, and raises it.
///
/// The twin of `emit_runtime_error_throw` for a message the emitter cannot spell: php-src's
/// `ValueError` for `Enum::from()` names the offending value, so the text is a concatenation
/// rather than a constant, and the fixed-message path has nowhere to put it.
///
/// The caller leaves an OWNED `(pointer, length)` on the stack; the slot takes that reference
/// rather than persisting a second copy, exactly as a string property store does.
pub(super) fn emit_runtime_error_throw_of_computed_message(
    ctx: &mut FnCtx,
    class_name: &str,
    failure_code: i32,
) -> Result<()> {
    let class_info = ctx
        .module
        .class_infos
        .get(class_name)
        .cloned()
        .ok_or_else(|| {
            WasmError::Unsupported(format!("runtime error class {} is unknown", class_name))
        })?;
    // The message is on the stack before anything else runs, and the allocation below emits
    // its own loads and stores — so it is parked in locals first rather than left underneath.
    let length = ctx.fresh_temp(ValType::I64);
    let pointer = ctx.fresh_temp(ValType::I32);
    ctx.fb
        .ins(&format!("local.set {}", length), "the computed message length");
    ctx.fb
        .ins(&format!("local.set {}", pointer), "the computed message bytes");

    let obj = emit_object_allocation(ctx, class_name, &class_info, 0)?;
    let (_, offset, _) = resolve_property_slot(&class_info, "message")?;
    ctx.fb.ins(&format!("local.get {}", obj), "object base address");
    ctx.fb.ins(
        &format!("i32.load offset={}", offset),
        "load the default message pointer",
    );
    ctx.fb
        .ins("call $__rt_decref_any", "release the default message (null-safe)");
    ctx.fb.ins(&format!("local.get {}", obj), "object base address");
    ctx.fb
        .ins(&format!("local.get {}", pointer), "the computed message bytes");
    ctx.fb.ins("i64.extend_i32_u", "widen the pointer for the slot");
    ctx.fb.ins(
        &format!("i64.store offset={}", offset),
        "store the message pointer",
    );
    ctx.fb.ins(&format!("local.get {}", obj), "object base address");
    ctx.fb
        .ins(&format!("local.get {}", length), "the computed message length");
    ctx.fb.ins(
        &format!("i64.store offset={}", offset + 8),
        "store the message length",
    );

    emit_raise_prepared_error(ctx, &obj, failure_code);
    Ok(())
}

/// Raises an Error object whose message slot is already written.
///
/// Shared so the two builders cannot disagree about what "raised" means: the same tag a `throw`
/// uses, so every landing pad already in place receives it, with the failure code travelling
/// alongside for the fatal an uncaught one prints.
fn emit_raise_prepared_error(ctx: &mut FnCtx, obj: &str, failure_code: i32) {
    ctx.fb.ins(&format!("local.get {}", obj), "the error being raised");
    ctx.fb.ins(
        &format!("i32.const {}", failure_code),
        "diagnostic this error prints if nothing catches it",
    );
    ctx.fb.ins(
        &format!(
            "global.set ${}",
            super::function::EXCEPTION_FATAL_CODE_GLOBAL
        ),
        "claim the uncaught diagnostic for this error",
    );
    ctx.fb.ins(
        &format!("throw ${}", super::function::EXCEPTION_TAG),
        "raise the PHP runtime error",
    );
}

/// Writes one scalar or string property default into the object slot at `offset`.
///
/// Int/Bool store the value as i64 and zero the tag word; Float stores the raw f64 bits as i64
/// (read back by `f64.load`) and zeroes the tag; Null leaves the slot zeroed. Boxed scalar/null
/// variants allocate an owned Mixed cell and stamp tag 7 in the property slot. A string default
/// resolves its bytes by content (they carry no `DataId` at a construction site) and stores an
/// OWNED heap copy, because the slot's gc_desc tag makes the object's release path decref it.
/// Arrays and sentinels remain unsupported.
pub(super) fn emit_scalar_default(
    ctx: &mut FnCtx,
    obj: &str,
    offset: usize,
    lit: &LiteralDefaultValue,
    prop_name: &str,
) -> Result<()> {
    match lit {
        LiteralDefaultValue::Int(v) => {
            ctx.fb.ins(&format!("local.get {}", obj), "object base address");
            ctx.fb.ins(&format!("i64.const {}", v), "int default value");
            ctx.fb.ins(&format!("i64.store offset={}", offset), "store int default value_lo");
            ctx.fb.ins(&format!("local.get {}", obj), "object base address");
            ctx.fb.ins("i64.const 0", "zero tag word");
            ctx.fb.ins(&format!("i64.store offset={}", offset + 8), "store int default value_hi (tag = 0)");
        }
        LiteralDefaultValue::Bool(b) => {
            let v: i64 = i64::from(*b);
            ctx.fb.ins(&format!("local.get {}", obj), "object base address");
            ctx.fb.ins(&format!("i64.const {}", v), "bool default value");
            ctx.fb.ins(&format!("i64.store offset={}", offset), "store bool default value_lo");
            ctx.fb.ins(&format!("local.get {}", obj), "object base address");
            ctx.fb.ins("i64.const 0", "zero tag word");
            ctx.fb.ins(&format!("i64.store offset={}", offset + 8), "store bool default value_hi (tag = 0)");
        }
        LiteralDefaultValue::Float(f) => {
            let bits = f.to_bits() as i64;
            ctx.fb.ins(&format!("local.get {}", obj), "object base address");
            ctx.fb.ins(&format!("i64.const {}", bits), "float default bits");
            ctx.fb.ins(&format!("i64.store offset={}", offset), "store float default value_lo");
            ctx.fb.ins(&format!("local.get {}", obj), "object base address");
            ctx.fb.ins("i64.const 0", "zero tag word");
            ctx.fb.ins(&format!("i64.store offset={}", offset + 8), "store float default value_hi (tag = 0)");
        }
        LiteralDefaultValue::Null => {
            // The zeroing loop already wrote (0, 0); skip.
        }
        // A TAGGED slot's null is not the zeroed slot: tag 0 is the INTEGER tag, so leaving
        // the zeros would make `public ?int $v = null;` read back as `0`. The same trap the
        // static-property layout has, and the same answer.
        LiteralDefaultValue::TaggedNull => {
            ctx.fb.ins(&format!("local.get {}", obj), "object base address");
            ctx.fb.ins("i64.const 0", "a null carries no payload");
            ctx.fb
                .ins(&format!("i64.store offset={}", offset), "store the payload");
            ctx.fb.ins(&format!("local.get {}", obj), "object base address");
            ctx.fb.ins("i64.const 8", "php null's Mixed tag");
            ctx.fb.ins(
                &format!("i64.store offset={}", offset + 8),
                "store the tag, which is what makes this null and not zero",
            );
        }
        LiteralDefaultValue::TaggedInt(v) => {
            ctx.fb.ins(&format!("local.get {}", obj), "object base address");
            ctx.fb
                .ins(&format!("i64.const {}", v), "tagged int default payload");
            ctx.fb
                .ins(&format!("i64.store offset={}", offset), "store the payload");
            ctx.fb.ins(&format!("local.get {}", obj), "object base address");
            ctx.fb.ins("i64.const 0", "the integer tag");
            ctx.fb
                .ins(&format!("i64.store offset={}", offset + 8), "store the tag");
        }
        LiteralDefaultValue::Array { elements, .. } if elements.is_empty() => {
            ctx.fb.ins(&format!("local.get {}", obj), "object base address");
            ctx.fb.ins("i64.const 0", "capacity 0");
            ctx.fb.ins("i64.const 16", "default elem_size");
            ctx.fb.ins("call $__rt_array_new", "fresh empty array for the default");
            ctx.fb.ins("i64.extend_i32_u", "widen the pointer");
            ctx.fb.ins(&format!("i64.store offset={}", offset), "store the pointer");
            ctx.fb.ins(&format!("local.get {}", obj), "object base address");
            ctx.fb.ins("i64.const 4", "container runtime tag");
            ctx.fb.ins(&format!("i64.store offset={}", offset + 8), "store the tag");
        }
        // A declared `array` property is a boxed value, so its `= []` is a Mixed cell holding a
        // fresh empty list. The cell increfs the list it is built from, so the construction's
        // own reference is dropped once the cell holds one.
        LiteralDefaultValue::BoxedArray { elements, .. } if elements.is_empty() => {
            let array = ctx.fresh_temp(ValType::I32);
            ctx.fb.ins("i64.const 4", "mixed tag (indexed array)");
            ctx.fb.ins("(call $__rt_array_new (i64.const 0) (i64.const 16))", "fresh empty list");
            ctx.fb.ins(&format!("local.tee {array}"), "keep it to drop our reference");
            ctx.fb.ins("i64.extend_i32_u", "the list pointer -> lo");
            ctx.fb.ins("i64.const 0", "hi unused");
            ctx.fb.ins("call $__rt_mixed_from_value", "box the list (increfs it)");
            let cell = ctx.fresh_temp(ValType::I32);
            ctx.fb.ins(&format!("local.set {cell}"), "the property's own cell");
            ctx.fb.ins(
                &format!("(call $__rt_decref_any (local.get {array}))"),
                "the cell holds the list now",
            );
            ctx.fb.ins(&format!("local.get {}", obj), "object base address");
            ctx.fb.ins(&format!("local.get {cell}"), "boxed array default cell");
            ctx.fb.ins("i64.extend_i32_u", "widen mixed cell pointer");
            ctx.fb.ins(&format!("i64.store offset={}", offset), "store mixed property value_lo");
            ctx.fb.ins(&format!("local.get {}", obj), "object base address");
            ctx.fb.ins("i64.const 7", "mixed property runtime tag");
            ctx.fb.ins(&format!("i64.store offset={}", offset + 8), "store mixed property value_hi");
        }
        LiteralDefaultValue::BoxedNull => {
            emit_boxed_scalar_default(ctx, obj, offset, 8, 0, "null");
        }
        LiteralDefaultValue::BoxedInt(value) => {
            emit_boxed_scalar_default(ctx, obj, offset, 0, *value, "int");
        }
        LiteralDefaultValue::BoxedBool(value) => {
            emit_boxed_scalar_default(ctx, obj, offset, 3, i64::from(*value), "bool");
        }
        LiteralDefaultValue::BoxedFloat(value) => {
            emit_boxed_scalar_default(
                ctx,
                obj,
                offset,
                2,
                value.to_bits() as i64,
                "float",
            );
        }
        LiteralDefaultValue::Str(value) => {
            // The slot owns its string exactly like a `PropSet`-written one: the gc_desc tag
            // for a Str property is 1, so `__rt_decref_object` will release this pointer when
            // the object dies. It must therefore be an OWNED heap copy, never the static data
            // segment — hence the persist rather than storing the literal address directly.
            let (ptr, len) = ctx.default_str_literal(value)?;
            ctx.fb.ins(&format!("i32.const {}", ptr), "string default literal ptr");
            ctx.fb.ins(&format!("i64.const {}", len), "string default literal len");
            ctx.fb.ins("call $__rt_str_persist", "persist an owned copy (ptr,len) -> (new_ptr,new_len)");
            let len_tmp = ctx.fresh_temp(ValType::I64);
            let ptr_tmp = ctx.fresh_temp(ValType::I32);
            ctx.fb.ins(&format!("local.set {}", len_tmp), "save persisted string len");
            ctx.fb.ins(&format!("local.set {}", ptr_tmp), "save persisted string ptr");
            ctx.fb.ins(&format!("local.get {}", obj), "object base address");
            ctx.fb.ins(&format!("local.get {}", ptr_tmp), "persisted string ptr");
            ctx.fb.ins("i64.extend_i32_u", "widen string ptr to i64 lo");
            ctx.fb.ins(&format!("i64.store offset={}", offset), "store string default value_lo (ptr)");
            ctx.fb.ins(&format!("local.get {}", obj), "object base address");
            ctx.fb.ins(&format!("local.get {}", len_tmp), "persisted string len");
            ctx.fb.ins(&format!("i64.store offset={}", offset + 8), "store string default value_hi (len)");
        }
        LiteralDefaultValue::BoxedStr(value) => {
            // A `mixed`/union slot holds a Mixed cell, so the string is boxed rather than
            // stored raw. `__rt_mixed_from_value` persists its own copy for tag 1, so no
            // separate `__rt_str_persist` is needed here.
            let (ptr, len) = ctx.default_str_literal(value)?;
            ctx.fb.ins("i64.const 1", "mixed string tag");
            ctx.fb.ins(&format!("i32.const {}", ptr), "string default literal ptr");
            ctx.fb.ins("i64.extend_i32_u", "widen string ptr to i64 lo");
            ctx.fb.ins(&format!("i64.const {}", len), "string default literal len -> hi");
            ctx.fb.ins("call $__rt_mixed_from_value", "box string default into a mixed cell");
            let cell = ctx.fresh_temp(ValType::I32);
            ctx.fb.ins(&format!("local.set {}", cell), "save boxed string default cell");
            ctx.fb.ins(&format!("local.get {}", obj), "object base address");
            ctx.fb.ins(&format!("local.get {}", cell), "boxed string default cell");
            ctx.fb.ins("i64.extend_i32_u", "widen mixed cell pointer");
            ctx.fb.ins(&format!("i64.store offset={}", offset), "store mixed property value_lo");
            ctx.fb.ins(&format!("local.get {}", obj), "object base address");
            ctx.fb.ins("i64.const 7", "mixed property runtime tag");
            ctx.fb.ins(&format!("i64.store offset={}", offset + 8), "store mixed property value_hi");
        }
        other => {
            return Err(WasmError::Unsupported(format!(
                "non-scalar default for property ${} on wasm32-wasi (kind {:?})",
                prop_name, std::mem::discriminant(other)
            )))
        }
    }
    Ok(())
}

/// Boxes one scalar literal and stores its owned Mixed cell in an object property slot.
fn emit_boxed_scalar_default(
    ctx: &mut FnCtx,
    obj: &str,
    offset: usize,
    tag: i64,
    value_lo: i64,
    description: &str,
) {
    ctx.fb
        .ins(&format!("i64.const {}", tag), &format!("mixed {description} tag"));
    ctx.fb
        .ins(&format!("i64.const {}", value_lo), &format!("mixed {description} value"));
    ctx.fb.ins("i64.const 0", "mixed scalar high word");
    ctx.fb
        .ins("call $__rt_mixed_from_value", "box property default into a mixed cell");
    let cell = ctx.fresh_temp(ValType::I32);
    ctx.fb
        .ins(&format!("local.set {}", cell), "save boxed property default cell");
    ctx.fb
        .ins(&format!("local.get {}", obj), "object base address");
    ctx.fb
        .ins(&format!("local.get {}", cell), "boxed property default cell");
    ctx.fb
        .ins("i64.extend_i32_u", "widen mixed cell pointer");
    ctx.fb
        .ins(&format!("i64.store offset={}", offset), "store mixed property value_lo");
    ctx.fb
        .ins(&format!("local.get {}", obj), "object base address");
    ctx.fb.ins("i64.const 7", "mixed property runtime tag");
    ctx.fb.ins(
        &format!("i64.store offset={}", offset + 8),
        "store mixed property value_hi",
    );
}

/// Boxes a scalar / string / container value into a fresh owned kind-5 Mixed cell via
/// `__rt_mixed_from_value`, leaving the cell pointer (i32) on the WASM operand stack and
/// returning the temp local name holding it.
///
/// Reused by the Mixed-property BOX sub-case of `lower_prop_set`. A value that is already a
/// Mixed cell (`Ptr` with `ir_type == Heap(Mixed)`) is NOT handled here — the MOVE path stores it
/// directly. Returns `Unsupported` for Tagged/Void/non-heap pointers.
pub(super) fn emit_box_value_into_mixed(ctx: &mut FnCtx, value: ValueId) -> Result<String> {
    let repr = ctx.value_repr(value)?.clone();
    let php = ctx.function.value(value).map(|v| v.php_type.codegen_repr());
    let ir = ctx.function.value(value).map(|v| v.ir_type);
    match &repr {
        WasmRepr::I64(local) => {
            // int 0, bool 3, callable 10 (a callable is a kind-6 descriptor carried as
            // i64; tag 10 makes `__rt_mixed_from_value` incref it so the cell shares
            // ownership and the release arm frees it via kind-6 dispatch).
            let tag: i64 = match php {
                Some(PhpType::Bool) => 3,
                Some(PhpType::Callable) => 10,
                _ => 0,
            };
            ctx.fb.ins(&format!("i64.const {}", tag), "mixed tag (int/bool/callable)");
            ctx.fb.ins(&format!("local.get {}", local), "scalar/descriptor value -> lo");
            ctx.fb.ins("i64.const 0", "hi unused");
            ctx.fb.ins("call $__rt_mixed_from_value", "box scalar/callable into a mixed cell");
        }
        WasmRepr::F64(local) => {
            ctx.fb.ins("i64.const 2", "mixed tag (float)");
            ctx.fb.ins(&format!("local.get {}", local), "float value (f64)");
            ctx.fb.ins("i64.reinterpret_f64", "float bits -> lo");
            ctx.fb.ins("i64.const 0", "hi unused");
            ctx.fb.ins("call $__rt_mixed_from_value", "box float into a mixed cell");
        }
        WasmRepr::Str { ptr, len } => {
            ctx.fb.ins("i64.const 1", "mixed tag (string)");
            ctx.fb.ins(&format!("local.get {}", ptr), "string pointer");
            ctx.fb.ins("i64.extend_i32_u", "ptr -> lo");
            ctx.fb.ins(&format!("local.get {}", len), "string length -> hi");
            ctx.fb.ins("call $__rt_mixed_from_value", "box string (persists a copy)");
        }
        WasmRepr::Ptr(local) => {
            let tag: i64 = match ir {
                Some(IrType::Heap(IrHeapKind::Array)) => 4,
                Some(IrType::Heap(IrHeapKind::Hash)) => 5,
                Some(IrType::Heap(IrHeapKind::Object)) => 6,
                Some(IrType::Heap(IrHeapKind::Mixed)) => {
                    return Err(WasmError::Unsupported(
                        "box of already-mixed value not handled by emit_box_value_into_mixed (MOVE path stores directly)".to_string(),
                    ))
                }
                _ => {
                    return Err(WasmError::Unsupported(
                        "box of a non-heap pointer into a mixed cell not supported on wasm32-wasi"
                            .to_string(),
                    ))
                }
            };
            ctx.fb.ins(&format!("i64.const {}", tag), "mixed tag (heap kind)");
            ctx.fb.ins(&format!("local.get {}", local), "heap pointer");
            ctx.fb.ins("i64.extend_i32_u", "ptr -> lo");
            ctx.fb.ins("i64.const 0", "hi unused");
            ctx.fb.ins("call $__rt_mixed_from_value", "box heap value (increfs the child)");
        }
        _ => {
            return Err(WasmError::Unsupported(
                "box of a tagged/void value into a mixed cell not supported on wasm32-wasi".to_string(),
            ))
        }
    }
    let cell_tmp = ctx.fresh_temp(ValType::I32);
    ctx.fb.ins(&format!("local.set {}", cell_tmp), "save fresh kind-5 cell ptr");
    Ok(cell_tmp)
}

/// Lowers `Op::PropGet` of a declared property to a direct memory load that returns an OWNED value.
///
/// Scalar arms (Int/Bool/Float) load directly with no incref (unrefcounted). Refcounted arms
/// (Str, Array/AssocArray/Object, Mixed/Union/Iterable) load the slot's child pointer, incref it
/// (so the read result is independent of the object and may be Released by the ownership pass),
/// then `store_result`. Str reads the persisted-string copy via `__rt_str_persist`. Non-object
/// receivers, undeclared properties, Resource, and Tagged/Void results return `Unsupported`.
pub(super) fn lower_prop_get(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let object = operand(inst, 0)?;
    let prop_data = data_immediate(inst)?;
    let property = ctx
        .module
        .data
        .strings
        .get(prop_data.as_raw() as usize)
        .cloned()
        .ok_or_else(|| WasmError::Unsupported(format!("invalid string data id {:?}", prop_data)))?;

    // A NULLABLE object receiver arrives as a boxed cell, so the read has to open it and take
    // php's warning branch when it holds null. `lower_nullsafe_prop_get` already does exactly
    // that opening; the two differ only in whether the null arm warns, which it decides from
    // the opcode.
    if matches!(value_php_type(ctx, object)?, PhpType::Union(_)) {
        return lower_nullsafe_prop_get(ctx, inst);
    }
    // A MIXED receiver names no class until run time, exactly like a mixed METHOD call — and the
    // answer is the same one: a ladder over the class id the object header carries.
    if value_php_type(ctx, object)? == PhpType::Mixed {
        return lower_mixed_prop_get(ctx, inst, object, &property, prop_data);
    }
    let (class_name, ci) = receiver_class_info(ctx, object)?;
    // Undeclared property on an ADP / stdClass class -> read through the dyn-prop hash tail.
    if let Some(dyn_off) = dynamic_property_hash_offset_for_class(&ci, &class_name, &property) {
        return lower_dyn_prop_get(ctx, inst, object, dyn_off, DynPropKey::Literal(prop_data));
    }
    let (_, offset, prop_type) = resolve_property_slot(&ci, &property)?;
    let prop_type = prop_type.codegen_repr();
    let obj_ref = object_ptr_ref(ctx, object)?;

    // php raises `Error: Typed property C::$p must not be accessed before initialization` for a
    // slot that has been `unset`, and it is CATCHABLE — the native backend answers that way too.
    // Only a slot some `unset` in the module can reach is guarded, so a program that never unsets
    // a property keeps the bare load it had.
    if property_can_be_unset(ctx, &class_name, &property) {
        emit_raise_if_unset(ctx, &obj_ref, offset, &class_name, &property)?;
    }

    // Whether an `Op::Acquire` follows is not a property of `Op::PropGet` — it is a property of
    // the value it produces, and the EIR says which: `ir_lower` acquires a borrowed result to
    // stabilize it, and skips that when the result is ALREADY owned. A DECLARED `string`/`array`
    // property reads as `MaybeOwned` and is acquired; an UNTYPED one reads as `Owned` and is not,
    // yet is still paired with a `release`. Retaining unconditionally leaks one reference per read
    // of the first kind; borrowing unconditionally frees the second kind out from under the object
    // — `$this->n` read once by `echo` left the slot dangling, and the next `$this->n + 1` read a
    // recycled cell.
    let load = match ctx.function.value(inst.result.unwrap_or(object)) {
        Some(value) if value.ownership == Ownership::Owned => PropertyLoad::Owned,
        _ => PropertyLoad::Borrowed,
    };

    // A raw object pointer can be 0 since a missed `array<Object>` element reads as null while
    // still carrying the element's own non-null type. Reading `obj + offset` off 0 answers
    // whatever sits in low linear memory — measured as a bare `1` for an int property, with no
    // diagnostic at all, where php-src warns and evaluates to null. Both backends drop the null
    // itself (the result type cannot carry one), but the WARNING is exact, and the substituted
    // value is at least defined rather than whatever the data segments happen to start with.
    //
    // The warning helper lives in the COMMAND runtime, which only a main-bearing module carries
    // — a reactor/library module has no stderr contract and no `__rt_warn_*` at all. Without a
    // main the plain load is emitted, exactly as before this guard existed, which is the same
    // line `array_get_shape_issue` already draws for its own warning-producing read.
    let has_main = ctx.module.functions.iter().any(|function| function.flags.is_main);
    if !has_main {
        emit_declared_property_load(ctx, &obj_ref, offset, &prop_type, &property, load)?;
        store_result(ctx, inst)?;
        return Ok(());
    }

    let (name_ptr, name_len) = ctx.str_literal(prop_data)?;
    let null_receiver = ctx.fresh_temp(ValType::I32);
    ctx.fb
        .ins(&format!("local.get {}", obj_ref), "property receiver");
    ctx.fb
        .ins(&format!("local.tee {}", null_receiver), "keep it for the null test");
    ctx.fb.ins("i32.eqz", "is the receiver null?");
    // Both arms leave a property value, so the block has to declare what that is: an `if` with
    // no result type is `[] -> []` and pushing inside it does not validate.
    ctx.fb.ins(
        &format!("if (result {})", property_value_val_types(&prop_type)?),
        "php-src warns and yields null here",
    );
    ctx.fb.ins(
        &format!(
            "(call $__rt_warn_property_on_null (i32.const {name_ptr}) (i32.const {name_len}))"
        ),
        "Attempt to read property \"X\" on null",
    );
    emit_null_property_value(ctx, &prop_type)?;
    ctx.fb.ins("else", "an ordinary receiver reads its slot");
    emit_declared_property_load(ctx, &obj_ref, offset, &prop_type, &property, load)?;
    ctx.fb.ins("end", "both arms leave one property value");

    store_result(ctx, inst)?;
    Ok(())
}

/// Reads one dynamic property and leaves the boxed value on the stack.
///
/// Shared by the statically typed read and by one arm of a mixed receiver's ladder, which is the
/// reason it is separate: `stdClass` is in every module's prelude, so a ladder that could not
/// read a hash would have no way to answer for the one class most mixed receivers actually are.
///
/// `warn` is the caller's, because php does NOT warn inside `isset`, `empty` or `??`: those ASK
/// whether the property is there, and the answer is the whole point.
fn emit_dyn_prop_read(
    ctx: &mut FnCtx,
    obj_ref: &str,
    dyn_off: usize,
    key: DynPropKey,
    warn: bool,
) -> Result<()> {
    ctx.fb.ins(&format!("local.get {}", obj_ref), "object base address");
    ctx.fb.ins(
        &format!("i32.load offset={}", dyn_off),
        "load dynamic-property hash pointer",
    );
    let hash = ctx.fresh_temp(ValType::I32);
    ctx.fb.ins(&format!("local.set {}", hash), "save dyn-prop hash pointer");
    ctx.fb.ins(&format!("local.get {}", hash), "dyn-prop hash pointer");
    emit_dyn_prop_key(ctx, key)?;
    ctx.fb.ins(
        "call $__rt_hash_get",
        "look up the dynamic property -> (found, vlo, vhi, vtag)",
    );
    let vtag = ctx.fresh_temp(ValType::I64);
    ctx.fb.ins(&format!("local.set {}", vtag), "captured value tag (7 = mixed)");
    let vhi = ctx.fresh_temp(ValType::I64);
    ctx.fb.ins(&format!("local.set {}", vhi), "captured value high word (0)");
    let vlo = ctx.fresh_temp(ValType::I64);
    ctx.fb.ins(&format!("local.set {}", vlo), "captured value low word (cell ptr)");
    let found = ctx.fresh_temp(ValType::I32);
    ctx.fb.ins(&format!("local.set {}", found), "captured found flag");
    // hit -> incref the borrowed Mixed cell (hash_get is borrow-only); miss -> box null.
    ctx.fb.ins(&format!("local.get {}", found), "found flag");
    ctx.fb.ins("if (result i32)", "dynamic property present?");
    ctx.fb.ins(&format!("local.get {}", vlo), "stored mixed cell ptr (i64)");
    ctx.fb.ins("i32.wrap_i64", "cell ptr -> i32");
    let cell = ctx.fresh_temp(ValType::I32);
    ctx.fb.ins(&format!("local.set {}", cell), "save hit cell ptr");
    ctx.fb.ins(&format!("local.get {}", cell), "hit cell ptr");
    ctx.fb.ins("call $__rt_incref", "retain the borrowed dyn-prop cell");
    ctx.fb.ins(&format!("local.get {}", cell), "hit cell ptr for result");
    ctx.fb.ins("else", "miss -> box a fresh null mixed cell");
    if warn {
        emit_undefined_property_warning(ctx, obj_ref, key)?;
    }
    ctx.fb.ins("i64.const 8", "mixed tag (null)");
    ctx.fb.ins("i64.const 0", "null lo");
    ctx.fb.ins("i64.const 0", "null hi");
    ctx.fb.ins("call $__rt_mixed_from_value", "box a fresh null mixed cell");
    ctx.fb.ins("end", "end hit/miss");
    Ok(())
}

/// Lowers `$mixed->prop`: a class-id ladder over every class that declares it.
///
/// A boxed receiver carries its class in the object header rather than in its type, so the read
/// is decided at run time — the same shape `lower_mixed_method_call` uses, and for the same
/// reason. Each arm reads one class's slot and boxes it, because the instruction's result is a
/// Mixed cell whatever the declared type behind it was.
///
/// The load is BORROWED and the box then takes its own reference: `__rt_mixed_from_value`
/// persists a copy of a string and increfs a container, so an owned load would leave the cell
/// with two references and the slot with none to give back.
///
/// php's two diagnostics are both here. A receiver that is not an object warns and answers null —
/// naming the type, and for a bool naming the VALUE (`on true`). An object whose class no arm
/// claims gets `Undefined property: C::$name`, read from the runtime class-name table, which is
/// the same helper a dynamic-property miss uses.
fn lower_mixed_prop_get(
    ctx: &mut FnCtx,
    inst: &Instruction,
    object: ValueId,
    property: &str,
    prop_data: crate::ir::DataId,
) -> Result<()> {
    let receiver_php = value_php_type(ctx, object)?;
    let candidates =
        super::classes::mixed_property_candidates(ctx.module, property, &receiver_php);
    if candidates.is_empty() {
        return Err(WasmError::Unsupported(format!(
            "no class in the module declares ${property} for a mixed receiver"
        )));
    }
    let (name_ptr, name_len) = ctx.str_literal(prop_data)?;
    let tag = ctx.fresh_temp(ValType::I64);
    let lo = ctx.fresh_temp(ValType::I64);
    let hi = ctx.fresh_temp(ValType::I64);
    let obj = ctx.fresh_temp(ValType::I32);
    let cid = ctx.fresh_temp(ValType::I64);
    ctx.emit_load_value(object)?;
    ctx.fb
        .ins("call $__rt_mixed_unbox", "unbox the receiver -> (tag, lo, hi)");
    ctx.fb.ins(&format!("local.set {hi}"), "receiver high word");
    ctx.fb.ins(&format!("local.set {lo}"), "receiver low word");
    ctx.fb.ins(&format!("local.set {tag}"), "receiver runtime tag");

    ctx.fb.ins(&format!("local.get {tag}"), "receiver runtime tag");
    ctx.fb.ins("i64.const 6", "object tag");
    ctx.fb.ins("i64.eq", "is the receiver an object?");
    ctx.fb
        .ins("if (result i32)", "both arms leave one boxed property value");
    ctx.fb.ins(&format!("local.get {lo}"), "receiver low word");
    ctx.fb.ins("i32.wrap_i64", "the object pointer");
    ctx.fb.ins(&format!("local.set {obj}"), "receiver object pointer");
    ctx.fb.ins(&format!("local.get {obj}"), "receiver object pointer");
    ctx.fb.ins("i64.load offset=0", "its runtime class id");
    ctx.fb.ins(&format!("local.set {cid}"), "receiver class id");
    ctx.fb
        .ins("block $mxprop (result i32)", "mixed property read merge");
    let warn = !read_only_asks_whether_it_is_set(ctx, inst.result);
    for (class_id, class_name, source) in &candidates {
        ctx.fb.ins(&format!("local.get {cid}"), "receiver class id");
        ctx.fb
            .ins(&format!("i64.const {}", *class_id as i64), "candidate class id");
        ctx.fb.ins("i64.eq", "is the receiver this class?");
        ctx.fb.ins("if (result i32)", "candidate class arm");
        match source {
            MixedPropertySource::Slot { offset, prop_type } => {
                if property_can_be_unset(ctx, class_name, property) {
                    emit_raise_if_unset(ctx, &obj, *offset, class_name, property)?;
                }
                emit_declared_property_load(
                    ctx,
                    &obj,
                    *offset,
                    prop_type,
                    property,
                    PropertyLoad::Borrowed,
                )?;
                super::transfer::emit_convert_stack_value(
                    ctx,
                    IrType::from_php(prop_type),
                    prop_type.clone(),
                    IrType::Heap(IrHeapKind::Mixed),
                    PhpType::Mixed,
                )?;
            }
            MixedPropertySource::DynamicTail { offset } => {
                emit_dyn_prop_read(ctx, &obj, *offset, DynPropKey::Literal(prop_data), warn)?;
            }
        }
        ctx.fb.ins("br $mxprop", "this class answered the read");
        ctx.fb.ins("else", "try the next candidate");
    }
    // An object whose class no arm claims simply does not have the property.
    if warn {
        emit_undefined_property_warning(ctx, &obj, DynPropKey::Literal(prop_data))?;
    }
    emit_null_mixed_cell(ctx);
    for _ in &candidates {
        ctx.fb.ins("end", "end candidate class arm");
    }
    ctx.fb.ins("end", "end mixed property read merge");
    ctx.fb.ins("else", "the receiver is not an object at all");
    ctx.fb.ins(&format!("i32.const {name_ptr}"), "property name pointer");
    ctx.fb.ins(&format!("i32.const {name_len}"), "property name length");
    ctx.fb.ins(&format!("local.get {tag}"), "receiver runtime tag");
    ctx.fb.ins(&format!("local.get {lo}"), "its payload, which names a bool");
    ctx.fb.ins(
        "call $__rt_warn_property_on_non_object",
        "Attempt to read property \"X\" on <type>",
    );
    emit_null_mixed_cell(ctx);
    ctx.fb.ins("end", "end object-receiver test");
    store_result(ctx, inst)
}

/// Pushes a freshly boxed php null, the value every failing property read answers.
fn emit_null_mixed_cell(ctx: &mut FnCtx) {
    ctx.fb.ins(
        "(call $__rt_mixed_from_value (i64.const 8) (i64.const 0) (i64.const 0))",
        "php answers null for a property it could not read",
    );
}

/// Answers the wasm result types one property read leaves on the stack.
///
/// Kept beside `emit_declared_property_load` and `emit_null_property_value`, which must agree
/// with it: the null-receiver guard branches between them, and the block's declared result is
/// what makes both arms type-check.
fn property_value_val_types(prop_type: &PhpType) -> Result<&'static str> {
    Ok(match prop_type {
        PhpType::Int | PhpType::Bool => "i64",
        PhpType::Float => "f64",
        PhpType::Str => "i32 i64",
        // Payload then tag, the order every tagged producer pushes them in.
        PhpType::TaggedScalar => "i64 i32",
        PhpType::Array(_)
        | PhpType::AssocArray { .. }
        | PhpType::Object(_)
        | PhpType::Mixed
        | PhpType::Union(_)
        | PhpType::Iterable => "i32",
        other => {
            return Err(WasmError::Unsupported(format!(
                "property read result types for {:?}",
                other
            )))
        }
    })
}

/// Pushes the stand-in a null receiver's property read leaves on the stack.
///
/// Same arity and value types as `emit_declared_property_load`, so both arms of the guard agree
/// and the block type-checks. The values are chosen to be the closest thing each representation
/// has to PHP's null: an empty string and a null container pointer ARE what php-src answers, and
/// the int arm carries the same `NULL_SENTINEL` the native backend leaves in a non-nullable int
/// slot, which keeps the two backends saying the same thing about a null neither can type.
fn emit_null_property_value(ctx: &mut FnCtx, prop_type: &PhpType) -> Result<()> {
    match prop_type {
        PhpType::Int | PhpType::Bool => {
            ctx.fb
                .ins("i64.const 9223372036854775806", "null sentinel, as the native backend leaves");
        }
        PhpType::Float => {
            ctx.fb.ins("f64.const 0", "null reads as 0.0 in a float slot");
        }
        PhpType::Str => {
            ctx.fb.ins("i32.const 0", "empty string pointer — php-src's own answer for null");
            ctx.fb.ins("i64.const 0", "empty string length");
        }
        // The one slot shape that can SAY null rather than approximate it.
        PhpType::TaggedScalar => {
            ctx.fb.ins("i64.const 0", "a null carries no payload");
            ctx.fb.ins("i32.const 8", "and this tag is php's null exactly");
        }
        PhpType::Array(_)
        | PhpType::AssocArray { .. }
        | PhpType::Object(_)
        | PhpType::Mixed
        | PhpType::Union(_)
        | PhpType::Iterable => {
            ctx.fb
                .ins("i32.const 0", "a null container pointer IS null, and `is_null` reads it");
        }
        other => {
            return Err(WasmError::Unsupported(format!(
                "null-receiver property stand-in for {:?}",
                other
            )));
        }
    }
    Ok(())
}

/// Who owns the value a property read leaves on the stack.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum PropertyLoad {
    /// The reader owns it: a string is persisted into a fresh heap copy and a refcounted child is
    /// increfed. For callers that produce a result nothing else will acquire.
    Owned,
    /// The reader only views it: no copy and no incref. For an `Op::PropGet` whose result the EIR
    /// follows with an `Op::Acquire` — which it does for every `MaybeOwned` result, across every
    /// use shape (store, echo, argument, concat, builtin argument, return, strict compare).
    Borrowed,
}

/// Pushes one DECLARED property slot's value onto the stack.
///
/// Shared by `PropGet` and by the Throwable accessors that method-call lowering open-codes. They
/// need OPPOSITE ownership, which is the whole reason this takes a parameter: `PropGet` is
/// followed by an `acquire` that persists or increfs, so retaining here as well left one extra
/// reference per read — measured at ~207 bytes per read of an array property, whose backing array
/// was then never freed, and ~87 for a string. The Throwable accessors have no such acquire and
/// outlive the object they read from, so they must own their copy.
///
/// Scalar arms (Int/Bool/Float) are the same either way: they are not refcounted.
fn emit_declared_property_load(
    ctx: &mut FnCtx,
    obj_ref: &str,
    offset: usize,
    prop_type: &PhpType,
    property: &str,
    load: PropertyLoad,
) -> Result<()> {
    match prop_type {
        PhpType::Int | PhpType::Bool => {
            ctx.fb.ins(&format!("local.get {}", obj_ref), "object base address");
            ctx.fb.ins(&format!("i64.load offset={}", offset), "load scalar property value_lo");
        }
        PhpType::Float => {
            ctx.fb.ins(&format!("local.get {}", obj_ref), "object base address");
            ctx.fb.ins(&format!("f64.load offset={}", offset), "load float property value_lo");
        }
        // The `(payload, tag)` pair, read flat out of the same 16 bytes. No heap pointer, so
        // an owned read is the same two loads as a borrowed one.
        PhpType::TaggedScalar => {
            ctx.fb.ins(&format!("local.get {}", obj_ref), "object base address");
            ctx.fb.ins(
                &format!("i64.load offset={}", offset),
                "load the tagged payload",
            );
            ctx.fb.ins(&format!("local.get {}", obj_ref), "object base address");
            ctx.fb.ins(
                &format!("i32.load offset={}", offset + 8),
                "load the tag (0 int, 2 float, 3 bool, 8 null)",
            );
        }
        PhpType::Str => {
            ctx.fb.ins(&format!("local.get {}", obj_ref), "object base address");
            ctx.fb.ins(&format!("i32.load offset={}", offset), "load string property ptr (lo)");
            ctx.fb.ins(&format!("local.get {}", obj_ref), "object base address");
            ctx.fb.ins(&format!("i64.load offset={}", offset + 8), "load string property len (hi)");
            if load == PropertyLoad::Owned {
                ctx.fb.ins(
                    "call $__rt_str_persist",
                    "persist string copy (ptr,len) -> (new_ptr,new_len)",
                );
            }
        }
        PhpType::Array(_)
        | PhpType::AssocArray { .. }
        | PhpType::Object(_)
        | PhpType::Mixed
        | PhpType::Union(_)
        | PhpType::Iterable => {
            let what = if matches!(prop_type, PhpType::Mixed | PhpType::Union(_) | PhpType::Iterable)
            {
                "mixed cell"
            } else {
                "container cell"
            };
            ctx.fb.ins(&format!("local.get {}", obj_ref), "object base address");
            ctx.fb.ins(
                &format!("i32.load offset={}", offset),
                &format!("load {what} property ptr (lo)"),
            );
            if load == PropertyLoad::Owned {
                let ptr_tmp = ctx.fresh_temp(ValType::I32);
                ctx.fb
                    .ins(&format!("local.set {}", ptr_tmp), &format!("save {what} ptr"));
                ctx.fb.ins(&format!("local.get {}", ptr_tmp), what);
                ctx.fb
                    .ins("call $__rt_incref", &format!("retain returned {what} (refcount++)"));
                ctx.fb
                    .ins(&format!("local.get {}", ptr_tmp), &format!("{what} for result"));
            }
        }
        other => {
            return Err(WasmError::Unsupported(format!(
                "property ${} of type {:?} not yet supported on wasm32-wasi",
                property, other
            )))
        }
    }
    Ok(())
}

/// Whether some `unset($obj->$property)` in this module can clear this class's slot.
///
/// Asked by all three sites that have to agree about it — the marker write, the `isset` probe and
/// the read — through `plan::properties_reached_by_unset`, which is also what lays the fatal
/// message out. A module that never unsets a property gets the old code exactly: a folded probe
/// and an unguarded read.
pub(super) fn property_can_be_unset(ctx: &FnCtx, class_name: &str, property: &str) -> bool {
    let Some(declaring) =
        super::plan::declaring_class_of_property(ctx.module, class_name, property)
    else {
        return false;
    };
    ctx.unset_properties
        .iter()
        .any(|(class, name)| class == &declaring && name == property)
}

/// Pushes 1 when the slot at `offset` is UNSET, 0 otherwise, as an **i32** — `i64.eq` answers in
/// i32 like every other wasm comparison, so a caller that wants a php bool has to widen it.
///
/// The slot's SECOND word is the marker, and the value is the native backend's own
/// `UNINITIALIZED_TYPED_PROPERTY_SENTINEL` so the two targets cannot drift. Reading it as an i64 is
/// what makes it exact for every property shape: that word is a tag of 0..8, a string LENGTH, or a
/// container tag, and none of those can equal `PHP_INT_MAX - 2`.
fn emit_slot_is_unset(ctx: &mut FnCtx, obj_ref: &str, offset: usize) {
    ctx.fb
        .ins(&format!("local.get {}", obj_ref), "object base address");
    ctx.fb.ins(
        &format!("i64.load offset={}", offset + 8),
        "the slot's marker word",
    );
    ctx.fb.ins(
        &format!(
            "i64.const {}",
            crate::codegen::UNINITIALIZED_TYPED_PROPERTY_SENTINEL
        ),
        "php's own uninitialized marker, shared with the native backend",
    );
    ctx.fb.ins("i64.eq", "is this slot unset?");
}

/// Lowers `Op::PropUnset`: `unset($obj->$declaredTyped)`.
///
/// php REMOVES the property, which a fixed slot cannot express — so the slot is marked instead, in
/// the same word and with the same value the native backend marks it with. Two things follow from
/// the marker rather than from this instruction: `isset()` reads it, and a read of the slot raises
/// php's `Error`. Both ask `property_can_be_unset` so they only pay for it where an unset exists.
///
/// The previous value is released FIRST — the slot owns a string's bytes or a container's
/// reference, and marking over it without giving that up leaks one per unset.
///
/// An UNTYPED declared property is refused, exactly as the native backend refuses it and for the
/// same reason: php's removed-then-read answers `null` with a warning there, not a fatal, and a
/// slot whose declared type has no null has nowhere to put that.
pub(super) fn lower_prop_unset(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let object = operand(inst, 0)?;
    let prop_data = data_immediate(inst)?;
    let property = ctx
        .module
        .data
        .strings
        .get(prop_data.as_raw() as usize)
        .cloned()
        .ok_or_else(|| WasmError::Unsupported(format!("invalid string data id {:?}", prop_data)))?;
    let (class_name, ci) = receiver_class_info(ctx, object)?;
    let (index, offset, prop_type) = resolve_property_slot(&ci, &property)?;
    if !ci.property_declared_slots.get(index).copied().unwrap_or(false) {
        return Err(WasmError::Unsupported(format!(
            "unset() of the untyped declared property {class_name}::${property}"
        )));
    }
    let prop_type = prop_type.codegen_repr();
    let obj_ref = object_ptr_ref(ctx, object)?;

    // Give up whatever the slot owned. A scalar owns nothing and the helper is null-safe, so the
    // shapes that need it are the string and the two pointer ones; asking by shape keeps the
    // release out of the arms that would only pass zero.
    if matches!(
        prop_type,
        PhpType::Str
            | PhpType::Array(_)
            | PhpType::AssocArray { .. }
            | PhpType::Object(_)
            | PhpType::Mixed
            | PhpType::Union(_)
            | PhpType::Iterable
    ) {
        ctx.fb
            .ins(&format!("local.get {}", obj_ref), "object base address");
        ctx.fb.ins(
            &format!("i32.load offset={}", offset),
            "the value this slot owns",
        );
        ctx.fb
            .ins("call $__rt_decref_any", "release it (null-safe)");
    }
    ctx.fb
        .ins(&format!("local.get {}", obj_ref), "object base address");
    ctx.fb.ins("i64.const 0", "a removed slot carries no value");
    ctx.fb
        .ins(&format!("i64.store offset={}", offset), "clear the payload");
    ctx.fb
        .ins(&format!("local.get {}", obj_ref), "object base address");
    ctx.fb.ins(
        &format!(
            "i64.const {}",
            crate::codegen::UNINITIALIZED_TYPED_PROPERTY_SENTINEL
        ),
        "php's own uninitialized marker",
    );
    ctx.fb.ins(
        &format!("i64.store offset={}", offset + 8),
        "mark the slot unset",
    );
    Ok(())
}

/// Pushes the `isset($obj->$p)` answer READ FROM THE SLOT, for a property some `unset` can clear.
///
/// `Some(())` means the answer is on the stack and the caller must not also push its constant;
/// `None` means no `unset` in this module reaches the slot, so the constant is still exact.
///
/// Kept beside the write and the read that share the marker, so the three cannot come to disagree
/// about which word holds it or what value means "removed".
pub(super) fn emit_prop_initialized_from_marker(
    ctx: &mut FnCtx,
    inst: &Instruction,
    object: ValueId,
) -> Result<Option<()>> {
    let Some(prop_data) = data_immediate(inst).ok() else {
        return Ok(None);
    };
    let Some(property) = ctx
        .module
        .data
        .strings
        .get(prop_data.as_raw() as usize)
        .cloned()
    else {
        return Ok(None);
    };
    let Ok((class_name, ci)) = receiver_class_info(ctx, object) else {
        return Ok(None);
    };
    if !property_can_be_unset(ctx, &class_name, &property) {
        return Ok(None);
    }
    let (_, offset, _) = resolve_property_slot(&ci, &property)?;
    let obj_ref = object_ptr_ref(ctx, object)?;
    emit_slot_is_unset(ctx, &obj_ref, offset);
    ctx.fb
        .ins("i32.eqz", "php answers false for a slot `unset` removed");
    ctx.fb.ins("i64.extend_i32_u", "bool i32 -> i64");
    Ok(Some(()))
}

/// Raises php's `Error` when the slot at `offset` has been `unset`, and falls through otherwise.
///
/// The message is a CONSTANT — both halves are compile-time names — so the fixed-message raise
/// path carries it, and `plan` has already given it an address.
fn emit_raise_if_unset(
    ctx: &mut FnCtx,
    obj_ref: &str,
    offset: usize,
    class_name: &str,
    property: &str,
) -> Result<()> {
    emit_slot_is_unset(ctx, obj_ref, offset);
    ctx.fb.ins("if", "php raises rather than reading a removed slot");
    let message = super::plan::unset_property_message(class_name, property);
    emit_runtime_error_throw(
        ctx,
        "Error",
        &message,
        super::function::UNCAUGHT_EXCEPTION_FAILURE_CODE,
    )?;
    ctx.fb.ins("end", "an ordinary slot reads on");
    Ok(())
}

/// Lowers `Op::PropSet` of a declared property slot to a direct memory store.
///
/// `PropSet` is void with operands `[object, value]`. An undeclared property on an ADP /
/// stdClass receiver writes through the dynamic-property hash tail; a declared one resolves its
/// slot and hands the store to `emit_declared_property_store`, which owns the per-type
/// release/retain rules. Non-object receivers and undeclared properties on a class without the
/// dynamic tail return `Unsupported`.
/// Lowers `Op::PropGetForWrite` of a boxed property: the cell a following write mutates.
///
/// The cell is replaced by a private copy first, exactly as the native backend's
/// `__rt_mixed_clone` split does: the copy holds one more reference to the same array, so the
/// write that follows splits the ARRAY copy-on-write if anything else shares it, while a cell
/// shared with another owner keeps what it had. The slot's own reference moves to the copy and
/// the old cell is released. The result is borrowed from the slot.
pub(super) fn lower_prop_get_for_write(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let object = operand(inst, 0)?;
    let prop_data = data_immediate(inst)?;
    let property = ctx
        .module
        .data
        .strings
        .get(prop_data.as_raw() as usize)
        .cloned()
        .ok_or_else(|| WasmError::Unsupported(format!("invalid string data id {:?}", prop_data)))?;
    let (_, ci) = receiver_class_info(ctx, object)?;
    let (_, offset, _) = resolve_property_slot(&ci, &property)?;
    let obj_ref = object_ptr_ref(ctx, object)?;
    let old = ctx.fresh_temp(ValType::I32);
    let new = ctx.fresh_temp(ValType::I32);
    ctx.fb.ins(
        &format!("(local.set {old} (i32.wrap_i64 (i64.load offset={offset} (local.get {obj_ref}))))"),
        "the property's cell",
    );
    ctx.fb.ins(&format!("(call $__rt_mixed_unbox (local.get {old}))"), "its (tag, lo, hi)");
    ctx.fb.ins("call $__rt_mixed_from_value", "a private copy of the cell");
    ctx.fb.ins(&format!("local.set {new}"), "the copy the write will mutate");
    ctx.fb.ins(
        &format!("(call $__rt_decref_any (local.get {old}))"),
        "the slot's reference moves to the copy",
    );
    ctx.fb.ins(
        &format!("(i64.store offset={offset} (local.get {obj_ref}) (i64.extend_i32_u (local.get {new})))"),
        "republish the copy in the slot",
    );
    ctx.fb.ins(&format!("local.get {new}"), "the cell, borrowed from the slot");
    store_result(ctx, inst)
}

/// Lowers `Op::PropSet`: stores a value into the declared slot of the receiver's class, or into the
/// dynamic-property hash when the name is not a declared property.
pub(super) fn lower_prop_set(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let object = operand(inst, 0)?;
    let value = operand(inst, 1)?;
    let prop_data = data_immediate(inst)?;
    let property = ctx
        .module
        .data
        .strings
        .get(prop_data.as_raw() as usize)
        .cloned()
        .ok_or_else(|| WasmError::Unsupported(format!("invalid string data id {:?}", prop_data)))?;

    // A NULLABLE receiver — `$head->next->next = ...` walking a `?self` chain — arrives boxed,
    // so the object pointer has to be recovered from the cell before the slot can be written.
    if matches!(value_php_type(ctx, object)?, PhpType::Union(_)) {
        return lower_nullable_prop_set(ctx, object, value, prop_data, &property);
    }
    let (class_name, ci) = receiver_class_info(ctx, object)?;
    // Undeclared property on an ADP / stdClass class -> write through the dyn-prop hash tail.
    if let Some(dyn_off) = dynamic_property_hash_offset_for_class(&ci, &class_name, &property) {
        return lower_dyn_prop_set(ctx, inst, object, value, dyn_off, DynPropKey::Literal(prop_data));
    }
    let (_, offset, prop_type) = resolve_property_slot(&ci, &property)?;
    let prop_type = prop_type.codegen_repr();
    let obj_ref = object_ptr_ref(ctx, object)?;

    emit_declared_property_store(ctx, &obj_ref, offset, &prop_type, value, &property)
}

/// Whether storing `value` into a slot of `prop_type` is an array WIDENING, and the pair it needs.
///
/// `array<int>` and `array<mixed>` are different layouts — 8-byte slots against boxed cells — so
/// a declared `array` property (which is `array<mixed>`) assigned a concrete array is a copy
/// rather than a pointer store. The answer doubles as the flag for the incref: the helper's
/// result is FRESH, so the slot already owns it.
fn widens_into_the_slot(
    ctx: &FnCtx,
    value: ValueId,
    prop_type: &PhpType,
) -> Result<Option<(IrType, PhpType)>> {
    if !matches!(prop_type, PhpType::Array(element) if element.codegen_repr() == PhpType::Mixed) {
        return Ok(None);
    }
    let Some((value_ir, value_php)) = ctx
        .function
        .value(value)
        .map(|value| (value.ir_type, value.php_type.codegen_repr()))
    else {
        return Ok(None);
    };
    if value_ir != IrType::Heap(IrHeapKind::Array) {
        return Ok(None);
    }
    match super::transfer::classify_transfer(
        value_ir,
        value_php.clone(),
        IrType::Heap(IrHeapKind::Array),
        prop_type.clone(),
    ) {
        Ok(super::transfer::TransferKind::WidenArrayToMixed { .. }) => {
            Ok(Some((value_ir, value_php)))
        }
        _ => Ok(None),
    }
}

/// Writes a declared property through a receiver whose type is `C|null`.
///
/// The read side already opens one of these (`lower_nullsafe_prop_get`); the write differs only
/// in what the null arm does. PHP does not warn-and-continue there the way a READ does — it
/// raises `Error: Attempt to assign property "x" on null` — so the null arm reports that and
/// exits rather than storing into address zero.
///
/// KNOWN DIVERGENCE, the same one every raise on this target carries: php's `Error` is
/// CATCHABLE. The audit refuses the write where a frame could catch it, so the two only disagree
/// where php would also have terminated.
fn lower_nullable_prop_set(
    ctx: &mut FnCtx,
    object: ValueId,
    value: ValueId,
    prop_data: crate::ir::DataId,
    property: &str,
) -> Result<()> {
    let (class_name, ci) = nullable_receiver_class_info(ctx, object)?;
    if dynamic_property_hash_offset_for_class(&ci, &class_name, property).is_some() {
        return Err(WasmError::Unsupported(format!(
            "dynamic property ${property} written through a nullable {class_name} receiver"
        )));
    }
    let (_, offset, prop_type) = resolve_property_slot(&ci, property)?;
    let prop_type = prop_type.codegen_repr();

    let hi = ctx.fresh_temp(ValType::I64);
    let lo = ctx.fresh_temp(ValType::I64);
    let tag = ctx.fresh_temp(ValType::I64);
    let obj = ctx.fresh_temp(ValType::I32);
    ctx.emit_load_value(object)?;
    ctx.fb
        .ins("call $__rt_mixed_unbox", "unbox nullable receiver -> (tag, lo, hi)");
    ctx.fb
        .ins(&format!("local.set {}", hi), "discard receiver high word");
    ctx.fb.ins(
        &format!("local.set {}", lo),
        "receiver low word (object ptr when non-null)",
    );
    ctx.fb
        .ins(&format!("local.set {}", tag), "receiver runtime tag");
    ctx.fb
        .ins(&format!("local.get {}", tag), "receiver runtime tag");
    ctx.fb.ins("i64.const 8", "null tag");
    ctx.fb.ins("i64.eq", "is the receiver null?");
    ctx.fb.ins("if", "null -> php raises an Error naming the property");
    let (name_ptr, name_len) = ctx.str_literal(prop_data)?;
    ctx.fb.ins(
        &format!(
            "(call $__rt_fail_assign_on_null (i32.const {name_ptr}) (i32.const {name_len}))"
        ),
        "report the uncaught Error and exit 255",
    );
    ctx.fb
        .ins("unreachable", "elephc-trap:post-noreturn:assign-on-null");
    ctx.fb.ins("end", "end of the null arm");
    ctx.fb
        .ins(&format!("local.get {}", lo), "object payload low word");
    ctx.fb.ins("i32.wrap_i64", "low word -> object pointer i32");
    ctx.fb
        .ins(&format!("local.set {}", obj), "save object pointer");

    emit_declared_property_store(ctx, &obj, offset, &prop_type, value, property)
}

/// Resolves the single concrete class a `C|null` receiver names, alongside its `ClassInfo`.
fn nullable_receiver_class_info(ctx: &FnCtx, object: ValueId) -> Result<(String, ClassInfo)> {
    let PhpType::Union(variants) = value_php_type(ctx, object)? else {
        return Err(WasmError::Unsupported(
            "nullable property receiver is not a union".to_string(),
        ));
    };
    let classes = variants
        .iter()
        .filter_map(|variant| match variant {
            PhpType::Object(name) => Some(name.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    let [class_name] = classes.as_slice() else {
        return Err(WasmError::Unsupported(format!(
            "nullable property receiver must name exactly one class, got {:?}",
            variants
        )));
    };
    let ci = ctx
        .module
        .class_infos
        .get(class_name)
        .cloned()
        .ok_or_else(|| WasmError::Unsupported(format!("unknown class {class_name}")))?;
    Ok((class_name.clone(), ci))
}

/// Open-codes the inherited Throwable constructor: each supplied argument is written into its
/// property slot, and an omitted trailing argument leaves the property default in place.
///
/// This is the WASM counterpart of the native `lower_builtin_throwable_new` field writes. The
/// stores go through `emit_declared_property_store`, so a message overwriting the `""` default
/// releases that default's heap copy instead of leaking it — the reason this cannot simply
/// stamp the slots.
fn emit_open_coded_throwable_constructor(
    ctx: &mut FnCtx,
    obj: &str,
    class_info: &ClassInfo,
    class_name: &str,
    args: &[ValueId],
) -> Result<()> {
    if args.len() > THROWABLE_CONSTRUCTOR_PROPERTIES.len() {
        return Err(WasmError::Unsupported(format!(
            "{}::__construct with {} arguments on wasm32-wasi, expected at most {}",
            class_name,
            args.len(),
            THROWABLE_CONSTRUCTOR_PROPERTIES.len()
        )));
    }
    for (&argument, property) in args
        .iter()
        .zip(THROWABLE_CONSTRUCTOR_PROPERTIES.iter())
    {
        let (_, offset, property_type) = resolve_property_slot(class_info, property)?;
        let property_type = property_type.codegen_repr();
        emit_declared_property_store(ctx, obj, offset, &property_type, argument, property)?;
    }
    Ok(())
}

/// Writes one value into a DECLARED property slot, honoring that slot's ownership rules.
///
/// Shared by `PropSet` and by the inherited Throwable constructor that object construction
/// open-codes, so both obey one set of release/retain rules rather than two that can drift.
/// Scalar arms (Int/Bool/Float) write the value directly and zero the tag word. Refcounted
/// arms (Str, Array/AssocArray/Object, Mixed/Union/Iterable) first release whatever the slot
/// already holds (null-safe via `__rt_decref_any`, which is what makes this correct over a
/// slot that a property default already populated), then retain + persist the incoming value
/// and store lo plus the hi-word (runtime tag, or string length). A Mixed/Union/Iterable slot
/// transfers an owned Mixed cell, retains an independently owned borrowed/local-loaded cell,
/// or boxes a concrete value via `emit_box_value_into_mixed`. Resource values and any other
/// declared type return `Unsupported`.
fn emit_declared_property_store(
    ctx: &mut FnCtx,
    obj_ref: &str,
    offset: usize,
    prop_type: &PhpType,
    value: ValueId,
    property: &str,
) -> Result<()> {
    // A Mixed source into a CONCRETE slot narrows the same way a local load does — `$this->n =
    // $this->n + 1` widens the value to Mixed through the checked add, and the slot is still an
    // int. Narrowing here rather than refusing keeps property counters working, and goes through
    // the shared transfer layer so the rule lives in one place.
    let narrows_from_mixed = matches!(
        ctx.function.value(value).map(|v| v.ir_type),
        Some(IrType::Heap(IrHeapKind::Mixed))
    ) && !matches!(
        prop_type,
        PhpType::Mixed | PhpType::Union(_) | PhpType::Iterable
    );
    match prop_type {
        PhpType::Int | PhpType::Bool => {
            ctx.fb.ins(&format!("local.get {}", obj_ref), "object base address");
            if narrows_from_mixed {
                super::transfer::emit_narrow_mixed_into(ctx, value, prop_type)?;
            } else {
                ctx.emit_load_value(value)?;
            }
            ctx.fb.ins(&format!("i64.store offset={}", offset), "store scalar property value_lo");
            ctx.fb.ins(&format!("local.get {}", obj_ref), "object base address");
            ctx.fb.ins("i64.const 0", "zero tag word");
            ctx.fb.ins(&format!("i64.store offset={}", offset + 8), "store property value_hi (tag = 0)");
        }
        PhpType::Float => {
            ctx.fb.ins(&format!("local.get {}", obj_ref), "object base address");
            if narrows_from_mixed {
                super::transfer::emit_narrow_mixed_into(ctx, value, prop_type)?;
            } else {
                ctx.emit_load_value(value)?;
            }
            ctx.fb.ins(&format!("f64.store offset={}", offset), "store float property value_lo");
            ctx.fb.ins(&format!("local.get {}", obj_ref), "object base address");
            ctx.fb.ins("i64.const 0", "zero tag word");
            ctx.fb.ins(&format!("i64.store offset={}", offset + 8), "store property value_hi (tag = 0)");
        }
        // A tagged slot owns nothing, so there is no previous value to release: the two words
        // are simply overwritten. `$this->v = 5;` and `$this->v = null;` both arrive in another
        // shape, and the transfer table makes the pair out of either.
        PhpType::TaggedScalar => {
            let tag = ctx.fresh_temp(ValType::I32);
            let payload = ctx.fresh_temp(ValType::I64);
            let (value_ir, value_php) = ctx
                .function
                .value(value)
                .map(|v| (v.ir_type, v.php_type.codegen_repr()))
                .ok_or_else(|| {
                    WasmError::Unsupported(format!("property store operand {value:?} missing"))
                })?;
            ctx.emit_load_value(value)?;
            let _ = super::transfer::emit_convert_stack_value(
                ctx,
                value_ir,
                value_php,
                IrType::TaggedScalar,
                PhpType::TaggedScalar,
            )?;
            // The pair carries the cell's two facts and not the cell. The EIR emits no release
            // for a value stored into a nullable slot — it reads that slot as one that TAKES
            // its value, which is true of the boxed shape and not of this one — so the store
            // gives it back, exactly as the concrete arms do through the same helper.
            // Measured: `$o->v = 70000 + $i;` on a `?int` grew 40 bytes an iteration without
            // this, one leaked cell per store, while the same loop on a plain `int` was flat.
            super::inst::release_consumed_static_source(
                ctx,
                value,
                value_ir == IrType::Heap(IrHeapKind::Mixed),
            )?;
            ctx.fb.ins(&format!("local.set {}", tag), "incoming tag");
            ctx.fb.ins(&format!("local.set {}", payload), "incoming payload");
            ctx.fb.ins(&format!("local.get {}", obj_ref), "object base address");
            ctx.fb.ins(&format!("local.get {}", payload), "the payload");
            ctx.fb.ins(
                &format!("i64.store offset={}", offset),
                "store the tagged payload",
            );
            ctx.fb.ins(&format!("local.get {}", obj_ref), "object base address");
            ctx.fb.ins(&format!("local.get {}", tag), "the tag");
            // Written as the whole EIGHT bytes, though the load reads the low four: the high half
            // is the UNSET marker's, and a four-byte store would leave half of it behind for the
            // `isset` probe to read. Little-endian, so the tag still lands where the i32 load looks.
            ctx.fb.ins("i64.extend_i32_u", "widen the tag to the full word");
            ctx.fb.ins(
                &format!("i64.store offset={}", offset + 8),
                "store the tag beside it, clearing any unset marker",
            );
        }
        PhpType::Str => {
            ctx.fb.ins(&format!("local.get {}", obj_ref), "object base address");
            ctx.fb.ins(&format!("i32.load offset={}", offset), "load previous slot value ptr");
            ctx.fb.ins("call $__rt_decref_any", "release previous slot value (null-safe)");
            if narrows_from_mixed {
                // `__rt_mixed_cast_string` already persists — the (ptr,len) it leaves on the
                // stack is the slot's own copy, and a second `__rt_str_persist` would strand it.
                super::transfer::emit_narrow_mixed_into(ctx, value, prop_type)?;
            } else {
                ctx.emit_load_value(value)?;
                ctx.fb.ins("call $__rt_str_persist", "persist string copy (ptr,len) -> (new_ptr,new_len)");
            }
            let len_tmp = ctx.fresh_temp(ValType::I64);
            let ptr_tmp = ctx.fresh_temp(ValType::I32);
            ctx.fb.ins(&format!("local.set {}", len_tmp), "save persisted string len");
            ctx.fb.ins(&format!("local.set {}", ptr_tmp), "save persisted string ptr");
            ctx.fb.ins(&format!("local.get {}", obj_ref), "object base address");
            ctx.fb.ins(&format!("local.get {}", ptr_tmp), "persisted string ptr");
            ctx.fb.ins("i64.extend_i32_u", "widen string ptr to i64 lo");
            ctx.fb.ins(&format!("i64.store offset={}", offset), "store string property value_lo (ptr)");
            ctx.fb.ins(&format!("local.get {}", obj_ref), "object base address");
            ctx.fb.ins(&format!("local.get {}", len_tmp), "persisted string len");
            ctx.fb.ins(&format!("i64.store offset={}", offset + 8), "store string property value_hi (len)");
        }
        PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Object(_) => {
            ctx.fb.ins(&format!("local.get {}", obj_ref), "object base address");
            ctx.fb.ins(&format!("i32.load offset={}", offset), "load previous slot value ptr");
            ctx.fb.ins("call $__rt_decref_any", "release previous slot value (null-safe)");
            // A concrete-element array into an `array<mixed>` slot is a real conversion: the two
            // layouts differ, so the helper builds a FRESH array — which is already the one
            // reference the slot takes, and increfing it again would leave one per store behind.
            let widened = widens_into_the_slot(ctx, value, prop_type)?;
            if narrows_from_mixed {
                // The tag-checked unbox yields the heap pointer BORROWED from the cell; the
                // shared incref below then makes the slot its own owner. Storing the raw cell
                // pointer here would validate — both are i32 — and corrupt every later read.
                super::transfer::emit_narrow_mixed_into(ctx, value, prop_type)?;
            } else {
                ctx.emit_load_value(value)?;
                if let Some((value_ir, value_php)) = widened.clone() {
                    let _ = super::transfer::emit_convert_stack_value(
                        ctx,
                        value_ir,
                        value_php,
                        IrType::Heap(IrHeapKind::Array),
                        prop_type.clone(),
                    )?;
                }
            }
            let ptr_tmp = ctx.fresh_temp(ValType::I32);
            ctx.fb.ins(&format!("local.set {}", ptr_tmp), "save container cell ptr");
            if widened.is_none() {
                ctx.fb.ins(&format!("local.get {}", ptr_tmp), "container cell ptr");
                ctx.fb.ins("call $__rt_incref", "retain new container cell (refcount++)");
            }
            ctx.fb.ins(&format!("local.get {}", obj_ref), "object base address");
            ctx.fb.ins(&format!("local.get {}", ptr_tmp), "container cell ptr");
            ctx.fb.ins("i64.extend_i32_u", "widen cell ptr to i64 lo");
            ctx.fb.ins(&format!("i64.store offset={}", offset), "store container property value_lo (ptr)");
            let tag = crate::codegen::runtime_value_tag(&prop_type) as i64;
            ctx.fb.ins(&format!("local.get {}", obj_ref), "object base address");
            ctx.fb.ins(&format!("i64.const {}", tag), "container runtime tag (hi word)");
            ctx.fb.ins(&format!("i64.store offset={}", offset + 8), "store container property value_hi (tag)");
        }
        PhpType::Mixed | PhpType::Union(_) | PhpType::Iterable => {
            ctx.fb.ins(&format!("local.get {}", obj_ref), "object base address");
            ctx.fb.ins(&format!("i32.load offset={}", offset), "load previous slot value ptr");
            ctx.fb.ins("call $__rt_decref_any", "release previous slot value (null-safe)");
            let value_repr = ctx.value_repr(value)?.clone();
            let (value_ir_type, value_ownership) = ctx
                .function
                .value(value)
                .map(|value| (value.ir_type, value.ownership))
                .ok_or_else(|| {
                    WasmError::Unsupported("mixed property value is missing".to_string())
                })?;
            let is_mixed_cell = matches!(&value_repr, WasmRepr::Ptr(_))
                && value_ir_type == IrType::Heap(IrHeapKind::Mixed);
            if is_mixed_cell {
                if !matches!(
                    value_ownership,
                    Ownership::Owned | Ownership::Borrowed | Ownership::Persistent
                ) && !(value_ownership == Ownership::MaybeOwned
                    && value_has_local_origin(ctx.function, value))
                {
                    return Err(WasmError::Unsupported(format!(
                        "mixed property write with {:?} ownership",
                        value_ownership
                    )));
                }
                ctx.emit_load_value(value)?;
                let cell_tmp = ctx.fresh_temp(ValType::I32);
                ctx.fb.ins(&format!("local.set {}", cell_tmp), "save mixed cell ptr");
                if value_ownership != Ownership::Owned {
                    ctx.fb.ins(&format!("local.get {}", cell_tmp), "borrowed mixed cell ptr");
                    ctx.fb.ins("call $__rt_incref", "retain borrowed mixed cell for property");
                }
                ctx.fb.ins(&format!("local.get {}", obj_ref), "object base address");
                ctx.fb.ins(&format!("local.get {}", cell_tmp), "mixed property cell ptr");
                ctx.fb.ins("i64.extend_i32_u", "widen cell ptr to i64 lo");
                ctx.fb.ins(&format!("i64.store offset={}", offset), "store mixed property value_lo (ptr)");
                ctx.fb.ins(&format!("local.get {}", obj_ref), "object base address");
                ctx.fb.ins("i64.const 7", "mixed runtime tag (hi word)");
                ctx.fb.ins(&format!("i64.store offset={}", offset + 8), "store mixed property value_hi (tag)");
            } else {
                let cell_tmp = emit_box_value_into_mixed(ctx, value)?;
                ctx.fb.ins(&format!("local.get {}", obj_ref), "object base address");
                ctx.fb.ins(&format!("local.get {}", cell_tmp), "fresh kind-5 cell ptr");
                ctx.fb.ins("i64.extend_i32_u", "widen cell ptr to i64 lo");
                ctx.fb.ins(&format!("i64.store offset={}", offset), "store mixed property value_lo (ptr)");
                ctx.fb.ins(&format!("local.get {}", obj_ref), "object base address");
                ctx.fb.ins("i64.const 7", "mixed runtime tag (hi word)");
                ctx.fb.ins(&format!("i64.store offset={}", offset + 8), "store mixed property value_hi (tag)");
            }
        }
        other => {
            return Err(WasmError::Unsupported(format!(
                "property ${} of type {:?} not yet supported on wasm32-wasi",
                property, other
            )))
        }
    }

    Ok(())
}

/// Lowers `Op::PropGet` of an UNDECLARED property on an ADP / `stdClass` class to a
/// `__rt_hash_get` of the property name against the dynamic-property hash tail.
///
/// The dyn hash pointer lives at `[obj + dyn_off]` (an i32). The key is the property-name
/// string `(zext ptr, len)` borrowed from static data (`__rt_hash_get` owns no key copy).
/// A hit returns `(1, cell_ptr, 0, 7)`; the stored Mixed cell is incref'd so the read result
/// owns it (the result slot is `Mixed`). A miss returns `(0, 0, 0, 8)`; a fresh null Mixed cell
/// is boxed via `__rt_mixed_from_value(8, 0, 0)` and stored (matching PHP's null for a missing
/// dynamic property).
/// Resolves the receiver and hash offset a `$obj->$name` access uses, or says why it cannot.
///
/// A COMPUTED name can only be routed to the hash when nothing else could answer it: the class
/// must carry a dyn-prop tail, and it must declare no property slots at all. With a declared slot
/// present the name might name THAT, and deciding which at run time is a chain of string compares
/// this does not emit — so it is refused rather than sent to the hash, where a declared property
/// would silently read as absent. The native backend draws the same line.
fn dynamic_access_target(ctx: &FnCtx, object: ValueId) -> Result<(String, usize)> {
    let (class_name, ci) = receiver_class_info(ctx, object)?;
    if !ci.properties.is_empty() {
        return Err(WasmError::Unsupported(format!(
            "a computed property name on {class_name}, which declares {} propert{} the name could \
             also reach",
            ci.properties.len(),
            if ci.properties.len() == 1 { "y" } else { "ies" }
        )));
    }
    let dyn_off = dynamic_property_hash_offset_for_class(&ci, &class_name, "")
        .ok_or_else(|| {
            WasmError::Unsupported(format!(
                "a computed property name on {class_name}, which carries no dynamic-property storage"
            ))
        })?;
    Ok((class_name, dyn_off))
}

/// Lowers `Op::DynamicPropGet`: `$obj->$name` where the name is computed.
///
/// Identical to the literal read but for where the key comes from, which is the whole point of
/// `DynPropKey`: the hash, the ownership of the stored cell, the miss answer and the warning all
/// have one implementation, so the two spellings of the same PHP cannot come to disagree.
pub(super) fn lower_dynamic_prop_get(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let object = operand(inst, 0)?;
    let name = operand(inst, 1)?;
    // A literal name on a mixed receiver is `$mixed->prop` (see `dynamic_property_shape_issue`).
    if matches!(value_php_type(ctx, object)?.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) {
        if let Some(prop_data) = super::capability::literal_property_name_data(ctx.function, inst) {
            let property = ctx
                .module
                .data
                .strings
                .get(prop_data.as_raw() as usize)
                .cloned()
                .ok_or_else(|| WasmError::Unsupported(format!("invalid string data id {prop_data:?}")))?;
            return lower_mixed_prop_get(ctx, inst, object, &property, prop_data);
        }
    }
    let (_, dyn_off) = dynamic_access_target(ctx, object)?;
    lower_dyn_prop_get(ctx, inst, object, dyn_off, DynPropKey::Computed(name))
}

/// Lowers `Op::DynamicPropSet`: `$obj->$name = $v` where the name is computed.
pub(super) fn lower_dynamic_prop_set(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let object = operand(inst, 0)?;
    let name = operand(inst, 1)?;
    let value = operand(inst, 2)?;
    let (_, dyn_off) = dynamic_access_target(ctx, object)?;
    lower_dyn_prop_set(ctx, inst, object, value, dyn_off, DynPropKey::Computed(name))
}

/// Where a dynamic-property access gets the name it looks up.
///
/// The two differ in nothing but this. `$obj->name` has the bytes in static data; `$obj->$name`
/// has them in a local at run time — and every other question (which hash, what the miss answers,
/// who owns the stored cell) has one answer, which is why both go through the same two helpers.
#[derive(Clone, Copy)]
pub(super) enum DynPropKey {
    /// A name the program wrote literally, laid out in the data segments.
    Literal(DataId),
    /// A name computed at run time, arriving as a `(pointer, length)` string value.
    Computed(ValueId),
}

/// Pushes `(key_lo, key_hi)` for the hash helpers: the name's pointer zero-extended, then its
/// length.
fn emit_dyn_prop_key(ctx: &mut FnCtx, key: DynPropKey) -> Result<()> {
    match key {
        DynPropKey::Literal(prop_data) => {
            let (kptr, klen) = ctx.str_literal(prop_data)?;
            ctx.fb
                .ins(&format!("i32.const {}", kptr), "property name pointer (static data)");
            ctx.fb.ins("i64.extend_i32_u", "key_lo = zext(name ptr)");
            ctx.fb.ins(&format!("i64.const {}", klen), "key_hi = name length");
        }
        DynPropKey::Computed(name) => {
            let WasmRepr::Str { ptr, len } = ctx.value_repr(name)?.clone() else {
                return Err(WasmError::Unsupported(
                    "a dynamic property name must be a string value".to_string(),
                ));
            };
            ctx.fb
                .ins(&format!("local.get {}", ptr), "computed property name pointer");
            ctx.fb.ins("i64.extend_i32_u", "key_lo = zext(name ptr)");
            ctx.fb
                .ins(&format!("local.get {}", len), "key_hi = name length");
        }
    }
    Ok(())
}

/// Whether this read exists to ask WHETHER the property is set.
///
/// `isset($o->$k)`, `empty($o->$k)` and `$o->$k ?? $d` all read the property and then test the
/// value, and php emits no undefined-property warning for any of them — the question they ask is
/// exactly the one the warning would answer. The EIR carries no flag for that, so the test is on
/// the value's USES: a null test on the read's own value, or an `isset`/`empty` construct over it.
///
/// A null test DIRECTLY on the read's value is the shape only those three produce. `$x = $o->$k;`
/// followed by `$x === null` tests the SLOT's later load instead, which is a different value — so
/// that read still warns, as php warns for it.
///
/// One matching use is enough, and that is deliberate: the `??` value arm goes on to `acquire` the
/// very same value, which is a use that asks nothing. Requiring EVERY use to be a question left
/// `$o->$k ?? "d"` warning where php is silent.
fn read_only_asks_whether_it_is_set(ctx: &FnCtx, result: Option<ValueId>) -> bool {
    let Some(result) = result else {
        return false;
    };
    for inst in &ctx.function.instructions {
        if !inst.operands.contains(&result) || inst.op == Op::Release {
            continue;
        }
        let asks = match inst.op {
            // `??` tests the value for null and takes the default arm.
            Op::IsNull => true,
            // `isset`/`empty` reach the backend as a construct call over the read value, and
            // the construct's NAME is the data immediate — `exit($o->$k)` is a construct call too
            // and php warns for that one.
            Op::LanguageConstructCall => match inst.immediate {
                // The construct's name lives in `function_names`, NOT in the string pool: the data
                // id indexes both, and `strings` at that index is an unrelated literal — looking it
                // up there answered "" for every construct and suppressed nothing.
                Some(Immediate::Data(data)) => ctx
                    .module
                    .data
                    .function_names
                    .get(data.as_raw() as usize)
                    .is_some_and(|name| {
                        name.eq_ignore_ascii_case("isset") || name.eq_ignore_ascii_case("empty")
                    }),
                _ => false,
            },
            _ => false,
        };
        if asks {
            return true;
        }
    }
    false
}

/// Emits the undefined-property warning php gives for a dynamic read that missed.
///
/// The class comes from the receiver's own id word rather than from a literal, so one helper and
/// no per-class bytes serve every class. A LITERAL name still needs an address, which it has.
fn emit_undefined_property_warning(
    ctx: &mut FnCtx,
    obj_ref: &str,
    key: DynPropKey,
) -> Result<()> {
    ctx.fb
        .ins(&format!("local.get {}", obj_ref), "object base address");
    ctx.fb
        .ins("i64.load", "the receiver's class id, at payload offset zero");
    match key {
        DynPropKey::Literal(prop_data) => {
            let (kptr, klen) = ctx.str_literal(prop_data)?;
            ctx.fb.ins(&format!("i32.const {}", kptr), "property name pointer");
            ctx.fb.ins(&format!("i32.const {}", klen), "property name length");
        }
        DynPropKey::Computed(name) => {
            let WasmRepr::Str { ptr, len } = ctx.value_repr(name)?.clone() else {
                return Err(WasmError::Unsupported(
                    "a dynamic property name must be a string value".to_string(),
                ));
            };
            ctx.fb.ins(&format!("local.get {}", ptr), "property name pointer");
            ctx.fb
                .ins(&format!("local.get {}", len), "property name length");
            ctx.fb.ins("i32.wrap_i64", "the helper takes an i32 length");
        }
    }
    ctx.fb.ins(
        "call $__rt_warn_undefined_property",
        "Warning: Undefined property: C::$name",
    );
    Ok(())
}

/// Lowers a dynamic-property read: looks the key up in the dynamic-property hash at `dyn_off` and
/// narrows the cell it finds to the instruction's result type.
fn lower_dyn_prop_get(
    ctx: &mut FnCtx,
    inst: &Instruction,
    object: ValueId,
    dyn_off: usize,
    key: DynPropKey,
) -> Result<()> {
    let obj_ref = object_ptr_ref(ctx, object)?;
    let warn = !read_only_asks_whether_it_is_set(ctx, inst.result);
    emit_dyn_prop_read(ctx, &obj_ref, dyn_off, key, warn)?;
    // The EIR narrows the read when it has seen the write — `$b->$k = 7; echo $b->$k;` is typed
    // `int` — and the cell both arms produced is not that shape. Opening it here is the same tail
    // a nullable property read carries, including the FREE: the concrete value borrows the
    // payload, so the cell that carried it has no owner left.
    if let Some(result) = inst.result {
        let (result_ir, result_php) = ctx
            .function
            .value(result)
            .map(|value| (value.ir_type, value.php_type.codegen_repr()))
            .ok_or_else(|| WasmError::Unsupported("dynamic property result is missing".to_string()))?;
        if result_ir != IrType::Heap(IrHeapKind::Mixed) {
            let cell = ctx.fresh_temp(ValType::I32);
            ctx.fb.ins(&format!("local.set {cell}"), "the looked-up cell");
            ctx.fb.ins(&format!("local.get {cell}"), "the looked-up cell");
            let _ = super::transfer::emit_convert_stack_value(
                ctx,
                IrType::Heap(IrHeapKind::Mixed),
                PhpType::Mixed,
                result_ir,
                result_php,
            )?;
            // Into the result's OWN locals rather than a parking pair: that is what makes the
            // free below safe for every width, since nothing is left on the stack to reorder.
            ctx.emit_store_value(result)?;
            ctx.fb.ins(
                &format!("(call $__rt_decref_any (local.get {cell}))"),
                "the cell the narrowed value was read out of",
            );
            return Ok(());
        }
    }
    store_result(ctx, inst)?;
    Ok(())
}

/// Lowers `Op::PropSet` of an UNDECLARED property on an ADP / `stdClass` class to a
/// `__rt_hash_set` against the dynamic-property hash tail.
///
/// A pre-boxed Mixed value is passed directly to `__rt_hash_set`; concrete values are first boxed.
/// The hash helper retains the cell. An owned or freshly boxed inbound reference is then dropped,
/// while a borrowed/local-loaded cell keeps its independent source owner. The returned hash pointer
/// is written back to `[obj + dyn_off]` as i64. `PropSet` is void.
fn lower_dyn_prop_set(
    ctx: &mut FnCtx,
    _inst: &Instruction,
    object: ValueId,
    value: ValueId,
    dyn_off: usize,
    key: DynPropKey,
) -> Result<()> {
    let obj_ref = object_ptr_ref(ctx, object)?;
    // Reuse an existing Mixed cell or box a concrete value. hash_set retains tag-7 cells.
    let value_repr = ctx.value_repr(value)?.clone();
    let (value_ir_type, value_ownership) = ctx
        .function
        .value(value)
        .map(|value| (value.ir_type, value.ownership))
        .ok_or_else(|| WasmError::Unsupported("dynamic property value is missing".to_string()))?;
    let is_mixed_cell = matches!(&value_repr, WasmRepr::Ptr(_))
        && value_ir_type == IrType::Heap(IrHeapKind::Mixed);
    if is_mixed_cell
        && !matches!(
            value_ownership,
            Ownership::Owned | Ownership::Borrowed | Ownership::Persistent
        )
        && !(value_ownership == Ownership::MaybeOwned
            && value_has_local_origin(ctx.function, value))
    {
        return Err(WasmError::Unsupported(format!(
            "dynamic mixed property write with {:?} ownership",
            value_ownership
        )));
    }
    let cell = if is_mixed_cell {
        ctx.emit_load_value(value)?;
        let c = ctx.fresh_temp(ValType::I32);
        ctx.fb.ins(&format!("local.set {}", c), "save mixed cell ptr");
        c
    } else {
        emit_box_value_into_mixed(ctx, value)?
    };
    // Load the dyn hash, call __rt_hash_set, write back the (possibly new) hash pointer.
    ctx.fb.ins(&format!("local.get {}", obj_ref), "object base address");
    ctx.fb.ins(&format!("i32.load offset={}", dyn_off), "load dynamic-property hash pointer");
    let hash = ctx.fresh_temp(ValType::I32);
    ctx.fb.ins(&format!("local.set {}", hash), "save dyn-prop hash pointer");
    ctx.fb.ins(&format!("local.get {}", hash), "dyn-prop hash pointer");
    emit_dyn_prop_key(ctx, key)?;
    ctx.fb.ins(&format!("local.get {}", cell), "boxed mixed value cell ptr");
    ctx.fb.ins("i64.extend_i32_u", "val_lo = zext(cell ptr)");
    ctx.fb.ins("i64.const 0", "val_hi = 0 (mixed cell)");
    ctx.fb.ins("i64.const 7", "val_tag = 7 (mixed)");
    ctx.fb.ins("call $__rt_hash_set", "store the dynamic property -> (possibly moved) hash ptr");
    let new_hash = ctx.fresh_temp(ValType::I32);
    ctx.fb.ins(&format!("local.set {}", new_hash), "capture possibly-reallocated hash ptr");
    ctx.fb.ins(&format!("local.get {}", obj_ref), "object base address (writeback addr)");
    ctx.fb.ins(&format!("local.get {}", new_hash), "writeback hash pointer");
    ctx.fb.ins("i64.extend_i32_u", "widen hash ptr to i64 for writeback");
    ctx.fb.ins(&format!("i64.store offset={}", dyn_off), "store back the dyn-prop hash ptr");
    // Drop only a freshly materialized or transferred owned reference. Borrowed/local-loaded
    // cells keep their source owner alongside the new hash owner.
    if !is_mixed_cell || value_ownership == Ownership::Owned {
        ctx.fb.ins(&format!("local.get {}", cell), "inbound cell temp for refcount balance");
        ctx.fb.ins("call $__rt_decref_any", "release the temp ref that __rt_hash_set replaced");
    }
    Ok(())
}

/// Traces ownership-forwarding values to a local load that remains an independent owner.
fn value_has_local_origin(function: &Function, mut value: ValueId) -> bool {
    for _ in 0..=function.values.len() {
        let Some(value_data) = function.value(value) else {
            return false;
        };
        let ValueDef::Instruction { inst, .. } = value_data.def else {
            return false;
        };
        let Some(instruction) = function.instruction(inst) else {
            return false;
        };
        match instruction.op {
            Op::LoadLocal => {
                return matches!(instruction.immediate, Some(Immediate::LocalSlot(_)));
            }
            Op::Move | Op::Borrow | Op::Acquire => {
                let Some(source) = instruction.operands.first() else {
                    return false;
                };
                value = *source;
            }
            _ => return false,
        }
    }
    false
}

/// Emits a property read that leaves an OWNED Mixed cell pointer (i32) on the stack.
///
/// Used by `lower_nullsafe_prop_get` (the result of `NullsafePropGet` is always `Mixed`).
/// When the property is undeclared on an ADP / `stdClass` class, it reads through the dyn-prop
/// hash (hit -> incref the stored cell; miss -> box null). Otherwise it loads the declared slot
/// and boxes it into a Mixed cell by property type: scalars via `__rt_mixed_from_value`, strings
/// via `__rt_mixed_from_value(1, ptr, len)` (persists a copy), containers (array/hash/object) via
/// `__rt_mixed_from_value(tag, ptr, 0)` (increfs the child itself), and an already-Mixed slot via
/// a plain incref + return-the-cell (no re-box, avoiding a double wrapper).
fn emit_prop_get_into_mixed(
    ctx: &mut FnCtx,
    obj: &str,
    ci: &ClassInfo,
    class_name: &str,
    prop_data: DataId,
    property: &str,
) -> Result<()> {
    // Dynamic-property hash tail (ADP / stdClass, undeclared property).
    if let Some(dyn_off) = dynamic_property_hash_offset_for_class(ci, class_name, property) {
        let (kptr, klen) = ctx.str_literal(prop_data)?;
        ctx.fb.ins(&format!("local.get {}", obj), "object base address");
        ctx.fb.ins(&format!("i32.load offset={}", dyn_off), "load dynamic-property hash pointer");
        let hash = ctx.fresh_temp(ValType::I32);
        ctx.fb.ins(&format!("local.set {}", hash), "save dyn-prop hash pointer");
        ctx.fb.ins(&format!("local.get {}", hash), "dyn-prop hash pointer");
        ctx.fb.ins(&format!("i32.const {}", kptr), "property name pointer (static data)");
        ctx.fb.ins("i64.extend_i32_u", "key_lo = zext(name ptr)");
        ctx.fb.ins(&format!("i64.const {}", klen), "key_hi = name length");
        ctx.fb.ins("call $__rt_hash_get", "look up the dynamic property -> (found, vlo, vhi, vtag)");
        let vtag = ctx.fresh_temp(ValType::I64);
        ctx.fb.ins(&format!("local.set {}", vtag), "captured value tag");
        let vhi = ctx.fresh_temp(ValType::I64);
        ctx.fb.ins(&format!("local.set {}", vhi), "captured value high word");
        let vlo = ctx.fresh_temp(ValType::I64);
        ctx.fb.ins(&format!("local.set {}", vlo), "captured value low word (cell ptr)");
        let found = ctx.fresh_temp(ValType::I32);
        ctx.fb.ins(&format!("local.set {}", found), "captured found flag");
        ctx.fb.ins(&format!("local.get {}", found), "found flag");
        ctx.fb.ins("if (result i32)", "dynamic property present?");
        ctx.fb.ins(&format!("local.get {}", vlo), "stored mixed cell ptr (i64)");
        ctx.fb.ins("i32.wrap_i64", "cell ptr -> i32");
        let cell = ctx.fresh_temp(ValType::I32);
        ctx.fb.ins(&format!("local.set {}", cell), "save hit cell ptr");
        ctx.fb.ins(&format!("local.get {}", cell), "hit cell ptr");
        ctx.fb.ins("call $__rt_incref", "retain the borrowed dyn-prop cell");
        ctx.fb.ins(&format!("local.get {}", cell), "hit cell ptr for result");
        ctx.fb.ins("else", "miss -> box null");
        ctx.fb.ins("i64.const 8", "mixed tag (null)");
        ctx.fb.ins("i64.const 0", "null lo");
        ctx.fb.ins("i64.const 0", "null hi");
        ctx.fb.ins("call $__rt_mixed_from_value", "box a fresh null mixed cell");
        ctx.fb.ins("end", "end hit/miss");
        return Ok(());
    }
    // Declared property: load the slot and box into a Mixed cell.
    let (_, offset, prop_type) = resolve_property_slot(ci, property)?;
    let prop_type = prop_type.codegen_repr();
    match &prop_type {
        PhpType::Int => {
            ctx.fb.ins("i64.const 0", "mixed tag (int)");
            ctx.fb.ins(&format!("local.get {}", obj), "object base address");
            ctx.fb.ins(&format!("i64.load offset={}", offset), "load int property value_lo");
            ctx.fb.ins("i64.const 0", "hi unused");
            ctx.fb.ins("call $__rt_mixed_from_value", "box int into a mixed cell");
        }
        PhpType::Bool => {
            ctx.fb.ins("i64.const 3", "mixed tag (bool)");
            ctx.fb.ins(&format!("local.get {}", obj), "object base address");
            ctx.fb.ins(&format!("i64.load offset={}", offset), "load bool property value_lo");
            ctx.fb.ins("i64.const 0", "hi unused");
            ctx.fb.ins("call $__rt_mixed_from_value", "box bool into a mixed cell");
        }
        PhpType::Float => {
            ctx.fb.ins("i64.const 2", "mixed tag (float)");
            ctx.fb.ins(&format!("local.get {}", obj), "object base address");
            ctx.fb.ins(&format!("f64.load offset={}", offset), "load float property value_lo");
            ctx.fb.ins("i64.reinterpret_f64", "float bits -> i64 lo");
            ctx.fb.ins("i64.const 0", "hi unused");
            ctx.fb.ins("call $__rt_mixed_from_value", "box float into a mixed cell");
        }
        PhpType::Str => {
            ctx.fb.ins("i64.const 1", "mixed tag (string)");
            ctx.fb.ins(&format!("local.get {}", obj), "object base address");
            ctx.fb.ins(&format!("i32.load offset={}", offset), "load string property ptr");
            ctx.fb.ins("i64.extend_i32_u", "ptr -> lo");
            ctx.fb.ins(&format!("local.get {}", obj), "object base address");
            ctx.fb.ins(&format!("i64.load offset={}", offset + 8), "load string property len -> hi");
            ctx.fb.ins("call $__rt_mixed_from_value", "box string (persists a copy)");
        }
        PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Object(_) => {
            let tag = crate::codegen::runtime_value_tag(&prop_type) as i64;
            ctx.fb.ins(&format!("i64.const {}", tag), "mixed tag (heap kind)");
            ctx.fb.ins(&format!("local.get {}", obj), "object base address");
            ctx.fb.ins(&format!("i32.load offset={}", offset), "load container property ptr");
            ctx.fb.ins("i64.extend_i32_u", "ptr -> lo");
            ctx.fb.ins("i64.const 0", "hi unused");
            ctx.fb.ins("call $__rt_mixed_from_value", "box container (increfs the child)");
        }
        PhpType::Mixed | PhpType::Union(_) | PhpType::Iterable => {
            // The slot already holds a Mixed cell; incref + return the cell (no re-box).
            ctx.fb.ins(&format!("local.get {}", obj), "object base address");
            ctx.fb.ins(&format!("i32.load offset={}", offset), "load mixed property cell ptr");
            let c = ctx.fresh_temp(ValType::I32);
            ctx.fb.ins(&format!("local.set {}", c), "save mixed cell ptr");
            ctx.fb.ins(&format!("local.get {}", c), "mixed cell ptr");
            ctx.fb.ins("call $__rt_incref", "retain the borrowed mixed cell");
            ctx.fb.ins(&format!("local.get {}", c), "mixed cell ptr for result");
        }
        other => {
            return Err(WasmError::Unsupported(format!(
                "nullsafe property ${} of type {:?} not yet supported on wasm32-wasi",
                property, other
            )))
        }
    }
    Ok(())
}

/// Lowers `Op::NullsafePropGet` (`$o?->p`) for a concrete-object or nullable-object receiver.
///
/// `NullsafePropGet` operands are `[receiver]` with the property name as the data immediate; the
/// result is always a `Mixed` cell (the EIR lowering forces it). A non-nullable `Object(C)`
/// receiver reads the property (declared or dyn) and boxes it into a Mixed cell directly. A
/// nullable `Union([Object(C), Void])` receiver is a Mixed cell: `__rt_mixed_unbox` yields the
/// `(tag, lo, hi)` triple, and a `tag == 8` (null) short-circuits to a fresh null Mixed cell; a
/// `tag == 6` (object) reads the property off `lo` (the object pointer) and boxes it. Non-object
/// or object-less union receivers return `Unsupported`, mirroring the native backend.
pub(super) fn lower_nullsafe_prop_get(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let receiver = operand(inst, 0)?;
    let prop_data = data_immediate(inst)?;
    let property = ctx
        .module
        .data
        .strings
        .get(prop_data.as_raw() as usize)
        .cloned()
        .ok_or_else(|| WasmError::Unsupported(format!("invalid string data id {:?}", prop_data)))?;
    let receiver_ty = value_php_type(ctx, receiver)?;

    // Resolve the receiver class. Non-nullable Object reads the property directly; a nullable
    // Union([Object(C), Void]) unboxes the Mixed cell and short-circuits on null. A bare Mixed /
    // non-object receiver is unsupported (mirrors native).
    let (class_name, nullable) = match &receiver_ty {
        PhpType::Object(name) => (name.clone(), false),
        PhpType::Union(variants) => {
            let obj_name = variants.iter().find_map(|v| {
                if let PhpType::Object(n) = v {
                    Some(n.clone())
                } else {
                    None
                }
            });
            match obj_name {
                Some(n) => (n, true),
                None => {
                    return Err(WasmError::Unsupported(
                        "nullsafe property access on a union without an object variant".to_string(),
                    ))
                }
            }
        }
        other => {
            return Err(WasmError::Unsupported(format!(
                "nullsafe property access on non-object receiver {:?}",
                other
            )))
        }
    };
    let ci = ctx
        .module
        .class_infos
        .get(&class_name)
        .cloned()
        .ok_or_else(|| WasmError::Unsupported(format!("unknown class {}", class_name)))?;

    if !nullable {
        // Non-nullable Object: read the property (declared or dyn) and box into the Mixed result.
        // EIR normally emits a plain PropGet here, but handle it safely so a stray nullsafe op
        // does not miscompile.
        let obj_ref = object_ptr_ref(ctx, receiver)?;
        emit_prop_get_into_mixed(ctx, &obj_ref, &ci, &class_name, prop_data, &property)?;
        return store_result(ctx, inst);
    }

    // Nullable: unbox the Mixed-cell receiver. tag 8 -> null -> box null; tag 6 -> object ->
    // read the property and box into the Mixed result. A plain `->` additionally WARNS on the
    // null arm, which is what php does there — it does not fatal.
    let hi = ctx.fresh_temp(ValType::I64);
    let lo = ctx.fresh_temp(ValType::I64);
    let tag = ctx.fresh_temp(ValType::I64);
    ctx.emit_load_value(receiver)?;
    ctx.fb.ins("call $__rt_mixed_unbox", "unbox nullsafe receiver -> (tag, lo, hi)");
    ctx.fb.ins(&format!("local.set {}", hi), "discard receiver high word");
    ctx.fb.ins(&format!("local.set {}", lo), "receiver low word (object ptr when non-null)");
    ctx.fb.ins(&format!("local.set {}", tag), "receiver runtime tag");
    ctx.fb.ins(&format!("local.get {}", tag), "receiver runtime tag");
    ctx.fb.ins("i64.const 8", "null tag");
    ctx.fb.ins("i64.eq", "is receiver null?");
    ctx.fb.ins("if (result i32)", "null -> box null, else read property");
    // null arm: php warns for `->` and stays silent for `?->`, then both answer null.
    if inst.op == Op::PropGet {
        let (name_ptr, name_len) = ctx.str_literal(prop_data)?;
        ctx.fb.ins(
            &format!("(call $__rt_warn_property_on_null (i32.const {name_ptr}) (i32.const {name_len}))"),
            "php names the property it could not read",
        );
    }
    ctx.fb.ins("i64.const 8", "mixed tag (null)");
    ctx.fb.ins("i64.const 0", "null lo");
    ctx.fb.ins("i64.const 0", "null hi");
    ctx.fb.ins("call $__rt_mixed_from_value", "box a fresh null mixed cell");
    ctx.fb.ins("else", "object -> read property into a mixed cell");
    // object arm: recover the object pointer and read the property.
    ctx.fb.ins(&format!("local.get {}", lo), "object payload low word");
    ctx.fb.ins("i32.wrap_i64", "low word -> object pointer i32");
    let obj = ctx.fresh_temp(ValType::I32);
    ctx.fb.ins(&format!("local.set {}", obj), "save object pointer");
    emit_prop_get_into_mixed(ctx, &obj, &ci, &class_name, prop_data, &property)?;
    ctx.fb.ins("end", "end nullsafe property get");
    // A NULLABLE read of a declared scalar answers `int|null`, which this target stores as a
    // tagged pair rather than as a cell — so the cell both arms produced is opened here and the
    // pair pushed in its place. The cell carries no reference the pair inherits, so it is freed.
    if let Some(result) = inst.result {
        if matches!(ctx.value_repr(result)?, WasmRepr::Tagged { .. }) {
            let cell = ctx.fresh_temp(ValType::I32);
            ctx.fb.ins(&format!("local.set {cell}"), "the boxed read");
            ctx.fb.ins(
                &format!("(call $__rt_mixed_unbox (local.get {cell}))"),
                "open it into (tag, lo, hi)",
            );
            ctx.fb.ins("drop", "hi is unused for a scalar|null");
            let lo = ctx.fresh_temp(ValType::I64);
            let tag = ctx.fresh_temp(ValType::I64);
            ctx.fb.ins(&format!("local.set {lo}"), "payload");
            ctx.fb.ins(&format!("local.set {tag}"), "tag");
            ctx.fb.ins(
                &format!("(call $__rt_decref_any (local.get {cell}))"),
                "the pair carries no reference",
            );
            ctx.fb.ins(&format!("local.get {lo}"), "tagged payload");
            ctx.fb.ins(
                &format!("(i32.wrap_i64 (local.get {tag}))"),
                "tagged tag (0 int, 8 null)",
            );
        }
    }
    store_result(ctx, inst)?;
    Ok(())
}
