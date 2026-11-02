//! Purpose:
//! Collects the SYMBOL references and dynamic-dispatch hazards of an AST — the functions,
//! classes and methods it can reach, the string literals it carries, and whether it does
//! anything the walk cannot see through.
//!
//! Called from:
//! - `crate::prelude_prune::prune`, which turns this into a kept/dropped decision.
//! - `crate::web_prelude::inject_if_web()` for its pay-for-use declaration selection.
//!
//! Key details:
//! - Literal `function_exists`, `is_callable`, `class_exists` and callback names count as
//!   references. That matters more than it looks: a guard whose subject gets pruned takes its
//!   ELSE branch silently, which is the one failure mode with no diagnostic.
//! - A METHOD name is recorded without a receiver type, because this walk runs before name
//!   resolution. The pruner therefore roots every class carrying that method — an
//!   over-approximation by construction, never an under-approximation.
//! - Every string literal is recorded. It is the fallback root set for a program that dispatches
//!   dynamically: instead of surrendering and keeping the whole surface, the pruner keeps the
//!   symbols the program actually names.
//!
//! THE CONTRACT THIS MODULE OWES ITS CALLER. A reachability pass is only as safe as the set of
//! channels its walk knows about, so those channels are enumerated here rather than discovered
//! one incident at a time:
//!
//! - NORMALIZATION. Every recorded name is `php_symbol_key`-folded with any leading `\` stripped,
//!   because PHP symbol names are case-insensitive and the pruner's index is keyed the same way.
//!   A raw name would silently fail to match.
//! - THE PROBE SET (`FUNCTION_PROBES`, `CLASS_PROBES`) — builtins that NAME a symbol and answer
//!   quietly when it is absent. Their subject is a reference; a probe on a name this walk cannot
//!   read is introspection, because its wrong answer is silent. `CLASS_PROBES` carries WHICH
//!   ARGUMENT names the class, because it is not always the first: `is_a($obj, 'Imagick')` names
//!   its class second, and reading position zero there is how the one name that had to survive
//!   gets recorded as a method instead.
//! - CALLBACK BOUNDARIES (`callable_taking_builtin`): builtins that INVOKE or retain a callable. A literal
//!   there is a plain reference and does not depend on harvest mode:
//!   `register_shutdown_function('session_write_close')` contains no dynamic call at all.
//! - THE ENUMERATION SET (`INTROSPECTION_BUILTINS`) — builtins that hand over the whole symbol
//!   table and name nothing.
//! - IMPORTS. `use function imagecreate as ic;` names `imagecreate`, and this walk runs before
//!   name resolution, so nothing later will connect `ic()` back to it.
//!
//! Adding a prelude to `crate::prelude_prune` means checking this list against how that prelude's
//! declarations can be reached — including by a compiler pass that SYNTHESISES a call to one.

use std::collections::{HashMap, HashSet};

use crate::names::php_symbol_key;
use crate::parser::ast::{
    CallableTarget, ClassConst, ClassMethod, ClassProperty, Expr, ExprKind, InstanceOfTarget,
    Stmt, StmtKind,
};

/// Symbol-reference summary for one AST subtree.
#[derive(Clone, Debug, Default)]
pub(crate) struct Usage {
    /// Free-function names, normalized case-insensitively.
    pub(crate) functions: HashSet<String>,
    /// Class names, normalized case-insensitively: `new C`, `C::`, `instanceof C`, a type hint,
    /// a `catch` type, an `extends`/`implements`, or a literal `class_exists('C')`.
    pub(crate) classes: HashSet<String>,
    /// Method names seen on a receiver whose class this walk cannot know.
    pub(crate) methods: HashSet<String>,
    /// Global constant names, case-sensitive, normalized without a leading separator.
    pub(crate) constants: HashSet<String>,
    /// Every string literal in the subtree, normalized. The fallback root set.
    pub(crate) literals: HashSet<String>,
    /// PHP variable names written exactly as they appear (`$_SERVER` → `_SERVER`),
    /// case-sensitively, since PHP variables are. Only names the source SPELLS land
    /// here: a `$$computed` access does not, which is what keeps a consumer honest
    /// about being a pay-for-use gate rather than a completeness claim.
    pub(crate) variables: HashSet<String>,
    /// Variable names held directly by statement storage slots or closure captures,
    /// rather than by a `Variable` expression. Kept separate from read-based CLI seeding.
    pub(crate) storage_variables: HashSet<String>,
    /// Conservative binding facts for calls whose spelling can become visible only after folding.
    pub(crate) callable_names: super::callable_names::CallableNames,
    /// A CALL whose target this walk cannot name (`$f()`, `call_user_func($x)`, a closure
    /// call). Its failure mode is LOUD — an undefined function at the call site — so the pruner
    /// answers it by widening the roots to the names the program does mention.
    pub(crate) dynamic_function_call: bool,
    /// The program contains a `yield` or `yield from`, which makes its enclosing function a
    /// generator and materializes a `Generator` object no source line names.
    pub(crate) uses_yield: bool,
    /// A `new $c` whose class name this walk cannot resolve to a literal.
    ///
    /// `codegen_support::dynamic_new::supported_dynamic_new_builtin_class_names` lists the
    /// builtin classes such a site can construct, and it includes every builtin throwable — so a
    /// gate that decides which of them to register has to treat this as "any of them". A literal
    /// name lands in `literals` and needs no such widening; this flag is for the rest.
    pub(crate) constructs_dynamic_class: bool,
    /// The program asks a question this walk cannot answer and whose WRONG ANSWER IS SILENT:
    /// it enumerates the symbol table (`get_defined_functions`, `eval`), or it PROBES a computed
    /// name (`function_exists($f)`). A dynamic CALL that is wrong fails loudly at the call site;
    /// a dynamic PROBE that is wrong just takes the guard's else branch and the program does
    /// something different for the rest of its life. Nothing approximates that, so it disables
    /// pruning outright.
    pub(crate) introspects: bool,
    /// Walk state, not a finding: true while the parsed source of a literal `eval` is being
    /// scanned, where an `include` resolves at runtime and names nothing this walk can read.
    pub(crate) scanning_eval_fragment: bool,
    /// An `include`/`require` whose path this walk cannot read as a string literal.
    ///
    /// Such an include RUNS PHP TEXT THAT DOES NOT EXIST YET, through the same interpreter
    /// `eval()` uses, and that text can name any class — exactly the hazard `introspects`
    /// records for `eval`. It is kept separate because the two answers differ: `eval` also
    /// hands over the symbol table and so must disable prelude pruning outright, whereas a
    /// runtime include reaches classes through the eval bridge's own construction surface.
    ///
    /// Every checker gate that widens a builtin class family for `eval` has the same reason to
    /// widen it for a runtime include, and must read this flag beside `introspects`:
    /// `types::checker::builtin_class_gate` (the throwable set) and
    /// `types::checker::builtin_types::reflection::gate` (the fourteen Reflection classes) do.
    pub(crate) includes_runtime_php: bool,
    /// An `eval()` that is not a transparent literal fragment (see `transparent_literal_eval`):
    /// text this walk cannot fully account for, which may load code, reach the program's own
    /// classes and functions, or run in the interpreter.
    pub(crate) opaque_eval: bool,
    /// Walk-only proof used by the environment gate to discard unreferenced callable facts.
    callable_declaration_roots: Option<super::environment_callables::DeclarationRoots>,
    /// Constructor roles are usable during full collection as well as declaration exclusion.
    callable_constructor_metadata: Option<super::environment_callables::ConstructorMetadata>,
    /// Defers native-constructor classification until forward aliases and parents are available.
    has_constructor_calls: bool,
    /// Environment-only static dispatch facts; ordinary symbol consumers still read `methods`.
    pub(super) callable_static_methods: HashSet<(String, String)>,
    pub(super) callable_wildcard_methods: HashSet<String>,
    pub(super) callable_class_edges: HashMap<String, HashSet<String>>,
    pub(super) callable_inherited_classes: HashSet<String>,
    pub(super) callable_interfaces: HashSet<String>,
    pub(super) callable_interface_methods: HashMap<String, HashSet<String>>,
    pub(super) callable_implementations: HashMap<String, HashSet<String>>,
    pub(super) callable_constructor_classes: HashSet<String>,
    /// Walk-only lexical class for self/static calls before namespace resolution.
    callable_current_class: Option<String>,
    /// Extra dispatch metadata is only needed by web environment demand, not ordinary consumers.
    environment_mode: bool,
    /// Whole-program call sites and escaped declaration identities, before name resolution.
    callable_parameter_inputs: super::environment_parameters::ParameterInputs,
    /// Present only in the environment walk, whose local binding facts are scope-isolated.
    callable_parameter_proof: Option<super::environment_parameters::ParameterProof>,
    /// Reachable global bindings whose mutation can invalidate top-level callable identities.
    callable_global_writes: HashSet<String>,
}

impl Usage {

    /// Returns true when a PHP function is referenced case-insensitively.
    pub(crate) fn references(&self, name: &str) -> bool {
        self.functions.contains(&php_symbol_key(name))
    }
}

/// PHP builtins that hand the whole symbol table to the program. None of them names what it
/// will reach, so seeing one means the prelude must be kept whole.
const INTROSPECTION_BUILTINS: &[&str] = &[
    "eval",
    "get_defined_functions",
    "get_declared_classes",
    "get_declared_interfaces",
    "get_declared_traits",
    "spl_autoload_register",
];

