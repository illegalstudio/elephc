//! Purpose:
//! Type and constant introspection: `gettype`, `get_resource_type` and `define`.
//!
//! Called from:
//! - `super::lower_direct_builtin` and `super::direct_builtin_shape_issue` dispatch to it.
//! - `crate::codegen_wasm::capability` reads its `*_shape_issue` predicates for the static audit.

use super::*;

/// Validates `define("NAME", $value)`.
///
/// The name must be a LITERAL, because the duplicate flag is a global named after it. The value
/// is unconstrained: the constant's VALUE is settled by the prescan's constant table, and what
/// runs here decides only whether this call is the first one, which is what PHP answers.
pub(super) fn define_shape_issue(module: &Module, function: &Function, call: &Instruction) -> Option<String> {
    if call.operands.len() != 2 {
        return Some(format!(
            "define takes a name and a value, got {} operands",
            call.operands.len()
        ));
    }
    if crate::codegen_wasm::capability::define_constant_name(module, function, call).is_none() {
        return Some("define needs a literal constant name".to_string());
    }
    (call.result_type != IrType::I64)
        .then(|| format!("define answers a bool, not {:?}", call.result_type))
}

/// Lowers `define("NAME", $value)` to its per-name flag.
///
/// php-src answers true the first time and false — after `Warning: Constant NAME already
/// defined` — for every later call. The flag is a module global rather than a compile-time
/// count, so a `define` inside a branch or a loop still answers correctly.
///
/// The VALUE operand is evaluated for its side effects and dropped: which value the constant
/// carries is settled by the prescan's constant table, exactly as every read of the constant
/// already resolves.
pub(super) fn lower_define(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let name = crate::codegen_wasm::capability::define_constant_name(ctx.module, ctx.function, inst)
        .ok_or_else(|| WasmError::Unsupported("define needs a literal name".to_string()))?
        .to_string();
    let flag = crate::codegen_wasm::symbols::define_flag_symbol(&name);
    let (name_ptr, name_len) = ctx.default_str_literal(&name)?;
    // Evaluate and discard the value: PHP runs the expression whether or not the name is taken.
    let value = operand(inst, 1)?;
    ctx.emit_load_value(value)?;
    let value_words = crate::codegen_wasm::values::WasmRepr::val_types(
        ctx.function
            .value(value)
            .map(|v| v.ir_type)
            .unwrap_or(IrType::I64),
    )
    .len();
    for _ in 0..value_words {
        ctx.fb.ins("drop", "define ignores the value here");
    }
    ctx.fb.ins(&format!("global.get ${flag}"), "already defined?");
    ctx.fb.ins(
        &format!(
            "(if (result i64) (then (call $__rt_warn_constant_already_defined (i32.const {name_ptr}) (i32.const {name_len})) (i64.const 0)) (else (global.set ${flag} (i32.const 1)) (i64.const 1)))"
        ),
        "first define answers true; a duplicate warns and answers false",
    );
    store_result(ctx, inst)
}

/// Validates `get_resource_type($handle)`.
///
/// A stream reaches this the same two ways the file builtins accept: a boxed cell carrying the
/// fd as a resource payload, or a predefined `STDOUT`/`STDERR`/`STDIN` that IS the fd. Matched
/// on the DECLARED type, since `codegen_repr()` folds every resource to `Int` and the whole
/// point here is to tell a stream from an ordinary integer.
pub(super) fn get_resource_type_shape_issue(function: &Function, call: &Instruction) -> Option<String> {
    let [operand] = call.operands.as_slice() else {
        return Some(format!(
            "get_resource_type takes one handle, got {}",
            call.operands.len()
        ));
    };
    let Some(value) = function.value(*operand) else {
        return Some("operand is missing from the value table".to_string());
    };
    let boxed = value.ir_type == IrType::Heap(IrHeapKind::Mixed);
    let predefined =
        value.ir_type == IrType::I64 && matches!(value.php_type, PhpType::Resource(_));
    if !boxed && !predefined {
        return Some(format!(
            "operand {:?}/{:?} is not a stream handle",
            value.ir_type, value.php_type
        ));
    }
    (call.result_type != IrType::Str)
        .then(|| format!("result {:?} must be a string", call.result_type))
}

