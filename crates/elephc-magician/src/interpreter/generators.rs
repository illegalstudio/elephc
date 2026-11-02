//! Purpose:
//! Implements PHP generators for interpreted code: the `Generator` object a function containing
//! `yield` returns, its resumption protocol, and `yield from` delegation.
//!
//! Called from:
//! - `crate::interpreter::dynamic_functions` when a called body contains a yield.
//! - `crate::interpreter::statements::method_dispatch` for the Generator methods.
//! - `crate::interpreter::statements::loop_statements` for `foreach` over a generator.
//!
//! Key details:
//! - ARCHITECTURE: a per-generator LINEAR STEP LIST (see `lowering`) plus a frame holding a
//!   program counter, the generator's own scope and its iteration state. Resuming is then just
//!   continuing at the stored index. A resumable tree walker was rejected because it must reify
//!   every loop's position anyway and then re-descend the tree on every resume; OS threads were
//!   rejected because the runtime handles and scope pointers are not `Send` and a generator
//!   outlives the scope that created it.
//! - Only constructs CONTAINING a yield are flattened. Everything else stays a statement subtree
//!   run atomically by the ordinary evaluator, so this module owns control flow and nothing else.
//! - PHP does not run a generator's body until it is first asked for a value, so creation is
//!   just building the frame.

mod lowering;

use super::*;
use crate::context::{
    EvalCallSite, EvalGeneratorActivation, EvalGeneratorDelegate, EvalGeneratorFrame, EvalGeneratorMagicScope,
    EvalGeneratorRegion, EvalGeneratorState, EvalGeneratorStep,
};

pub(in crate::interpreter) use lowering::eval_body_is_generator;
use lowering::lower_generator_body;

/// Builds the `Generator` object that one call to a generator function returns.
pub(in crate::interpreter) fn eval_generator_new(
    body: &[EvalStmt],
    scope: ElephcEvalScope,
    activation: EvalGeneratorActivation,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let program = lower_generator_body(body)?;
    let foreach_slots = vec![None; program.foreach_slots];
    let object = values.new_object("Generator")?;
    let identity = values.object_identity(object)?;
    context.register_eval_generator(
        identity,
        EvalGeneratorFrame {
            program,
            activation,
            step: 0,
            scope: Box::new(scope),
            state: EvalGeneratorState::NotStarted,
            auto_key: 0,
            current_key: None,
            current_value: None,
            return_value: None,
            foreach_slots,
            regions: Vec::new(),
            delegate: None,
            pending_send_slot: None,
            advanced: false,
        },
    );
    Ok(object)
}

/// Builds the activation of a generator produced by a plain function.
///
/// A plain function pushes no class scope and no magic frame, so neither does its generator.
pub(in crate::interpreter) fn eval_plain_function_activation(
    function_name: &str,
) -> EvalGeneratorActivation {
    EvalGeneratorActivation {
        function_name: function_name.to_string(),
        class_scope: None,
        called_class: None,
        magic: None,
        site: None,
    }
}

/// Builds the activation of a generator produced by a closure.
///
/// A bound closure pushes the class scopes but still no magic frame, exactly as the ordinary
/// closure call path does.
pub(in crate::interpreter) fn eval_closure_activation(
    function_name: &str,
    class_scope: Option<String>,
    called_class: Option<String>,
    site: Option<EvalCallSite>,
) -> EvalGeneratorActivation {
    EvalGeneratorActivation {
        function_name: function_name.to_string(),
        class_scope,
        called_class,
        magic: None,
        site,
    }
}

/// Builds the activation of a generator produced by a class method.
///
/// This is the case the activation exists for: the resumer's class scope must not leak into the
/// body, so the declaring class, the late-static-bound class and the magic frame the ordinary
/// method path pushes are all recorded and re-pushed on every resumption.
pub(in crate::interpreter) fn eval_method_activation(
    qualified_method_name: String,
    class_name: &str,
    called_class_name: &str,
    method: &EvalClassMethod,
    site: Option<EvalCallSite>,
) -> EvalGeneratorActivation {
    EvalGeneratorActivation {
        function_name: qualified_method_name,
        class_scope: Some(class_name.to_string()),
        called_class: Some(called_class_name.to_string()),
        magic: Some(EvalGeneratorMagicScope {
            function_name: method.magic_function_name().to_string(),
            method_name: method.magic_method_name(class_name),
            class_name: Some(class_name.trim_start_matches('\\').to_string()),
            trait_name: method
                .trait_origin()
                .map(|trait_name| trait_name.trim_start_matches('\\').to_string()),
        }),
        site,
    }
}

/// Runs a destroyed generator's pending `finally` blocks and drops its frame.
///
/// PHP runs the `finally` of every `try` a suspended generator is still inside when the object
/// is destroyed, innermost first — abandoning a `foreach` with `break` is the ordinary way this
/// happens. A generator that never started is inside nothing and runs nothing, which is also
/// PHP's answer.
pub(in crate::interpreter) fn eval_generator_finalize(
    identity: u64,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let Some(mut frame) = context.take_eval_generator(identity) else {
        return Ok(());
    };
    let unwind = if frame.state == EvalGeneratorState::Suspended {
        let resumer_site = eval_generator_enter_activation(&frame, context);
        let result = eval_generator_unwind_regions(&mut frame, context, values);
        eval_generator_leave_activation(&mut frame, resumer_site, context);
        result
    } else {
        Ok(())
    };
    let release = eval_generator_release_frame(&mut frame, values);
    unwind.and(release)
}