/// Builtins whose FIRST argument names a FUNCTION and is answered silently when it is missing.
///
/// A probe is not a call. `function_exists('imagecreatefromwebp')` returning `false` because the
/// pruner dropped the subject does not fail — it takes the guard's else branch, and the program
/// quietly does something else forever. So the subject is a reference, and a probe on a name
/// this walk cannot read is INTROSPECTION, not a mere dynamic call.
const FUNCTION_PROBES: &[&str] = &["function_exists", "is_callable"];

/// The same, for builtins whose first argument names a CLASS, INTERFACE or TRAIT.
/// One class probe, and WHICH ARGUMENT of it names the class.
///
/// The position is not decoration. `is_a($obj, 'Imagick')` names its class SECOND — the first
/// argument is the subject being tested. A table that assumes position zero reads `$obj`, finds
/// no literal, and keeps the whole surface: safe, but it throws the pass away for a construct
/// that was perfectly readable. Worse, on `is_a('Foo', 'Imagick')` it recorded `Foo` as the class
/// and `Imagick` as a MEMBER, so the one name that actually had to survive was rooted nowhere and
/// the probe would have answered false in silence.
///
/// `member` is the index of an argument naming a method or property, where the builtin has one.
struct ClassProbe {
    name: &'static str,
    class: usize,
    member: Option<usize>,
}

/// Constructs a builtin-class usage probe with an optional member identifier.
const fn probe(name: &'static str, class: usize, member: Option<usize>) -> ClassProbe {
    ClassProbe {
        name,
        class,
        member,
    }
}

const CLASS_PROBES: &[ClassProbe] = &[
    probe("class_exists", 0, None),
    probe("interface_exists", 0, None),
    probe("trait_exists", 0, None),
    probe("enum_exists", 0, None),
    probe("method_exists", 0, Some(1)),
    probe("property_exists", 0, Some(1)),
    probe("get_class_methods", 0, None),
    probe("get_class_vars", 0, None),
    probe("get_parent_class", 0, None),
    probe("class_implements", 0, None),
    probe("class_parents", 0, None),
    probe("class_uses", 0, None),
    probe("is_a", 1, None),
    probe("is_subclass_of", 1, None),
];

/// Prelude hooks and dynamic-name APIs absent from shared fixed-parameter callback roles.
const CALLBACK_BOUNDARY_EXCEPTIONS: &[&str] = &[
    "class_alias",
    "header_register_callback",
    "preg_replace_callback_array",
    "register_shutdown_function",
    "register_tick_function",
    "session_set_save_handler",
];

/// Joins shared callback roles with prelude hooks, XML setters and descriptor-producing escapes.
fn callable_taking_builtin(name: &str) -> bool {
    // Autoload registration is handled by introspection; neither SPL stub invokes its value.
    if matches!(name, "spl_autoload_register" | "spl_autoload_unregister") { return false; }
    if CALLBACK_BOUNDARY_EXCEPTIONS.contains(&name) { return true; }
    crate::builtins::registry::lookup(name).is_some_and(|definition| {
        !crate::builtins::registry::callback_parameter_indices(definition).is_empty()
            || matches!(definition.spec.semantics.argument_lowering,
                crate::builtins::semantics::BuiltinArgumentLowering::XmlHandlerSetter)
            || definition.return_type == crate::types::PhpType::Callable
    })
}

/// Treats observer and callback-dispatch functions as opaque when carried by a callable value.
pub(super) fn callable_gateway(name: &str) -> bool {
    let key = php_symbol_key(name.trim_start_matches('\\'));
    matches!(key.as_str(), "eval" | "get_defined_vars" | "compact")
        || callable_taking_builtin(&key)
        || key.rsplit_once("::").is_some_and(|(_, method)| method == "fromcallable")
}

/// Records one class name.
fn record_class(usage: &mut Usage, name: &str) {
    let name = name.trim_start_matches('\\');
    if !name.is_empty() {
        usage.classes.insert(php_symbol_key(name));
    }
}

/// Records one method name seen without a known receiver class.
fn record_method(usage: &mut Usage, name: &str) {
    usage.methods.insert(php_symbol_key(name));
    if usage.environment_mode { usage.callable_wildcard_methods.insert(php_symbol_key(name)); }
}

/// Records a named static receiver without rooting unrelated classes' same-named methods.
fn record_static_method(usage: &mut Usage, receiver: &crate::parser::ast::StaticReceiver, method: &str) {
    use crate::parser::ast::StaticReceiver;
    usage.methods.insert(php_symbol_key(method));
    if !usage.environment_mode { return; }
    let class = match receiver.written_class_receiver() {
        StaticReceiver::Named(name) => Some(name.as_canonical()),
        StaticReceiver::Self_ | StaticReceiver::Static => usage.callable_current_class.clone(),
        _ => None,
    };
    if let Some(class) = class {
        usage.callable_static_methods.insert((php_symbol_key(&class), php_symbol_key(method)));
    } else {
        usage.callable_wildcard_methods.insert(php_symbol_key(method));
    }
}

/// Applies native callback roles to every construction spelling before lowering changes its shape.
fn scan_constructor_callbacks(usage: &mut Usage, receiver: &crate::parser::ast::StaticReceiver, args: &[Expr]) {
    use super::environment_callables::ConstructorCallbacks;
    usage.has_constructor_calls |= usage.environment_mode && !args.is_empty();
    let Some(metadata) = &usage.callable_constructor_metadata else { return; };
    match metadata.constructor_callbacks_for_receiver(receiver, usage.callable_current_class.as_deref()) {
        Err(()) | Ok(Some(ConstructorCallbacks::Opaque)) => usage.callable_names.unknown_call(),
        Ok(Some(ConstructorCallbacks::Slots(indices))) => {
            let opaque_order = args.iter().any(|arg| matches!(arg.kind,
                ExprKind::NamedArg { .. } | ExprKind::Spread(_)));
            for index in indices {
                if opaque_order || !args.get(index).is_some_and(|arg| record_callback_slot(usage, arg, false)) {
                    usage.callable_names.unknown_call();
                }
            }
        }
        Ok(None) => {}
    }
}

/// Records a literal callable, in every form PHP accepts one, and reports whether it was READ.
///
/// `'strlen'` names a function; `'Imagick::clear'` names a class AND a method, and is legal
/// wherever a callable is; `['Imagick', 'clear']` and `[$obj, 'clear']` are the array forms,
/// whose second element names the method. `[$obj, 'clear']` counts as read even though the
/// receiver is opaque: the method name roots every class declaring it, which is the same
/// approximation `$obj->clear()` gets.
///
/// THE RETURN VALUE IS THE POINT. `$obj->$m()` desugars to `call_user_func([$obj, $m], …)`, so it
/// arrives wearing the very array literal that `['Imagick', 'clear']` wears. Judging the argument
/// by its SHAPE calls both analysable; judging it by what could be read separates them.
fn record_literal_callable(usage: &mut Usage, expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::StringLiteral(text) => {
            let Some(text) = crate::names::literal_symbol_name(text) else { return false; };
            if callable_gateway(&text) { usage.callable_names.unknown_call(); }
            if let Some((class, method)) = text.split_once("::") {
                record_class(usage, class);
                record_method(usage, method);
                true
            } else if text.is_empty() {
                false
            } else {
                record_name(usage, &text);
                true
            }
        }
        ExprKind::ArrayLiteral(items) => match items.as_slice() {
            [receiver, method] => {
                let receiver_read = if let ExprKind::StringLiteral(name) = &receiver.kind {
                    let Some(name) = crate::names::literal_symbol_name(name) else { return false; };
                    record_class(usage, &name);
                    true
                } else {
                    // Inline, `[$obj, 'm']` roots every class declaring `m`. Inside a literal
                    // fragment the interpreter resolves `[$c, 'fromArray']` against whatever
                    // builtin classes were registered, so an opaque receiver is a dynamic call.
                    !usage.scanning_eval_fragment
                };
                match &method.kind {
                    ExprKind::StringLiteral(name) => {
                        let Some(name) = crate::names::literal_symbol_name(name) else { return false; };
                        if name.eq_ignore_ascii_case("fromCallable") { usage.callable_names.unknown_call(); }
                        record_method(usage, &name);
                        receiver_read
                    }
                    _ => false,
                }
            }
            _ => false,
        },
        _ => false,
    }
}

/// Recognizes a value only after its position is known to carry a callback.
/// The ordinary expression scan still visits closure bodies and their symbol uses.
fn record_callback_slot(usage: &mut Usage, arg: &Expr, nullable: bool) -> bool {
    matches!(arg.kind, ExprKind::Closure { .. })
        || (nullable && matches!(arg.kind, ExprKind::Null))
        || record_literal_callable(usage, arg)
}

/// Requires every catalogued callback slot to be known, preserving opaque siblings.
/// Named/spread order remains conservative before the shared call planner runs.
fn record_builtin_callbacks(usage: &mut Usage, name: &str, args: &[Expr]) -> bool {
    if let Some(definition) = crate::builtins::registry::lookup(name) {
        let xml = matches!(definition.spec.semantics.argument_lowering,
            crate::builtins::semantics::BuiltinArgumentLowering::XmlHandlerSetter);
        let indices = if xml { (1..definition.params.len()).collect() }
            else { crate::builtins::registry::callback_parameter_indices(definition) };
        if !indices.is_empty() {
            if args.iter().any(|arg| matches!(arg.kind, ExprKind::NamedArg { .. } | ExprKind::Spread(_))) {
                return false;
            }
            return indices.into_iter().fold(true, |known, index| {
                let read = args.get(index).is_some_and(|arg| record_callback_slot(usage, arg, xml));
                read && known
            });
        }
    }
    // Prelude-only boundaries retain their existing symbol scan without accepting closure data.
    args.iter().fold(false, |seen, arg| record_literal_callable(usage, arg) || seen)
}