/// Lowers `get_resource_type($handle)`.
///
/// php-src answers `stream` while a handle is open and `Unknown` once it has been closed —
/// measured on 8.5.6, and the native backend agrees. The closed predicate is the SIGN of the
/// box's payload word, which `fclose` stamps above; both names are ordinary string literals, so
/// the branch is two `select`s rather than a runtime helper.
pub(super) fn lower_get_resource_type(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let value = operand(inst, 0)?;
    let (stream_ptr, stream_len) = ctx.default_str_literal("stream")?;
    let (unknown_ptr, unknown_len) = ctx.default_str_literal("Unknown")?;
    let boxed = ctx
        .function
        .value(value)
        .is_some_and(|value| value.ir_type == IrType::Heap(IrHeapKind::Mixed));
    if !boxed {
        // `STDIN`/`STDOUT`/`STDERR` arrive as the fd itself; `fclose` of the constant sets its
        // bit in the std-closed mark (the host fd stays open), which this reads.
        let closed = ctx.fresh_temp(crate::codegen_wasm::wat::ValType::I32);
        ctx.emit_load_value(value)?;
        ctx.fb.ins("call $__rt_handle_closed", "has fclose run on the constant?");
        ctx.fb.ins(&format!("local.set {closed}"), "remember the verdict");
        ctx.fb.ins(&format!("i32.const {unknown_ptr}"), "closed name");
        ctx.fb.ins(&format!("i32.const {stream_ptr}"), "open name");
        ctx.fb.ins(&format!("local.get {closed}"), "");
        ctx.fb.ins("select", "the type name");
        ctx.fb.ins(&format!("i64.const {unknown_len}"), "closed length");
        ctx.fb.ins(&format!("i64.const {stream_len}"), "open length");
        ctx.fb.ins(&format!("local.get {closed}"), "");
        ctx.fb.ins("select", "its length");
        return store_result(ctx, inst);
    }
    let closed = ctx.fresh_temp(crate::codegen_wasm::wat::ValType::I32);
    ctx.emit_load_value(value)?;
    ctx.fb.ins("call $__rt_box_closed", "the box's stamp, or the stream's own record");
    ctx.fb.ins(&format!("local.set {closed}"), "remember the verdict");
    // `select` yields its FIRST operand when the condition is true, so the closed name leads.
    ctx.fb.ins(&format!("i32.const {unknown_ptr}"), "closed name");
    ctx.fb.ins(&format!("i32.const {stream_ptr}"), "open name");
    ctx.fb.ins(&format!("local.get {closed}"), "");
    ctx.fb.ins("select", "the type name");
    ctx.fb.ins(&format!("i64.const {unknown_len}"), "closed length");
    ctx.fb.ins(&format!("i64.const {stream_len}"), "open length");
    ctx.fb.ins(&format!("local.get {closed}"), "");
    ctx.fb.ins("select", "its length");
    store_result(ctx, inst)
}

