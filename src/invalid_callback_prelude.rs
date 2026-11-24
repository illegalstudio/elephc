//! Purpose:
//! Builds the one helper every unresolvable boxed callable reaches, so a callback that names no
//! invokable target raises php's catchable error instead of ending the program.
//!
//! Called from:
//! - `crate::backend_gap_prelude::inject_if_used`, which declares it for every program.
//! - `crate::codegen::lower_inst::callables`, which calls it from the open dispatchers' miss
//!   paths (the scope-aware dispatcher, its inline ladder, the string and array resolvers).
//!
//! Key details:
//! - The wording follows `php -n` 8.5: a callback validated by a builtin raises a `TypeError`
//!   `<prefix>, <reason>` (prefix e.g. `call_user_func(): Argument #1 ($callback) must be a
//!   valid callback`), a callable invoked directly (`$f()`) raises an `Error` with a capitalised
//!   reason of its own. An empty prefix selects the direct call.
//! - Visibility is not decided here: an unreachable method has its own diagnostic at the miss
//!   site, which knows the calling scope. Only the reasons that need no scope are worded.
//! - php names an abstract method as its own reason, which no runtime builtin reports, so the
//!   program's abstract and interface methods are listed in the helper when there are any.
//! - The helper is BUILT, never parsed: elephc does not parse PHP source outside tests.

use crate::parser::ast::{BinOp, Expr, Program, Stmt, StmtKind, TypeExpr};
use crate::synthetic_class::{
    e_array, e_binop, e_bool, e_call, e_concat_all, e_index, e_int, e_new_fq, e_not, e_null,
    e_str, e_ternary, e_var, function, php_semantics_declarations, s_assign, s_if, s_return,
    s_array_push, s_foreach, s_throw, t_mixed,
};

/// Reserved name of the helper, shared with the lowering that calls it.
pub(crate) const INVALID_CALLBACK_HELPER_NAME: &str = "__elephc_invalid_callback";

/// Reserved name of the helper that raises an internal function's argument-count error.
pub(crate) const ARITY_ERROR_HELPER_NAME: &str = "__elephc_internal_arity_error";

/// Builds `__elephc_internal_arity_error(string $name, int $min, int $max, int $given)`.
///
/// php refuses a wrong argument count for an internal function with
/// `strlen() expects exactly 1 argument, 0 given` (`at least` / `at most` when the function
/// accepts a range; `$max` is -1 for a variadic). The descriptor invokers of builtins call it.
fn arity_error_helper() -> Stmt {
    let exact = e_binop(e_var("max"), BinOp::StrictEq, e_var("min"));
    function(ARITY_ERROR_HELPER_NAME)
        .param("name", TypeExpr::Str)
        .param("min", TypeExpr::Int)
        .param("max", TypeExpr::Int)
        .param("given", TypeExpr::Int)
        .returns(t_mixed())
        .body(vec![
            s_if(
                e_binop(e_var("given"), BinOp::Lt, e_var("min")),
                vec![
                    s_assign("kind", e_ternary(exact.clone(), e_str("exactly"), e_str("at least"))),
                    s_assign("count", e_var("min")),
                ],
                vec![],
                Some(vec![
                    s_assign("kind", e_ternary(exact, e_str("exactly"), e_str("at most"))),
                    s_assign("count", e_var("max")),
                ]),
            ),
            s_throw(e_new_fq(
                "ArgumentCountError",
                vec![e_concat_all(vec![
                    e_var("name"),
                    e_str("() expects "),
                    e_var("kind"),
                    e_str(" "),
                    e_var("count"),
                    e_str(" argument"),
                    e_ternary(e_binop(e_var("count"), BinOp::StrictEq, e_int(1)), e_str(""), e_str("s")),
                    e_str(", "),
                    e_var("given"),
                    e_str(" given"),
                ])],
            )),
            s_return(e_null()),
        ])
        .build()
}

