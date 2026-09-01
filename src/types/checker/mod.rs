//! Purpose:
//! Defines the checker state and public type-checking surface for the compiler pipeline.
//! Owns cross-phase metadata including environments, declarations, warnings, FFI, classes, and required libraries.
//!
//! Called from:
//! - `crate::types::check()`
//!
//! Key details:
//! - Checker state is populated in ordered phases; later passes assume schemas, builtins, and signatures are complete.

mod binding_decision_ambiguity;
mod builtin_enums;
mod builtin_interfaces;
mod builtin_iterators;
mod builtin_json;
mod builtin_spl_classes;
mod builtin_class_gate;
mod builtin_spl_exceptions;
/// builtin_stdclass
pub(crate) mod builtin_stdclass;
mod builtin_types;
pub(crate) use builtin_types::{
    set_state_contract_error, set_state_contract_violation, set_state_visibility_warnings,
    SetStateContractViolation,
};
mod builtin_user_filter;
pub(crate) mod builtins;
mod callables;
mod driver;
mod extern_decl;
mod functions;
mod inference;
mod loop_storage;
mod method_pass;
mod mixed_storage_scan;
pub(crate) mod null_probe;
mod schema;
mod stmt_check;
mod type_compat;
/// yield_validation
pub(crate) mod yield_validation;

use std::collections::{HashMap, HashSet};

use crate::codegen::platform::Target;
use crate::errors::CompileError;
use crate::parser::ast::{
    CallableTarget, Expr, ExprKind, Program, TypeExpr,
};
use crate::span::Span;
use crate::types::{
    collect_attribute_args, collect_attribute_names, CheckResult, ClassInfo, EnumInfo,
    ExternClassInfo, ExternFunctionSig, FunctionSig, InterfaceInfo, PackedClassInfo, PhpType,
    ThrowAccessInfo, TypeEnv,
};

pub use inference::{infer_expr_type_syntactic, infer_return_type_syntactic};
pub(crate) use loop_storage::loop_carried_storage_types;
pub(crate) use inference::closure_body_uses_this;
pub(crate) use builtin_types::InterfaceDeclInfo;
use builtin_types::validate_magic_method_contracts;
use schema::propagate_abstract_return_types;

