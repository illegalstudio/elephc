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
//! - For `int` and `float` the instruction yields the converted value: the hot path loads an
//!   int or float payload directly; the cold path applies PHP's coercive rules (a fully
//!   numeric string converts, a lossy float or float-string to int emits the 8.1+
//!   deprecation, a float outside the int range and every other string throw).
//! - Registers are free to clobber: later instructions reload their operands, and the only
//!   value the instruction leaves is its own result.
//! - Descriptor invokers (calls through a callable value) have no `FunctionContext`; their
//!   coercive `int`/`float` binding calls the module helpers `emit_invoker_numeric_param_helpers`
//!   emits once from this same conversion code.

use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::codegen::{CodegenIrError, Result};
use crate::ir::{Immediate, Instruction};
use crate::types::PhpType;

use super::super::context::FunctionContext;
use super::store_if_result;

/// The numeric parameter kinds whose guard yields the converted value.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Numeric {
    Int,
    Float,
}

/// Decodes the numeric target and strictness of a mode letter, or `None` for string/bool.
fn numeric_mode(mode: char) -> Option<(Numeric, bool)> {
    match mode {
        'i' => Some((Numeric::Int, false)),
        'I' => Some((Numeric::Int, true)),
        'f' => Some((Numeric::Float, false)),
        'F' => Some((Numeric::Float, true)),
        _ => None,
    }
}

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
    let numeric = mode.chars().next().and_then(numeric_mode);
    // A trailing `r`: the operand is an owned temporary no cleanup root covers, so the
    // refusal releases it before throwing.
    let release = mode.chars().nth(1) == Some('r');

    // Constant folding can retype the operand under the guard; a raw value is decided now.
    let loaded = ctx.load_value_to_result(value)?.codegen_repr();
    if !matches!(loaded, PhpType::Mixed | PhpType::Union(_)) {
        if rule.accepts_static(&loaded) {
            if let Some((target, _)) = numeric {
                emit_static_numeric_conversion(ctx, &loaded, target, &prefix, &suffix);
                return store_if_result(ctx, inst);
            }
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
    if let Some((target, strict)) = numeric {
        emit_numeric_hot_check(ctx, target, strict, &cold, &done);
        ctx.emitter.label(&done);
        ctx.emit_cold_path(|ctx| {
            ctx.emitter.label(&cold);
            emit_numeric_cold_check(ctx, inst, value, target, strict, &done, &prefix, &suffix, release)
        })?;
        return store_if_result(ctx, inst);
    }
    emit_hot_check(ctx, rule, &cold, &done);
    ctx.emitter.label(&done);
    ctx.emit_cold_path(|ctx| {
        ctx.emitter.label(&cold);
        emit_cold_check(ctx, inst, value, rule, &done, &prefix, &suffix, release)
    })
}

/// Converts a raw scalar operand (left by a pass that retyped the guard's operand) to the
/// numeric parameter with the same rules as the cold path; nothing is reinterpreted.
///
/// An int or bool payload is already the integer word, and a float is already the float
/// result; a string goes through the numeric scan (and the int precision diagnostics), and a
/// float bound to `int` through the float precision diagnostics. A refused value throws with
/// its static type name.
fn emit_static_numeric_conversion(
    ctx: &mut FunctionContext<'_>,
    loaded: &PhpType,
    target: Numeric,
    prefix: &str,
    suffix: &str,
) {
    let refuse = ctx.next_label("scalar_param_guard_static_refuse");
    let done = ctx.next_label("scalar_param_guard_static_done");
    match (loaded, target) {
        (PhpType::Float, Numeric::Float) => {}
        (PhpType::Float, Numeric::Int) => {
            // -- a raw float bound to int: PHP's precision deprecation or range refusal --
            super::builtins::strings::emit_float_result_int_coercion_diagnostics(ctx, &refuse);
        }
        (PhpType::Str, Numeric::Int) => {
            // -- a raw string bound to int: the numeric scan and precision diagnostics --
            let payload = crate::codegen_support::mixed_unbox_payload_reg(ctx.emitter.target);
            let (string_ptr, _) = abi::string_result_regs(ctx.emitter);
            if string_ptr != payload {
                ctx.emitter.instruction(&format!("mov {payload}, {string_ptr}")); // stage the string pointer where the numeric scan reads it
            }
            emit_string_to_int_parameter(ctx, &refuse);
        }
        (PhpType::Str, Numeric::Float) => {
            // -- a raw string bound to float: only a fully numeric string binds --
            abi::emit_call_label(ctx.emitter, "__rt_str_to_number");
            match ctx.emitter.target.arch {
                Arch::AArch64 => {
                    ctx.emitter.instruction("cbnz x0, 1f");                     // a fully numeric string keeps its parsed float
                    ctx.emitter.instruction(&format!("b {refuse}"));            // any other string is refused
                    ctx.emitter.label("1");
                }
                Arch::X86_64 => {
                    ctx.emitter.instruction("test rax, rax");                   // did the numeric scan accept the whole string?
                    ctx.emitter.instruction(&format!("jz {refuse}"));           // any other string is refused
                }
            }
        }
        (_, Numeric::Float) => emit_int_result_to_float(ctx),
        (_, Numeric::Int) => {}
    }
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&refuse);
    let given = if matches!(loaded, PhpType::Str) { "string" } else { "float" };
    super::exceptions::emit_type_error(ctx, &format!("{prefix}{given}{suffix}"));
    ctx.emitter.label(&done);
}

