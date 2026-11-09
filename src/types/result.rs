//! Purpose:
//! Defines the aggregate result returned by type checking to the pipeline.
//! Carries type environments, declarations, class metadata, warnings, FFI data, and required libraries forward.
//!
//! Called from:
//! - `crate::types::check()`
//! - `crate::pipeline::compile()`
//!
//! Key details:
//! - Fields are consumed by optimizer, codegen, and linker setup; keep additions explicit and phase-owned.

use crate::fast_hash::{FastMap as HashMap, FastSet as HashSet};

use crate::codegen::platform::Target;
use crate::errors::{CompileError, CompileWarning};
use crate::parser::ast::Program;
use crate::span::Span;

use super::{
    checker, AttrArgEntry, ClassInfo, EnumInfo, ExternClassInfo, ExternFunctionSig, FunctionSig,
    InterfaceInfo, PackedClassInfo, PhpType, ReturnAliasSummaries, TypeEnv,
};

/// Fixed-point loop storage contracts keyed by `(function-like scope, loop span)`.
pub(crate) type LoopStorageTypes = HashMap<(String, Span), TypeEnv>;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
/// Describes a statically-known access violation that PHP raises as a catchable
/// exception at runtime instead of a compile-time rejection.
pub struct ThrowAccessInfo {
    /// Source span of the offending call/assignment, used as the lookup key.
    pub span: Span,
    /// The kind of violation, selecting the PHP error message template.
    pub kind: ThrowAccessKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
/// Categorizes statically-decided access violations lowered to runtime exception throws.
pub enum ThrowAccessKind {
    /// A private/protected method call from an inaccessible scope.
    PrivateMethod {
        /// Visibility label (`private` or `protected`).
        visibility: String,
        /// Class holding the method (e.g. `C`).
        class_name: String,
        /// Method name without the trailing parentheses (e.g. `secret`).
        method: String,
    },
    /// A readonly write requiring a runtime initialization check.
    ReadonlyProperty {
        /// Class holding the property (e.g. `Box`).
        class_name: String,
        /// Property name without the leading `$` (e.g. `x`).
        property: String,
        /// The setter-access Error for first initialization, or `None` when it is allowed.
        initialization_error: Option<String>,
    },
    /// A direct write to one of Reflection's internal public metadata mirrors.
    ReflectionReadOnlyProperty {
        /// Property name without the leading `$`.
        property: String,
    },
    /// An argument whose type the checker proved incompatible with the parameter it binds.
    ///
    /// php performs this check when the call RUNS, so a program that never reaches the call is
    /// perfectly loadable — and real code relies on that. php also exempts constructors from
    /// inheritance signature rules, so an overriding `__construct` may declare a completely
    /// different parameter list and still forward positionally to `parent::__construct`, landing
    /// arguments in differently typed slots. `twig/twig`'s `ExtensionSet` does exactly that in a
    /// deprecated conversion path, and refusing it at compile time stopped the whole build.
    ArgumentType {
        /// The complete `TypeError` message, already in php's wording.
        message: String,
    },
    /// An argument the checker proved can never bind to its declared parameter (a strict_types
    /// scalar mismatch, or a class/array/null no coercion accepts). Keyed by the ARGUMENT's span.
    ///
    /// php never refuses this at compile time: the call evaluates its arguments, then throws
    /// `TypeError`. Lowering finds the site from the call's arguments, evaluates all of them, and
    /// appends php's `, called in FILE on line N` clause to `message`.
    CallArgumentType {
        /// The message without the `called in` clause.
        message: String,
    },
    /// A weak-mode argument php coerces to an `int` parameter at run time: a string or float
    /// whose value only the call knows. Keyed by the ARGUMENT's span.
    ///
    /// A numeric string or an integral float binds, a fractional one binds after php's
    /// `Deprecated:` precision notice, and anything else throws `TypeError`. Lowering converts
    /// the argument with the return-boundary int coercion, whose message is `prefix` followed by
    /// the runtime type word and php's `given, called in FILE on line N` clause.
    CallArgumentIntCoercion {
        /// `f(): Argument #N ($p) must be of type int, `
        prefix: String,
    },
    /// A `self::`/`parent::` call naming a method the class does not have.
    ///
    /// php resolves the method when the call RUNS. A trait shared by several classes may call a
    /// method only one of them declares (Carbon's `Mixin` calls `self::copyStep()`, which only
    /// `CarbonInterval` has): the other users load fine and fail only if that branch executes.
    UndefinedMethod {
        /// The complete `Error` message, already in php's wording.
        message: String,
    },
}

/// Checker-selected native property storage and any indirect-write refusal at one call site.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArrayPushPropertyPlan {
    /// The proven receiver class, or `None` when specialization requires dynamic property access.
    pub class_name: Option<String>,
    /// PHP's catchable Error at reference binding, before evaluating later arguments.
    pub rejection: Option<String>,
    /// Whether lowering may bind a managed live reference to this exact property slot.
    pub live_addressable: bool,
}