/// Returns the helper declaration for injection, listing the program's abstract methods.
///
/// `deep` asks the program's own classes about their methods (`method_exists`, `class_exists`,
/// `class_parents`). A name-keyed introspection builtin makes the closed world keep every class
/// and method alive (hundreds of functions: the Reflection classes among them), so the helper
/// only pays that when the program already calls such a builtin. Without it the reasons that
/// need no class tables stay exact, an unknown class is judged against the program's declared
/// names, and a missing method is reported as such.
pub(crate) fn declarations(program: &Program, deep: bool) -> Program {
    let abstract_methods = abstract_method_keys(program);
    let classes = declared_class_names(program);
    let tables = method_tables(program);
    php_semantics_declarations(|| {
        vec![helper(&abstract_methods, &classes, &tables, deep), arity_error_helper()]
    })
}

/// Most `class::method` entries the light helper embeds; a larger program answers every missing
/// method as "does not have a method" instead of growing the binary by an unbounded string.
const INSTANCE_TABLE_LIMIT: usize = 200_000;

/// One declaration the instance-method table is flattened from.
struct MethodOwner {
    parent: Option<String>,
    traits: Vec<String>,
    instance_methods: Vec<String>,
    /// Methods only the class (private) or its family (protected) can reach, with that word.
    restricted_methods: Vec<(String, &'static str)>,
}

/// The two compile-time method tables the light helper reads with `strpos`.
struct MethodTables {
    /// `|class::method|...` for every non-static method.
    instance: String,
    /// `|class::method:private|` and `|class::method:protected|` entries.
    restricted: String,
}

/// `|class::method|class::method|` for every non-static method a declared class has, its
/// ancestors' and its traits' included, all lower-cased. The light helper looks a callable's
/// method up in it with one `strpos`, which is how it tells "exists but is not static" (php's
/// non-static wording) from "does not exist" without asking the class tables by name. Empty when
/// there is nothing to say or the table would pass [`INSTANCE_TABLE_LIMIT`]. Trait adaptations
/// (`insteadof`, `as`) are not applied: an aliased trait method is judged by its own name.
///
/// The second table lists the methods php refuses with `cannot access private method C::m()`
/// (`Call to private method C::m() from global scope` when called directly): a private method
/// belongs to its own class only, a protected one is inherited.
fn method_tables(program: &Program) -> MethodTables {
    let mut owners: std::collections::BTreeMap<String, MethodOwner> = std::collections::BTreeMap::new();
    let mut classes: Vec<String> = Vec::new();
    crate::parser::ast::visit::walk_stmts(program, &mut |stmt| {
        let (name, extends, trait_uses, methods, is_class) = match &stmt.kind {
            StmtKind::ClassDecl { name, extends, trait_uses, methods, .. } => {
                (name, extends.as_ref().map(|parent| parent.as_str()), trait_uses.as_slice(), methods, true)
            }
            StmtKind::TraitDecl { name, trait_uses, methods, .. } => {
                (name, None, trait_uses.as_slice(), methods, false)
            }
            _ => return,
        };
        let key = name.trim_start_matches('\\').to_ascii_lowercase();
        if is_class {
            classes.push(key.clone());
        }
        owners.insert(key, MethodOwner {
            parent: extends.map(|parent| parent.trim_start_matches('\\').to_ascii_lowercase()),
            traits: trait_uses.iter()
                .flat_map(|used| used.trait_names.iter())
                .map(|used| used.as_str().trim_start_matches('\\').to_ascii_lowercase())
                .collect(),
            instance_methods: methods.iter()
                .filter(|method| !method.is_static && !method.is_abstract)
                .map(|method| method.name.to_ascii_lowercase())
                .collect(),
            restricted_methods: methods.iter()
                .filter_map(|method| match method.visibility {
                    crate::parser::ast::Visibility::Public => None,
                    crate::parser::ast::Visibility::Protected => {
                        Some((method.name.to_ascii_lowercase(), "protected"))
                    }
                    crate::parser::ast::Visibility::Private => {
                        Some((method.name.to_ascii_lowercase(), "private"))
                    }
                })
                .collect(),
        });
    });
    let mut instance = String::new();
    let mut restricted = String::new();
    let mut entries = 0usize;
    for class in &classes {
        let mut pending = vec![class.clone()];
        let mut seen = std::collections::BTreeSet::new();
        while let Some(owner) = pending.pop() {
            if !seen.insert(owner.clone()) {
                continue;
            }
            let Some(decl) = owners.get(&owner) else { continue };
            for method in &decl.instance_methods {
                entries += 1;
                if entries > INSTANCE_TABLE_LIMIT {
                    return MethodTables { instance: String::new(), restricted: String::new() };
                }
                instance.push_str(&format!("|{class}::{method}"));
            }
            for (method, word) in &decl.restricted_methods {
                // A private method is not part of a subclass; a trait's methods are the using
                // class's own.
                if *word == "private" && owner != *class && classes.contains(&owner) {
                    continue;
                }
                restricted.push_str(&format!("|{class}::{method}:{word}"));
            }
            pending.extend(decl.parent.iter().cloned());
            pending.extend(decl.traits.iter().cloned());
        }
    }
    if !instance.is_empty() {
        instance.push('|');
    }
    if !restricted.is_empty() {
        restricted.push('|');
    }
    MethodTables { instance, restricted }
}

/// Lower-cased names of the classes and interfaces the program declares.
fn declared_class_names(program: &Program) -> Vec<String> {
    let mut names = Vec::new();
    crate::parser::ast::visit::walk_stmts(program, &mut |stmt| {
        if let StmtKind::ClassDecl { name, .. } | StmtKind::InterfaceDecl { name, .. } = &stmt.kind {
            names.push(name.trim_start_matches('\\').to_ascii_lowercase());
        }
    });
    names.sort();
    names.dedup();
    names
}

/// `true` when `$first` (a class-name string) names no class: the introspection builtins when
/// `deep`, otherwise the program's declared names (everything is assumed to exist when the
/// program declares more classes than the list is worth).
fn class_is_unknown(deep: bool, classes: &[String]) -> Expr {
    if deep {
        return e_binop(
            e_not(e_call("class_exists", vec![e_var("first")])),
            BinOp::And,
            e_not(e_call("interface_exists", vec![e_var("first")])),
        );
    }
    if classes.is_empty() || classes.len() > 256 {
        return e_bool(classes.is_empty());
    }
    e_not(e_call(
        "in_array",
        vec![
            e_call("strtolower", vec![e_call("ltrim", vec![e_var("first"), e_str("\\")])]),
            e_array(classes.iter().map(|name| e_str(name)).collect()),
        ],
    ))
}

/// Lower-cased `class::method` keys of every abstract class method and interface method.
fn abstract_method_keys(program: &Program) -> Vec<String> {
    let mut keys = Vec::new();
    crate::parser::ast::visit::walk_stmts(program, &mut |stmt| {
        let (name, methods, interface) = match &stmt.kind {
            StmtKind::ClassDecl { name, methods, .. } => (name, methods, false),
            StmtKind::InterfaceDecl { name, methods, .. } => (name, methods, true),
            _ => return,
        };
        for method in methods {
            if method.is_abstract || (interface && !method.has_body) {
                keys.push(format!(
                    "{}::{}",
                    name.trim_start_matches('\\').to_ascii_lowercase(),
                    method.name.to_ascii_lowercase()
                ));
            }
        }
    });
    keys.sort();
    keys.dedup();
    keys
}

/// `$prefix === ""`: the callable was invoked directly rather than validated by a builtin.
fn is_direct() -> Expr {
    e_binop(
        e_binop(e_var("prefix"), BinOp::StrictEq, e_str("")),
        BinOp::Or,
        e_binop(
            e_call("substr", vec![e_var("prefix"), e_int(0), e_int(1)]),
            BinOp::StrictEq,
            e_str(SCOPE_MARKER),
        ),
    )
}

/// Marks a direct call's wording prefix that carries the caller's class: `"\u{1}Scope"`. The empty
/// prefix is a direct call from the global scope.
pub(crate) const SCOPE_MARKER: &str = "\u{1}";

/// Throws the direct-call `Error` or the callback-validation `TypeError` with the given reasons.
fn fail(direct: Expr, validation: Expr) -> Vec<Stmt> {
    vec![
        s_if(is_direct(), vec![s_throw(e_new_fq("Error", vec![direct]))], vec![], None),
        s_throw(e_new_fq(
            "TypeError",
            vec![e_concat_all(vec![e_var("prefix"), e_str(", "), validation])],
        )),
    ]
}

/// `"<lead>" . <middle> . "<tail>"` for the wordings that quote a runtime name.
fn quoted(lead: &str, middle: Expr, tail: &str) -> Expr {
    e_concat_all(vec![e_str(lead), middle, e_str(tail)])
}

/// Builds `__elephc_invalid_callback(mixed $callback, string $prefix): mixed`.
fn helper(abstract_methods: &[String], classes: &[String], tables: &MethodTables, deep: bool) -> Stmt {
    let instance_methods = tables.instance.as_str();
    let not_string_or_object = e_binop(
        e_not(e_call("is_string", vec![e_var("first")])),
        BinOp::And,
        e_not(e_call("is_object", vec![e_var("first")])),
    );
    let mut array_arm = vec![
        s_assign("callback", e_call("array_values", vec![e_var("callback")])),
        s_if(
            e_binop(e_call("count", vec![e_var("callback")]), BinOp::StrictNotEq, e_int(2)),
            fail(
                e_str("Array callback must have exactly two elements"),
                e_str("array callback must have exactly two members"),
            ),
            vec![],
            None,
        ),
        s_assign("first", e_index(e_var("callback"), e_int(0))),
        s_assign("method", e_index(e_var("callback"), e_int(1))),
        s_if(
            not_string_or_object,
            fail(
                e_str("First array member is not a valid class name or object"),
                e_str("first array member is not a valid class name or object"),
            ),
            vec![],
            None,
        ),
        s_if(
            e_not(e_call("is_string", vec![e_var("method")])),
            fail(
                e_str("Second array member is not a valid method"),
                e_str("second array member is not a valid method"),
            ),
            vec![],
            None,
        ),
        s_if(
            e_binop(
                e_call("is_string", vec![e_var("first")]),
                BinOp::And,
                class_is_unknown(deep, classes),
            ),
            fail(
                quoted("Class \"", e_var("first"), "\" not found"),
                quoted("class \"", e_var("first"), "\" not found"),
            ),
            vec![],
            None,
        ),
        s_assign(
            "class",
            e_ternary(
                e_call("is_object", vec![e_var("first")]),
                e_call("get_class", vec![e_var("first")]),
                e_var("first"),
            ),
        ),
    ];
    if !abstract_methods.is_empty() {
        let keys = e_array(abstract_methods.iter().map(|key| e_str(key)).collect());
        // The class and its ancestors, nearest first: the declaring class names the method.
        array_arm.push(s_assign("chain", e_array(vec![e_var("class")])));
        if deep {
            array_arm.push(s_foreach(
                e_call("class_parents", vec![e_var("class")]),
                None,
                "ancestor",
                vec![s_array_push("chain", e_var("ancestor"))],
            ));
        }
        array_arm.push(s_foreach(
            e_var("chain"),
            None,
            "declaring",
            vec![
                s_if(
                    e_call(
                        "in_array",
                        vec![
                            e_concat_all(vec![
                                e_call("strtolower", vec![e_var("declaring")]),
                                e_str("::"),
                                e_call("strtolower", vec![e_var("method")]),
                            ]),
                            keys,
                        ],
                    ),
                    fail(
                        e_concat_all(vec![
                            e_str("Cannot call abstract method "),
                            e_var("declaring"),
                            e_str("::"),
                            e_var("method"),
                            e_str("()"),
                        ]),
                        e_concat_all(vec![
                            e_str("cannot call abstract method "),
                            e_var("declaring"),
                            e_str("::"),
                            e_var("method"),
                            e_str("()"),
                        ]),
                    ),
                    vec![],
                    None,
                ),
            ],
        ));
    }
    // A method the caller's scope cannot reach: php words it as its own reason, naming the
    // visibility. A scope that could reach it never gets here, so the answer needs no scope but
    // the direct call's `from global scope` tail.
    if !tables.restricted.is_empty() {
        for word in ["private", "protected"] {
            let key = e_concat_all(vec![
                e_str("|"),
                e_call("strtolower", vec![e_var("class")]),
                e_str("::"),
                e_call("strtolower", vec![e_var("method")]),
                e_str(&format!(":{word}|")),
            ]);
            array_arm.push(s_if(
                e_binop(
                    e_call("strpos", vec![e_str(&tables.restricted), key]),
                    BinOp::StrictNotEq,
                    e_bool(false),
                ),
                fail(
                    e_concat_all(vec![
                        e_str(&format!("Call to {word} method ")),
                        e_var("class"),
                        e_str("::"),
                        e_var("method"),
                        e_str("() from "),
                        e_ternary(
                            e_binop(e_var("prefix"), BinOp::StrictEq, e_str("")),
                            e_str("global scope"),
                            e_concat_all(vec![
                                e_str("scope "),
                                e_call("substr", vec![e_var("prefix"), e_int(1)]),
                            ]),
                        ),
                    ]),
                    e_concat_all(vec![
                        e_str(&format!("cannot access {word} method ")),
                        e_var("class"),
                        e_str("::"),
                        e_var("method"),
                        e_str("()"),
                    ]),
                ),
                vec![],
                None,
            ));
        }
    }
    // A method the class does not have. Without the introspection builtins this is the answer for
    // every method that is not abstract, which is the common failure of a callable array.
    let method_missing = if deep {
        e_not(e_call("method_exists", vec![e_var("first"), e_var("method")]))
    } else if instance_methods.is_empty() {
        e_bool(true)
    } else {
        // The program's own non-static methods, flattened at compile time: a hit is a method that
        // exists and only lacks an object, which php words as its own reason below.
        e_binop(
            e_call("strpos", vec![
                e_str(instance_methods),
                e_concat_all(vec![
                    e_str("|"),
                    e_call("strtolower", vec![e_var("class")]),
                    e_str("::"),
                    e_call("strtolower", vec![e_var("method")]),
                    e_str("|"),
                ]),
            ]),
            BinOp::StrictEq,
            e_bool(false),
        )
    };
    array_arm.push(s_if(
        method_missing,
        fail(
            e_concat_all(vec![
                e_str("Call to undefined method "),
                e_var("class"),
                e_str("::"),
                e_var("method"),
                e_str("()"),
            ]),
            e_concat_all(vec![
                e_str("class "),
                e_var("class"),
                e_str(" does not have a method \""),
                e_var("method"),
                e_str("\""),
            ]),
        ),
        vec![],
        None,
    ));
    // Reachable only for a method that exists, is not abstract and is not reached from the
    // caller's scope as a static call: php words it as a non-static method called statically.
    array_arm.extend(fail(
        e_concat_all(vec![
            e_str("Non-static method "),
            e_var("class"),
            e_str("::"),
            e_var("method"),
            e_str("() cannot be called statically"),
        ]),
        e_concat_all(vec![
            e_str("non-static method "),
            e_var("class"),
            e_str("::"),
            e_var("method"),
            e_str("() cannot be called statically"),
        ]),
    ));

    let string_arm = fail(
        e_concat_all(vec![e_str("Call to undefined function "), e_var("callback"), e_str("()")]),
        quoted("function \"", e_var("callback"), "\" not found or invalid function name"),
    );
    let other_arm = fail(
        e_ternary(
            e_call("is_object", vec![e_var("callback")]),
            e_concat_all(vec![
                e_str("Object of type "),
                e_call("get_class", vec![e_var("callback")]),
                e_str(" is not callable"),
            ]),
            e_concat_all(vec![
                e_str("Value of type "),
                e_call("get_debug_type", vec![e_var("callback")]),
                e_str(" is not callable"),
            ]),
        ),
        e_str("no array or string given"),
    );
    let mut body = vec![s_if(
        e_binop(
            e_call("is_string", vec![e_var("callback")]),
            BinOp::And,
            e_binop(
                e_call("strpos", vec![e_var("callback"), e_str("::")]),
                BinOp::StrictNotEq,
                e_bool(false),
            ),
        ),
        vec![s_assign("callback", e_call("explode", vec![e_str("::"), e_var("callback"), e_int(2)]))],
        vec![],
        None,
    )];
    body.push(s_if(
        e_call("is_array", vec![e_var("callback")]),
        array_arm,
        vec![(e_call("is_string", vec![e_var("callback")]), string_arm)],
        Some(other_arm),
    ));
    body.push(s_return(e_null()));
    function(INVALID_CALLBACK_HELPER_NAME)
        .param_untyped("callback")
        .param("prefix", TypeExpr::Str)
        .returns(t_mixed())
        .body(body)
        .build()
}
