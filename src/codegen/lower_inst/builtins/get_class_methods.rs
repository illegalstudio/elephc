//! Purpose:
//! Lowers `get_class_methods(object|string)` to the per-class method rows and the calling
//! scope's visibility filter.
//!
//! Called from:
//! - `crate::codegen::lower_inst::runtime_functions` for `RuntimeFnId::GetClassMethods`.
//!
//! Key details:
//! - The class id comes from the object header, or from `__rt_class_id_by_name` for a
//!   class-name string or a boxed value; the calling scope is the function's lexical class,
//!   known here, so one runtime helper serves every call site.
//! - The result is a boxed `Mixed` array. A class the compiled world does not know (declared by
//!   `eval` or a runtime include) goes to the eval bridge when it is linked, which answers with
//!   a Mixed cell too; without the bridge it is php's `TypeError`.

use crate::codegen::platform::Arch;
use crate::codegen::{abi, emit_box_current_owned_value_as_mixed};
use crate::codegen::Result;
use crate::ir::Instruction;
use crate::types::PhpType;

use super::super::super::context::FunctionContext;
use super::{expect_operand, store_if_result};

/// Lowers one `get_class_methods()` call.
pub(crate) fn lower_get_class_methods(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "get_class_methods", 1)?;
    let target = expect_operand(inst, 0)?;
    let scope_class_id = super::types::lexical_object_vars_scope_class_id(ctx);
    let miss = ctx.next_label("get_class_methods_miss");
    let done = ctx.next_label("get_class_methods_done");

    crate::codegen::lower_inst::objects::emit_class_id_of_value(ctx, target)?;
    // An object the interpreter built carries the generic `stdClass` header, whose rows list
    // nothing the interpreted class declares: Twig's compiled `getAttribute()` found no getter on
    // an eval-declared entity. With the bridge linked that id is asked of the bridge, which also
    // answers a real `stdClass` (no methods).
    let carrier_class_id = ctx
        .module
        .required_runtime_features
        .eval_bridge
        .then(|| ctx.module.class_infos.get("stdClass").map(|info| info.class_id))
        .flatten();
    if let Some(carrier) = carrier_class_id {
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                abi::emit_load_int_immediate(ctx.emitter, "x9", carrier as i64);
                ctx.emitter.instruction("cmp x0, x9");                          // is this the interpreter's object carrier?
                ctx.emitter.instruction(&format!("b.eq {}", miss));             // its methods live in the interpreter
            }
            Arch::X86_64 => {
                abi::emit_load_int_immediate(ctx.emitter, "r10", carrier as i64);
                ctx.emitter.instruction("cmp rax, r10");                        // is this the interpreter's object carrier?
                ctx.emitter.instruction(&format!("je {}", miss));               // its methods live in the interpreter
            }
        }
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #0");                              // a negative id names no compiled class
            ctx.emitter.instruction(&format!("b.lt {}", miss));                 // hand unknown classes to the fallback
            abi::emit_symbol_address(ctx.emitter, "x9", "_class_method_rows_count");
            ctx.emitter.instruction("ldr x9, [x9]");                            // number of class ids the rows cover
            ctx.emitter.instruction("cmp x0, x9");                              // is the class id inside the row table?
            ctx.emitter.instruction(&format!("b.hs {}", miss));                 // an id past the table has no rows
            abi::emit_load_int_immediate(ctx.emitter, "x1", scope_class_id);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // a negative id names no compiled class
            ctx.emitter.instruction(&format!("js {}", miss));                   // hand unknown classes to the fallback
            abi::emit_symbol_address(ctx.emitter, "r10", "_class_method_rows_count");
            ctx.emitter.instruction("cmp rax, QWORD PTR [r10]");                // is the class id inside the row table?
            ctx.emitter.instruction(&format!("jae {}", miss));                  // an id past the table has no rows
            ctx.emitter.instruction("mov rdi, rax");                            // class id is the helper's first argument
            abi::emit_load_int_immediate(ctx.emitter, "rsi", scope_class_id);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_class_method_names");
    // The helper hands back the array it just built, an owner nothing else holds: the box adopts
    // it. Boxing with a retain left one extra reference, so every call stranded the array and its
    // name strings.
    emit_box_current_owned_value_as_mixed(ctx.emitter, &PhpType::Array(Box::new(PhpType::Str)));
    store_if_result(ctx, inst)?;
    abi::emit_jump(ctx.emitter, &done);

    ctx.emitter.label(&miss);
    if ctx.module.required_runtime_features.eval_bridge {
        super::eval::lower_eval_function_call_named(ctx, inst, "get_class_methods")?;
    } else {
        super::super::exceptions::emit_type_error(
            ctx,
            "get_class_methods(): Argument #1 ($object_or_class) must be an object or a valid class name, string given",
        );
    }
    ctx.emitter.label(&done);
    Ok(())
}
