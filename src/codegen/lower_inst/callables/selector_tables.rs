//! Purpose:
//! Resolves boxed-callable candidates through data tables scanned by one emitted loop.
//!
//! Called from:
//! - The descriptor-value selector in the parent callable lowerer (string names, callable
//!   arrays and invokable objects) and the scope-only prefix in `shared_selector`.
//!
//! Key details:
//! - A selector used to emit a compare-and-bind sequence per candidate, so its code grew with
//!   every function, builtin and public method in the program. The candidates now live in
//!   `.data` rows and one loop per shape walks them in the original first-match order.
//! - String rows are `(name, length, descriptor)`; the length is compared before the
//!   case-insensitive `__rt_strcasecmp`, as the per-case compares did implicitly.
//! - Instance rows are `(class id, method name, length, template)`; the receiver must be a
//!   non-null object and the method a string before the scan, exactly as each case checked.
//! - A matched receiver row binds through one shared sequence: a fresh runtime descriptor
//!   copied from the row's template, owning one retained receiver capture.
//! - The scan keeps its cursor and remaining-row count in a 16-byte temporary slot, because
//!   `__rt_strcasecmp` clobbers every caller-saved register. Both exits leave the caller's
//!   temporary stack exactly as they found it.

use crate::codegen::data_section::DataWord;

use super::*;

/// Bytes in one string-name row: name pointer, name length, descriptor pointer.
const STRING_ROW_BYTES: usize = 24;
/// Bytes in one receiver row: class id, method name pointer, name length, template pointer.
const INSTANCE_ROW_BYTES: usize = 32;
/// Temporary bytes holding the scan cursor (offset 0) and the remaining-row count (offset 8).
const SCAN_STATE_BYTES: usize = 16;

/// Looks the stripped name at temporary offsets 0/8 up among `cases`.
///
/// On a match the integer result register holds the case descriptor and control reaches
/// `matched_label`; otherwise `miss_label`. The name stays pushed on both exits.
///
/// With an `origin` (`CALLABLE_DESC_ORIGIN_*`), a row holds the static descriptor variant that
/// records the PHP string the callable stands for; a descriptor without such a variant gets a
/// private runtime copy after the match.
pub(super) fn emit_string_case_table_lookup(
    ctx: &mut FunctionContext<'_>,
    cases: &[callable_dispatch::RuntimeCallableCase],
    origin: Option<u64>,
    matched_label: &str,
    miss_label: &str,
) {
    let mut words = Vec::new();
    let mut rows = 0usize;
    let mut runtime_origin_copy = false;
    for case in cases {
        let Some(php_name) = case.php_name.as_ref() else { continue; };
        let descriptor = match origin {
            Some(origin) => callable_descriptor::static_descriptor_with_origin(
                ctx.data, &case.descriptor_label, origin,
            )
            .unwrap_or_else(|| {
                runtime_origin_copy = true;
                case.descriptor_label.clone()
            }),
            None => case.descriptor_label.clone(),
        };
        // The per-case compare accepted the bare name and its `\`-prefixed spelling.
        let mut spellings = vec![php_name.clone()];
        if !php_name.starts_with('\\') {
            spellings.push(format!("\\{}", php_name));
        }
        for spelling in spellings {
            let (label, len) = ctx.data.add_string(spelling.as_bytes());
            words.extend([
                DataWord::Symbol(label),
                DataWord::U64(len as u64),
                DataWord::Symbol(descriptor.clone()),
            ]);
            rows += 1;
        }
    }
    if rows == 0 {
        abi::emit_jump(ctx.emitter, miss_label);
        return;
    }
    let table = ctx.data.add_words(words);
    let scan = ScanLabels::new(ctx, "callable_name_table");
    emit_scan_start(ctx, &table, rows);
    ctx.emitter.label(&scan.top);
    emit_scan_exhausted_branch(ctx, &scan.exhausted);
    // -- compare the row length, then the bytes case-insensitively --
    let state = SCAN_STATE_BYTES;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x9, [sp]");                            // load the current callable-name row
            ctx.emitter.instruction("ldr x4, [x9, #8]");                        // load the row's name length
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x2", state + 8);
            ctx.emitter.instruction("cmp x2, x4");                              // a name of another length cannot match this row
            ctx.emitter.instruction(&format!("b.ne {}", scan.next));            // skip the byte compare for a different length
            ctx.emitter.instruction("ldr x3, [x9]");                            // load the row's name bytes
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x1", state);
            abi::emit_call_label(ctx.emitter, "__rt_strcasecmp");
            ctx.emitter.instruction("cmp x0, #0");                              // did the runtime name match this row case-insensitively?
            ctx.emitter.instruction(&format!("b.ne {}", scan.next));            // keep scanning after a different name
            ctx.emitter.instruction("ldr x9, [sp]");                            // reload the matched row after the compare call
            ctx.emitter.instruction("ldr x0, [x9, #16]");                       // select the row's callable descriptor
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov r10, QWORD PTR [rsp]");                // load the current callable-name row
            ctx.emitter.instruction("mov rcx, QWORD PTR [r10 + 8]");            // load the row's name length
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rsi", state + 8);
            ctx.emitter.instruction("cmp rsi, rcx");                            // a name of another length cannot match this row
            ctx.emitter.instruction(&format!("jne {}", scan.next));             // skip the byte compare for a different length
            ctx.emitter.instruction("mov rdx, QWORD PTR [r10]");                // load the row's name bytes
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rdi", state);
            abi::emit_call_label(ctx.emitter, "__rt_strcasecmp");
            ctx.emitter.instruction("test rax, rax");                           // did the runtime name match this row case-insensitively?
            ctx.emitter.instruction(&format!("jne {}", scan.next));             // keep scanning after a different name
            ctx.emitter.instruction("mov r10, QWORD PTR [rsp]");                // reload the matched row after the compare call
            ctx.emitter.instruction("mov rax, QWORD PTR [r10 + 16]");           // select the row's callable descriptor
        }
    }
    abi::emit_release_temporary_stack(ctx.emitter, SCAN_STATE_BYTES);
    if let (Some(origin), true) = (origin, runtime_origin_copy) {
        emit_runtime_origin_copy_unless_marked(ctx, origin, matched_label);
    }
    abi::emit_jump(ctx.emitter, matched_label);
    emit_scan_advance(ctx, &scan, STRING_ROW_BYTES, miss_label);
}

