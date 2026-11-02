//! Purpose:
//! Lowers a generator function body into the flat step list the generator frame executes.
//!
//! Called from:
//! - `super::eval_generator_new()` once, when a generator object is created.
//!
//! Key details:
//! - ONLY constructs that contain a `yield` are flattened. Any statement that does not is kept
//!   whole and handed to `execute_statements()`, so the ordinary evaluator still does all the
//!   real work and this file owns nothing but the control flow around suspension points.
//! - The step list is a program with an explicit counter, which is what makes resuming free:
//!   the frame stores the index of the next step and continues there. A resumable tree walker
//!   would have to reify every loop's and every `try`'s position anyway, and would then have to
//!   re-descend the tree on each resume.

use super::*;
use crate::context::{EvalGeneratorProgram, EvalGeneratorStep};
use crate::parser::{EVAL_YIELD_FROM_INTRINSIC, EVAL_YIELD_INTRINSIC};

/// The reserved scope name a `return yield ...;` stores its sent value under.
///
/// PHP allows the sent value to become the return value, so the yield needs somewhere to put it
/// before the return reads it back. The name cannot collide with a PHP variable because `$` is
/// not part of it and PHP identifiers cannot contain a NUL.
pub(super) const EVAL_GENERATOR_SENT_SLOT: &str = "\0generator\0sent";

/// The reserved scope name a `return` inside a `try` parks its value in.
///
/// PHP evaluates the returned expression before running the `finally` blocks, so the value has
/// to survive them rather than be recomputed after.
const EVAL_GENERATOR_RETURN_SLOT: &str = "\0generator\0return";

/// Lowers one function body, or refuses a yield this lowering does not model.
pub(super) fn lower_generator_body(
    body: &[EvalStmt],
) -> Result<EvalGeneratorProgram, EvalStatus> {
    let mut lowering = Lowering {
        steps: Vec::new(),
        loops: Vec::new(),
        foreach_slots: 0,
        subject_slots: 0,
        thrown_slots: 0,
        regions: Vec::new(),
    };
    lowering.lower_statements(body)?;
    lowering.steps.push(EvalGeneratorStep::Return(None));
    Ok(EvalGeneratorProgram {
        steps: lowering.steps,
        foreach_slots: lowering.foreach_slots,
    })
}

/// Where a `break` and a `continue` jump to inside one lowered loop or switch.
struct LoopLabels {
    continue_patches: Vec<usize>,
    break_patches: Vec<usize>,
    /// A `switch` is a break level but not a continue target.
    ///
    /// PHP counts a switch as one level for BOTH keywords and makes `continue` inside a switch
    /// behave like `break`, so a continue that lands here takes the break edge.
    is_switch: bool,
    /// How many `try` regions were open when this level opened, so an exit knows how many
    /// `finally` blocks it has to run on the way out.
    region_depth: usize,
}

/// Accumulates the step list while walking the body.
struct Lowering {
    steps: Vec<EvalGeneratorStep>,
    loops: Vec<LoopLabels>,
    foreach_slots: usize,
    /// Number of `switch` and `match` subjects lowered so far, used to name their scope slots.
    subject_slots: usize,
    /// Number of `try` regions opened so far, used to name their thrown-value slots.
    thrown_slots: usize,
    /// The `finally` body of every `try` region currently open, innermost last.
    regions: Vec<Vec<EvalStmt>>,
}

/// Returns the reserved scope name holding one lowered `switch` or `match` subject.
///
/// PHP evaluates the subject exactly once and compares it against each arm, so it has to be
/// stored somewhere the comparison steps can read it back. The name cannot collide with a PHP
/// variable because PHP identifiers cannot contain a NUL.
fn subject_slot_name(index: usize) -> String {
    format!("\0generator\0subject\0{index}")
}

impl Lowering {
    /// Lowers a statement list, batching yield-free runs into single steps.
    fn lower_statements(&mut self, body: &[EvalStmt]) -> Result<(), EvalStatus> {
        let mut pending: Vec<EvalStmt> = Vec::new();
        for statement in body {
            if self.statement_needs_lowering_here(statement) {
                self.flush(&mut pending);
                self.lower_statement(statement)?;
            } else {
                pending.push(statement.clone());
            }
        }
        self.flush(&mut pending);
        Ok(())
    }

    /// Returns whether this statement must be lowered where it currently stands.
    ///
    /// Inside a `try`, a `return` is one of them even with no yield in sight: run atomically it
    /// would finish the generator straight from the chunk and PHP's `finally` would never run.
    fn statement_needs_lowering_here(&self, statement: &EvalStmt) -> bool {
        (!self.regions.is_empty() && statement_contains_return(statement))
            || statement_needs_lowering(statement)
    }

