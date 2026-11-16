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
                        // The return type a class template took over comes back before the
                        // erasure, which would otherwise leave the method without one.
                        let native_return = generics
                            .iter()
                            .flat_map(|g| g.type_params.iter())
                            .filter(|param| param.from_docblock)
                            .filter_map(|param| param.native_hint.as_ref())
                            .flat_map(|hint| hint.member_returns.iter())
                            .find(|(name, _)| name.eq_ignore_ascii_case(&method.name))
                            .map(|(_, native)| native.clone());
                        if native_return.is_some() {
                            method.return_type = native_return;
                        }
                        // The parameter hints it took over cannot come back as types (see
                        // `erase_method`), so the check php-src makes on entry is compiled
                        // into the body instead.
                        let guards: Vec<Stmt> = generics
                            .iter()
                            .flat_map(|g| g.type_params.iter())
                            .filter(|param| param.from_docblock)
                            .filter_map(|param| param.native_hint.as_ref())
                            .flat_map(|hint| hint.member_params.iter())
                            .filter(|(member, _, _)| member.eq_ignore_ascii_case(&method.name))
                            .flat_map(|(_, param, native)| {
                                native_param_guard(name, method, param, native)
                            })
                            .collect();
                        if method.has_body {
                            method.body.splice(0..0, guards);
                        }
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

/// Builds the entry check php-src makes for a parameter whose loose native hint a class
/// template took over: `if (!is_object($v)) { throw new \TypeError(...); }`.
///
/// The erased parameter is untyped, so without this an argument the hint rejects (`put(5)` on
/// `put(object $v)`) would run. A `TypeError` thrown on entry is exactly what php-src does, and
/// unlike a restored hint it leaves the parameter's static type to the call, so the body still
/// stores it in a property of the template. `null` passes when the hint is nullable or the
/// parameter defaults to `null`. A variadic is checked element by element, and reported the
/// way php-src reports it: without the parameter name, a positional element under its own
/// argument number, and every named element under the number after the last positional one.
fn native_param_guard(
    class: &str,
    method: &ClassMethod,
    param: &str,
    native: &TypeExpr,
) -> Vec<Stmt> {
    use crate::synthetic_class::{
        e_binop, e_call, e_concat_all, e_instance_of, e_int, e_new_fq, e_not, e_null, e_post_inc,
        e_str, e_var, s_assign, s_expr, s_foreach, s_if,
    };
    // php-src names a fixed parameter in the message and leaves a variadic one unnamed.
    let named = if is_variadic_param(method, param) { String::new() } else { format!(" (${param})") };
    let fixed = method.params.iter().position(|(name, _, _, _)| name == param);
    if fixed.is_none() && !is_variadic_param(method, param) {
        return Vec::new();
    }
    let (hint, mut nullable) = match native {
        TypeExpr::Nullable(inner) => (inner.as_ref(), true),
        other => (other, false),
    };
    nullable |= fixed
        .and_then(|index| method.params[index].2.as_ref())
        .is_some_and(|default| matches!(default.kind, crate::parser::ast::ExprKind::Null));
    let predicate = match hint {
        TypeExpr::Array(_) | TypeExpr::AssocArray { .. } => "\\is_array",
        TypeExpr::Iterable => "\\is_iterable",
        TypeExpr::Named(name) => {
            match name.as_str().trim_start_matches('\\').to_ascii_lowercase().as_str() {
                "object" => "\\is_object",
                "array" => "\\is_array",
                "iterable" => "\\is_iterable",
                "callable" => "\\is_callable",
                "closure" => "",
                _ => return Vec::new(),
            }
        }
        _ => return Vec::new(),
    };
    // php-src spells `iterable` as the union it stands for, with `null` as a member of it.
    let expected = if predicate == "\\is_iterable" {
        let null = if matches!(native, TypeExpr::Nullable(_)) { "|null" } else { "" };
        format!("Traversable|array{null}")
    } else {
        super::describe_type(native)
    };
    // `if (<value> is rejected) { throw new \TypeError("...Argument #<position> ..."); }`
    let check = |value: &str, position: crate::parser::ast::Expr| {
        let accepted = if predicate.is_empty() {
            e_instance_of(e_var(value), "\\Closure")
        } else {
            e_call(predicate, vec![e_var(value)])
        };
        let mut rejected = e_not(accepted);
        if nullable {
            rejected = e_binop(
                e_binop(e_var(value), crate::parser::ast::BinOp::StrictNotEq, e_null()),
                crate::parser::ast::BinOp::And,
                rejected,
            );
        }
        let message = e_concat_all(vec![
            e_str(&format!("{}::{}(): Argument #", class, method.name)),
            position,
            e_str(&format!("{} must be of type {}, ", named, expected)),
            e_call("\\gettype", vec![e_var(value)]),
            e_str(" given"),
        ]);
        // A distinct span: the throw's span is what the compiler keys the exception class by.
        let throw = Stmt::new(
            StmtKind::Throw(e_new_fq("TypeError", vec![message])),
            crate::span::Span::synthetic(),
        );
        s_if(rejected, vec![throw], Vec::new(), None)
    };
    match fixed {
        Some(index) => vec![check(param, e_str(&(index + 1).to_string()))],
        None => {
            // Generated names carry a `#`, which no PHP source can spell, so they cannot take
            // over a parameter or a local of the method, and stay out of `get_defined_vars()`.
            let local = |role: &str| {
                crate::names::generated_local_name(&format!("__elephc_guard_{param}_{role}"))
            };
            let (position, key, element) = (local("position"), local("key"), local("element"));
            // The number advances past positional elements only: named ones come last and all
            // share the number that follows them.
            let advance = s_if(
                e_call("\\is_int", vec![e_var(&key)]),
                vec![s_expr(e_post_inc(&position))],
                Vec::new(),
                None,
            );
            vec![
                s_assign(&position, e_int(method.params.len() as i64 + 1)),
                s_foreach(
                    e_var(param),
                    Some(&key),
                    &element,
                    vec![check(&element, e_var(&position)), advance],
                ),
            ]
        }
    }
}