/// Replaces a matched descriptor that does not record `origin` yet (a row without a static
/// origin variant) with a private runtime copy that does; a marked variant reaches `done_label`.
fn emit_runtime_origin_copy_unless_marked(ctx: &mut FunctionContext<'_>, origin: u64, done_label: &str) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x9, [x0]");                            // load the selected descriptor kind word
            ctx.emitter.instruction(&format!("tst x9, #{}", origin));           // is it a static variant that records the origin?
            ctx.emitter.instruction(&format!("b.ne {}", done_label));           // a marked variant needs no runtime copy
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("test QWORD PTR [rax], {}", origin)); // is it a static variant that records the origin?
            ctx.emitter.instruction(&format!("jnz {}", done_label));            // a marked variant needs no runtime copy
        }
    }
    callable_descriptor::emit_runtime_copy_with_origin(ctx.emitter, origin);
}

/// Where the receiver (and, for callable arrays, the method name) of a scan sits on the
/// caller's temporary stack.
#[derive(Clone, Copy)]
pub(super) enum ReceiverSelector {
    /// The 64-byte callable-array selector block: method tag/payload and receiver tag/payload.
    CallableArray,
    /// The 32-byte unboxed object block: tag at 0, receiver pointer at 16.
    InvokableObject,
}

impl ReceiverSelector {
    /// Returns the temporary-stack offset of the receiver pointer before the scan state.
    fn receiver_offset(self) -> usize {
        match self {
            Self::CallableArray => MIXED_RECEIVER_PAYLOAD_OFFSET,
            Self::InvokableObject => MIXED_VALUE_PAYLOAD_OFFSET,
        }
    }
}