    /// Emits the batched yield-free statements as one atomic step.
    fn flush(&mut self, pending: &mut Vec<EvalStmt>) {
        if !pending.is_empty() {
            self.steps
                .push(EvalGeneratorStep::Run(std::mem::take(pending)));
        }
    }

    /// Lowers one statement that contains a yield, a break, or a continue.
    fn lower_statement(&mut self, statement: &EvalStmt) -> Result<(), EvalStatus> {
        match statement {
            EvalStmt::Expr(expr) => self.lower_yield_expr(expr, None),
            EvalStmt::StoreVar { name, value } => self.lower_yield_expr(value, Some(name.clone())),
            EvalStmt::Return(Some(expr)) if expression_contains_yield(expr) => {
                let slot = EVAL_GENERATOR_SENT_SLOT.to_string();
                self.lower_yield_expr(expr, Some(slot.clone()))?;
                self.unwind_regions_to(0)?;
                self.steps
                    .push(EvalGeneratorStep::Return(Some(EvalExpr::LoadVar(slot))));
                Ok(())
            }
            EvalStmt::Return(value) => self.lower_return(value.as_ref()),
            EvalStmt::Break(level) => self.lower_loop_exit(*level, true),
            EvalStmt::Continue(level) => self.lower_loop_exit(*level, false),
            EvalStmt::If {
                condition,
                then_branch,
                else_branch,
            } => self.lower_if(condition, then_branch, else_branch),
            EvalStmt::While { condition, body } => self.lower_while(condition, body),
            EvalStmt::DoWhile { body, condition } => self.lower_do_while(body, condition),
            EvalStmt::For {
                init,
                condition,
                update,
                body,
            } => self.lower_for(init, condition.as_ref(), update, body),
            EvalStmt::Foreach {
                array,
                key_name,
                value_name,
                value_by_ref,
                body,
            } => self.lower_foreach(array, key_name.as_deref(), value_name, *value_by_ref, body),
            EvalStmt::Switch { expr, cases } => self.lower_switch(expr, cases),
            EvalStmt::Try {
                body,
                catches,
                finally_body,
            } => self.lower_try(body, catches, finally_body),
            _ => Err(EvalStatus::UnsupportedConstruct),
        }
    }

    /// Lowers a `try`/`catch`/`finally` whose body, handlers or finally contain a yield.
    ///
    /// The region is data on the frame rather than a Rust stack frame, so a generator suspended
    /// inside a `try` still knows which handler covers it when something resumes it later. The
    /// `finally` body is emitted once per EXIT EDGE — normal completion, each catch clause, and
    /// the unhandled path — because PHP runs it on every one of them and a shared copy would
    /// need a return address the step list has nowhere to put.
    fn lower_try(
        &mut self,
        body: &[EvalStmt],
        catches: &[EvalCatch],
        finally_body: &[EvalStmt],
    ) -> Result<(), EvalStatus> {
        let body_slot = self.next_thrown_slot();
        let enter = self.steps.len();
        self.steps.push(EvalGeneratorStep::EnterTry {
            handler: usize::MAX,
            finally_entry: usize::MAX,
            thrown_slot: body_slot.clone(),
        });
        self.regions.push(finally_body.to_vec());
        self.lower_statements(body)?;
        self.regions.pop();
        self.steps.push(EvalGeneratorStep::LeaveTry);
        self.lower_statements(finally_body)?;
        let mut to_after = vec![self.steps.len()];
        self.steps.push(EvalGeneratorStep::Jump(usize::MAX));

        let handler = self.steps.len();
        self.patch_enter_try(enter, handler);
        // A throw from inside a catch body still has to run the `finally`, so the dispatch runs
        // inside a region of its own.
        let catch_slot = self.next_thrown_slot();
        let catch_enter = self.steps.len();
        self.steps.push(EvalGeneratorStep::EnterTry {
            handler: usize::MAX,
            finally_entry: usize::MAX,
            thrown_slot: catch_slot.clone(),
        });
        let mut body_patches: Vec<Vec<usize>> = Vec::new();
        for catch in catches {
            let mut matched: Vec<usize> = Vec::new();
            for class_name in &catch.class_names {
                let skip = self.steps.len();
                self.steps.push(EvalGeneratorStep::JumpIfFalse {
                    condition: EvalExpr::InstanceOf {
                        value: Box::new(EvalExpr::LoadVar(body_slot.clone())),
                        target: EvalInstanceOfTarget::ClassName(class_name.clone()),
                    },
                    target: usize::MAX,
                });
                matched.push(self.steps.len());
                self.steps.push(EvalGeneratorStep::Jump(usize::MAX));
                let next_test = self.steps.len();
                self.patch_jump(skip, next_test);
            }
            body_patches.push(matched);
        }
        // No clause matched: leave the catch region, run the finally, and raise onward.
        self.steps.push(EvalGeneratorStep::LeaveTry);
        self.lower_statements(finally_body)?;
        self.steps.push(EvalGeneratorStep::Rethrow {
            thrown_slot: body_slot.clone(),
        });

        let mut body_starts: Vec<usize> = Vec::new();
        for catch in catches {
            body_starts.push(self.steps.len());
            if let Some(var_name) = &catch.var_name {
                self.steps.push(EvalGeneratorStep::Run(vec![EvalStmt::StoreVar {
                    name: var_name.clone(),
                    value: EvalExpr::LoadVar(body_slot.clone()),
                }]));
            }
            self.regions.push(finally_body.to_vec());
            self.lower_statements(&catch.body)?;
            self.regions.pop();
            self.steps.push(EvalGeneratorStep::LeaveTry);
            self.lower_statements(finally_body)?;
            to_after.push(self.steps.len());
            self.steps.push(EvalGeneratorStep::Jump(usize::MAX));
        }

        let catch_handler = self.steps.len();
        self.patch_enter_try(catch_enter, catch_handler);
        self.lower_statements(finally_body)?;
        self.steps.push(EvalGeneratorStep::Rethrow {
            thrown_slot: catch_slot,
        });

        // A generator destroyed while suspended inside this region runs its `finally` from here,
        // which is why the block is emitted a last time with no exit edge of its own.
        let unwind = self.steps.len();
        self.lower_statements(finally_body)?;
        self.steps.push(EvalGeneratorStep::EndUnwind);
        self.patch_finally_entry(enter, unwind);
        self.patch_finally_entry(catch_enter, unwind);

        let after = self.steps.len();
        for (patches, start) in body_patches.into_iter().zip(body_starts) {
            for patch in patches {
                self.patch_jump(patch, start);
            }
        }
        for patch in to_after {
            self.patch_jump(patch, after);
        }
        Ok(())
    }