/// Returns whether `param` is the method's variadic parameter.
fn is_variadic_param(method: &ClassMethod, param: &str) -> bool {
    method.variadic.as_deref() == Some(param)
}

/// Drops a method's own type parameters, and reads every one of them, and every name in
/// `class_params`, as `mixed` in the declared types: `array<TKey, TValue>` keeps the `array` an
/// override is checked against, and a bare `T` (or `?T`, `T|mixed`) becomes untyped.
///
/// A position whose METHOD template took over a native hint (`TypeParam::native_hint`) gets
/// that hint back first, so the erased method declares what php-src enforces: `@param T $m` on
/// `Model $m` is `Model $m` again, and `@return T` on `: Model` is `: Model`. A parameter the
/// same template annotates without a native hint stays untyped.
///
/// A CLASS template's parameters are not restored. The instantiation keeps the template's
/// properties at the type argument (`Bin<User>::$value` is a `User`), and a body that stores an
/// erased parameter there (`$this->value = $v`) only checks while that parameter is untyped: a
/// restored `object $v` is not a `User`, and php-src runs the method. `erase_in` compiles the
/// native hint into the body as an entry check instead (`native_param_guard`), and restores the
/// return type (a `User` is a legal `object` return), which is what keeps the method
/// compatible with the interface or parent method it implements.
fn erase_method(method: &mut ClassMethod, class_params: &[String]) {
    for hint in method.type_params.iter().filter_map(|param| param.native_hint.as_ref()) {
        for (name, native) in &hint.params {
            if let Some((_, declared, _, _)) =
                method.params.iter_mut().find(|(param, _, _, _)| param == name)
            {
                *declared = Some(native.clone());
            } else if method.variadic.as_ref() == Some(name) {
                method.variadic_type = Some(native.clone());
            }
        }
        if let Some(native) = &hint.return_type {
            method.return_type = Some(native.clone());
        }
    }
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
/// that way (`?mixed` is a compile error there), and an array one of whose element types erased
/// to `mixed` (`array<mixed, mixed>`, `array<int, mixed>`, `mixed[]`) to the plain `array`
/// php-src sees. Nested components collapse first.
///
/// The plain keyword matters beyond spelling: `array<int, mixed>` is hash storage and `mixed[]`
/// a packed vector of boxed cells, and neither is the representation a native `array` parameter
/// or return has. A signature left at one of them does not match the interface or parent method
/// it implements, and a body that returns its plain `array` parameter through it reads the
/// wrong layout.
pub(crate) fn collapse_mixed(ty: TypeExpr) -> TypeExpr {
    let is_mixed = |t: &TypeExpr| super::is_mixed_type(t);
    let plain_array = || TypeExpr::Named(crate::names::Name::unqualified("array"));
    match ty {
        TypeExpr::Nullable(inner) => {
            let inner = collapse_mixed(*inner);
            if is_mixed(&inner) {
                inner
            } else {
                TypeExpr::Nullable(Box::new(inner))
            }
        }
        TypeExpr::Union(members) => {
            let members: Vec<TypeExpr> = members.into_iter().map(collapse_mixed).collect();
            if members.iter().any(is_mixed) {
                super::mixed_type()
            } else {
                TypeExpr::Union(members)
            }
        }
        TypeExpr::AssocArray { key, value } => {
            let key = collapse_mixed(*key);
            let value = collapse_mixed(*value);
            if is_mixed(&key) || is_mixed(&value) {
                plain_array()
            } else {
                TypeExpr::AssocArray { key: Box::new(key), value: Box::new(value) }
            }
        }
        TypeExpr::Array(element) => {
            let element = collapse_mixed(*element);
            if is_mixed(&element) {
                plain_array()
            } else {
                TypeExpr::Array(Box::new(element))
            }
        }
        other => other,
    }
}