/// Checker carries program-wide type-checking state including function signatures,
/// class/interface/enum definitions, variable environments, and warnings collected
/// during type checking.
pub(crate) struct Checker {
    /// Target platform for codegen (affects ABI, sizes, and platform checks).
    /// Full compilation target. `platform` drives OS constants; `apple_variant`
    /// distinguishes iOS from macOS, which share every one of those constants but
    /// not their sandbox rules.
    pub target: Target,
    /// User-defined function declarations, keyed by canonical name.
    pub fn_decls: HashMap<String, FnDecl>,
    /// Groups of function variant names that share the same logical function
    /// (used for overload resolution and `function_exists()`).
    pub function_variant_groups: HashMap<String, Vec<String>>,
    /// Canonical function signatures indexed by fully-qualified name.
    pub functions: HashMap<String, FunctionSig>,
    /// Functions whose body is currently being checked.
    ///
    /// Recursive calls may use their provisional signature, but must not re-specialize the same
    /// declaration while it is in flight.
    pub resolving_functions: HashSet<String>,
    /// Top-level constant types indexed by canonical name.
    pub constants: HashMap<String, PhpType>,
    /// Tracks the return type of closures assigned to variables, keyed by variable name.
    pub closure_return_types: HashMap<String, PhpType>,
    /// Tracks known callable signatures for variables holding first-class callables,
    /// keyed by variable name.
    pub callable_sigs: HashMap<String, FunctionSig>,
    /// Tracks source-declared callable parameters in the active function body.
    pub callable_param_names: HashSet<String>,
    /// Tracks callable signatures inferred for user-function callable parameters,
    /// keyed by (function_name, param_name).
    pub callable_param_sigs: HashMap<(String, String), FunctionSig>,
    /// Whether the statement currently being checked was written in a file that opened with
    /// `declare(strict_types=1)`.
    ///
    /// PHP scopes the directive to the file containing the *call site*, so this is installed
    /// from `Stmt::strict_types` by `check_stmt` and restored afterwards. It is `false` outside
    /// any statement check, which keeps class/constant/default-value checking on PHP's coercive
    /// rules — the behaviour elephc had before the directive was honoured.
    pub strict_types: bool,
    /// Tracks which undeclared function parameters have already had their type
    /// adopted from a real call site, keyed by (function_name, param_index). The
    /// first such call adopts the actual argument type; later disagreeing calls
    /// widen the parameter to `Mixed` (so e.g. a parameter called with both an int
    /// and a string is `Mixed`, not collapsed to one type).
    pub param_specialization_seen: HashSet<(String, usize)>,
    /// Tracks callable signatures inferred for user-function callable returns.
    pub callable_return_sigs: HashMap<String, FunctionSig>,
    /// Tracks callable element signatures inferred for user-function array returns.
    pub callable_array_return_sigs: HashMap<String, FunctionSig>,
    /// Tracks capture payloads for closures assigned to variables, keyed by variable name.
    /// Each entry is (capture_name, capture_type, is_by_ref).
    pub callable_captures: HashMap<String, Vec<(String, PhpType, bool)>>,
    /// Tracks callable-array targets assigned to variables, keyed by variable name.
    pub callable_array_targets: HashMap<String, CallableTarget>,
    /// Tracks first-class callable targets assigned to variables, keyed by variable name.
    pub first_class_callable_targets: HashMap<String, CallableTarget>,
    /// Tracks `ReflectionClass` locals whose reflected class is statically known.
    pub reflection_class_targets: HashMap<String, String>,
    /// Interface definitions collected during the first pass, keyed by canonical name.
    pub interfaces: HashMap<String, InterfaceInfo>,
    /// Class definitions collected during the first pass, keyed by canonical name.
    pub classes: HashMap<String, ClassInfo>,
    /// Canonical class names declared in the program, available for forward references
    /// before the full class definitions are available.
    pub declared_classes: HashSet<String>,
    /// Enum definitions collected during the first pass, keyed by canonical name.
    pub enums: HashMap<String, EnumInfo>,
    /// Canonical interface names declared in the program, available for forward references
    /// before the full interface definitions are available.
    pub declared_interfaces: HashSet<String>,
    /// Canonical trait names declared in the program, available for reflection
    /// and class-like metadata probes that accept traits.
    pub declared_traits: HashSet<String>,
    /// Reflection-visible method signatures declared directly on each trait.
    pub declared_trait_methods: HashMap<String, HashMap<String, FunctionSig>>,
    /// Reflection-visible class constant names declared directly on each trait.
    pub declared_trait_constants: HashMap<String, HashSet<String>>,
    /// Name of the class currently being type-checked (used for `$this` resolution).
    pub current_class: Option<String>,
    /// Name of the current method being type-checked, when inside a class body.
    pub current_method: Option<String>,
    /// Instance properties read through `$this` before their current constructor write.
    pub constructor_properties_read: HashSet<(String, String)>,
    /// Name of the top-level function whose body is currently being type-checked
    /// (saved/restored around each `resolve_function_signature` body walk, so
    /// nested resolution of a called function tracks the innermost body). Used
    /// by prelude friend channels to identify compiler-owned procedural
    /// aliases, which have no class context.
    pub current_function: Option<String>,
    /// Whether the current method being type-checked is static.
    pub current_method_is_static: bool,
    /// Whether the function/method/closure body currently being checked returns by
    /// reference (`function &f()`). A `return $obj->prop` in such a body promotes the
    /// property to a reference property (see `reference_property_promotions`).
    pub current_by_ref_return: bool,
    /// Nesting depth of closure bodies currently being type-checked. A non-zero
    /// depth means `$this` is allowed even outside a class method: such a
    /// closure can be bound to an object later via `Closure::bind` / `bindTo`.
    pub closure_depth: usize,
    /// Extern function declarations (e.g. `extern "C" { function foo(): void; }`).
    pub extern_functions: HashMap<String, ExternFunctionSig>,
    /// Extern class (C struct) declarations keyed by canonical name.
    pub extern_classes: HashMap<String, ExternClassInfo>,
    /// Packed layout-only records (`packed class`), keyed by canonical name.
    pub packed_classes: HashMap<String, PackedClassInfo>,
    /// Extern global variable declarations, keyed by variable name.
    pub extern_globals: HashMap<String, PhpType>,
    /// Libraries required by `#[link]` attributes on extern blocks, in link order.
    pub required_libraries: Vec<String>,
    /// Best-known top-level variable types visible to `global` statements in the
    /// current file scope.
    pub top_level_env: TypeEnv,
    /// Names that are by-ref parameters in the current function/closure scope.
    pub active_ref_params: HashSet<String>,
    /// Names introduced via `global` declarations in the current local scope.
    pub active_globals: HashSet<String>,
    /// Names ANY function-like body in the program declares `global`, collected once before the
    /// first walk from `crate::global_decls` — the same walk EIR lowering uses for
    /// `all_global_var_names`.
    ///
    /// `active_globals` cannot answer this question: it is per-body, and it is EMPTY at top level,
    /// which is exactly where a `global $a;` elsewhere aliases the top-level local's storage. See
    /// [`Checker::top_level_binding_is_program_global`].
    pub program_global_names: HashSet<String>,
    /// Names introduced via `static` declarations in the current local scope.
    pub active_statics: HashSet<String>,
    /// Explicit type contracts declared by Elephc typed-local syntax in the active scope.
    ///
    /// Ordinary PHP locals are flow-sensitive and may freely change type. Typed locals keep
    /// their declared contract across subsequent plain assignments, so the checker must track
    /// that provenance separately from the current inferred value in `TypeEnv`.
    /// Names bound as `foreach` loop keys in the current function/closure scope.
    /// A foreach key is a boxed `Mixed` cell at runtime (`Op::IterCurrentKey`)
    /// even when the checker types it as `Int`/`Str` from the source array, so an
    /// `$dst[$k] = $v` write under such a key must defer the indexed-vs-hash
    /// decision to `Op::ArraySetMixedKey` (destination `Array(Mixed)`) instead of
    /// promoting to `AssocArray` like a statically-known string key would. Mirrors
    /// the lowering's `foreach_int_key_locals` lifetime (per function, not popped).
    pub foreach_key_locals: HashSet<String>,
    /// Whether the active local statement stream has crossed an `eval()` call.
    ///
    /// Once set, unknown local reads are treated as dynamic `Mixed` values because
    /// eval fragments can create caller-scope variables at runtime.
    pub eval_barrier_active: bool,
    /// Types recorded for `return <expr>;` statements at the moment each one was checked,
    /// keyed by the statement node's address in the AST plus its span.
    ///
    /// `collect_return_infos` runs *after* a body has been checked and re-infers every return
    /// against the body's FINAL environment. Without this side channel a flow-sensitive fact
    /// that only holds on part of the body — a property narrowing established halfway down —
    /// would be applied to returns that execute before it, silently accepting a nullable
    /// return. The address key is exact because both passes borrow the same immutable AST; the
    /// span is carried alongside so a recycled address can never be mistaken for a hit.
    pub flow_typed_returns: HashMap<usize, (Span, PhpType)>,
    /// Whether the statements being checked belong to the top-level (global) scope rather than a
    /// function, method, or closure body. `with_local_storage_context` clears it for every local
    /// scope, so `null_probe` only records deferred roots for the scope whose environment
    /// actually becomes `CheckResult::global_env`.
    pub null_probe_scope_is_top_level: bool,
    /// Never-declared variables named by a null probe (`isset`/`empty`/`unset`/`??`) at top level,
    /// with the span to blame if the tolerance turns out to be unjustified.
    ///
    /// PHP answers these probes without an `Undefined variable` warning, but EIR lowering can only
    /// represent the storage when the variable stays `null` for the whole scope: main's local
    /// types come from `global_env`, so a name that is *also* assigned somewhere at top level ends
    /// up with that assigned type and its slot is read before any store. `check_top_level_program`
    /// therefore defers the decision to the end of the pass, where `global_env` is authoritative.
    pub pending_null_probe_roots: Vec<(String, Span)>,
    /// Nesting depth of null-probe operand inference (`isset`/`empty`/`unset` arguments and the
    /// left operand of `??`).
    ///
    /// Inside a probe, reaching through a `null` base is legal PHP — `isset($n['k'])` and
    /// `$n->p ?? $d` answer `false`/`$d` for a null `$n` instead of faulting — so index and
    /// property access on `PhpType::Void` yield `Void` rather than a diagnostic. Outside a probe
    /// those accesses keep their errors.
    pub null_probe_depth: usize,
    /// Active break/continue target depth in the current function or closure body.
    pub break_continue_depth: usize,
    /// Stacks of break/continue depths at each enclosing `finally` block boundary,
    /// used to restore correct depth when branching through `finally`.
    pub finally_break_continue_bases: Vec<usize>,
    /// Stable function-like scope key used to disambiguate identical loop spans.
    pub current_loop_storage_scope: String,
    /// Warnings raised during type checking (e.g. `#[\Deprecated]` call sites).
    /// Merged with AST-only warnings from `collect_warnings` before being returned
    /// in `CheckResult`.
    pub warnings: Vec<crate::errors::CompileWarning>,
    /// `(class, property)` pairs for regular properties that had a reference taken
    /// (`$x = &$obj->prop`, by-reference return of `$obj->prop`). Recorded while
    /// checking bodies and applied to `classes` after checking so every access lowers
    /// through the property's ref-cell. See `apply_reference_property_promotions`.
    pub reference_property_promotions: HashSet<(String, String)>,
    /// Statically-decided access violations that must lower to a catchable
    /// `Error` throw instead of a compile-time error, keyed by source span.
    ///
    /// Record through `record_throw_access_site`, which refuses a span that cannot identify
    /// one: lowering looks entries up by the span of the node it is lowering, so a key shared
    /// by many nodes makes all of them throw. See that method for why the preludes make this
    /// reachable rather than theoretical.
    pub throw_access_sites: HashMap<Span, ThrowAccessInfo>,
    /// Authoritative result type of each checked builtin call, keyed by call span.
    /// EIR lowering consumes this instead of reimplementing builtin return inference.
    pub builtin_call_types: HashMap<Span, PhpType>,
    /// Fixed-point storage contracts keyed by function-like scope and loop span.
    pub loop_storage_types: crate::types::LoopStorageTypes,
    /// `(scope, local)` pairs for `string` locals used as a `++`/`--` target.
    ///
    /// PHP's string increment can change the value's type (`"9"++` is `int(10)`), so EIR
    /// lowering must give those locals boxed `Mixed` frame storage from their FIRST store
    /// instead of widening the slot at the increment. Recorded here because the checker
    /// already visits every expression with a typed environment, so no second AST walk is
    /// needed. See `crate::ir_lower::context::LoweringContext::boxed_incdec_storage_type`.
    pub string_incdec_locals: HashSet<(String, String)>,
    /// Mirrors `CheckOptions::strict_locals` for the duration of the check. When set, the
    /// permissive local-retype path in `merge_local_assignment_type` and the branch-divergent
    /// `Mixed`-storage marking in `mixed_storage_scan::run_mixed_storage_scan` both step aside
    /// and leave the existing hard "cannot reassign" error / no-marking behavior in place
    /// instead of rebinding-with-warning or marking-with-warning.
    pub strict_locals: bool,
    /// Nesting depth of conditional statements (`if`/`switch`/loops/`try`/…) in the body being
    /// checked. Only depth 0 is straight-line code that unconditionally dominates everything
    /// after it, which is what makes a binding kill or re-bind safe to lower.
    pub local_conditional_depth: u32,
    /// Conditional depth at which each currently-bound local of the body was FIRST bound.
    /// A binding created inside a branch may be uninitialized at a later depth-0 point, so
    /// releasing or abandoning its slot there is not safe.
    pub local_binding_depth: HashMap<String, u32>,
    /// Locals of the current body that a reference can reach: `=&` target or source,
    /// by-reference closure captures (`use (&$x)`), any plain variable passed to a
    /// by-reference parameter, and BOTH names a `foreach ($arr as &$v)` touches — the iterable
    /// the loop holds references into, and the value variable that IS one of those references.
    /// A reference can escape through a callee, so the alias is permanent for the rest of the
    /// body.
    pub ref_aliased_locals: HashSet<String>,
    /// Names declared `static` in the current body. Their storage outlives the call, so the
    /// binding is never killable.
    pub static_local_names: HashSet<String>,
    /// Names whose type was DECLARED in the current body: `TypedAssign` locals (`int $x = …`)
    /// and parameters carrying a type hint. A declaration is a programmer contract and stays
    /// strict in both permissive and `--strict-locals` mode.
    pub typed_local_names: HashSet<String>,
    /// Locals mentioned by `unset()` anywhere in the current body. Retyping an assignment
    /// expression must not silently revive one of these names through a fresh storage shape.
    pub unset_mentioned_locals: HashSet<String>,
    /// The `unset()` ARGUMENTS whose local binding the checker killed, as span -> the SET of local
    /// NAMES killed there. EIR lowering consults these to abandon the old frame slot instead of
    /// null-storing into it. The name is half the key: a `Span` has no file identity and include
    /// resolution does not rebase line numbers, so lowering has to confirm the decision is about
    /// the variable in front of it — and two DIFFERENT names at one position are two decisions,
    /// not one. See `CheckResult::local_bind_kill_sites`.
    pub local_bind_kill_sites: HashMap<Span, HashSet<String>>,
    /// Statement-form assignments the checker re-bound to a fresh binding of an incompatible
    /// type, as span -> the SET of local NAMES re-bound there. Carried in `CheckResult` from here
    /// so the three decision maps travel together, keyed and shaped the same way.
    pub local_retype_sites: HashMap<Span, HashSet<String>>,
    /// AST address of the expression that forms the ENTIRE expression-statement currently being
    /// checked, or `None` outside one.
    ///
    /// PHP's grammar makes `unset(...)` a statement; elephc's parser is more permissive and accepts
    /// it as an expression, so `$c = $cond ? 1 : unset($a);` reaches the checker. The binding kill
    /// must not fire from there: the operand may never run, and the kill's side effects (the
    /// recorded kill site, the cleared per-name metadata, the dropped binding depth) live on the
    /// CHECKER, so they survive even when the branch's cloned `TypeEnv` is discarded. Measured: a
    /// kill recorded for a never-executed ternary arm (the program then printed nothing), and a
    /// callable-argument diagnostic lost because the metadata clear escaped the discarded clone.
    ///
    /// The address is exact because the statement walk and the expression walk borrow the same
    /// immutable AST, and it is scoped by the save/restore around the one place that sets it
    /// (`check_stmt`'s `ExprStmt` arm), so it can never name a stale node.
    pub statement_position_expr: Option<usize>,
    /// Whether the body being checked calls `eval()` ANYWHERE, above or below the statement being
    /// checked. Recorded by `mixed_storage_scan::run_mixed_storage_scan` before the body's first
    /// statement is checked, and consulted by [`Checker::local_binding_is_killable`].
    ///
    /// Distinct from `eval_barrier_active`, which is POINT-IN-TIME ("have we crossed an eval
    /// yet") and therefore cannot answer this question: an `unset` above the body's only `eval`
    /// sees no barrier at all. Per-body, like every other field in [`SavedLocalBindingScope`].
    pub body_contains_eval: bool,
    /// Names of the CURRENT body's locals that the syntactic pre-scan marked as whole-frame boxed
    /// `Mixed` storage, because they are assigned incompatible types across a branch.
    ///
    /// A marked name is bound `PhpType::Mixed` on EVERY assignment path, not just at its first
    /// store: `merge_local_assignment_type` consults this set before it looks at the environment
    /// at all, so the retype hook and the "cannot reassign" error never fire for a marked name.
    ///
    /// "Mixed at the first store, and `Mixed` absorbs the rest" was the original story and it does
    /// not hold. Flow narrowing writes the guard's target straight into the shared environment for
    /// a guarded branch, so a marked local reached the merge bound `int` — an EXISTING binding,
    /// which the first-store consult never saw — and `if (…) { $a = 1; } else { $a = "s"; }
    /// if (is_int($a)) { $a = "z"; }` was a hard "cannot reassign" in both modes. The mark has to
    /// be authoritative rather than merely initial. Per-body, like every other field in
    /// [`SavedLocalBindingScope`].
    pub mixed_storage_locals: HashSet<String>,
    /// Every statement-form assignment to a mixed-storage local, as span -> the SET of local names
    /// boxed at that position, so EIR lowering can declare the slot boxed BEFORE the first store
    /// instead of inferring it from the first stored value. Cumulative across bodies, and keyed
    /// like the other two decision maps — see `CheckResult::local_bind_kill_sites` for why the
    /// name is half the key.
    ///
    /// The value is a SET rather than one name because a `Span` carries no file identity: two
    /// different locals in two different files can be assigned at the same (line, column), and
    /// both may be marked. A single-name value made the second body's insert EVICT the first's,
    /// with no retirement (the retire loop only matches a decision carrying the same name), so the
    /// evicted name left `CheckResult::mixed_storage_local_names` while the checker still typed the
    /// local `Mixed` — the compiler then PANICKED (`strlen cannot lower checked operand type Int`)
    /// on valid PHP. Same-name collisions are still ambiguous and still rejected; see
    /// `retired_mixed_storage_store_sites`.
    pub mixed_storage_store_sites: HashMap<Span, HashSet<String>>,
    /// The warnings that BELONG to a local-binding decision, keyed by that decision's
    /// `(span, local name)` instead of being pushed straight into `warnings`.
    ///
    /// Both the retype hook and the mixed-storage marking RE-DECIDE their sites on every walk, and
    /// the checker walks a body more than once — a function body once per call-site
    /// specialization. A warning pushed by a SUPERSEDED walk used to survive the removal of the
    /// decision that justified it, which made the diagnostic depend on the order of the call sites:
    /// `f(1); f("x");` warned "$a changes type from int to string" while `f("x"); f(1);` — the same
    /// two calls — did not, because the second specialization widens the parameter to `mixed` and
    /// the retype stops happening. Keying the warning like the decision lets the removal take the
    /// warning with it, so what is reported is what the LAST walk decided.
    ///
    /// Assembled into `CheckResult::warnings` at the end of the check, sorted by position so the
    /// output is deterministic. A decision whose span identifies no node keeps pushing its warning
    /// directly into `warnings`: it files no decision either, so there is nothing to retract it.
    pub binding_decision_warnings: HashMap<(Span, String), crate::errors::CompileWarning>,
    /// Every `(span, name)` key `run_mixed_storage_scan` ever REMOVED from
    /// `mixed_storage_store_sites`, kept for the checker's whole lifetime.
    ///
    /// The removal is a re-decision: a scan drops the decisions already filed against the sites
    /// it is about to re-decide, so only the last walk's answer survives. A `Span` carries no
    /// file identity, though, so the removal loop of ONE body also matches an equal `(span, name)`
    /// recorded for a DIFFERENT body — and the decision then vanishes with nothing left for
    /// `binding_decision_ambiguity` to check. Recording the retired keys is what keeps that
    /// collision visible: a key that was legitimately re-decided names exactly one node, while a
    /// key retired by collision names two or more.
    ///
    /// Only the mixed-storage map needs this. Losing a KILL or RETYPE decision costs a lowering
    /// optimization — the site falls back to the null-store / slot-widening path it used before
    /// those decisions were lowered, which is correct — but losing a MIXED-storage decision
    /// changes what the rest of the compiler believes: `CheckResult::mixed_storage_local_names`
    /// is derived from this map's VALUES, so a stripped name stops blocking constant propagation
    /// while the checker still types the local `Mixed`. That disagreement panicked the compiler
    /// (`strlen cannot lower checked operand type Int`) on valid PHP.
    pub retired_mixed_storage_store_sites: HashSet<(Span, String)>,
}