    /// Lowers a `return`, running the `finally` of every `try` region it leaves.
    ///
    /// PHP evaluates the returned expression FIRST and only then runs the finally blocks, so the
    /// value is parked in a reserved slot rather than re-evaluated after them: a `finally` that
    /// changes what the expression reads must not change what was already returned.
    fn lower_return(&mut self, value: Option<&EvalExpr>) -> Result<(), EvalStatus> {
        if self.regions.is_empty() {
            self.steps.push(EvalGeneratorStep::Return(value.cloned()));
            return Ok(());
        }
        let Some(value) = value else {
            self.unwind_regions_to(0)?;
            self.steps.push(EvalGeneratorStep::Return(None));
            return Ok(());
        };
        let slot = EVAL_GENERATOR_RETURN_SLOT.to_string();
        self.steps.push(EvalGeneratorStep::Run(vec![EvalStmt::StoreVar {
            name: slot.clone(),
            value: value.clone(),
        }]));
        self.unwind_regions_to(0)?;
        self.steps
            .push(EvalGeneratorStep::Return(Some(EvalExpr::LoadVar(slot))));
        Ok(())
    }

    /// Returns a fresh reserved scope name for one `try` region's thrown value.
    fn next_thrown_slot(&mut self) -> String {
        let slot = format!("\0generator\0thrown\0{}", self.thrown_slots);
        self.thrown_slots += 1;
        slot
    }

    /// Emits the `LeaveTry` and `finally` copies for every region left by an early exit.
    ///
    /// PHP runs a `finally` when control jumps out of its `try`, so a `return`, `break` or
    /// `continue` that crosses one has to carry the block with it. `depth` is the number of
    /// regions that stay open.
    fn unwind_regions_to(&mut self, depth: usize) -> Result<(), EvalStatus> {
        // The region stack itself is NOT popped: the lowering keeps emitting code that is still
        // lexically inside these regions, and only the jump about to be emitted leaves them.
        let bodies: Vec<Vec<EvalStmt>> = self.regions[depth..].iter().rev().cloned().collect();
        for finally_body in bodies {
            self.steps.push(EvalGeneratorStep::LeaveTry);
            self.lower_statements(&finally_body)?;
        }
        Ok(())
    }

    /// Points one emitted `EnterTry` at its catch dispatch.
    fn patch_enter_try(&mut self, index: usize, target: usize) {
        if let Some(EvalGeneratorStep::EnterTry { handler, .. }) = self.steps.get_mut(index) {
            *handler = target;
        }
    }