/// Selects a receiver-bound descriptor for `targets` from the saved selector block.
///
/// On a match the integer result register holds a fresh runtime descriptor owning one retained
/// receiver and control reaches `selected_label`; otherwise `miss_label`. The selector block
/// stays on the temporary stack on both exits.
pub(super) fn emit_instance_target_table_lookup(
    ctx: &mut FunctionContext<'_>,
    targets: &[RuntimeArrayInstanceMethodTarget],
    selector: ReceiverSelector,
    selected_label: &str,
    miss_label: &str,
) -> Result<()> {
    emit_instance_target_table_scan(ctx, targets, selector, selected_label, miss_label, true)
}

/// Reports whether a row of `targets` matches the saved selector block, without binding it.
///
/// Control reaches `matched_label` or `miss_label` with the selector block still on the
/// temporary stack and no descriptor allocated (`is_callable()` only needs the verdict).
pub(super) fn emit_instance_target_table_match(
    ctx: &mut FunctionContext<'_>,
    targets: &[RuntimeArrayInstanceMethodTarget],
    selector: ReceiverSelector,
    matched_label: &str,
    miss_label: &str,
) -> Result<()> {
    emit_instance_target_table_scan(ctx, targets, selector, matched_label, miss_label, false)
}

/// Scans the receiver rows; a match binds the row's template only when `bind` is set.
fn emit_instance_target_table_scan(
    ctx: &mut FunctionContext<'_>,
    targets: &[RuntimeArrayInstanceMethodTarget],
    selector: ReceiverSelector,
    selected_label: &str,
    miss_label: &str,
    bind: bool,
) -> Result<()> {
    if targets.is_empty() {
        abi::emit_jump(ctx.emitter, miss_label);
        return Ok(());
    }
    let mut words = Vec::new();
    for target in targets {
        // Templates (and their entry thunks) are module-wide and emitted on first request.
        let template = runtime_instance_method_descriptor_template(
            ctx,
            &target.class_name,
            &target.method_name,
            &target.method_key,
            &target.impl_class,
            &target.sig,
        )?;
        let (name_label, name_len) = ctx.data.add_string(target.method_name.as_bytes());
        words.extend([
            DataWord::U64(target.class_id),
            DataWord::Symbol(name_label),
            DataWord::U64(name_len as u64),
            DataWord::Symbol(template.descriptor_label),
        ]);
    }
    let table = ctx.data.add_words(words);
    let receiver_offset = selector.receiver_offset();
    if matches!(selector, ReceiverSelector::CallableArray) {
        emit_branch_if_stack_tag_mismatch(ctx, MIXED_RECEIVER_TAG_OFFSET, MIXED_TAG_OBJECT, miss_label);
        emit_branch_if_stack_tag_mismatch(ctx, MIXED_METHOD_TAG_OFFSET, MIXED_TAG_STRING, miss_label);
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x9", receiver_offset);
            ctx.emitter.instruction(&format!("cbz x9, {}", miss_label));        // a null receiver matches no class row
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "r10", receiver_offset);
            ctx.emitter.instruction("test r10, r10");                           // a null receiver matches no class row
            ctx.emitter.instruction(&format!("je {}", miss_label));             // leave the scan before reading a class id
        }
    }
    let scan = ScanLabels::new(ctx, "callable_receiver_table");
    let matched = ctx.next_label("callable_receiver_table_matched");
    emit_scan_start(ctx, &table, targets.len());
    ctx.emitter.label(&scan.top);
    emit_scan_exhausted_branch(ctx, &scan.exhausted);
    let state = SCAN_STATE_BYTES;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x9, [sp]");                            // load the current receiver row
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x10", state + receiver_offset);
            ctx.emitter.instruction("ldr x10, [x10]");                          // load the receiver's runtime class id
            ctx.emitter.instruction("ldr x11, [x9]");                           // load the row's class id
            ctx.emitter.instruction("cmp x10, x11");                            // only the row's exact class binds this target
            ctx.emitter.instruction(&format!("b.ne {}", scan.next));            // try the next receiver row
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov r10, QWORD PTR [rsp]");                // load the current receiver row
            abi::emit_load_temporary_stack_slot(ctx.emitter, "r11", state + receiver_offset);
            ctx.emitter.instruction("mov r11, QWORD PTR [r11]");                // load the receiver's runtime class id
            ctx.emitter.instruction("cmp r11, QWORD PTR [r10]");                // only the row's exact class binds this target
            ctx.emitter.instruction(&format!("jne {}", scan.next));             // try the next receiver row
        }
    }
    if matches!(selector, ReceiverSelector::CallableArray) {
        let method_ptr = state + MIXED_METHOD_PAYLOAD_OFFSET;
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction("ldr x4, [x9, #16]");                   // load the row's method name length
                abi::emit_load_temporary_stack_slot(ctx.emitter, "x2", method_ptr + 8);
                ctx.emitter.instruction("cmp x2, x4");                          // a method name of another length cannot match
                ctx.emitter.instruction(&format!("b.ne {}", scan.next));        // skip the byte compare for a different length
                ctx.emitter.instruction("ldr x3, [x9, #8]");                    // load the row's method name bytes
                abi::emit_load_temporary_stack_slot(ctx.emitter, "x1", method_ptr);
                abi::emit_call_label(ctx.emitter, "__rt_strcasecmp");
                ctx.emitter.instruction("cmp x0, #0");                          // did the method name match this row case-insensitively?
                ctx.emitter.instruction(&format!("b.ne {}", scan.next));        // keep scanning after a different method
            }
            Arch::X86_64 => {
                ctx.emitter.instruction("mov rcx, QWORD PTR [r10 + 16]");       // load the row's method name length
                abi::emit_load_temporary_stack_slot(ctx.emitter, "rsi", method_ptr + 8);
                ctx.emitter.instruction("cmp rsi, rcx");                        // a method name of another length cannot match
                ctx.emitter.instruction(&format!("jne {}", scan.next));         // skip the byte compare for a different length
                ctx.emitter.instruction("mov rdx, QWORD PTR [r10 + 8]");        // load the row's method name bytes
                abi::emit_load_temporary_stack_slot(ctx.emitter, "rdi", method_ptr);
                abi::emit_call_label(ctx.emitter, "__rt_strcasecmp");
                ctx.emitter.instruction("test rax, rax");                       // did the method name match this row case-insensitively?
                ctx.emitter.instruction(&format!("jne {}", scan.next));         // keep scanning after a different method
            }
        }
    }
    abi::emit_jump(ctx.emitter, &matched);
    emit_scan_advance(ctx, &scan, INSTANCE_ROW_BYTES, miss_label);

    ctx.emitter.label(&matched);
    if !bind {
        abi::emit_release_temporary_stack(ctx.emitter, SCAN_STATE_BYTES);
        abi::emit_jump(ctx.emitter, selected_label);
        return Ok(());
    }
    // -- bind the matched row's template to a retained receiver --
    let result_reg = abi::int_result_reg(ctx.emitter);
    let descriptor_reg = abi::nested_call_reg(ctx.emitter);
    let template_reg = abi::symbol_scratch_reg(ctx.emitter);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x9, [sp]");                            // reload the matched receiver row
            ctx.emitter.instruction("ldr x9, [x9, #24]");                       // load the row's descriptor template
            ctx.emitter.instruction("str x9, [sp]");                            // keep the template in the retired scan slot across allocation
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov r10, QWORD PTR [rsp]");                // reload the matched receiver row
            ctx.emitter.instruction("mov r10, QWORD PTR [r10 + 24]");           // load the row's descriptor template
            ctx.emitter.instruction("mov QWORD PTR [rsp], r10");                // keep the template in the retired scan slot across allocation
        }
    }
    let receiver_ty = PhpType::Object(targets[0].class_name.clone());
    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, state + receiver_offset);
    abi::emit_incref_if_refcounted(ctx.emitter, &receiver_ty);
    abi::emit_push_reg(ctx.emitter, result_reg);
    abi::emit_load_int_immediate(
        ctx.emitter,
        result_reg,
        (callable_descriptor::CALLABLE_DESC_RUNTIME_CAPTURE_OFFSET + 16) as i64,
    );
    abi::emit_call_label(ctx.emitter, "__rt_heap_alloc");
    abi::emit_reg_move(ctx.emitter, descriptor_reg, result_reg);
    abi::emit_load_temporary_stack_slot(ctx.emitter, template_reg, 16);
    callable_descriptor::emit_copy_descriptor_header_to_runtime(ctx.emitter, descriptor_reg, template_reg);
    abi::emit_pop_reg(ctx.emitter, result_reg);
    abi::emit_release_temporary_stack(ctx.emitter, SCAN_STATE_BYTES);
    callable_descriptor::emit_store_current_result_to_runtime_capture(
        ctx.emitter,
        descriptor_reg,
        0,
        &receiver_ty,
    );
    abi::emit_reg_move(ctx.emitter, result_reg, descriptor_reg);
    abi::emit_jump(ctx.emitter, selected_label);
    Ok(())
}