/// Collects direct and literal-indirect function references from a program.
pub(crate) fn collect(program: &[Stmt]) -> Usage {
    let mut usage = Usage::default();
    for stmt in program {
        scan_stmt(stmt, &mut usage);
    }
    usage
}

thread_local! {
    /// The usage of the program being type-checked, tagged with its slice (address, length).
    static PUBLISHED: std::cell::RefCell<Option<(usize, usize, std::rc::Rc<Usage>)>> =
        const { std::cell::RefCell::new(None) };
}

/// Restores the enclosing publication on drop.
pub(crate) struct PublishedUsage {
    previous: Option<(usize, usize, std::rc::Rc<Usage>)>,
    usage: std::rc::Rc<Usage>,
}

impl PublishedUsage {
    /// The published usage.
    pub(crate) fn usage(&self) -> &Usage {
        &self.usage
    }
}

impl Drop for PublishedUsage {
    fn drop(&mut self) {
        let previous = self.previous.take();
        PUBLISHED.with(|cell| *cell.borrow_mut() = previous);
    }
}

/// Collects `program`'s usage once and publishes it to [`collect_shared`] until the guard drops.
///
/// The type checker asked the same whole-program question from nine places (its own setup and
/// eight class-family gates), each a full walk of the program: 5% of a Symfony build's checker.
pub(crate) fn publish(program: &[Stmt]) -> PublishedUsage {
    let usage = std::rc::Rc::new(collect(program));
    let entry = (program.as_ptr() as usize, program.len(), std::rc::Rc::clone(&usage));
    let previous = PUBLISHED.with(|cell| cell.borrow_mut().replace(entry));
    PublishedUsage { previous, usage }
}

/// The usage of `program`: the published one when `program` is the slice it was collected from,
/// otherwise a fresh walk.
pub(crate) fn collect_shared(program: &[Stmt]) -> std::rc::Rc<Usage> {
    let identity = (program.as_ptr() as usize, program.len());
    let published = PUBLISHED.with(|cell| {
        cell.borrow()
            .as_ref()
            .filter(|(address, len, _)| (*address, *len) == identity)
            .map(|(_, _, usage)| std::rc::Rc::clone(usage))
    });
    published.unwrap_or_else(|| std::rc::Rc::new(collect(program)))
}

/// Collects ordinary usage plus the dispatch metadata needed to exclude unreferenced callbacks.
pub(crate) fn collect_environment(program: &[Stmt]) -> Usage {
    let mut usage = Usage { environment_mode: true, ..Usage::default() };
    scan_program(program, &mut usage);
    // A native callback can be the only opaque call, with no source helper to activate the gate.
    if usage.has_constructor_calls && !usage.callable_names.has_unknown_target() {
        usage.callable_constructor_metadata = Some(super::environment_callables::ConstructorMetadata::collect(&usage));
        scan_program(program, &mut usage);
        usage.callable_constructor_metadata = None;
    }
    usage
}

/// Checks opaque callable demand in a fixed point rooted by executable and implicit entries.
/// Direct storage observers remain the caller's whole-program findings. Escaped literals root
/// their declarations, and any remaining unknown callable forces population.
pub(crate) fn has_reachable_unknown_callable(program: &[Stmt], all_usage: &Usage) -> bool {
    let Some(roots) = super::environment_callables::declaration_roots(all_usage) else {
        return true;
    };
    let mut roots = roots.implicit_only(all_usage);
    let parameters = all_usage.callable_parameter_inputs.prove(&all_usage.literals, &roots, &all_usage.callable_class_edges);
    let constructors = super::environment_callables::ConstructorMetadata::collect(all_usage);
    loop {
        let mut usage = Usage {
            callable_declaration_roots: Some(roots.clone()),
            callable_constructor_metadata: Some(constructors.clone()),
            callable_parameter_proof: Some(parameters.clone()),
            environment_mode: true, ..Usage::default()
        };
        scan_program(program, &mut usage);
        for name in &usage.callable_global_writes { usage.callable_names.write(name); }
        let Some(next) = super::environment_callables::declaration_roots(&usage) else { return true; };
        if !roots.expand(next) { return usage.callable_names.has_unknown_target(); }
    }
}



/// Scans the source of a literal `eval` as the inline code it will run as, returning false
/// when the argument is not a string literal or does not parse (the interpreter reports the
/// latter at runtime). The fragment's own names, literals and hazards land in `usage` exactly
/// as if the statements were spelled inline; a runtime `include` inside it is introspection.
fn scan_literal_eval_fragment(source: Option<&Expr>, usage: &mut Usage) -> bool {
    let Some(Expr {
        kind: ExprKind::StringLiteral(source),
        ..
    }) = source
    else {
        return false;
    };
    let Some(fragment) =
        crate::eval_aot::parse_literal_fragment(source, crate::strict_php::is_enabled())
    else {
        return false;
    };
    let outer = std::mem::replace(&mut usage.scanning_eval_fragment, true);
    // The fragment's own dynamic calls are judged on their own: the two flags are cleared for
    // the scan and restored afterwards, so inline code that already made a computed call does
    // not hide the one the fragment makes.
    let dynamic_call_before = std::mem::take(&mut usage.dynamic_function_call);
    let dynamic_class_before = std::mem::take(&mut usage.constructs_dynamic_class);
    scan_program(&fragment, usage);
    usage.scanning_eval_fragment = outer;
    // Inline, a computed call or `new $c` is answered by rooting the names the program spells,
    // because the compiled call site fails loudly when the pruner guessed wrong. The interpreter
    // resolves the same constructs against whatever the binary registered and reports a missing
    // builtin as if the program never had it, so inside a fragment they are introspection: the
    // whole surface stays registered, exactly as it did when every eval was opaque.
    if usage.dynamic_function_call || usage.constructs_dynamic_class {
        usage.introspects = true;
    }
    usage.dynamic_function_call |= dynamic_call_before;
    usage.constructs_dynamic_class |= dynamic_class_before;
    true
}

/// Records one normalized PHP function name.
fn record_name(usage: &mut Usage, name: &str) {
    usage
        .functions
        .insert(php_symbol_key(name.trim_start_matches('\\')));
}

/// Records a literal callback/probe target when it denotes a free function.
fn record_literal_function(usage: &mut Usage, expr: Option<&Expr>) {
    let Some(Expr {
        kind: ExprKind::StringLiteral(name),
        ..
    }) = expr
    else {
        return;
    };
    if let Some(name) = crate::names::literal_symbol_name(name) {
        if !name.contains("::") && !name.is_empty() { record_name(usage, &name); }
    }
}

/// Scans parameter default expressions.
fn scan_params(
    params: &[(String, Option<crate::parser::ast::TypeExpr>, Option<Expr>, bool)],
    owner: Option<&str>,
    usage: &mut Usage,
) {
    for (index, (name, ty, default, _)) in params.iter().enumerate() {
        if owner.zip(usage.callable_parameter_proof.as_ref())
            .is_some_and(|(owner, proof)| proof.contains(owner, index))
        {
            usage.callable_names.simple_binding(name);
        } else { usage.callable_names.write(name); }
        if let Some(ty) = ty {
            scan_type(ty, usage);
        }
        if let Some(default) = default {
            scan_expr(default, usage);
        }
    }
}

/// Records every class named by a type expression.
///
/// A type hint is a reference: a kept method whose parameter is typed `ImagickPixel` needs that
/// class to exist, so the closure has to follow hints as well as call sites.
fn scan_type(ty: &crate::parser::ast::TypeExpr, usage: &mut Usage) {
    use crate::parser::ast::TypeExpr;
    match ty {
        TypeExpr::Named(name) => record_class(usage, &name.as_canonical()),
        TypeExpr::Nullable(inner) | TypeExpr::Array(inner) | TypeExpr::Buffer(inner) => {
            scan_type(inner, usage)
        }
        // Both halves, so a prelude class named only in an `array<K, V>` key or value is not
        // pruned out from under the program that declared it.
        TypeExpr::AssocArray { key, value } => {
            scan_type(key, usage);
            scan_type(value, usage);
        }
        TypeExpr::Union(members) | TypeExpr::Intersection(members) => {
            for member in members {
                scan_type(member, usage);
            }
        }
        TypeExpr::Ptr(Some(name)) => record_class(usage, crate::names::pointer_target_leaf(&name.as_canonical())),
        _ => {}
    }
}

/// Scans one class property's type and initializer.
fn scan_property(property: &ClassProperty, usage: &mut Usage) {
    if let Some(ty) = &property.type_expr {
        scan_type(ty, usage);
    }
    if let Some(default) = &property.default {
        scan_expr(default, usage);
    }
}