    /// Points one emitted `EnterTry` at its destruction-time `finally` copy.
    fn patch_finally_entry(&mut self, index: usize, target: usize) {
        if let Some(EvalGeneratorStep::EnterTry { finally_entry, .. }) = self.steps.get_mut(index) {
            *finally_entry = target;
        }
    }

    /// Lowers a `switch` whose arms contain a yield.
    ///
    /// PHP evaluates the subject once, compares it loosely against each `case` in source order,
    /// enters the first match, and then FALLS THROUGH the remaining bodies until a `break`. The
    /// layout below keeps that exactly: every test comes first and jumps into a single run of
    /// bodies laid out in source order, so falling out of one body lands in the next.
    fn lower_switch(
        &mut self,
        subject: &EvalExpr,
        cases: &[EvalSwitchCase],
    ) -> Result<(), EvalStatus> {
        let slot = subject_slot_name(self.subject_slots);
        self.subject_slots += 1;
        self.steps.push(EvalGeneratorStep::Run(vec![EvalStmt::StoreVar {
            name: slot.clone(),
            value: subject.clone(),
        }]));
        let mut body_patches: Vec<usize> = Vec::new();
        let mut default_patch: Option<usize> = None;
        for case in cases {
            match &case.condition {
                Some(condition) => {
                    let skip = self.steps.len();
                    self.steps.push(EvalGeneratorStep::JumpIfFalse {
                        condition: EvalExpr::Binary {
                            op: EvalBinOp::LooseEq,
                            left: Box::new(EvalExpr::LoadVar(slot.clone())),
                            right: Box::new(condition.clone()),
                        },
                        target: usize::MAX,
                    });
                    body_patches.push(self.steps.len());
                    self.steps.push(EvalGeneratorStep::Jump(usize::MAX));
                    let next_test = self.steps.len();
                    self.patch_jump(skip, next_test);
                }
                None => {
                    // A `default` is tested last however it is written, so its jump is emitted
                    // after every `case` test rather than in source position.
                    body_patches.push(usize::MAX);
                    default_patch = Some(body_patches.len() - 1);
                }
            }
        }
        let no_match_patch = self.steps.len();
        self.steps.push(EvalGeneratorStep::Jump(usize::MAX));
        self.loops.push(LoopLabels {
            continue_patches: Vec::new(),
            break_patches: Vec::new(),
            is_switch: true,
            region_depth: self.regions.len(),
        });
        let mut body_starts: Vec<usize> = Vec::new();
        for case in cases {
            body_starts.push(self.steps.len());
            self.lower_statements(&case.body)?;
        }
        let after = self.steps.len();
        for (index, patch) in body_patches.iter().enumerate() {
            if *patch != usize::MAX {
                self.patch_jump(*patch, body_starts[index]);
            }
        }
        match default_patch {
            Some(index) => self.patch_jump(no_match_patch, body_starts[index]),
            None => self.patch_jump(no_match_patch, after),
        }
        let labels = self.loops.pop().ok_or(EvalStatus::RuntimeFatal)?;
        for patch in labels.break_patches.into_iter().chain(labels.continue_patches) {
            self.patch_jump(patch, after);
        }
        Ok(())
    }

