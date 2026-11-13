//! Purpose:
//! Runs a compiled EvalIR slice on an explicit frame stack.
//! The same loop serves a program, a tree-walker call into a compiled body, and
//! a compiled call to another compiled body.
//!
//! Called from:
//! - `crate::interpreter::bytecode::try_execute()`.
//! - `crate::interpreter::dynamic_functions` after the walker has bound arguments.
//!
//! Key details:
//! - `ip` is the next opcode and the resume point. No line or exception table is stored.
//! - Value semantics stay in `RuntimeValueOps` and the existing scope helpers.
//! - A fast positional call pushes a frame and returns to this loop. It does not
//!   call the walker binder, so deep recursion stays on this `Vec`.

#[cfg(test)]
use std::cell::Cell;
use std::sync::Arc;

use crate::context::{ElephcEvalContext, EvalFunctionArgsFrame};
use crate::errors::EvalStatus;
use crate::eval_ir::{CompiledBody, CompiledModule, EvalCastType, EvalFunction, Op};
use crate::interpreter::{
    copy_scope_value, eval_binary_result, eval_cast_value, eval_call, eval_expr_language_construct_name,
    eval_inc_dec_value, eval_output_value, finish_eval_argument_values, release_function_args,
    eval_pcntl_maybe_dispatch, eval_release_value, eval_unary_value,
    finish_activation_scope, set_owned_scope_cell, visible_scope_cell, EvalControl, RuntimeValueOps,
    warn_eval_discarded_function_call,
};
use crate::runtime_hooks::owner_frames::with_runtime_owners;
use crate::scope::ElephcEvalScope;
use crate::value::RuntimeCellHandle;

mod activation;
use activation::{call, define_fn, leave_frame, prepare_call, unwind};

#[cfg(test)]
thread_local! {
    /// True when the current thread's latest program entry ran the slice.
    static USED_VM: Cell<bool> = const { Cell::new(false) };
}

/// Frame count and other counters from one slice execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct VmStats {
    /// Deepest number of explicit frames, including the program or bound-body frame.
    pub(crate) max_frames: usize,
}

/// One explicit activation.
///
/// `ip` is the next opcode. A future `yield` or `Fiber::suspend` would park this
/// record. Line numbers and exception regions wait until EvalIR carries spans.
struct Frame {
    body: Arc<CompiledBody>,
    ip: usize,
    stack_base: usize,
    /// `None` borrows the scope the host passed in. `Some` indexes `Vm::scopes`.
    scope_slot: Option<usize>,
    /// True when this frame pushed the function and empty class scopes.
    pushed_activation: bool,
}

/// Operand stack, owned function scopes, and the frames the loop is running.
struct Vm {
    frames: Vec<Frame>,
    scopes: Vec<Box<ElephcEvalScope>>,
    stack: Vec<RuntimeCellHandle>,
    /// Callees selected before evaluating nested argument calls.
    prepared_calls: Vec<Arc<CompiledBody>>,
    max_frames: usize,
    result: Option<EvalControl>,
}

impl Vm {
    /// Creates an empty machine.
    fn new() -> Self {
        Self {
            frames: Vec::new(),
            scopes: Vec::new(),
            stack: Vec::new(),
            prepared_calls: Vec::new(),
            max_frames: 0,
            result: None,
        }
    }

    /// Pushes a frame and records the deepest nesting.
    fn push_frame(&mut self, frame: Frame) {
        self.frames.push(frame);
        self.max_frames = self.max_frames.max(self.frames.len());
    }
}

/// Reports whether the latest program entry on this thread used the slice.
#[cfg(test)]
pub(in crate::interpreter) fn last_execution_used_vm() -> bool {
    USED_VM.with(Cell::get)
}

/// Marks the following tree-walker execution so a previous slice run cannot leak the flag.
pub(super) fn mark_tree_walk() {
    #[cfg(test)]
    USED_VM.with(|used| used.set(false));
}

