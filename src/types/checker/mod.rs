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
pub(crate) mod core_attributes;
mod builtin_iterators;
mod builtin_json;
mod builtin_spl_classes;
pub(crate) mod builtin_class_gate;
mod builtin_spl_exceptions;
/// Shared builtin SPL parent edges for checker injection and exception-flow analysis.
pub(crate) use builtin_spl_exceptions::SPL_EXCEPTION_HIERARCHY;
/// builtin_stdclass
pub(crate) mod builtin_stdclass;
mod builtin_types;
mod builtin_user_filter;
pub(crate) mod builtins;
mod callables;
pub(crate) mod clone_override_storage;
pub(crate) mod scope_dynamic_storage;
mod driver;
mod extern_decl;
mod functions;
mod generic_bounds;
mod generic_method;
mod generic_new;
mod generic_variance;
mod inference;
#[cfg(test)]
pub(crate) use inference::INTERFACE_IMPLEMENTATION_SIG_COPIES;
pub(crate) mod loop_storage;
mod method_pass;
mod method_worklist;
mod member_receivers;
mod mixed_storage_scan;
mod numeric_regions;
mod packed_counter;

#[cfg(test)]
thread_local! {
    /// Classes `propagate_constructor_arg_type` visited, so a test can pin its cost to the program
    /// size: scanning every class per constructor argument is quadratic in the class count.
    pub(crate) static SHARED_PROPERTY_CLASSES_VISITED: std::cell::Cell<u64> =
        const { std::cell::Cell::new(0) };
    /// Owner classes `subtype_dispatch_owners` examined, so a test can pin its cost to the program
    /// size: re-walking every declarer's parent chain per call site is quadratic.
    pub(crate) static SUBTYPE_DISPATCH_OWNERS_EXAMINED: std::cell::Cell<u64> =
        const { std::cell::Cell::new(0) };
    /// Whole-table scans `descendant_class_names` made, so a test can pin them to the number of
    /// distinct ancestors instead of property accesses.
    pub(crate) static DESCENDANT_CLASS_SCANS: std::cell::Cell<u64> =
        const { std::cell::Cell::new(0) };
    /// Whole-table scans `direct_interface_implementers` made.
    pub(crate) static INTERFACE_IMPLEMENTER_SCANS: std::cell::Cell<u64> =
        const { std::cell::Cell::new(0) };
    /// Whole-table scans `record_scope_dynamic_mixed_receiver_mutation` made.
    pub(crate) static MIXED_RECEIVER_MUTATION_SCANS: std::cell::Cell<u64> =
        const { std::cell::Cell::new(0) };
}

mod ref_param_widening;
pub(crate) mod null_probe;
mod schema;
mod stmt_check;
mod type_compat;
#[cfg(test)]
pub(crate) use type_compat::SUBCLASS_QUERIES;
/// yield_validation
pub(crate) mod yield_validation;

use crate::fast_hash::{FastMap as HashMap, FastSet as HashSet};

use crate::codegen::platform::Target;
use crate::errors::CompileError;
use crate::parser::ast::{
    CallableTarget, Expr, ExprKind, Program, StmtKind, TypeExpr,
};
use crate::span::Span;
use crate::types::{
    collect_attribute_args, collect_attribute_names, CheckResult, ClassInfo, EnumInfo,
    ExternClassInfo, ExternFunctionSig, FunctionSig, InterfaceInfo, PackedClassInfo, PhpType,
    ThrowAccessInfo, TypeEnv,
};

pub use inference::{infer_expr_type_syntactic, infer_return_type_syntactic};
pub(crate) use loop_storage::loop_carried_storage_types;
pub(crate) use functions::call_validation::is_call_result_argument;
pub(crate) use inference::closure_body_uses_this;
pub(crate) use type_compat::set_program_evaluates_code;
pub(crate) use inference::php_declared_type_spelling;
pub(crate) use builtin_types::InterfaceDeclInfo;
pub(crate) use schema::normalize_signature_default_receivers;
pub(crate) use schema::normalize_lexical_callable_default;
pub(crate) use builtin_types::REFLECTION_CLASS_NAMES;
pub(crate) use builtin_types::{
    reflection_virtual_property_backing_in, reflection_virtual_property_type_in,
};
use builtin_types::validate_magic_method_contracts;
use schema::propagate_abstract_return_types;