    /// Lowers a `match` expression whose arm values contain a yield.
    ///
    /// `match` compares STRICTLY, evaluates exactly one arm, and raises `\UnhandledMatchError`
    /// when nothing matches. The no-match edge re-runs the original expression through the
    /// ordinary evaluator with the arms it can evaluate removed, so the error object, its
    /// message and its class come from the one place that already builds them.
    fn lower_match(
        &mut self,
        subject: &EvalExpr,
        arms: &[EvalMatchArm],
        default: Option<&EvalExpr>,
        into: Option<String>,
    ) -> Result<(), EvalStatus> {
        let slot = subject_slot_name(self.subject_slots);
        self.subject_slots += 1;
        self.steps.push(EvalGeneratorStep::Run(vec![EvalStmt::StoreVar {
            name: slot.clone(),
            value: subject.clone(),
        }]));
        let mut arm_patches: Vec<usize> = Vec::new();
        for arm in arms {
            let mut matched: Vec<usize> = Vec::new();
            for pattern in &arm.patterns {
                let skip = self.steps.len();
                self.steps.push(EvalGeneratorStep::JumpIfFalse {
                    condition: EvalExpr::Binary {
                        op: EvalBinOp::StrictEq,
                        left: Box::new(EvalExpr::LoadVar(slot.clone())),
                        right: Box::new(pattern.clone()),
                    },
                    target: usize::MAX,
                });
                matched.push(self.steps.len());
                self.steps.push(EvalGeneratorStep::Jump(usize::MAX));
                let next_pattern = self.steps.len();
                self.patch_jump(skip, next_pattern);
            }
            arm_patches.push(matched.len());
            for patch in matched {
                arm_patches.push(patch);
            }
        }
        let no_match = self.steps.len();
        self.steps.push(EvalGeneratorStep::Jump(usize::MAX));
        let mut arm_starts: Vec<usize> = Vec::new();
        let mut end_patches: Vec<usize> = Vec::new();
        for arm in arms {
            arm_starts.push(self.steps.len());
            self.lower_match_value(&arm.value, into.clone())?;
            end_patches.push(self.steps.len());
            self.steps.push(EvalGeneratorStep::Jump(usize::MAX));
        }
        let default_start = self.steps.len();
        match default {
            Some(value) => {
                self.lower_match_value(value, into.clone())?;
            }
            None => {
                // No arm matched and there is no default: PHP raises `\UnhandledMatchError`.
                // Replaying the subject through an arm-less `match` lets the ordinary evaluator
                // build that error, message included, instead of duplicating it here.
                self.steps.push(EvalGeneratorStep::Run(vec![EvalStmt::Expr(
                    EvalExpr::Match {
                        subject: Box::new(EvalExpr::LoadVar(slot.clone())),
                        arms: Vec::new(),
                        default: None,
                    },
                )]));
            }
        }
        let after = self.steps.len();
        self.patch_jump(no_match, default_start);
        for patch in end_patches {
            self.patch_jump(patch, after);
        }
        let mut cursor = 0;
        for start in arm_starts {
            let count = arm_patches[cursor];
            cursor += 1;
            for _ in 0..count {
                self.patch_jump(arm_patches[cursor], start);
                cursor += 1;
            }
        }
        Ok(())
    }

    /// Emits one `match` arm's value, which may itself be the yield that suspends.
    fn lower_match_value(
        &mut self,
        value: &EvalExpr,
        into: Option<String>,
    ) -> Result<(), EvalStatus> {
        if expression_contains_yield(value) {
            return self.lower_yield_expr(value, into);
        }
        let statement = match into {
            Some(name) => EvalStmt::StoreVar {
                name,
                value: value.clone(),
            },
            None => EvalStmt::Expr(value.clone()),
        };
        self.steps.push(EvalGeneratorStep::Run(vec![statement]));
        Ok(())
    }

    /// Lowers one expression that carries a yield in a position this lowering models.
    ///
    /// The three positions are the ones PHP programs actually use: a bare `yield ...;`, the
    /// right-hand side of an assignment to a variable, and the operand of a `return`. A yield
    /// buried deeper in an expression would need the evaluator itself to suspend mid-expression.
    fn lower_yield_expr(
        &mut self,
        expr: &EvalExpr,
        into: Option<String>,
    ) -> Result<(), EvalStatus> {
        match expr {
            EvalExpr::Call { name, args } if name == EVAL_YIELD_INTRINSIC => {
                let (key, value) = match args.as_slice() {
                    [value] => (None, value.value().clone()),
                    [key, value] => (Some(key.value().clone()), value.value().clone()),
                    _ => return Err(EvalStatus::RuntimeFatal),
                };
                self.steps
                    .push(EvalGeneratorStep::Yield { key, value, into });
                Ok(())
            }
            EvalExpr::Call { name, args } if name == EVAL_YIELD_FROM_INTRINSIC => {
                let [source] = args.as_slice() else {
                    return Err(EvalStatus::RuntimeFatal);
                };
                self.steps.push(EvalGeneratorStep::YieldFrom {
                    source: source.value().clone(),
                    into,
                });
                Ok(())
            }
            EvalExpr::Match {
                subject,
                arms,
                default,
            } => self.lower_match(subject, arms, default.as_deref(), into),
            _ => Err(EvalStatus::UnsupportedConstruct),
        }
    }

    /// Emits the jump for a `break` or a `continue` targeting an enclosing lowered loop.
    fn lower_loop_exit(&mut self, level: u32, is_break: bool) -> Result<(), EvalStatus> {
        let depth = self.loops.len();
        let level = level.max(1) as usize;
        if level > depth {
            return Err(EvalStatus::UnsupportedConstruct);
        }
        let index = depth - level;
        // Leaving a loop or switch also leaves every `try` opened inside it, and PHP runs each
        // of their `finally` blocks on the way out.
        self.unwind_regions_to(self.loops[index].region_depth)?;
        let patch = self.steps.len();
        self.steps.push(EvalGeneratorStep::Jump(usize::MAX));
        // PHP counts a `switch` as one level for `continue` too, and a `continue` that lands on
        // one leaves the switch exactly as `break` would.
        if is_break || self.loops[index].is_switch {
            self.loops[index].break_patches.push(patch);
        } else {
            self.loops[index].continue_patches.push(patch);
        }
        Ok(())
    }