/// Lowers `substr`, passing whether an explicit length was written.
///
/// The two- and three-argument forms differ in more than a default: without a length the slice
/// runs to the end, while a NEGATIVE length names an end offset from the right. A single flag
/// tells the helper which rule to apply rather than inventing a length that would have to encode
/// both.
/// Lowers `gettype($value)`.
///
/// A settled EIR type answers with a constant string: the type is already decided, and emitting a
/// dispatch would only be a slower way to reach the same answer. A boxed value dispatches on the
/// cell tag through a chain of comparisons, which is exact for every tag this target can build.
/// `resource (closed)` is the one php-src answer no tag carries, so a resource (tag 9, or a
/// settled resource type) also reads the stream's closed state.
pub(super) fn lower_gettype(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let value = operand(inst, 0)?;
    let declared = ctx.value_php_type(value)?.clone();
    // A nullable scalar arrives TAGGED: the tag alone separates the two settled answers.
    if ctx.function.value(value).map(|v| v.ir_type) == Some(IrType::TaggedScalar) {
        if let Some(member) = gettype_nullable_member(&declared) {
            let crate::codegen_wasm::values::WasmRepr::Tagged { tag, .. } = ctx.value_repr(value)?.clone() else {
                return Err(WasmError::Unsupported(
                    "gettype over a tagged scalar with a non-tagged representation".to_string(),
                ));
            };
            let (null_ptr, null_len) = ctx.default_str_literal("NULL")?;
            let (member_ptr, member_len) = ctx.default_str_literal(member)?;
            ctx.fb
                .ins(&format!("i32.const {member_ptr}"), "the non-null answer");
            ctx.fb.ins(&format!("i64.const {member_len}"), "its length");
            ctx.fb.ins(&format!("local.get {tag}"), "tagged scalar tag");
            ctx.fb.ins("i32.const 8", "tagged null tag");
            ctx.fb.ins("i32.eq", "tagged scalar is null?");
            ctx.fb.ins(
                &format!(
                    "(if (param i32 i64) (result i32 i64) (then (drop) (drop) (i32.const {null_ptr}) (i64.const {null_len})))"
                ),
                "PHP names the null arm NULL",
            );
            return store_result(ctx, inst);
        }
    }
    // A resource answers by the stream's own closed state, which only the runtime can read.
    if matches!(declared, PhpType::Resource(_)) {
        let (open_ptr, open_len) = ctx.default_str_literal("resource")?;
        let (closed_ptr, closed_len) = ctx.default_str_literal("resource (closed)")?;
        ctx.fb.ins(&format!("i32.const {open_ptr}"), "open handle: resource");
        ctx.fb.ins(&format!("i64.const {open_len}"), "its length");
        ctx.emit_load_value(value)?;
        ctx.fb.ins("call $__rt_handle_closed", "has the stream behind this handle been closed?");
        ctx.fb.ins(
            &format!(
                "(if (param i32 i64) (result i32 i64) (then (drop) (drop) (i32.const {closed_ptr}) (i64.const {closed_len})))"
            ),
            "php-src names a closed handle resource (closed)",
        );
        return store_result(ctx, inst);
    }
    if let Some(name) = gettype_declared_name(&declared) {
        let (ptr, len) = ctx.default_str_literal(name)?;
        ctx.fb
            .ins(&format!("i32.const {ptr}"), "gettype: the EIR type already decides this");
        ctx.fb.ins(&format!("i64.const {len}"), "its length");
        return store_result(ctx, inst);
    }
    // tag -> name, in the order the tags are numbered. Tag 7 (nested) and tag 10 (callable) are
    // internal shapes PHP has no separate name for: a nested container is an array and a callable
    // descriptor reaches PHP as an object.
    const TAGGED: &[(i64, &str)] = &[
        (0, "integer"),
        (1, "string"),
        (2, "double"),
        (3, "boolean"),
        (4, "array"),
        (5, "array"),
        (6, "object"),
        (7, "array"),
        (8, "NULL"),
        (10, "object"),
    ];
    let tag = ctx.fresh_temp(crate::codegen_wasm::wat::ValType::I64);
    ctx.emit_load_value(value)?;
    ctx.fb.ins("call $__rt_mixed_unbox", "unbox -> (tag, lo, hi)");
    ctx.fb.ins("drop", "discard hi");
    ctx.fb.ins("drop", "discard lo");
    ctx.fb.ins(&format!("local.set {tag}"), "the cell's runtime tag");
    let (fallback_ptr, fallback_len) = ctx.default_str_literal("object")?;
    ctx.fb
        .ins(&format!("i32.const {fallback_ptr}"), "default: object");
    ctx.fb.ins(&format!("i64.const {fallback_len}"), "its length");
    for (candidate, name) in TAGGED.iter().rev() {
        let (ptr, len) = ctx.default_str_literal(name)?;
        ctx.fb.ins(
            &format!(
                "(if (param i32 i64) (result i32 i64) (i64.eq (local.get {tag}) (i64.const {candidate})) (then (drop) (drop) (i32.const {ptr}) (i64.const {len})))"
            ),
            &format!("tag {candidate} is \"{name}\""),
        );
    }
    // A resource cell (tag 9) is "resource" while its stream is open and "resource (closed)"
    // once it is not; the box's stamp or the stream's own record decides.
    let (res_ptr, res_len) = ctx.default_str_literal("resource")?;
    let (closed_ptr, closed_len) = ctx.default_str_literal("resource (closed)")?;
    ctx.fb.ins(
        &format!(
            "(if (param i32 i64) (result i32 i64) (i64.eq (local.get {tag}) (i64.const 9)) (then (drop) (drop) (i32.const {res_ptr}) (i64.const {res_len})))"
        ),
        "tag 9 is a resource",
    );
    ctx.emit_load_value(value)?;
    ctx.fb.ins("call $__rt_box_closed", "is the stream behind the cell closed?");
    ctx.fb.ins(
        &format!(
            "(if (param i32 i64) (result i32 i64) (then (drop) (drop) (i32.const {closed_ptr}) (i64.const {closed_len})))"
        ),
        "php-src names a closed handle resource (closed)",
    );
    store_result(ctx, inst)
}