/// Checker carries program-wide type-checking state including function signatures,
/// class/interface/enum definitions, variable environments, and warnings collected
/// during type checking.
/// Classes declaring each instance method name, for calls whose receiver is not a known class.
pub(crate) struct MethodOwnerIndex {
    /// `classes.len()` when the index was built; a different length means classes were added.
    pub(crate) class_count: usize,
    pub(crate) owners: crate::fast_hash::FastMap<String, std::sync::Arc<[String]>>,
}

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
    /// Immutable declared-contract failures cached independently from recursive placeholders.
    pub failed_variant_contracts: HashMap<String, CompileError>,
    /// Signatures whose body and return validation completed, rather than mere placeholders.
    pub completed_function_signatures: HashSet<String>,
    /// Folded names of `fn_decls`, `function_variant_groups` and `extern_functions`, tagged with
    /// the three table sizes they were folded from (the tables only grow). See
    /// `has_function_decl_folded`.
    pub(crate) folded_function_names:
        std::cell::RefCell<((usize, usize, usize), crate::fast_hash::FastSet<String>)>,
    /// Folded name -> canonical key of `functions`, `function_variant_groups` and `fn_decls`,
    /// tagged with their sizes. Insertions change a size; the one removal (`functions`, in
    /// `callables::first_class`) clears it. See `canonical_function_name_folded`.
    pub(crate) canonical_function_names: std::cell::RefCell<
        Option<((usize, usize, usize), [crate::fast_hash::FastMap<String, String>; 3])>,
    >,
    /// Canonical function signatures indexed by fully-qualified name.
    pub functions: HashMap<String, FunctionSig>,
    /// Functions whose body is currently being checked.
    ///
    /// Recursive calls may use their provisional signature, but must not re-specialize the same
    /// declaration while it is in flight.
    pub resolving_functions: HashSet<String>,
    /// Parameter types a recursive self-call asked for while its function was in flight.
    ///
    /// `resolving_functions` stops such a call from re-specializing the declaration it is inside,
    /// so without this the body was checked, and lowered, with the FIRST call's parameter types:
    /// `function f($key) { $s = is_array($key) ? $key : explode('.', $key); ... f($s); }` kept
    /// `$key` a string, and the recursive call converted its array to "Array" (Laravel's
    /// `data_get()`/`data_forget()`). `resolve_function_signature` re-runs the body with them.
    pub recursive_param_widenings: HashMap<String, Vec<(String, PhpType)>>,
    /// Functions a real call site invoked with an argument list.
    ///
    /// Membership in `functions` cannot answer this: a first-class callable reference (`h(...)`)
    /// resolves a function's signature without passing it anything, so it inserts an entry while
    /// leaving every untyped parameter on the `Int` placeholder. That is the same situation as a
    /// function no call site touched at all, and both need the pass-through return widening
    /// (issue #576).
    pub functions_called_directly: HashSet<String>,
    /// Top-level constant types indexed by canonical name.
    pub constants: HashMap<String, PhpType>,
    /// Tracks the return type of closures assigned to variables, keyed by variable name.
    pub closure_return_types: HashMap<String, PhpType>,
    /// Tracks known callable signatures for variables holding first-class callables,
    /// keyed by variable name.
    pub callable_sigs: HashMap<String, FunctionSig>,
    /// Callable signatures for closures stored in static properties, keyed `Class::$prop`.
    ///
    /// This map is program-wide rather than part of the per-body callable state: a static
    /// property is one shared slot whose signature must remain visible across method bodies.
    pub static_property_callable_sigs: HashMap<String, FunctionSig>,
    /// Tracks source-declared callable parameters in the active function body.
    pub callable_param_names: HashSet<String>,
    /// Tracks callable signatures inferred for user-function callable parameters,
    /// keyed by (function_name, param_name).
    pub callable_param_sigs: HashMap<(String, String), FunctionSig>,
    /// The signature each call site's argument gives a callable parameter, keyed like
    /// `callable_param_sigs` and then by the argument's span. `callable_param_sigs` holds their
    /// join, so one parameter reached by closures of different shapes is typed for all of them
    /// instead of for whichever site was checked last.
    pub callable_param_site_sigs: HashMap<(String, String), HashMap<crate::span::Span, FunctionSig>>,
    /// Whether the statement currently being checked was written in a file that opened with
    /// `declare(strict_types=1)`.
    ///
    /// PHP scopes the directive to the file containing the *call site*, so this is installed
    /// from `Stmt::strict_types` by `check_stmt` and restored afterwards. It is `false` outside
    /// any statement check, which keeps class/constant/default-value checking on PHP's coercive
    /// rules — the behaviour elephc had before the directive was honoured.
    pub strict_types: bool,
    /// Whether the checker is validating arguments synthesized by an internal callback site.
    /// These expressions describe engine-provided values, not caller-owned lvalues, so they
    /// must not publish reference aliases or boxed write-back metadata.
    pub internal_callback_binding: bool,
    /// Tracks which undeclared function parameters have already had their type
    /// adopted from a real call site, keyed by (function_name, param_index). The
    /// first such call adopts the actual argument type; later disagreeing calls
    /// widen the parameter to `Mixed` (so e.g. a parameter called with both an int
    /// and a string is `Mixed`, not collapsed to one type).
    pub param_specialization_seen: HashSet<(String, usize)>,
    /// Direct subclasses of every class, keyed by parent name, plus the `classes.len()` it was
    /// built for. Rebuilt whenever a class is registered, so it never answers for a stale table.
    ///
    /// `class_and_descendants` answers subclass queries from it (the late-bound `new static`
    /// target validation), which used to walk the parent chain from every registered class.
    pub class_children_index: Option<(usize, HashMap<String, Vec<String>>)>,
    /// The classes whose method table dispatches each `(implementing class, method key,
    /// is_static)` to that implementation, plus the `classes.len()` it was built for.
    ///
    /// Every method body's inferred return is copied to those classes after the body is
    /// checked. That used to scan the whole class table once per method body (methods ×
    /// classes per pass). Implementation maps are only written while classes are registered,
    /// which also changes the table's size, so the length is a sound staleness check.
    pub method_impl_users_index:
        Option<(usize, crate::fast_hash::FastMap<(String, String, bool), Vec<String>>)>,
    /// The classes sharing each `(property, declaring class)` slot, plus the `classes.len()` it
    /// was built for; the same staleness rule as `method_impl_users_index`. A constructor
    /// argument retypes that slot in every class sharing it, which used to scan the whole class
    /// table per argument of every `new`.
    pub property_sharers_index: Option<(usize, HashMap<(String, String), Vec<String>>)>,
    /// Untyped interface positions whose implementation uses a stable boxed ABI.
    pub interface_method_boxed_params: HashSet<(String, usize)>,
    /// Every generic instantiation a call site asked for, as (template name, type arguments).
    ///
    /// The checker instantiates into `fn_decls`/`functions` so the CALL type-checks, but
    /// lowering walks the AST, not the checker's tables. The pipeline therefore reads this
    /// list, splices a real declaration per entry into the program, and re-checks — repeating
    /// until no new entry appears, since an instantiated body may itself call a template.
    /// Deduplicated by instantiated name, so the fixpoint terminates for any program without
    /// polymorphic recursion.
    pub requested_instantiations: Vec<(String, crate::generics::Bindings)>,
    /// (enclosing function, call site) -> the instantiated function that call resolved to.
    ///
    /// Lowering cannot re-derive this: picking the instantiation needs the argument TYPES, and
    /// `lower_args_with_signature` needs the signature before it lowers the arguments. So the
    /// checker's answer is carried across, the way `builtin_call_types` already is.
    ///
    /// The ENCLOSING FUNCTION is half the key, and it has to be. One source position inside a
    /// generic template is reached once per instantiation of that template, and legitimately
    /// resolves differently each time: `twice<T>() { return identity($value); }` calls
    /// `identity<int>` from `twice<int>` and `identity<string>` from `twice<string>`, at the
    /// same line and column. Keying on `Span` alone collapsed those two into one and made the
    /// second call invoke the first's function.
    ///
    /// The scope half is `current_loop_storage_scope` (`main`, a function's name,
    /// `Class::method`, or a closure's nested scope), which lowering spells the same way and
    /// `generics::site_scope` rebuilds for the AST rename passes. All three generic site maps
    /// use it.
    ///
    /// `Span` still carries no file identity, so two calls at the same position inside
    /// same-named functions in two included files remain indistinguishable. That residue is a
    /// hard error rather than a silent miscompile — see `instantiate_generic_call`.
    pub generic_call_sites: HashMap<(String, Span), String>,
    /// The generic class templates, for inferring type arguments at `new Box(5)`.
    ///
    /// Empty on every path that does not run `generics::monomorphize`, which makes those paths
    /// behave exactly as they did before this feature: a bare `new Box(…)` on a template is an
    /// unknown class, as it was.
    pub class_templates: Vec<crate::generics::classes::TemplateSignature>,
    /// Every generic METHOD the program declares, by (class key, method key).
    ///
    /// Rebuilt each fixpoint round, because a template rides along when its class is
    /// instantiated: `Box<int>` gets `map<U>` from `Box<T>`.
    pub method_templates: crate::fast_hash::FastMap<
        (String, String),
        crate::generics::methods::MethodTemplate,
    >,
    /// The method instantiations this round asked for, as (class key, method key, arguments).
    pub requested_method_instantiations:
        Vec<((String, String), crate::generics::Bindings)>,
    /// (enclosing body scope, call site) -> the instantiated method name that site selected.
    pub generic_method_sites: crate::fast_hash::FastMap<
        (String, crate::span::Span),
        std::collections::BTreeSet<String>,
    >,
    /// Every generic CLASS instantiation a `new` asked for, as (template key, type arguments).
    ///
    /// The mirror of `requested_instantiations` for classes. `generics::classes` splices one
    /// declaration per entry on the next round.
    pub requested_class_instantiations: Vec<(String, crate::generics::Bindings)>,
    /// (enclosing body scope, `new` site) -> the instantiated class that construction resolved to.
    ///
    /// The ENCLOSING BODY is half the key for the same reason it is in `generic_call_sites`:
    /// one position inside a generic function is reached once per instantiation of it, and
    /// legitimately means a different class each time. `function wrap<T>(T $v) { new Box($v); }`
    /// builds a `Box<int>` from `wrap<int>` and a `Box<string>` from `wrap<string>`, at one line
    /// and column. The bodies are DIFFERENT AST nodes after splicing — a clone keeps its source
    /// spans — so the span alone was never enough to tell them apart.
    ///
    /// The consumer is not lowering but `generics::classes`, which rewrites the AST node in
    /// place; once it has, the program names the instantiated class outright and every later
    /// pass — reachability included — sees an ordinary construction with nothing to re-derive.
    ///
    /// The value is a SET because the key still cannot separate everything: a `Span` carries no
    /// file identity, so two same-named functions in two included files collide. That residue
    /// is a compile error naming both classes, not a choice between them.
    pub generic_new_sites: HashMap<(String, Span), std::collections::BTreeSet<String>>,
    /// `(class, method key, is_static, parameter index)` for every undeclared parameter the LAST
    /// method pass bound as `mixed` because no call site refined its `int` inference seed.
    pub unspecialized_seed_params: HashSet<(String, String, bool, usize)>,
    /// `(class, method key, is_static)` of undeclared-return methods whose own body was refused
    /// while a call to THEMSELVES was still typed by the `int` return seed (`array_merge($r,
    /// $this->walk(...))` inside `walk()`). Their self-recursive calls are typed `mixed`: the
    /// return type is only inferred from a body that checks, and the body cannot check until the
    /// recursive call has a type -- php resolves it at run time.
    pub recursive_mixed_methods: HashSet<(String, String, bool)>,
    /// Untyped method parameters the body tests with `instanceof`, keyed by (class, method key,
    /// index): they stay `mixed` instead of taking the type of their call sites.
    pub instanceof_tested_params: HashSet<(String, String, usize)>,
    /// Tracks callable signatures inferred for user-function callable returns.
    pub callable_return_sigs: HashMap<String, FunctionSig>,
    /// Tracks callable element signatures inferred for user-function array returns.
    pub callable_array_return_sigs: HashMap<String, FunctionSig>,
    /// Tracks capture payloads for closures assigned to variables, keyed by variable name.
    /// Each entry is (capture_name, capture_type, is_by_ref).
    pub callable_captures: HashMap<String, Vec<(String, PhpType, bool)>>,
    /// Tracks callable-array targets assigned to variables, keyed by variable name.
    pub callable_array_targets: HashMap<String, CallableTarget>,
    /// Identifies the assignment that produced each callable-array target fact.
    pub callable_array_target_versions: HashMap<String, u64>,
    /// Monotonic source for callable-array target assignment identities.
    pub next_callable_array_target_version: u64,
    /// Tracks first-class callable targets assigned to variables, keyed by variable name.
    pub first_class_callable_targets: HashMap<String, CallableTarget>,
    /// Tracks `ReflectionClass` locals whose reflected class is statically known.
    pub reflection_class_targets: HashMap<String, String>,
    /// Interface definitions collected during the first pass, keyed by canonical name.
    pub interfaces: HashMap<String, InterfaceInfo>,
    /// Doc comments of user interface declarations, by declared name, for `InterfaceInfo`.
    pub interface_doc_comments: HashMap<String, String>,
    /// Class definitions collected during the first pass, keyed by canonical name.
    pub classes: crate::fast_hash::FastMap<String, ClassInfo>,
    /// What the method fixpoint knows of each body, kept across its runs.
    method_worklist: method_worklist::MethodWorklist,
    /// `names_an_operator_class` over `classes`, with the table size it was computed for. Exact:
    /// the checker only ever adds class names, so an unchanged size is an unchanged key set.
    operator_classes: std::cell::Cell<Option<(usize, bool)>>,
    /// Instance method name -> the classes declaring it, in `classes` iteration order, with the
    /// `classes.len()` it was built at. See `method_owners_of`.
    pub(crate) method_owners: std::cell::RefCell<Option<MethodOwnerIndex>>,
    /// Instance-method owners compatible with a receiver type, keyed `(receiver, method)`, with the
    /// `classes.len()` they were computed at. Which classes extend or implement a receiver is fixed
    /// once the class table is built; asking again for every call site walked each owner's parent
    /// chain. See `subtype_dispatch_owners`.
    pub(crate) subtype_dispatch_cache: std::cell::RefCell<(usize, crate::fast_hash::FastMap<(String, String), std::sync::Arc<[String]>>)>,
    /// Classes extending or implementing an ancestor, sorted, keyed by the ancestor, with the
    /// `classes.len()` they were computed at. Cleared with `subtype_dispatch_cache`. See
    /// `descendant_class_names`: every property access on a class scanned the whole table.
    pub(crate) descendant_classes_cache: std::cell::RefCell<(usize, crate::fast_hash::FastMap<String, std::sync::Arc<[String]>>)>,
    /// Classes naming an interface in their own `implements` list, sorted, keyed by the
    /// interface, with the `classes.len()` they were computed at. Cleared with
    /// `subtype_dispatch_cache`. See `direct_interface_implementers`.
    pub(crate) interface_implementers_cache: std::cell::RefCell<(usize, crate::fast_hash::FastMap<String, std::sync::Arc<[String]>>)>,
    /// Canonical class names declared in the program, available for forward references
    /// before the full class definitions are available.
    pub declared_classes: HashSet<String>,
    /// Declared parent classes, available before full `ClassInfo` construction.
    ///
    /// Override covariance may mention a sibling class whose metadata has not been built yet, so
    /// nominal subtype checks must not depend on lexicographic class-processing order.
    pub declared_class_parents: HashMap<String, Option<String>>,
    /// Directly declared interfaces for each class, available before full metadata construction.
    pub declared_class_interfaces: HashMap<String, Vec<String>>,
    /// Canonical names of the classes declared by compiler-generated statements (an injected
    /// prelude, `SourceMode::Internal`). Together with the builtins that have no source span,
    /// these are the classes `ClassInfo::is_internal` marks as elephc's own.
    pub internal_class_decls: HashSet<String>,
    /// Enum definitions collected during the first pass, keyed by canonical name.
    pub enums: HashMap<String, EnumInfo>,
    /// Canonical interface names declared in the program, available for forward references
    /// before the full interface definitions are available.
    pub declared_interfaces: HashSet<String>,
    /// Canonical trait names declared in the program, available for reflection
    /// and class-like metadata probes that accept traits.
    pub declared_traits: HashSet<String>,
    /// Nominal types introduced only by unresolved catch clauses.
    ///
    /// PHP guarantees that a value reaching such a clause implements `Throwable`, even though
    /// the absent declaration must not be installed in the runtime class registry.
    pub unresolved_catch_types: HashSet<String>,
    /// Reflection-visible method signatures declared directly on each trait.
    pub declared_trait_methods: HashMap<String, HashMap<String, FunctionSig>>,
    /// Reflection-visible class constant names declared directly on each trait.
    pub declared_trait_constants: HashMap<String, HashSet<String>>,
    /// Properties imported from traits, distinguished from directly declared class defaults.
    pub trait_imported_properties: HashMap<String, HashSet<String>>,
    /// Classes whose own flattened constructor was imported from a trait.
    pub trait_imported_constructors: HashSet<String>,
    /// Name of the class currently being type-checked (used for `$this` resolution).
    pub current_class: Option<String>,
    /// Name of the current method being type-checked, when inside a class body.
    pub current_method: Option<String>,
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
    /// The single class a closure literal is being bound to by `Closure::bind($closure, $object)`
    /// while that literal is type-checked, when the object's class is statically known.
    ///
    /// A by-reference closure returning `$this->prop` promotes the property it names. With no
    /// receiver known it must promote it on EVERY class declaring that name, and each of those
    /// classes then stores the property in an owned reference cell the runtime object walkers
    /// (`(array)`, `var_export`, `get_object_vars`, the polyfill DeepClone) do not read through.
    /// Symfony's `KernelTrait` binds `fn &() => $this->instanceof` to a `PhpFileLoader` and
    /// thereby turned `Definition::$instanceof` into such a cell for every container definition.
    pub closure_bound_this_class: Option<String>,
    /// Extern function declarations (e.g. `extern "C" { function foo(): void; }`).
    pub extern_functions: HashMap<String, ExternFunctionSig>,
    /// Extern class (C struct) declarations keyed by canonical name.
    pub extern_classes: HashMap<String, ExternClassInfo>,
    /// Packed layout-only records (`packed class`), keyed by canonical name.
    pub packed_classes: HashMap<String, PackedClassInfo>,
    /// Canonical names of every packed or extern record declared in the program.
    ///
    /// This predeclaration set lets `ptr<T>` fields name their own record or a record whose
    /// layout has not been built yet, without making duplicate detection treat each record as
    /// colliding with itself.
    pub declared_native_records: HashSet<String>,
    /// Extern global variable declarations, keyed by variable name.
    pub extern_globals: HashMap<String, PhpType>,
    /// Libraries required by `#[link]` attributes on extern blocks, in link order.
    pub required_libraries: Vec<String>,
    /// Best-known top-level variable types visible to `global` statements in the
    /// current file scope.
    pub top_level_env: TypeEnv,
    /// Names that are by-ref parameters in the current function/closure scope.
    pub active_ref_params: HashSet<String>,
    /// The by-reference PARAMETERS of the current body that carry no type declaration.
    ///
    /// Unlike `active_ref_params`, which `$x = &$y` also joins, this names parameters only. Such a
    /// parameter's cell is `mixed`: its declared type is `mixed`, and the caller boxes its
    /// argument from the callee's effective parameter type. See `merge_local_assignment_type`.
    pub untyped_ref_params: HashSet<String>,
    /// Active incoming by-reference parameters and captures whose storage is owned outside this frame.
    ///
    /// `active_ref_params` additionally accumulates local `=&` targets while checking the body,
    /// so it cannot distinguish external storage from a frame-owned alias. This subset follows
    /// the incoming binding until `unset()` detaches the local name from the external cell.
    pub active_external_ref_bindings: HashSet<String>,
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
    /// The `active_statics` whose declaration left them `Mixed` (`static $x = null;`): their cell
    /// is boxed for the whole body, so every store keeps the name `Mixed`.
    pub mixed_static_locals: HashSet<String>,
    /// Names bound as `foreach` loop keys in the current function/closure scope.
    /// A foreach key is a boxed `Mixed` cell at runtime (`Op::IterCurrentKey`)
    /// even when the checker types it as `Int`/`Str` from the source array, so an
    /// `$dst[$k] = $v` write under such a key must defer the indexed-vs-hash
    /// decision to `Op::ArraySetMixedKey` (destination `Array(Mixed)`) instead of
    /// promoting to `AssocArray` like a statically-known string key would. Mirrors
    /// the lowering's `foreach_int_key_locals` lifetime (per function, not popped).
    pub foreach_key_locals: HashSet<String>,
    /// The counter of the innermost `for` loop whose writes stay on packed storage, with the
    /// `local_conditional_depth` its body runs at.
    ///
    /// Packed storage has no keys — slot `n` IS key `n` — so a write past the logical end has
    /// to zero-fill the gap, and php would instead have promoted the array to a hash. The
    /// checker therefore refuses packed storage for a write it cannot bound against the
    /// array's length (see `array_key_contiguity_is_unproven`), EXCEPT for the one shape it
    /// can bound: a `for ($i = 0; …; $i++)` counter, written unconditionally in that loop's
    /// body, into an array that is still empty. There `$i` is 0 at the first write and grows
    /// one at a time with the array, so no gap is reachable.
    ///
    /// The depth is what makes "unconditionally" checkable: the loop body runs one conditional
    /// scope deeper than the `for` statement, and an `if` inside adds another, so a write at any
    /// greater depth may be skipped and is not covered. `Some` only ever names a loop whose body
    /// also has no `continue` to skip the write with, and no second write to the counter.
    pub(crate) packed_loop_counter: Option<packed_counter::PackedLoopCounter>,
    /// Whether the active local statement stream has crossed an `eval()` call.
    ///
    /// Once set, unknown local reads are treated as dynamic `Mixed` values because
    /// eval fragments can create caller-scope variables at runtime.
    pub eval_barrier_active: bool,
    /// Whether this PROGRAM can bring a class into existence at run time, so a name the walk
    /// cannot resolve must be deferred to runtime resolution instead of refused.
    ///
    /// PROGRAM-WIDE AND POINT-IN-TIME-FREE, which is the whole reason it is not
    /// `eval_barrier_active`. That flag is set only once the walk has PASSED a literal `eval()`
    /// call (`inference::expr::effects` is its sole writer), and neither of the two routes this
    /// records is an `eval` call at all: an `include`/`require` whose path is not a literal, and
    /// a registered autoloader. Both make classes appear that no static walk can see, and both
    /// can sit textually BELOW the `new` that needs them — `spl_autoload_register` at the top of
    /// a bootstrap and the `new` in a function called later is the ordinary Symfony shape.
    ///
    /// Filled once in `check_types_impl`, before the first walk, from the same
    /// `prelude_prune::usage` scan the builtin-class gates read.
    ///
    /// THE FAILURE THIS FIXES IS A REFUSED COMPILE, not a miscompile. `php -n` 8.5.6 exits 0 on
    /// both reducers; elephc reported `Undefined class: MadeByLoader` and
    /// `ReflectionClass::__construct(): undefined class 'S1Plain'` at TYPE CHECK. Lowering was
    /// always ready for this: `ir_lower::stmt::includes::lower_include` calls
    /// `ctx.apply_eval_barrier()`, so `ir_lower::expr::object_construction` already lowers a
    /// `new` of an unknown class to `Op::EvalObjectNew`, which resolves the name at run time
    /// through the bridge. Only the checker stood in the way.
    ///
    /// DEFERRING IS NOT ACCEPTING. A name nothing supplies still has to fail, and it fails where
    /// PHP fails it — at run time, with the catchable `Error` reading `Class "X" not found`,
    /// which `interpreter::expressions::evaluation::eval_new_object_result` raises after the
    /// autoload chain comes back empty.
    pub program_defers_unknown_classes: bool,
    /// Whether interpreted code can call into this program: it runs `eval()`, includes a file
    /// chosen at run time, or registers an autoloader. Code the compiler never sees may then call
    /// any public method with any argument, so facts drawn from the compiled call sites alone do
    /// not describe every caller.
    pub program_has_interpreted_callers: bool,
    /// Whether any checked `eval()` can select an AOT callable through the runtime registry.
    ///
    /// Function resolution can encounter eval while the selected function itself is still in
    /// flight, so the driver repeats the idempotent variadic-container promotion after all free
    /// functions have signatures.
    pub eval_native_callables_reachable: bool,
    /// Types recorded for `return <expr>;` statements at the moment each one was checked,
    /// keyed by the statement node's address in the AST plus its span.
    ///
    /// `collect_return_infos` runs *after* a body has been checked and re-infers every return
    /// against the body's FINAL environment. Without this side channel a flow-sensitive fact
    /// that only holds on part of the body — a property narrowing established halfway down —
    /// would be applied to returns that execute before it, silently accepting a nullable
    /// return. The address key is exact because both passes borrow the same immutable AST; the
    /// span is carried alongside so a recycled address can never be mistaken for a hit.
    pub flow_typed_returns: HashMap<(String, usize), (Span, PhpType)>,
    /// Return types widened so every untyped member of an override family returns in one
    /// storage, keyed by (class, method key, is static). See
    /// `Checker::unify_override_family_return_types`.
    pub(crate) family_return_widenings: HashMap<(String, String, bool), PhpType>,
    /// Checker-proven result types for property reads whose receiver was flow-narrowed.
    ///
    /// EIR lowering cannot reconstruct synthetic property facts from statement-local checker
    /// environments, so these scoped source sites preserve the required storage representation.
    pub flow_typed_property_accesses: HashMap<(String, Span), PhpType>,
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
    /// One frame per enclosing `break` target (loop or `switch`), innermost last, holding the
    /// environment every `break` that leaves THAT construct carried out of it.
    ///
    /// A `break` is an exit edge of its target, and the environment after the construct has to
    /// join it: `$n = count($m); foreach ($m as $v) { if ($v) { $n = $m; break; } }` leaves
    /// `$n` an `int` OR an `array`, but the `if` join drops the arm that breaks (it does not fall
    /// through) and the loop resumed from the fallthrough environment alone -- `int`. Every
    /// caller was then compiled against a return type that claimed `int` for what was a boxed
    /// array; Symfony's DeepClone polyfill does exactly this, and each container rebuild read a
    /// pointer as `array_fill()`'s count.
    pub loop_break_envs: Vec<Vec<TypeEnv>>,
    /// Stacks of break/continue depths at each enclosing `finally` block boundary,
    /// used to restore correct depth when branching through `finally`.
    pub finally_break_continue_bases: Vec<usize>,
    /// Stable function-like scope key used to disambiguate identical loop spans.
    pub current_loop_storage_scope: String,
    /// `(scope, receiver key, method key)` triples a `method_exists()` call has tested in a body.
    ///
    /// A body that asks whether a method is there has declared that it does not know, so a call
    /// the closed world cannot resolve is admitted and left to fault at run time the way PHP does
    /// — see `infer_lenient_subtype_method_call`. Keyed by scope so one body's uncertainty never
    /// admits another's typo.
    pub method_exists_guards: crate::fast_hash::FastSet<(String, String, String)>,
    /// Explicit local type declarations keyed by function-like scope and variable name.
    /// Ordinary PHP locals may change type after any assignment; only this extension syntax
    /// keeps a declaration contract across later writes.
    pub declared_local_types: HashMap<(String, String), PhpType>,
    /// Warnings raised during type checking (e.g. `#[\Deprecated]` call sites).
    /// Merged with AST-only warnings from `collect_warnings` before being returned
    /// in `CheckResult`.
    pub warnings: Vec<crate::errors::CompileWarning>,
    /// Latest foreach diagnostic per body/site. Method return seeds can look scalar on an
    /// early walk, so a later walk of the same site must replace that provisional answer.
    /// The optional function owner also identifies loops inside its nested closures.
    pub foreach_warnings: HashMap<(String, Span), (Option<String>, crate::errors::CompileWarning)>,
    /// `(class, property)` pairs for regular properties that had a reference taken
    /// (`$x = &$obj->prop`, by-reference return of `$obj->prop`). Recorded while
    /// checking bodies and applied to `classes` after checking so every access lowers
    /// through the property's ref-cell. See `apply_reference_property_promotions`.
    pub reference_property_promotions: HashSet<(String, String)>,
    /// `(target class, target property, source class, source property, span)` for every
    /// `$a->p =& $b->q` between declared properties. Both slots then share ONE cell, so their
    /// payload representations have to agree; `validate_reference_property_bindings` checks that
    /// once the class table has settled.
    pub reference_property_bindings: Vec<(String, String, String, String, crate::span::Span)>,
    /// Classes a two-argument `clone()` in this program can hand an override array to.
    ///
    /// Recorded by the `clone` builtin's check hook, and applied to `classes` after checking by
    /// `clone_override_storage::widen_clone_override_property_storage`: a PHP untyped property is
    /// `mixed`, so a slot an override can reach must carry runtime-shaped storage rather than the
    /// narrow type its default happened to infer.
    pub clone_override_destinations: clone_override_storage::CloneOverrideDestinations,
    /// Fixed untyped property slots that a reachable `unset()` may remove.
    ///
    /// Applied after body checking so the physical slot and every inherited view use boxed
    /// `Mixed` storage, which can safely represent the post-unset null value.
    pub property_unset_destinations: clone_override_storage::PropertyUnsetDestinations,
    /// Classes where a reachable mutation in this program can create a dynamic property.
    ///
    /// Recorded by the property-write and `unset` checks, and applied to `classes` after checking
    /// by `scope_dynamic_storage::reserve_scope_dynamic_property_storage`: a runtime name can miss
    /// every declared slot, while a strict ancestor's private name becomes a distinct dynamic
    /// property outside its declaring scope. Both need per-instance hash storage.
    pub scope_dynamic_mutation_targets: scope_dynamic_storage::ScopeDynamicMutationTargets,
    /// Statically-decided access violations that must lower to a catchable `Error` throw instead
    /// of a compile-time error, keyed by function-like scope and source span.
    ///
    /// Record through `record_throw_access_site`, which refuses a span that cannot identify one:
    /// lowering looks entries up by the scope and span of the node it is lowering, so a span
    /// shared by many nodes makes all of them throw. See that function for why the preludes make
    /// this reachable rather than theoretical.
    pub throw_access_sites: HashMap<(String, Span), ThrowAccessInfo>,
    /// One-based position of the regular parameter the argument being bound fills, or zero when
    /// unknown. Set by the call validators around each bind so a definite mismatch can name
    /// `Argument #N` when it is deferred to a run-time `TypeError`.
    pub(crate) binding_position: usize,
    /// Authoritative result type of each checked builtin call, keyed by function-like scope and
    /// call span so separately loaded sources may reuse the same line and column safely.
    /// EIR lowering consumes this instead of reimplementing builtin return inference.
    pub builtin_call_types: HashMap<(String, Span), PhpType>,
    /// Closure literals whose checked source bodies contain yield, keyed by source identity.
    pub closure_generator_spans: HashSet<Span>,
    /// Native property reference plans retain checker facts across opaque argument effects.
    pub array_push_property_plans: HashMap<(String, Span), crate::types::ArrayPushPropertyPlan>,
    /// Receiver types observed at member uses, with conflicting checker visits disabled.
    pub member_receiver_types: HashMap<(String, Span), Option<PhpType>>,
    /// Named-function contexts and body scopes queried without their local branch guards.
    pub member_receiver_analysis_scopes: Vec<(Option<String>, String)>,
    /// Caller locals converted to canonical boxed storage by a validated by-reference call.
    /// Scoped call sites keep recursive signature checking from changing another body's env.
    pub boxed_reference_outputs: HashMap<(String, Span), HashMap<String, PhpType>>,
    /// Argument sites where the checker authorized widening an ordinary local to a Mixed cell.
    /// EIR lowering consumes this instead of inferring permission from a broad builtin signature.
    pub boxed_reference_promotion_sites: HashMap<(String, Span), HashSet<String>>,
    /// Result types for builtin calls reached through a FIRST-CLASS CALLABLE variable.
    ///
    /// Kept apart from `builtin_call_types` because that map is keyed by the span of whatever
    /// call the checker was inferring, and for `call_user_func($f, …)` that is the OUTER call.
    /// Lowering resolves `$f` statically and would otherwise read the outer call's `Mixed` as
    /// the inner builtin's result — the exact span collision that made a `bool`-returning
    /// builtin's raw result get dereferenced as a pointer. Only a span whose callee the checker
    /// itself resolved gets an entry here.
    pub first_class_builtin_call_types: HashMap<Span, PhpType>,
    /// Fixed-point storage contracts keyed by function-like scope and loop span.
    pub loop_storage_types: crate::types::LoopStorageTypes,
    /// The loop spans `loop_storage_types` holds for each scope, so a re-walk clears its scope
    /// in time proportional to that scope's loops. Clearing used to `retain` over the whole map
    /// once per method, function and closure body, which on Laravel was quadratic.
    pub loop_storage_scope_spans: HashMap<String, Vec<crate::span::Span>>,
    /// `(scope, local)` pairs for `string` locals used as a `++`/`--` target.
    ///
    /// PHP's string increment can change the value's type (`"9"++` is `int(10)`), so EIR
    /// lowering must give those locals boxed `Mixed` frame storage from their FIRST store
    /// instead of widening the slot at the increment. Recorded here because the checker
    /// already visits every expression with a typed environment, so no second AST walk is
    /// needed. See `crate::ir_lower::context::LoweringContext::required_local_storage_type`.
    pub string_incdec_locals: HashSet<(String, String)>,
    /// Checker-selected boxed storage contracts for caller locals passed through
    /// source-declared by-reference parameters whose writable type is represented as `Mixed`.
    pub by_ref_local_storage_types: HashMap<(String, String), PhpType>,
    /// Lowercase bare names of functions and methods with a by-reference parameter that a
    /// closure binds by reference (directly or through forwarding), so the cell may outlive the
    /// call. See `Checker::scan_widened_ref_params`.
    pub ref_param_escaping_callees: HashSet<String>,
    /// `(folded callee scope, parameter)` for by-reference parameters some body binds a property
    /// to (`$this->p = &$param`), directly or by forwarding. See `binds_property_to_ref_param`.
    pub property_bound_ref_params: HashSet<(String, String)>,
    /// `(caller scope, argument span)` of every local passed to such a parameter: lowering gives
    /// that local a managed cell the property can keep.
    pub property_bound_promotion_sites: HashSet<(String, Span)>,
    /// Spans of call results passed to a by-reference parameter (`f(g())`): php binds a
    /// temporary and raises "Only variables should be passed by reference", which lowering
    /// emits after evaluating the argument.
    pub by_ref_call_result_sites: HashSet<Span>,
    /// Binary and unary operations typed as a `BcMath\Number` operator overload; the lowering
    /// rewrites exactly these into the prelude's helper calls (`bcmath_number_prelude`).
    pub bcmath_operator_sites: HashSet<Span>,
    /// By-reference `foreach` walks of a `stdClass` the checker checked as their property-name
    /// loop; the lowering lowers exactly these the same way (`stdclass_by_ref_walk`).
    pub stdclass_by_ref_walk_sites: HashSet<Span>,
    /// Names some `define()` call creates, as the call spells them without a leading `\`.
    ///
    /// Whether such a constant exists is a RUNTIME fact -- the call may sit in a branch, a
    /// function, or a file a request never reaches -- so `defined()` on one of these names reads
    /// the define's own flag instead of folding, and a `--web` request clears the flags.
    pub runtime_define_names: HashSet<String>,
    /// Constant names the program reads that nothing declares or `define()`s, without a leading
    /// `\`. php compiles such a read and throws `Error: Undefined constant "X"` when it runs, so
    /// the checker types it `mixed`, warns, and lowering emits the runtime lookup that throws.
    pub undefined_constant_reads: HashSet<String>,
    /// DECLARED by-reference parameters whose own body stores a value the declaration cannot
    /// represent, as `(body scope key, parameter name)`.
    ///
    /// PHP's reference cell is untyped — a declared `int &$i` gates what may be BOUND to it, not
    /// what the body may later store through it — so such a parameter needs one boxed cell, and
    /// the callee's cell IS the caller's slot. Computed once by
    /// [`Checker::scan_widened_ref_params`] over EVERY body, because the walk order does not
    /// guarantee a callee is checked before its callers, and read by the callee's environment
    /// seeding and by every call site's argument storage.
    pub widened_ref_params: HashSet<(String, String)>,
    /// The same decisions under the DECLARATION's own spelling, in discovery order.
    ///
    /// `widened_ref_params` folds its keys so a call site's spelling of the callee matches; this
    /// keeps the exact one, which is what
    /// [`Checker::publish_widened_ref_param_signatures`] needs to find the signature to rewrite
    /// and what EIR lowering builds when it declares the callee's own parameter slot.
    pub widened_ref_param_decls: Vec<(String, String)>,
    /// String suffixes known for locals in a function-like scope.
    ///
    /// A binding such as `$property = $type.'Passes'` records `"Passes"`. Dynamic property
    /// reference checking uses that constraint to select only representation-compatible slots.
    pub string_suffix_locals: HashMap<(String, String), String>,
    /// Checker-proven types for locals introduced by dynamic property reference binds.
    pub dynamic_ref_local_types: HashMap<(String, String), PhpType>,
    /// Mirrors `CheckOptions::lint`: emit the PHP-divergent notices (unused variables,
    /// unreachable code, permissive local retypes) while this check runs.
    pub lint_warnings: bool,
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
    /// For each local currently VIEWED through a flow guard's narrowing, the type its binding
    /// held before the guard narrowed it, paired with the view the guard published.
    ///
    /// A guard narrows by overwriting the environment entry (`control_flow` inserts
    /// `guard.then_ty` / `guard.else_ty`), which is right for reads but leaves an assignment
    /// inside the guarded region measuring itself against a FLOW FACT about the value instead
    /// of against the binding that has to hold the new one. That is how the commonest PHP
    /// fallback idiom there is became a compile error (issue #509):
    ///
    /// ```php
    /// $g = glob($dir . '/*.meta');      // array<string>|false
    /// if ($g === false) { $g = []; }    // "cannot reassign $g from false to array<never>"
    /// ```
    ///
    /// The store is fine — the binding is `array<string>|false` and an empty array is one of
    /// the things it holds — so `merge_local_assignment_type` retries a failed merge against
    /// the recorded origin before reporting.
    ///
    /// Empty outside a guarded region, and reset per body — a name narrowed in the caller says
    /// nothing about a same-named local in a closure checked inside the branch.
    pub narrowed_local_origins: HashMap<String, NarrowedLocalOrigin>,
    /// Locals of the current body that a reference can reach: `=&` target or source,
    /// by-reference closure captures (`use (&$x)`), any plain variable passed to a
    /// by-reference parameter, and BOTH names a `foreach ($arr as &$v)` touches — the iterable
    /// the loop holds references into, and the value variable that IS one of those references.
    /// A reference can escape through a callee, so the alias is permanent for the rest of the
    /// body.
    pub ref_aliased_locals: HashSet<String>,
    /// Reference-aliased locals whose alias did not arise solely from a registry builtin call.
    ///
    /// Registry builtins can mutate by-reference arguments but do not retain their cells. Direct
    /// PHP aliases, captures, by-reference parameters, foreach references, and user callees can
    /// keep another path to the storage, so representation-changing writes must distinguish this
    /// subset from the broader kill/retype conservatism in `ref_aliased_locals`.
    pub non_builtin_ref_aliased_locals: HashSet<String>,
    /// Locals joined by an explicit `=&` assignment in the current body.
    ///
    /// Unlike captures and by-reference parameters, these can retain native packed-array
    /// storage through multiple local names, so a write must not widen only one view.
    pub ref_assign_aliased_locals: HashSet<String>,
    /// Earlier by-reference captures whose native element layout a later push must preserve.
    pub array_push_capture_sites: HashMap<String, HashSet<Span>>,
    /// The subset of [`Checker::ref_aliased_locals`] whose HOME stops being an ordinary frame
    /// slot: EIR lowering binds these names to a shared reference cell
    /// (`LoweringContext::mark_ref_bound_local`), so a store through the name writes the cell at
    /// the CELL's representation and `store_local` deliberately refuses to widen the slot.
    ///
    /// That is the difference the widening eligibility turns on, and the full aliased set cannot
    /// express it. A `=&` target or source, a `use (&$x)` capture, and both names a
    /// `foreach ($arr as &$v)` touches all keep a lasting alias — the cell outlives the statement
    /// that made it — while a variable merely handed to a by-reference PARAMETER lends its slot
    /// address for the duration of one call and keeps its own frame storage afterwards
    /// (`ir_lower::expr::ref_place_args` already adapts a boxed caller slot to a declared
    /// parameter representation and writes the result back). `sort($c); … $c = 'x';` is that
    /// shape, and it is `array_shift`, `array_pop`, `array_splice`, `preg_match_all`'s out
    /// parameter and `uksort` in the stock fixture.
    ///
    /// The KILL keeps consulting the full set: abandoning a slot strands a reference that
    /// escaped through the callee, which a lend cannot be proven not to do.
    pub ref_bound_locals: HashSet<String>,
    /// Names ref-bound by a `use (&$x)` capture whose closure body WRITES them.
    ///
    /// A subset of [`Checker::ref_bound_locals`], tracked separately because lowering treats it
    /// differently: `expr::closures` sets such a capture's cell to `Mixed` on the spot, since a
    /// PHP reference has no type and the cell must hold whatever the closure stores through it.
    /// The slot is therefore already boxed, and a straight-line retype of the name has nothing
    /// left to break — which is why [`Checker::local_binding_is_widenable`] admits it.
    ///
    /// Names this does NOT contain: the recursive self-binding `$f = function () use (&$f) {…}`,
    /// whose capture lowering gives the backing cell's own representation instead of `Mixed`, and
    /// any name a `=&` or a by-reference `foreach` also binds — those sites remove it.
    pub by_ref_capture_boxed_locals: HashSet<String>,
    /// Current local bindings that alias a managed container entry whose physical cell payload
    /// is canonical boxed `Mixed`, even when the checker's logical value type is narrower.
    pub boxed_ref_aliased_locals: HashSet<String>,
    /// Per-conditional-group bindings whose entry provenance was invalidated on any path.
    pub conditional_boxed_ref_invalidations: Vec<HashSet<String>>,
    /// Names declared `static` in the current body. Their storage outlives the call, so the
    /// binding is never killable.
    pub static_local_names: HashSet<String>,
    /// Every parameter name of the current body (empty at top level). The storage-only numeric
    /// re-bind skips these: backtrace frames read a parameter's argument value from the FIRST
    /// slot of that name, so splitting a parameter's binding would report a stale argument.
    pub local_parameter_names: HashSet<String>,
    /// Untyped formal bindings that still load through the callee's boxed incoming storage.
    pub boxed_parameter_locals: HashSet<String>,
    /// Disjoint binding starts for names dead between syntactically closed regions.
    pub numeric_region_sites: HashMap<Span, HashSet<String>>,
    /// Retype decisions justified only by region analysis, excluding independent binding rules.
    pub numeric_region_retypes: HashSet<(Span, String)>,
    /// Assignment dependencies whose numeric region decisions must be invalidated together.
    pub numeric_region_dependents: HashMap<String, HashSet<String>>,
    /// Names whose type was DECLARED in the current body by a `TypedAssign` local
    /// (`int $x = …`), and what each was declared AS. A declaration is a programmer contract
    /// and stays strict in both permissive and `--strict-locals` mode.
    ///
    /// The declared type is kept, not just the name, because a guard's narrowing is not the
    /// contract: `?Node $c = $head; while ($c !== null) { $c = $c->next; }` reaches the
    /// reassignment with `$c` bound `Node`, and only the declaration knows that `?Node` was
    /// always allowed. See `check_local_reassignment`.
    ///
    /// PARAMETERS are deliberately absent, type hint or not. A PHP parameter type constrains
    /// the INCOMING ARGUMENT; it says nothing about the local afterwards, and php-src lets
    /// `function f(float $v) { $v = (string) $v; }` reassign it freely (issue #857). Listing
    /// typed parameters here made them permanently monomorphic and rejected that valid PHP
    /// with `cannot reassign $v from float to string`.
    pub typed_local_names: HashMap<String, PhpType>,
    /// The `unset()` ARGUMENTS whose local binding the checker killed, as span -> the SET of local
    /// NAMES killed there. EIR lowering consults these to abandon the old frame slot instead of
    /// null-storing into it. The name is half the key: a `Span` has no file identity and include
    /// resolution does not rebase line numbers, so lowering has to confirm the decision is about
    /// the variable in front of it — and two DIFFERENT names at one position are two decisions,
    /// not one. See `CheckResult::local_bind_kill_sites`.
    pub local_bind_kill_sites: HashMap<Span, HashSet<String>>,
    /// Unconditional reference unsets that may retire a promoted ordinary local's binding.
    /// These do not relax escaped-reference type checking or ordinary kill eligibility.
    pub local_ref_detach_sites: HashMap<Span, HashSet<String>>,
    /// Removed detach keys still participate in cross-file ambiguity checks.
    pub retired_ref_detach_sites: HashSet<(Span, String)>,
    /// Statement-form assignments the checker re-bound to a fresh binding of an incompatible
    /// type, as span -> the SET of local NAMES re-bound there. Carried in `CheckResult` from here
    /// so the binding-decision maps travel together, keyed and shaped the same way.
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
    /// Whether any checked body contains eval. Runtime fragments can write missing names on AOT
    /// objects even when no equivalent property mutation is visible in the source AST.
    pub program_contains_eval: bool,
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
    /// Names of the CURRENT body's locals whose frame slot became boxed `Mixed` because an
    /// assignment RETYPED them, as opposed to the syntactic pre-scan's `mixed_storage_locals`.
    ///
    /// The two sets exist for the same reason and are consulted differently. A pre-scan mark makes
    /// `Mixed` the only type the environment may hold for the name, so nothing can narrow it away.
    /// A retype cannot do that: `$e = $this; while ($e = $e->getP())` binds `$e` to `N|null`, whose
    /// `codegen_repr` is the boxed cell the slot really holds, and the environment must keep that
    /// PHP type — it is what reports `$e->noSuchMethod()`. What flow narrowing then does to it is
    /// correct as a PHP type and WRONG as a representation: the loop body narrows `$e` to `N`,
    /// whose repr is a raw object pointer, while the slot still holds a cell.
    ///
    /// Nothing that only reads the value cares. What cares is a write that STORES the value into a
    /// container, because the container's element type is a representation and the lowering reads
    /// the slot: `$out[] = $e` typed the element `N` in the checker and stored a boxed cell,
    /// leaving the callee returning `array<mixed>` while every CALLER was compiled against
    /// `array<N>` (see `Checker::stored_element_type`).
    ///
    /// Per-body, like every other field in [`SavedLocalBindingScope`]: a name boxed in the caller
    /// says nothing about a same-named local in the callee.
    pub retyped_boxed_storage_locals: HashSet<String>,
    /// Union storage contracts retained while guards narrow reads of a local in the current body.
    pub guarded_union_types: HashMap<String, PhpType>,
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
    /// True only when every visit to an indexed-read span used native buffer storage.
    /// Intersecting observations also fails closed when included files share a span.
    pub buffer_read_observations: HashMap<Span, bool>,
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
    /// Whether any class can become a userspace stream filter: the program references
    /// `stream_filter_register` (`program_may_register_user_filter`). Without it, a class method
    /// named `filter`, `onCreate` or `onClose` is an ordinary method, not the runtime's raw-ABI
    /// filter contract; `PhpOption\Some::filter($callable)` kept the contract's `int` seed and
    /// refused `$callable(...)`.
    pub user_filters_reachable: bool,
    /// The classes `stream_filter_register()` names, case-folded; `None` when a call names one at
    /// run time (`builtin_class_gate::user_filter_registered_classes`).
    pub user_filter_registered: Option<HashSet<String>>,
}