#[derive(Debug)]
/// Aggregate result of type checking, carrying type environments, declarations,
/// class metadata, warnings, FFI data, and required libraries forward to optimizer,
/// codegen, and linker setup.
pub struct CheckResult {
    /// Every instantiation a call site asked for, as (template name, type arguments).
    ///
    /// The pipeline splices one declaration per entry into the program and re-checks, because
    /// lowering walks the AST rather than the checker's tables.
    pub requested_instantiations: Vec<(String, crate::generics::Bindings)>,
    /// Call site -> the instantiated function that call resolved to.
    ///
    /// Keyed by `Span` exactly like `builtin_call_types`, and consumed by lowering the same
    /// way. See `Checker::generic_call_sites` for why lowering cannot re-derive it.
    pub generic_call_sites: HashMap<(String, Span), String>,
    /// Every generic CLASS instantiation a `new` asked for, as (template key, type arguments).
    pub requested_class_instantiations: Vec<(String, crate::generics::Bindings)>,
    /// The generic METHOD instantiations this round asked for.
    pub requested_method_instantiations: Vec<((String, String), crate::generics::Bindings)>,
    /// (enclosing body scope, call site) -> the instantiated method name that site selected.
    pub generic_method_sites: crate::fast_hash::FastMap<
        (String, crate::span::Span),
        std::collections::BTreeSet<String>,
    >,
    /// (enclosing body scope, `new` site) -> the instantiated class it resolved to, consumed by
    /// `generics::classes` as an AST rewrite. See `Checker::generic_new_sites` for the key.
    pub generic_new_sites: HashMap<(String, Span), std::collections::BTreeSet<String>>,
    pub global_env: TypeEnv,
    pub functions: HashMap<String, FunctionSig>,
    pub function_attribute_names: HashMap<String, Vec<String>>,
    pub function_attribute_args: HashMap<String, Vec<Option<Vec<AttrArgEntry>>>>,
    pub callable_param_sigs: HashMap<(String, String), FunctionSig>,
    /// Proven return-to-parameter storage aliases for source-declared callables.
    pub(crate) return_alias_summaries: ReturnAliasSummaries,
    #[allow(dead_code)]
    pub callable_return_sigs: HashMap<String, FunctionSig>,
    #[allow(dead_code)]
    pub callable_array_return_sigs: HashMap<String, FunctionSig>,
    pub interfaces: HashMap<String, InterfaceInfo>,
    pub classes: crate::fast_hash::FastMap<String, ClassInfo>,
    pub enums: HashMap<String, EnumInfo>,
    pub packed_classes: HashMap<String, PackedClassInfo>,
    pub extern_functions: HashMap<String, ExternFunctionSig>,
    pub extern_classes: HashMap<String, ExternClassInfo>,
    pub extern_globals: HashMap<String, PhpType>,
    pub required_libraries: Vec<String>,
    pub warnings: Vec<CompileWarning>,
    /// Statically-decided access violations lowered to runtime `Error` throws,
    /// keyed by function-like scope and the source span of the offending call/assignment.
    pub throw_access_sites: HashMap<(String, Span), ThrowAccessInfo>,
    /// Authoritative checker result types for builtin calls, keyed by function-like scope and
    /// call span so equal coordinates in different loaded files cannot overwrite one another.
    pub builtin_call_types: HashMap<(String, Span), PhpType>,
    /// Closure literals whose checked source bodies contain yield, keyed by source identity.
    pub closure_generator_spans: HashSet<Span>,
    /// Native array_push property decisions, scoped to the body and original place span.
    pub array_push_property_plans: HashMap<(String, Span), ArrayPushPropertyPlan>,
    /// Checked member receiver types by body scope and source node; conflicting visits are None.
    pub member_receiver_types: HashMap<(String, Span), Option<PhpType>>,
    /// Authoritative checker result types for property reads through flow-narrowed receivers.
    pub flow_typed_property_accesses: HashMap<(String, Span), PhpType>,
    /// Checker-authorized call-argument sites that may widen one local to a boxed Mixed cell.
    pub boxed_reference_promotion_sites: HashMap<(String, Span), HashSet<String>>,
    /// Reads proven to use native buffers on every checker visit to the source span.
    /// Buffer bounds checks cannot invoke PHP warning handlers.
    pub buffer_read_sites: HashSet<Span>,
    /// See `Checker::first_class_builtin_call_types`.
    pub first_class_builtin_call_types: HashMap<Span, PhpType>,
    /// Fixed-point array-local storage contracts keyed by function-like scope and loop span.
    pub loop_storage_types: LoopStorageTypes,
    /// `(function-like scope, local name)` pairs for `string` locals that are a `++`/`--`
    /// target, so EIR lowering can give them boxed `Mixed` storage from their first store.
    pub string_incdec_locals: HashSet<(String, String)>,
    /// Boxed caller-local storage contracts selected for source-declared by-reference calls.
    pub by_ref_local_storage_types: HashMap<(String, String), PhpType>,
    /// Lowercase bare names of callees whose by-reference parameter cell may outlive the call.
    pub ref_param_escaping_callees: HashSet<String>,
    /// Call-site locals passed to a property-bound by-reference parameter; see
    /// `Checker::property_bound_promotion_sites`.
    pub property_bound_promotion_sites: HashSet<(String, crate::span::Span)>,
    /// Call results passed to a by-reference parameter; see `Checker::by_ref_call_result_sites`.
    pub by_ref_call_result_sites: HashSet<crate::span::Span>,
    /// See `Checker::bcmath_operator_sites`.
    pub bcmath_operator_sites: HashSet<crate::span::Span>,
    /// See `Checker::stdclass_by_ref_walk_sites`.
    pub stdclass_by_ref_walk_sites: HashSet<crate::span::Span>,
    /// Names some `define()` call creates; see `Checker::runtime_define_names`.
    pub runtime_define_names: HashSet<String>,
    /// Constant names read but never defined; see `Checker::undefined_constant_reads`.
    pub undefined_constant_reads: HashSet<String>,
    /// Checker-proven types for dynamic property reference locals, keyed by scope and local.
    pub dynamic_ref_local_types: HashMap<(String, String), PhpType>,
    /// The `unset()` arguments whose local binding the checker killed, as span -> the SET of local
    /// NAMES killed at that position, so EIR lowering abandons the old frame slot (after releasing
    /// its value) instead of null-storing into it.
    ///
    /// The name is not decoration, it is half the key. A `Span` carries line/col and NOTHING
    /// about which file they are in, and include resolution splices every included file's AST
    /// into one program without rebasing line numbers, so line 4 column 1 of `main.php` and of
    /// `lib.php` are the SAME `Span`. Lowering must therefore check that the decision it found
    /// is about the variable in front of it; without that check a retype recorded in one file
    /// re-binds an unrelated same-position assignment in another (measured: a conditional
    /// `$w = …` in an included file lost its whole binding, printing `|s` where PHP prints
    /// `a1|s`).
    ///
    /// The value is a SET, exactly like [`CheckResult::mixed_storage_store_sites`], because two
    /// DIFFERENT locals can be killed at the same (line, column) in two different files. With one
    /// name per span the later insert silently EVICTED the earlier decision; the evicted site then
    /// fell back to the pre-feature null store, which widens the abandoned slot to boxed `Mixed`
    /// while every read already lowered above it keeps its narrower IR type — measured as `Fatal
    /// error: heap memory exhausted` on a loop of `string` reads where PHP prints its sum.
    ///
    /// The name does NOT make the key unique: the same NAME at the same line and column in two
    /// files is still one key for two sites, and that shape was measured printing `|s` where PHP
    /// prints `a1|5` — in a program that was a plain compile error before this feature existed.
    /// It is not left ambiguous. `checker::binding_decision_ambiguity` counts, per role, how many
    /// nodes each recorded key actually names and REJECTS the program when a key names more than
    /// one, so an ambiguous decision is a compile error and never a silent re-bind. Giving `Span`
    /// file identity would remove the ambiguity outright, and every span-keyed map here would
    /// want it.
    pub local_bind_kill_sites: HashMap<Span, HashSet<String>>,
    /// Unconditional statement-form unsets allowed to detach an ordinary promoted reference.
    /// Unlike binding kills, these preserve the checker's escaped-reference type restrictions.
    /// Lowering keeps the old capture storage type and retires only this binding's cell owner.
    pub local_ref_detach_sites: HashMap<Span, HashSet<String>>,
    /// The statement-form assignments the checker re-bound to a fresh binding of an incompatible
    /// type, as span -> the SET of local NAMES re-bound at that position, so EIR lowering mints a
    /// new slot there. Read by `crate::ir_lower` alongside `local_bind_kill_sites`, and keyed the
    /// same way for the same reasons — see that field.
    pub local_retype_sites: HashMap<Span, HashSet<String>>,
    /// Every statement-form assignment to a local the checker's syntactic pre-scan marked as
    /// whole-frame boxed `Mixed` storage (a branch-divergently assigned local), as span -> the SET
    /// of local NAMES boxed at that position, so EIR lowering declares the slot boxed before the
    /// FIRST store instead of inferring it from the first stored value.
    ///
    /// Keyed like the other two decision maps and for the same reason — see
    /// `local_bind_kill_sites` — and checked by `checker::binding_decision_ambiguity` against the
    /// same statement-form-assignment tally the retype decisions use, because both roles name
    /// exactly that node kind.
    ///
    /// The value is a SET because two DIFFERENT locals can be marked at the same (line, column) in
    /// two different files, and a `Span` cannot tell them apart. With one name per span the later
    /// body's decision evicted the earlier one silently — the evicted local dropped out of
    /// [`CheckResult::mixed_storage_local_names`] while the checker still typed it `Mixed`, and the
    /// compiler panicked (`strlen cannot lower checked operand type Int`) on valid PHP. Same-NAME
    /// collisions remain genuinely ambiguous and are still rejected by the ambiguity pass.
    pub mixed_storage_store_sites: HashMap<Span, HashSet<String>>,
    /// See `Checker::user_filters_reachable`: EIR metadata keeps the raw filter ABI only then.
    pub user_filters_reachable: bool,
    /// See `Checker::user_filter_registered`.
    pub user_filter_registered: Option<crate::fast_hash::FastSet<String>>,
}

