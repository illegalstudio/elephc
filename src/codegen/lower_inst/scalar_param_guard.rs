//! Purpose:
//! Lowers `Op::ScalarParamGuard`, the runtime check on a boxed `mixed` argument bound to a
//! declared non-nullable `int`/`float`/`string`/`bool` parameter.
//!
//! Called from:
//! - `crate::codegen::lower_inst::lower_instruction`.
//!
//! Key details:
//! - The hot path reads the cell's tag and branches once; everything else lives in the
//!   function's out-of-line cold section (`FunctionContext::emit_cold_path`), so the check
//!   adds no EIR basic block and loop passes keep seeing a straight-line body.
//! - The cold path peels nested cells, re-checks, accepts a `Stringable` object for a
//!   coercive `string`, and otherwise throws `TypeError` with PHP's value-type name: the
//!   object's class, `true`/`false` for booleans, `Closure` for callables.
//! - Registers are free to clobber: the instruction has no result and later instructions
//!   reload their operands.

use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::codegen::{CodegenIrError, Result};
use crate::ir::{Immediate, Instruction};
use crate::types::PhpType;

use super::super::context::FunctionContext;

/// The acceptance rule encoded in the instruction's mode letter.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Rule {
    /// Coercive mode: any scalar; `string` also takes a `Stringable` object.
    Coercive { string: bool },
    /// `strict_types=1`: exactly this runtime tag, plus int for a `float` parameter.
    Strict { tag: i64, int_widens: bool },
}

impl Rule {
    /// Decodes the mode letter written by EIR lowering.
    fn parse(mode: char) -> Option<Self> {
        Some(match mode {
            's' => Rule::Coercive { string: true },
            'i' | 'f' | 'b' => Rule::Coercive { string: false },
            'S' => Rule::Strict { tag: 1, int_widens: false },
            'I' => Rule::Strict { tag: 0, int_widens: false },
            'F' => Rule::Strict { tag: 2, int_widens: true },
            'B' => Rule::Strict { tag: 3, int_widens: false },
            _ => return None,
        })
    }

    /// Returns whether a statically known scalar operand type passes without a runtime test.
    fn accepts_static(self, ty: &PhpType) -> bool {
        let tag = match ty {
            PhpType::Int => 0,
            PhpType::Str => 1,
            PhpType::Float => 2,
            PhpType::Bool | PhpType::False => 3,
            _ => return false,
        };
        match self {
            Rule::Coercive { .. } => true,
            Rule::Strict { tag: expected, int_widens } => tag == expected || (int_widens && tag == 0),
        }
    }
}

/// Lowers one scalar-parameter guard: fast tag test inline, rejection out of line.
pub(in crate::codegen::lower_inst) fn lower_scalar_param_guard(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let value = *inst.operands.first().ok_or_else(|| {
        CodegenIrError::unsupported("scalar_param_guard without operand".to_string())
    })?;
    let Some(Immediate::Data(data_id)) = inst.immediate else {
        return Err(CodegenIrError::unsupported(
            "scalar_param_guard without a message immediate".to_string(),
        ));
    };
    let encoded = ctx
        .module
        .data
        .strings
        .get(data_id.as_raw() as usize)
        .cloned()
        .ok_or_else(|| CodegenIrError::missing_entry("data string", data_id.as_raw()))?;
    let mut parts = encoded.splitn(3, '\u{1f}');
    let (Some(mode), Some(prefix), Some(suffix)) = (parts.next(), parts.next(), parts.next()) else {
        return Err(CodegenIrError::unsupported("malformed scalar_param_guard message".to_string()));
    };
    let rule = mode
        .chars()
        .next()
        .and_then(Rule::parse)
        .ok_or_else(|| CodegenIrError::unsupported(format!("unknown scalar_param_guard mode {mode}")))?;
    let (prefix, suffix) = (prefix.to_string(), suffix.to_string());

    // Constant folding can retype the operand under the guard; a raw value is decided now.
    let loaded = ctx.load_value_to_result(value)?.codegen_repr();
    if !matches!(loaded, PhpType::Mixed | PhpType::Union(_)) {
        if rule.accepts_static(&loaded) {
            return Ok(());
        }
        let given = match loaded {
            PhpType::Int => "int",
            PhpType::Str => "string",
            PhpType::Float => "float",
            PhpType::Bool | PhpType::False => "bool",
            PhpType::Void => "null",
            PhpType::Array(_) | PhpType::AssocArray { .. } => "array",
            PhpType::Callable => "Closure",
            _ => "object",
        };
        super::exceptions::emit_type_error(ctx, &format!("{prefix}{given}{suffix}"));
        return Ok(());
    }

    let cold = ctx.next_label("scalar_param_guard_cold");
    let done = ctx.next_label("scalar_param_guard_done");
    emit_hot_check(ctx, rule, &cold, &done);
    ctx.emitter.label(&done);
    ctx.emit_cold_path(|ctx| {
        ctx.emitter.label(&cold);
        emit_cold_check(ctx, inst, value, rule, &done, &prefix, &suffix)
    })
}