/// The exact string php-src's `gettype()` answers for a settled PHP type.
///
/// These are php-src's historical spellings, not the type names PHP 8 prints elsewhere: an int is
/// "integer", a float is "double", a bool is "boolean", and null is "NULL" in capitals. Measured
/// on php-src 8.5.6 rather than transcribed from the manual.
pub(super) fn gettype_name(php_type: &PhpType) -> Option<&'static str> {
    Some(match php_type {
        // A resource reprs as `Int`, and php-src answers "resource" or "resource (closed)"
        // depending on a liveness only the runtime reads — so it has no COMPILE-TIME name.
        PhpType::Resource(_) => return None,
        PhpType::Int => "integer",
        PhpType::Float => "double",
        PhpType::Bool | PhpType::False => "boolean",
        PhpType::Str => "string",
        PhpType::Array(_) | PhpType::AssocArray { .. } => "array",
        PhpType::Object(_) => "object",
        PhpType::Void | PhpType::Never => "NULL",
        _ => return None,
    })
}

/// `gettype_name` over the DECLARED type, so a resource is seen before `codegen_repr` flattens
/// it into `Int` and it silently becomes "integer".
pub(super) fn gettype_declared_name(php_type: &PhpType) -> Option<&'static str> {
    if matches!(php_type, PhpType::Resource(_)) {
        return None;
    }
    gettype_name(&php_type.codegen_repr())
}

/// The non-null member of an exact `T|null` union, when `T` has a settled `gettype()` name.
///
/// A nullable scalar is stored as a TAGGED scalar, so the runtime tag decides between the two
/// answers and both are settled: `int|null` is "integer" or "NULL", never anything else.
pub(super) fn gettype_nullable_member(php_type: &PhpType) -> Option<&'static str> {
    let PhpType::Union(members) = php_type else {
        return None;
    };
    if members.len() != 2
        || !members
            .iter()
            .any(|member| matches!(member, PhpType::Void | PhpType::Never))
    {
        return None;
    }
    members
        .iter()
        .find(|member| !matches!(member, PhpType::Void | PhpType::Never))
        .and_then(gettype_name)
}

/// Validates `gettype($value)`: one operand answering a string.
///
/// A settled type answers at compile time. A boxed one dispatches on the cell tag, which decides
/// every case this target can produce; a RESOURCE additionally consults the stream's closed
/// state to tell `"resource"` from `"resource (closed)"`.
pub(super) fn gettype_shape_issue(function: &Function, call: &Instruction) -> Option<String> {
    let [operand] = call.operands.as_slice() else {
        return Some(format!(
            "gettype takes exactly one value, got {}",
            call.operands.len()
        ));
    };
    let Some(value) = function.value(*operand) else {
        return Some("gettype operand is missing from the value table".to_string());
    };
    if call.result_type != IrType::Str || call.result_php_type.codegen_repr() != PhpType::Str {
        return Some(format!(
            "gettype result {:?}/{:?} is not the expected Str/Str",
            call.result_type,
            call.result_php_type.codegen_repr()
        ));
    }
    // The check reads the DECLARED type, not `codegen_repr()`: a resource reprs as `Int`, so
    // asking the repr would answer "integer" for a handle php-src calls "resource".
    if gettype_declared_name(&value.php_type).is_some() {
        return None;
    }
    // A resource is "resource" or "resource (closed)" by the stream's own closed state, which
    // the runtime reads from the raw handle payload.
    if matches!(value.php_type, PhpType::Resource(_)) {
        return None;
    }
    // A nullable scalar is a TAGGED scalar whose tag picks between two settled names.
    if value.ir_type == IrType::TaggedScalar
        && gettype_nullable_member(&value.php_type).is_some()
    {
        return None;
    }
    let source = value.php_type.codegen_repr();
    if source == PhpType::Mixed && value.ir_type == IrType::Heap(IrHeapKind::Mixed) {
        return None;
    }
    Some(format!(
        "gettype over {:?} has no settled php-src name",
        value.php_type
    ))
}
