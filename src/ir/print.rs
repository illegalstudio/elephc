//! Purpose:
//! Prints deterministic textual EIR for modules and functions.
//!
//! Called from:
//! - Phase 02 tests and future `--emit-ir` diagnostics.
//!
//! Key details:
//! - Printer output is intentionally one-way; there is no textual IR parser in
//!   the v0.24.x implementation track.

use std::fmt::Write;

use crate::ir::block::{SwitchCase, Terminator};
use crate::ir::function::Function;
use crate::ir::instr::{Immediate, Instruction};
use crate::ir::module::{DataPool, Module};
use crate::ir::value::{Ownership, ValueId};

/// Prints a complete module in deterministic textual EIR format.
pub fn print_module(module: &Module) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "module target={} {{", module.target);
    print_data_pool(&mut out, &module.data);
    for function in module
        .functions
        .iter()
        .chain(module.class_methods.iter())
        .chain(module.closures.iter())
        .chain(module.fiber_wrappers.iter())
        .chain(module.callback_wrappers.iter())
        .chain(module.extern_callback_trampolines.iter())
        .chain(module.runtime_callable_invokers.iter())
    {
        print_function_into(&mut out, function, &module.data);
    }
    out.push_str("}\n");
    out
}

/// Prints one function with an empty data pool context.
pub fn print_function(function: &Function) -> String {
    let mut out = String::new();
    print_function_into(&mut out, function, &DataPool::default());
    out
}

/// Prints data-pool literals and symbol names when present.
fn print_data_pool(out: &mut String, data: &DataPool) {
    if data.strings.is_empty()
        && data.float_literals.is_empty()
        && data.global_names.is_empty()
        && data.function_names.is_empty()
        && data.class_names.is_empty()
    {
        return;
    }
    out.push_str("  data {\n");
    for (idx, value) in data.strings.iter().enumerate() {
        let _ = writeln!(out, "    str[{}] = {:?}", idx, value);
    }
    for (idx, value) in data.float_literals.iter().enumerate() {
        let _ = writeln!(out, "    float[{}] = {:?}", idx, value);
    }
    for (idx, value) in data.global_names.iter().enumerate() {
        let _ = writeln!(out, "    global[{}] = {:?}", idx, value);
    }
    for (idx, value) in data.function_names.iter().enumerate() {
        let _ = writeln!(out, "    function[{}] = {:?}", idx, value);
    }
    for (idx, value) in data.class_names.iter().enumerate() {
        let _ = writeln!(out, "    class[{}] = {:?}", idx, value);
    }
    out.push_str("  }\n");
}

/// Prints a function body into an existing module buffer.
fn print_function_into(out: &mut String, function: &Function, data: &DataPool) {
    let _ = write!(out, "\n  function {}(", function.name);
    for (idx, param) in function.params.iter().enumerate() {
        if idx > 0 {
            out.push_str(", ");
        }
        let _ = write!(
            out,
            "{}: {} php={}",
            param.name,
            param.ir_type.as_eir(),
            param.php_type
        );
    }
    let _ = write!(out, ") -> {}", function.return_type.as_eir());
    let flags = function_flags(function);
    if !flags.is_empty() {
        let _ = write!(out, " flags({})", flags.join(", "));
    }
    out.push_str(" {\n");
    for block in &function.blocks {
        let _ = write!(out, "    {}", block.name);
        if !block.params.is_empty() {
            out.push('(');
            for (idx, param) in block.params.iter().enumerate() {
                if idx > 0 {
                    out.push_str(", ");
                }
                print_value_signature(out, function, *param);
            }
            out.push(')');
        }
        out.push_str(":\n");
        for inst_id in &block.instructions {
            let inst = &function.instructions[inst_id.as_raw() as usize];
            print_instruction(out, function, data, inst);
        }
        out.push_str("      ");
        match &block.terminator {
            Some(term) => print_terminator(out, term),
            None => out.push_str("<missing terminator>"),
        }
        out.push('\n');
    }
    out.push_str("  }\n");
}