/// Binds a string (pointer in the Mixed unbox payload register, length in the string length
/// register) to an `int` parameter, leaving the integer in the int result register or jumping
/// to `refuse`.
///
/// An integer-form string is parsed exactly by `__rt_str_numeric_value`, so every value of the
/// int range binds unchanged (`"9223372036854775807"`, `"9007199254740993"`); a value outside
/// it is classified as float-form. Only a float-form string goes through the float path, with
/// PHP's float-string precision deprecation and range refusal; a non-numeric one is refused.
fn emit_string_to_int_parameter(ctx: &mut FunctionContext<'_>, refuse: &str) {
    let payload = crate::codegen_support::mixed_unbox_payload_reg(ctx.emitter.target);
    let (_, length) = abi::string_result_regs(ctx.emitter);
    let float_form = ctx.next_label("scalar_param_guard_float_string");
    let refused = ctx.next_label("scalar_param_guard_string_refused");
    let exact = ctx.next_label("scalar_param_guard_exact_int");
    let (scratch_lo, scratch_hi) = match ctx.emitter.target.arch {
        Arch::AArch64 => ("x9", "x10"),
        Arch::X86_64 => ("r10", "r11"),
    };
    // -- classify the whole string: exact integer, float form, or not numeric --
    abi::emit_push_reg_pair(ctx.emitter, payload, length);
    let ptr_arg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    let len_arg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    // The pointer moves first: on AArch64 the length argument register is the payload register.
    abi::emit_reg_move(ctx.emitter, ptr_arg, payload);
    abi::emit_reg_move(ctx.emitter, len_arg, length);
    abi::emit_call_label(ctx.emitter, "__rt_str_numeric_value");
    let tag = abi::int_result_reg(ctx.emitter);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbnz {length}, {refused}"));      // only a whole-string numeric spelling binds
            ctx.emitter.instruction(&format!("cbz {tag}, {exact}"));            // tag 0: the classifier parsed an exact integer
            ctx.emitter.instruction(&format!("b {float_form}"));                // any other numeric spelling is float-form
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("test {length}, {length}"));       // set flags from the whole-string numeric status
            ctx.emitter.instruction(&format!("jne {refused}"));                 // only a whole-string numeric spelling binds
            ctx.emitter.instruction(&format!("test {tag}, {tag}"));             // tag 0: the classifier parsed an exact integer
            ctx.emitter.instruction(&format!("je {exact}"));                    // bind the exact integer
            ctx.emitter.instruction(&format!("jmp {float_form}"));              // any other numeric spelling is float-form
        }
    }
    // -- an integer-form string binds its exact value --
    ctx.emitter.label(&exact);
    super::enums::emit_move_reg(ctx, tag, payload);
    abi::emit_pop_reg_pair(ctx.emitter, scratch_lo, scratch_hi);
    let done = ctx.next_label("scalar_param_guard_string_int_done");
    abi::emit_jump(ctx.emitter, &done);
    // -- a non-numeric string is refused --
    ctx.emitter.label(&refused);
    abi::emit_pop_reg_pair(ctx.emitter, scratch_lo, scratch_hi);
    abi::emit_jump(ctx.emitter, refuse);
    // -- a float-form string takes the float-string precision and range rules --
    ctx.emitter.label(&float_form);
    abi::emit_pop_reg_pair(ctx.emitter, payload, length);
    super::objects::emit_string_into_int_diagnostics(ctx, refuse);
    ctx.emitter.label(&done);
}

