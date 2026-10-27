//! Purpose:
//! Erases generic methods that take part in dynamic dispatch, before any template is collected.
//!
//! Called from:
//! - `crate::generics::monomorphize`, once, after constructor lifting and before class templates
//!   are collected.
//!
//! Key details:
//! - An instantiated generic method is called statically and never takes a vtable slot, so a
//!   template on an interface or abstract method, on a method that implements or overrides an
//!   inherited one, or on a method a subclass overrides, cannot be monomorphized soundly.
//! - Erasure keeps the method and drops its type parameters plus every declared type naming one,
//!   which is how php-src reads the same PHPDoc. Trait methods are judged by their using classes.
//! - On a PHPDoc template (`TypeParam::from_docblock`), a dispatched method also drops the types
//!   naming the CLASS parameters, so an instantiation never narrows an inherited signature that
//!   php-src sees untyped. Constructors and native templates are untouched.

use std::collections::{BTreeSet, HashMap};

use crate::parser::ast::{ClassMethod, Program, Stmt, StmtKind, TypeExpr};

/// One class-like declaration as the erasure pass sees it: what it inherits and what it declares.
#[derive(Default)]
struct Owner {
    is_interface: bool,
    parent: Option<String>,
    interfaces: Vec<String>,
    traits: Vec<String>,
    methods: BTreeSet<String>,
}

/// Erases every generic method that takes part in dynamic dispatch.
///
/// An instantiated generic method is dispatched STATICALLY (see the module docs), so a template
/// is only sound where the call site's static receiver names the exact method that runs. That
/// fails as soon as the method is declared on an interface, is abstract, implements or
/// overrides an inherited method, or is overridden by a subclass: the call renamed to
/// `Base::pick<int>` would run `Base`'s body for a `Child`, and a class whose template was
/// stripped would no longer implement its interface. Such a method keeps its declaration and
/// loses its type parameters; every type naming one of them is dropped, which is php-src's view
/// of the same PHPDoc.
pub fn erase_dispatched(program: Program) -> Program {
    let mut owners: HashMap<String, Owner> = HashMap::new();
    index_owners(&program, &mut owners);
    let mut children: HashMap<String, Vec<String>> = HashMap::new();
    let mut users: HashMap<String, Vec<String>> = HashMap::new();
    for (key, owner) in &owners {
        for inherited in owner.parent.iter().chain(owner.interfaces.iter()) {
            children.entry(inherited.clone()).or_default().push(key.clone());
        }
        for used in &owner.traits {
            users.entry(used.clone()).or_default().push(key.clone());
        }
    }
    let mut dispatched: BTreeSet<(String, String)> = BTreeSet::new();
    for (key, owner) in &owners {
        let mut related = BTreeSet::new();
        let mut seen = BTreeSet::new();
        // A trait's method is dispatched wherever a class using it would dispatch it.
        let mut hosts = vec![key.clone()];
        hosts.extend(users_of(key, &users));
        for host in &hosts {
            ancestors_methods(host, &owners, &mut related, &mut seen, false);
            descendants_methods(host, &owners, &children, &mut related, &mut BTreeSet::new());
        }
        let everything = related.contains(ANY_METHOD);
        for method in &owner.methods {
            if owner.is_interface || everything || related.contains(method) {
                dispatched.insert((key.clone(), method.clone()));
            }
        }
    }
    let mut program = program;
    erase_in(&mut program, &dispatched);
    program
}

/// Records every class, interface, trait and enum with its inheritance edges and method names.
fn index_owners(stmts: &[Stmt], owners: &mut HashMap<String, Owner>) {
    use super::classes::template_key as key;
    let names = |list: &[crate::names::Name]| list.iter().map(|n| key(n.as_str())).collect();
    let trait_names = |uses: &[crate::parser::ast::TraitUse]| {
        uses.iter()
            .flat_map(|u| u.trait_names.iter().map(|n| key(n.as_str())))
            .collect()
    };
    let method_names = |methods: &[ClassMethod]| {
        methods.iter().map(|m| m.name.to_ascii_lowercase()).collect()
    };
    for stmt in stmts {
        let (name, owner) = match &stmt.kind {
            StmtKind::ClassDecl { name, extends, implements, trait_uses, methods, .. } => (
                name,
                Owner {
                    is_interface: false,
                    parent: extends.as_ref().map(|n| key(n.as_str())),
                    interfaces: names(implements),
                    traits: trait_names(trait_uses),
                    methods: method_names(methods),
                },
            ),
            StmtKind::InterfaceDecl { name, extends, methods, .. } => (
                name,
                Owner {
                    is_interface: true,
                    parent: None,
                    interfaces: names(extends),
                    traits: Vec::new(),
                    methods: method_names(methods),
                },
            ),
            StmtKind::TraitDecl { name, trait_uses, methods, .. } => (
                name,
                Owner {
                    traits: trait_names(trait_uses),
                    methods: method_names(methods),
                    ..Owner::default()
                },
            ),
            StmtKind::EnumDecl { name, implements, trait_uses, methods, .. } => (
                name,
                Owner {
                    interfaces: names(implements),
                    traits: trait_names(trait_uses),
                    methods: method_names(methods),
                    ..Owner::default()
                },
            ),
            StmtKind::NamespaceBlock { body, .. } | StmtKind::Synthetic(body) => {
                index_owners(body, owners);
                continue;
            }
            _ => continue,
        };
        owners.insert(key(name), owner);
    }
}

