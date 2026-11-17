//! Purpose:
//! Models how php-src evaluates a constant expression it stores (a class or interface
//! constant, a property default, a parameter default, a trait-collision default), so a
//! deprecated constant read inside one reports at php-src's line and as often as php-src
//! reports it.
//!
//! Called from:
//! - `crate::ir_lower::expr::constants::lower_const_ref()` and the scoped-constant lowering,
//!   which push a frame while they inline a class constant's initializer.
//! - `crate::ir_lower::initializer_reads`, which walks an initializer's reads without lowering
//!   its value (class initialization, reflection, `get_class_vars()`).
//! - The reflection-default helpers in `crate::ir_lower::function`.
//!
//! Key details:
//! - With a class scope, `zend_ast_evaluate_ex` sets `EG(lineno_override)` to the line of
//!   every node it evaluates, and a `Class::CONST` node keeps it while the constant it names is
//!   evaluated. An initializer that is one bare global constant skips that walk
//!   (`zval_update_constant_with_ctx`'s fast path) and reports at the enclosing line: the line
//!   of the `Class::CONST` node that reached it, or the executing line. Without a class scope
//!   no override is set, so every read reports at the executing line.
//! - php-src evaluates a class constant once and caches it. Its reads report on that first
//!   evaluation only, behind a program-wide flag keyed by the constant (the class that holds
//!   it, so a trait constant counts once per using class) and the read's own span.

use std::cell::RefCell;

use crate::ir::{Immediate, Op};
use crate::ir_lower::context::LoweringContext;
use crate::parser::ast::{Expr, ExprKind};
use crate::span::Span;
use crate::types::PhpType;

/// Where the executing line of an evaluation comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LineSource {
    /// A source line known while lowering: the read, `new`, or call that started it.
    Static(Span),
    /// The nearest PHP call site at run time, for an evaluation inside a builtin method
    /// (`ReflectionClassConstant::getValue()`), where php-src reports the caller's line.
    Caller,
}

/// One initializer evaluation php-src performs.
#[derive(Clone, Debug)]
pub(crate) struct InitializerFrame {
    /// Whether php-src evaluates the initializer with a class scope, which sets a per-node
    /// line override.
    scoped: bool,
    /// The executing line.
    base: LineSource,
    /// Line override inherited from the `Class::CONST` node whose constant this evaluates.
    override_line: Option<Span>,
    /// Span of the initializer when it is one bare global constant (the fast path).
    lone_global: Option<Span>,
    /// Once-flag key of a cached evaluation, or `None` when every evaluation reports.
    once: Option<String>,
}

impl InitializerFrame {
    /// Returns the frame of a top-level evaluation whose initializer is one bare global
    /// constant at `lone_global` (after php-src's compile-time folding), if it is.
    pub(crate) fn top(scoped: bool, base: LineSource, lone_global: Option<Span>, once: Option<String>) -> Self {
        Self { scoped, base, override_line: None, lone_global, once }
    }

    /// [`Self::top`] for an initializer php-src does not fold at compile time (a parameter
    /// default, compiled without constant substitution): bare only when written bare.
    pub(crate) fn top_unfolded(scoped: bool, base: LineSource, root: &Expr, once: Option<String>) -> Self {
        Self::top(scoped, base, lone_global_span(root), once)
    }

    /// Returns the frame of the first evaluation of class constant `owner::name`, reached by a
    /// `Class::CONST` node at `read` inside `outer`, or by code outside any initializer when
    /// `outer` is `None`. `lone_global` is the constant's initializer when it is one bare
    /// global constant after compile-time folding.
    pub(crate) fn class_constant(
        outer: Option<&InitializerFrame>,
        read: Span,
        owner: &str,
        name: &str,
        lone_global: Option<Span>,
    ) -> Self {
        let (base, override_line) = match outer {
            None => (LineSource::Static(read), None),
            Some(outer) if outer.scoped => (outer.base, Some(read)),
            Some(outer) => (outer.base, outer.override_line),
        };
        Self {
            scoped: true,
            base,
            override_line,
            lone_global,
            once: Some(class_constant_once_key(owner, name)),
        }
    }