/// Runs each open region's destruction-time `finally`, innermost first.
fn eval_generator_unwind_regions(
    frame: &mut EvalGeneratorFrame,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    frame.state = EvalGeneratorState::Finished;
    let mut result = Ok(());
    while let Some(region) = frame.regions.pop() {
        frame.step = region.finally_entry;
        let outcome = eval_generator_run_unwind_block(frame, context, values);
        if result.is_ok() {
            result = outcome;
        }
    }
    result
}

/// Executes one destruction-time `finally` copy up to its `EndUnwind`.
fn eval_generator_run_unwind_block(
    frame: &mut EvalGeneratorFrame,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    loop {
        let Some(step) = frame.program.steps.get(frame.step).cloned() else {
            return Ok(());
        };
        frame.step += 1;
        if matches!(step, EvalGeneratorStep::EndUnwind) {
            return Ok(());
        }
        // PHP refuses to suspend a generator that is being force-closed, because there is no
        // longer anyone to resume it: `Cannot yield from finally in a force-closed generator`.
        if matches!(
            step,
            EvalGeneratorStep::Yield { .. } | EvalGeneratorStep::YieldFrom { .. }
        ) {
            return eval_throw_error_message(
                "Cannot yield from finally in a force-closed generator",
                context,
                values,
            );
        }
        match eval_generator_execute_step(frame, step, context, values)? {
            EvalGeneratorFlow::Continue => {}
            EvalGeneratorFlow::Suspended | EvalGeneratorFlow::Finished => return Ok(()),
        }
    }
}

/// Releases every runtime value a finished generator's frame still holds.
fn eval_generator_release_frame(
    frame: &mut EvalGeneratorFrame,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let mut result = eval_generator_clear_current(frame, values);
    if let Some(value) = frame.return_value.take() {
        let released = values.release(value);
        if result.is_ok() {
            result = released;
        }
    }
    for slot in frame.foreach_slots.iter_mut() {
        if let Some((array, _)) = slot.take() {
            let released = values.release(array);
            if result.is_ok() {
                result = released;
            }
        }
    }
    for value in frame.scope.drain_owned_cells() {
        let released = values.release(value);
        if result.is_ok() {
            result = released;
        }
    }
    result
}

/// Runs the generator up to its first yield when it has not started yet.
fn eval_generator_prime(
    identity: u64,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let not_started = context
        .eval_generator(identity, |frame| frame.state == EvalGeneratorState::NotStarted)
        .unwrap_or(false);
    if not_started {
        eval_generator_step(identity, None, false, context, values)?;
    }
    Ok(())
}

/// Advances one generator, optionally delivering a value to the yield it is suspended on.
///
/// The frame is taken out of the context for the duration, because the body it runs needs the
/// context mutably and the frame lives inside it.
fn eval_generator_step(
    identity: u64,
    sent: Option<RuntimeCellHandle>,
    resuming: bool,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let Some(mut frame) = context.take_eval_generator(identity) else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let resumer_site = eval_generator_enter_activation(&frame, context);
    let result = eval_generator_run(&mut frame, sent, resuming, context, values);
    eval_generator_leave_activation(&mut frame, resumer_site, context);
    context.restore_eval_generator(identity, frame);
    result
}

/// Re-establishes the generator's own naming scope for the duration of one resumption.
///
/// The caller that resumes a generator is arbitrary, so the body must not inherit the resumer's
/// class scope: `self::`, `static::`, `__CLASS__` and private-member access inside a generator
/// method all read these stacks.
///
/// Returns the resumer's source site when the body has one of its own installed, for
/// `eval_generator_leave_activation` to put back.
fn eval_generator_enter_activation(
    frame: &EvalGeneratorFrame,
    context: &mut ElephcEvalContext,
) -> Option<EvalCallSite> {
    let activation = &frame.activation;
    context.push_function(activation.function_name.clone());
    if let Some(class_scope) = &activation.class_scope {
        context.push_class_scope(class_scope.clone());
    }
    if let Some(called_class) = &activation.called_class {
        context.push_called_class_scope(called_class.clone());
    }
    if let Some(magic) = &activation.magic {
        context.push_callable_magic_scope(
            &magic.function_name,
            &magic.method_name,
            magic.class_name.as_deref(),
            magic.trait_name.as_deref(),
        );
    }
    // The generator's own frame keeps the RESUMER's position, as php does: the call that started
    // the body is the `file:line` the frame reports, so it is built before the body's site goes in.
    let call_frame = eval_generator_call_frame(frame, context);
    context.push_call_frame(call_frame);
    let (file, dir, line, file_magic) = activation.site.as_ref()?.clone();
    let resumer_site = context.call_site();
    context.set_call_site(file, dir, line);
    context.set_file_magic_override(file_magic);
    Some(resumer_site)
}