/// Converts the integer result register to a double in the float result register.
fn emit_int_result_to_float(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.emitter.instruction("scvtf d0, x0"),               // widen the integer argument to the float parameter
        Arch::X86_64 => ctx.emitter.instruction("cvtsi2sd xmm0, rax"),          // widen the integer argument to the float parameter
    }
}

/// Emits the inline numeric fast path: an int (or, for `float`, a float) payload is loaded
/// straight from the cell into the result register; every other shape enters the cold path.
fn emit_numeric_hot_check(
    ctx: &mut FunctionContext<'_>,
    target: Numeric,
    strict: bool,
    cold: &str,
    done: &str,
) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            // -- hot path: read the boxed tag and load an accepted payload inline --
            ctx.emitter.instruction("cbz x0, 1f");                              // a null cell pointer is PHP null: reject out of line
            ctx.emitter.instruction("ldr x9, [x0]");                            // read the boxed value's runtime tag
            if target == Numeric::Float {
                ctx.emitter.instruction("cmp x9, #2");                          // is the payload already a float?
                ctx.emitter.instruction("b.ne 3f");                             // other tags convert or reject below
                ctx.emitter.instruction("ldr x9, [x0, #8]");                    // load the float payload bits
                ctx.emitter.instruction("fmov d0, x9");                         // move the float payload into the float result
                ctx.emitter.instruction(&format!("b {done}"));                  // the float binds unchanged
                ctx.emitter.label("3");
            }
            ctx.emitter.instruction("cmp x9, #0");                              // is the payload an int?
            ctx.emitter.instruction("b.eq 2f");                                 // an int binds (and widens for float)
            if !strict {
                ctx.emitter.instruction("cmp x9, #3");                          // is the payload a bool?
                ctx.emitter.instruction("b.eq 2f");                             // a bool binds coercively as 0 or 1
            }
            ctx.emitter.instruction("b 1f");                                    // every other tag needs the cold path
            ctx.emitter.label("2");
            ctx.emitter.instruction("ldr x0, [x0, #8]");                        // load the integer payload
            if target == Numeric::Float {
                ctx.emitter.instruction("scvtf d0, x0");                        // widen the integer to the float parameter
            }
            ctx.emitter.instruction(&format!("b {done}"));                      // the converted value binds
            // -- every refused or converting shape leaves for the cold section --
            ctx.emitter.label("1");
            ctx.emitter.instruction(&format!("b {cold}"));                      // enter the out-of-line conversion path
        }
        Arch::X86_64 => {
            let int_label = ctx.next_label("scalar_param_guard_int");
            // -- hot path: read the boxed tag and load an accepted payload inline --
            ctx.emitter.instruction("test rax, rax");                           // is the boxed cell pointer null?
            ctx.emitter.instruction(&format!("jz {cold}"));                     // a null cell pointer is PHP null: reject out of line
            ctx.emitter.instruction("mov r10, QWORD PTR [rax]");                // read the boxed value's runtime tag
            if target == Numeric::Float {
                let not_float = ctx.next_label("scalar_param_guard_not_float");
                ctx.emitter.instruction("cmp r10, 2");                          // is the payload already a float?
                ctx.emitter.instruction(&format!("jne {not_float}"));           // other tags convert or reject below
                ctx.emitter.instruction("movq xmm0, QWORD PTR [rax + 8]");      // load the float payload into the float result
                ctx.emitter.instruction(&format!("jmp {done}"));                // the float binds unchanged
                ctx.emitter.label(&not_float);
            }
            ctx.emitter.instruction("test r10, r10");                           // is the payload an int?
            ctx.emitter.instruction(&format!("je {int_label}"));                // an int binds (and widens for float)
            if !strict {
                ctx.emitter.instruction("cmp r10, 3");                          // is the payload a bool?
                ctx.emitter.instruction(&format!("je {int_label}"));            // a bool binds coercively as 0 or 1
            }
            // -- every refused or converting shape leaves for the cold section --
            ctx.emitter.instruction(&format!("jmp {cold}"));                    // enter the out-of-line conversion path
            ctx.emitter.label(&int_label);
            ctx.emitter.instruction("mov rax, QWORD PTR [rax + 8]");            // load the integer payload
            if target == Numeric::Float {
                ctx.emitter.instruction("cvtsi2sd xmm0, rax");                  // widen the integer to the float parameter
            }
            ctx.emitter.instruction(&format!("jmp {done}"));                    // the converted value binds
        }
    }
}