impl CheckResult {
    /// Every span carrying a local-binding decision: kills, reference detaches, retypes and stores.
    ///
    /// Handed to the post-typecheck optimizer. These decisions are keyed BY SPAN and EIR lowering
    /// consults them by span, so any pass that CLONES an AST node would hand one decision to two
    /// syntactic sites — and abandoning a binding is not idempotent. The invariant is therefore on
    /// the OPTIMIZER as a whole: no pass may clone a node carrying one of these spans, and a pass
    /// that clones must consult this set and veto itself. Two passes clone today and both are
    /// guarded through `optimize::control::binding_decisions` — DCE's tail-sinking
    /// (`optimize::control::dce`) and the single-case switch rewrite reached from the prune and
    /// normalize phases (`optimize::control::switch::prune_switch_stmt`), which materializes a
    /// `switch` default body into both branches of a synthesized `if`.
    ///
    /// The mixed-storage store sites are included even though their consumer — declaring the slot
    /// `Mixed` when the local has none yet, and storing at `Mixed` rather than at the value's own
    /// type — IS idempotent under duplication (the second clone finds the slot already declared,
    /// and substitutes the same type). They are kept singular anyway because
    /// the ambiguity check that runs in the checker counts the nodes of the ORIGINAL program: a
    /// pass that cloned one of these statements afterwards would create a second site for a key
    /// that check already cleared, and "the decision names exactly one node" is the invariant the
    /// two halves of this feature are built on. The cost is one lost tail-sink per boxed local,
    /// plus one un-rewritten single-case switch per boxed local assigned in a `default` body.
    pub fn local_binding_decision_spans(&self) -> HashSet<Span> {
        self.local_bind_kill_sites
            .keys()
            .chain(self.local_ref_detach_sites.keys())
            .chain(self.local_retype_sites.keys())
            .chain(self.mixed_storage_store_sites.keys())
            .chain(
                self.boxed_reference_promotion_sites
                    .keys()
                    .map(|(_, span)| span),
            )
            .copied()
            .collect()
    }