/// Emits the inline tag test: the accepted tags fall through to `done`, everything else
/// (a null cell pointer, a nested cell, any refused tag) enters the cold path.
fn emit_hot_check(ctx: &mut FunctionContext<'_>, rule: Rule, cold: &str, done: &str) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            // -- hot path: read the boxed tag and accept the declared scalar inline --
            ctx.emitter.instruction("cbz x0, 1f");                              // a null cell pointer is PHP null: reject out of line
            ctx.emitter.instruction("ldr x9, [x0]");                            // read the boxed value's runtime tag
            match rule {
                Rule::Coercive { .. } => {
                    ctx.emitter.instruction("cmp x9, #3");                      // tags 0..3 are int, string, float and bool
                    ctx.emitter.instruction(&format!("b.ls {done}"));           // every scalar binds coercively
                }
                Rule::Strict { tag, int_widens } => {
                    ctx.emitter.instruction(&format!("cmp x9, #{tag}"));        // strict mode accepts the declared scalar tag
                    ctx.emitter.instruction(&format!("b.eq {done}"));           // the exact scalar binds unchanged
                    if int_widens {
                        ctx.emitter.instruction("cmp x9, #0");                  // strict float still accepts an int
                        ctx.emitter.instruction(&format!("b.eq {done}"));       // PHP widens the int to float
                    }
                }
            }
            // -- every refused shape leaves for the cold section --
            ctx.emitter.label("1");
            ctx.emitter.instruction(&format!("b {cold}"));                      // enter the out-of-line rejection path
        }
        Arch::X86_64 => {
            // -- hot path: read the boxed tag and accept the declared scalar inline --
            ctx.emitter.instruction("test rax, rax");                           // is the boxed cell pointer null?
            ctx.emitter.instruction(&format!("jz {cold}"));                     // a null cell pointer is PHP null: reject out of line
            ctx.emitter.instruction("mov r10, QWORD PTR [rax]");                // read the boxed value's runtime tag
            match rule {
                Rule::Coercive { .. } => {
                    ctx.emitter.instruction("cmp r10, 3");                      // tags 0..3 are int, string, float and bool
                    ctx.emitter.instruction(&format!("jbe {done}"));            // every scalar binds coercively
                }
                Rule::Strict { tag, int_widens } => {
                    ctx.emitter.instruction(&format!("cmp r10, {tag}"));        // strict mode accepts the declared scalar tag
                    ctx.emitter.instruction(&format!("je {done}"));             // the exact scalar binds unchanged
                    if int_widens {
                        ctx.emitter.instruction("test r10, r10");               // strict float still accepts an int
                        ctx.emitter.instruction(&format!("je {done}"));         // PHP widens the int to float
                    }
                }
            }
            // -- every refused shape leaves for the cold section --
            ctx.emitter.instruction(&format!("jmp {cold}"));                    // enter the out-of-line rejection path
        }
    }
}

/// Branches to `done` when the unboxed tag in the integer result register passes `rule`.
fn emit_peeled_accept(ctx: &mut FunctionContext<'_>, rule: Rule, done: &str) {
    // -- cold path: re-check the peeled tag, which a nested cell may have hidden --
    match ctx.emitter.target.arch {
        Arch::AArch64 => match rule {
            Rule::Coercive { .. } => {
                ctx.emitter.instruction("cmp x0, #3");                          // tags 0..3 are the PHP scalars
                ctx.emitter.instruction("b.hi 1f");                             // keep refused tags on the rejection path
                ctx.emitter.instruction(&format!("b {done}"));                  // a peeled scalar binds coercively
                ctx.emitter.label("1");
            }
            Rule::Strict { tag, int_widens } => {
                ctx.emitter.instruction(&format!("cmp x0, #{tag}"));            // compare the peeled tag with the declared scalar
                ctx.emitter.instruction("b.ne 1f");                             // keep other tags on the rejection path
                ctx.emitter.instruction(&format!("b {done}"));                  // the exact scalar binds unchanged
                ctx.emitter.label("1");
                if int_widens {
                    ctx.emitter.instruction("cbnz x0, 1f");                     // only an int widens to a strict float
                    ctx.emitter.instruction(&format!("b {done}"));              // PHP widens the int to float
                    ctx.emitter.label("1");
                }
            }
        },
        Arch::X86_64 => match rule {
            Rule::Coercive { .. } => {
                ctx.emitter.instruction("cmp rax, 3");                          // tags 0..3 are the PHP scalars
                ctx.emitter.instruction(&format!("jbe {done}"));                // a peeled scalar binds coercively
            }
            Rule::Strict { tag, int_widens } => {
                ctx.emitter.instruction(&format!("cmp rax, {tag}"));            // compare the peeled tag with the declared scalar
                ctx.emitter.instruction(&format!("je {done}"));                 // the exact scalar binds unchanged
                if int_widens {
                    ctx.emitter.instruction("test rax, rax");                   // only an int widens to a strict float
                    ctx.emitter.instruction(&format!("je {done}"));             // PHP widens the int to float
                }
            }
        },
    }
}

