//! Purpose:
//! Regression tests for destructor-aware catch routing in post-typecheck DCE.
//! Pins that an implicitly run `__destruct` which throws keeps the handler it can reach, and
//! that every pruning control which does not involve a throwing destructor is unchanged.
//!
//! Called from:
//! - `crate::optimize::tests` through Rust's test harness.
//!
//! Key details:
//! - Fixtures are parsed from PHP source because a hand-built `ClassDecl` would not exercise
//!   the `__destruct` collection path the analysis depends on.
//! - These fixtures run WITHOUT checker metadata, so `function_returns` is empty and a call
//!   result falls back to the program-wide destructor summary. That summary is the gate: it is
//!   empty unless the program holds a destructor that can throw, which is exactly what the
//!   negative fixtures pin.
//! - Thrown classes are user-declared subclasses of `Exception` so the no-metadata hierarchy
//!   can prove their constructors quiet and the assertions are about routing, not about an
//!   external class family staying conservative.

use super::*;

/// Parses PHP source and runs post-typecheck dead-code elimination over it.
fn dce_source(source: &str) -> Program {
    let tokens = crate::lexer::tokenize(source).expect("fixture must tokenize");
    eliminate_dead_code(crate::parser::parse(&tokens).expect("fixture must parse"))
}

/// Returns whether any statement in the program, at any depth, is a `try` with a catch clause.
fn has_catch(stmts: &[Stmt]) -> bool {
    stmts.iter().any(stmt_has_catch)
}

/// Returns whether one statement is, or encloses, a `try` that still has a catch clause.
fn stmt_has_catch(stmt: &Stmt) -> bool {
    match &stmt.kind {
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            !catches.is_empty()
                || has_catch(try_body)
                || catches.iter().any(|catch| has_catch(&catch.body))
                || finally_body.as_deref().is_some_and(has_catch)
        }
        StmtKind::FunctionDecl { body, .. }
        | StmtKind::Synthetic(body)
        | StmtKind::NamespaceBlock { body, .. }
        | StmtKind::IncludeOnceGuard { body, .. }
        | StmtKind::While { body, .. }
        | StmtKind::DoWhile { body, .. }
        | StmtKind::Foreach { body, .. }
        | StmtKind::For { body, .. } => has_catch(body),
        StmtKind::ClassDecl { methods, .. } => {
            methods.iter().any(|method| has_catch(&method.body))
        }
        StmtKind::If {
            condition: _,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            has_catch(then_body)
                || elseif_clauses.iter().any(|(_, body)| has_catch(body))
                || else_body.as_deref().is_some_and(has_catch)
        }
        StmtKind::Switch { cases, default, .. } => {
            cases.iter().any(|(_, body)| has_catch(body))
                || default.as_deref().is_some_and(has_catch)
        }
        _ => false,
    }
}

/// Source shared by the fixtures that need a destructor which throws a user exception class.
const THROWING_DESTRUCTOR_DECLARATIONS: &str = r#"
class CleanupFailure extends Exception {}
class Payload {}
class Detonator { public function __destruct() { throw new CleanupFailure('boom'); } }
"#;

/// Verifies a throwing argument-temporary destructor keeps a same-frame catch alive.
///
/// The callee body cannot throw, so the only reachable throw is the argument temporary's
/// `__destruct`, which PHP runs in the CALLER's frame after the call returns. This is the exact
/// shape of the `callable_operand_owners` direct-call runtime regression.
#[test]
fn test_throwing_argument_temporary_destructor_preserves_a_same_frame_catch() {
    let program = dce_source(&format!(
        "<?php{THROWING_DESTRUCTOR_DECLARATIONS}
function buildResult(Detonator $marker): Payload {{ return new Payload(); }}
function run(): string {{
    try {{ buildResult(new Detonator()); return 'no'; }}
    catch (CleanupFailure $error) {{ return 'caught'; }}
}}
echo run();"
    ));

    assert!(
        has_catch(&program),
        "an argument destructor that throws must keep its same-frame catch: {program:?}"
    );
}