/// The per-body local-binding eligibility state, saved while a nested body is checked.
///
/// Every field it carries describes ONE function/method/closure/top-level body: a name that is
/// reference-aliased in the caller says nothing about a same-named local in the callee. See
/// [`Checker::enter_local_binding_scope`].
pub(crate) struct SavedLocalBindingScope {
    conditional_depth: u32,
    binding_depth: HashMap<String, u32>,
    narrowed_origins: HashMap<String, NarrowedLocalOrigin>,
    ref_aliased: HashSet<String>,
    non_builtin_ref_aliased: HashSet<String>,
    ref_assign_aliased: HashSet<String>,
    array_push_capture_sites: HashMap<String, HashSet<Span>>,
    ref_bound: HashSet<String>,
    by_ref_capture_boxed: HashSet<String>,
    boxed_ref_aliased: HashSet<String>,
    boxed_ref_invalidations: Vec<HashSet<String>>,
    statics: HashSet<String>,
    parameters: HashSet<String>,
    boxed_parameters: HashSet<String>,
    typed: HashMap<String, PhpType>,
    mixed_storage: HashSet<String>,
    retyped_boxed_storage: HashSet<String>,
    guarded_unions: HashMap<String, PhpType>,
    contains_eval: bool,
    numeric_regions: HashMap<Span, HashSet<String>>,
    numeric_region_retypes: HashSet<(Span, String)>,
    numeric_region_dependents: HashMap<String, HashSet<String>>,
}

