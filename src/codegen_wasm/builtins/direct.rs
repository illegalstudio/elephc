//! Purpose:
//! Dispatch for builtins that lower inline: which runtime functions are direct, their admission audit, and the lowering switch.
//!
//! Called from:
//! - `crate::codegen_wasm::inst::lower_runtime_call` calls `is_direct_builtin` and `lower_direct_builtin`.
//! - `crate::codegen_wasm::capability::runtime_function_shape_issue` calls `direct_builtin_shape_issue`.
//!
//! Key details:
//! - The audit and the emitter read the same operand contract.

use super::*;

/// The storage one direct builtin accepts and produces.
///
/// Both the audit and the emitter derive from this single description, which is what keeps a
/// newly admitted builtin from being auditable but unlowerable, or the reverse.
pub(super) struct DirectSignature {
    /// EIR type every operand must carry.
    pub(super) operand_ir: IrType,
    /// PHP type every operand must carry, after `codegen_repr`.
    pub(super) operand_php: PhpType,
    /// EIR type the result must carry.
    pub(super) result_ir: IrType,
    /// PHP type the result must carry, after `codegen_repr`.
    pub(super) result_php: PhpType,
}

/// Returns the signature and WebAssembly instruction for a builtin lowered inline, or `None`
/// when the builtin needs a runtime helper.
///
/// `count` is absent from the instruction column because it is a memory load rather than an
/// arithmetic operation; it is handled separately by [`lower_count`].
pub(super) fn direct_builtin(target: RuntimeFnId, operand_php: &PhpType) -> Option<(DirectSignature, &'static str)> {
    let float = |instruction| {
        Some((
            DirectSignature {
                operand_ir: IrType::F64,
                operand_php: PhpType::Float,
                result_ir: IrType::F64,
                result_php: PhpType::Float,
            },
            instruction,
        ))
    };
    match target {
        // `abs` is the one entry whose storage depends on its argument: PHP keeps an integer
        // argument integral and a float one floating.
        RuntimeFnId::Abs => match operand_php {
            PhpType::Int => Some((
                DirectSignature {
                    operand_ir: IrType::I64,
                    operand_php: PhpType::Int,
                    result_ir: IrType::I64,
                    result_php: PhpType::Int,
                },
                // WebAssembly has no i64 absolute value; the branchless form is
                // `(x ^ (x >> 63)) - (x >> 63)`, emitted by `lower_int_abs`.
                "",
            )),
            PhpType::Float => float("f64.abs"),
            _ => None,
        },
        RuntimeFnId::Floor => float("f64.floor"),
        RuntimeFnId::Round => float("call $__rt_round"),
        RuntimeFnId::Ceil => float("f64.ceil"),
        RuntimeFnId::Sqrt => float("f64.sqrt"),
        // The prompt arrives as an ordinary (pointer, length) pair; the ANSWER is a boxed cell,
        // because php's `readline()` is `string|false` and EOF is the `false`. `__rt_readline`
        // explains why the prompt is accepted and not written.
        RuntimeFnId::Readline => Some((
            DirectSignature {
                operand_ir: IrType::Str,
                operand_php: PhpType::Str,
                result_ir: IrType::Heap(IrHeapKind::Mixed),
                // `string|false` reduces to `Mixed` storage, which is the cell the helper builds.
                result_php: PhpType::Mixed,
            },
            "call $__rt_readline",
        )),
        _ => None,
    }
}