/// The per-body local-binding eligibility state, saved while a nested body is checked.
///
/// Every field it carries describes ONE function/method/closure/top-level body: a name that is
/// reference-aliased in the caller says nothing about a same-named local in the callee. See
/// [`Checker::enter_local_binding_scope`].
pub(crate) struct SavedLocalBindingScope {
    conditional_depth: u32,
    binding_depth: HashMap<String, u32>,
    ref_aliased: HashSet<String>,
    statics: HashSet<String>,
    typed: HashSet<String>,
    unset_mentioned: HashSet<String>,
    mixed_storage: HashSet<String>,
    contains_eval: bool,
}

impl Checker {
    /// True when `name`'s current binding may be killed (by `unset`) or re-bound to an
    /// incompatible type (by assignment) at the current program point. See
    /// `.plans/local-retype-and-strict-locals.md` — depth-0 + non-aliased rule.
    ///
    /// A body that calls `eval()` anywhere answers `false` for EVERY name, so both the kill and
    /// the straight-line retype step aside there and the body keeps its pre-feature behaviour
    /// (`unset` is a typing no-op; an incompatible reassignment is the hard error). Ending a
    /// binding is not a typing detail down in EIR — it drops the name's frame slot and mints a
    /// fresh one — while the eval scope addresses caller locals BY NAME, so the fragment then
    /// reads or writes a slot the rest of the body no longer uses. Measured on HEAD before this
    /// gate, all three printing NOTHING where PHP prints a value:
    /// `$a = 1; unset($a); eval('$a = 5;'); echo $a;` (PHP: `5`),
    /// `$a = "old" . $argc; unset($a); $a = 7; eval('echo $a;');` (PHP: `7`), and — with no
    /// `unset` in sight — `$a = "old"; $a = 7; eval('echo $a;');` (PHP: `7`).
    ///
    /// Body-scoped, not point-in-time: `eval_barrier_active` is only set once the walk has
    /// PASSED an eval, and every repro above puts the eval BELOW the site being judged. The flag
    /// consulted here is filled by the pre-scan, which sees the whole body first.
    ///
    /// The branch-divergent (`Mixed`-storage) shape is deliberately NOT gated: it never ends a
    /// binding, it gives the local one boxed `Mixed` slot for the entire frame — which is the
    /// representation the eval scope wants anyway — so
    /// `if (…) { $b = 1; } else { $b = "z"; } eval('echo $b; $b = "w";'); echo "|", $b;` prints
    /// PHP's `z|w` with and without this gate.
    pub(crate) fn local_binding_is_killable(&self, name: &str) -> bool {
        !self.body_contains_eval
            && self.local_conditional_depth == 0
            // NO entry means "not a binding this body created", which is NOT killable. The
            // environment a body starts from is seeded with names nothing in it assigned:
            // superglobals in every scope, `$argc`/`$argv` and extern C globals at top level,
            // by-value closure captures. None of those live in a frame slot this body owns, so
            // abandoning them would strand storage the program still reaches by name.
            // PARAMETERS are the one seeded shape that IS frame storage, and they are seeded
            // with an explicit depth 0 by `enter_local_binding_scope` — an untyped parameter
            // stays kill-eligible (`test_typed_param_not_killable` pins both halves).
            //
            // A name a CONDITIONAL group introduced without going through an assignment (a
            // `foreach` or `list()` target, a `catch` variable, a builtin out-parameter, array
            // auto-vivification) also has no entry, and gets the same "not killable" answer for
            // free — which is why `in_conditional_scope` records nothing for those names. `Some(1)`
            // or more and `None` are indistinguishable HERE, and this is the only killable-relevant
            // reader of the map.
            && self.local_binding_depth.get(name).copied() == Some(0)
            && !self.active_ref_params.contains(name)
            && !self.ref_aliased_locals.contains(name)
            && !self.active_globals.contains(name)
            && !self.static_local_names.contains(name)
            && !self.typed_local_names.contains(name)
    }