/// Prints one instruction line.
fn print_instruction(out: &mut String, function: &Function, data: &DataPool, inst: &Instruction) {
    out.push_str("      ");
    if let Some(result) = inst.result {
        print_value_signature(out, function, result);
        out.push_str(" = ");
    }
    out.push_str(inst.op.name());
    for operand in &inst.operands {
        let _ = write!(out, " v{}", operand.as_raw());
    }
    if let Some(immediate) = &inst.immediate {
        print_immediate(out, data, immediate);
    }
    if !inst.effects.is_empty() {
        let _ = write!(out, " ; effects: {}", inst.effects.names().join(", "));
    }
    if let Some(span) = inst.span {
        let _ = write!(out, " ; span: {}:{}", span.line, span.col);
    }
    if let Some(origin) = inst.origin {
        let _ = write!(out, " ; origin: {}", origin.name());
    }
    out.push('\n');
}

/// Prints a value's textual signature.
fn print_value_signature(out: &mut String, function: &Function, value: ValueId) {
    let value_ref = &function.values[value.as_raw() as usize];
    let _ = write!(
        out,
        "v{}: {} php={}",
        value.as_raw(),
        value_ref.ir_type.as_eir(),
        value_ref.php_type
    );
    if value_ref.ownership != Ownership::NonHeap {
        let _ = write!(out, " own={}", value_ref.ownership.as_eir());
    }
}

/// Prints an immediate operand in stable textual form.
fn print_immediate(out: &mut String, data: &DataPool, immediate: &Immediate) {
    match immediate {
        Immediate::I64(value) => {
            let _ = write!(out, " {}", value);
        }
        Immediate::F64(value) => {
            let _ = write!(out, " {}", value);
        }
        Immediate::Bool(value) => {
            let _ = write!(out, " {}", if *value { "true" } else { "false" });
        }
        Immediate::Data(id) => {
            let _ = write!(out, " data[{}]", id.as_raw());
        }
        Immediate::LocalSlot(id) => {
            let _ = write!(out, " slot[{}]", id.as_raw());
        }
        Immediate::LocalSlotPair { first, second } => {
            let _ = write!(out, " slots[{},{}]", first.as_raw(), second.as_raw());
        }
        Immediate::GlobalName(id) => {
            let value = data
                .global_names
                .get(id.as_raw() as usize)
                .map(String::as_str)
                .unwrap_or("<unknown>");
            let _ = write!(out, " global[{}:{:?}]", id.as_raw(), value);
        }
        Immediate::FunctionRef(id) => {
            let _ = write!(out, " function#{}", id.as_raw());
        }
        Immediate::BuiltinRef(id) => {
            let _ = write!(out, " builtin#{}", id.0);
        }
        Immediate::RuntimeRef(id) => {
            let _ = write!(out, " runtime#{}", id.0);
        }
        Immediate::RuntimeCall(target) => {
            let _ = write!(out, " runtime.{}", target.as_eir());
        }
        Immediate::ExternRef(id) => {
            let _ = write!(out, " extern#{}", id);
        }
        Immediate::ClassRef(id) => {
            let _ = write!(out, " class#{}", id);
        }
        Immediate::EnumCaseRef { enum_id, case_id } => {
            let _ = write!(out, " enum#{}::case#{}", enum_id, case_id);
        }
        Immediate::MethodRef { class, method } => {
            let _ = write!(out, " method#{}::{}", class, method);
        }
        Immediate::PropertyRef { class, property } => {
            let _ = write!(out, " property#{}::{}", class, property);
        }
        Immediate::FieldRef { layout, field } => {
            let _ = write!(out, " field#{}::{}", layout, field);
        }
        Immediate::FunctionVariantRef { group, variant } => {
            let _ = write!(out, " variant#{}::{}", group, variant);
        }
        Immediate::HeapKind(kind) => {
            let _ = write!(out, " {}", kind.as_eir());
        }
        Immediate::MixedTag(tag) => {
            let _ = write!(out, " tag#{}", tag);
        }
        Immediate::TypePredicate(predicate) => {
            let _ = write!(out, " {}", predicate.as_eir());
        }
        Immediate::MixedNumericOp(op) => {
            let _ = write!(out, " {}", op.as_eir());
        }
        Immediate::CmpPredicate(predicate) => {
            let _ = write!(out, " {:?}", predicate);
        }
        Immediate::CastTarget(target) => {
            let _ = write!(out, " {}", target.as_eir());
        }
        Immediate::TypeName(id) => {
            let _ = write!(out, " type_name[{}]", id.as_raw());
        }
        Immediate::Capacity(capacity) => {
            let _ = write!(out, " capacity={}", capacity);
        }
        Immediate::WidthBytes(width) => {
            let _ = write!(out, " width={}", width);
        }
    }
}