    /// Every local NAME the mixed-storage marking compiled as boxed `mixed` frame storage.
    ///
    /// Handed to the post-typecheck constant propagator, which must not replace a read of one of
    /// these with a literal: the local is bound `mixed` for its whole frame and EIR lowering
    /// stores and reads it through that one type, so a literal of a concrete type hands lowering a
    /// view the checker never approved. See `optimize::binding_decisions`.
    ///
    /// This is the NAMES, not the spans, because the consumer is name-keyed. It is also a
    /// program-wide union rather than per-body: the AST passes have no body identity to key on,
    /// and over-approximating only costs constant propagation on a name some other body boxed.
    pub fn mixed_storage_local_names(&self) -> HashSet<String> {
        self.mixed_storage_store_sites
            .values()
            .flatten()
            .cloned()
            .collect()
    }
}

/// Runs type checking using the host platform (auto-detected from the build environment).
/// Returns `Ok(CheckResult)` on success, or `Err(CompileError)` if type checking fails.
/// The `CheckResult` carries the resolved type environment, function signatures, class
/// metadata, warnings, and any required native libraries for linking.
/// Delegates to `check_with_options` with default `CheckOptions`.
#[allow(dead_code)]
pub fn check(program: &Program) -> Result<CheckResult, CompileError> {
    check_with_options(program, checker::CheckOptions::default())
}