/// Labels of one table scan.
struct ScanLabels {
    top: String,
    next: String,
    exhausted: String,
}

impl ScanLabels {
    /// Allocates the loop, advance and exhaustion labels of one scan.
    fn new(ctx: &mut FunctionContext<'_>, prefix: &str) -> Self {
        Self {
            top: ctx.next_label(&format!("{prefix}_scan")),
            next: ctx.next_label(&format!("{prefix}_next")),
            exhausted: ctx.next_label(&format!("{prefix}_exhausted")),
        }
    }
}

/// Pushes the scan state: the first row's address and the row count.
fn emit_scan_start(ctx: &mut FunctionContext<'_>, table: &str, rows: usize) {
    abi::emit_reserve_temporary_stack(ctx.emitter, SCAN_STATE_BYTES);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(ctx.emitter, "x9", table);
            ctx.emitter.instruction("str x9, [sp]");                            // start the scan at the first candidate row
            abi::emit_load_int_immediate(ctx.emitter, "x9", rows as i64);
            ctx.emitter.instruction("str x9, [sp, #8]");                        // remember how many candidate rows remain
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "r10", table);
            ctx.emitter.instruction("mov QWORD PTR [rsp], r10");                // start the scan at the first candidate row
            abi::emit_load_int_immediate(ctx.emitter, "r10", rows as i64);
            ctx.emitter.instruction("mov QWORD PTR [rsp + 8], r10");            // remember how many candidate rows remain
        }
    }
}