    /// True when `name` is bound in a body's INCOMING environment by seeding rather than by
    /// anything the body does, and the storage behind it is not this frame's.
    ///
    /// That is exactly `driver::top_level::seed_global_env`'s contents — `$argc`, `$argv`, the
    /// request superglobals and the extern C globals — with the superglobals additionally
    /// recognised in every scope, because `resolve_function_signature` and `seed_method_env` seed
    /// them into function/method/closure environments too.
    ///
    /// Enumerated rather than read off the environment so the pre-scan, which runs before the first
    /// statement is checked and holds no environment, can consult it. PARAMETERS and by-value
    /// closure captures are deliberately NOT in it: both are seeded, but both live in a slot the
    /// body's own frame owns.
    pub(crate) fn name_is_seeded_program_storage(&self, name: &str) -> bool {
        crate::superglobals::is_superglobal(name)
            || (self.null_probe_scope_is_top_level
                && (name == "argc" || name == "argv" || self.extern_globals.contains_key(name)))
    }

    /// True when `expr` IS the whole expression of the expression-statement being checked.
    ///
    /// The gate the `unset` binding kill fires behind. See [`Checker::statement_position_expr`] for
    /// why an `unset` anywhere else must leave the checker untouched.
    pub(crate) fn expr_is_in_statement_position(&self, expr: &Expr) -> bool {
        self.statement_position_expr == Some(expr as *const Expr as usize)
    }