/// Emits the numeric cold path: peel, convert by PHP's coercive (or strict) rules, or throw.
#[allow(clippy::too_many_arguments)]
fn emit_numeric_cold_check(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    value: crate::ir::ValueId,
    target: Numeric,
    strict: bool,
    done: &str,
    prefix: &str,
    suffix: &str,
    release: bool,
) -> Result<()> {
    let null_label = ctx.next_label("scalar_param_guard_null");
    let refuse = ctx.next_label("scalar_param_guard_refuse");
    let int_label = ctx.next_label("scalar_param_guard_peeled_int");
    let float_label = ctx.next_label("scalar_param_guard_peeled_float");
    let string_label = ctx.next_label("scalar_param_guard_peeled_string");
    let payload = crate::codegen_support::mixed_unbox_payload_reg(ctx.emitter.target);
    let tag = abi::int_result_reg(ctx.emitter);
    if target == Numeric::Int && !strict {
        // The precision deprecation names the parameter's line (the instruction's span).
        super::core_builtins::publish_diagnostic_site(ctx, inst);
    }
    // -- cold path: peel nested cells and dispatch on the concrete tag --
    ctx.load_value_to_result(value)?;
    emit_branch_if_null_cell(ctx, &null_label);
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    super::enums::emit_mixed_tag_branch(ctx, tag, 0, &int_label);
    if !strict {
        super::enums::emit_mixed_tag_branch(ctx, tag, 3, &int_label);
        super::enums::emit_mixed_tag_branch(ctx, tag, 1, &string_label);
    }
    if target == Numeric::Float || !strict {
        super::enums::emit_mixed_tag_branch(ctx, tag, 2, &float_label);
    }
    abi::emit_jump(ctx.emitter, &refuse);

    // -- cold path: an int or bool payload binds directly --
    ctx.emitter.label(&int_label);
    super::enums::emit_move_reg(ctx, tag, payload);
    if target == Numeric::Float {
        emit_int_result_to_float(ctx);
    }
    abi::emit_jump(ctx.emitter, done);

    // -- cold path: a float binds to float, or to int after PHP's precision rules --
    ctx.emitter.label(&float_label);
    super::mixed_narrowing::emit_float_bits_to_float_result(ctx, payload);
    if target == Numeric::Int {
        super::builtins::strings::emit_float_result_int_coercion_diagnostics(ctx, &refuse);
    }
    abi::emit_jump(ctx.emitter, done);

    // -- cold path: only a fully numeric string binds --
    ctx.emitter.label(&string_label);
    match target {
        Numeric::Int => emit_string_to_int_parameter(ctx, &refuse),
        Numeric::Float => {
            let (string_ptr, _) = abi::string_result_regs(ctx.emitter);
            if string_ptr != payload {
                ctx.emitter.instruction(&format!("mov {string_ptr}, {payload}")); // move the unboxed string pointer into the numeric-scan input
            }
            abi::emit_call_label(ctx.emitter, "__rt_str_to_number");
            match ctx.emitter.target.arch {
                Arch::AArch64 => {
                    ctx.emitter.instruction("cbnz x0, 1f");                     // a fully numeric string keeps its parsed float
                    ctx.emitter.instruction(&format!("b {refuse}"));            // any other string is refused
                    ctx.emitter.label("1");
                }
                Arch::X86_64 => {
                    ctx.emitter.instruction("test rax, rax");                   // did the numeric scan accept the whole string?
                    ctx.emitter.instruction(&format!("jz {refuse}"));           // any other string is refused
                }
            }
        }
    }
    abi::emit_jump(ctx.emitter, done);

    // -- message: name the refused value and throw --
    ctx.emitter.label(&refuse);
    emit_rejected_type_name(ctx, value, &null_label)?;
    emit_throw_with_name(ctx, prefix, suffix, if release { &inst.operands[1..] } else { &[] })?;
    Ok(())
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
    release: bool,
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
    emit_throw_with_name(ctx, prefix, suffix, if release { &inst.operands[1..] } else { &[] })?;
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

/// Throws `TypeError(prefix . <name in the string result registers> . suffix)`, first
/// releasing `releases`, the call's owned operands no cleanup root covers (the refused value
/// among them when it is one).
fn emit_throw_with_name(
    ctx: &mut FunctionContext<'_>,
    prefix: &str,
    suffix: &str,
    releases: &[crate::ir::ValueId],
) -> Result<()> {
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
    if !releases.is_empty() {
        // -- release the call's unpublished owned operands; the persisted message survives --
        let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
        abi::emit_push_reg_pair(ctx.emitter, ptr_reg, len_reg);
        for value in releases {
            // Lowering lists only owned temporaries, so a box is released unconditionally;
            // anything else (a converted string) takes the generic release.
            if matches!(ctx.value_php_type(*value)?.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) {
                ctx.load_value_to_result(*value)?;
                abi::emit_decref_if_refcounted(ctx.emitter, &PhpType::Mixed);
            } else {
                super::ownership::release_value_if_owned(ctx, *value)?;
            }
        }
        abi::emit_pop_reg_pair(ctx.emitter, ptr_reg, len_reg);
    }
    super::enums::emit_throw_type_error_from_string_result(ctx);
    Ok(())
}

/// Label of the module helper that binds a boxed argument to a coercive `int` parameter for a
/// descriptor invoker (see [`emit_invoker_numeric_param_helpers`]).
pub(in crate::codegen) const INVOKER_INT_PARAM_HELPER: &str = "__elephc_invoker_int_param";
/// Label of the module helper that binds a boxed argument to a coercive `float` parameter.
pub(in crate::codegen) const INVOKER_FLOAT_PARAM_HELPER: &str = "__elephc_invoker_float_param";

/// Emits, once per module, the coercive `int`/`float` conversions descriptor invokers call.
///
/// An invoker has no `FunctionContext`, so it cannot inline the numeric-string scan, the
/// precision deprecations or the range refusal the direct-call guard uses. These helpers are
/// that same code behind a plain call: the boxed argument pointer in the integer result
/// register; on return the converted value is in the integer (or float) result register and the
/// status register (`x1` / `rdx`) is 0, or 1 when PHP refuses the value (the caller builds the
/// `TypeError`, which needs the callee name only the invoker's descriptor knows).
pub(in crate::codegen) fn emit_invoker_numeric_param_helpers(ctx: &mut FunctionContext<'_>) {
    if ctx.shared.invoker_numeric_param_helpers {
        return;
    }
    ctx.shared.invoker_numeric_param_helpers = true;
    let scope = ctx.emitter.begin_out_of_line();
    emit_invoker_numeric_param_helper(ctx, Numeric::Int, INVOKER_INT_PARAM_HELPER);
    emit_invoker_numeric_param_helper(ctx, Numeric::Float, INVOKER_FLOAT_PARAM_HELPER);
    ctx.emitter.end_out_of_line(scope);
}

/// The register an invoker numeric helper reports its accept/refuse status in.
pub(in crate::codegen) fn invoker_numeric_status_reg(arch: Arch) -> &'static str {
    match arch {
        Arch::AArch64 => "x1",
        Arch::X86_64 => "rdx",
    }
}