/// Scans one class-like method body, parameter types and defaults.
fn scan_method(method: &ClassMethod, usage: &mut Usage, can_exclude: bool) {
    let name = usage.callable_current_class.as_ref()
        .map(|class| format!("{class}::{}", method.name));
    if usage.environment_mode {
        // Trait/enum/interface methods may have no lexical class in this scan.
        usage.callable_parameter_inputs.declaration(
            name.as_deref().unwrap_or(&method.name), &method.params,
            can_exclude && method.attributes.is_empty(),
        );
    }
    scan_callable(
        name.as_deref().filter(|_| can_exclude && method.attributes.is_empty()),
        &method.params, method.variadic.as_deref(), method.return_type.as_ref(), &method.body, usage,
    );
}

/// Scans a callable contract/body only after its entry becomes reachable in the environment walk.
/// A missing name means an implicit, inherited, attributed or trait entry that stays conservative.
fn scan_callable(
    name: Option<&str>,
    params: &[(String, Option<crate::parser::ast::TypeExpr>, Option<Expr>, bool)],
    variadic: Option<&str>,
    return_type: Option<&crate::parser::ast::TypeExpr>,
    body: &[Stmt],
    usage: &mut Usage,
) {
    let exclude = name.zip(usage.callable_declaration_roots.as_ref()).is_some_and(|(name, roots)| {
        !super::environment_callables::is_referenced(roots, name)
    });
    if exclude { return; }
    let outer = enter_callable_scope(usage, &[]);
    scan_params(params, name, usage);
    if let Some(name) = variadic { usage.callable_names.write(name); }
    if let Some(ty) = return_type {
        scan_type(ty, usage);
    }
    scan_program(body, usage);
    leave_callable_scope(usage, outer);
}

/// Separates callable-local bindings while preserving only explicitly captured identities.
fn enter_callable_scope(usage: &mut Usage, captures: &[String]) -> Option<super::callable_names::CallableNames> {
    usage.callable_parameter_proof.as_ref()?;
    let inner = usage.callable_names.captures(captures);
    Some(std::mem::replace(&mut usage.callable_names, inner))
}

/// Propagates unresolved child calls without leaking its local identities into its parent.
fn leave_callable_scope(usage: &mut Usage, outer: Option<super::callable_names::CallableNames>) {
    if let Some(mut outer) = outer {
        if usage.callable_names.has_unknown_target() { outer.unknown_call(); }
        usage.callable_names = outer;
    }
}

/// Scans one class constant initializer.
fn scan_class_const(constant: &ClassConst, usage: &mut Usage) {
    scan_expr(&constant.value, usage);
}

/// Scans a statement list into an existing summary.
fn scan_program(program: &[Stmt], usage: &mut Usage) {
    for stmt in program {
        scan_stmt(stmt, usage);
    }
}

/// Scans every expression-bearing position of one statement.
fn scan_stmt(stmt: &Stmt, usage: &mut Usage) {
    match &stmt.kind {
        StmtKind::Assign { name, value } | StmtKind::TypedAssign { name, value, .. }
        | StmtKind::StaticVar { name, init: value } => usage.callable_names.assignment(name, value),
        StmtKind::RefAssign { target, source } => {
            usage.callable_names.write(target);
            usage.callable_names.write_target(source);
        }
        StmtKind::ArrayAssign { array, .. } | StmtKind::ArrayPush { array, .. } => usage.callable_names.write(array),
        StmtKind::NestedArrayAssign { target, .. } => usage.callable_names.write_target(target),
        StmtKind::Global { vars } => {
            for name in vars { usage.callable_names.write(name); }
            if usage.environment_mode { usage.callable_global_writes.extend(vars.iter().cloned()); }
        }
        StmtKind::ListUnpack { vars, .. } => {
            for name in vars { usage.callable_names.write(name); }
        }
        StmtKind::Foreach { array, key_var, value_var, value_by_ref, .. } => {
            if let Some(name) = key_var { usage.callable_names.write(name); }
            usage.callable_names.write(value_var);
            if *value_by_ref { usage.callable_names.write_target(array); }
        }
        StmtKind::Try { catches, .. } => {
            for name in catches.iter().filter_map(|catch| catch.variable.as_ref()) {
                usage.callable_names.write(name);
            }
        }
        _ => {}
    }
    match &stmt.kind {
        StmtKind::Assign { name, .. }
        | StmtKind::TypedAssign { name, .. }
        | StmtKind::StaticVar { name, .. }
        | StmtKind::RefAssign { target: name, .. }
        | StmtKind::ArrayAssign { array: name, .. }
        | StmtKind::ArrayPush { array: name, .. } => {
            usage.storage_variables.insert(name.clone());
        }
        StmtKind::Global { vars } | StmtKind::ListUnpack { vars, .. } => {
            usage.storage_variables.extend(vars.iter().cloned());
        }
        StmtKind::Foreach { key_var, value_var, .. } => {
            usage.storage_variables.extend(key_var.iter().cloned());
            usage.storage_variables.insert(value_var.clone());
        }
        StmtKind::Try { catches, .. } => {
            usage.storage_variables.extend(catches.iter().filter_map(|c| c.variable.clone()));
        }
        _ => {}
    }
    match &stmt.kind {
        StmtKind::Echo(expr)
        | StmtKind::Throw(expr)
        | StmtKind::ExprStmt(expr)
        | StmtKind::ConstDecl { value: expr, .. }
        | StmtKind::Assign { value: expr, .. }
        | StmtKind::TypedAssign { value: expr, .. }
        | StmtKind::StaticVar { init: expr, .. }
        | StmtKind::ListUnpack { value: expr, .. }
        | StmtKind::Return(Some(expr))
        | StmtKind::ArrayPush { value: expr, .. }
        | StmtKind::StaticPropertyAssign { value: expr, .. }
        | StmtKind::StaticPropertyArrayPush { value: expr, .. } => scan_expr(expr, usage),
        StmtKind::Include { path, .. } => {
            // The resolver has already inlined every include it could read, so one still here
            // resolves at runtime. Inside a literal eval fragment that is the normal case, and the
            // included file's contents are exactly what this walk cannot see.
            if usage.scanning_eval_fragment {
                usage.introspects = true;
            }
            record_include_path(path, usage);
            scan_expr(path, usage);
        }
        StmtKind::RefAssign { source, .. } => scan_expr(source, usage),
        StmtKind::PropertyAssign { object, value, .. }
        | StmtKind::PropertyArrayPush { object, value, .. } => {
            scan_expr(object, usage);
            scan_expr(value, usage);
        }
        StmtKind::PropertyArrayAssign {
            object,
            index,
            value,
            ..
        } => {
            scan_expr(object, usage);
            scan_expr(index, usage);
            scan_expr(value, usage);
        }
        StmtKind::ArrayAssign { index, value, .. }
        | StmtKind::StaticPropertyArrayAssign { index, value, .. } => {
            scan_expr(index, usage);
            scan_expr(value, usage);
        }
        // The by-reference property/static-element binds and the computed static-property write
        // are ordinary writes as far as this walk is concerned: every expression they carry can
        // still NAME a prelude function or class, and a name only reachable here would otherwise
        // be pruned and fail at run time.
        StmtKind::PropertyRefAssign { object, source, .. } => {
            scan_expr(object, usage);
            scan_expr(source, usage);
        }
        StmtKind::StaticPropertyElementRefAssign { index, source, .. } => {
            scan_expr(index, usage);
            scan_expr(source, usage);
        }
        StmtKind::DynamicStaticPropertyWrite {
            property,
            index,
            value,
            ..
        } => {
            scan_expr(property, usage);
            if let Some(index) = index {
                scan_expr(index, usage);
            }
            scan_expr(value, usage);
        }
        StmtKind::NestedArrayAssign { target, value } => {
            scan_expr(target, usage);
            scan_expr(value, usage);
        }
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            scan_expr(condition, usage);
            scan_program(then_body, usage);
            for (condition, body) in elseif_clauses {
                scan_expr(condition, usage);
                scan_program(body, usage);
            }
            if let Some(body) = else_body {
                scan_program(body, usage);
            }
        }
        StmtKind::IfDef {
            then_body,
            else_body,
            ..
        } => {
            scan_program(then_body, usage);
            if let Some(body) = else_body {
                scan_program(body, usage);
            }
        }
        StmtKind::While { condition, body } | StmtKind::DoWhile { condition, body } => {
            scan_expr(condition, usage);
            scan_program(body, usage);
        }
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => {
            if let Some(init) = init {
                scan_stmt(init, usage);
            }
            if let Some(condition) = condition {
                scan_expr(condition, usage);
            }
            if let Some(update) = update {
                scan_stmt(update, usage);
            }
            scan_program(body, usage);
        }
        StmtKind::Foreach { array, body, .. } => {
            scan_expr(array, usage);
            scan_program(body, usage);
        }
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => {
            scan_expr(subject, usage);
            for (patterns, body) in cases {
                for pattern in patterns {
                    scan_expr(pattern, usage);
                }
                scan_program(body, usage);
            }
            if let Some(body) = default {
                scan_program(body, usage);
            }
        }
        StmtKind::NamespaceBlock { body, .. } => {
            // Names inside a namespaced fragment resolve through aliases this walk does not
            // perform, so such a fragment is introspection.
            if usage.scanning_eval_fragment {
                usage.introspects = true;
            }
            scan_program(body, usage)
        }
        StmtKind::Synthetic(body)
        | StmtKind::IncludeOnceGuard { body, .. } => scan_program(body, usage),
        StmtKind::FunctionDecl {
            name,
            params,
            variadic,
            return_type,
            body,
            ..
        } => {
            // A function the fragment declares keeps a builtin or native declaration of the
            // same name registered, so the interpreter can refuse the duplicate as PHP does.
            if usage.scanning_eval_fragment {
                record_name(usage, name);
            }
            if usage.environment_mode {
                usage.callable_parameter_inputs.declaration(name, params, stmt.attributes.is_empty());
            }
            scan_callable(
                stmt.attributes.is_empty().then_some(name.as_str()),
                params, variadic.as_deref(), return_type.as_ref(), body, usage,
            );
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            scan_program(try_body, usage);
            for catch in catches {
                // `catch (ImagickException $e)` names the class as surely as `new` does.
                for exception in &catch.exception_types {
                    record_class(usage, &exception.as_canonical());
                }
                scan_program(&catch.body, usage);
            }
            if let Some(body) = finally_body {
                scan_program(body, usage);
            }
        }
        StmtKind::ClassDecl {
            name,
            extends,
            implements,
            trait_uses,
            properties,
            methods,
            constants,
            ..
        } => {
            let previous_class = if usage.environment_mode {
                if methods.iter().any(|method| method.name.eq_ignore_ascii_case("__construct")) {
                    usage.callable_constructor_classes.insert(php_symbol_key(name));
                }
                usage.callable_current_class.replace(name.clone())
            } else { None };
            // A class the fragment declares keeps a builtin of the same name registered, so the
            // interpreter can refuse the duplicate as PHP does (`eval('class SplStack {}')`).
            if usage.scanning_eval_fragment {
                record_class(usage, name);
            }
            // A supertype is a reference: keeping a class means keeping what it inherits from.
            if let Some(parent) = extends {
                record_class(usage, &parent.as_canonical());
                if usage.environment_mode {
                    super::environment_callables::class_edge(
                        &mut usage.callable_class_edges, name, &parent.as_canonical(),
                    );
                    usage.callable_inherited_classes.insert(php_symbol_key(&parent.as_canonical()));
                }
            }
            for interface in implements {
                record_class(usage, &interface.as_canonical());
                if usage.environment_mode {
                    usage.callable_implementations.entry(php_symbol_key(name)).or_default()
                        .insert(php_symbol_key(&interface.as_canonical()));
                }
            }
            for property in properties {
                scan_property(property, usage);
            }
            for method in methods {
                let known_interfaces = implements.is_empty()
                    || usage.callable_declaration_roots.as_ref().is_some_and(|roots| {
                        implements.iter().all(|name| roots.knows_interface(&name.as_canonical()))
                    });
                scan_method(method, usage, extends.is_none() && known_interfaces
                    && trait_uses.is_empty() && stmt.attributes.is_empty());
            }
            for constant in constants {
                scan_class_const(constant, usage);
            }
            usage.callable_current_class = previous_class;
        }
        StmtKind::InterfaceDecl {
            name,
            extends,
            properties,
            methods,
            constants,
            ..
        } => {
            if usage.environment_mode {
                usage.callable_interfaces.insert(php_symbol_key(name));
                usage.callable_interface_methods.entry(php_symbol_key(name)).or_default()
                    .extend(methods.iter().map(|method| php_symbol_key(&method.name)));
            }
            // Same as a class declaration: the declared name keeps a builtin of that name, and
            // a parent interface is a reference (`interface Child extends DateTimeInterface`).
            if usage.scanning_eval_fragment {
                record_class(usage, name);
            }
            for parent in extends {
                record_class(usage, &parent.as_canonical());
                if usage.environment_mode {
                    super::environment_callables::class_edge(
                        &mut usage.callable_class_edges, name, &parent.as_canonical(),
                    );
                }
            }
            for property in properties {
                scan_property(property, usage);
            }
            for method in methods {
                scan_method(method, usage, false);
            }
            for constant in constants {
                scan_class_const(constant, usage);
            }
        }
        StmtKind::TraitDecl {
            name,
            properties,
            methods,
            constants,
            ..
        } => {
            // Same as a class declaration: the declared name keeps a builtin of that name.
            if usage.scanning_eval_fragment {
                record_class(usage, name);
            }
            for property in properties {
                scan_property(property, usage);
            }
            for method in methods {
                scan_method(method, usage, false);
            }
            for constant in constants {
                scan_class_const(constant, usage);
            }
        }
        StmtKind::EnumDecl { name, cases, methods, constants, .. } => {
            if usage.scanning_eval_fragment {
                record_class(usage, name);
            }
            for case in cases {
                if let Some(value) = &case.value {
                    scan_expr(value, usage);
                }
            }
            for method in methods {
                scan_method(method, usage, false);
            }
            for constant in constants {
                scan_class_const(constant, usage);
            }
        }
        // `use function imagecreate as ic;` names `imagecreate`, and this walk runs BEFORE name
        // resolution — so by the time anything could resolve `ic()` back to it, the pruner has
        // already decided. The IMPORT is the reference, whichever kind it is.
        StmtKind::UseDecl { imports } => {
            // An import inside a fragment aliases names this walk cannot follow.
            if usage.scanning_eval_fragment {
                usage.introspects = true;
            }
            for import in imports {
                let name = import.name.as_canonical();
                if usage.environment_mode && import.kind == crate::parser::ast::UseKind::Class {
                    super::environment_callables::class_edge(
                        &mut usage.callable_class_edges, &import.alias, &name,
                    );
                }
                record_name(usage, &name);
                record_class(usage, &name);
                if usage.environment_mode && import.kind == crate::parser::ast::UseKind::Function {
                    usage.callable_parameter_inputs.escape(&name, false);
                }
                if callable_taking_builtin(&php_symbol_key(&name)) {
                    usage.callable_names.unknown_call();
                }
            }
        }
        StmtKind::FunctionVariantGroup { variants, .. } => {
            // The resolver dispatches the public name through any of these internal bodies.
            for variant in variants {
                record_name(usage, variant);
                if usage.environment_mode { usage.callable_parameter_inputs.escape(variant, false); }
            }
        }
        StmtKind::ExternFunctionDecl { params, .. } => {
            // Native code can retain and invoke an escaped callable without a PHP call site.
            if params.iter().any(|param| matches!(param.c_type, crate::parser::ast::CType::Callable)) {
                usage.callable_names.unknown_call();
            }
        }
        StmtKind::NamespaceDecl { .. } if usage.scanning_eval_fragment => {
            // A namespaced fragment resolves its names through aliases this walk does not perform.
            usage.introspects = true;
        }
        StmtKind::Return(None)
        | StmtKind::Break(_)
        | StmtKind::Continue(_)
        | StmtKind::NamespaceDecl { .. }
        | StmtKind::FunctionVariantMark { .. }
        | StmtKind::IncludeOnceMark { .. }
        | StmtKind::Global { .. }
        | StmtKind::PackedClassDecl { .. }
        | StmtKind::ExternClassDecl { .. }
        | StmtKind::ClassLikeActivate { .. }
        | StmtKind::ExternGlobalDecl { .. } => {}
    }
}

