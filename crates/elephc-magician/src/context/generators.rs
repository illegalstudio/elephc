//! Purpose:
//! Owns the execution state of every live PHP generator, keyed by its object identity.
//!
//! Called from:
//! - `crate::interpreter::generators` for creation, resumption and the Generator protocol.
//!
//! Key details:
//! - A generator's state has to outlive the call that created it, so it lives here rather than
//!   on the stack of the function that produced it. That includes the generator's SCOPE: the
//!   ordinary call path drops its scope at return, which a generator cannot do because its
//!   locals must survive every suspension.
//! - The step list is a flat program with an explicit counter. Resuming is then continuing at
//!   the stored index, with no need to re-descend a statement tree or to replay side effects.
//! - A frame is TAKEN OUT while it runs and put back afterwards, because the body it executes
//!   needs the context mutably and the frame lives inside that context. A generator that tries
//!   to resume itself therefore finds itself absent and fails, rather than corrupting its frame.

use super::*;
use crate::eval_ir::{EvalExpr, EvalStmt};

/// One step of a lowered generator body.
#[derive(Clone)]
pub enum EvalGeneratorStep {
    /// Execute a yield-free statement subtree with the ordinary evaluator.
    Run(Vec<EvalStmt>),
    /// Evaluate the key and value, suspend, then store what `send()` passed into `into`.
    Yield {
        key: Option<EvalExpr>,
        value: EvalExpr,
        into: Option<String>,
    },
    /// Delegate to an array or another generator, then store its return value into `into`.
    YieldFrom {
        source: EvalExpr,
        into: Option<String>,
    },
    /// Jump when the condition is falsy.
    JumpIfFalse { condition: EvalExpr, target: usize },
    /// Jump unconditionally.
    Jump(usize),
    /// Finish the generator, optionally with a `getReturn()` value.
    Return(Option<EvalExpr>),
    /// Materialize the subject of a `foreach` into the frame's iteration slot.
    ForeachInit { subject: EvalExpr, slot: usize },
    /// Bind the next `foreach` pair into the scope, or jump to `exit` when exhausted.
    ForeachNext {
        slot: usize,
        key_name: Option<String>,
        value_name: String,
        exit: usize,
    },
    /// Open a `try` region: a throw raised before the matching `LeaveTry` stores the thrown
    /// value in `thrown_slot` and continues at `handler` instead of ending the generator.
    EnterTry {
        handler: usize,
        /// Step index of a standalone copy of this region's `finally`, run when a suspended
        /// generator is destroyed instead of resumed.
        finally_entry: usize,
        thrown_slot: String,
    },
    /// Close the innermost `try` region because control left it without raising.
    LeaveTry,
    /// Re-raise the value a handler stored, once its `finally` copy has run.
    Rethrow { thrown_slot: String },
    /// End one region's destruction-time `finally` copy.
    EndUnwind,
}

/// One `try` region a generator body is currently inside.
///
/// A suspended generator can be resumed anywhere, so the regions it is inside cannot live on the
/// Rust stack of whatever called `next()`. They live on the frame, which is what makes a `yield`
/// inside a `try` possible at all.
pub struct EvalGeneratorRegion {
    /// Step index of the catch dispatch for this region.
    pub handler: usize,
    /// Step index of this region's destruction-time `finally` copy.
    pub finally_entry: usize,
    /// Scope name the thrown value is stored under before the handler runs.
    pub thrown_slot: String,
}

/// A lowered generator body plus the number of foreach slots its frame must hold.
pub struct EvalGeneratorProgram {
    pub steps: Vec<EvalGeneratorStep>,
    pub foreach_slots: usize,
}

/// What a running generator is currently delegating to, for `yield from`.
pub enum EvalGeneratorDelegate {
    /// A materialized array, walked by position so the inner keys are preserved.
    Array {
        array: RuntimeCellHandle,
        position: usize,
    },
    /// Another generator, pumped through the same protocol.
    Generator { identity: u64 },
    /// A Traversable the interpreter does NOT own -- a compiled generator, or any Iterator --
    /// pumped through its php-visible `valid`/`current`/`key`/`next` methods.
    ///
    /// A compiled class whose generator method is inherited by an eval-declared subclass produces
    /// exactly this: `twig/twig`'s `Template::yield()` is compiled, the template's `doDisplay()` is
    /// interpreted, and each one delegates to the other. Without this the delegation could not
    /// start, and the outer loop re-read the same first value for ever.
    Foreign {
        /// The boxed cell, retained so the delegate cannot be freed mid-iteration.
        object: RuntimeCellHandle,
        /// The generator object itself, which is what `__rt_gen_*` takes -- passing the boxed
        /// cell instead read a header that is not a generator's and reported "finished" on the
        /// first `valid()`, so the delegation ended before producing anything.
        generator: u64,
    },
}

/// Where a generator is in its lifecycle.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum EvalGeneratorState {
    NotStarted,
    Suspended,
    Finished,
}