    /// True when `name` is a TOP-LEVEL binding whose storage some other body reaches through a
    /// `global` declaration.
    ///
    /// `global $x;` inside a function binds `$x` to the program-global cell that the top level
    /// writes through its own slot, so ending the top-level binding of `$x` strands a name the
    /// rest of the program still uses: `function w() { global $a; $a = 5; } $a = 1; unset($a);
    /// w(); echo $a;` prints `5` in PHP and compiled before this feature existed, but the `unset`
    /// removed `$a` from the environment and the `echo` became `Undefined variable: $a`.
    ///
    /// Scoped to the top-level body on purpose, mirroring lowering's `in_main &&
    /// all_global_var_names.contains(name)` gate (`LoweringContext::uses_global_storage`) and
    /// reading the SAME collected set (`crate::global_decls::collect_global_var_names`). A
    /// same-named local inside a function body is that frame's own storage — no `global` alias
    /// reaches it — so vetoing it would only cost the feature coverage it is entitled to.
    ///
    /// Consulted by the `unset` KILL alone, not by the straight-line retype. The kill REMOVES the
    /// name from the environment, which is the part that strands it; a retype leaves the name
    /// bound, and lowering already refuses to abandon the slot for exactly these names, so that
    /// site degrades to the pre-feature widening path and prints PHP's answer (measured). Vetoing
    /// it too would turn a working program into a compile error.
    pub(crate) fn top_level_binding_is_program_global(&self, name: &str) -> bool {
        self.null_probe_scope_is_top_level && self.program_global_names.contains(name)
    }