/// What a flow guard did to one local: the type the binding held before it, and the view it
/// published to the environment in place of it.
#[derive(Clone)]
pub struct NarrowedLocalOrigin {
    /// Type the binding held before any guard on this name narrowed it.
    pub origin: PhpType,
    /// The guard's own view, or `None` once the region has stored something of its own.
    ///
    /// A failed assignment merge is re-measured against `origin` only while the environment
    /// still holds a guard view. After a store the entry is a fact the REGION established, the
    /// ordinary merge governs, and the shapes the branch-divergent `Mixed`-storage marking
    /// exists for keep reaching it: `$a = 1; if (is_string($a)) { $a = "x"; $a = 2; }` is one
    /// of those, decided by `mixed_storage_scan` and by `--strict-locals`, not here. Comparing
    /// the view to the environment is not enough to tell the two apart — the store in that
    /// example leaves `string` behind, which is also what the guard published.
    ///
    /// Restored by a COMPLEMENT: `restore_narrowed_var` puts the pre-`if` type back before the
    /// `elseif`/`else` clauses are checked, so what they see is a fresh guard fact whatever the
    /// then-branch stored — the `else` is a path the then-branch's store says nothing about.
    pub view: Option<PhpType>,
    /// Whether ANYTHING inside this entry's region has bound the name, on any path.
    ///
    /// Distinct from `view.is_none()` because a complement re-opens a view and this has to
    /// survive it. It travels OUTWARD at [`Checker::exit_flow_narrowing`]: the enclosing region
    /// CONTAINS the closing one, so a store the inner one made is a store the enclosing one
    /// made. Without that, nesting was a way around the rule —
    /// `$a = 1; if (is_string($a)) { if (is_float($a)) { $a = 1.5; } $a = 2; }` compiled while
    /// the same two stores written flat did not (raised in review on #509).
    pub stored_in_region: bool,
}