/// The naming scope a generator body must run under, whichever call later resumes it.
///
/// A generator is resumed from anywhere — a `foreach` in another class, a `->next()` call at top
/// level — so the class scope that was current when it was CREATED cannot be read off the stack
/// at resume time. PHP keeps it on the generator, and so does this: without it `self::`,
/// `static::`, `__CLASS__` and private-member access inside a generator method would resolve
/// against whoever happened to call `next()`.
pub struct EvalGeneratorActivation {
    /// Name pushed for the function stack, and the key static locals persist under.
    pub function_name: String,
    /// Declaring class, pushed as the class scope while the body runs.
    pub class_scope: Option<String>,
    /// Late-static-binding class for `static::`.
    pub called_class: Option<String>,
    /// The magic-constant frame, present only where the creating call pushed one.
    ///
    /// A method pushes one; a plain function and a closure do not, and inventing one for them
    /// here would change `__FUNCTION__` and `__METHOD__` relative to the ordinary call path.
    pub magic: Option<EvalGeneratorMagicScope>,
    /// The source file, directory, line and `__FILE__` override the body was written in.
    ///
    /// The resumer is arbitrary -- compiled code driving a `foreach` over an interpreted Twig
    /// template runs `doDisplay()` with the AOT caller's file installed -- so a body that reads
    /// `__FILE__`, or a frame built while it runs, would name the resumer's file. Captured when
    /// the generator is created, installed on every resumption, and updated with the line the
    /// body reached when it suspends. `None` keeps the resumer's site, as before.
    pub site: Option<EvalCallSite>,
}

/// A source position as `ElephcEvalContext::call_site()` reports it: file, directory, line and
/// the `__FILE__` override.
pub type EvalCallSite = (String, String, i64, Option<String>);

/// The `__FUNCTION__` / `__METHOD__` / `__CLASS__` / `__TRAIT__` frame of a generator method.
pub struct EvalGeneratorMagicScope {
    pub function_name: String,
    pub method_name: String,
    pub class_name: Option<String>,
    pub trait_name: Option<String>,
}

/// One generator's whole execution state.
pub struct EvalGeneratorFrame {
    pub program: EvalGeneratorProgram,
    /// The naming scope re-established around every resumption.
    pub activation: EvalGeneratorActivation,
    pub step: usize,
    pub scope: Box<ElephcEvalScope>,
    pub state: EvalGeneratorState,
    pub auto_key: i64,
    pub current_key: Option<RuntimeCellHandle>,
    pub current_value: Option<RuntimeCellHandle>,
    pub return_value: Option<RuntimeCellHandle>,
    pub foreach_slots: Vec<Option<(RuntimeCellHandle, usize)>>,
    /// The `try` regions the body is currently inside, innermost last.
    pub regions: Vec<EvalGeneratorRegion>,
    pub delegate: Option<EvalGeneratorDelegate>,
    /// The scope name the next `send()` value is stored under, set by the suspended yield.
    pub pending_send_slot: Option<String>,
    /// Set once the generator has advanced past its first yield, which `rewind()` refuses.
    pub advanced: bool,
}

impl ElephcEvalContext {
    /// Stores one newly created generator's frame under its object identity.
    pub fn register_eval_generator(&mut self, identity: u64, frame: EvalGeneratorFrame) {
        self.eval_generators.insert(identity, frame);
        // ALSO publish the owner context. `Generator` is an AOT class, so a generator the
        // INTERPRETER created is indistinguishable from an AOT one at an AOT call site, and
        // `lower_method_call`'s dynamic-ownership probe finds no owner, falls through to the
        // native Iterator protocol, and that switches onto an activation stack this generator
        // never had -- measured as `ret` into 0x4 with sp inside heap_buf. Publishing ownership
        // routes the call through the bridge, where `eval_generator_method_result` resumes the
        // frame registered just above, by the same identity.
        //
        // Deliberately NOT `register_dynamic_object`: that also records the object as an
        // instance of an EVAL-DECLARED class, and `Generator` is not one.
        #[cfg(not(test))]
        crate::ffi::dynamic_destructors::register_dynamic_object_context(
            identity,
            std::ptr::from_mut::<Self>(self),
        );
    }

    /// Returns whether one object identity is a generator this context owns.
    pub fn has_eval_generator(&self, identity: u64) -> bool {
        self.eval_generators.contains_key(&identity)
    }

    /// Removes one generator's frame so its body can run with the context borrowed mutably.
    pub fn take_eval_generator(&mut self, identity: u64) -> Option<EvalGeneratorFrame> {
        self.eval_generators.remove(&identity)
    }

    /// Puts a running generator's frame back once its body has suspended or finished.
    pub fn restore_eval_generator(&mut self, identity: u64, frame: EvalGeneratorFrame) {
        self.eval_generators.insert(identity, frame);
    }

    /// Reads one field of a generator's frame without taking it out.
    pub fn eval_generator<T>(
        &self,
        identity: u64,
        read: impl FnOnce(&EvalGeneratorFrame) -> T,
    ) -> Option<T> {
        self.eval_generators.get(&identity).map(read)
    }
}