/// Leaves the scan once no row remains.
fn emit_scan_exhausted_branch(ctx: &mut FunctionContext<'_>, exhausted: &str) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x9, [sp, #8]");                        // load the remaining candidate row count
            ctx.emitter.instruction(&format!("cbz x9, {}", exhausted));         // stop after the last candidate row
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp QWORD PTR [rsp + 8], 0");              // does any candidate row remain?
            ctx.emitter.instruction(&format!("je {}", exhausted));              // stop after the last candidate row
        }
    }
}

/// Emits the advance step and the exhaustion exit, which drops the scan state.
fn emit_scan_advance(ctx: &mut FunctionContext<'_>, scan: &ScanLabels, row_bytes: usize, miss_label: &str) {
    ctx.emitter.label(&scan.next);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x9, [sp]");                            // load the row just rejected
            ctx.emitter.instruction(&format!("add x9, x9, #{}", row_bytes));    // step to the next candidate row
            ctx.emitter.instruction("str x9, [sp]");                            // publish the advanced scan cursor
            ctx.emitter.instruction("ldr x9, [sp, #8]");                        // load the remaining candidate row count
            ctx.emitter.instruction("sub x9, x9, #1");                          // one fewer candidate row remains
            ctx.emitter.instruction("str x9, [sp, #8]");                        // publish the remaining candidate row count
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("add QWORD PTR [rsp], {}", row_bytes)); // step to the next candidate row
            ctx.emitter.instruction("sub QWORD PTR [rsp + 8], 1");              // one fewer candidate row remains
        }
    }
    abi::emit_jump(ctx.emitter, &scan.top);
    ctx.emitter.label(&scan.exhausted);
    abi::emit_release_temporary_stack(ctx.emitter, SCAN_STATE_BYTES);
    abi::emit_jump(ctx.emitter, miss_label);
}