    /// Enters a fresh local-binding eligibility scope for one function/method/closure/top-level
    /// body, seeding the parameters (all of them at binding depth 0, and the type-hinted ones
    /// additionally as declared).
    ///
    /// Eligibility is a property of a single frame: the caller's aliasing, `static` names and
    /// binding depths say nothing about the body about to be checked, and a body entered from
    /// inside an `if` still starts at conditional depth 0 of its OWN statements. The returned
    /// value must be handed back to [`Checker::exit_local_binding_scope`].
    ///
    /// `param_names` is EVERY parameter, typed or not. A parameter is bound unconditionally on
    /// entry, so depth 0 is its true binding depth; recording it explicitly is what lets a
    /// MISSING depth entry mean "seeded, not bound here — not killable" in
    /// [`Checker::local_binding_is_killable`].
    pub(crate) fn enter_local_binding_scope(
        &mut self,
        param_names: Vec<String>,
        typed_param_names: Vec<String>,
    ) -> SavedLocalBindingScope {
        let saved = SavedLocalBindingScope {
            conditional_depth: self.local_conditional_depth,
            binding_depth: std::mem::take(&mut self.local_binding_depth),
            ref_aliased: std::mem::take(&mut self.ref_aliased_locals),
            statics: std::mem::take(&mut self.static_local_names),
            typed: std::mem::take(&mut self.typed_local_names),
            unset_mentioned: std::mem::take(&mut self.unset_mentioned_locals),
            // The mixed-storage marking describes ONE frame: a name boxed in the caller says
            // nothing about a same-named local in the callee, and leaking the set inward would
            // box a callee local the pre-scan never marked.
            mixed_storage: std::mem::take(&mut self.mixed_storage_locals),
            contains_eval: self.body_contains_eval,
        };
        self.local_conditional_depth = 0;
        self.local_binding_depth = param_names.into_iter().map(|name| (name, 0)).collect();
        self.typed_local_names = typed_param_names.into_iter().collect();
        self.unset_mentioned_locals.clear();
        // Cleared rather than inherited: a caller that runs `eval` says nothing about the body
        // about to be checked. `run_mixed_storage_scan` fills it in for real, and BOTH callers of
        // this method run that scan immediately afterwards — `with_local_storage_context` and
        // `check_top_level_program`. The clear is what makes a body whose scan somehow did not
        // run default to "no eval" rather than to the enclosing body's answer.
        self.body_contains_eval = false;
        saved
    }

    /// Restores the enclosing body's local-binding eligibility scope.
    pub(crate) fn exit_local_binding_scope(&mut self, saved: SavedLocalBindingScope) {
        self.local_conditional_depth = saved.conditional_depth;
        self.local_binding_depth = saved.binding_depth;
        self.ref_aliased_locals = saved.ref_aliased;
        self.static_local_names = saved.statics;
        self.typed_local_names = saved.typed;
        self.unset_mentioned_locals = saved.unset_mentioned;
        self.mixed_storage_locals = saved.mixed_storage;
        self.body_contains_eval = saved.contains_eval;
    }

    /// Drops every per-name fact the checker carries for a local whose binding just ended.
    ///
    /// The callable/reflection tables are keyed by variable name and outlive the type
    /// environment, so a killed (or re-bound) name must not keep answering with the signature,
    /// captures, or reflected class of the binding that is gone — that is how a stale
    /// `$f()` signature would survive an `unset($f)`.
    pub(crate) fn clear_local_binding_metadata(&mut self, name: &str) {
        self.closure_return_types.remove(name);
        self.callable_sigs.remove(name);
        self.callable_captures.remove(name);
        self.callable_array_targets.remove(name);
        self.first_class_callable_targets.remove(name);
        self.reflection_class_targets.remove(name);
        self.foreach_key_locals.remove(name);
    }

    /// Records that a reference was taken to the storage `expr` names, so the local at the root
    /// of its access chain is reference-aliased for the rest of the body.
    ///
    /// Covers a by-reference call argument and a `=&` source alike. The reference can escape
    /// through the callee (stored in a property, captured by a closure it creates), so the
    /// alias is permanent rather than scoped to the call. Walking the whole chain is
    /// deliberate: a reference to `$a[0]` or `$o->p` keeps `$a` / `$o` alive too.
    ///
    /// The unwrapping arms are kept in step with `mixed_storage_scan::disqualify_root`, which asks
    /// the same question about the same expressions: `sort(...$args)`, `sort(@$a)` and
    /// `sort(array: $a)` all reach the local behind the wrapper, and a shape one walker sees
    /// through while the other stops at is exactly how the checker and the pre-scan drift apart.
    pub(crate) fn record_reference_alias_root(&mut self, expr: &Expr) {
        let mut current = expr;
        loop {
            match &current.kind {
                ExprKind::Variable(name) => {
                    self.ref_aliased_locals.insert(name.clone());
                    return;
                }
                ExprKind::ArrayAccess { array: base, .. }
                | ExprKind::PropertyAccess { object: base, .. }
                | ExprKind::NullsafePropertyAccess { object: base, .. }
                | ExprKind::DynamicPropertyAccess { object: base, .. }
                | ExprKind::NullsafeDynamicPropertyAccess { object: base, .. }
                | ExprKind::Spread(base)
                | ExprKind::ErrorSuppress(base)
                | ExprKind::NamedArg { value: base, .. } => current = base,
                _ => return,
            }
        }
    }

    /// Records every argument of a call whose callee the checker could NOT resolve to a signature
    /// as reference-aliased, so none of the locals behind them stays kill/retype eligible.
    ///
    /// A `$cb($a)` on a signatureless `callable`, a variable function (`$f = "sort"; $f($a);`), a
    /// dynamic constructor or a runtime-dispatched method has no `ref_params` to consult: the
    /// callee may bind `$a` by reference, and a reference can escape. Ending or re-binding the
    /// local then abandons storage the callee still points into, so the conservative answer is the
    /// only sound one — and it is the answer `mixed_storage_scan::disqualify_call_arguments`
    /// already gives on exactly these shapes. Losing eligibility costs nothing but the feature:
    /// the kill degrades to the pre-feature null store, the retype to the pre-feature error.
    ///
    /// Only for UNRESOLVABLE callees. A known signature records its aliases per parameter through
    /// [`Checker::record_reference_alias_root`] at the by-ref slots alone, and must keep doing so —
    /// blanket-disqualifying a resolved by-VALUE call would take the feature away from the
    /// ordinary case it exists for.
    pub(crate) fn record_unresolved_callee_argument_aliases(&mut self, args: &[Expr]) {
        for arg in args {
            self.record_reference_alias_root(arg);
        }
    }
}