/// Emits the cold path: peel, re-check, `Stringable` acceptance, then build and throw.
fn emit_cold_check(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    value: crate::ir::ValueId,
    rule: Rule,
    done: &str,
    prefix: &str,
    suffix: &str,
) -> Result<()> {
    let null_label = ctx.next_label("scalar_param_guard_null");
    let name_label = ctx.next_label("scalar_param_guard_name");
    // -- cold path: peel nested cells before deciding --
    ctx.load_value_to_result(value)?;
    emit_branch_if_null_cell(ctx, &null_label);
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    emit_peeled_accept(ctx, rule, done);
    if rule == (Rule::Coercive { string: true }) {
        // -- cold path: a Stringable object still binds to a coercive string --
        emit_tag_not_equal_branch(ctx, 6, &name_label);
        if ctx.module.required_runtime_features.eval_bridge {
            super::builtins::lower_eval_object_is_a(ctx, inst, value, "Stringable", false)?;
        } else if let Some((target_id, target_kind)) =
            super::objects::classify_named_target(ctx, "Stringable")
        {
            ctx.load_value_to_reg(value, abi::int_arg_reg_name(ctx.emitter.target, 0))?;
            super::objects::emit_match_call(ctx, target_id, target_kind, "__rt_mixed_instanceof");
        } else {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
        }
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction("cbz x0, 1f");                          // a non-Stringable object is rejected
                ctx.emitter.instruction(&format!("b {done}"));                  // a Stringable object binds to string
                ctx.emitter.label("1");
            }
            Arch::X86_64 => {
                ctx.emitter.instruction("test rax, rax");                       // did the Stringable match succeed?
                ctx.emitter.instruction(&format!("jnz {done}"));                // a Stringable object binds to string
            }
        }
    }
    ctx.emitter.label(&name_label);
    emit_rejected_type_name(ctx, value, &null_label)?;
    emit_throw_with_name(ctx, prefix, suffix);
    Ok(())
}

/// Branches to `target` when the cell pointer in the integer result register is null.
fn emit_branch_if_null_cell(ctx: &mut FunctionContext<'_>, target: &str) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cbnz x0, 1f");                             // a real cell is unboxed below
            ctx.emitter.instruction(&format!("b {target}"));                    // a null cell pointer names PHP null
            ctx.emitter.label("1");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // is the boxed cell pointer null?
            ctx.emitter.instruction(&format!("jz {target}"));                   // a null cell pointer names PHP null
        }
    }
}

/// Branches to `target` when the unboxed tag differs from `tag`.
fn emit_tag_not_equal_branch(ctx: &mut FunctionContext<'_>, tag: i64, target: &str) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cmp x0, #{tag}"));                // compare the peeled tag with this kind
            ctx.emitter.instruction("b.eq 1f");                                 // keep this kind on the current path
            ctx.emitter.instruction(&format!("b {target}"));                    // every other kind goes to the target
            ctx.emitter.label("1");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("cmp rax, {tag}"));                // compare the peeled tag with this kind
            ctx.emitter.instruction(&format!("jne {target}"));                  // every other kind goes to the target
        }
    }
}