/// One entry of [`Checker::narrowed_local_origins`] as it stood before a guard region opened it.
/// Produced by [`Checker::enter_flow_narrowing`] and consumed by
/// [`Checker::exit_flow_narrowing`].
pub(crate) struct SavedNarrowingOrigin {
    name: String,
    previous: Option<NarrowedLocalOrigin>,
}

impl Checker {
    /// Returns whether the call at `span` was typed as a builtin call.
    ///
    /// `builtin_call_types` is keyed by the loop-storage scope as well, so the same span can be
    /// recorded once per closure specialization; whether a call is a builtin does not depend on
    /// which one. The current scope is the common hit.
    pub(crate) fn is_recorded_builtin_call(&self, span: crate::span::Span) -> bool {
        self.builtin_call_types
            .contains_key(&(self.current_loop_storage_scope.clone(), span))
            || self.builtin_call_types.keys().any(|(_, recorded)| *recorded == span)
    }

    /// Returns whether a reflected class name the checker cannot see may still exist at run time.
    ///
    /// [`Checker::program_defers_unknown_classes`] covers a runtime include or an autoloader,
    /// wherever they sit; `eval_barrier_active` covers a literal `eval()` already passed, whose
    /// text can declare the class (`eval('class Signer {...}'); new ReflectionClass('Signer')`
    /// was a compile error where `php -n` runs it).
    pub(crate) fn reflection_defers_unknown_classes(&self) -> bool {
        self.program_defers_unknown_classes || self.eval_barrier_active
    }

    /// Returns whether unresolved class names may be deferred to PHP's runtime `Error` path.
    ///
    /// Function, method, and closure bodies can contain optional-extension code that is never
    /// reached. Top-level fixed construction remains a compile-time diagnostic for compatibility.
    ///
    /// The third clause is the one that covers a class only a RUNTIME INCLUDE or a registered
    /// autoloader can supply — see [`Checker::program_defers_unknown_classes`]. It is program-wide
    /// rather than point-in-time because both of its routes can sit textually below the `new` that
    /// needs them, which is exactly the case `eval_barrier_active` cannot express.
    pub(crate) fn allows_absent_runtime_class(&self) -> bool {
        !self.null_probe_scope_is_top_level
            || self.eval_barrier_active
            || self.program_defers_unknown_classes
    }