    /// Lowers `if`/`else` into a conditional jump around each branch.
    fn lower_if(
        &mut self,
        condition: &EvalExpr,
        then_branch: &[EvalStmt],
        else_branch: &[EvalStmt],
    ) -> Result<(), EvalStatus> {
        let branch_patch = self.steps.len();
        self.steps.push(EvalGeneratorStep::JumpIfFalse {
            condition: condition.clone(),
            target: usize::MAX,
        });
        self.lower_statements(then_branch)?;
        if else_branch.is_empty() {
            let after = self.steps.len();
            self.patch_jump(branch_patch, after);
            return Ok(());
        }
        let skip_else = self.steps.len();
        self.steps.push(EvalGeneratorStep::Jump(usize::MAX));
        let else_start = self.steps.len();
        self.patch_jump(branch_patch, else_start);
        self.lower_statements(else_branch)?;
        let after = self.steps.len();
        self.patch_jump(skip_else, after);
        Ok(())
    }

    /// Lowers `while` into a test at the top and a back edge at the bottom.
    fn lower_while(
        &mut self,
        condition: &EvalExpr,
        body: &[EvalStmt],
    ) -> Result<(), EvalStatus> {
        let top = self.steps.len();
        let exit_patch = self.steps.len();
        self.steps.push(EvalGeneratorStep::JumpIfFalse {
            condition: condition.clone(),
            target: usize::MAX,
        });
        self.loops.push(LoopLabels {
            continue_patches: Vec::new(),
            break_patches: Vec::new(),
            is_switch: false,
            region_depth: self.regions.len(),
        });
        self.lower_statements(body)?;
        self.steps.push(EvalGeneratorStep::Jump(top));
        let after = self.steps.len();
        self.patch_jump(exit_patch, after);
        self.close_loop(top, after);
        Ok(())
    }

    /// Lowers `do`/`while`, whose body runs before the first test.
    fn lower_do_while(
        &mut self,
        body: &[EvalStmt],
        condition: &EvalExpr,
    ) -> Result<(), EvalStatus> {
        let top = self.steps.len();
        self.loops.push(LoopLabels {
            continue_patches: Vec::new(),
            break_patches: Vec::new(),
            is_switch: false,
            region_depth: self.regions.len(),
        });
        self.lower_statements(body)?;
        let test = self.steps.len();
        self.steps.push(EvalGeneratorStep::JumpIfFalse {
            condition: condition.clone(),
            target: usize::MAX,
        });
        self.steps.push(EvalGeneratorStep::Jump(top));
        let after = self.steps.len();
        self.patch_jump(test, after);
        self.close_loop(test, after);
        Ok(())
    }

    /// Lowers `for`, whose update runs on the continue edge as well as the fall-through.
    fn lower_for(
        &mut self,
        init: &[EvalStmt],
        condition: Option<&EvalExpr>,
        update: &[EvalStmt],
        body: &[EvalStmt],
    ) -> Result<(), EvalStatus> {
        if !init.is_empty() {
            self.steps.push(EvalGeneratorStep::Run(init.to_vec()));
        }
        let top = self.steps.len();
        let exit_patch = condition.map(|condition| {
            let patch = self.steps.len();
            self.steps.push(EvalGeneratorStep::JumpIfFalse {
                condition: condition.clone(),
                target: usize::MAX,
            });
            patch
        });
        self.loops.push(LoopLabels {
            continue_patches: Vec::new(),
            break_patches: Vec::new(),
            is_switch: false,
            region_depth: self.regions.len(),
        });
        self.lower_statements(body)?;
        let continue_target = self.steps.len();
        if !update.is_empty() {
            self.steps.push(EvalGeneratorStep::Run(update.to_vec()));
        }
        self.steps.push(EvalGeneratorStep::Jump(top));
        let after = self.steps.len();
        if let Some(exit_patch) = exit_patch {
            self.patch_jump(exit_patch, after);
        }
        self.close_loop(continue_target, after);
        Ok(())
    }