/// Verifies a throwing destructor on a fresh array child keeps the same-frame catch alive.
///
/// Retiring the container retires the children it owns, so the element is the destruction site
/// rather than the array literal itself.
#[test]
fn test_throwing_destructor_inside_an_argument_array_literal_preserves_the_catch() {
    let program = dce_source(&format!(
        "<?php{THROWING_DESTRUCTOR_DECLARATIONS}
function buildResult(array $markers): Payload {{ return new Payload(); }}
function run(): string {{
    try {{ buildResult([new Detonator()]); return 'no'; }}
    catch (CleanupFailure $error) {{ return 'caught'; }}
}}
echo run();"
    ));

    assert!(
        has_catch(&program),
        "a fresh array child destructor must keep its same-frame catch: {program:?}"
    );
}

/// Verifies an immediately invoked closure's by-value captures keep a same-frame catch alive.
///
/// The descriptor is created and retired by this call site, so it may hold the last owner of a
/// captured value; retiring it runs that value's destructor in this frame.
#[test]
fn test_immediately_invoked_closure_captures_preserve_a_catch_when_a_destructor_throws() {
    let program = dce_source(&format!(
        "<?php{THROWING_DESTRUCTOR_DECLARATIONS}
function run(): string {{
    $held = new Detonator();
    try {{
        (function () use ($held): Payload {{ return new Payload(); }})();
        return 'no';
    }} catch (CleanupFailure $error) {{ return 'caught'; }}
}}
echo run();"
    ));

    assert!(
        has_catch(&program),
        "an immediately invoked closure's captures must keep its catch: {program:?}"
    );
}

/// Verifies retiring a variable through `unset` keeps a same-frame catch alive.
///
/// `unset` retires whatever the name held, and that retirement runs the destructor in this
/// frame. Without modelling it, a `try` whose only throw source is the retired value loses its
/// handler before lowering.
#[test]
fn test_unsetting_a_possibly_last_owner_preserves_the_catch() {
    let program = dce_source(&format!(
        "<?php{THROWING_DESTRUCTOR_DECLARATIONS}
function run(): string {{
    $held = new Detonator();
    try {{ unset($held); return 'no'; }}
    catch (CleanupFailure $error) {{ return 'caught'; }}
}}
echo run();"
    ));

    assert!(
        has_catch(&program),
        "unsetting a possibly last owner must keep its same-frame catch: {program:?}"
    );
}

/// Verifies rebinding cleanup keeps its handler through pruning, normalization, and DCE.
#[test]
fn test_throwing_local_rebind_preserves_a_same_frame_catch_through_all_optimizer_phases() {
    let tokens = crate::lexer::tokenize(&format!(
        "<?php{THROWING_DESTRUCTOR_DECLARATIONS}
function run(): string {{
    $held = new Detonator();
    try {{ $held = 42; return 'no'; }}
    catch (CleanupFailure $error) {{ return 'caught'; }}
}}
echo run();"
    ))
    .expect("fixture must tokenize");
    let program = crate::parser::parse(&tokens).expect("fixture must parse");
    let optimizer = PostTypecheckOptimizer::new(&program);

    let program = optimizer.prune(program, HashSet::new());
    assert!(has_catch(&program), "pruning hoisted a throwing rebind out of its try: {program:?}");
    let program = optimizer.normalize(program, HashSet::new());
    assert!(
        has_catch(&program),
        "normalization hoisted a throwing rebind out of its try: {program:?}",
    );
    let program = optimizer.eliminate_dead_code(program, HashSet::new());
    assert!(
        has_catch(&program),
        "DCE pruned the same-frame destructor catch: {program:?}",
    );
}

/// Verifies a quiet local rebind still permits the same optimizer simplifications.
#[test]
fn test_quiet_local_rebind_does_not_keep_an_unreachable_catch() {
    let tokens = crate::lexer::tokenize(
        r#"<?php
class CleanupFailure extends Exception {}
class Quiet { public function __destruct() { echo 'quiet'; } }
function run(): string {
    $held = new Quiet();
    try { $held = 42; return 'no'; }
    catch (CleanupFailure $error) { return 'caught'; }
}
echo run();
"#,
    )
    .expect("fixture must tokenize");
    let program = crate::parser::parse(&tokens).expect("fixture must parse");
    let optimizer = PostTypecheckOptimizer::new(&program);
    let program = optimizer.prune(program, HashSet::new());
    let program = optimizer.normalize(program, HashSet::new());
    let program = optimizer.eliminate_dead_code(program, HashSet::new());

    assert!(
        !has_catch(&program),
        "a quiet local retirement must not retain an unreachable catch: {program:?}",
    );
}