/// The `debug_backtrace()` frame of a running generator body.
///
/// php lists the generator's function while its body runs -- a method with its object, so
/// `debug_backtrace(DEBUG_BACKTRACE_PROVIDE_OBJECT)` can find the instance that is yielding.
/// Twig 3 renders every template through generator methods (`doDisplay()`, `block_*()`), and
/// Symfony UX's `DeterministicTwigIdCalculator` walks the backtrace for the template object: with
/// no frame for the resumed body it found none and every page rendering a live component failed
/// with "Could not determine template while generating deterministic id".
fn eval_generator_call_frame(frame: &EvalGeneratorFrame, context: &ElephcEvalContext) -> EvalCallFrame {
    let activation = &frame.activation;
    match &activation.magic {
        Some(magic) => {
            let class_name = magic
                .class_name
                .clone()
                .or_else(|| activation.class_scope.clone())
                .unwrap_or_default();
            EvalCallFrame::method(
                &class_name,
                magic.function_name.clone(),
                frame.scope.visible_cell("this"),
                None,
                context,
            )
        }
        None => EvalCallFrame::function(activation.function_name.clone(), None, context),
    }
}

/// Undoes `eval_generator_enter_activation` in the reverse order.
///
/// A body that installed its own source site records the line it reached -- where a suspended
/// generator's next resumption continues -- before the resumer's site is put back.
fn eval_generator_leave_activation(
    frame: &mut EvalGeneratorFrame,
    resumer_site: Option<EvalCallSite>,
    context: &mut ElephcEvalContext,
) {
    if let Some((file, dir, line, file_magic)) = resumer_site {
        if let Some(site) = frame.activation.site.as_mut() {
            site.2 = context.call_line();
        }
        context.set_call_site(file, dir, line);
        context.set_file_magic_override(file_magic);
    }
    let activation = &frame.activation;
    context.pop_call_frame();
    if activation.magic.is_some() {
        context.pop_magic_scope();
    }
    if activation.called_class.is_some() {
        context.pop_called_class_scope();
    }
    if activation.class_scope.is_some() {
        context.pop_class_scope();
    }
    context.pop_function();
}

/// Executes steps until the generator suspends on a yield or finishes.
fn eval_generator_run(
    frame: &mut EvalGeneratorFrame,
    sent: Option<RuntimeCellHandle>,
    resuming: bool,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if frame.state == EvalGeneratorState::Finished {
        return Ok(());
    }
    if resuming {
        eval_generator_deliver_sent(frame, sent, values)?;
        if frame.delegate.is_some() {
            eval_generator_advance_delegate(frame, sent, context, values)?;
            if eval_generator_produce_from_delegate(frame, context, values)? {
                return Ok(());
            }
            eval_generator_end_delegation(frame, context, values)?;
        }
    }
    frame.state = EvalGeneratorState::Suspended;
    loop {
        let Some(step) = frame.program.steps.get(frame.step).cloned() else {
            return eval_generator_finish(frame, None, values);
        };
        frame.step += 1;
        match eval_generator_execute_step(frame, step, context, values) {
            Ok(EvalGeneratorFlow::Continue) => {}
            Ok(EvalGeneratorFlow::Suspended) | Ok(EvalGeneratorFlow::Finished) => return Ok(()),
            Err(EvalStatus::UncaughtThrowable) => {
                // A `try` region the body is inside absorbs the throw and continues at its
                // handler; with none open the generator ends and the throw carries on outward.
                let Some(thrown) = context.take_pending_throw() else {
                    frame.state = EvalGeneratorState::Finished;
                    return Err(EvalStatus::UncaughtThrowable);
                };
                let Some(region) = frame.regions.pop() else {
                    frame.state = EvalGeneratorState::Finished;
                    context.set_pending_throw(thrown);
                    return Err(EvalStatus::UncaughtThrowable);
                };
                if let Some(replaced) =
                    frame
                        .scope
                        .set(&region.thrown_slot, thrown, ScopeCellOwnership::Owned)
                {
                    values.release(replaced)?;
                }
                frame.step = region.handler;
            }
            Err(status) => {
                frame.state = EvalGeneratorState::Finished;
                return Err(status);
            }
        }
    }
}

/// What running one step told the loop to do next.
enum EvalGeneratorFlow {
    /// Run the next step.
    Continue,
    /// A yield produced a value: hand control back to whoever resumed the generator.
    Suspended,
    /// The body ran to its end or returned.
    Finished,
}

