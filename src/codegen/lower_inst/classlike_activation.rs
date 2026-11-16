//! Purpose:
//! Lowers explicit compiled interface activation events into request-local cells.
//!
//! Called from:
//! - `crate::codegen::lower_inst::lower_instruction()` for `Op::ClassLikeActivate`.
//!
//! Key details:
//! - A second activation of the same interface is php's compile-time fatal
//!   `Cannot redeclare interface X (previously declared in F:L)`: it ends the program and no
//!   `catch` sees it, so it is not lowered as a throwable.

use super::*;

/// Marks a compiled interface as declared for this request, or stops the program with php's
/// redeclaration fatal when an earlier activation already declared it.
///
/// A source the autoload pass spliced in only raises the cell (see the body comment). The fatal
/// names the declaration's file and line, which is both the "previously declared" location and
/// the location of the failing declaration, as php reports it.
pub(super) fn lower_classlike_activate(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let Some(Immediate::ClassLikeActivation(activation)) = &inst.immediate else {
        return Err(CodegenIrError::invalid_module(
            "class_like_activate requires typed declaration identity",
        ));
    };
    let (source, kind, name) = (&activation.source, &activation.kind, &activation.name);
    if *kind != crate::parser::ast::ClassLikeKind::Interface {
        return Err(CodegenIrError::unsupported(format!(
            "class-like activation for {:?}", kind
        )));
    }
    let name = ctx
        .module
        .data
        .strings
        .get(name.as_raw() as usize)
        .ok_or_else(|| CodegenIrError::missing_entry("data string", name.as_raw()))?;
    let cell = crate::names::classlike_activation_symbol(*kind, name);
    ctx.data.add_comm(cell.clone(), 8);
    let unit = ctx.module.source_catalog().and_then(|catalog| catalog.get(*source));
    // A source the autoload pass spliced in stands for the AUTOLOADER's inclusion, which php
    // performs at most once and only while the symbol is missing. When something dynamic asked
    // first, the interpreter already raised this cell as it skipped that include
    // (`__elephc_eval_source_activate`), so finding it raised here is that same single load.
    let autoloaded =
        unit.is_some_and(|unit| ctx.module.preincluded_sources.contains(&unit.canonical_path));
    if autoloaded {
        let value = abi::temp_int_reg(ctx.emitter.target);
        abi::emit_load_int_immediate(ctx.emitter, value, 1);
        abi::emit_store_reg_to_symbol(ctx.emitter, value, &cell, 0);
        return Ok(());
    }
    let location = unit.map(|unit| {
        let line = inst.span.map_or(0, |span| span.line);
        (unit.canonical_path.display().to_string(), line)
    });
    let message = match location {
        Some((file, line)) => format!(
            "Fatal error: Cannot redeclare interface {name} (previously declared in {file}:{line}) in {file} on line {line}\n"
        ),
        None => format!("Fatal error: Cannot redeclare interface {name}\n"),
    };

    let active = ctx.next_label("classlike_activate_already_active");
    let done = ctx.next_label("classlike_activate_done");
    let value = abi::temp_int_reg(ctx.emitter.target);
    abi::emit_load_symbol_to_reg(ctx.emitter, value, &cell, 0);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbnz {}, {}", value, active));    // an already-raised cell means this is a redeclaration
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("test {}, {}", value, value));     // has an earlier activation raised the cell?
            ctx.emitter.instruction(&format!("jne {}", active));                // an already-raised cell means this is a redeclaration
        }
    }
    abi::emit_load_int_immediate(ctx.emitter, value, 1);
    abi::emit_store_reg_to_symbol(ctx.emitter, value, &cell, 0);
    abi::emit_jump(ctx.emitter, &done);

    ctx.emitter.label(&active);
    super::objects::emit_fatal_message(ctx, message.as_bytes());
    ctx.emitter.label(&done);
    Ok(())
}