/// Options controlling type-checker behavior beyond its default permissive rules.
///
/// Threaded from `CliConfig::strict_locals` (the `--strict-locals` flag) down to the
/// `Checker`. All fields default to today's behavior, so a caller that does not opt in
/// (`CheckOptions::default()`) sees no change.
#[derive(Debug, Clone, Copy, Default)]
pub struct CheckOptions {
    /// Make an incompatible local retype (e.g. a variable assigned `int` then later
    /// `string`) a compile error instead of a warning.
    pub strict_locals: bool,
}

/// Returns whether a called name is PHP's `unset`, the only builtin that ends a local binding.
///
/// ONE recogniser for all three sides of the feature — the pre-scan's disqualification
/// (`mixed_storage_scan`), the kill itself (`inference::expr::effects`) and the ambiguity pass's
/// node tally (`binding_decision_ambiguity`). They have to agree exactly: a name the scan does not
/// see as `unset`-mentioned can be marked while the kill still fires on it, and a kill the tally
/// does not count is a decision nothing checks for ambiguity.
///
/// `php_symbol_key` is an ASCII lowercase, so this comparison gives that key's answer without the
/// `String` it would allocate at every call node the pre-scan walks.
pub(crate) fn is_unset_call(name: &str) -> bool {
    name.trim_start_matches('\\').eq_ignore_ascii_case("unset")
}

/// Records an access violation that must lower to a catchable `Error` throw.
///
/// Refuses a span that identifies no single node. `throw_access_sites` is read back by lowering
/// as `get(&span)` for the node being lowered, so an entry filed under a span many nodes share
/// turns ALL of them into throws — a `$this->prop = ...` in an unrelated class would start
/// raising "Cannot modify readonly property".
///
/// That is reachable, not theoretical. Every node the `synthetic_class` builders produce carries
/// `Span::dummy()`, the PDO prelude declares `public readonly string $queryString` and writes it,
/// and the write is allowed only by an `internal_pdo_statement_initializer` special case in
/// `check_property_assignment`. The day that case stops matching, the violation would be filed
/// under the shared dummy span rather than reported. Failing the build is the right answer there:
/// a readonly violation inside a compiler-generated prelude is a compiler bug, not user code to
/// diagnose at run time.
///
/// Takes the map rather than the `Checker` so a caller already holding a borrow of another field
/// — `self.classes`, at both current call sites — can still record.
pub(crate) fn record_throw_access_site(
    sites: &mut HashMap<Span, ThrowAccessInfo>,
    span: Span,
    info: ThrowAccessInfo,
) {
    debug_assert!(
        span.line != 0,
        "throw-access site at a span that names no node ({:?}). Lowering would attach this throw \
         to every other node sharing the span.",
        info.kind,
    );
    if span.line == 0 {
        return;
    }
    sites.insert(span, info);
}

#[derive(Clone)]
/// FnDecl stores a user-defined function's declaration metadata: parameter names,
/// types, defaults, variadic marker, return type, span, body statements, and
/// attributes (currently only `#[\Deprecated]` is consulted).
pub(crate) struct FnDecl {
    pub params: Vec<String>,
    pub param_types: Vec<Option<TypeExpr>>,
    /// Attribute groups aligned with the declared parameters plus the variadic parameter, if any.
    pub param_attributes: Vec<Vec<crate::parser::ast::AttributeGroup>>,
    pub defaults: Vec<Option<Expr>>,
    pub ref_params: Vec<bool>,
    pub variadic: Option<String>,
    /// Whether the variadic parameter was declared by reference (`&...$args`).
    pub variadic_by_ref: bool,
    /// Declared element type hint on the variadic parameter (`int ...$xs`), if any.
    pub variadic_type: Option<TypeExpr>,
    pub return_type: Option<TypeExpr>,
    /// `true` when declared with `function &f()` — the function returns a reference.
    pub by_ref_return: bool,
    pub span: crate::span::Span,
    pub body: Vec<crate::parser::ast::Stmt>,
    /// Attribute groups attached to the original `function` declaration.
    /// Currently consulted only for `#[\Deprecated]` detection.
    pub attributes: Vec<crate::parser::ast::AttributeGroup>,
}

/// Runs the type checker on `program` for the given `target` with default
/// `CheckOptions`, returning a `CheckResult` on success or a `CompileError` on failure.
/// Delegates to `check_types_with_options`; see it for the full behavior.
///
/// Only reachable through `result::check`/`check_with_target` (both of which now go through
/// `*_with_options`) and a unit test, so this is dead code outside of tests.
#[allow(dead_code)]
pub fn check_types(
    program: &Program,
    target: Target,
) -> Result<CheckResult, CompileError> {
    check_types_with_options(program, target, CheckOptions::default())
}