/// Verifies a non-throwing destructor still lets an unreachable catch be pruned.
///
/// This is the control that proves the model is gated on a destructor that can actually THROW
/// rather than on the presence of any destructor at all.
#[test]
fn test_non_throwing_destructor_still_prunes_an_unreachable_catch() {
    let program = dce_source(
        r#"<?php
class Payload {}
class Quiet { public function __destruct() { echo 'q'; } }
function buildResult(Quiet $marker): Payload { return new Payload(); }
function run(): string {
    try { buildResult(new Quiet()); return 'no'; }
    catch (TypeError $error) { return 'caught'; }
}
echo run();
"#,
    );

    assert!(
        !has_catch(&program),
        "a quiet destructor must not resurrect an unreachable catch: {program:?}"
    );
}

/// Verifies a destructor throwing a disjoint class still prunes a non-matching catch.
///
/// Pins that the destructor source feeds the ordinary typed routing rather than bypassing it
/// with a blanket unknown throwable domain.
#[test]
fn test_destructor_throwing_a_disjoint_class_still_prunes_a_non_matching_catch() {
    let program = dce_source(&format!(
        "<?php{THROWING_DESTRUCTOR_DECLARATIONS}
class Unrelated extends Exception {{}}
function buildResult(Detonator $marker): Payload {{ return new Payload(); }}
function run(): string {{
    try {{ buildResult(new Detonator()); return 'no'; }}
    catch (Unrelated $error) {{ return 'caught'; }}
}}
echo run();"
    ));

    assert!(
        !has_catch(&program),
        "a destructor throwing a disjoint class must not keep the catch: {program:?}"
    );
}

/// Verifies a program with no destructor at all keeps catch pruning exactly as before.
///
/// This is the scalar/nothrow control: with an empty program-wide destructor summary every new
/// destruction term contributes nothing, so a non-throwing try body still loses its handler.
#[test]
fn test_program_without_any_destructor_keeps_catch_pruning_unchanged() {
    let program = dce_source(
        r#"<?php
class Payload {}
function makePayload(): Payload { return new Payload(); }
function run(): string {
    $marker = makePayload();
    try { makePayload(); unset($marker); return 'no'; }
    catch (TypeError $error) { return 'caught'; }
}
echo run();
"#,
    );

    assert!(
        !has_catch(&program),
        "a destructor-free program must keep its previous catch pruning: {program:?}"
    );
}

/// Verifies a trait-provided destructor keeps the program-wide destructor summary conservative.
///
/// Trait bodies never reach the exception summary collector, so claiming the program has no
/// destructor throws would drop a handler the destructor can reach.
#[test]
fn test_trait_provided_destructor_keeps_the_destructor_summary_conservative() {
    let program = dce_source(
        r#"<?php
class Payload {}
trait Detonating { public function __destruct() { throw new Exception('boom'); } }
class Holder { use Detonating; }
function buildResult(Holder $marker): Payload { return new Payload(); }
function run(): string {
    try { buildResult(new Holder()); return 'no'; }
    catch (TypeError $error) { return 'caught'; }
}
echo run();
"#,
    );

    assert!(
        has_catch(&program),
        "a trait-provided destructor must keep the summary conservative: {program:?}"
    );
}