/// Prints one terminator line.
fn print_terminator(out: &mut String, term: &Terminator) {
    match term {
        Terminator::Br { target, args } => {
            let _ = write!(out, "br bb{}", target.as_raw());
            print_args(out, args);
        }
        Terminator::CondBr {
            cond,
            then_target,
            then_args,
            else_target,
            else_args,
        } => {
            let _ = write!(out, "cond_br v{}, bb{}", cond.as_raw(), then_target.as_raw());
            print_args(out, then_args);
            let _ = write!(out, ", bb{}", else_target.as_raw());
            print_args(out, else_args);
        }
        Terminator::Switch {
            scrutinee,
            cases,
            default,
            default_args,
        } => {
            let _ = write!(out, "switch v{} [", scrutinee.as_raw());
            for (idx, case) in cases.iter().enumerate() {
                if idx > 0 {
                    out.push_str(", ");
                }
                print_switch_case(out, case);
            }
            if !cases.is_empty() {
                out.push_str(", ");
            }
            let _ = write!(out, "default => bb{}", default.as_raw());
            print_args(out, default_args);
            out.push(']');
        }
        Terminator::Return { value: Some(value) } => {
            let _ = write!(out, "return v{}", value.as_raw());
        }
        Terminator::Return { value: None } => out.push_str("return"),
        Terminator::Throw { value } => {
            let _ = write!(out, "throw v{}", value.as_raw());
        }
        Terminator::Fatal { message } => {
            let _ = write!(out, "fatal data[{}]", message.as_raw());
        }
        Terminator::GeneratorSuspend {
            key,
            value,
            resume,
            resume_args,
        } => {
            out.push_str("generator_suspend");
            if let Some(key) = key {
                let _ = write!(out, " key=v{}", key.as_raw());
            }
            if let Some(value) = value {
                let _ = write!(out, " value=v{}", value.as_raw());
            }
            let _ = write!(out, " resume=bb{}", resume.as_raw());
            print_args(out, resume_args);
        }
        Terminator::Unreachable => out.push_str("unreachable"),
    }
}

/// Prints one switch case edge.
fn print_switch_case(out: &mut String, case: &SwitchCase) {
    let _ = write!(out, "{} => bb{}", case.value, case.target.as_raw());
    print_args(out, &case.args);
}

/// Prints a parenthesized argument list when non-empty.
fn print_args(out: &mut String, args: &[ValueId]) {
    if args.is_empty() {
        return;
    }
    out.push('(');
    for (idx, arg) in args.iter().enumerate() {
        if idx > 0 {
            out.push_str(", ");
        }
        let _ = write!(out, "v{}", arg.as_raw());
    }
    out.push(')');
}

/// Returns deterministic textual flag names for a function.
fn function_flags(function: &Function) -> Vec<&'static str> {
    let mut flags = Vec::new();
    if function.flags.is_main {
        flags.push("main");
    }
    if function.flags.is_method {
        flags.push("method");
    }
    if function.flags.is_closure {
        flags.push("closure");
    }
    if function.flags.is_generator {
        flags.push("generator");
    }
    if function.flags.is_fiber_wrapper {
        flags.push("fiber_wrapper");
    }
    if function.flags.is_callback_wrapper {
        flags.push("callback_wrapper");
    }
    if function.flags.is_runtime_callable_invoker {
        flags.push("runtime_callable_invoker");
    }
    if function.flags.is_static {
        flags.push("static");
    }
    flags
}
