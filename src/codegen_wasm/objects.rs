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
pub(super) fn emit_object_runtime(wm: &mut WatModule) {
    wm.add_raw_func(RT_DECREF_OBJECT);
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

/// One `Throwable` accessor that has no EIR body and is open-coded at the call site.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum ThrowableIntrinsic {
    /// `getMessage()` and `__toString()`: the `$message` slot.
    Message,
    /// `getCode()`: the `$code` slot.
    Code,
    /// `getPrevious()`: the `$previous` slot.
    Previous,
    /// `getFile()` and `getTraceAsString()`: the synthetic empty string.
    EmptyString,
    /// `getLine()`: the synthetic zero.
    ZeroInt,
    /// `getTrace()`: the synthetic empty array.
    EmptyTrace,
}

/// Maps a `Throwable` accessor's method key to what it produces.
///
/// The four synthetic results are elephc's established behavior rather than php-src's: the
/// compiler records no per-throw file, line or backtrace on either backend, so `getFile()` and
/// `getTraceAsString()` answer the empty string, `getLine()` zero, and `getTrace()` an empty
/// array — exactly what `lower_throwable_standard_method_loaded` answers natively. Keeping the
/// two backends identical here matters more than either one being closer to php-src alone,
/// because a program that behaves differently once compiled for WebAssembly is the failure this
/// target is trying to avoid.
fn throwable_intrinsic_for_key(method_key: &str) -> Option<ThrowableIntrinsic> {
    match method_key {
        "getmessage" | "__tostring" => Some(ThrowableIntrinsic::Message),
        "getcode" => Some(ThrowableIntrinsic::Code),
        "getprevious" => Some(ThrowableIntrinsic::Previous),
        "getfile" | "gettraceasstring" => Some(ThrowableIntrinsic::EmptyString),
        "getline" => Some(ThrowableIntrinsic::ZeroInt),
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
        ThrowableIntrinsic::EmptyString => return Ok((IrType::Str, PhpType::Str)),
        ThrowableIntrinsic::ZeroInt => return Ok((IrType::I64, PhpType::Int)),
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
        ThrowableIntrinsic::EmptyString
        | ThrowableIntrinsic::ZeroInt
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
        ThrowableIntrinsic::EmptyString => {
            // A zero-length string needs no data segment: only the length is ever read, and a
            // null pointer is never dereferenced for it.
            ctx.fb.ins("i32.const 0", "empty string pointer");
            ctx.fb.ins("i64.const 0", "empty string length");
        }
        ThrowableIntrinsic::ZeroInt => {
            ctx.fb.ins("i64.const 0", "synthetic line number");
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

/// Returns true when `class_name` implements `Throwable`, directly or through its parents.
///
/// The walk is depth-bounded rather than trusting the hierarchy to be acyclic: this runs inside
/// a backend audit, where a malformed module must produce a rejection, not a hang.
fn is_throwable_class(module: &Module, class_name: &str) -> bool {
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
                        gc_desc_tag(ty)
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
fn gc_desc_tag(ty: &PhpType) -> u8 {
    match ty {
        PhpType::Int => 0,
        PhpType::Str => 1,
        PhpType::Float => 2,
        PhpType::Bool => 3,
        PhpType::Array(_) => 4,
        PhpType::AssocArray { .. } => 5,
        PhpType::Object(_) => 6,
        PhpType::Mixed | PhpType::Union(_) | PhpType::Iterable | PhpType::TaggedScalar => 7,
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
const RT_DECREF_OBJECT: &str = r#"(func $__rt_decref_object (param $ptr i32)
  (local $rc i32) (local $n i32) (local $cid i32) (local $desc i32) (local $i i32) (local $tag i32) (local $slot i32) (local $tail_off i32) (local $meta i32)
  (if (i32.eqz (local.get $ptr)) (then (return)))                    ;; guard: null pointer
  (if (i32.lt_u (local.get $ptr) (i32.add (global.get $__heap_base) (i32.const 16)))
    (then (return)))                                                  ;; guard: below first payload (borrowed/literal)
  (if (i32.ge_u (local.get $ptr) (global.get $__heap_ptr))
    (then (return)))                                                  ;; guard: at/after bump cursor (not live)
  (local.set $rc (i32.load (i32.sub (local.get $ptr) (i32.const 12))))  ;; refcount = [ptr-12]
  (if (i32.eqz (local.get $rc)) (then (return)))                    ;; guard: refcount == 0 (re-entrancy)
  (local.set $rc (i32.add (local.get $rc) (i32.const -1)))          ;; rc = rc - 1
  (if (i32.ne (local.get $rc) (i32.const 0)) (then                  ;; keep path: rc > 0, store and return
    (i32.store (i32.sub (local.get $ptr) (i32.const 12)) (local.get $rc))  ;; store decremented refcount
    (return)))                                                       ;; keep live and return
  (i32.store (i32.sub (local.get $ptr) (i32.const 12)) (i32.const 0))  ;; mark refcount 0 (re-entrancy guard)
  (call $__rt_call_object_destructor (local.get $ptr))          ;; run __destruct (if any) before the property walk
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
        (if (i32.or (i32.eq (local.get $tag) (i32.const 1)) (i32.and (i32.ge_u (local.get $tag) (i32.const 4)) (i32.le_u (local.get $tag) (i32.const 7)))) (then  ;; tag in {1,4,5,6,7} -> release the slot
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
  (call $__rt_heap_free (local.get $ptr))                         ;; free the object storage (unsafe: refcount already 0)
  (return)                                                    ;; top-level return
)                                                              ;; close func
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
fn receiver_class_info(ctx: &FnCtx, object: ValueId) -> Result<(String, ClassInfo)> {
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
fn dynamic_property_hash_offset_for_class(
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

    let obj = emit_object_allocation(ctx, &class_name, &ci)?;

    // -- call __construct (if declared) with [$this, ...user_args]; defaults run first --
    match &ctor_sig {
        None => {
            // No constructor: operands MUST be empty (matches native's args-without-ctor reject).
            if !inst.operands.is_empty() {
                return Err(WasmError::Unsupported(format!(
                    "constructor arguments for class {} with no __construct on wasm32-wasi",
                    class_name
                )));
            }
        }
        Some(sig) => {
            if inst.operands.len() != sig.params.len() {
                return Err(WasmError::Unsupported(format!(
                    "constructor argument count mismatch for class {} on wasm32-wasi (got {}, expected {})",
                    class_name, inst.operands.len(), sig.params.len()
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
                emit_open_coded_throwable_constructor(ctx, &obj, &ci, &class_name, inst)?;
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
            if ctor_params.len() != inst.operands.len() {
                return Err(WasmError::Unsupported(format!(
                    "constructor body {}::__construct takes {} arguments, got {}",
                    impl_class,
                    ctor_params.len(),
                    inst.operands.len()
                )));
            }
            ctx.fb.ins(&format!("local.get {}", obj), "push $this (fresh object) as first ctor arg");
            let mut boxed_args: Vec<(String, IrType)> = Vec::new();
            for (&arg, (param_ir, param_php)) in inst.operands.iter().zip(&ctor_params) {
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

/// Allocates one instance of `class_name` and returns the local holding its pointer.
///
/// Covers everything a fresh instance needs before any constructor runs: the heap block, the
/// kind-4 header word, the compile-time class id, a zeroed slot per declared property, the
/// property defaults, and the dynamic-property hash tail. Shared by `ObjectNew` and by the
/// runtime-error raise sites, so an object built by a division-by-zero guard is byte-identical
/// to one built by `new` — including the gc_desc contract its release path depends on.
pub(super) fn emit_object_allocation(
    ctx: &mut FnCtx,
    class_name: &str,
    ci: &ClassInfo,
) -> Result<String> {
    let n = ci.properties.len();
    let has_dyn = has_dynamic_properties(ci, class_name);
    let dyn_off = if has_dyn { Some(8 + (n as i32) * 16) } else { None };
    let payload_size: i32 = 8 + (n as i32) * 16 + if has_dyn { 8 } else { 0 };
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

    Ok(obj)
}

/// PHP's Error class and message for each runtime failure reference PHP makes CATCHABLE.
///
/// Indexed by the `__rt_fail` diagnostic code the same failure carries when it reaches the top
/// of `main` uncaught, so one table pins both halves of the behaviour together: the class a
/// `catch` clause matches, and the fatal text printed when no clause matches. `runtime.rs`'s
/// `ERR_*` strings are the second half and must keep naming these same classes and messages.
pub(super) const CATCHABLE_RUNTIME_ERRORS: [(i32, &str, &str); 8] = [
    (1, "DivisionByZeroError", "Division by zero"),
    (2, "DivisionByZeroError", "Modulo by zero"),
    (3, "ArithmeticError", "Bit shift by negative number"),
    (
        4,
        "ArithmeticError",
        "Division of PHP_INT_MIN by -1 is not an integer",
    ),
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
    let obj = emit_object_allocation(ctx, class_name, &class_info)?;

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
    Ok(())
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

    let (class_name, ci) = receiver_class_info(ctx, object)?;
    // Undeclared property on an ADP / stdClass class -> read through the dyn-prop hash tail.
    if let Some(dyn_off) = dynamic_property_hash_offset_for_class(&ci, &class_name, &property) {
        return lower_dyn_prop_get(ctx, inst, object, dyn_off, prop_data);
    }
    let (_, offset, prop_type) = resolve_property_slot(&ci, &property)?;
    let prop_type = prop_type.codegen_repr();
    let obj_ref = object_ptr_ref(ctx, object)?;

    // `Op::PropGet` is always followed by `Op::Acquire`, which persists a string and increfs a
    // refcounted child — so retaining here too would leave one extra reference per read.
    emit_declared_property_load(
        ctx,
        &obj_ref,
        offset,
        &prop_type,
        &property,
        PropertyLoad::Borrowed,
    )?;

    store_result(ctx, inst)?;
    Ok(())
}

/// Who owns the value a property read leaves on the stack.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum PropertyLoad {
    /// The reader owns it: a string is persisted into a fresh heap copy and a refcounted child is
    /// increfed. For callers that produce a result nothing else will acquire.
    Owned,
    /// The reader only views it: no copy and no incref. For `Op::PropGet`, which the EIR ALWAYS
    /// follows with an `Op::Acquire` — verified across every use shape (store, echo, argument,
    /// concat, builtin argument, return, strict compare).
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

/// Lowers `Op::PropSet` of a declared property slot to a direct memory store.
///
/// `PropSet` is void with operands `[object, value]`. An undeclared property on an ADP /
/// stdClass receiver writes through the dynamic-property hash tail; a declared one resolves its
/// slot and hands the store to `emit_declared_property_store`, which owns the per-type
/// release/retain rules. Non-object receivers and undeclared properties on a class without the
/// dynamic tail return `Unsupported`.
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

    let (class_name, ci) = receiver_class_info(ctx, object)?;
    // Undeclared property on an ADP / stdClass class -> write through the dyn-prop hash tail.
    if let Some(dyn_off) = dynamic_property_hash_offset_for_class(&ci, &class_name, &property) {
        return lower_dyn_prop_set(ctx, inst, object, value, dyn_off, prop_data);
    }
    let (_, offset, prop_type) = resolve_property_slot(&ci, &property)?;
    let prop_type = prop_type.codegen_repr();
    let obj_ref = object_ptr_ref(ctx, object)?;

    emit_declared_property_store(ctx, &obj_ref, offset, &prop_type, value, &property)
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
    inst: &Instruction,
) -> Result<()> {
    if inst.operands.len() > THROWABLE_CONSTRUCTOR_PROPERTIES.len() {
        return Err(WasmError::Unsupported(format!(
            "{}::__construct with {} arguments on wasm32-wasi, expected at most {}",
            class_name,
            inst.operands.len(),
            THROWABLE_CONSTRUCTOR_PROPERTIES.len()
        )));
    }
    for (&argument, property) in inst
        .operands
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
        PhpType::Str => {
            ctx.fb.ins(&format!("local.get {}", obj_ref), "object base address");
            ctx.fb.ins(&format!("i32.load offset={}", offset), "load previous slot value ptr");
            ctx.fb.ins("call $__rt_decref_any", "release previous slot value (null-safe)");
            ctx.emit_load_value(value)?;
            ctx.fb.ins("call $__rt_str_persist", "persist string copy (ptr,len) -> (new_ptr,new_len)");
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
            ctx.emit_load_value(value)?;
            let ptr_tmp = ctx.fresh_temp(ValType::I32);
            ctx.fb.ins(&format!("local.set {}", ptr_tmp), "save container cell ptr");
            ctx.fb.ins(&format!("local.get {}", ptr_tmp), "container cell ptr");
            ctx.fb.ins("call $__rt_incref", "retain new container cell (refcount++)");
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
fn lower_dyn_prop_get(
    ctx: &mut FnCtx,
    inst: &Instruction,
    object: ValueId,
    dyn_off: usize,
    prop_data: DataId,
) -> Result<()> {
    let (kptr, klen) = ctx.str_literal(prop_data)?;
    let obj_ref = object_ptr_ref(ctx, object)?;
    // Load the dyn hash pointer at [obj + dyn_off] and look up the property name.
    ctx.fb.ins(&format!("local.get {}", obj_ref), "object base address");
    ctx.fb.ins(&format!("i32.load offset={}", dyn_off), "load dynamic-property hash pointer");
    let hash = ctx.fresh_temp(ValType::I32);
    ctx.fb.ins(&format!("local.set {}", hash), "save dyn-prop hash pointer");
    ctx.fb.ins(&format!("local.get {}", hash), "dyn-prop hash pointer");
    ctx.fb.ins(&format!("i32.const {}", kptr), "property name pointer (static data)");
    ctx.fb.ins("i64.extend_i32_u", "key_lo = zext(name ptr)");
    ctx.fb.ins(&format!("i64.const {}", klen), "key_hi = name length");
    ctx.fb.ins("call $__rt_hash_get", "look up the dynamic property -> (found, vlo, vhi, vtag)");
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
    ctx.fb.ins("i64.const 8", "mixed tag (null)");
    ctx.fb.ins("i64.const 0", "null lo");
    ctx.fb.ins("i64.const 0", "null hi");
    ctx.fb.ins("call $__rt_mixed_from_value", "box a fresh null mixed cell");
    ctx.fb.ins("end", "end hit/miss");
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
    prop_data: DataId,
) -> Result<()> {
    let (kptr, klen) = ctx.str_literal(prop_data)?;
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
    ctx.fb.ins(&format!("i32.const {}", kptr), "property name pointer (static data)");
    ctx.fb.ins("i64.extend_i32_u", "key_lo = zext(name ptr)");
    ctx.fb.ins(&format!("i64.const {}", klen), "key_hi = name length");
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
    // read the property and box into the Mixed result.
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
    // null arm: box a fresh null Mixed cell.
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
    store_result(ctx, inst)?;
    Ok(())
}