/// Records an `include`/`require` whose path this walk cannot read as a literal.
///
/// A literal path names a file the compiler resolves and inlines, so its declarations are part
/// of the program this walk already sees. Anything else is decided at run time and executed by
/// the interpreter, which can name a class no static walk can read.
fn record_include_path(path: &Expr, usage: &mut Usage) {
    if !matches!(path.kind, ExprKind::StringLiteral(_)) {
        usage.includes_runtime_php = true;
    }
}

/// Scans every child and callable position of one expression.
fn scan_expr(expr: &Expr, usage: &mut Usage) {
    if usage.environment_mode {
        match &expr.kind {
            ExprKind::FunctionCall { name, args } => usage.callable_parameter_inputs.call(name.as_str(), false, args),
            ExprKind::StaticMethodCall { receiver, method, args } => usage.callable_parameter_inputs.static_call(
                receiver, usage.callable_current_class.as_deref(), method, args,
            ),
            ExprKind::MethodCall { method, args, .. }
            | ExprKind::NullsafeMethodCall { method, args, .. } => usage.callable_parameter_inputs.call(method, true, args),
            ExprKind::FirstClassCallable(CallableTarget::Function(name)) => usage.callable_parameter_inputs.escape(name.as_str(), false),
            ExprKind::FirstClassCallable(CallableTarget::Method { method, .. }
                | CallableTarget::StaticMethod { method, .. }) => usage.callable_parameter_inputs.escape(method, true),
            _ => {}
        }
    }
    match &expr.kind {
        ExprKind::FunctionCall { args, .. } | ExprKind::ExprCall { args, .. }
        | ExprKind::ClosureCall { args, .. } | ExprKind::MethodCall { args, .. }
        | ExprKind::NullsafeMethodCall { args, .. } | ExprKind::NullsafeDynamicMethodCall { args, .. }
        | ExprKind::StaticMethodCall { args, .. } | ExprKind::NewObject { args, .. }
        | ExprKind::NewGeneric { args, .. } | ExprKind::NewDynamic { args, .. }
        | ExprKind::NewDynamicObject { args, .. } | ExprKind::NewScopedObject { args, .. } => {
            usage.callable_names.arguments(args);
        }
        _ => {}
    }
    match &expr.kind {
        ExprKind::IncludeValue { path, .. } => {
            // Like the include statement: a runtime include inside a fragment loads code this
            // walk cannot read.
            if usage.scanning_eval_fragment {
                usage.introspects = true;
            }
            record_include_path(path, usage);
            scan_expr(path, usage);
        }
        ExprKind::FunctionCall { name, args } => {
            let name = php_symbol_key(name.as_str().trim_start_matches('\\'));
            record_name(usage, &name);
            if callable_taking_builtin(&name) && !args.is_empty() {
                usage.callable_names.unknown_call();
            }
            let literal_first = matches!(
                args.first().map(|arg| &arg.kind),
                Some(ExprKind::StringLiteral(_))
            );
            if FUNCTION_PROBES.contains(&name.as_str()) {
                // A PROBE on a name this walk cannot read fails SILENTLY when it is wrong, so it
                // is introspection rather than a mere dynamic call.
                if literal_first {
                    record_literal_function(usage, args.first());
                } else {
                    usage.introspects = true;
                }
            } else if let Some(probe) = CLASS_PROBES.iter().find(|p| p.name == name) {
                // Read the argument that actually names the class, not whichever came first.
                if let Some(Expr {
                    kind: ExprKind::StringLiteral(class),
                    ..
                }) = args.get(probe.class)
                {
                    record_class(usage, class);
                    // `method_exists('C', 'm')` and friends name the member too.
                    if let Some(Expr {
                        kind: ExprKind::StringLiteral(member),
                        ..
                    }) = probe.member.and_then(|index| args.get(index))
                    {
                        record_method(usage, member);
                    }
                } else {
                    // The class it asks about cannot be read, and a wrong answer is silent.
                    usage.introspects = true;
                }
                // `is_a`/`is_subclass_of` accept a class NAME as their subject as well, so that
                // argument names a class too when it is a literal.
                if probe.class != 0 {
                    if let Some(Expr {
                        kind: ExprKind::StringLiteral(subject),
                        ..
                    }) = args.first()
                    {
                        record_class(usage, subject);
                    }
                }
            } else if callable_taking_builtin(&name) {
                // Callback positions come from the catalog, including every XML handler slot.
                let read = record_builtin_callbacks(usage, &name, args);
                // An argumentless `ob_start()` installs no callback and is not a hazard; one whose
                // arguments exist but say nothing — `usort($a, $cmp)`, `call_user_func($x)` — is.
                if !read && !args.is_empty() {
                    usage.dynamic_function_call = true;
                }
            } else if name == "eval" {
                // A TRANSPARENT fragment (see `transparent_literal_eval`) is source like any
                // other: its names are read here. Treating `eval('return 1;')` as introspection
                // disabled every prelude's pruning and, through the session group's interpreter
                // calls, linked the eval bridge into a program whose eval compiles ahead of time.
                match transparent_literal_eval(args) {
                    Some(fragment) => scan_program(&fragment, usage),
                    None => {
                        // Any other eval may load code, reach the program's own classes and
                        // functions, or run in the interpreter.
                        usage.opaque_eval = true;
                        // Only a runtime-sourced (or unparsable) eval hands the whole symbol
                        // table over; a literal one is scanned as the inline code it is.
                        if !scan_literal_eval_fragment(args.first(), usage) {
                            usage.introspects = true;
                        }
                    }
                }
            } else if INTROSPECTION_BUILTINS.contains(&name.as_str()) {
                usage.introspects = true;
            }
            for arg in args {
                scan_expr(arg, usage);
            }
        }
        ExprKind::FirstClassCallable(CallableTarget::Function(name)) => {
            record_name(usage, name.as_str());
            if callable_gateway(name.as_str()) { usage.callable_names.unknown_call(); }
        }
        // A first-class callable ROOTS EXACTLY WHAT CALLING IT WOULD. `Imagick::queryFormats(...)`
        // names a class and a method just as `Imagick::queryFormats()` does; deferring the call
        // does not make the declaration unnecessary, it makes the reference easier to miss.
        //
        // These two arms used to root less than their calling counterparts a few lines below:
        // the static one recorded nothing at all, and the instance one scanned the receiver but
        // dropped the method name — which is the ONLY handle there is, since the receiver's
        // class is unknown before name resolution. Either way the pruner then deleted a
        // declaration the program goes on to use, and the failure surfaces as an undefined
        // class rather than as anything pointing back here.
        ExprKind::FirstClassCallable(CallableTarget::Method { object, method }) => {
            if method.eq_ignore_ascii_case("fromCallable") { usage.callable_names.unknown_call(); }
            record_method(usage, method);
            scan_expr(object, usage);
        }
        ExprKind::FirstClassCallable(CallableTarget::StaticMethod { receiver, method }) => {
            if method.eq_ignore_ascii_case("fromCallable") { usage.callable_names.unknown_call(); }
            if let crate::parser::ast::StaticReceiver::Named(name) = receiver {
                record_class(usage, &name.as_canonical());
            }
            record_static_method(usage, receiver, method);
        }
        ExprKind::ExprCall { callee, args } => {
            usage.callable_names.expression_call(callee);
            usage.dynamic_function_call = true;
            scan_expr(callee, usage);
            for arg in args {
                scan_expr(arg, usage);
            }
        }
        ExprKind::ClosureCall { var, args } => {
            usage.callable_names.variable_call(var);
            usage.dynamic_function_call = true;
            for arg in args {
                scan_expr(arg, usage);
            }
        }
        ExprKind::BinaryOp { left, right, .. } => {
            scan_expr(left, usage);
            scan_expr(right, usage);
        }
        ExprKind::InstanceOf { value, target } => {
            scan_expr(value, usage);
            match target {
                InstanceOfTarget::Expr(target) => scan_expr(target, usage),
                InstanceOfTarget::Name(name) => record_class(usage, &name.as_canonical()),
                InstanceOfTarget::Generic(class_type) => {
                    for name in class_type.named_classes() {
                        record_class(usage, crate::names::pointer_target_leaf(&name.as_canonical()));
                    }
                }
            }
        }
        ExprKind::Negate(inner)
        | ExprKind::Not(inner)
        | ExprKind::BitNot(inner)
        | ExprKind::Throw(inner)
        | ExprKind::Clone(inner)
        | ExprKind::ErrorSuppress(inner)
        | ExprKind::Print(inner)
        | ExprKind::Spread(inner)
        | ExprKind::Cast { expr: inner, .. }
        | ExprKind::PtrCast { expr: inner, .. } => scan_expr(inner, usage),
        ExprKind::YieldFrom(inner) => {
            usage.uses_yield = true;
            scan_expr(inner, usage);
        }
        ExprKind::NullCoalesce { value, default }
        | ExprKind::ShortTernary { value, default } => {
            scan_expr(value, usage);
            scan_expr(default, usage);
        }
        ExprKind::Pipe { value, callable } => {
            usage.callable_names.expression_call(callable);
            usage.callable_names.arguments(std::slice::from_ref(value.as_ref()));
            scan_expr(value, usage);
            scan_expr(callable, usage);
        }
        ExprKind::Assignment {
            target,
            value,
            result_target,
            prelude,
            ..
        } => {
            if let ExprKind::Variable(name) = &target.kind {
                usage.callable_names.assignment(name, value);
            } else {
                usage.callable_names.write_target(target);
            }
            scan_expr(target, usage);
            scan_expr(value, usage);
            if let Some(result_target) = result_target {
                scan_expr(result_target, usage);
            }
            scan_program(prelude, usage);
        }
        ExprKind::ArrayLiteral(items) => {
            for item in items {
                scan_expr(item, usage);
            }
        }
        ExprKind::ArrayLiteralAssoc(items) => {
            for (key, value) in items {
                scan_expr(key, usage);
                scan_expr(value, usage);
            }
        }
        ExprKind::ArrayLiteralMixed(entries) => {
            for entry in entries.iter() {
                for expr in entry.exprs() {
                    scan_expr(expr, usage);
                }
            }
        }
        ExprKind::Match {
            subject,
            arms,
            default,
        } => {
            scan_expr(subject, usage);
            for (patterns, value) in arms {
                for pattern in patterns {
                    scan_expr(pattern, usage);
                }
                scan_expr(value, usage);
            }
            if let Some(default) = default {
                scan_expr(default, usage);
            }
        }
        ExprKind::ArrayAccess { array, index } => {
            scan_expr(array, usage);
            scan_expr(index, usage);
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            scan_expr(condition, usage);
            scan_expr(then_expr, usage);
            scan_expr(else_expr, usage);
        }
        ExprKind::Closure { params, variadic, body, captures, capture_refs, .. } => {
            for name in capture_refs { usage.callable_names.write(name); }
            usage.storage_variables.extend(captures.iter().chain(capture_refs).cloned());
            let mut outer = enter_callable_scope(usage, captures);
            scan_params(params, None, usage);
            if let Some(name) = variadic { usage.callable_names.write(name); }
            scan_program(body, usage);
            if let Some(outer) = &mut outer {
                usage.callable_names.propagate_capture_calls(outer, captures);
            }
            leave_callable_scope(usage, outer);
        }
        ExprKind::NamedArg { value, .. } => scan_expr(value, usage),
        ExprKind::StaticMethodCall {
            receiver,
            method,
            args,
        } => {
            if method.eq_ignore_ascii_case("__construct") {
                scan_constructor_callbacks(usage, receiver, args);
            }
            // An alias may hide the Closure class before name resolution.
            if method.eq_ignore_ascii_case("fromCallable") && !args.is_empty()
                && !args.first().is_some_and(|arg| record_callback_slot(usage, arg, false))
            {
                usage.callable_names.unknown_call();
            }
            if let crate::parser::ast::StaticReceiver::Named(name) = receiver {
                let class = name.as_canonical();
                record_class(usage, &class);
                // `Closure::fromCallable('f')` invokes what it names, like `call_user_func`; one
                // whose argument this walk cannot read is a dynamic call.
                if php_symbol_key(&class) == "closure" && php_symbol_key(method) == "fromcallable" {
                    let read = args.iter().fold(false, |seen, arg| {
                        record_literal_callable(usage, arg) || seen
                    });
                    if !read && !args.is_empty() {
                        usage.dynamic_function_call = true;
                    }
                }
            }
            record_static_method(usage, receiver, method);
            for arg in args {
                scan_expr(arg, usage);
            }
        }
        // The head of the constructed type is the class this construction keeps alive; its
        // type arguments name classes of their own and are recorded the same way.
        ExprKind::NewGeneric { class_type, args } => {
            scan_constructor_callbacks(usage, &crate::parser::ast::StaticReceiver::Generic(class_type.clone()), args);
            for name in class_type.named_classes() {
                record_class(usage, crate::names::pointer_target_leaf(&name.as_canonical()));
            }
            for arg in args {
                scan_expr(arg, usage);
            }
        }
        ExprKind::NewObject { class_name, args } => {
            record_class(usage, &class_name.as_canonical());
            scan_constructor_callbacks(usage, &crate::parser::ast::StaticReceiver::Named(class_name.clone()), args);
            for arg in args {
                scan_expr(arg, usage);
            }
        }
        ExprKind::NewScopedObject { receiver, args } => {
            scan_constructor_callbacks(usage, receiver, args);
            for arg in args {
                scan_expr(arg, usage);
            }
        }
        ExprKind::NewDynamic { name_expr, args } => {
            usage.constructs_dynamic_class = true;
            usage.callable_names.unknown_call();
            scan_expr(name_expr, usage);
            for arg in args {
                scan_expr(arg, usage);
            }
        }
        ExprKind::NewDynamicObject {
            class_name, args, ..
        } => {
            usage.constructs_dynamic_class = true;
            usage.callable_names.unknown_call();
            scan_expr(class_name, usage);
            for arg in args {
                scan_expr(arg, usage);
            }
        }
        ExprKind::PropertyAccess { object, .. }
        | ExprKind::NullsafePropertyAccess { object, .. } => scan_expr(object, usage),
        ExprKind::DynamicPropertyAccess { object, property }
        | ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
            scan_expr(object, usage);
            scan_expr(property, usage);
        }
        ExprKind::MethodCall {
            object,
            method,
            args,
        }
        | ExprKind::NullsafeMethodCall {
            object,
            method,
            args,
        } => {
            if method.eq_ignore_ascii_case("__construct") { usage.callable_names.unknown_call(); }
            // The receiver's class is unknown before name resolution, so the METHOD NAME is the
            // only handle. The pruner roots every class that declares it.
            record_method(usage, method);
            scan_expr(object, usage);
            for arg in args {
                scan_expr(arg, usage);
            }
        }
        ExprKind::NullsafeDynamicMethodCall {
            object,
            method,
            args,
        } => {
            // `$obj?->$m()` keeps its own node, where `$obj->$m()` desugars to `call_user_func`.
            // Only the nullsafe spelling reaches here, so the hazard is armed at both places or
            // at neither.
            if !record_literal_callable(usage, method) {
                usage.dynamic_function_call = true;
                usage.callable_names.unknown_call();
            }
            scan_expr(object, usage);
            scan_expr(method, usage);
            for arg in args {
                scan_expr(arg, usage);
            }
        }
        ExprKind::BufferNew { len, .. } => scan_expr(len, usage),
        ExprKind::Yield { key, value } => {
            usage.uses_yield = true;
            if let Some(key) = key {
                scan_expr(key, usage);
            }
            if let Some(value) = value {
                scan_expr(value, usage);
            }
        }
        ExprKind::Variable(name) => {
            // A name repeats far more often than it first appears, and `collect` runs
            // five times per compile, so check membership before paying for the clone.
            if !usage.variables.contains(name.as_str()) {
                usage.variables.insert(name.clone());
            }
        }
        ExprKind::StringLiteral(text) => {
            if let Some(text) = crate::names::literal_symbol_name(text) {
                usage.literals.insert(php_symbol_key(text.trim_start_matches('\\')));
            }
        }
        ExprKind::ClassConstant { receiver, .. }
        | ExprKind::StaticPropertyAccess { receiver, .. }
        | ExprKind::ScopedConstantAccess { receiver, .. } => {
            if let crate::parser::ast::StaticReceiver::Named(name) = receiver {
                record_class(usage, &name.as_canonical());
            }
        }
        ExprKind::ConstRef(name) => {
            usage.constants.insert(
                name.as_canonical()
                    .trim_start_matches('\\')
                    .to_string(),
            );
        }
        ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::Null
        | ExprKind::This
        | ExprKind::MagicConstant(_) => {}
        ExprKind::PreIncrement(name)
        | ExprKind::PostIncrement(name)
        | ExprKind::PreDecrement(name)
        | ExprKind::PostDecrement(name) => {
            usage.callable_names.write(name);
            usage.storage_variables.insert(name.clone());
        }
        ExprKind::ObjectClassName { object } => scan_expr(object, usage),
        // A by-reference array element, a computed static-property read and a computed scoped
        // constant all carry expressions that can still NAME a prelude declaration.
        ExprKind::ArrayReference(inner) => scan_expr(inner, usage),
        ExprKind::DynamicStaticPropertyAccess { receiver, property } => {
            if let crate::parser::ast::StaticReceiver::Named(name) = receiver {
                record_class(usage, &name.as_canonical());
            }
            scan_expr(property, usage);
        }
        ExprKind::DynamicScopedConstantAccess { receiver, .. } => scan_expr(receiver, usage),
    }
}