    /// True when `name`'s current binding may be killed (by `unset`) or re-bound to an
    /// incompatible type (by assignment) at the current program point. See
    /// `docs/php/types.md`, Local retyping: the depth-0 and non-aliased rule.
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
            // with an explicit depth 0 by `enter_local_binding_scope`, so a parameter stays
            // kill-eligible — a TYPE HINT does not change that, because it constrains the
            // incoming argument rather than the local (`test_typed_param_is_killable`).
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
            && !self.typed_local_names.contains_key(name)
    }

    /// True when `name`'s binding may simply TAKE a type it cannot merge with, leaving its frame
    /// slot in place for `store_local` to widen — the storage path every incompatible reassignment
    /// used before precise re-binds existed.
    ///
    /// PHP has no illegal local reassignment. `function f(int $a) { $a = "s"; … }` — a write to a
    /// TYPE-HINTED parameter — is ordinary code that `php -n` runs, and so is a write to a
    /// `foreach`/`list()`/`catch` name the body never assigned through `check_assign`. Both are
    /// refused by [`Checker::local_binding_is_killable`], which answers the strictly harder
    /// question — may the old frame slot be ABANDONED, so the store mints a fresh one. Widening
    /// does not abandon anything: the slot stays and `store_local` joins its storage type.
    ///
    /// THREE of the kill's conditions are dropped, and only those three:
    ///
    /// - the BINDING's conditional depth, which the kill reads as `Some(0)`. That test answers
    ///   "bound unconditionally by this body" and rejects two different things at once: a binding
    ///   created inside a branch, and a name with NO entry at all. A missing entry is deliberate
    ///   for every name a conditional group INTRODUCES without an assignment — a `foreach` or
    ///   `list()` target, a `catch` variable, a builtin out-parameter — because the kill cannot
    ///   tell it apart from depth ≥ 1 and sweeping them in was measurably expensive. Widening CAN
    ///   tell them apart, and asks the question it actually cares about instead:
    ///   [`Checker::name_is_seeded_program_storage`], the enumerated set of names whose storage is
    ///   not this frame's (superglobals everywhere, `$argc`/`$argv` and extern C globals at top
    ///   level). A `catch` variable is an ordinary frame slot and widens; `$_SERVER` is not and
    ///   does not;
    /// - the typed-local set, which at this point holds type-hinted PARAMETERS (a declared LOCAL
    ///   returns from the `declared_local_types` arm above before reaching here). A parameter's
    ///   type hint is a call-boundary contract, not a storage contract for the rest of the body;
    /// - the CURRENT conditional depth, which the kill needs because a kill must PROVE the store
    ///   runs before it may abandon a slot, and a widening proves nothing: it keeps the slot and
    ///   lets `store_local` widen it, which is sound whether or not the branch is taken.
    ///   `if (\is_string($extensions)) { $extensions = [$extensions]; }` — Symfony's
    ///   `Filesystem\Path::getExtension` — is the shape, and it is the single most common one in
    ///   real code: on the stock fixture the depth condition alone accounted for 208 of the 234
    ///   `cannot reassign` errors.
    ///
    ///   Dropping it was gated on lowering, not on the checker. A store inside a branch widens the
    ///   local's FRAME slot, and until `join_arm_types` learned to join the merge through that
    ///   widened slot, the code below the branch was lowered against the untaken arm's type:
    ///   `$i = \strlen('ab'); if ($argc > 0) { $i = 'si'; } echo $i;` printed `0` (an `int` load of
    ///   a boxed slot holding a string) instead of `si`. That is fixed in the `if` join itself,
    ///   where a switch's edges have always been joined; measured against `php -n`, byte-identical.
    ///
    /// - the LENT half of the reference-aliased set. `ref_aliased_locals` answers "a reference
    ///   reached this name", which is what the kill needs: abandoning a slot strands a reference
    ///   that escaped through a callee. Widening asks a narrower question — is the name's HOME
    ///   still an ordinary frame slot this frame may re-represent? — and the answer differs
    ///   between the two halves of that set. A `=&` alias, a `use (&$x)` capture and a
    ///   `foreach ($arr as &$v)` all leave a cell alive after the statement, and lowering
    ///   ref-binds the name so `store_local` refuses to widen the slot at all; widening in the
    ///   checker alone would degrade to a store through the cell at the CELL's representation,
    ///   which is a miscompile rather than a diagnostic. A variable merely handed to a
    ///   by-reference PARAMETER lends its slot address for one call and keeps its own frame
    ///   storage afterwards, so it widens like any other local — `sort($c); … $c = 'x';` and the
    ///   `array_shift` / `array_pop` / `array_splice` / `preg_match_all` / `uksort` shapes it
    ///   stands for. [`Checker::ref_bound_locals`] is the narrower set this reads;
    /// - the EVAL-body flag, for the reason [`Checker::local_binding_is_killable`]'s own doc
    ///   already gives for exempting the `Mixed`-storage marking. The kill needs it because ending
    ///   a binding DROPS the name's frame slot and mints a fresh one, while the eval scope
    ///   addresses caller locals BY NAME — the fragment then reads or writes a slot the rest of
    ///   the body no longer uses, which is why all three repros there printed nothing. A widening
    ///   drops nothing: it keeps the slot and lets `store_local` box it, which is the
    ///   representation the eval scope wants anyway. `ContainerBuilder::createService` is the
    ///   shape — one `eval()` at its line 1129 made `$factory = fn (…) => …` 36 lines below it a
    ///   hard error. Measured against `php -n` and byte-identical: the widening before the eval
    ///   with and without a refused `unset` between them, the eval before the widening, a fragment
    ///   that WRITES the name on either side of it, and a widening inside a branch.
    ///
    /// Everything else the kill demands is kept, plus the program-global veto the kill applies
    /// separately: a by-reference parameter, a `=&` alias, a `global` write through a cell another
    /// body owns and a `static` whose storage outlives the call all stay hard errors rather than
    /// becoming a silent widening of storage this frame does not own. `--strict-locals` still
    /// rejects every one of these shapes, at every depth.
    pub(crate) fn local_binding_is_widenable(&self, name: &str) -> bool {
        !self.name_is_seeded_program_storage(name)
            && !self.top_level_binding_is_program_global(name)
            && !self.active_ref_params.contains(name)
            && (!self.ref_bound_locals.contains(name)
                || self.by_ref_capture_boxed_locals.contains(name))
            && !self.active_globals.contains(name)
            && !self.static_local_names.contains(name)
    }

    /// Authorizes reference-binding detachment outside conditional flow and name-addressed storage.
    /// The reference may be frame-owned or incoming from a caller/capture. `unset()` detaches the
    /// local name in both cases, while the external cell and its value remain owned by that scope.
    pub(crate) fn local_reference_is_detachable(&self, name: &str) -> bool {
        !self.body_contains_eval
            && self.local_conditional_depth == 0
            && (self.ref_aliased_locals.contains(name)
                || self.active_external_ref_bindings.contains(name))
            && !self.active_globals.contains(name)
            && !self.static_local_names.contains(name)
            && !self.typed_local_names.contains_key(name)
            && !self.name_is_seeded_program_storage(name)
            && !self.top_level_binding_is_program_global(name)
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
    ///
    /// The declared-type exclusion set starts EMPTY rather than seeded with the type-hinted
    /// parameters: a hint constrains the incoming argument, not the local, so a parameter is
    /// as retypable as any other local the body binds (see [`Checker::typed_local_names`]).
    pub(crate) fn enter_local_binding_scope(
        &mut self,
        param_names: Vec<String>,
    ) -> SavedLocalBindingScope {
        let saved = SavedLocalBindingScope {
            conditional_depth: self.local_conditional_depth,
            binding_depth: std::mem::take(&mut self.local_binding_depth),
            // A guard in the ENCLOSING body narrows the enclosing frame's local. A closure
            // checked inside that branch binds its own name in its own frame, so inheriting the
            // origin would measure the closure's store against a binding it does not own.
            narrowed_origins: std::mem::take(&mut self.narrowed_local_origins),
            ref_aliased: std::mem::take(&mut self.ref_aliased_locals),
            non_builtin_ref_aliased: std::mem::take(
                &mut self.non_builtin_ref_aliased_locals,
            ),
            ref_assign_aliased: std::mem::take(&mut self.ref_assign_aliased_locals),
            array_push_capture_sites: std::mem::take(&mut self.array_push_capture_sites),
            ref_bound: std::mem::take(&mut self.ref_bound_locals),
            by_ref_capture_boxed: std::mem::take(&mut self.by_ref_capture_boxed_locals),
            boxed_ref_aliased: std::mem::take(&mut self.boxed_ref_aliased_locals),
            boxed_ref_invalidations: std::mem::take(
                &mut self.conditional_boxed_ref_invalidations,
            ),
            statics: std::mem::take(&mut self.static_local_names),
            parameters: std::mem::take(&mut self.local_parameter_names),
            boxed_parameters: std::mem::take(&mut self.boxed_parameter_locals),
            typed: std::mem::take(&mut self.typed_local_names),
            // The mixed-storage marking describes ONE frame: a name boxed in the caller says
            // nothing about a same-named local in the callee, and leaking the set inward would
            // box a callee local the pre-scan never marked.
            mixed_storage: std::mem::take(&mut self.mixed_storage_locals),
            // Boxed by a RETYPE in the enclosing body, which describes that frame's slot and no
            // other — leaking it inward would type a callee's container element `Mixed` on the
            // strength of a caller local that merely shares the name.
            retyped_boxed_storage: std::mem::take(&mut self.retyped_boxed_storage_locals),
            guarded_unions: std::mem::take(&mut self.guarded_union_types),
            contains_eval: self.body_contains_eval,
            numeric_regions: std::mem::take(&mut self.numeric_region_sites),
            numeric_region_retypes: std::mem::take(&mut self.numeric_region_retypes),
            numeric_region_dependents: std::mem::take(&mut self.numeric_region_dependents),
        };
        self.local_conditional_depth = 0;
        self.local_parameter_names = param_names.iter().cloned().collect();
        self.local_binding_depth = param_names.into_iter().map(|name| (name, 0)).collect();
        self.typed_local_names = HashMap::default();
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
        self.narrowed_local_origins = saved.narrowed_origins;
        self.ref_aliased_locals = saved.ref_aliased;
        self.non_builtin_ref_aliased_locals = saved.non_builtin_ref_aliased;
        self.ref_assign_aliased_locals = saved.ref_assign_aliased;
        self.array_push_capture_sites = saved.array_push_capture_sites;
        self.ref_bound_locals = saved.ref_bound;
        self.by_ref_capture_boxed_locals = saved.by_ref_capture_boxed;
        self.boxed_ref_aliased_locals = saved.boxed_ref_aliased;
        self.conditional_boxed_ref_invalidations = saved.boxed_ref_invalidations;
        self.static_local_names = saved.statics;
        self.local_parameter_names = saved.parameters;
        self.boxed_parameter_locals = saved.boxed_parameters;
        self.typed_local_names = saved.typed;
        self.mixed_storage_locals = saved.mixed_storage;
        self.retyped_boxed_storage_locals = saved.retyped_boxed_storage;
        self.guarded_union_types = saved.guarded_unions;
        self.body_contains_eval = saved.contains_eval;
        self.numeric_region_sites = saved.numeric_regions;
        self.numeric_region_retypes = saved.numeric_region_retypes;
        self.numeric_region_dependents = saved.numeric_region_dependents;
    }

    /// Returns the element type an array element WRITE contributes when it stores `value`.
    ///
    /// `inferred` is what the value's expression means as a PHP type; the answer is what its slot
    /// HOLDS. The two differ for exactly one shape — a plain read of a local whose slot a retype
    /// boxed (`retyped_boxed_storage_locals`) and which flow narrowing has since re-typed to
    /// something with a concrete representation. `$out[] = $e` inside
    /// `$e = $this; while ($e = $e->getP()) { … }` is that shape: `$e` is honestly `N` there, and
    /// the value the store moves is honestly a boxed cell.
    ///
    /// The element type of an array is a REPRESENTATION — it decides the slot width, the header's
    /// value-type stamp and which runtime helper reads the payload — so it has to be answered from
    /// the storage. Answering it from the narrowed type left the checker's array type `array<N>`
    /// while the EIR lowering, which reads the same slot and cannot see the narrowing, ran
    /// `__rt_array_to_mixed` and returned `array<mixed>`. That disagreement does not stay inside
    /// the body: the method's inferred return type is what every CALLER is compiled against, so a
    /// caller read raw object pointers out of an array of cells. `foreach` faulted; `array_pop`,
    /// `array_shift`, `array_map`, `array_filter`, `array_merge`, a by-reference `foreach` and an
    /// append followed by a read all answered SILENTLY wrong (see the `locals_retype` fixtures).
    ///
    /// Only ever widens, and only to `Mixed` — the one representation that can hold anything —
    /// which is why it cannot make a correct program wrong: an array whose elements really are
    /// cells is typed as holding cells.
    pub(crate) fn stored_element_type(&self, value: &Expr, inferred: PhpType) -> PhpType {
        let ExprKind::Variable(name) = &value.kind else {
            return inferred;
        };
        if inferred.codegen_repr() != PhpType::Mixed
            && self.retyped_boxed_storage_locals.contains(name.as_str())
        {
            return PhpType::Mixed;
        }
        inferred
    }

    /// Opens a region in which `name` is viewed as `view` through a flow guard's narrowing,
    /// recording the type its binding held just before (`pre_guard`, the environment entry the
    /// guard is about to overwrite). The returned value must be handed back to
    /// [`Checker::exit_flow_narrowing`] when the region ends.
    ///
    /// NESTED guards keep the OUTERMOST origin, which is the binding's own type: in
    /// `if (is_scalar($x)) { if (is_float($x)) { $x = "s"; } }` the inner guard's `pre_guard` is
    /// already the outer guard's target, and a store inside it has to be measured against the
    /// binding rather than against another guard's view of the value. The VIEW is the innermost
    /// guard's, because that is what the environment holds.
    pub(crate) fn enter_flow_narrowing(
        &mut self,
        name: &str,
        pre_guard: Option<&PhpType>,
        view: &PhpType,
    ) -> SavedNarrowingOrigin {
        let previous = self.narrowed_local_origins.get(name).cloned();
        match previous
            .as_ref()
            .map(|entry| entry.origin.clone())
            .or_else(|| pre_guard.cloned())
        {
            Some(origin) => {
                self.narrowed_local_origins.insert(
                    name.to_string(),
                    NarrowedLocalOrigin {
                        origin,
                        view: Some(view.clone()),
                        // The REGION is new even when the entry inherits an outer origin, so a
                        // store the enclosing region already made does not follow it inward. It
                        // is still recorded on the enclosing entry, which comes back at
                        // `exit_flow_narrowing`.
                        stored_in_region: false,
                    },
                );
            }
            // `guard_narrowing` bails on an unbound plain variable, so this is the property and
            // synthetic-key shapes: nothing to measure a later store against, and recording an
            // absent origin would only make `merge_local_assignment_type` look one up in vain.
            None => {
                self.narrowed_local_origins.remove(name);
            }
        }
        SavedNarrowingOrigin { name: name.to_string(), previous }
    }

    /// Replaces the published view of an OPEN narrowing region, keeping its origin.
    ///
    /// An `if`/`elseif` chain narrows twice per guarded clause: to the guard's target for its own
    /// body, then to the COMPLEMENT for every clause after it and the final `else`. Both are the
    /// guard's view of the value rather than anything the region stored, so
    /// `if ($h !== false) { … } else { $h = []; }` — the same issue-#509 idiom written the other
    /// way round — has to be recognised on the complement side too.
    ///
    /// `stored_in_region` deliberately SURVIVES this. The complement is a fresh fact about the
    /// `else` path, but a store the then-branch made is still a store the construct made, and
    /// that is what the enclosing region has to hear about. It also fires for a single-clause
    /// `if` with nothing after it, where the "rest of the chain" is empty — which is how
    /// clearing only `view` let a nested store's effect evaporate before `exit_flow_narrowing`
    /// could see it.
    pub(crate) fn republish_flow_narrowing(&mut self, name: &str, view: &PhpType) {
        if let Some(entry) = self.narrowed_local_origins.get_mut(name) {
            entry.view = Some(view.clone());
        }
    }

    /// Records that a guarded region has bound `name` itself, which ends the guard view: from
    /// here on the environment entry is the region's own fact and an assignment measures itself
    /// against it in the ordinary way.
    ///
    /// A no-op for a name no guard has narrowed, which is every name most of the time.
    pub(crate) fn record_store_over_flow_narrowing(&mut self, name: &str) {
        if let Some(entry) = self.narrowed_local_origins.get_mut(name) {
            entry.view = None;
            entry.stored_in_region = true;
        }
    }

    /// Closes a region opened by [`Checker::enter_flow_narrowing`]. Regions opened in order must
    /// be closed in REVERSE order, the way the environment's own narrowings are restored.
    ///
    /// A store inside the closing region travels OUTWARD: the enclosing entry comes back with
    /// its view cleared. See [`NarrowedLocalOrigin::stored_in_region`] for why nesting would
    /// otherwise be a way around the rule.
    pub(crate) fn exit_flow_narrowing(&mut self, saved: SavedNarrowingOrigin) {
        let region_stored = self
            .narrowed_local_origins
            .get(&saved.name)
            .is_some_and(|entry| entry.stored_in_region);
        match saved.previous {
            Some(mut entry) => {
                if region_stored {
                    entry.view = None;
                    entry.stored_in_region = true;
                }
                self.narrowed_local_origins.insert(saved.name, entry);
            }
            None => {
                self.narrowed_local_origins.remove(&saved.name);
            }
        }
    }

    /// Drops every per-name fact the checker carries for a local whose binding just ended.
    ///
    /// The callable/reflection tables are keyed by variable name and outlive the type
    /// environment, so a killed (or re-bound) name must not keep answering with the signature,
    /// captures, or reflected class of the binding that is gone — that is how a stale
    /// `$f()` signature would survive an `unset($f)`.
    pub(crate) fn clear_local_binding_metadata(&mut self, name: &str) {
        self.boxed_parameter_locals.remove(name);
        self.array_push_capture_sites.remove(name);
        // A kill ABANDONS the slot, so whatever representation a retype had given it ends with the
        // binding. Left standing, the mark would keep typing container writes from the name
        // `Mixed` after `unset($e); $e = 5;` had put a raw integer back in a fresh slot.
        self.retyped_boxed_storage_locals.remove(name);
        self.invalidate_boxed_ref_alias_binding(name);
        self.closure_return_types.remove(name);
        self.callable_sigs.remove(name);
        self.callable_captures.remove(name);
        self.callable_array_targets.remove(name);
        self.mark_callable_array_target_write(name);
        self.first_class_callable_targets.remove(name);
        self.reflection_class_targets.remove(name);
        self.foreach_key_locals.remove(name);
    }

    /// Ends the current binding's managed-cell provenance and records conditional uncertainty.
    pub(crate) fn invalidate_boxed_ref_alias_binding(&mut self, name: &str) {
        self.boxed_ref_aliased_locals.remove(name);
        for invalidations in &mut self.conditional_boxed_ref_invalidations {
            invalidations.insert(name.to_string());
        }
    }

    /// Records that one local's callable-array target fact was assigned or invalidated.
    pub(crate) fn mark_callable_array_target_write(&mut self, name: &str) {
        self.next_callable_array_target_version =
            self.next_callable_array_target_version.wrapping_add(1);
        self.callable_array_target_versions.insert(
            name.to_string(),
            self.next_callable_array_target_version,
        );
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
    /// Returns whether `property`, reached through `class_name`, is a slot of the compact
    /// Throwable payload that the builtin `Exception`/`Error` constructor and accessors read and
    /// write directly as a plain value.
    ///
    /// Such a slot can never become a reference cell: promotion is hierarchy-wide, so one
    /// `new Holder($this->message)` in any subclass made every exception's `message` a cell the
    /// builtin constructor then overwrote with a bare string, and the next write through the
    /// "cell" released a string pointer (Twig's `Error::updateRepr` died on a bad refcount).
    pub(crate) fn is_native_throwable_payload_property(
        &self,
        class_name: &str,
        property: &str,
    ) -> bool {
        let Some(declaring) = self
            .classes
            .get(class_name.trim_start_matches('\\'))
            .and_then(|info| info.property_declaring_classes.get(property))
        else {
            return false;
        };
        matches!(declaring.as_str(), "Exception" | "Error")
            && self.classes.get(declaring).is_some_and(|info| info.is_internal)
    }

    pub(crate) fn record_reference_alias_root(&mut self, expr: &Expr) {
        self.record_reference_alias_root_with_origin(expr, true);
    }

    /// Records a registry builtin's by-reference argument without treating it as an escaped cell.
    pub(crate) fn record_builtin_reference_alias_root(&mut self, expr: &Expr) {
        self.record_reference_alias_root_with_origin(expr, false);
    }

    /// Returns whether a local at an indexed receiver's root has a non-builtin reference alias.
    pub(crate) fn local_has_non_builtin_reference_alias(&self, expr: &Expr) -> bool {
        self.local_root_is_in_set(expr, &self.non_builtin_ref_aliased_locals)
    }

    /// Returns whether a local at an indexed receiver's root belongs to an explicit `=&` group.
    pub(crate) fn local_has_ref_assign_alias(&self, expr: &Expr) -> bool {
        self.local_root_is_in_set(expr, &self.ref_assign_aliased_locals)
    }

    /// Returns whether an earlier closure in this body can still read the receiver's native layout.
    pub(crate) fn local_has_prior_array_capture(&self, expr: &Expr) -> bool {
        let mut current = expr;
        while let ExprKind::ArrayAccess { array, .. }
        | ExprKind::NamedArg { value: array, .. }
        | ExprKind::ErrorSuppress(array) = &current.kind { current = array; }
        matches!(&current.kind, ExprKind::Variable(name) if self.array_push_capture_sites.contains_key(name))
    }

    /// Returns whether the local at an expression's root belongs to `names`.
    fn local_root_is_in_set(&self, expr: &Expr, names: &HashSet<String>) -> bool {
        let mut current = expr;
        while let ExprKind::ArrayAccess { array: value, .. }
        | ExprKind::NamedArg { value, .. }
        | ExprKind::ErrorSuppress(value) = &current.kind
        {
            current = value;
        }
        matches!(&current.kind, ExprKind::Variable(name)
            if names.contains(name))
    }

    /// Records the local root of an explicit `=&` source.
    pub(crate) fn record_ref_assign_alias_root(&mut self, expr: &Expr) {
        let mut current = expr;
        while let ExprKind::ArrayAccess { array: value, .. }
        | ExprKind::PropertyAccess { object: value, .. }
        | ExprKind::NullsafePropertyAccess { object: value, .. }
        | ExprKind::DynamicPropertyAccess { object: value, .. }
        | ExprKind::NullsafeDynamicPropertyAccess { object: value, .. }
        | ExprKind::ErrorSuppress(value)
        | ExprKind::NamedArg { value, .. } = &current.kind
        {
            current = value;
        }
        if let ExprKind::Variable(name) = &current.kind {
            self.ref_assign_aliased_locals.insert(name.clone());
        }
    }

    /// Records the local root of a reference-producing expression and its origin category.
    fn record_reference_alias_root_with_origin(&mut self, expr: &Expr, non_builtin: bool) {
        if let Some(name) = Self::reference_alias_root_name(expr) {
            self.ref_aliased_locals.insert(name.clone());
            if non_builtin {
                self.non_builtin_ref_aliased_locals.insert(name);
            }
        }
    }

    /// Records a reference alias whose CELL outlives the statement that made it, so lowering
    /// ref-binds the name for the rest of the body.
    ///
    /// Aliases the whole of [`Checker::record_reference_alias_root`] and additionally files the
    /// root in [`Checker::ref_bound_locals`]. The three shapes that need it are the ones whose
    /// cell is reachable after the expression finishes: a `=&` target and its source, a
    /// `use (&$x)` capture, and both names a `foreach ($arr as &$v)` touches. A by-reference
    /// call ARGUMENT is deliberately not one of them -- see `ref_bound_locals`.
    pub(crate) fn record_ref_bound_alias_root(&mut self, expr: &Expr) {
        if let Some(name) = Self::reference_alias_root_name(expr) {
            self.ref_aliased_locals.insert(name.clone());
            self.non_builtin_ref_aliased_locals.insert(name.clone());
            self.ref_bound_locals.insert(name);
        }
    }

    /// Returns the local at the root of a reference's access chain, if the chain reaches one.
    fn reference_alias_root_name(expr: &Expr) -> Option<String> {
        let mut current = expr;
        loop {
            match &current.kind {
                ExprKind::Variable(name) => {
                    return Some(name.clone());
                }
                ExprKind::ArrayAccess { array: base, .. }
                | ExprKind::PropertyAccess { object: base, .. }
                | ExprKind::NullsafePropertyAccess { object: base, .. }
                | ExprKind::DynamicPropertyAccess { object: base, .. }
                | ExprKind::NullsafeDynamicPropertyAccess { object: base, .. }
                | ExprKind::Spread(base)
                | ExprKind::ErrorSuppress(base)
                | ExprKind::NamedArg { value: base, .. } => current = base,
                _ => return None,
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
            let mut value = arg;
            while let ExprKind::NamedArg { value: inner, .. }
                | ExprKind::ErrorSuppress(inner) = &value.kind
            {
                value = inner;
            }
            let ExprKind::Variable(name) = &value.kind else {
                continue;
            };
            // The call-site promotion below changes this local to a rebindable Mixed cell.
            // Keep the checker environment aligned with that storage after the call so a
            // later assignment writes through the cell instead of being rejected as a change
            // from the pre-call concrete type. `unset()` clears this provenance normally.
            self.boxed_ref_aliased_locals.insert(name.clone());
            if value.span.identifies_a_node() {
                self.boxed_reference_promotion_sites
                    .entry((self.current_loop_storage_scope.clone(), value.span))
                    .or_default()
                    .insert(name.clone());
            }
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
    /// Enable elephc's PHP-divergent lints: unused variables, unreachable code, and the
    /// permissive local-retype notice.
    ///
    /// php compiles every one of those silently, so the default build stays quiet: a framework
    /// with ordinary dynamic locals (`$x = 0; $x = 'name';`) and unused assignments produced 303
    /// of 303 warnings from these three families. Linting is a diagnostic mode, not a
    /// compatibility mode, so callers opt in (`--lint`; the diagnostic test helpers do too).
    pub lint: bool,
    /// Whether the program registered an autoloader that `autoload::Registry::build` CONSUMED.
    ///
    /// THE CHECKER CANNOT SEE THIS FOR ITSELF, and that is the whole reason it is passed in.
    /// `Registry::build` returns "the program with consumed register sites stripped", so by the
    /// time the type checker walks the AST a successfully collected
    /// `spl_autoload_register(function (string $c) { … })` IS NO LONGER THERE. A `usage` scan of
    /// the checker's program therefore answers `false` for it — measured with a probe printing the
    /// scan's own fields: a program whose only autoloader argument was a closure, or a named
    /// function that actually exists, reported `spl_autoload_register=false introspects=false`,
    /// while the one case the collector had to REJECT (a string naming a function that does not
    /// exist) reported `true`. Reading the registry instead of re-scanning is the fix, and it is
    /// the same lesson as `required_runtime_features.eval_bridge` in echelon 44: ask the pass that
    /// owns the answer.
    pub registers_autoloader: bool,
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

/// Records an access violation that must lower to a catchable exception throw.
///
/// Refuses a span that identifies no single node. `throw_access_sites` is read back by lowering
/// as `get(&(scope, span))` for the node being lowered, so an entry filed under a span many nodes
/// of the same scope share turns ALL of them into throws — a `$this->prop = ...` in an unrelated
/// class would start raising "Cannot modify readonly property".
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
    sites: &mut HashMap<(String, Span), ThrowAccessInfo>,
    scope: String,
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
    sites.insert((scope, span), info);
}

#[derive(Clone)]
/// FnDecl stores a user-defined function's declaration metadata: parameter names,
/// types, defaults, variadic marker, return type, span, body statements, and
/// attributes (currently only `#[\Deprecated]` is consulted).
pub(crate) struct FnDecl {
    /// Declared type parameters (`function f<T>(...)`), empty for an ordinary function.
    ///
    /// A non-empty list makes this declaration a TEMPLATE: it is deliberately absent from
    /// `Checker::functions`, because its parameter types mention names that have no
    /// representation until a call site supplies type arguments. Each call site instantiates
    /// it under a synthetic monomorphic name instead.
    pub type_params: Vec<crate::parser::ast::TypeParam>,
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
    check_types_with_options(
        program,
        target,
        CheckOptions::default(),
        &crate::generics::GenericContext::default(),
    )
}

/// Runs the type checker on `program` for the given `target` and `options`,
/// returning a `CheckResult` on success or a `CompileError` on failure. The checker
/// validates types, resolves declarations, infers return types, and collects warnings.
/// Abstract return types are propagated from concrete implementations before returning.
pub fn check_types_with_options(
    program: &Program,
    target: Target,
    options: CheckOptions,
    generics: &crate::generics::GenericContext,
) -> Result<CheckResult, CompileError> {
    // Every `check*` spelling funnels through here, so this is the one place the checker needs
    // the stack its own recursion depth implies (issue #686).
    crate::compiler_stack::with_compiler_stack(|| {
        check_types_on_compiler_stack(program, target, options, generics)
    })
}

/// The checker body, running on the stack [`check_types_with_options`] established.
fn check_types_on_compiler_stack(
    program: &Program,
    target: Target,
    options: CheckOptions,
    generics: &crate::generics::GenericContext,
) -> Result<CheckResult, CompileError> {
    let (mut checker, global_env) = driver::check_types_impl(program, target, options, generics)?;

    // Generic class instantiation happened before this checker ran, and on purpose: its type
    // arguments are written, not inferred. What it could NOT do is decide whether `User`
    // satisfies `T : Entity` — that is a subtyping question, and the class table that answers it
    // exists only here. `check_types_impl` already answered every obligation it could before
    // the bodies; this answers the ones it deferred, against the final interface lists.
    checker.verify_class_type_argument_bounds(&generics.class_type_argument_bounds)?;

    attach_class_doc_comments(program, &mut checker.classes);

    propagate_abstract_return_types(&mut checker);
    apply_reference_property_promotions(&mut checker);
    validate_reference_property_bindings(&checker)?;
    clone_override_storage::widen_clone_override_property_storage(&mut checker);
    clone_override_storage::widen_property_unset_storage(&mut checker);
    scope_dynamic_storage::reserve_scope_dynamic_property_storage(&mut checker);
    validate_magic_method_contracts(&checker)?;
    core_attributes::validate(&checker)?;

    let mut warnings = if options.lint {
        crate::types::warnings::collect_warnings(program)
    } else {
        Vec::new()
    };
    warnings.extend(checker.warnings);
    // Binding and foreach warnings are appended once the last walk has settled which
    // diagnostics survive. Sorted by position (then message) because they
    // come out of a hash map — the order has to be the same on every run.
    let mut decision_warnings: Vec<crate::errors::CompileWarning> =
        std::mem::take(&mut checker.binding_decision_warnings)
            .into_values()
            .chain(
                checker.foreach_warnings.into_values().map(|(_, warning)| warning),
            )
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
    let return_alias_summaries = crate::types::collect_return_alias_summaries(program);
    dedupe_warnings(&mut warnings);

    // A decision key that names more than one syntactic site would let EIR lowering re-bind a
    // local the checker never approved — silently, and with the wrong answer. Reject it here so
    // the checker and lowering agree on exactly which programs are accepted.
    binding_decision_ambiguity::reject_ambiguous_local_binding_decisions(
        program,
        &checker.local_bind_kill_sites,
        &checker.local_ref_detach_sites,
        &checker.retired_ref_detach_sites,
        &checker.local_retype_sites,
        &checker.mixed_storage_store_sites,
        &checker.retired_mixed_storage_store_sites,
    )?;

    Ok(CheckResult {
        requested_instantiations: checker.requested_instantiations,
        generic_call_sites: checker.generic_call_sites,
        requested_class_instantiations: checker.requested_class_instantiations,
        requested_method_instantiations: checker.requested_method_instantiations,
        generic_method_sites: checker.generic_method_sites,
        generic_new_sites: checker.generic_new_sites,
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
        closure_generator_spans: checker.closure_generator_spans,
        array_push_property_plans: checker.array_push_property_plans,
        member_receiver_types: checker.member_receiver_types,
        flow_typed_property_accesses: checker.flow_typed_property_accesses,
        boxed_reference_promotion_sites: checker.boxed_reference_promotion_sites,
        buffer_read_sites: checker.buffer_read_observations.into_iter()
            .filter_map(|(span, buffer)| buffer.then_some(span)).collect(),
        first_class_builtin_call_types: checker.first_class_builtin_call_types,
        loop_storage_types: checker.loop_storage_types,
        string_incdec_locals: checker.string_incdec_locals,
        by_ref_local_storage_types: checker.by_ref_local_storage_types,
        ref_param_escaping_callees: checker.ref_param_escaping_callees,
        property_bound_promotion_sites: checker.property_bound_promotion_sites,
        by_ref_call_result_sites: checker.by_ref_call_result_sites,
        bcmath_operator_sites: checker.bcmath_operator_sites,
        stdclass_by_ref_walk_sites: checker.stdclass_by_ref_walk_sites,
        runtime_define_names: checker.runtime_define_names,
        undefined_constant_reads: checker.undefined_constant_reads,
        dynamic_ref_local_types: checker.dynamic_ref_local_types,
        local_bind_kill_sites: checker.local_bind_kill_sites,
        local_ref_detach_sites: checker.local_ref_detach_sites,
        local_retype_sites: checker.local_retype_sites,
        mixed_storage_store_sites: checker.mixed_storage_store_sites,
        user_filters_reachable: checker.user_filters_reachable,
        user_filter_registered: checker.user_filter_registered,
    })
}

/// Copies direct class declaration doc comments into the completed AOT class schema map.
fn attach_class_doc_comments(program: &Program, classes: &mut crate::fast_hash::FastMap<String, ClassInfo>) {
    for stmt in program {
        match &stmt.kind {
            StmtKind::ClassDecl {
                name, doc_comment, ..
            } => {
                let canonical_name = classes
                    .keys()
                    .find(|class_name| {
                        class_name
                            .trim_start_matches('\\')
                            .eq_ignore_ascii_case(name.trim_start_matches('\\'))
                    })
                    .cloned();
                if let Some(class) = canonical_name.and_then(|name| classes.get_mut(&name)) {
                    class.doc_comment = doc_comment.clone();
                }
            }
            StmtKind::NamespaceBlock { body, .. } => attach_class_doc_comments(body, classes),
            _ => {}
        }
    }
}

/// Removes duplicate warnings emitted by repeated checker stabilization passes.
///
/// The first occurrence is preserved so diagnostic order stays stable; duplicates are keyed by
/// source span and message, matching the user-visible warning identity.
fn dedupe_warnings(warnings: &mut Vec<crate::errors::CompileWarning>) {
    let mut seen = HashSet::default();
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

/// Refuses a `$a->p =& $b->q` whose two declared slots store different payloads in the shared cell.
///
/// The cell is one block of storage that both properties read and write with THEIR declared
/// representation: a `?string` slot keeps a boxed value where a `string` slot keeps a pointer and
/// a length, so binding one to the other silently turned every later read and write of either
/// into garbage (an empty string for the narrower slot, a corrupted box for the wider one). php
/// keeps a typed-reference constraint set instead; this compiler has one representation per cell,
/// so a disagreement is a diagnostic, not a miscompile.
fn validate_reference_property_bindings(checker: &Checker) -> Result<(), CompileError> {
    for (target_class, target, source_class, source, span) in &checker.reference_property_bindings
    {
        let payload = |class: &str, property: &str| {
            checker.classes.get(class).and_then(|info| {
                info.properties
                    .iter()
                    .find(|(name, _)| name == property)
                    .map(|(_, ty)| ty.clone())
            })
        };
        let (Some(target_ty), Some(source_ty)) =
            (payload(target_class, target), payload(source_class, source))
        else {
            continue;
        };
        if !target_ty.reference_payload_compatible(&source_ty) {
            return Err(CompileError::new(
                *span,
                &format!(
                    "Cannot bind {}::${} ({}) by reference to {}::${} ({}): the two properties \
                     would share one cell but store different representations; declare both with \
                     the same type",
                    target_class, target, target_ty, source_class, source, source_ty
                ),
            ));
        }
    }
    Ok(())
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
///
/// The promotion follows the SLOT, not the name: a private property keeps its own slot in a
/// descendant that declares a same-named property, and the declaring class's methods reach
/// that hidden slot through the cell, so it is promoted per slot there too.
fn apply_reference_property_promotions(checker: &mut Checker) {
    let promotions = std::mem::take(&mut checker.reference_property_promotions);
    for (access_class, prop) in promotions {
        let declaring = checker
            .classes
            .get(&access_class)
            .and_then(|info| info.property_declaring_classes.get(&prop).cloned())
            .unwrap_or_else(|| access_class.clone());
        if checker.is_native_throwable_payload_property(&access_class, &prop) {
            continue;
        }
        for (class_name, info) in checker.classes.iter_mut() {
            for slot in 0..info.properties.len() {
                if info.properties[slot].0 != prop {
                    continue;
                }
                if info.property_slot_owner(slot, class_name).1 != declaring {
                    continue;
                }
                if info.property_slot_is_reference(slot, &prop) {
                    continue;
                }
                if let Some(slot_reference) = info.property_reference_slots.get_mut(slot) {
                    *slot_reference = true;
                }
                let slot_count = info.properties.len();
                info.property_owned_reference_slots.resize(slot_count, false);
                info.property_owned_reference_slots[slot] = true;
                if !info.property_slot_is_hidden(slot) {
                    info.reference_properties.insert(prop.clone());
                    info.owned_reference_properties.insert(prop.clone());
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
    use crate::fast_hash::FastMap as HashMap;

    /// Builds a readonly write violation for throw-site identity regression tests.
    fn readonly_violation(span: Span) -> ThrowAccessInfo {
        ThrowAccessInfo {
            span,
            kind: ThrowAccessKind::ReadonlyProperty {
                class_name: "PDOStatement".to_string(),
                property: "queryString".to_string(),
                initialization_error: None,
            },
        }
    }

    /// A violation the checker CAN locate is recorded, keyed by where it happened.
    #[test]
    fn a_located_violation_is_recorded() {
        let mut sites = HashMap::default();
        let span = Span::new(7, 3);
        record_throw_access_site(&mut sites, String::new(), span, readonly_violation(span));
        assert_eq!(sites.len(), 1);
        assert!(sites.contains_key(&(String::new(), span)));
    }

    /// A violation the checker CANNOT locate never becomes a key.
    ///
    /// Debug builds abort on it instead — a compiler-generated node that violates readonly is a
    /// compiler bug — so this covers the release behaviour, which is to degrade to silence
    /// rather than attach the throw to every other node carrying the same dummy span.
    #[cfg(not(debug_assertions))]
    #[test]
    fn an_unlocatable_violation_never_becomes_a_key() {
        let mut sites = HashMap::default();
        record_throw_access_site(
            &mut sites,
            String::new(),
            Span::dummy(),
            readonly_violation(Span::dummy()),
        );
        assert!(sites.is_empty());
    }
}

/// The type PHP's `$matches` destination receives, for `preg_match` and `preg_match_all`.
///
/// ONE authority for the shape, read by the checker (to type the destination in the environment)
/// and by EIR lowering (to WIDEN the destination local's frame storage). Deciding it twice is how
/// an indexed destination ends up holding a hash, which reads back renumbered.
pub(crate) fn regex_matches_destination_type(
    builtin_name: &str,
    args: &[crate::parser::ast::Expr],
) -> PhpType {
    // `preg_match_all` fills its outer container with BOXED cells -- one per column in pattern
    // order, one per row in set order -- so the element type is Mixed whatever the pattern looks
    // like. Typing the no-flags form as `array<array<string>>` described a nesting the runtime
    // never builds.
    let capture_type = PhpType::Mixed;
    let assoc = PhpType::AssocArray {
        key: Box::new(PhpType::Mixed),
        value: Box::new(PhpType::Mixed),
    };
    if !crate::types::checker::inference::expr::effects::pattern_declares_named_capture_group(
        args.first(),
    ) {
        return PhpType::Array(Box::new(capture_type));
    }
    if builtin_name.eq_ignore_ascii_case("preg_match") {
        return assoc;
    }
    match crate::types::checker::inference::expr::effects::preg_match_all_uses_pattern_order(
        args.get(3),
    ) {
        Some(true) => assoc,
        Some(false) => PhpType::Array(Box::new(capture_type)),
        // Either storage can arrive, and stamping a hash into an indexed destination reads it
        // back as a renumbered list, so stay gradual and let `store_matches_array` classify it.
        None => PhpType::Mixed,
    }
}