/// Runs one lowered step.
///
/// Every throw leaves here as `Err(UncaughtThrowable)` with the value parked on the context, so
/// the caller has exactly one place to consult the open `try` regions — whether the throw came
/// from a statement chunk, a yielded expression or a condition.
fn eval_generator_execute_step(
    frame: &mut EvalGeneratorFrame,
    step: EvalGeneratorStep,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalGeneratorFlow, EvalStatus> {
    Ok(match step {
        EvalGeneratorStep::Run(body) => {
            match execute_statements(&body, context, &mut frame.scope, values)? {
                EvalControl::None => EvalGeneratorFlow::Continue,
                EvalControl::Return(value) => {
                    eval_generator_finish(frame, Some(value), values)?;
                    EvalGeneratorFlow::Finished
                }
                EvalControl::ReturnVoid => {
                    eval_generator_finish(frame, None, values)?;
                    EvalGeneratorFlow::Finished
                }
                EvalControl::Throw(value) => {
                    context.set_pending_throw(value);
                    return Err(EvalStatus::UncaughtThrowable);
                }
                // A break or continue reaching here escaped the chunk it was batched into,
                // which the lowering prevents by lowering such a statement itself.
                _ => return Err(EvalStatus::UnsupportedConstruct),
            }
        }
        EvalGeneratorStep::EnterTry {
            handler,
            finally_entry,
            thrown_slot,
        } => {
            frame.regions.push(EvalGeneratorRegion {
                handler,
                finally_entry,
                thrown_slot,
            });
            EvalGeneratorFlow::Continue
        }
        EvalGeneratorStep::LeaveTry => {
            frame.regions.pop();
            EvalGeneratorFlow::Continue
        }
        EvalGeneratorStep::Rethrow { thrown_slot } => {
            let Some(thrown) = frame.scope.unset(thrown_slot) else {
                return Err(EvalStatus::RuntimeFatal);
            };
            context.set_pending_throw(thrown);
            return Err(EvalStatus::UncaughtThrowable);
        }
        step => return eval_generator_execute_value_step(frame, step, context, values),
    })
}

/// Evaluates one yielded expression as a value the FRAME may keep.
///
/// `eval_expr` hands back a BORROWED cell for a variable read, and the frame's current pair is
/// given back by `eval_generator_clear_current`, so storing the borrow releases a cell the
/// generator's own scope still holds — `yield $i` inside a loop gave back `$i` on every pass.
/// This is the rule assignment already applies with `copy_value`.
fn eval_generator_kept_expr(
    expr: &EvalExpr,
    frame: &mut EvalGeneratorFrame,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let value = eval_expr(expr, context, &mut frame.scope, values)?;
    // Mirrors `eval_assign`'s copy rule: any expression whose result aliases persistent
    // storage -- a variable read, but also a class constant or static property fetch -- needs
    // an independent copy before the frame keeps it, or a later step releases storage this
    // yield never owned.
    if eval_expr_result_aliases_storage(expr) {
        return values.copy_value(value);
    }
    Ok(value)
}

/// Runs the steps that evaluate expressions or move the iteration on.
fn eval_generator_execute_value_step(
    frame: &mut EvalGeneratorFrame,
    step: EvalGeneratorStep,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalGeneratorFlow, EvalStatus> {
    Ok(match step {
        EvalGeneratorStep::Yield { key, value, into } => {
            let value = eval_generator_kept_expr(&value, frame, context, values)?;
            let key = match key {
                Some(key) => eval_generator_kept_expr(&key, frame, context, values)?,
                None => {
                    let key = values.int(frame.auto_key)?;
                    frame.auto_key += 1;
                    key
                }
            };
            eval_generator_set_current(frame, key, value, values)?;
            frame.pending_send_slot = into;
            EvalGeneratorFlow::Suspended
        }
        EvalGeneratorStep::YieldFrom { source, into } => {
            let source = eval_expr(&source, context, &mut frame.scope, values)?;
            frame.pending_send_slot = into;
            eval_generator_begin_delegation(frame, source, context, values)?;
            if eval_generator_produce_from_delegate(frame, context, values)? {
                return Ok(EvalGeneratorFlow::Suspended);
            }
            eval_generator_end_delegation(frame, context, values)?;
            EvalGeneratorFlow::Continue
        }
        EvalGeneratorStep::JumpIfFalse { condition, target } => {
            let condition = eval_expr(&condition, context, &mut frame.scope, values)?;
            if !values.truthy(condition)? {
                frame.step = target;
            }
            EvalGeneratorFlow::Continue
        }
        EvalGeneratorStep::Jump(target) => {
            frame.step = target;
            EvalGeneratorFlow::Continue
        }
        EvalGeneratorStep::Return(value) => {
            let value = match value {
                Some(expr) => Some(eval_expr(&expr, context, &mut frame.scope, values)?),
                None => None,
            };
            eval_generator_finish(frame, value, values)?;
            EvalGeneratorFlow::Finished
        }
        EvalGeneratorStep::ForeachInit { subject, slot } => {
            // The slot OWNS what it iterates: `eval_generator_release_frame` releases it. A
            // subject read from existing storage (`foreach ($items as ...)`) comes back BORROWED
            // from the scope, which releases it too, so it takes a reference of its own here --
            // the same ownership rule the plain `foreach` applies. Without it the array was
            // released twice when the generator was destroyed.
            let owned = eval_foreach_owns_subject(&subject);
            let subject = eval_expr(&subject, context, &mut frame.scope, values)?;
            let subject = if owned { subject } else { values.retain(subject)? };
            // An OBJECT subject is walked the way a plain `foreach` walks it: an `Iterator`
            // through its five methods, an `IteratorAggregate` through `getIterator()` (asked
            // again while it answers another aggregate), anything else through its visible
            // properties. Laravel's `LazyCollection` is one `foreach ($this as ...)` inside a
            // generator closure; the array-only walk ended that loop before its first element,
            // so every lazy pipeline answered an empty array.
            let (subject, position) = eval_generator_foreach_subject(subject, context, values)?;
            frame.foreach_slots[slot] = Some((subject, position));
            EvalGeneratorFlow::Continue
        }
        EvalGeneratorStep::ForeachNext {
            slot,
            key_name,
            value_name,
            exit,
        } => {
            let Some((array, position)) = frame.foreach_slots[slot] else {
                return Err(EvalStatus::RuntimeFatal);
            };
            let (key, value) = if position >= EVAL_GENERATOR_FOREACH_ITERATOR_RUNNING {
                // An `Iterator` subject: rewind on the first visit, `next()` on every later one.
                let step = if position == EVAL_GENERATOR_FOREACH_ITERATOR_FRESH {
                    eval_method_call_result(array, "rewind", Vec::new(), context, values)?
                } else {
                    eval_method_call_result(array, "next", Vec::new(), context, values)?
                };
                values.release(step)?;
                let valid = eval_method_call_result(array, "valid", Vec::new(), context, values)?;
                let is_valid = values.truthy(valid)?;
                values.release(valid)?;
                frame.foreach_slots[slot] = Some((array, EVAL_GENERATOR_FOREACH_ITERATOR_RUNNING));
                if !is_valid {
                    frame.step = exit;
                    return Ok(EvalGeneratorFlow::Continue);
                }
                let value = eval_method_call_result(array, "current", Vec::new(), context, values)?;
                let key = if key_name.is_some() {
                    eval_method_call_result(array, "key", Vec::new(), context, values)?
                } else {
                    values.null()?
                };
                (key, value)
            } else {
                if position >= values.array_len(array)? {
                    frame.step = exit;
                    return Ok(EvalGeneratorFlow::Continue);
                }
                let key = values.array_iter_key(array, position)?;
                let value = values.array_get(array, key)?;
                frame.foreach_slots[slot] = Some((array, position + 1));
                (key, value)
            };
            match key_name {
                Some(key_name) => {
                    if let Some(replaced) =
                        frame.scope.set(key_name, key, ScopeCellOwnership::Owned)
                    {
                        values.release(replaced)?;
                    }
                }
                None => values.release(key)?,
            }
            if let Some(replaced) = frame.scope.set(value_name, value, ScopeCellOwnership::Owned) {
                values.release(replaced)?;
            }
            EvalGeneratorFlow::Continue
        }
        // `EndUnwind` only ever runs under the destruction walk, which stops on it itself.
        EvalGeneratorStep::EndUnwind => EvalGeneratorFlow::Finished,
        EvalGeneratorStep::Run(_)
        | EvalGeneratorStep::EnterTry { .. }
        | EvalGeneratorStep::LeaveTry
        | EvalGeneratorStep::Rethrow { .. } => return Err(EvalStatus::RuntimeFatal),
    })
}

/// Marks a generator `foreach` slot that holds an `Iterator` object not yet rewound.
const EVAL_GENERATOR_FOREACH_ITERATOR_FRESH: usize = usize::MAX;
/// Marks a generator `foreach` slot that holds an `Iterator` object already walking.
const EVAL_GENERATOR_FOREACH_ITERATOR_RUNNING: usize = usize::MAX - 1;

/// Resolves a generator `foreach` subject to what the step machine walks: an array at position
/// zero, or an `Iterator` object in its "fresh" state. The returned cell is owned by the slot.
fn eval_generator_foreach_subject(
    subject: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(RuntimeCellHandle, usize), EvalStatus> {
    let mut subject = subject;
    loop {
        match values.type_tag(subject)? {
            EVAL_TAG_ARRAY | EVAL_TAG_ASSOC => return Ok((subject, 0)),
            EVAL_TAG_OBJECT => {
                if eval_foreach_object_is_a(subject, "Iterator", context, values)? {
                    return Ok((subject, EVAL_GENERATOR_FOREACH_ITERATOR_FRESH));
                }
                let identity = values.object_identity(subject)?;
                if context.has_eval_generator(identity) {
                    return Ok((subject, EVAL_GENERATOR_FOREACH_ITERATOR_FRESH));
                }
                if eval_foreach_object_is_a(subject, "IteratorAggregate", context, values)? {
                    let iterator =
                        eval_method_call_result(subject, "getIterator", Vec::new(), context, values)?;
                    eval_release_value(context, values, subject)?;
                    subject = iterator;
                    continue;
                }
                let properties = eval_get_object_vars_result(&[subject], context, values)?;
                eval_release_value(context, values, subject)?;
                return Ok((properties, 0));
            }
            _ => return Err(EvalStatus::RuntimeFatal),
        }
    }
}

/// Stores what `send()` passed in under the slot the suspended yield named.
fn eval_generator_deliver_sent(
    frame: &mut EvalGeneratorFrame,
    sent: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let Some(name) = frame.pending_send_slot.take() else {
        return Ok(());
    };
    let value = match sent {
        Some(sent) => values.retain(sent)?,
        None => values.null()?,
    };
    if let Some(replaced) = frame.scope.set(name, value, ScopeCellOwnership::Owned) {
        values.release(replaced)?;
    }
    Ok(())
}

/// Starts delegating to an array or another generator.
fn eval_generator_begin_delegation(
    frame: &mut EvalGeneratorFrame,
    source: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    match values.type_tag(source)? {
        EVAL_TAG_ARRAY | EVAL_TAG_ASSOC => {
            frame.delegate = Some(EvalGeneratorDelegate::Array {
                array: source,
                position: 0,
            });
            Ok(())
        }
        EVAL_TAG_OBJECT => {
            let identity = values.object_identity(source)?;
            if context.has_eval_generator(identity) {
                eval_generator_prime(identity, context, values)?;
                frame.delegate = Some(EvalGeneratorDelegate::Generator { identity });
                return Ok(());
            }
            // Not one of ours. php accepts any Traversable here, and a compiled generator is one:
            // it answers the same `valid`/`current`/`key`/`next` methods, so pump it through those.
            if !values.object_is_a(source, "Traversable", false)? {
                return eval_throw_error(
                    "Can use \"yield from\" only with arrays and Traversables",
                    context,
                    values,
                );
            }
            frame.delegate = Some(EvalGeneratorDelegate::Foreign {
                object: values.retain(source)?,
                generator: identity,
            });
            Ok(())
        }
        _ => eval_throw_error(
            "Can use \"yield from\" only with arrays and Traversables",
            context,
            values,
        ),
    }
}

/// Produces the delegate's current pair, or reports that it is exhausted.
///
/// The delegated keys are the INNER ones and the outer generator's auto-increment counter is
/// left alone: `php -n` 8.5.6 gives `yield 0; yield from [10, 20]; yield 99;` the keys 0, 0, 1, 1.
fn eval_generator_produce_from_delegate(
    frame: &mut EvalGeneratorFrame,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    match frame.delegate {
        None => Ok(false),
        Some(EvalGeneratorDelegate::Array { array, position }) => {
            if position >= values.array_len(array)? {
                return Ok(false);
            }
            let key = values.array_iter_key(array, position)?;
            let value = values.array_get(array, key)?;
            eval_generator_set_current(frame, key, value, values)?;
            Ok(true)
        }
        Some(EvalGeneratorDelegate::Foreign { generator, .. }) => {
            if !crate::runtime_hooks::native_generator_valid(generator) {
                return Ok(false);
            }
            let (Some(key), Some(value)) = (
                crate::runtime_hooks::native_generator_key(generator),
                crate::runtime_hooks::native_generator_current(generator),
            ) else {
                return Ok(false);
            };
            eval_generator_set_current(frame, key, value, values)?;
            Ok(true)
        }
        Some(EvalGeneratorDelegate::Generator { identity }) => {
            let valid = context
                .eval_generator(identity, |inner| {
                    inner.state != EvalGeneratorState::Finished && inner.current_value.is_some()
                })
                .unwrap_or(false);
            if !valid {
                return Ok(false);
            }
            let Some((key, value)) = context.eval_generator(identity, |inner| {
                inner.current_key.zip(inner.current_value)
            }).flatten() else {
                return Ok(false);
            };
            let key = values.retain(key)?;
            let value = values.retain(value)?;
            eval_generator_set_current(frame, key, value, values)?;
            Ok(true)
        }
    }
}

/// Moves the delegate on by one before its next value is read.
fn eval_generator_advance_delegate(
    frame: &mut EvalGeneratorFrame,
    sent: Option<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    match frame.delegate {
        None => Ok(()),
        Some(EvalGeneratorDelegate::Array {
            array,
            ref mut position,
        }) => {
            let _ = array;
            *position += 1;
            Ok(())
        }
        Some(EvalGeneratorDelegate::Foreign { generator, .. }) => {
            // `send()` into a foreign delegate would need `__rt_gen_send`; a plain advance is what
            // `yield from` does when nothing was sent, which is every case a compiled generator
            // reaches today.
            let _ = sent;
            crate::runtime_hooks::native_generator_next(generator);
            Ok(())
        }
        Some(EvalGeneratorDelegate::Generator { identity }) => {
            eval_generator_step(identity, sent, true, context, values)
        }
    }
}


/// Clears an exhausted delegation and hands its return value to the waiting `into` slot.
fn eval_generator_end_delegation(
    frame: &mut EvalGeneratorFrame,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let delegate = frame.delegate.take();
    let produced = match delegate {
        Some(EvalGeneratorDelegate::Array { array, .. }) => {
            values.release(array)?;
            None
        }
        Some(EvalGeneratorDelegate::Generator { identity }) => context
            .eval_generator(identity, |inner| inner.return_value)
            .flatten(),
        Some(EvalGeneratorDelegate::Foreign { object, generator }) => {
            // `$x = yield from $gen;` takes the delegate's own return value, which only a
            // Generator has. Anything else Traversable produces none, exactly as in php.
            let produced = if values.object_is_a(object, "Generator", false)? {
                crate::runtime_hooks::native_generator_return(generator)
            } else {
                None
            };
            values.release(object)?;
            produced
        }
        None => None,
    };
    let Some(name) = frame.pending_send_slot.take() else {
        return Ok(());
    };
    let value = match produced {
        Some(value) => values.retain(value)?,
        None => values.null()?,
    };
    if let Some(replaced) = frame.scope.set(name, value, ScopeCellOwnership::Owned) {
        values.release(replaced)?;
    }
    Ok(())
}

/// Marks the generator finished and stores what `getReturn()` will hand back.
fn eval_generator_finish(
    frame: &mut EvalGeneratorFrame,
    value: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    frame.state = EvalGeneratorState::Finished;
    eval_generator_clear_current(frame, values)?;
    frame.return_value = value;
    Ok(())
}

/// Replaces the currently yielded pair, releasing the previous one.
fn eval_generator_set_current(
    frame: &mut EvalGeneratorFrame,
    key: RuntimeCellHandle,
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    eval_generator_clear_current(frame, values)?;
    frame.current_key = Some(key);
    frame.current_value = Some(value);
    Ok(())
}

/// Drops the currently yielded pair.
fn eval_generator_clear_current(
    frame: &mut EvalGeneratorFrame,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if let Some(key) = frame.current_key.take() {
        values.release(key)?;
    }
    if let Some(value) = frame.current_value.take() {
        values.release(value)?;
    }
    Ok(())
}

/// Runs one `Generator` protocol operation on behalf of the native `__rt_gen_*` helpers.
///
/// The generated runtime drives a `Generator` through its fiber fields, which an
/// INTERPRETER-created generator does not have: it has a frame in this context instead, and
/// reading the fiber slots of one yields whatever the object's storage happens to hold. The
/// `__rt_gen_*` helpers therefore probe for an eval owner first and land here, where the
/// protocol is answered from the registered frame. `argument` is the borrowed cell `send()`
/// and `throw()` pass; every other operation ignores it.
#[cfg(not(test))]
pub(crate) fn eval_generator_protocol_result(
    identity: u64,
    method_name: &str,
    argument: Option<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    // `owned: false`: the cell belongs to the AOT caller, which releases it after the call.
    let evaluated_args = argument
        .map(|value| EvaluatedCallArg {
            name: None,
            value: value.borrowed(),
            ref_target: None,
        })
        .into_iter()
        .collect();
    eval_generator_method_result(identity, method_name, evaluated_args, context, values)
}

/// Dispatches the `Generator` methods PHP exposes.
pub(in crate::interpreter) fn eval_generator_method_result(
    identity: u64,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if !context.has_eval_generator(identity) {
        return Ok(None);
    }
    match method_name.to_ascii_lowercase().as_str() {
        "current" => {
            eval_generator_prime(identity, context, values)?;
            match context
                .eval_generator(identity, |frame| frame.current_value)
                .flatten()
            {
                Some(value) => values.retain(value).map(Some),
                None => values.null().map(Some),
            }
        }
        "key" => {
            eval_generator_prime(identity, context, values)?;
            match context
                .eval_generator(identity, |frame| frame.current_key)
                .flatten()
            {
                Some(key) => values.retain(key).map(Some),
                None => values.null().map(Some),
            }
        }
        "valid" => {
            eval_generator_prime(identity, context, values)?;
            let valid = context
                .eval_generator(identity, |frame| frame.current_value.is_some())
                .unwrap_or(false);
            values.bool_value(valid).map(Some)
        }
        "next" => {
            eval_generator_prime(identity, context, values)?;
            eval_generator_mark_advanced(identity, context);
            eval_generator_step(identity, None, true, context, values)?;
            values.null().map(Some)
        }
        "send" => {
            let sent = evaluated_args.first().map(|arg| arg.value);
            // PHP runs the body up to the first yield before a `send()` can deliver anything,
            // and that first yield's value is discarded by the send itself.
            eval_generator_prime(identity, context, values)?;
            eval_generator_mark_advanced(identity, context);
            eval_generator_step(identity, sent, true, context, values)?;
            match context
                .eval_generator(identity, |frame| frame.current_value)
                .flatten()
            {
                Some(value) => values.retain(value).map(Some),
                None => values.null().map(Some),
            }
        }
        "rewind" => {
            eval_generator_prime(identity, context, values)?;
            let advanced = context
                .eval_generator(identity, |frame| frame.advanced)
                .unwrap_or(false);
            if advanced {
                return eval_throw_exception_message(
                    "Cannot rewind a generator that was already run",
                    context,
                    values,
                );
            }
            values.null().map(Some)
        }
        "getreturn" => {
            let finished = context
                .eval_generator(identity, |frame| frame.state == EvalGeneratorState::Finished)
                .unwrap_or(false);
            if !finished {
                return eval_throw_exception_message(
                    "Cannot get return value of a generator that hasn't returned",
                    context,
                    values,
                );
            }
            match context
                .eval_generator(identity, |frame| frame.return_value)
                .flatten()
            {
                Some(value) => values.retain(value).map(Some),
                None => values.null().map(Some),
            }
        }
        _ => Ok(None),
    }
}

/// Records that a generator has moved past its first yield, which `rewind()` refuses after.
fn eval_generator_mark_advanced(identity: u64, context: &mut ElephcEvalContext) {
    if let Some(mut frame) = context.take_eval_generator(identity) {
        frame.advanced = true;
        context.restore_eval_generator(identity, frame);
    }
}

/// Creates and schedules a plain `Exception`, which is what the generator errors are.
/// Raises one PHP `\Error` with a fixed message.
fn eval_throw_error_message<T>(
    message: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<T, EvalStatus> {
    eval_throw_builtin_exception_with_code("Error", message, 0, context, values)
}

fn eval_throw_exception_message<T>(
    message: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<T, EvalStatus> {
    eval_throw_builtin_exception_with_code("Exception", message, 0, context, values)
}

/// Drives `foreach` over one generator, which PHP refuses once the generator has finished.
/// Drains one generator into owned key/value pairs, for the array-literal spread.
///
/// PHP's array unpacking materializes the whole operand into the new array, so the generator is
/// run to completion here rather than suspended. Both handles in each pair are OWNED: the frame
/// keeps its own reference to the current key and value, and the caller stores these into an
/// array that takes them over.
pub(in crate::interpreter) fn eval_generator_collect_entries(
    identity: u64,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<(RuntimeCellHandle, RuntimeCellHandle)>, EvalStatus> {
    let already_closed = context
        .eval_generator(identity, |frame| {
            frame.state == EvalGeneratorState::Finished && frame.advanced
        })
        .unwrap_or(false);
    if already_closed {
        eval_throw_exception_message(
            "Cannot traverse an already closed generator",
            context,
            values,
        )?;
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_generator_prime(identity, context, values)?;
    let mut entries = Vec::new();
    loop {
        let Some((key, value)) = context
            .eval_generator(identity, |frame| frame.current_key.zip(frame.current_value))
            .flatten()
        else {
            return Ok(entries);
        };
        entries.push((values.retain(key)?, values.retain(value)?));
        eval_generator_mark_advanced(identity, context);
        eval_generator_step(identity, None, true, context, values)?;
    }
}

pub(in crate::interpreter) fn execute_foreach_generator_stmt(
    identity: u64,
    key_name: Option<&str>,
    value_name: &str,
    body: &[EvalStmt],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalControl, EvalStatus> {
    let already_closed = context
        .eval_generator(identity, |frame| {
            frame.state == EvalGeneratorState::Finished && frame.advanced
        })
        .unwrap_or(false);
    if already_closed {
        return eval_throw_exception_message(
            "Cannot traverse an already closed generator",
            context,
            values,
        );
    }
    // A resumed generator frame reports its caller on the foreach line, not on the last body
    // statement the loop ran (php: Twig's `yield from $this->block_x()` stack in live ids).
    let foreach_line = context.call_line();
    eval_generator_prime(identity, context, values)?;
    loop {
        let Some((key, value)) = context
            .eval_generator(identity, |frame| frame.current_key.zip(frame.current_value))
            .flatten()
        else {
            return Ok(EvalControl::None);
        };
        let key = values.retain(key)?;
        let value = values.retain(value)?;
        match key_name {
            Some(key_name) => {
                for replaced in set_scope_cell(
                    context,
                    scope,
                    key_name.to_string(),
                    key,
                    ScopeCellOwnership::Owned,
                    values,
                )? {
                    values.release(replaced)?;
                }
            }
            None => values.release(key)?,
        }
        for replaced in set_scope_cell(
            context,
            scope,
            value_name.to_string(),
            value,
            ScopeCellOwnership::Owned,
            values,
        )? {
            values.release(replaced)?;
        }
        match execute_statements(body, context, scope, values)? {
            EvalControl::None | EvalControl::Continue(1) => {}
            EvalControl::Break(1) => break,
            EvalControl::Break(level) => return Ok(EvalControl::Break(level - 1)),
            EvalControl::Continue(level) => return Ok(EvalControl::Continue(level - 1)),
            EvalControl::Throw(result) => return Ok(EvalControl::Throw(result)),
            EvalControl::ReturnVoid => return Ok(EvalControl::ReturnVoid),
            EvalControl::Return(result) => return Ok(EvalControl::Return(result)),
            EvalControl::Goto(label) => return Ok(EvalControl::Goto(label)),
        }
        eval_generator_mark_advanced(identity, context);
        context.set_call_line(foreach_line);
        eval_generator_step(identity, None, true, context, values)?;
    }
    Ok(EvalControl::None)
}