    /// Returns the line a deprecated global constant read at `span` reports.
    pub(crate) fn global_read_line(&self, span: Span) -> LineSource {
        if self.lone_global == Some(span) || !self.scoped || !span.identifies_a_node() {
            return self.inherited_line();
        }
        LineSource::Static(span)
    }

    /// Returns the line a `Class::CONST` node at `span` reports (and evaluates its constant
    /// at).
    pub(crate) fn node_line(&self, span: Span) -> LineSource {
        if self.scoped && span.identifies_a_node() {
            return LineSource::Static(span);
        }
        self.inherited_line()
    }

    /// The enclosing line: an inherited override, else the executing line.
    fn inherited_line(&self) -> LineSource {
        self.override_line.map(LineSource::Static).unwrap_or(self.base)
    }
}

/// Returns `root`'s span when it is one bare global constant read.
pub(crate) fn lone_global_span(root: &Expr) -> Option<Span> {
    matches!(root.kind, ExprKind::ConstRef(_)).then_some(root.span)
}

/// Returns the once-flag key of class constant `owner::name`.
pub(crate) fn class_constant_once_key(owner: &str, name: &str) -> String {
    format!("{}::{}", owner.trim_start_matches('\\').to_ascii_lowercase(), name)
}

thread_local! {
    /// Initializer evaluations being lowered, innermost last.
    static FRAMES: RefCell<Vec<InitializerFrame>> = const { RefCell::new(Vec::new()) };
}

/// Runs `f` while `frame` is the innermost initializer evaluation.
pub(crate) fn with_frame<T>(frame: InitializerFrame, f: impl FnOnce() -> T) -> T {
    /// Pops the frame when the scope exits, including on unwind.
    struct Pop;
    impl Drop for Pop {
        /// Leaves the frame pushed by the enclosing call.
        fn drop(&mut self) {
            FRAMES.with(|frames| frames.borrow_mut().pop());
        }
    }
    FRAMES.with(|frames| frames.borrow_mut().push(frame));
    let _pop = Pop;
    f()
}

/// Returns the innermost initializer evaluation being lowered, if any.
pub(crate) fn current_frame() -> Option<InitializerFrame> {
    FRAMES.with(|frames| frames.borrow().last().cloned())
}

/// Emits the deprecation of a global constant read at `span` inside `frame`.
pub(crate) fn emit_global_read(
    ctx: &mut LoweringContext<'_, '_>,
    frame: &InitializerFrame,
    message: &str,
    span: Span,
) {
    emit_framed(ctx, frame, message, span, frame.global_read_line(span));
}

/// Emits the deprecation of a deprecated class constant read by a node at `span` inside
/// `frame`.
pub(crate) fn emit_node_read(
    ctx: &mut LoweringContext<'_, '_>,
    frame: &InitializerFrame,
    message: &str,
    span: Span,
) {
    emit_framed(ctx, frame, message, span, frame.node_line(span));
}

/// Emits one framed read: behind the frame's once flag for that read, or every time.
fn emit_framed(
    ctx: &mut LoweringContext<'_, '_>,
    frame: &InitializerFrame,
    message: &str,
    span: Span,
    line: LineSource,
) {
    if crate::ir_lower::expr::constant_deprecations_suppressed() {
        return;
    }
    let Some(key) = &frame.once else {
        emit_deprecation_at(ctx, message, span, line);
        return;
    };
    let label = once_label(key, span);
    let message = message.to_string();
    emit_until_success(ctx, &label, span, |ctx| {
        emit_deprecation_at(ctx, &message, span, line);
    });
}

/// Returns the program-wide flag symbol of the read at `span` inside the cached evaluation
/// `key`. The key is hashed so any class or constant spelling yields a valid symbol.
pub(crate) fn once_label(key: &str, span: Span) -> String {
    flag_label("_constant_deprecation_once", &format!("{key}@{}:{}:{}", span.source_id(), span.line, span.col))
}

/// Returns a program-wide flag symbol named by `prefix` and a hash of `text`.
pub(crate) fn flag_label(prefix: &str, text: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{prefix}_{hash:016x}")
}