/// Verifies a destructor declared in a conditional branch is not treated as absent.
///
/// The summary collector only reaches top-level and namespace-block declarations, so a class
/// declared inside an `if` body has no summarized destructor. The authoritative reference walk
/// still finds it, which is what opens the summary to the unknown throwable domain.
#[test]
fn test_destructor_declared_in_a_conditional_branch_opens_the_destructor_summary() {
    let program = dce_source(
        r#"<?php
class Payload {}
function makePayload(): Payload { return new Payload(); }
function run(): string {
    $marker = makePayload();
    try { makePayload(); unset($marker); return 'no'; }
    catch (TypeError $error) { return 'caught'; }
}
if ($argc > 1) {
    class Nested { public function __destruct() { throw new Exception('boom'); } }
}
echo run();
"#,
    );

    assert!(
        has_catch(&program),
        "a conditionally declared destructor must open the summary: {program:?}"
    );
}

/// Verifies the effect layer still reports a destructor-bearing try body as possibly throwing.
///
/// The typed `ThrownTypes` routing is the layer this change fixes; the coarser `may_throw`
/// effect model was already conservative, and control-flow pruning depends on it staying that
/// way. Pinning it here keeps the two layers from drifting into "the try survives pruning but
/// loses its handler", which is exactly the shape that produced the runtime fatal.
#[test]
fn test_effect_layer_keeps_a_destructor_bearing_try_body_throwing() {
    let tokens = crate::lexer::tokenize(&format!(
        "<?php{THROWING_DESTRUCTOR_DECLARATIONS}
function buildResult(Detonator $marker): Payload {{ return new Payload(); }}
function run(): string {{
    try {{ buildResult(new Detonator()); return 'no'; }}
    catch (CleanupFailure $error) {{ return 'caught'; }}
}}
echo run();"
    ))
    .expect("fixture must tokenize");
    let program = crate::parser::parse(&tokens).expect("fixture must parse");

    let declaration = program
        .iter()
        .find(|stmt| matches!(&stmt.kind, StmtKind::FunctionDecl { name, .. } if name == "run"))
        .expect("the fixture declares run()");
    let StmtKind::FunctionDecl { body, .. } = &declaration.kind else {
        panic!("expected a function declaration");
    };
    let StmtKind::Try { try_body, .. } = &body[0].kind else {
        panic!("expected run() to open with a try statement");
    };

    assert!(
        block_may_throw(try_body),
        "the effect layer must keep the destructor-bearing try body throwing: {try_body:?}"
    );
}

/// Two classes whose destructors throw DISJOINT exception classes, with no shared storage.
///
/// Every fixture below that needs exact-class routing builds on this pair: retiring an `Alpha`
/// can only raise `AlphaFailure`, and a `BetaFailure` handler over that retirement is therefore
/// unreachable. `Beta` exists purely so the program-wide summary is strictly larger than the
/// exact one, which is what makes the pruning assertions meaningful.
const DISJOINT_DESTRUCTOR_DECLARATIONS: &str = r#"
class AlphaFailure extends Exception {}
class BetaFailure extends Exception {}
class Alpha { public function __destruct() { throw new AlphaFailure('alpha'); } }
class Beta { public function __destruct() { throw new BetaFailure('beta'); } }
"#;

/// Verifies retiring an exactly known class routes only that class's destructor throw.
///
/// Both destructors are summarized, so the program-wide summary holds `AlphaFailure` AND
/// `BetaFailure`. Only the exact-class term keeps the `BetaFailure` handler unreachable, and it
/// only survives the fixed point because `Alpha::__destruct`'s own frame is proven to retire
/// nothing: unioning scope cleanup into every destructor summary would put `BetaFailure` back
/// into `Alpha::__destruct` and resurrect this handler.
#[test]
fn test_two_disjoint_destructor_classes_keep_their_exact_routing() {
    let program = dce_source(&format!(
        "<?php{DISJOINT_DESTRUCTOR_DECLARATIONS}
try {{ new Alpha(); echo 'ran'; }}
catch (BetaFailure $error) {{ echo 'beta'; }}"
    ));

    assert!(
        !has_catch(&program),
        "an Alpha retirement must not keep a BetaFailure handler: {program:?}"
    );
}