/// Runs the type checker on `program` for the given `target` and `options`,
/// returning a `CheckResult` on success or a `CompileError` on failure. The checker
/// validates types, resolves declarations, infers return types, and collects warnings.
/// Abstract return types are propagated from concrete implementations before returning.
pub fn check_types_with_options(
    program: &Program,
    target: Target,
    options: CheckOptions,
) -> Result<CheckResult, CompileError> {
    let (mut checker, global_env) = driver::check_types_impl(program, target, options)?;

    propagate_abstract_return_types(&mut checker);
    apply_reference_property_promotions(&mut checker);
    validate_magic_method_contracts(&checker)?;

    let mut warnings = crate::types::warnings::collect_warnings(program);
    warnings.extend(checker.warnings);
    // The warnings that belong to a local-binding decision, appended once the last walk has
    // settled which decisions actually survive. Sorted by position (then message) because they
    // come out of a hash map — the order has to be the same on every run.
    let mut decision_warnings: Vec<crate::errors::CompileWarning> =
        std::mem::take(&mut checker.binding_decision_warnings)
            .into_values()
            .collect();
    decision_warnings.sort_by(|left, right| {
        (left.span.line, left.span.col, &left.message).cmp(&(
            right.span.line,
            right.span.col,
            &right.message,
        ))
    });
    warnings.extend(decision_warnings);
    let function_attribute_names = checker
        .fn_decls
        .iter()
        .map(|(name, decl)| (name.clone(), collect_attribute_names(&decl.attributes)))
        .collect();
    let function_attribute_args = checker
        .fn_decls
        .iter()
        .map(|(name, decl)| (name.clone(), collect_attribute_args(&decl.attributes)))
        .collect();
    let mut return_alias_summaries = crate::types::collect_return_alias_summaries(program);
    crate::types::extend_return_alias_summaries_with_classes(
        &mut return_alias_summaries,
        &checker.classes,
    );
    dedupe_warnings(&mut warnings);

    // A decision key that names more than one syntactic site would let EIR lowering re-bind a
    // local the checker never approved — silently, and with the wrong answer. Reject it here so
    // the checker and lowering agree on exactly which programs are accepted.
    binding_decision_ambiguity::reject_ambiguous_local_binding_decisions(
        program,
        &checker.local_bind_kill_sites,
        &checker.local_retype_sites,
        &checker.mixed_storage_store_sites,
        &checker.retired_mixed_storage_store_sites,
    )?;

    Ok(CheckResult {
        global_env,
        functions: checker.functions,
        function_attribute_names,
        function_attribute_args,
        callable_param_sigs: checker.callable_param_sigs,
        return_alias_summaries,
        callable_return_sigs: checker.callable_return_sigs,
        callable_array_return_sigs: checker.callable_array_return_sigs,
        interfaces: checker.interfaces,
        classes: checker.classes,
        enums: checker.enums,
        packed_classes: checker.packed_classes,
        extern_functions: checker.extern_functions,
        extern_classes: checker.extern_classes,
        extern_globals: checker.extern_globals,
        required_libraries: checker.required_libraries,
        warnings,
        throw_access_sites: checker.throw_access_sites,
        builtin_call_types: checker.builtin_call_types,
        loop_storage_types: checker.loop_storage_types,
        string_incdec_locals: checker.string_incdec_locals,
        local_bind_kill_sites: checker.local_bind_kill_sites,
        local_retype_sites: checker.local_retype_sites,
        mixed_storage_store_sites: checker.mixed_storage_store_sites,
    })
}

/// Removes duplicate warnings emitted by repeated checker stabilization passes.
///
/// The first occurrence is preserved so diagnostic order stays stable; duplicates are keyed by
/// source span and message, matching the user-visible warning identity.
fn dedupe_warnings(warnings: &mut Vec<crate::errors::CompileWarning>) {
    let mut seen = HashSet::new();
    warnings.retain(|warning| {
        seen.insert((
            warning.span.line,
            warning.span.col,
            warning.message.clone(),
        ))
    });
}

/// Returns the single object class named by a type, ignoring a nullable arm.
///
/// `Foo` or `Foo|null` yields `Foo`; unions of multiple classes, `Mixed`, or non-object
/// types yield `None` (so reference promotion only applies to a statically known class).
pub(crate) fn single_object_class_name(ty: &PhpType) -> Option<String> {
    match ty {
        PhpType::Object(name) => Some(name.trim_start_matches('\\').to_string()),
        PhpType::Union(members) => {
            let mut found: Option<String> = None;
            for member in members {
                match member {
                    PhpType::Void => {}
                    PhpType::Object(name) => {
                        let name = name.trim_start_matches('\\').to_string();
                        if found.as_ref().is_some_and(|existing| existing != &name) {
                            return None;
                        }
                        found = Some(name);
                    }
                    _ => return None,
                }
            }
            found
        }
        _ => None,
    }
}

/// Applies recorded reference-property promotions to the class table after body checking.
///
/// A regular property that had a reference taken (`$x = &$obj->prop`, or returned by
/// reference) must be treated as a reference property by codegen so every access lowers
/// through its ref-cell. Promotion is applied to the declaring class and every class that
/// inherits the property, keeping the runtime representation consistent across the
/// hierarchy. Constructor-promoted `&$param` properties already are reference properties
/// (borrowed cell) and are left untouched. Object-owned reference cells are also recorded
/// in `owned_reference_properties` so the object allocates and frees them.
fn apply_reference_property_promotions(checker: &mut Checker) {
    let promotions = std::mem::take(&mut checker.reference_property_promotions);
    for (access_class, prop) in promotions {
        let declaring = checker
            .classes
            .get(&access_class)
            .and_then(|info| info.property_declaring_classes.get(&prop).cloned())
            .unwrap_or_else(|| access_class.clone());
        for info in checker.classes.values_mut() {
            if !info.properties.iter().any(|(name, _)| name == &prop) {
                continue;
            }
            let same_decl = info
                .property_declaring_classes
                .get(&prop)
                .is_some_and(|decl| decl == &declaring);
            if !same_decl {
                continue;
            }
            if info.reference_properties.contains(&prop) {
                continue;
            }
            info.reference_properties.insert(prop.clone());
            info.owned_reference_properties.insert(prop.clone());
            if let Some(slot) = info.visible_property_index(&prop) {
                if let Some(slot_reference) = info.property_reference_slots.get_mut(slot) {
                    *slot_reference = true;
                }
            }
        }
    }
}

#[cfg(test)]
mod throw_access_site_tests {
    use super::record_throw_access_site;
    use crate::span::Span;
    use crate::types::{ThrowAccessInfo, ThrowAccessKind};
    use std::collections::HashMap;

    fn readonly_violation(span: Span) -> ThrowAccessInfo {
        ThrowAccessInfo {
            span,
            kind: ThrowAccessKind::ReadonlyProperty {
                class_name: "PDOStatement".to_string(),
                property: "queryString".to_string(),
            },
        }
    }

    /// A violation the checker CAN locate is recorded, keyed by where it happened.
    #[test]
    fn a_located_violation_is_recorded() {
        let mut sites = HashMap::new();
        let span = Span::new(7, 3);
        record_throw_access_site(&mut sites, span, readonly_violation(span));
        assert_eq!(sites.len(), 1);
        assert!(sites.contains_key(&span));
    }

    /// A violation the checker CANNOT locate never becomes a key.
    ///
    /// Debug builds abort on it instead — a compiler-generated node that violates readonly is a
    /// compiler bug — so this covers the release behaviour, which is to degrade to silence
    /// rather than attach the throw to every other node carrying the same dummy span.
    #[cfg(not(debug_assertions))]
    #[test]
    fn an_unlocatable_violation_never_becomes_a_key() {
        let mut sites = HashMap::new();
        record_throw_access_site(&mut sites, Span::dummy(), readonly_violation(Span::dummy()));
        assert!(sites.is_empty());
    }
}