/// The parsed statements of an `eval()` whose argument is a TRANSPARENT literal fragment: it
/// parses, declares nothing, and calls functions by name only -- no class, method, static
/// member, dynamic call, runtime include, further opaque `eval`, or a builtin that defines a
/// symbol at run time. Such a fragment cannot load code, so every walk may read it as ordinary
/// source, its named functions included. A fragment that touches classes keeps the opaque
/// treatment: the interpreter may run it, and it needs their bridge metadata.
pub(crate) fn transparent_literal_eval(args: &[Expr]) -> Option<Vec<Stmt>> {
    let ExprKind::StringLiteral(fragment) = &args.first()?.kind else {
        return None;
    };
    // Any include loads a file this walk never reads, whatever its path spells: the file may not
    // even exist until run time.
    let lowered = fragment.to_ascii_lowercase();
    if lowered.contains("include") || lowered.contains("require") {
        return None;
    }
    let program = crate::eval_aot::parse_literal_fragment(fragment, false)?;
    if fragment_declares(&program) {
        return None;
    }
    let usage = collect(&program);
    let builtin_only = !usage.opaque_eval
        && !usage.dynamic_function_call
        && !usage.includes_runtime_php
        && !usage.introspects
        && !usage.constructs_dynamic_class
        && usage.methods.is_empty()
        && usage.classes.is_empty()
        && !usage.functions.iter().any(|name| {
            matches!(
                name.as_str(),
                "define" | "class_alias" | "spl_autoload_register" | "set_include_path"
            )
        });
    builtin_only.then_some(program)
}