/// Verifies the matching handler over the same retirement is kept.
///
/// The companion to the disjoint control above: without it, a pruning assertion alone could be
/// satisfied by a model that lost the destruction term altogether.
#[test]
fn test_the_matching_destructor_class_keeps_its_handler() {
    let program = dce_source(&format!(
        "<?php{DISJOINT_DESTRUCTOR_DECLARATIONS}
try {{ new Alpha(); echo 'ran'; }}
catch (AlphaFailure $error) {{ echo 'alpha'; }}"
    ));

    assert!(
        has_catch(&program),
        "an Alpha retirement must keep its own AlphaFailure handler: {program:?}"
    );
}

/// Verifies a class with dynamic property storage keeps the conservative storage term.
///
/// `#[AllowDynamicProperties]` lets an instance hold an object under a name no declaration
/// mentions, so the declared property list is not a layout proof and the retirement has to stay
/// on the program-wide summary. The disjoint control above is the same fixture without the
/// attribute, which is what makes this assertion about the attribute alone.
#[test]
fn test_dynamic_property_storage_keeps_the_conservative_retirement_term() {
    let program = dce_source(
        r#"<?php
class AlphaFailure extends Exception {}
class BetaFailure extends Exception {}
#[AllowDynamicProperties]
class Alpha { public function __destruct() { throw new AlphaFailure('alpha'); } }
class Beta { public function __destruct() { throw new BetaFailure('beta'); } }
try { new Alpha(); echo 'ran'; }
catch (BetaFailure $error) { echo 'beta'; }
"#,
    );

    assert!(
        has_catch(&program),
        "a dynamic-property class must keep the conservative storage term: {program:?}"
    );
}

/// Verifies a subclass source routes the SUBCLASS destructor, not the parent's.
///
/// `new Sub()` names an exact runtime class whose `__destruct` overrides the parent's, and PHP
/// does not chain to the parent destructor. A `BaseFailure` handler over that retirement is
/// therefore unreachable.
#[test]
fn test_a_subclass_temporary_routes_the_subclass_destructor() {
    let program = dce_source(
        r#"<?php
class BaseFailure extends Exception {}
class SubFailure extends Exception {}
class Base { public function __destruct() { throw new BaseFailure('base'); } }
class Sub extends Base { public function __destruct() { throw new SubFailure('sub'); } }
try { new Sub(); echo 'ran'; }
catch (BaseFailure $error) { echo 'base'; }
"#,
    );

    assert!(
        !has_catch(&program),
        "a subclass temporary must route the subclass destructor: {program:?}"
    );
}

/// Verifies `new self()` stays exact while `new static()` does not.
///
/// `self` is early-bound, so retiring the temporary inside `Base` can only run `Base::__destruct`
/// and a `SubFailure` handler is unreachable. `static` is late-bound: the same source line
/// instantiates `Sub` when `Sub::spawnLate()` is the entry, so both the constructor and the
/// retirement refuse to resolve it and the handler stays. The two methods sit in ONE fixture so
/// the only difference between the assertions is the receiver keyword.
#[test]
fn test_late_static_construction_is_not_treated_as_the_declaring_class() {
    let source = r#"<?php
class BaseFailure extends Exception {}
class SubFailure extends Exception {}
class Base {
    public function __destruct() { throw new BaseFailure('base'); }
    public function spawnSelf(): int {
        try { new self(); return 1; }
        catch (SubFailure $error) { return 0; }
    }
    public function spawnLate(): int {
        try { new static(); return 1; }
        catch (SubFailure $error) { return 0; }
    }
}
class Sub extends Base { public function __destruct() { throw new SubFailure('sub'); } }
echo (new Sub())->spawnLate(), (new Base())->spawnSelf();
"#;
    let program = dce_source(source);

    let spawn_self = method_body(&program, "Base", "spawnSelf");
    assert!(
        !has_catch(spawn_self),
        "an early-bound `new self()` must route the declaring class: {spawn_self:?}"
    );
    let spawn_late = method_body(&program, "Base", "spawnLate");
    assert!(
        has_catch(spawn_late),
        "a late-bound `new static()` must stay conservative: {spawn_late:?}"
    );
}

