//! Purpose:
//! Pins that a `try` whose only reachable throw is an implicitly run `__destruct` still reaches
//! EIR with its handler installed, and still emits that handler on every supported target.
//!
//! Called from:
//! - The AST-to-EIR unit suite.
//!
//! Key details:
//! - A pruned catch is invisible in the runtime fixture until the program is executed, because
//!   the emitted code is merely missing a handler rather than malformed. These assertions catch
//!   it structurally instead, against the exact source shapes of the `callable_operand_owners`
//!   runtime regressions.
//! - `_exc_handler_top` and `setjmp` are the target-neutral needles
//!   `lower_try_push_handler` emits on every supported target, so the emitter assertion is not
//!   ARM64-first.
//! - The quiet-destructor control pins that the handler is kept because the destructor can
//!   THROW, not because a destructor exists.
//! - The precision fixtures at the end pin the opposite direction: an exactly known retirement
//!   routes only its own destructor class, and a callable whose frame is proven to retire
//!   nothing does not import the program-wide destructor domain. Both are asserted at the EIR
//!   level only, because a NEGATIVE assembly assertion would depend on the symbol-region split
//!   being exact rather than merely generous.

use std::path::Path;

use crate::codegen::platform::Target;
use crate::ir::{Function, Op};

/// The supported target matrix, lowered and emitted independently for each fixture.
const TARGETS: [&str; 5] = [
    "macos-aarch64",
    "ios-arm64",
    "ios-sim-arm64",
    "linux-aarch64",
    "linux-x86_64",
];

/// The direct-call argument-cleanup fixture from `tests/codegen/runtime_gc/callable_operand_owners.rs`.
const THROWING_ARGUMENT_SOURCE: &str = r#"<?php
class ResultPayload {
    public function __destruct() { echo 'result|'; }
}
class ThrowingArgument {
    public function __destruct() { echo 'argument|'; throw new RuntimeException('cleanup'); }
}
class ResultFactory {
    public function build(ThrowingArgument $marker): ResultPayload { return new ResultPayload(); }
}
function buildResult(ThrowingArgument $marker): ResultPayload { return new ResultPayload(); }
function buildWithSameFrameCatch(): string {
    try {
        buildResult(new ThrowingArgument());
        echo 'unreached|';
        return 'no';
    } catch (RuntimeException $error) {
        echo 'caught|';
        return 'fn';
    }
}
function buildThroughMethodWithSameFrameCatch(): string {
    $factory = new ResultFactory();
    try {
        $factory->build(new ThrowingArgument());
        echo 'unreached|';
        return 'no';
    } catch (RuntimeException $error) {
        echo 'caught|';
        return 'method';
    }
}
echo buildWithSameFrameCatch(), ':', buildThroughMethodWithSameFrameCatch();
"#;

/// The immediately invoked closure fixture from the same runtime regression file.
const IIFE_CAPTURE_SOURCE: &str = r#"<?php
class ThrowingCapture {
    public function __destruct() { echo 'capture|'; throw new RuntimeException('capture'); }
}
class ResultPayload {
    public function __destruct() { echo 'result|'; }
}
function dropHolder(array &$slot): int { $slot = []; return 1; }
function invokeIifeWithLastOwnedCapture(): string {
    $held = [new ThrowingCapture()];
    try {
        (function (int $ignored) use ($held): ResultPayload { return new ResultPayload(); })(
            dropHolder($held)
        );
        echo 'unreached|';
        return 'no';
    } catch (RuntimeException $error) {
        echo 'caught|';
        return 'ok';
    }
}
echo invokeIifeWithLastOwnedCapture(), ':', invokeIifeWithLastOwnedCapture();
"#;

/// A quiet destructor over the same shape, which must still lose its unreachable handler.
const QUIET_DESTRUCTOR_SOURCE: &str = r#"<?php
class ResultPayload {
    public function __destruct() { echo 'result|'; }
}
class QuietArgument {
    public function __destruct() { echo 'argument|'; }
}
function buildResult(QuietArgument $marker): ResultPayload { return new ResultPayload(); }
function buildWithSameFrameCatch(): string {
    try {
        buildResult(new QuietArgument());
        return 'no';
    } catch (RuntimeException $error) {
        return 'fn';
    }
}
echo buildWithSameFrameCatch();
"#;