/// Returns whether `target` is lowered inline by this module.
pub(in crate::codegen_wasm) fn is_direct_builtin(target: RuntimeFnId) -> bool {
    matches!(
        target,
        RuntimeFnId::Base64Decode
            | RuntimeFnId::ParseUrl
            | RuntimeFnId::Abs
            | RuntimeFnId::Gettype
            | RuntimeFnId::Floor
            | RuntimeFnId::Round
            | RuntimeFnId::Ceil
            | RuntimeFnId::Sqrt
            | RuntimeFnId::Readline
            | RuntimeFnId::Fopen
            | RuntimeFnId::Feof
            | RuntimeFnId::Ftell
            | RuntimeFnId::Rewind
            | RuntimeFnId::Fseek
            | RuntimeFnId::StreamGetContents
            | RuntimeFnId::StreamGetLine
            | RuntimeFnId::StreamGetMetaData
            | RuntimeFnId::StreamCopyToStream
            | RuntimeFnId::Getenv
            | RuntimeFnId::GetResourceType
            | RuntimeFnId::Define
            | RuntimeFnId::PrintR
            | RuntimeFnId::Fwrite
            | RuntimeFnId::Fread
            | RuntimeFnId::ArraySplice
            | RuntimeFnId::ArrayUnshift
            | RuntimeFnId::ArrayPad
            | RuntimeFnId::ArrayUnique
            | RuntimeFnId::ArrayColumn
            | RuntimeFnId::JsonEncode
            | RuntimeFnId::ObStart
            | RuntimeFnId::ObGetClean
            | RuntimeFnId::ObEndFlush
            | RuntimeFnId::ObEndClean
            | RuntimeFnId::ObGetStatus
            | RuntimeFnId::Long2ip
            | RuntimeFnId::Ip2long
            | RuntimeFnId::InetNtop
            | RuntimeFnId::InetPton
            | RuntimeFnId::VarDump
            | RuntimeFnId::Fgetc
            | RuntimeFnId::Readfile
            | RuntimeFnId::Flock
            | RuntimeFnId::Tmpfile
            | RuntimeFnId::Fclose
            | RuntimeFnId::FileExists
            | RuntimeFnId::Unlink
            | RuntimeFnId::FileGetContents
            | RuntimeFnId::FilePutContents
            | RuntimeFnId::Count
            | RuntimeFnId::ArrayIsList
            | RuntimeFnId::ArrayKeys
            | RuntimeFnId::ArrayValues
            | RuntimeFnId::ClassExists
            | RuntimeFnId::InterfaceExists
            | RuntimeFnId::TraitExists
            | RuntimeFnId::EnumExists
            | RuntimeFnId::FunctionExists
            | RuntimeFnId::ExtensionLoaded
            | RuntimeFnId::GetLoadedExtensions
            | RuntimeFnId::GetDeclaredClasses
            | RuntimeFnId::SplClasses
            | RuntimeFnId::Getprotobyname
            | RuntimeFnId::Getprotobynumber
            | RuntimeFnId::StreamSetChunkSize
            | RuntimeFnId::StreamSetReadBuffer
            | RuntimeFnId::StreamSetWriteBuffer
            | RuntimeFnId::SplObjectId
            | RuntimeFnId::IsA
            | RuntimeFnId::ClassImplements
            | RuntimeFnId::ClassParents
            | RuntimeFnId::ClassUses
            | RuntimeFnId::InArray
            | RuntimeFnId::ArrayReverse
            | RuntimeFnId::ArraySum
            | RuntimeFnId::ArrayProduct
            | RuntimeFnId::Max
            | RuntimeFnId::Min
            | RuntimeFnId::Intdiv
            | RuntimeFnId::ArrayFill
            | RuntimeFnId::StrContains
            | RuntimeFnId::StrStartsWith
            | RuntimeFnId::StrEndsWith
            | RuntimeFnId::Chr
            | RuntimeFnId::Ord
            | RuntimeFnId::Ucfirst
            | RuntimeFnId::Lcfirst
            | RuntimeFnId::Ucwords
            | RuntimeFnId::Strcmp
            | RuntimeFnId::Strcasecmp
            | RuntimeFnId::Trim
            | RuntimeFnId::Ltrim
            | RuntimeFnId::Rtrim
            | RuntimeFnId::Substr
            | RuntimeFnId::StrRepeat
            | RuntimeFnId::Strpos
            | RuntimeFnId::Strrpos
            | RuntimeFnId::Implode
            | RuntimeFnId::ArraySlice
            | RuntimeFnId::ArrayMerge
            | RuntimeFnId::Range
            | RuntimeFnId::ArrayKeyExists
            | RuntimeFnId::Sort
            | RuntimeFnId::Rsort
            | RuntimeFnId::ArrayKeyFirst
            | RuntimeFnId::ArrayKeyLast
            | RuntimeFnId::ArrayReplace
            | RuntimeFnId::ArrayDiffAssoc
            | RuntimeFnId::ArrayIntersectAssoc
            | RuntimeFnId::Ksort
            | RuntimeFnId::Krsort
            | RuntimeFnId::Asort
            | RuntimeFnId::Arsort
            | RuntimeFnId::ArraySearch
            | RuntimeFnId::Explode
            | RuntimeFnId::StrSplit
            | RuntimeFnId::Wordwrap
            | RuntimeFnId::Sprintf
            | RuntimeFnId::Fprintf
            | RuntimeFnId::Fscanf
            | RuntimeFnId::Time
            | RuntimeFnId::Microtime
            | RuntimeFnId::PregMatch
            | RuntimeFnId::Gzdeflate
            | RuntimeFnId::Gzinflate
            | RuntimeFnId::Gzcompress
            | RuntimeFnId::Gzuncompress
            | RuntimeFnId::BcAdd
            | RuntimeFnId::BcMul
            | RuntimeFnId::BcDivmod
            | RuntimeFnId::BcScale
            | RuntimeFnId::Printf
            | RuntimeFnId::Strstr
            | RuntimeFnId::StrPad
            | RuntimeFnId::StrReplace
            | RuntimeFnId::Crc32
            | RuntimeFnId::Sha1
            | RuntimeFnId::Md5
            | RuntimeFnId::Htmlspecialchars
    )
}