    /// Lowers `foreach` into a frame-held iteration slot plus a bind-or-exit step.
    fn lower_foreach(
        &mut self,
        subject: &EvalExpr,
        key_name: Option<&str>,
        value_name: &str,
        value_by_ref: bool,
        body: &[EvalStmt],
    ) -> Result<(), EvalStatus> {
        if value_by_ref {
            // A by-reference foreach whose body suspends would have to keep the alias alive
            // across the suspension; refusing is better than writing through a stale binding.
            return Err(EvalStatus::UnsupportedConstruct);
        }
        let slot = self.foreach_slots;
        self.foreach_slots += 1;
        self.steps.push(EvalGeneratorStep::ForeachInit {
            subject: subject.clone(),
            slot,
        });
        let top = self.steps.len();
        let next_patch = self.steps.len();
        self.steps.push(EvalGeneratorStep::ForeachNext {
            slot,
            key_name: key_name.map(str::to_string),
            value_name: value_name.to_string(),
            exit: usize::MAX,
        });
        self.loops.push(LoopLabels {
            continue_patches: Vec::new(),
            break_patches: Vec::new(),
            is_switch: false,
            region_depth: self.regions.len(),
        });
        self.lower_statements(body)?;
        self.steps.push(EvalGeneratorStep::Jump(top));
        let after = self.steps.len();
        if let Some(EvalGeneratorStep::ForeachNext { exit, .. }) = self.steps.get_mut(next_patch) {
            *exit = after;
        }
        self.close_loop(top, after);
        Ok(())
    }

    /// Resolves the pending `break` and `continue` jumps of the innermost lowered loop.
    fn close_loop(&mut self, continue_target: usize, break_target: usize) {
        let Some(labels) = self.loops.pop() else {
            return;
        };
        for patch in labels.continue_patches {
            self.patch_jump(patch, continue_target);
        }
        for patch in labels.break_patches {
            self.patch_jump(patch, break_target);
        }
    }

    /// Rewrites one placeholder jump target now that the destination index is known.
    fn patch_jump(&mut self, patch: usize, target: usize) {
        match self.steps.get_mut(patch) {
            Some(EvalGeneratorStep::Jump(slot)) => *slot = target,
            Some(EvalGeneratorStep::JumpIfFalse { target: slot, .. }) => *slot = target,
            _ => {}
        }
    }
}

/// Returns whether a statement has to be lowered rather than run atomically.
///
/// A statement is lowered when it contains a suspension point, or when it is a `break` or a
/// `continue` that would otherwise escape the atomic chunk it was batched into.
fn statement_needs_lowering(statement: &EvalStmt) -> bool {
    statement_escapes_with_loop_exit(statement, 0) || statement_contains_yield(statement)
}

/// Returns whether a `return` appears anywhere inside this statement.
///
/// A closure body is not searched: it belongs to another function, and its `return` never leaves
/// the generator. Statements are the only thing walked here because the eval IR keeps a closure
/// inside an EXPRESSION, so a statement walk cannot descend into one by accident.
fn statement_contains_return(statement: &EvalStmt) -> bool {
    match statement {
        EvalStmt::Return(_) => true,
        EvalStmt::If {
            then_branch,
            else_branch,
            ..
        } => {
            then_branch.iter().any(statement_contains_return)
                || else_branch.iter().any(statement_contains_return)
        }
        EvalStmt::While { body, .. }
        | EvalStmt::DoWhile { body, .. }
        | EvalStmt::For { body, .. }
        | EvalStmt::Foreach { body, .. } => body.iter().any(statement_contains_return),
        EvalStmt::Switch { cases, .. } => cases
            .iter()
            .any(|case| case.body.iter().any(statement_contains_return)),
        EvalStmt::Try {
            body,
            catches,
            finally_body,
        } => {
            body.iter().any(statement_contains_return)
                || catches
                    .iter()
                    .any(|catch| catch.body.iter().any(statement_contains_return))
                || finally_body.iter().any(statement_contains_return)
        }
        _ => false,
    }
}