/// Lowers `source` for `target` and hands back the whole module plus one named function.
fn lower_function(source: &str, target: &str, name: &str) -> (crate::ir::Module, Function) {
    let module = super::lower_source_at_for_target(
        source,
        Path::new("main.php"),
        Path::new("."),
        Target::parse(target).unwrap(),
    );
    let function = module
        .functions
        .iter()
        .find(|function| function.name.eq_ignore_ascii_case(name))
        .unwrap_or_else(|| panic!("{target}: {name} is lowered"))
        .clone();
    (module, function)
}

/// Returns the emitted assembly for one lowered function's own symbol region.
///
/// The region runs from the function's label to the next global symbol directive, which is how
/// every supported target separates emitted functions.
fn function_assembly(module: &crate::ir::Module, name: &str, target: &str) -> String {
    let asm = crate::codegen::generate_user_asm_from_ir(module, false, false)
        .unwrap_or_else(|error| panic!("{target}: {error:?}"));
    let symbol = crate::names::function_symbol(name);
    let label = format!("{symbol}:");
    let start = asm
        .find(&label)
        .unwrap_or_else(|| panic!("{target}: {symbol} must be emitted:\n{asm}"))
        + label.len();
    let body = &asm[start..];
    let end = body.find("\n.globl ").unwrap_or(body.len());
    body[..end].to_string()
}

/// Verifies a throwing argument-temporary destructor keeps its handler through EIR and assembly.
///
/// Without destructor-aware exception flow the whole `try` statement is removed before lowering,
/// so the emitted function carries no `TryPushHandler`, no `setjmp`, and no catch block, and the
/// destructor's throw becomes a fatal instead of being caught in frame.
#[test]
fn throwing_argument_destructor_keeps_its_same_frame_handler_on_every_target() {
    for target in TARGETS {
        for name in ["buildWithSameFrameCatch", "buildThroughMethodWithSameFrameCatch"] {
            let (module, function) = lower_function(THROWING_ARGUMENT_SOURCE, target, name);
            assert!(
                function.instructions.iter().any(|inst| inst.op == Op::TryPushHandler),
                "{target}: {name} must install an EIR handler",
            );
            let body = function_assembly(&module, name, target);
            assert!(
                body.contains("_exc_handler_top"),
                "{target}: {name} must push a handler in assembly:\n{body}",
            );
            assert!(
                body.contains("setjmp"),
                "{target}: {name} must arm its handler in assembly:\n{body}",
            );
        }
    }
}

/// Verifies an immediately invoked closure's capture cleanup keeps its handler on every target.
///
/// The closure descriptor is retired by the call site and may hold the last owner of the
/// captured array, so the `try` around the invocation stays reachable.
#[test]
fn immediately_invoked_closure_capture_cleanup_keeps_its_handler_on_every_target() {
    for target in TARGETS {
        let (module, function) =
            lower_function(IIFE_CAPTURE_SOURCE, target, "invokeIifeWithLastOwnedCapture");
        assert!(
            function.instructions.iter().any(|inst| inst.op == Op::TryPushHandler),
            "{target}: the IIFE capture cleanup must install an EIR handler",
        );
        let body = function_assembly(&module, "invokeIifeWithLastOwnedCapture", target);
        assert!(
            body.contains("_exc_handler_top") && body.contains("setjmp"),
            "{target}: the IIFE handler must survive to assembly:\n{body}",
        );
    }
}

/// Verifies a quiet destructor still lets the unreachable handler be pruned on every target.
///
/// This is the emitter-side control: the model must not install a handler merely because the
/// program declares a destructor.
#[test]
fn quiet_destructor_still_prunes_its_unreachable_handler_on_every_target() {
    for target in TARGETS {
        let (_, function) = lower_function(QUIET_DESTRUCTOR_SOURCE, target, "buildWithSameFrameCatch");
        assert!(
            !function.instructions.iter().any(|inst| inst.op == Op::TryPushHandler),
            "{target}: a quiet destructor must not resurrect the handler",
        );
    }
}