/// Validates one direct builtin's operand and result storage before planning.
pub(in crate::codegen_wasm) fn direct_builtin_shape_issue(
    module: &Module,
    function: &Function,
    call: &Instruction,
    target: RuntimeFnId,
) -> Option<String> {
    if let Some(file) = file_builtin_helper(target) {
        return file_builtin_shape_issue(module, function, call, &file);
    }
    if target == RuntimeFnId::Count {
        return count_shape_issue(module, function, call);
    }
    if target == RuntimeFnId::ArrayIsList {
        return array_is_list_shape_issue(function, call);
    }
    if matches!(
        target,
        RuntimeFnId::Long2ip
            | RuntimeFnId::Ip2long
            | RuntimeFnId::InetNtop
            | RuntimeFnId::InetPton
    ) {
        return inet_shape_issue(module, function, call, target);
    }
    if target == RuntimeFnId::VarDump {
        return var_dump_shape_issue(module, function, call);
    }
    if target == RuntimeFnId::JsonEncode {
        return json_encode_shape_issue(module, function, call);
    }
    if matches!(
        target,
        RuntimeFnId::ArrayPad | RuntimeFnId::ArrayUnique | RuntimeFnId::ArrayColumn
    ) {
        return array_rebuild_shape_issue(function, call, target);
    }
    if matches!(target, RuntimeFnId::ArraySplice | RuntimeFnId::ArrayUnshift) {
        return array_mutator_shape_issue(function, call, target);
    }
    if matches!(
        target,
        RuntimeFnId::ObStart
            | RuntimeFnId::ObGetClean
            | RuntimeFnId::ObEndFlush
            | RuntimeFnId::ObEndClean
            | RuntimeFnId::ObGetStatus
    ) {
        return output_buffer_shape_issue(module, function, call, target);
    }
    if matches!(
        target,
        RuntimeFnId::ClassExists
            | RuntimeFnId::InterfaceExists
            | RuntimeFnId::TraitExists
            | RuntimeFnId::EnumExists
            | RuntimeFnId::FunctionExists
            | RuntimeFnId::ExtensionLoaded
    ) {
        return class_exists_shape_issue(module, function, call, target);
    }
    if target == RuntimeFnId::GetLoadedExtensions {
        return get_loaded_extensions_shape_issue(function, call);
    }
    if matches!(
        target,
        RuntimeFnId::GetDeclaredClasses | RuntimeFnId::SplClasses
    ) {
        let name = if target == RuntimeFnId::SplClasses {
            "spl_classes"
        } else {
            "get_declared_classes"
        };
        return declared_names_shape_issue(call, name);
    }
    if target == RuntimeFnId::SplObjectId {
        return spl_object_id_shape_issue(function, call);
    }
    if matches!(
        target,
        RuntimeFnId::Getprotobyname | RuntimeFnId::Getprotobynumber
    ) {
        return protocol_lookup_shape_issue(function, call, target);
    }
    if matches!(
        target,
        RuntimeFnId::StreamSetChunkSize
            | RuntimeFnId::StreamSetReadBuffer
            | RuntimeFnId::StreamSetWriteBuffer
    ) {
        return stream_tuner_shape_issue(function, call, target);
    }
    if target == RuntimeFnId::IsA {
        return is_a_shape_issue(function, call);
    }
    if matches!(
        target,
        RuntimeFnId::ClassImplements | RuntimeFnId::ClassParents | RuntimeFnId::ClassUses
    ) {
        return class_relation_shape_issue(module, function, call, target);
    }
    if matches!(
        target,
        RuntimeFnId::ArrayKeys
            | RuntimeFnId::ArrayValues
            | RuntimeFnId::ArrayReverse
            | RuntimeFnId::ArraySum
            | RuntimeFnId::ArrayProduct
    ) && (boxed_array_operand(function, call)
        || (matches!(target, RuntimeFnId::ArraySum | RuntimeFnId::ArrayProduct)
            && mixed_list_operand(function, call)))
    {
        return boxed_array_builtin_shape_issue(module, call, target);
    }
    if matches!(
        target,
        RuntimeFnId::ArrayKeys | RuntimeFnId::ArrayValues | RuntimeFnId::ArrayReverse
    ) {
        return indexed_array_result_shape_issue(function, call, target);
    }
    if matches!(target, RuntimeFnId::ArraySum | RuntimeFnId::ArrayProduct) {
        return array_fold_shape_issue(function, call, target);
    }
    if matches!(target, RuntimeFnId::Max | RuntimeFnId::Min | RuntimeFnId::Intdiv) {
        return int_pair_shape_issue(module, function, call, target);
    }
    if target == RuntimeFnId::ArrayFill {
        return array_fill_shape_issue(function, call);
    }
    if matches!(
        target,
        RuntimeFnId::StrContains | RuntimeFnId::StrStartsWith | RuntimeFnId::StrEndsWith
    ) {
        return string_predicate_shape_issue(function, call, target);
    }
    if target == RuntimeFnId::InArray {
        return in_array_shape_issue(function, call);
    }
    // `array_search` shares that rule, and both of them reach a HASH haystack through the
    // generic walk instead — which needs no per-type pairing, so the only shape left to check
    // is the arity.

    if matches!(target, RuntimeFnId::Chr | RuntimeFnId::Ord) {
        return byte_conversion_shape_issue(function, call, target);
    }
    if matches!(target, RuntimeFnId::Strcmp | RuntimeFnId::Strcasecmp) {
        return string_compare_shape_issue(function, call, target);
    }
    if matches!(
        target,
        RuntimeFnId::Trim | RuntimeFnId::Ltrim | RuntimeFnId::Rtrim
    ) {
        return trim_shape_issue(function, call, target);
    }
    if target == RuntimeFnId::Substr {
        return substr_shape_issue(module, function, call);
    }
    if target == RuntimeFnId::Round && call.operands.len() == 2 {
        return round_places_shape_issue(function, call);
    }
    if target == RuntimeFnId::Gettype {
        return gettype_shape_issue(function, call);
    }
    if target == RuntimeFnId::StrRepeat {
        return str_repeat_shape_issue(function, call);
    }
    if matches!(target, RuntimeFnId::Strpos | RuntimeFnId::Strrpos) {
        return string_search_shape_issue(function, call, target);
    }
    if target == RuntimeFnId::Implode {
        return implode_shape_issue(function, call);
    }
    if target == RuntimeFnId::ArraySlice {
        return array_slice_shape_issue(function, call);
    }
    if target == RuntimeFnId::ArrayMerge {
        return array_merge_shape_issue(function, call);
    }
    if target == RuntimeFnId::Range {
        return range_shape_issue(function, call);
    }
    if matches!(target, RuntimeFnId::Sort | RuntimeFnId::Rsort) {
        return scalar_sort_shape_issue(module, function, call);
    }
    if hash_sort_mode(target).is_some() {
        return hash_sort_shape_issue(function, call, target);
    }
    if matches!(target, RuntimeFnId::ArrayKeyFirst | RuntimeFnId::ArrayKeyLast) {
        return hash_edge_key_shape_issue(function, call, target);
    }
    if matches!(
        target,
        RuntimeFnId::ArrayReplace | RuntimeFnId::ArrayDiffAssoc | RuntimeFnId::ArrayIntersectAssoc
    ) {
        return hash_pair_shape_issue(function, call, target);
    }
    if target == RuntimeFnId::ArrayKeyExists {
        return array_key_exists_shape_issue(function, call);
    }
    if target == RuntimeFnId::ArraySearch {
        return in_array_shape_issue(function, call);
    }
    if target == RuntimeFnId::Explode {
        return explode_shape_issue(function, call);
    }
    if target == RuntimeFnId::StrSplit {
        return str_split_shape_issue(function, call);
    }
    if target == RuntimeFnId::Wordwrap {
        return wordwrap_shape_issue(function, call);
    }
    if matches!(
        target,
        RuntimeFnId::Sprintf | RuntimeFnId::Printf | RuntimeFnId::Fprintf
    ) {
        return sprintf_shape_issue(function, module, call, target);
    }
    if target == RuntimeFnId::Fscanf {
        return fscanf_shape_issue(function, module, call);
    }
    if matches!(
        target,
        RuntimeFnId::BcAdd | RuntimeFnId::BcMul | RuntimeFnId::BcDivmod | RuntimeFnId::BcScale
    ) {
        return bcmath_shape_issue(function, call, target);
    }
    if matches!(
        target,
        RuntimeFnId::Gzdeflate
            | RuntimeFnId::Gzinflate
            | RuntimeFnId::Gzcompress
            | RuntimeFnId::Gzuncompress
    ) {
        return zlib_shape_issue(function, call, target);
    }
    if target == RuntimeFnId::PregMatch {
        return preg_match_shape_issue(function, module, call);
    }
    if matches!(target, RuntimeFnId::Time | RuntimeFnId::Microtime) {
        return clock_shape_issue(function, call, target);
    }
    if target == RuntimeFnId::Strstr {
        return strstr_shape_issue(function, call);
    }
    if target == RuntimeFnId::Base64Decode {
        return base64_decode_shape_issue(function, call);
    }
    if target == RuntimeFnId::ParseUrl {
        return parse_url_shape_issue(function, call);
    }
    if target == RuntimeFnId::StrPad {
        return str_pad_shape_issue(function, call);
    }
    if target == RuntimeFnId::StrReplace {
        return str_replace_shape_issue(function, call);
    }
    if target == RuntimeFnId::Crc32 {
        return crc32_shape_issue(function, call);
    }
    if matches!(
        target,
        RuntimeFnId::Sha1 | RuntimeFnId::Md5 | RuntimeFnId::Htmlspecialchars
    ) {
        return trim_shape_issue(function, call, target)
            .or_else(|| (call.operands.len() != 1).then(|| {
                format!("{target:?} takes exactly one string, got {}", call.operands.len())
            }));
    }
    if matches!(
        target,
        RuntimeFnId::Ucfirst | RuntimeFnId::Lcfirst | RuntimeFnId::Ucwords
    ) {
        return trim_shape_issue(function, call, target)
            .or_else(|| (call.operands.len() != 1).then(|| {
                format!("{target:?} takes exactly one string, got {}", call.operands.len())
            }));
    }
    if target == RuntimeFnId::GetResourceType {
        return get_resource_type_shape_issue(function, call);
    }
    if target == RuntimeFnId::Define {
        return define_shape_issue(module, function, call);
    }
    if target == RuntimeFnId::PrintR {
        return print_r_shape_issue(function, call);
    }
    let [operand] = call.operands.as_slice() else {
        return Some(format!(
            "expected one operand, got {}",
            call.operands.len()
        ));
    };
    let Some(value) = function.value(*operand) else {
        return Some("operand is missing from the value table".to_string());
    };
    let operand_php = value.php_type.codegen_repr();
    let Some((signature, _)) = direct_builtin(target, &operand_php) else {
        return Some(format!(
            "no inline lowering for a {operand_php:?} argument"
        ));
    };
    if value.ir_type != signature.operand_ir || operand_php != signature.operand_php {
        return Some(format!(
            "operand {:?}/{operand_php:?} is not the expected {:?}/{:?}",
            value.ir_type, signature.operand_ir, signature.operand_php
        ));
    }
    // `abs(int)` is honestly `int|float` — PHP_INT_MIN has no positive counterpart, so php
    // answers a float for it — and the EIR says so with a boxed result. The lowering carries
    // both arms, so the boxed spelling is admitted alongside the raw one the EIR uses when it
    // proved the argument cannot be PHP_INT_MIN.
    let int_abs_may_overflow = target == RuntimeFnId::Abs
        && operand_php == PhpType::Int
        && call.result_type == IrType::Heap(IrHeapKind::Mixed)
        && call.result_php_type.codegen_repr() == PhpType::Mixed;
    if call.result.is_none()
        || (!int_abs_may_overflow
            && (call.result_type != signature.result_ir
                || call.result_php_type.codegen_repr() != signature.result_php))
    {
        return Some(format!(
            "result {:?}/{:?} is not the expected {:?}/{:?}",
            call.result_type,
            call.result_php_type.codegen_repr(),
            signature.result_ir,
            signature.result_php
        ));
    }
    None
}