/// Returns the body of one named method of one named class in an optimized program.
///
/// The late-static fixture asserts opposite outcomes for two methods of the same class, so the
/// assertions have to look at one body each rather than at the whole program.
fn method_body<'a>(program: &'a [Stmt], class: &str, method: &str) -> &'a [Stmt] {
    program
        .iter()
        .find_map(|stmt| match &stmt.kind {
            StmtKind::ClassDecl { name, methods, .. } if name == class => methods
                .iter()
                .find(|candidate| candidate.name == method)
                .map(|candidate| &candidate.body[..]),
            _ => None,
        })
        .unwrap_or_else(|| panic!("the fixture declares {class}::{method}()"))
}

/// Verifies a proven scalar callee does not import an unrelated destructor summary.
///
/// `twice()` takes one scalar parameter, binds nothing, and returns to its caller, so its frame
/// teardown cannot run any destructor. Charging it with the program-wide summary, which is
/// non-empty only because an entirely unrelated `Detonator` exists, would resurrect a handler
/// that no reachable throw can enter.
#[test]
fn test_a_proven_scalar_callee_does_not_import_an_unrelated_destructor_summary() {
    let program = dce_source(&format!(
        "<?php{THROWING_DESTRUCTOR_DECLARATIONS}
function twice(int $value): int {{ return $value * 2; }}
function run(): int {{
    try {{ return twice(2); }}
    catch (CleanupFailure $error) {{ return 0; }}
}}
echo run();"
    ));

    assert!(
        !has_catch(&program),
        "a proven scalar callee must not import an unrelated destructor summary: {program:?}"
    );
}

/// Verifies an UNTYPED parameter still imports the conservative scope-cleanup summary.
///
/// The companion control to the fixture above, and the reason it is untyped rather than
/// object-typed: the call site is identical (`countUp(2)` retires nothing of its own), so the
/// only difference between keeping and pruning the handler is whether the callee's frame was
/// proven. An unannotated parameter can hold an object, so it is not, and the summary stays.
#[test]
fn test_an_untyped_parameter_callee_still_imports_the_scope_cleanup_summary() {
    let program = dce_source(&format!(
        "<?php{THROWING_DESTRUCTOR_DECLARATIONS}
function countUp($value): int {{ return 1; }}
function run(): int {{
    try {{ return countUp(2); }}
    catch (CleanupFailure $error) {{ return 0; }}
}}
echo run();"
    ));

    assert!(
        has_catch(&program),
        "an untyped-parameter callee must keep the conservative scope cleanup: {program:?}"
    );
}

/// Verifies increment of a scalar parameter still proves the callee frame clean.
///
/// `++$value` is parser-supported `PreIncrement` of a named local, not a nested lvalue.
/// PHP 8 cannot leave an object in that slot, so the unrelated `Detonator` destructor
/// must not resurrect the handler.
#[test]
fn test_simple_parameter_increment_does_not_import_an_unrelated_destructor_summary() {
    let program = dce_source(&format!(
        "<?php{THROWING_DESTRUCTOR_DECLARATIONS}
function bump(int $value): int {{ return ++$value; }}
function run(): int {{
    try {{ return bump(1); }}
    catch (CleanupFailure $error) {{ return 0; }}
}}
echo run();"
    ));

    assert!(
        !has_catch(&program),
        "a named-local increment of a scalar parameter must not import an unrelated destructor summary: {program:?}"
    );
}

/// Verifies a reachable `eval` opens the destructor summary.
///
/// `eval` source is opaque to AOT compilation: it can declare a class with a throwing
/// `__destruct` and hand back an instance, so the summarized destructor bodies would be an
/// under-approximation rather than a union. The fixture body is the destructor-free control
/// from `test_program_without_any_destructor_keeps_catch_pruning_unchanged`, so the only thing
/// that changes the outcome is the `eval` call.
#[test]
fn test_a_reachable_eval_opens_the_destructor_summary() {
    let program = dce_source(
        r#"<?php
class Payload {}
function makePayload(): Payload { return new Payload(); }
function run(): string {
    $marker = makePayload();
    try { makePayload(); unset($marker); return 'no'; }
    catch (TypeError $error) { return 'caught'; }
}
eval('$ignored = 1;');
echo run();
"#,
    );

    assert!(
        has_catch(&program),
        "a reachable eval must open the destructor summary: {program:?}"
    );
}