/// Two classes whose destructors throw disjoint exception classes, exercised through EIR.
///
/// `makeBeta()` exists so `Beta` stays a reachable declaration: the point of the fixture is that
/// the program-wide destructor summary is strictly larger than the exact one, which it is not if
/// `Beta` never survives to the analysis.
const DISJOINT_DESTRUCTOR_SOURCE: &str = r#"<?php
class AlphaFailure extends Exception {}
class BetaFailure extends Exception {}
class Alpha {
    public function __destruct() { echo 'alpha|'; throw new AlphaFailure('alpha'); }
}
class Beta {
    public function __destruct() { echo 'beta|'; throw new BetaFailure('beta'); }
}
function makeBeta(): Beta { return new Beta(); }
function dropAlphaWithBetaCatch(): string {
    try {
        new Alpha();
        return 'no';
    } catch (BetaFailure $error) {
        return 'beta';
    }
}
function dropAlphaWithAlphaCatch(): string {
    try {
        new Alpha();
        return 'no';
    } catch (AlphaFailure $error) {
        return 'alpha';
    }
}
makeBeta();
echo dropAlphaWithBetaCatch(), dropAlphaWithAlphaCatch();
"#;

/// A scalar helper called inside a `try`, in a program that also holds a throwing destructor.
const SCALAR_HELPER_SOURCE: &str = r#"<?php
class CleanupFailure extends Exception {}
class Detonator {
    public function __destruct() { echo 'boom|'; throw new CleanupFailure('boom'); }
}
function twice(int $value): int { return $value * 2; }
function callScalarHelper(): int {
    try {
        return twice(2);
    } catch (CleanupFailure $error) {
        return 0;
    }
}
try { $held = new Detonator(); unset($held); } catch (CleanupFailure $error) { echo 'outer|'; }
echo callScalarHelper();
"#;

/// Verifies an exactly known retirement routes only its own destructor class, through EIR.
///
/// Both handlers sit over the identical retirement, so the pair pins routing rather than the
/// mere presence of a destruction term: the disjoint handler must be gone and the matching one
/// must survive to EIR on every supported target.
#[test]
fn exact_class_retirement_routes_only_its_own_failure_on_every_target() {
    for target in TARGETS {
        let (_, disjoint) =
            lower_function(DISJOINT_DESTRUCTOR_SOURCE, target, "dropAlphaWithBetaCatch");
        assert!(
            !disjoint
                .instructions
                .iter()
                .any(|inst| inst.op == Op::TryPushHandler),
            "{target}: an Alpha retirement must not install a BetaFailure handler",
        );
        let (_, matching) =
            lower_function(DISJOINT_DESTRUCTOR_SOURCE, target, "dropAlphaWithAlphaCatch");
        assert!(
            matching
                .instructions
                .iter()
                .any(|inst| inst.op == Op::TryPushHandler),
            "{target}: an Alpha retirement must install its own AlphaFailure handler",
        );
    }
}

/// Verifies a proven scalar callee does not resurrect a handler, through EIR.
///
/// `twice()` takes a scalar by value and binds nothing, so its frame teardown cannot run a
/// destructor. If its summary imported the program-wide destructor domain instead, the handler
/// in `callScalarHelper()` would reach EIR and every supported target would emit a `setjmp`
/// prologue for a path no throw can enter.
#[test]
fn a_proven_scalar_callee_does_not_resurrect_a_handler_on_every_target() {
    for target in TARGETS {
        let (_, function) = lower_function(SCALAR_HELPER_SOURCE, target, "callScalarHelper");
        assert!(
            !function
                .instructions
                .iter()
                .any(|inst| inst.op == Op::TryPushHandler),
            "{target}: a proven scalar callee must not resurrect the handler",
        );
    }
}