/// Emits one coercive numeric binding helper (see [`emit_invoker_numeric_param_helpers`]).
fn emit_invoker_numeric_param_helper(ctx: &mut FunctionContext<'_>, target: Numeric, label: &str) {
    let refuse = ctx.next_label("invoker_numeric_param_refuse");
    let accept = ctx.next_label("invoker_numeric_param_accept");
    let int_label = ctx.next_label("invoker_numeric_param_int");
    let float_label = ctx.next_label("invoker_numeric_param_float");
    let string_label = ctx.next_label("invoker_numeric_param_string");
    let payload = crate::codegen_support::mixed_unbox_payload_reg(ctx.emitter.target);
    let tag = abi::int_result_reg(ctx.emitter);
    let status = invoker_numeric_status_reg(ctx.emitter.target.arch);
    ctx.emitter.blank();
    ctx.emitter.comment(&format!("invoker {} parameter binding", if target == Numeric::Int { "int" } else { "float" }));
    ctx.emitter.raw(".align 2");
    ctx.emitter.label_global(label);
    abi::emit_frame_prologue(ctx.emitter, 16);
    // -- peel the boxed argument and dispatch on its concrete tag --
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    super::enums::emit_mixed_tag_branch(ctx, tag, 0, &int_label);
    super::enums::emit_mixed_tag_branch(ctx, tag, 3, &int_label);
    super::enums::emit_mixed_tag_branch(ctx, tag, 2, &float_label);
    super::enums::emit_mixed_tag_branch(ctx, tag, 1, &string_label);
    abi::emit_jump(ctx.emitter, &refuse);

    // -- an int or bool payload binds directly (widened for float) --
    ctx.emitter.label(&int_label);
    super::enums::emit_move_reg(ctx, tag, payload);
    if target == Numeric::Float {
        emit_int_result_to_float(ctx);
    }
    abi::emit_jump(ctx.emitter, &accept);

    // -- a float binds to float, or to int after PHP's precision and range rules --
    ctx.emitter.label(&float_label);
    super::mixed_narrowing::emit_float_bits_to_float_result(ctx, payload);
    if target == Numeric::Int {
        super::builtins::strings::emit_float_result_int_coercion_diagnostics(ctx, &refuse);
    }
    abi::emit_jump(ctx.emitter, &accept);

    // -- only a fully numeric string binds --
    ctx.emitter.label(&string_label);
    match target {
        Numeric::Int => emit_string_to_int_parameter(ctx, &refuse),
        Numeric::Float => {
            let (string_ptr, _) = abi::string_result_regs(ctx.emitter);
            if string_ptr != payload {
                ctx.emitter.instruction(&format!("mov {string_ptr}, {payload}")); // move the unboxed string pointer into the numeric-scan input
            }
            abi::emit_call_label(ctx.emitter, "__rt_str_to_number");
            match ctx.emitter.target.arch {
                Arch::AArch64 => {
                    ctx.emitter.instruction(&format!("cbz x0, {refuse}"));      // a string that is not numeric as a whole is refused
                }
                Arch::X86_64 => {
                    ctx.emitter.instruction("test rax, rax");                   // did the numeric scan accept the whole string?
                    ctx.emitter.instruction(&format!("jz {refuse}"));           // a string that is not numeric as a whole is refused
                }
            }
        }
    }

    // -- report the verdict in the status register and return --
    ctx.emitter.label(&accept);
    abi::emit_load_int_immediate(ctx.emitter, status, 0);
    abi::emit_frame_restore(ctx.emitter, 16);
    abi::emit_return(ctx.emitter);
    ctx.emitter.label(&refuse);
    abi::emit_load_int_immediate(ctx.emitter, status, 1);
    abi::emit_frame_restore(ctx.emitter, 16);
    abi::emit_return(ctx.emitter);
}