/// Runs type checking using the host platform with explicit `CheckOptions`
/// (e.g. `--strict-locals`). Returns `Ok(CheckResult)` on success, or
/// `Err(CompileError)` if type checking fails.
#[allow(dead_code)]
pub fn check_with_options(
    program: &Program,
    options: checker::CheckOptions,
) -> Result<CheckResult, CompileError> {
    check_with_options_and_bounds(program, options, &crate::generics::GenericContext::default())
}

/// Runs type checking on the host platform, answering the bound obligations that generic class
/// instantiation left behind.
///
/// The host-target counterpart of [`check_with_target_and_bounds`]; see it for why the
/// obligations travel separately from the program.
pub fn check_with_options_and_bounds(
    program: &Program,
    options: checker::CheckOptions,
    generics: &crate::generics::GenericContext,
) -> Result<CheckResult, CompileError> {
    checker::check_types_with_options(program, Target::detect_host(), options, generics)
}

/// Runs type checking targeting a specific platform (e.g., Linux instead of the host macOS).
/// Returns `Ok(CheckResult)` on success, or `Err(CompileError)` if type checking fails.
/// The target affects FFI metadata, required library resolution, and platform-specific type behavior.
/// Delegates to `check_with_target_and_options` with default `CheckOptions`.
///
/// No longer called by the compile pipeline directly (it now threads `CheckOptions`
/// via `check_with_target_and_options`), so this is dead code outside of tests.
#[allow(dead_code)]
pub fn check_with_target(program: &Program, target: Target) -> Result<CheckResult, CompileError> {
    check_with_target_and_options(
        program,
        target,
        checker::CheckOptions::default(),
        &crate::generics::GenericContext::default(),
    )
}