/// Lowers one direct builtin.
pub(in crate::codegen_wasm) fn lower_direct_builtin(
    ctx: &mut FnCtx,
    inst: &Instruction,
    target: RuntimeFnId,
) -> Result<()> {
    if target == RuntimeFnId::GetResourceType {
        return lower_get_resource_type(ctx, inst);
    }
    if target == RuntimeFnId::Define {
        return lower_define(ctx, inst);
    }
    if target == RuntimeFnId::PrintR {
        return lower_print_r(ctx, inst);
    }
    if let Some(helper) = file_builtin_helper(target) {
        return lower_file_builtin(ctx, inst, helper, target);
    }
    if target == RuntimeFnId::Count {
        return lower_count(ctx, inst);
    }
    if matches!(
        target,
        RuntimeFnId::ClassImplements | RuntimeFnId::ClassParents | RuntimeFnId::ClassUses
    ) {
        return lower_class_relation(ctx, inst, target);
    }
    if target == RuntimeFnId::ArrayIsList {
        return lower_array_is_list(ctx, inst);
    }
    if matches!(
        target,
        RuntimeFnId::Long2ip
            | RuntimeFnId::Ip2long
            | RuntimeFnId::InetNtop
            | RuntimeFnId::InetPton
    ) {
        let helper = match target {
            RuntimeFnId::Long2ip => "call $__rt_long2ip",
            RuntimeFnId::Ip2long => "call $__rt_ip2long",
            RuntimeFnId::InetNtop => "call $__rt_inet_ntop",
            _ => "call $__rt_inet_pton",
        };
        let source = operand(inst, 0)?;
        ctx.emit_load_value(source)?;
        ctx.fb.ins(helper, "IPv4 conversion");
        // `inet_ntop($mixed)` casts the cell to a string first, and that cast hands over a private
        // heap copy which this call alone reads. Nothing else owns it, so it is released here;
        // `heap_free_safe` ignores a literal or a buffer view that was never allocated.
        if target != RuntimeFnId::Long2ip
            && matches!(
                ctx.value_repr(source)?.clone(),
                crate::codegen_wasm::values::WasmRepr::Str { .. }
            )
            && crate::codegen_wasm::inst::value_is_an_unshared_temporary(ctx, source)
        {
            if let crate::codegen_wasm::values::WasmRepr::Str { ptr, .. } = ctx.value_repr(source)?.clone() {
                ctx.fb.ins(
                    &format!("(call $__rt_heap_free_safe (local.get {ptr}))"),
                    "the cast's private copy of the argument",
                );
            }
        }
        return store_result(ctx, inst);
    }
    if matches!(
        target,
        RuntimeFnId::ObStart
            | RuntimeFnId::ObGetClean
            | RuntimeFnId::ObEndFlush
            | RuntimeFnId::ObEndClean
            | RuntimeFnId::ObGetStatus
    ) {
        return lower_output_buffer(ctx, inst, target);
    }
    if matches!(
        target,
        RuntimeFnId::ArrayPad | RuntimeFnId::ArrayUnique | RuntimeFnId::ArrayColumn
    ) {
        return lower_array_rebuild(ctx, inst, target);
    }
    if matches!(target, RuntimeFnId::ArraySplice | RuntimeFnId::ArrayUnshift) {
        return lower_array_mutator(ctx, inst, target);
    }
    if target == RuntimeFnId::JsonEncode {
        // Boxed like `var_dump`'s operand, and for the same reason: the encoder dispatches on a
        // cell TAG, which a raw container pointer does not carry.
        let allocated = crate::codegen_wasm::transfer::emit_push_builtin_argument(
            ctx,
            operand(inst, 0)?,
            IrType::Heap(IrHeapKind::Mixed),
            PhpType::Mixed,
        )?;
        ctx.fb
            .ins("call $__rt_json_encode", "php's json_encode, boxed as string|false");
        if let Some((cell, _kind)) = allocated {
            ctx.fb.ins(
                &format!("local.get {cell}"),
                "the cell synthesized for the argument",
            );
            ctx.fb
                .ins("call $__rt_decref_any", "free it now the encode is done");
        }
        return store_result(ctx, inst);
    }
    if target == RuntimeFnId::VarDump {
        // Boxed first, because the renderer dispatches on a cell TAG: a raw container pointer
        // carries no tag of its own, and the same call site can hand over either.
        let allocated = crate::codegen_wasm::transfer::emit_push_builtin_argument(
            ctx,
            operand(inst, 0)?,
            IrType::Heap(IrHeapKind::Mixed),
            PhpType::Mixed,
        )?;
        ctx.fb
            .ins("call $__rt_var_dump", "php's typed dump");
        if let Some((cell, _kind)) = allocated {
            ctx.fb.ins(
                &format!("local.get {cell}"),
                "the cell synthesized for the argument",
            );
            ctx.fb
                .ins("call $__rt_decref_any", "free it now the dump is written");
        }
        return store_result(ctx, inst);
    }
    if matches!(
        target,
        RuntimeFnId::ClassExists
            | RuntimeFnId::InterfaceExists
            | RuntimeFnId::TraitExists
            | RuntimeFnId::EnumExists
            | RuntimeFnId::FunctionExists
            | RuntimeFnId::ExtensionLoaded
    ) {
        return lower_class_exists(ctx, inst, target);
    }
    if target == RuntimeFnId::GetLoadedExtensions {
        return lower_get_loaded_extensions(ctx, inst);
    }
    if matches!(
        target,
        RuntimeFnId::GetDeclaredClasses | RuntimeFnId::SplClasses
    ) {
        return lower_declared_names(ctx, inst, target);
    }
    if target == RuntimeFnId::SplObjectId {
        return lower_spl_object_id(ctx, inst);
    }
    if matches!(
        target,
        RuntimeFnId::Getprotobyname | RuntimeFnId::Getprotobynumber
    ) {
        return lower_protocol_lookup(ctx, inst, target);
    }
    if matches!(
        target,
        RuntimeFnId::StreamSetChunkSize
            | RuntimeFnId::StreamSetReadBuffer
            | RuntimeFnId::StreamSetWriteBuffer
    ) {
        return lower_stream_tuner(ctx, inst, target);
    }
    if target == RuntimeFnId::IsA {
        return lower_is_a(ctx, inst);
    }
    if matches!(
        target,
        RuntimeFnId::ArrayKeys
            | RuntimeFnId::ArrayValues
            | RuntimeFnId::ArrayReverse
            | RuntimeFnId::ArraySum
            | RuntimeFnId::ArrayProduct
    ) && (boxed_array_operand(ctx.function, inst)
        || (matches!(target, RuntimeFnId::ArraySum | RuntimeFnId::ArrayProduct)
            && mixed_list_operand(ctx.function, inst)))
    {
        return lower_boxed_array_builtin(ctx, inst, target);
    }
    if target == RuntimeFnId::ArrayKeys {
        return lower_array_keys(ctx, inst);
    }
    if target == RuntimeFnId::ArrayValues {
        return lower_array_values(ctx, inst);
    }
    if target == RuntimeFnId::ArrayReverse {
        return lower_array_reverse(ctx, inst);
    }
    if matches!(target, RuntimeFnId::ArraySum | RuntimeFnId::ArrayProduct) {
        return lower_array_fold(ctx, inst, target);
    }
    if matches!(target, RuntimeFnId::Max | RuntimeFnId::Min) {
        return lower_int_extremum(ctx, inst, target);
    }
    if target == RuntimeFnId::Intdiv {
        return crate::codegen_wasm::inst::lower_signed_int_div(ctx, inst);
    }
    if target == RuntimeFnId::ArrayFill {
        return lower_array_fill(ctx, inst);
    }
    if matches!(
        target,
        RuntimeFnId::StrContains | RuntimeFnId::StrStartsWith | RuntimeFnId::StrEndsWith
    ) {
        return lower_string_predicate(ctx, inst, target);
    }
    if target == RuntimeFnId::InArray {
        if in_array_haystack_is_hash(ctx.function, inst) {
            return lower_hash_find(ctx, inst, false);
        }
        return lower_in_array(ctx, inst);
    }
    if target == RuntimeFnId::Chr {
        ctx.emit_load_value(operand(inst, 0)?)?;
        ctx.fb
            .ins("call $__rt_str_chr", "the byte PHP's chr returns for this integer");
        return store_result(ctx, inst);
    }
    if target == RuntimeFnId::Ord {
        ctx.emit_load_value(operand(inst, 0)?)?;
        ctx.fb
            .ins("call $__rt_str_ord", "the first byte, or zero for the empty string");
        return store_result(ctx, inst);
    }
    if matches!(target, RuntimeFnId::Ucfirst | RuntimeFnId::Lcfirst) {
        ctx.emit_load_value(operand(inst, 0)?)?;
        ctx.fb.ins(
            if target == RuntimeFnId::Ucfirst {
                "i32.const 1"
            } else {
                "i32.const 0"
            },
            "map the first byte towards upper or lower case",
        );
        ctx.fb
            .ins("call $__rt_str_case_edge", "case-map the first byte only");
        return store_result(ctx, inst);
    }
    if target == RuntimeFnId::Ucwords {
        ctx.emit_load_value(operand(inst, 0)?)?;
        ctx.fb
            .ins("call $__rt_str_ucwords", "upper-case each word's first letter");
        return store_result(ctx, inst);
    }
    if matches!(target, RuntimeFnId::Strcmp | RuntimeFnId::Strcasecmp) {
        ctx.emit_load_value(operand(inst, 0)?)?;
        ctx.emit_load_value(operand(inst, 1)?)?;
        ctx.fb.ins(
            if target == RuntimeFnId::Strcasecmp {
                "i32.const 1"
            } else {
                "i32.const 0"
            },
            "fold ASCII case before comparing",
        );
        ctx.fb
            .ins("call $__rt_str_cmp", "byte distance, or +/-1 on length alone");
        return store_result(ctx, inst);
    }
    if matches!(
        target,
        RuntimeFnId::Trim | RuntimeFnId::Ltrim | RuntimeFnId::Rtrim
    ) {
        return lower_trim(ctx, inst, target);
    }
    if target == RuntimeFnId::Substr {
        return lower_substr(ctx, inst);
    }
    if target == RuntimeFnId::Gettype {
        return lower_gettype(ctx, inst);
    }
    // `round($v, $p)` is a different function from `round($v)`, not the same one with a default:
    // php-src extracts the integral part and then REPAIRS the extraction, which is what makes
    // `round(1.005, 2)` answer 1.01.
    if target == RuntimeFnId::Round && inst.operands.len() == 2 {
        ctx.emit_load_value(operand(inst, 0)?)?;
        ctx.emit_load_value(operand(inst, 1)?)?;
        ctx.fb.ins(
            "call $__rt_round_places",
            "PHP's round() at a requested precision",
        );
        return store_result(ctx, inst);
    }
    if target == RuntimeFnId::StrRepeat {
        return lower_str_repeat(ctx, inst);
    }
    if target == RuntimeFnId::Strpos {
        return lower_string_search(ctx, inst);
    }
    if target == RuntimeFnId::Strrpos {
        return lower_string_rsearch(ctx, inst);
    }
    if target == RuntimeFnId::Implode {
        return lower_implode(ctx, inst);
    }
    if target == RuntimeFnId::ArraySlice {
        return lower_array_slice(ctx, inst);
    }
    if matches!(target, RuntimeFnId::Sort | RuntimeFnId::Rsort) {
        return lower_scalar_sort(ctx, inst, target == RuntimeFnId::Rsort);
    }
    if let Some(mode) = hash_sort_mode(target) {
        return lower_hash_sort(ctx, inst, mode);
    }
    if matches!(target, RuntimeFnId::ArrayKeyFirst | RuntimeFnId::ArrayKeyLast) {
        let source = operand(inst, 0)?;
        ctx.emit_load_value(source)?;
        // `array_key_first([...])` over a literal: the array is a temporary nothing else names, and
        // the key cell keeps its own copy of the bytes, so the array is ours to give back. The EIR
        // emits no release for it.
        let temporary = is_unconsumed_literal_array(ctx, source)
            .then(|| ctx.fresh_temp(crate::codegen_wasm::wat::ValType::I32));
        if let Some(temporary) = &temporary {
            ctx.fb.ins(&format!("local.tee {temporary}"), "keep the temporary array to release it");
        }
        ctx.fb.ins(
            &format!("i32.const {}", i32::from(target == RuntimeFnId::ArrayKeyLast)),
            "which end of the insertion order",
        );
        ctx.fb.ins(
            "call $__rt_hash_edge_key",
            "the key at that end, boxed, or php's null for an empty array",
        );
        if let Some(temporary) = &temporary {
            ctx.fb.ins(
                &format!("(call $__rt_decref_any (local.get {temporary}))"),
                "release the literal array the key was read from",
            );
        }
        return store_result(ctx, inst);
    }
    if target == RuntimeFnId::ArrayReplace {
        emit_null_array_argument_guard(
            ctx,
            inst,
            crate::codegen_wasm::function::REPLACE_NULL_FIRST_FAILURE_CODE,
            crate::codegen_wasm::function::REPLACE_NULL_SECOND_FAILURE_CODE,
            "array_replace() null argument",
        )?;
        ctx.emit_load_value(operand(inst, 0)?)?;
        ctx.emit_load_value(operand(inst, 1)?)?;
        ctx.fb.ins(
            "call $__rt_hash_replace",
            "the left's key order, with every right entry overwriting or appended",
        );
        return store_result(ctx, inst);
    }
    if matches!(target, RuntimeFnId::ArrayDiffAssoc | RuntimeFnId::ArrayIntersectAssoc) {
        ctx.emit_load_value(operand(inst, 0)?)?;
        ctx.emit_load_value(operand(inst, 1)?)?;
        ctx.fb.ins(
            &format!("i32.const {}", i32::from(target == RuntimeFnId::ArrayDiffAssoc)),
            "diff keeps what is missing; intersect keeps what matches",
        );
        ctx.fb.ins(
            "call $__rt_hash_assoc_set_op",
            "compare the key AND the value, the value rendered as php renders it",
        );
        return store_result(ctx, inst);
    }
    if target == RuntimeFnId::ArrayKeyExists {
        // An indexed haystack is a range check and an associative one is a probe; the audit
        // has already settled which this is.
        let key = operand(inst, 0)?;
        let array = operand(inst, 1)?;
        if ctx.function.value(array).map(|value| value.ir_type)
            == Some(IrType::Heap(IrHeapKind::Array))
        {
            ctx.emit_load_value(array)?;
            ctx.emit_load_value(key)?;
            ctx.fb.ins(
                "call $__rt_array_key_exists",
                "an indexed array's keys are 0..len-1, so the key exists iff it is in range",
            );
            return store_result(ctx, inst);
        }
        return crate::codegen_wasm::inst_hash::lower_array_key_exists(ctx, inst);
    }
    if target == RuntimeFnId::ArraySearch {
        if in_array_haystack_is_hash(ctx.function, inst) {
            return lower_hash_find(ctx, inst, true);
        }
        return lower_array_search(ctx, inst);
    }
    if target == RuntimeFnId::Range {
        ctx.emit_load_value(operand(inst, 0)?)?;
        ctx.emit_load_value(operand(inst, 1)?)?;
        ctx.fb.ins(
            "call $__rt_range_int",
            "count from start to end inclusive, in whichever direction",
        );
        return store_result(ctx, inst);
    }
    if target == RuntimeFnId::ArrayMerge {
        emit_null_array_argument_guard(
            ctx,
            inst,
            crate::codegen_wasm::function::MERGE_NULL_FIRST_FAILURE_CODE,
            crate::codegen_wasm::function::MERGE_NULL_SECOND_FAILURE_CODE,
            "array_merge() null argument",
        )?;
        ctx.emit_load_value(operand(inst, 0)?)?;
        ctx.emit_load_value(operand(inst, 1)?)?;
        ctx.fb.ins(
            "call $__rt_array_merge",
            "clone the left, then append every element of the right",
        );
        return store_result(ctx, inst);
    }
    if target == RuntimeFnId::Explode {
        return lower_explode(ctx, inst);
    }
    if target == RuntimeFnId::StrSplit {
        return lower_str_split(ctx, inst);
    }
    if target == RuntimeFnId::Wordwrap {
        return lower_wordwrap(ctx, inst);
    }
    if target == RuntimeFnId::Fprintf {
        return lower_fprintf(ctx, inst);
    }
    if target == RuntimeFnId::Fscanf {
        return lower_fscanf(ctx, inst);
    }
    if matches!(
        target,
        RuntimeFnId::BcAdd | RuntimeFnId::BcMul | RuntimeFnId::BcDivmod | RuntimeFnId::BcScale
    ) {
        return lower_bcmath(ctx, inst, target);
    }
    if matches!(
        target,
        RuntimeFnId::Gzdeflate
            | RuntimeFnId::Gzinflate
            | RuntimeFnId::Gzcompress
            | RuntimeFnId::Gzuncompress
    ) {
        return lower_zlib(ctx, inst, target);
    }
    if target == RuntimeFnId::PregMatch {
        return lower_preg_match(ctx, inst);
    }
    if matches!(target, RuntimeFnId::Time | RuntimeFnId::Microtime) {
        return lower_clock(ctx, inst, target);
    }
    if target == RuntimeFnId::Sprintf {
        return lower_sprintf(ctx, inst);
    }
    if target == RuntimeFnId::Printf {
        return lower_printf(ctx, inst);
    }
    if target == RuntimeFnId::Strstr {
        return lower_strstr(ctx, inst);
    }
    if target == RuntimeFnId::Base64Decode {
        ctx.emit_load_value(operand(inst, 0)?)?;
        ctx.fb
            .ins("call $__rt_base64_decode", "tolerant decode, boxed as string|false");
        return store_result(ctx, inst);
    }
    if target == RuntimeFnId::ParseUrl {
        ctx.emit_load_value(operand(inst, 0)?)?;
        if inst.operands.len() == 2 {
            ctx.emit_load_value(operand(inst, 1)?)?;
        } else {
            ctx.fb
                .ins("i64.const -1", "no selector: php's default is the whole array");
        }
        ctx.fb
            .ins("call $__rt_parse_url", "php_url_parse_ex2, boxed as array|string|int|false");
        return store_result(ctx, inst);
    }
    if target == RuntimeFnId::StrPad {
        return lower_str_pad(ctx, inst);
    }
    if target == RuntimeFnId::StrReplace {
        return lower_str_replace(ctx, inst);
    }
    if target == RuntimeFnId::Crc32 {
        ctx.emit_load_value(operand(inst, 0)?)?;
        ctx.fb.ins("call $__rt_crc32", "reflected IEEE 802.3 remainder");
        return store_result(ctx, inst);
    }
    if target == RuntimeFnId::Sha1 {
        ctx.emit_load_value(operand(inst, 0)?)?;
        ctx.fb.ins("call $__rt_sha1_hex", "40-character lowercase digest");
        return store_result(ctx, inst);
    }
    if target == RuntimeFnId::Md5 {
        ctx.emit_load_value(operand(inst, 0)?)?;
        ctx.fb.ins("call $__rt_md5_hex", "32-character lowercase digest");
        return store_result(ctx, inst);
    }
    if target == RuntimeFnId::Htmlspecialchars {
        ctx.emit_load_value(operand(inst, 0)?)?;
        ctx.fb.ins("call $__rt_htmlspecialchars", "escape under PHP 8.1+ default flags");
        return store_result(ctx, inst);
    }
    let argument = operand(inst, 0)?;
    let operand_php = ctx.value_php_type(argument)?.codegen_repr();
    let Some((_, instruction)) = direct_builtin(target, &operand_php) else {
        return Err(WasmError::Unsupported(format!(
            "builtin {:?} over a {operand_php:?} argument",
            target
        )));
    };
    if target == RuntimeFnId::Abs && operand_php == PhpType::Int {
        return lower_int_abs(ctx, inst, argument);
    }
    ctx.emit_load_value(argument)?;
    ctx.fb.ins(instruction, "PHP builtin lowered inline");
    store_result(ctx, inst)
}