/// Every class that uses `trait_key`, directly or through another trait.
fn users_of(trait_key: &str, users: &HashMap<String, Vec<String>>) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![trait_key.to_string()];
    let mut seen = BTreeSet::new();
    while let Some(current) = stack.pop() {
        for user in users.get(&current).into_iter().flatten() {
            if seen.insert(user.clone()) {
                out.push(user.clone());
                // A trait that uses this trait has users of its own; a class has none, so
                // following every user is exact and terminates on `seen`.
                stack.push(user.clone());
            }
        }
    }
    out
}

/// Collects the methods every ancestor of `key` declares (its own only when `include_self`).
fn ancestors_methods(
    key: &str,
    owners: &HashMap<String, Owner>,
    out: &mut BTreeSet<String>,
    seen: &mut BTreeSet<String>,
    include_self: bool,
) {
    if !seen.insert(key.to_string()) {
        return;
    }
    let Some(owner) = owners.get(key) else {
        // Not declared by the program: a built-in. Its methods are reached by dynamic dispatch
        // as much as a user interface's are (`ArrayAccess::offsetGet` runs for `$bag["k"]`).
        match builtin_methods(key) {
            Some(methods) => out.extend(methods.iter().map(|m| m.to_string())),
            // A built-in this table does not describe (an SPL or exception class, or a name
            // from outside the program): every method may override one of its, so all are
            // treated as dispatched. Erasure is php-src's own reading, so this only costs
            // monomorphization.
            None => {
                out.insert(ANY_METHOD.to_string());
            }
        }
        return;
    };
    if include_self {
        flattened_methods(key, owners, out, &mut BTreeSet::new());
    }
    for inherited in owner.parent.iter().chain(owner.interfaces.iter()) {
        ancestors_methods(inherited, owners, out, seen, true);
    }
}

/// Stands for "every method" in a related-method set, when an ancestor is a built-in whose
/// methods are not known here. No PHP method name can contain `*`.
const ANY_METHOD: &str = "*";

/// The lower-cased methods of the built-in interfaces a user class commonly implements, by
/// template key, inherited interfaces included; `None` for any other undeclared name.
fn builtin_methods(key: &str) -> Option<&'static [&'static str]> {
    const ITERATOR: &[&str] = &["current", "key", "next", "rewind", "valid"];
    Some(match key {
        "traversable" => &[],
        "iterator" => ITERATOR,
        "iteratoraggregate" => &["getiterator"],
        "arrayaccess" => &["offsetexists", "offsetget", "offsetset", "offsetunset"],
        "countable" => &["count"],
        "outeriterator" => &["current", "key", "next", "rewind", "valid", "getinneriterator"],
        "recursiveiterator" => &["current", "key", "next", "rewind", "valid", "haschildren", "getchildren"],
        "seekableiterator" => &["current", "key", "next", "rewind", "valid", "seek"],
        "splobserver" => &["update"],
        "splsubject" => &["attach", "detach", "notify"],
        "stringable" => &["__tostring"],
        "jsonserializable" => &["jsonserialize"],
        "serializable" => &["serialize", "unserialize"],
        "unitenum" => &["cases"],
        "backedenum" => &["cases", "from", "tryfrom"],
        "throwable" => &[
            "getmessage", "getcode", "getfile", "getline", "gettrace", "getprevious",
            "gettraceasstring", "__tostring",
        ],
        _ => return None,
    })
}

/// Collects the methods every descendant of `key` declares, traits included.
fn descendants_methods(
    key: &str,
    owners: &HashMap<String, Owner>,
    children: &HashMap<String, Vec<String>>,
    out: &mut BTreeSet<String>,
    seen: &mut BTreeSet<String>,
) {
    for child in children.get(key).into_iter().flatten() {
        if seen.insert(child.clone()) {
            flattened_methods(child, owners, out, &mut BTreeSet::new());
            descendants_methods(child, owners, children, out, seen);
        }
    }
}

/// The methods `key` declares, plus those its traits contribute.
fn flattened_methods(
    key: &str,
    owners: &HashMap<String, Owner>,
    out: &mut BTreeSet<String>,
    seen: &mut BTreeSet<String>,
) {
    if !seen.insert(key.to_string()) {
        return;
    }
    let Some(owner) = owners.get(key) else { return };
    out.extend(owner.methods.iter().cloned());
    for used in &owner.traits {
        flattened_methods(used, owners, out, seen);
    }
}