/// Returns whether a `break` or `continue` inside this statement leaves it.
///
/// A `break`/`continue` that stays inside its own loop or switch is the ordinary evaluator's
/// business and the whole statement can be run atomically. One that targets an ENCLOSING lowered
/// loop cannot: the step list owns that jump, so the statement has to be lowered even when it
/// holds no yield at all. `depth` counts the breakable levels between the keyword and the
/// statement being tested, which is exactly what PHP's level argument counts.
fn statement_escapes_with_loop_exit(statement: &EvalStmt, depth: u32) -> bool {
    match statement {
        EvalStmt::Break(level) | EvalStmt::Continue(level) => (*level).max(1) > depth,
        EvalStmt::If {
            then_branch,
            else_branch,
            ..
        } => {
            then_branch
                .iter()
                .any(|inner| statement_escapes_with_loop_exit(inner, depth))
                || else_branch
                    .iter()
                    .any(|inner| statement_escapes_with_loop_exit(inner, depth))
        }
        EvalStmt::While { body, .. }
        | EvalStmt::DoWhile { body, .. }
        | EvalStmt::For { body, .. }
        | EvalStmt::Foreach { body, .. } => body
            .iter()
            .any(|inner| statement_escapes_with_loop_exit(inner, depth + 1)),
        EvalStmt::Switch { cases, .. } => cases.iter().any(|case| {
            case.body
                .iter()
                .any(|inner| statement_escapes_with_loop_exit(inner, depth + 1))
        }),
        EvalStmt::Try {
            body,
            catches,
            finally_body,
        } => {
            body.iter()
                .any(|inner| statement_escapes_with_loop_exit(inner, depth))
                || catches.iter().any(|catch| {
                    catch
                        .body
                        .iter()
                        .any(|inner| statement_escapes_with_loop_exit(inner, depth))
                })
                || finally_body
                    .iter()
                    .any(|inner| statement_escapes_with_loop_exit(inner, depth))
        }
        _ => false,
    }
}

/// Returns whether a statement contains a yield anywhere inside it.
pub(super) fn statement_contains_yield(statement: &EvalStmt) -> bool {
    match statement {
        EvalStmt::Expr(expr) | EvalStmt::Echo(expr) => expression_contains_yield(expr),
        EvalStmt::StoreVar { value, .. } => expression_contains_yield(value),
        EvalStmt::Return(value) => value.as_ref().is_some_and(expression_contains_yield),
        EvalStmt::If {
            condition,
            then_branch,
            else_branch,
        } => {
            expression_contains_yield(condition)
                || then_branch.iter().any(statement_contains_yield)
                || else_branch.iter().any(statement_contains_yield)
        }
        EvalStmt::While { condition, body } => {
            expression_contains_yield(condition) || body.iter().any(statement_contains_yield)
        }
        EvalStmt::DoWhile { body, condition } => {
            expression_contains_yield(condition) || body.iter().any(statement_contains_yield)
        }
        EvalStmt::For {
            init,
            condition,
            update,
            body,
        } => {
            init.iter().any(statement_contains_yield)
                || condition.as_ref().is_some_and(expression_contains_yield)
                || update.iter().any(statement_contains_yield)
                || body.iter().any(statement_contains_yield)
        }
        EvalStmt::Foreach { array, body, .. } => {
            expression_contains_yield(array) || body.iter().any(statement_contains_yield)
        }
        EvalStmt::Switch { expr, cases } => {
            expression_contains_yield(expr)
                || cases.iter().any(|case| {
                    case.condition.as_ref().is_some_and(expression_contains_yield)
                        || case.body.iter().any(statement_contains_yield)
                })
        }
        EvalStmt::Try {
            body,
            catches,
            finally_body,
        } => {
            body.iter().any(statement_contains_yield)
                || catches
                    .iter()
                    .any(|catch| catch.body.iter().any(statement_contains_yield))
                || finally_body.iter().any(statement_contains_yield)
        }
        _ => false,
    }
}

/// Returns whether an expression contains a yield marker anywhere inside it.
///
/// Only the shallow shapes need to be exact: a yield the lowering cannot place is refused, and
/// this predicate exists to decide whether a statement may be run atomically at all. Missing a
/// deeply buried yield would run it through the ordinary evaluator, where the marker resolves
/// to no function and fails loudly rather than silently.
pub(super) fn expression_contains_yield(expr: &EvalExpr) -> bool {
    match expr {
        EvalExpr::Call { name, args } => {
            name == EVAL_YIELD_INTRINSIC
                || name == EVAL_YIELD_FROM_INTRINSIC
                || args.iter().any(|arg| expression_contains_yield(arg.value()))
        }
        EvalExpr::Assign { value, .. } => expression_contains_yield(value),
        EvalExpr::Binary { left, right, .. } => {
            expression_contains_yield(left) || expression_contains_yield(right)
        }
        EvalExpr::Unary { expr, .. } => expression_contains_yield(expr),
        EvalExpr::Match {
            subject,
            arms,
            default,
        } => {
            expression_contains_yield(subject)
                || arms.iter().any(|arm| {
                    arm.patterns.iter().any(expression_contains_yield)
                        || expression_contains_yield(&arm.value)
                })
                || default.as_deref().is_some_and(expression_contains_yield)
        }
        _ => false,
    }
}

/// Returns whether a function body makes its function a generator.
pub(in crate::interpreter) fn eval_body_is_generator(body: &[EvalStmt]) -> bool {
    body.iter().any(statement_contains_yield)
}