/// Runs a compiled program in `scope` and reports how deep the explicit stack became.
pub(in crate::interpreter) fn execute_module(
    module: &CompiledModule,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(EvalControl, VmStats), EvalStatus> {
    #[cfg(test)]
    USED_VM.with(|used| used.set(true));
    let mut vm = Vm::new();
    vm.push_frame(Frame {
        body: Arc::clone(&module.main),
        ip: 0,
        stack_base: 0,
        scope_slot: None,
        pushed_activation: false,
    });
    let control = run_until_baseline(&mut vm, 0, context, scope, values)?;
    Ok((control, VmStats { max_frames: vm.max_frames }))
}

/// Runs a compiled body whose caller already bound arguments and pushed the function scope.
pub(in crate::interpreter) fn execute_bound_body(
    body: Arc<CompiledBody>,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalControl, EvalStatus> {
    let mut vm = Vm::new();
    vm.push_frame(Frame {
        body,
        ip: 0,
        stack_base: 0,
        scope_slot: None,
        pushed_activation: false,
    });
    // Nested entry from the walker. Further compiled calls stay on `vm.frames`.
    run_until_baseline(&mut vm, 0, context, scope, values)
}

/// Runs every frame above `baseline`, then returns that activation's control.
///
/// A host callback re-enters here after pushing its frame. `Call` inside the loop
/// only pushes; it does not start another Rust call.
fn run_until_baseline(
    vm: &mut Vm,
    baseline: usize,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalControl, EvalStatus> {
    while vm.frames.len() > baseline {
        if let Err(status) = step(vm, baseline, context, scope, values) {
            unwind(vm, baseline, context, values);
            return Err(status);
        }
    }
    Ok(vm.result.take().unwrap_or(EvalControl::ReturnVoid))
}

/// Executes the current opcode, or the implicit void return past the last instruction.
fn step(
    vm: &mut Vm,
    baseline: usize,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let op = {
        let frame = vm.frames.last_mut().ok_or(EvalStatus::RuntimeFatal)?;
        if frame.ip >= frame.body.ops.len() {
            Op::ReturnVoid
        } else {
            let op = frame.body.ops[frame.ip];
            frame.ip += 1;
            op
        }
    };
    match op {
        Op::StatementBoundary => eval_pcntl_maybe_dispatch(context, values)?,
        Op::Null => vm.stack.push(values.null()?),
        Op::Bool(value) => vm.stack.push(values.bool_value(value)?),
        Op::Int(value) => vm.stack.push(values.int(value)?),
        Op::Float(bits) => vm.stack.push(values.float(f64::from_bits(bits))?),
        Op::String(index) => {
            let text = current_body(vm)?.strings.get(index as usize).cloned().ok_or(EvalStatus::RuntimeFatal)?;
            vm.stack.push(values.string(&text)?);
        }
        Op::Load(slot) => {
            let name = local_name(vm, slot)?;
            let value = load_local(vm, &name, context, scope, values)?;
            vm.stack.push(value);
        }
        Op::Store(slot) => {
            let name = local_name(vm, slot)?;
            let value = pop(&mut vm.stack)?;
            store_local(vm, &name, value, context, scope, values)?;
        }
        Op::Pop => {
            let value = pop(&mut vm.stack)?;
            eval_release_value(context, values, value)?;
        }
        Op::PopDiscardedCall(index) => {
            let body = current_body(vm)?;
            let site = body.calls.get(index as usize).ok_or(EvalStatus::RuntimeFatal)?;
            let value = pop(&mut vm.stack)?;
            let warned = warn_eval_discarded_function_call(&site.name, context, values);
            let released = eval_release_value(context, values, value);
            warned?;
            released?;
        }
        Op::Dup => {
            let top = *vm.stack.last().ok_or(EvalStatus::RuntimeFatal)?;
            vm.stack.push(values.retain(top)?);
        }
        Op::Binary(op) => {
            let right = pop(&mut vm.stack)?;
            let left = pop(&mut vm.stack)?;
            let operands = [left, right];
            let result = with_runtime_owners(&operands.as_slice(), || {
                eval_binary_result(op, left, right, context, values)
            });
            vm.stack.push(finish_eval_argument_values(result, operands, context, values)?);
        }
        Op::Unary(op) => {
            let value = pop(&mut vm.stack)?;
            let result = with_runtime_owners(&value, || eval_unary_value(op, value, values));
            vm.stack.push(finish_eval_argument_values(result, [value], context, values)?);
        }
        Op::CastInt | Op::CastFloat | Op::CastString | Op::CastBool => {
            let value = pop(&mut vm.stack)?;
            let target = match op {
                Op::CastInt => EvalCastType::Int,
                Op::CastFloat => EvalCastType::Float,
                Op::CastString => EvalCastType::String,
                Op::CastBool => EvalCastType::Bool,
                _ => unreachable!("cast opcode"),
            };
            let result = with_runtime_owners(&value, || eval_cast_value(&target, value, context, values));
            vm.stack.push(finish_eval_argument_values(result, [value], context, values)?);
        }
        Op::Jump(target) => set_ip(vm, target)?,
        Op::JumpIfFalse(target) => {
            let condition = pop(&mut vm.stack)?;
            let truthy = match values.truthy(condition) {
                Ok(truthy) => truthy,
                Err(status) => {
                    let _ = eval_release_value(context, values, condition);
                    return Err(status);
                }
            };
            eval_release_value(context, values, condition)?;
            if !truthy {
                set_ip(vm, target)?;
            }
        }
        Op::Echo | Op::Print => {
            let value = pop(&mut vm.stack)?;
            eval_output_value(value, context, values)?;
            if op == Op::Print {
                vm.stack.push(values.int(1)?);
            }
        }
        Op::Return => {
            let value = pop(&mut vm.stack)?;
            leave_frame(vm, Some(value), baseline, context, scope, values)?;
        }
        Op::ReturnVoid => leave_frame(vm, None, baseline, context, scope, values)?,
        Op::PrepareCall(index, end) => prepare_call(vm, index, end, context, scope, values)?,
        Op::Call(argc) => call(vm, argc as usize, context, values)?,
        Op::DefineFn(index) => define_fn(vm, index, context)?,
        Op::IncDec(slot, increment) => {
            let name = local_name(vm, slot)?;
            let current = load_local(vm, &name, context, scope, values)?;
            let updated = with_runtime_owners(&current, || eval_inc_dec_value(current, increment, values));
            let updated = finish_eval_argument_values(updated, [current], context, values)?;
            store_local(vm, &name, updated, context, scope, values)?;
        }
    }
    Ok(())
}

/// Releases every stacked temporary at and above `from`.
fn release_span(
    stack: &mut Vec<RuntimeCellHandle>,
    from: usize,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let mut status = Ok(());
    while stack.len() > from {
        let cell = stack.pop().unwrap();
        if let Err(error) = eval_release_value(context, values, cell) {
            status = Err(error);
        }
    }
    status
}

/// Releases cells and keeps the first release error.
fn release_cells(
    cells: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let mut status = Ok(());
    for cell in cells {
        if let Err(error) = eval_release_value(context, values, *cell) {
            status = Err(error);
        }
    }
    status
}

/// Pops one owned operand.
fn pop(stack: &mut Vec<RuntimeCellHandle>) -> Result<RuntimeCellHandle, EvalStatus> {
    stack.pop().ok_or(EvalStatus::RuntimeFatal)
}

/// Returns the body of the frame currently executing.
fn current_body(vm: &Vm) -> Result<Arc<CompiledBody>, EvalStatus> {
    vm.frames.last().map(|frame| Arc::clone(&frame.body)).ok_or(EvalStatus::RuntimeFatal)
}

/// Returns one local name from the current body.
fn local_name(vm: &Vm, slot: u16) -> Result<String, EvalStatus> {
    current_body(vm)?
        .locals
        .get(slot as usize)
        .cloned()
        .ok_or(EvalStatus::RuntimeFatal)
}

/// Sets the current frame's next instruction.
fn set_ip(vm: &mut Vm, target: u32) -> Result<(), EvalStatus> {
    let frame = vm.frames.last_mut().ok_or(EvalStatus::RuntimeFatal)?;
    frame.ip = target as usize;
    Ok(())
}

/// Copies a visible local, or warns and pushes null when the name is undefined.
fn load_local(
    vm: &mut Vm,
    name: &str,
    context: &mut ElephcEvalContext,
    external: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let slot = vm.frames.last().ok_or(EvalStatus::RuntimeFatal)?.scope_slot;
    let scope = frame_scope(slot, &mut vm.scopes, external);
    if let Some(value) = visible_scope_cell(context, scope, name) {
        copy_scope_value(value, context, values)
    } else {
        values.warning(&format!("Warning: Undefined variable ${name}\n"))?;
        values.null()
    }
}

/// Stores an owned value into a local and releases the cells it replaced.
fn store_local(
    vm: &mut Vm,
    name: &str,
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    external: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let slot = vm.frames.last().ok_or(EvalStatus::RuntimeFatal)?.scope_slot;
    let scope = frame_scope(slot, &mut vm.scopes, external);
    store_into(scope, name, value, context, values)
}

/// Consumes `value` into `scope` and releases every previous owner.
fn store_into(
    scope: &mut ElephcEvalScope,
    name: &str,
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let replaced = set_owned_scope_cell(context, scope, name.to_string(), value, values)?;
    for cell in replaced {
        eval_release_value(context, values, cell)?;
    }
    Ok(())
}

/// Borrows the scope this frame reads and writes.
fn frame_scope<'a>(
    slot: Option<usize>,
    scopes: &'a mut [Box<ElephcEvalScope>],
    external: &'a mut ElephcEvalScope,
) -> &'a mut ElephcEvalScope {
    match slot {
        Some(index) => &mut scopes[index],
        None => external,
    }
}