/// Erases every dispatched generic method, and the PHPDoc class-parameter types of every
/// dispatched method of a PHPDoc template.
///
/// The second half is what keeps a PHPDoc template's members compatible with the methods they
/// implement or override: `@param TValue $value` on `Collection::contains` would otherwise be a
/// native `int $value` in `Collection<int, int>` and narrow `Enumerable::contains($value)`, a
/// signature php-src never sees. Native class parameters (`class Box<T>`) are left alone.
fn erase_in(stmts: &mut [Stmt], dispatched: &BTreeSet<(String, String)>) {
    for stmt in stmts.iter_mut() {
        match &mut stmt.kind {
            StmtKind::ClassDecl { name, methods, generics, .. }
            | StmtKind::InterfaceDecl { name, methods, generics, .. }
            | StmtKind::TraitDecl { name, methods, generics, .. }
            | StmtKind::EnumDecl { name, methods, generics, .. } => {
                let owner = super::classes::template_key(name);
                let doc_class_params: Vec<String> = generics
                    .iter()
                    .flat_map(|g| g.type_params.iter())
                    .filter(|param| param.from_docblock)
                    .map(|param| param.name.clone())
                    .collect();
                for method in methods.iter_mut() {
                    let key = (owner.clone(), method.name.to_ascii_lowercase());
                    let is_dispatched = dispatched.contains(&key)
                        && !method.name.eq_ignore_ascii_case("__construct");
                    if !method.type_params.is_empty()
                        && (method.is_abstract || !method.has_body || is_dispatched)
                    {
                        erase_method(method, &[]);
                    }
                    if is_dispatched && !doc_class_params.is_empty() {
                        erase_method(method, &doc_class_params);
                    }
                }
            }
            StmtKind::NamespaceBlock { body, .. } | StmtKind::Synthetic(body) => {
                erase_in(body, dispatched)
            }
            _ => {}
        }
    }
}

/// Drops a method's own type parameters, and reads every one of them, and every name in
/// `class_params`, as `mixed` in the declared types: `array<TKey, TValue>` keeps the `array` an
/// override is checked against, and a bare `T` (or `?T`, `T|mixed`) becomes untyped.
fn erase_method(method: &mut ClassMethod, class_params: &[String]) {
    let mut names: Vec<String> = method.type_params.iter().map(|p| p.name.clone()).collect();
    names.extend(class_params.iter().cloned());
    method.type_params.clear();
    let mixed = TypeExpr::Named(crate::names::Name::unqualified("mixed"));
    let bindings: Vec<(String, TypeExpr)> =
        names.iter().map(|name| (name.clone(), mixed.clone())).collect();
    let erase = |ty: &mut Option<TypeExpr>| {
        if let Some(declared) = ty.as_mut() {
            if declared.mentions_type_param(&names) {
                let erased = collapse_mixed(declared.substitute_type_params(&bindings));
                // A position that erases to plain `mixed` was only ever the PHPDoc's: leaving it
                // untyped is what php-src sees, and an untyped override stays compatible.
                *ty = (erased != mixed).then_some(erased);
            }
        }
    };
    for (_, declared, _, _) in method.params.iter_mut() {
        erase(declared);
    }
    erase(&mut method.variadic_type);
    erase(&mut method.return_type);
}

/// Collapses `?mixed` and any union with a `mixed` member to `mixed`, which php-src spells only
/// that way (`?mixed` is a compile error there), and an array whose element types erased to
/// `mixed` (`array<mixed, mixed>`, `mixed[]`) to the plain `array` php-src sees.
///
/// The plain keyword matters beyond spelling: `array<mixed, mixed>` is hash storage and `mixed[]`
/// packed storage, so either one gives the method a physical signature that differs from an
/// inherited `array` parameter, and the backend cannot adapt a dispatched call across that.
fn collapse_mixed(ty: TypeExpr) -> TypeExpr {
    let is_mixed = |t: &TypeExpr| matches!(t, TypeExpr::Named(n) if n.as_str().eq_ignore_ascii_case("mixed"));
    let plain_array = || TypeExpr::Named(crate::names::Name::unqualified("array"));
    match ty {
        TypeExpr::Nullable(inner) if is_mixed(&inner) => *inner,
        TypeExpr::Nullable(inner) => TypeExpr::Nullable(Box::new(collapse_mixed(*inner))),
        TypeExpr::Union(members) if members.iter().any(is_mixed) => {
            TypeExpr::Named(crate::names::Name::unqualified("mixed"))
        }
        TypeExpr::Union(members) => {
            TypeExpr::Union(members.into_iter().map(collapse_mixed).collect())
        }
        TypeExpr::AssocArray { key, value } if is_mixed(&key) && is_mixed(&value) => plain_array(),
        TypeExpr::Array(element) if is_mixed(&element) => plain_array(),
        other => other,
    }
}