/// Leaves PHP's name for the rejected value's type in the string result registers.
///
/// `null_label` is entered for a null cell pointer; every path meets at the end.
fn emit_rejected_type_name(
    ctx: &mut FunctionContext<'_>,
    value: crate::ir::ValueId,
    null_label: &str,
) -> Result<()> {
    let joined = ctx.next_label("scalar_param_guard_named");
    let object = ctx.next_label("scalar_param_guard_object");
    let boolean = ctx.next_label("scalar_param_guard_bool");
    let words: [(i64, &[u8]); 9] = [
        (0, b"int"), (1, b"string"), (2, b"float"), (4, b"array"), (5, b"array"),
        (8, b"null"), (9, b"resource"), (10, b"Closure"), (-1, b"object"),
    ];
    let word_labels: Vec<String> =
        words.iter().map(|_| ctx.next_label("scalar_param_guard_word")).collect();
    // -- message: dispatch on the rejected value's tag to PHP's type name --
    ctx.load_value_to_result(value)?;
    emit_branch_if_null_cell(ctx, null_label);
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    super::enums::emit_mixed_tag_branch(ctx, abi::int_result_reg(ctx.emitter), 6, &object);
    super::enums::emit_mixed_tag_branch(ctx, abi::int_result_reg(ctx.emitter), 3, &boolean);
    for ((tag, _), label) in words.iter().zip(&word_labels) {
        if *tag >= 0 {
            super::enums::emit_mixed_tag_branch(ctx, abi::int_result_reg(ctx.emitter), *tag, label);
        }
    }
    abi::emit_jump(ctx.emitter, word_labels.last().expect("fallback word"));

    // -- message: an object is named by its runtime class --
    ctx.emitter.label(&object);
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.emitter.instruction("mov x0, x1"),                 // expose the unboxed object pointer to the class lookup
        Arch::X86_64 => ctx.emitter.instruction("mov rax, rdi"),                // expose the unboxed object pointer to the class lookup
    }
    super::builtins::types::emit_dynamic_object_class_name(ctx, "get_class");
    abi::emit_jump(ctx.emitter, &joined);

    // -- message: a boolean is named by its value, true or false --
    ctx.emitter.label(&boolean);
    let (true_label, true_len) = ctx.data.add_string(b"true");
    let (false_label, false_len) = ctx.data.add_string(b"false");
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cbz x1, 1f");                              // a zero boolean payload is PHP false
            abi::emit_symbol_address(ctx.emitter, ptr_reg, &true_label);
            abi::emit_load_int_immediate(ctx.emitter, len_reg, true_len as i64);
            abi::emit_jump(ctx.emitter, &joined);
            ctx.emitter.label("1");
        }
        Arch::X86_64 => {
            let falsy = ctx.next_label("scalar_param_guard_false");
            ctx.emitter.instruction("test rdi, rdi");                           // inspect the unboxed boolean payload
            ctx.emitter.instruction(&format!("jz {falsy}"));                    // a zero boolean payload is PHP false
            abi::emit_symbol_address(ctx.emitter, ptr_reg, &true_label);
            abi::emit_load_int_immediate(ctx.emitter, len_reg, true_len as i64);
            abi::emit_jump(ctx.emitter, &joined);
            ctx.emitter.label(&falsy);
        }
    }
    abi::emit_symbol_address(ctx.emitter, ptr_reg, &false_label);
    abi::emit_load_int_immediate(ctx.emitter, len_reg, false_len as i64);
    abi::emit_jump(ctx.emitter, &joined);

    ctx.emitter.label(null_label);
    emit_word(ctx, b"null");
    abi::emit_jump(ctx.emitter, &joined);
    for ((_, word), label) in words.iter().zip(&word_labels) {
        ctx.emitter.label(label);
        emit_word(ctx, word);
        abi::emit_jump(ctx.emitter, &joined);
    }
    ctx.emitter.label(&joined);
    Ok(())
}

/// Loads a static type word into the string result registers.
fn emit_word(ctx: &mut FunctionContext<'_>, word: &[u8]) {
    let (label, len) = ctx.data.add_string(word);
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    abi::emit_symbol_address(ctx.emitter, ptr_reg, &label);
    abi::emit_load_int_immediate(ctx.emitter, len_reg, len as i64);
}

/// Throws `TypeError(prefix . <name in the string result registers> . suffix)`.
fn emit_throw_with_name(ctx: &mut FunctionContext<'_>, prefix: &str, suffix: &str) {
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    let (second_ptr, second_len) = match ctx.emitter.target.arch {
        Arch::AArch64 => ("x3", "x4"),
        Arch::X86_64 => ("rdi", "rsi"),
    };
    // -- message: prefix . type name . suffix, persisted, then thrown as TypeError --
    abi::emit_push_reg_pair(ctx.emitter, ptr_reg, len_reg);
    let (prefix_label, prefix_len) = ctx.data.add_string(prefix.as_bytes());
    abi::emit_symbol_address(ctx.emitter, ptr_reg, &prefix_label);
    abi::emit_load_int_immediate(ctx.emitter, len_reg, prefix_len as i64);
    abi::emit_pop_reg_pair(ctx.emitter, second_ptr, second_len);
    abi::emit_call_label(ctx.emitter, "__rt_concat");
    super::enums::emit_concat_current_string_with_static_suffix(ctx, suffix);
    abi::emit_call_label(ctx.emitter, "__rt_str_persist");
    super::enums::emit_throw_type_error_from_string_result(ctx);
}