/// Whether a parsed fragment declares a function, class-like or constant at any depth.
fn fragment_declares(program: &[Stmt]) -> bool {
    fn declares(stmt: &Stmt) -> bool {
        match &stmt.kind {
            StmtKind::FunctionDecl { .. }
            | StmtKind::ClassDecl { .. }
            | StmtKind::InterfaceDecl { .. }
            | StmtKind::TraitDecl { .. }
            | StmtKind::EnumDecl { .. }
            | StmtKind::ConstDecl { .. } => true,
            StmtKind::If { then_body, elseif_clauses, else_body, .. } => {
                then_body.iter().any(declares)
                    || elseif_clauses.iter().any(|(_, body)| body.iter().any(declares))
                    || else_body.as_ref().is_some_and(|body| body.iter().any(declares))
            }
            StmtKind::NamespaceBlock { body, .. } => body.iter().any(declares),
            _ => false,
        }
    }
    program.iter().any(declares)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn program(source: &str) -> Vec<Stmt> {
        let tokens = crate::lexer::tokenize(source).expect("tokenize");
        crate::parser::parse(&tokens).expect("parse")
    }

    /// The checker publishes one walk of its program and every gate asking about the SAME slice
    /// gets that walk back; any other slice is walked afresh, and the guard ends the publication.
    #[test]
    fn published_usage_is_shared_only_for_its_own_slice() {
        let checked = program("<?php strlen('a');");
        let other = program("<?php trim('a');");
        {
            let published = publish(&checked);
            assert!(published.usage().references("strlen"));
            let first = collect_shared(&checked);
            let second = collect_shared(&checked);
            assert!(std::rc::Rc::ptr_eq(&first, &second), "the same slice reuses the walk");
            let fresh = collect_shared(&other);
            assert!(fresh.references("trim") && !fresh.references("strlen"));
        }
        let after = collect_shared(&checked);
        assert!(after.references("strlen"));
        assert!(!std::rc::Rc::ptr_eq(&after, &collect_shared(&checked)), "the guard ended the publication");
    }

    use crate::parser::ast::Program;

    /// Parses one tagged fixture without resolution or optimization passes.
    fn parse(source: &str) -> Program {
        let tokens = crate::lexer::tokenize(source).expect("fixture must tokenize");
        crate::parser::parse(&tokens).expect("fixture must parse")
    }

    /// Literal XML handlers retain their bodies without activating unrelated native bridges.
    #[test]
    fn literal_xml_handlers_do_not_activate_tokenizer() {
        let program = parse(include_str!("../../scripts/ci/fixtures/ios-xml-link/main.php"));
        assert!(!crate::tokenizer_prelude::program_uses_surface(&program));
        assert!(!collect(&program).dynamic_function_call);

        let observing = parse("<?php xml_set_character_data_handler($parser, function () { echo $_ENV['PATH']; });");
        assert!(!crate::tokenizer_prelude::program_uses_surface(&observing));
        assert!(crate::web_prelude::program_observes_environment(&observing));

        let opaque = parse("<?php xml_set_character_data_handler($parser, $callback);");
        assert!(crate::tokenizer_prelude::program_uses_surface(&opaque));
        assert!(crate::web_prelude::program_observes_environment(&opaque));
    }

    /// A known callback cannot hide an opaque callback slot or a dynamic method target.
    #[test]
    fn literal_callback_slots_preserve_opaque_siblings() {
        for source in [
            "<?php xml_set_element_handler($parser, function () {}, $callback);",
            "<?php xml_set_element_handler($parser, $callback, function () {});",
            "<?php xml_set_element_handler($parser, 'start_handler', $callback);",
            "<?php xml_set_element_handler($parser, function () {}, ...$handlers);",
            "<?php xml_set_element_handler(parser: $parser, start_handler: function () {}, end_handler: $callback);",
            "<?php xml_set_character_data_handler($parser, function () use ($callback) { $callback(); });",
            "<?php call_user_func($callback, function () { return 1; });",
            "<?php $object->$method(function () { return 1; });",
            "<?php $object?->$method(function () { return 1; });",
        ] {
            let program = parse(source);
            assert!(collect(&program).dynamic_function_call, "{source}");
            assert!(crate::tokenizer_prelude::program_uses_surface(&program), "{source}");
        }
        let usage = collect(&parse("<?php eval('call_user_func($callback, function () {});');"));
        assert!(usage.introspects);
    }

    /// Disabled XML handlers and known callback slots retain precise symbol references.
    #[test]
    fn literal_callback_slots_keep_disabled_xml_handlers_precise() {
        for source in [
            "<?php xml_set_element_handler($parser, function () {}, null);",
            "<?php xml_set_element_handler($parser, null, function () {});",
            "<?php xml_set_character_data_handler($parser, null);",
            "<?php call_user_func(function ($value) {}, $unknownData);",
            "<?php usort($array, function ($a, $b) { return 0; });",
        ] {
            let program = parse(source);
            assert!(!collect(&program).dynamic_function_call, "{source}");
            assert!(!crate::tokenizer_prelude::program_uses_surface(&program), "{source}");
        }
        let usage = collect(&parse("<?php xml_set_element_handler($parser, 'start_handler', 'end_handler');"));
        assert!(usage.references("start_handler"));
        assert!(usage.references("end_handler"));
    }

    /// Autoload registration preserves the complete prelude without implying a callback invoke.
    #[test]
    fn autoload_registration_preserves_introspection_without_callback_demand() {
        for source in [
            "<?php spl_autoload_register();",
            "<?php spl_autoload_register(function ($class) { require __DIR__ . '/' . $class . '.php'; });",
            "<?php spl_autoload_register('loader');",
        ] {
            let program = parse(source);
            let usage = collect(&program);
            assert!(usage.introspects, "{source}");
            assert!(!usage.dynamic_function_call, "{source}");
            assert!(!crate::web_prelude::program_observes_environment(&program), "{source}");
        }
        let import = parse("<?php use function spl_autoload_register as reg;");
        assert!(!crate::web_prelude::program_observes_environment(&import));
        let observer = parse("<?php spl_autoload_register(function () { echo $_ENV['PATH']; });");
        assert!(crate::web_prelude::program_observes_environment(&observer));
    }

    /// Removing an autoload hook does not invoke or dynamically discover the supplied callable.
    #[test]
    fn autoload_unregistration_does_not_invoke_callbacks() {
        for source in [
            "<?php spl_autoload_unregister('loader');",
            "<?php spl_autoload_unregister($callback);",
            "<?php use function spl_autoload_unregister as unreg;",
        ] {
            let program = parse(source);
            let usage = collect(&program);
            assert!(!usage.introspects, "{source}");
            assert!(!usage.dynamic_function_call, "{source}");
            assert!(!crate::tokenizer_prelude::program_uses_surface(&program), "{source}");
            assert!(!crate::web_prelude::program_observes_environment(&program), "{source}");
        }
    }

    /// A literal eval contributes the names and literals of its source and is not introspection.
    #[test]
    fn literal_eval_is_collected_as_inline_code() {
        let usage = collect(&parse(
            "<?php eval('throw new DomainException(\"x\"); echo imagecreate(1, 1);');",
        ));
        assert!(!usage.introspects);
        assert!(!usage.scanning_eval_fragment);
        assert!(usage.classes.contains("domainexception"));
        assert!(usage.references("imagecreate"));
    }

    /// A runtime-sourced eval, an unparsable literal, and a runtime include inside a literal
    /// fragment are all introspection: nothing the walk can read names what they reach.
    #[test]
    fn opaque_evals_remain_introspection() {
        for source in [
            "<?php eval($argv[1]);",
            "<?php eval('not php at all');",
            "<?php eval('include \"part.php\";');",
        ] {
            let usage = collect(&parse(source));
            assert!(usage.introspects, "{source}");
            assert!(!usage.scanning_eval_fragment, "{source}");
        }
    }

    /// A computed call inside a literal fragment is introspection: the interpreter resolves it
    /// against whatever was registered, with no literal fallback roots to lean on.
    #[test]
    fn literal_eval_dynamic_call_is_introspection() {
        let usage = collect(&parse("<?php eval('$f = \"strlen\"; echo $f(\"x\");');"));
        assert!(usage.introspects);
        assert!(usage.dynamic_function_call);
    }

    /// A computed call outside the fragment stays a plain dynamic call even when a literal eval
    /// is present, so the fragment scan does not retroactively reclassify inline code.
    #[test]
    fn inline_dynamic_call_beside_a_literal_eval_stays_a_dynamic_call() {
        let usage = collect(&parse("<?php $f = $argv[1]; $f(); eval('echo 1;');"));
        assert!(!usage.introspects);
        assert!(usage.dynamic_function_call);
    }

    /// A computed call inside the fragment is introspection even when inline code already made
    /// one: the fragment is judged on its own calls, not on the flags it inherited.
    #[test]
    fn literal_eval_dynamic_call_after_an_inline_one_is_still_introspection() {
        let usage = collect(&parse(
            "<?php $f = 'strlen'; $f('x'); eval('$p = $argv[1]; echo $p($argv[2]) ? \"yes\" : \"no\";');",
        ));
        assert!(usage.introspects);
        assert!(usage.dynamic_function_call);
    }

    /// An array callable with an opaque receiver, `Closure::fromCallable` with a computed
    /// argument, and a runtime include expression are introspection inside a fragment, while
    /// the fully literal pair stays a plain reference.
    #[test]
    fn literal_eval_opaque_callables_and_include_values_are_introspection() {
        for source in [
            "<?php eval('class F {} $c = $argv[1]; $a = $c::fromArray([1]); echo count($a);');",
            "<?php eval('echo call_user_func([$argv[1], \"fromArray\"], [1]);');",
            "<?php eval('$c = Closure::fromCallable($argv[1]); echo $c();');",
            "<?php eval('$result = require \"part.php\"; echo $result;');",
        ] {
            let usage = collect(&parse(source));
            assert!(usage.introspects, "{source}");
        }
        let usage = collect(&parse("<?php eval('echo call_user_func([\"SplFixedArray\", \"fromArray\"], [1]);');"));
        assert!(!usage.introspects);
        assert!(usage.classes.contains("splfixedarray"));
        let usage = collect(&parse("<?php $o = new SplStack(); call_user_func([$o, 'push'], 1);"));
        assert!(!usage.introspects);
    }

    /// A function the fragment declares is recorded by name, so a builtin of that name stays
    /// registered for the interpreter to refuse the duplicate.
    #[test]
    fn literal_eval_declared_functions_are_recorded() {
        let usage = collect(&parse("<?php eval('function strlen(string $s): int { return 1; }');"));
        assert!(usage.references("strlen"));
        assert!(!usage.introspects);
    }

    /// A class, interface, trait or enum the fragment declares is recorded by name for the same
    /// reason, without turning the fragment into introspection.
    #[test]
    fn literal_eval_declared_class_likes_are_recorded() {
        let usage = collect(&parse(
            "<?php eval('class SplStack {} interface DateTimeInterface {} trait Fiber {} enum Generator {}');",
        ));
        for name in ["splstack", "datetimeinterface", "fiber", "generator"] {
            assert!(usage.classes.contains(name), "{name}");
        }
        assert!(!usage.introspects);
        let usage = collect(&parse("<?php eval('interface Child extends DateTimeInterface {}');"));
        assert!(usage.classes.contains("datetimeinterface"));
        assert!(!usage.introspects);
    }
}