/// Verifies a non-static closure written in a class body retires its implicit `$this`.
///
/// The descriptor an immediately invoked non-static closure creates binds the enclosing `$this`,
/// and this call site both creates and retires that descriptor. The closure has no explicit
/// captures at all, so the implicit receiver binding is the only retirement the handler can
/// depend on.
#[test]
fn test_an_immediately_invoked_non_static_closure_retires_its_implicit_receiver() {
    let program = dce_source(
        r#"<?php
class CleanupFailure extends Exception {}
class Holder {
    public function __destruct() { throw new CleanupFailure('holder'); }
    public function build(): int {
        try { (function (): int { return 1; })(); return 1; }
        catch (CleanupFailure $error) { return 0; }
    }
}
echo (new Holder())->build();
"#,
    );

    assert!(
        has_catch(&program),
        "an implicit receiver binding must keep its same-frame handler: {program:?}"
    );
}

/// Verifies a `static` closure in the same position binds nothing and loses the handler.
///
/// The control for the fixture above: `static function` cannot bind `$this`, and with no
/// captures the descriptor owns nothing, so the handler is unreachable again.
#[test]
fn test_an_immediately_invoked_static_closure_retires_nothing() {
    let program = dce_source(
        r#"<?php
class CleanupFailure extends Exception {}
class Holder {
    public function __destruct() { throw new CleanupFailure('holder'); }
    public function build(): int {
        try { (static function (): int { return 1; })(); return 1; }
        catch (CleanupFailure $error) { return 0; }
    }
}
echo (new Holder())->build();
"#,
    );

    assert!(
        !has_catch(&program),
        "a static closure with no captures must not keep the handler: {program:?}"
    );
}

/// Verifies an arrow function stays conservative about its implicit by-value captures.
///
/// An arrow function captures by value automatically and the AST records no capture list for it,
/// so there is nothing to inspect and the only sound answer is the program-wide summary. The
/// fixture is written at top level and marked `static` so neither an explicit capture nor an
/// implicit `$this` can be the reason the handler survives.
#[test]
fn test_an_immediately_invoked_arrow_function_stays_conservative() {
    let program = dce_source(&format!(
        "<?php{THROWING_DESTRUCTOR_DECLARATIONS}
$held = new Detonator();
try {{ (static fn (): int => 1)(); echo 'ran'; }}
catch (CleanupFailure $error) {{ echo 'caught'; }}"
    ));

    assert!(
        has_catch(&program),
        "an arrow function's implicit captures must stay conservative: {program:?}"
    );
}

/// Verifies a first-class callable retires the receiver it binds, at its exact class.
///
/// `(new Alpha())->label(...)` binds the fresh `Alpha` into the descriptor this statement
/// creates and retires, so the retirement is exactly `Alpha`'s. A `BetaFailure` handler over it
/// is unreachable, while the matching `AlphaFailure` handler is not.
#[test]
fn test_a_first_class_callable_retires_its_bound_receiver_at_the_exact_class() {
    let declarations = r#"
class AlphaFailure extends Exception {}
class BetaFailure extends Exception {}
class Alpha {
    public function label(): string { return 'alpha'; }
    public function __destruct() { throw new AlphaFailure('alpha'); }
}
class Beta { public function __destruct() { throw new BetaFailure('beta'); } }
"#;

    let disjoint = dce_source(&format!(
        "<?php{declarations}
try {{ (new Alpha())->label(...); echo 'ran'; }}
catch (BetaFailure $error) {{ echo 'beta'; }}"
    ));
    assert!(
        !has_catch(&disjoint),
        "a bound receiver must route its exact class: {disjoint:?}"
    );

    let matching = dce_source(&format!(
        "<?php{declarations}
try {{ (new Alpha())->label(...); echo 'ran'; }}
catch (AlphaFailure $error) {{ echo 'alpha'; }}"
    ));
    assert!(
        has_catch(&matching),
        "a bound receiver must keep its own handler: {matching:?}"
    );
}