/// Emits a read-only test of program-wide flag `label`.
fn emit_flag_test(ctx: &mut LoweringContext<'_, '_>, label: &str, span: Span) -> crate::ir::ValueId {
    let data = ctx.intern_string(label);
    ctx.builder
        .emit_with_effects(
            Op::IncludeOnceTest,
            Vec::new(),
            Some(Immediate::Data(data)),
            crate::ir::IrType::I64,
            PhpType::Bool,
            crate::ir::Ownership::NonHeap,
            Op::IncludeOnceTest.default_effects(),
            Some(span),
        )
        .expect("a flag test produces a branch condition")
}

/// Runs `body` while program-wide flag `label` is clear, and sets the flag once `body` completes.
/// php-src marks a class initialized, a constant evaluated, or a default cached only after the
/// evaluation succeeds: when an error handler throws from one of its notices, the next attempt
/// evaluates again and reports again.
pub(crate) fn emit_until_success(
    ctx: &mut LoweringContext<'_, '_>,
    label: &str,
    span: Span,
    body: impl FnOnce(&mut LoweringContext<'_, '_>),
) {
    emit_unless_any_flag_set(ctx, &[label.to_string()], span, |ctx| {
        body(ctx);
        emit_flag_mark(ctx, label, span);
    });
}

/// Sets program-wide flag `label`.
pub(crate) fn emit_flag_mark(ctx: &mut LoweringContext<'_, '_>, label: &str, span: Span) {
    let data = ctx.intern_string(label);
    ctx.emit_void(Op::IncludeOnceMark, Vec::new(), Some(Immediate::Data(data)), Op::IncludeOnceMark.default_effects(), Some(span));
}

/// Runs `body` only when program-wide flag `label` is set.
pub(crate) fn emit_if_flag_set(
    ctx: &mut LoweringContext<'_, '_>,
    label: &str,
    span: Span,
    body: impl FnOnce(&mut LoweringContext<'_, '_>),
) {
    let set = emit_flag_test(ctx, label, span);
    let then = ctx.builder.create_named_block("flag.set", Vec::new());
    let after = ctx.builder.create_named_block("flag.after", Vec::new());
    ctx.builder.terminate(crate::ir::Terminator::CondBr {
        cond: set,
        then_target: then,
        then_args: Vec::new(),
        else_target: after,
        else_args: Vec::new(),
    });
    ctx.builder.position_at_end(then);
    body(ctx);
    ctx.builder.terminate(crate::ir::Terminator::Br { target: after, args: Vec::new() });
    ctx.builder.position_at_end(after);
}

/// Runs `body` only while every program-wide flag in `labels` is still clear.
pub(crate) fn emit_unless_any_flag_set(
    ctx: &mut LoweringContext<'_, '_>,
    labels: &[String],
    span: Span,
    body: impl FnOnce(&mut LoweringContext<'_, '_>),
) {
    if labels.is_empty() {
        body(ctx);
        return;
    }
    let after = ctx.builder.create_named_block("flags.after", Vec::new());
    for label in labels {
        let set = emit_flag_test(ctx, label, span);
        let clear = ctx.builder.create_named_block("flag.clear", Vec::new());
        ctx.builder.terminate(crate::ir::Terminator::CondBr {
            cond: set,
            then_target: after,
            then_args: Vec::new(),
            else_target: clear,
            else_args: Vec::new(),
        });
        ctx.builder.position_at_end(clear);
    }
    body(ctx);
    ctx.builder.terminate(crate::ir::Terminator::Br { target: after, args: Vec::new() });
    ctx.builder.position_at_end(after);
}

/// Emits PHP's `E_DEPRECATED` for `message` at `line`; `span` locates the instructions.
pub(crate) fn emit_deprecation_at(
    ctx: &mut LoweringContext<'_, '_>,
    message: &str,
    span: Span,
    line: LineSource,
) {
    let line_value = match line {
        LineSource::Static(line_span) => {
            crate::ir_lower::expr::emit_constant_deprecation(ctx, message, line_span);
            return;
        }
        LineSource::Caller => ctx.emit_value(
            Op::CoreBuiltin,
            Vec::new(),
            Some(Immediate::I64(crate::ir::CoreBuiltinOp::DiagnosticCallerLine.as_i64())),
            PhpType::Int,
            crate::ir::CoreBuiltinOp::DiagnosticCallerLine.effects(),
            Some(span),
        ),
    };
    crate::ir_lower::expr::emit_constant_deprecation_with_line(ctx, message, span, line_value, true);
}