/// Runs type checking targeting a specific platform, answering the bound obligations that
/// generic class instantiation left behind.
///
/// The entry point for a caller driving [`crate::generics::monomorphize`] by hand: instantiating
/// `Box<User>` is syntax, but deciding that `User` satisfies `T : Entity` needs this checker's
/// class table, so the obligations travel from the one pass to the other.
#[allow(dead_code)]
pub fn check_with_target_and_bounds(
    program: &Program,
    target: Target,
    generics: &crate::generics::GenericContext,
) -> Result<CheckResult, CompileError> {
    check_with_target_and_options(
        program,
        target,
        checker::CheckOptions::default(),
        generics,
    )
}

/// Runs type checking targeting a specific platform with explicit `CheckOptions`
/// (e.g. `--strict-locals`). Returns `Ok(CheckResult)` on success, or
/// `Err(CompileError)` if type checking fails.
pub fn check_with_target_and_options(
    program: &Program,
    target: Target,
    options: checker::CheckOptions,
    generics: &crate::generics::GenericContext,
) -> Result<CheckResult, CompileError> {
    checker::set_program_evaluates_code(crate::ast_usage::collect(program).references("eval"));
    checker::check_types_with_options(program, target, options, generics)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Platform;
    use crate::codegen::platform::{Arch, Target};

    /// Parses PHP source text into an AST for use in tests.
    fn parse_program(source: &str) -> Program {
        let tokens = crate::lexer::tokenize(source).expect("tokenize failed");
        crate::parser::parse(&tokens).expect("parse failed")
    }

    /// Verifies that hashing builtins (here `md5`) require the pure-Rust
    /// `elephc_crypto` bridge on EVERY target after the Phase 2 migration off
    /// CommonCrypto/libcrypto — not the old linux-only system `crypto` library
    /// (which previously left macOS with no required library).
    #[test]
    fn test_hash_builtin_requires_elephc_crypto_on_all_targets() {
        let program = parse_program("<?php echo md5(\"abc\");");

        let linux = check_with_target(&program, Target::new(Platform::Linux, Arch::AArch64))
            .expect("linux type check failed");
        assert_eq!(linux.required_libraries, vec!["elephc_crypto"]);

        let mac = check_with_target(&program, Target::new(Platform::MacOS, Arch::AArch64))
            .expect("mac type check failed");
        assert_eq!(mac.required_libraries, vec!["elephc_crypto"]);
    }

    /// Verifies class declaration doc comments survive parsing and resolved schema construction.
    #[test]
    fn test_class_info_retains_declared_doc_comment() {
        let program = parse_program(
            "<?php /**\n * Schema metadata.\n */ class TypeCheckedDocumentedClass {}",
        );
        let checked = check_with_target(
            &program,
            Target::new(Platform::MacOS, Arch::AArch64),
        )
        .expect("type check failed");
        assert_eq!(
            checked
                .classes
                .get("TypeCheckedDocumentedClass")
                .and_then(|class| class.doc_comment.as_deref()),
            Some("/**\n * Schema metadata.\n */"),
        );
    }

    /// Verifies enum class metadata preserves flattened trait relation data for runtime reflection.
    #[test]
    fn test_enum_class_info_preserves_trait_metadata() {
        let program = parse_program(
            r#"<?php
trait EnumMetaTrait {
    public function original() {}
}
enum EnumMetaTarget {
    use EnumMetaTrait {
        original as aliasOriginal;
    }
    case Ready;
}
"#,
        );

        let result = check(&program).expect("type check failed");
        let enum_class = result
            .classes
            .get("EnumMetaTarget")
            .expect("missing enum class metadata");

        assert_eq!(enum_class.used_traits, vec!["EnumMetaTrait"]);
        assert_eq!(
            enum_class.trait_aliases,
            vec![(
                "aliasOriginal".to_string(),
                "EnumMetaTrait::original".to_string()
            )]
        );
    }
}
