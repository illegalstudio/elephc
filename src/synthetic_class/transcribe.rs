//! Purpose:
//! Emits the `synthetic_class` builder calls that reconstruct a parsed PHP program. It is the
//! migration tool for converting a PHP-text prelude into Rust: parse the text once, print the
//! builders, paste them in.
//!
//! Called from:
//! - `synthetic_class::transcribe::tests`, which is how a conversion is actually run.
//!
//! Key details:
//! - THIS IS A DEVELOPMENT TOOL, not part of any compile. It exists because transcribing
//!   thousands of lines of PHP into builder calls by hand is mechanical work with a high
//!   error rate, and every mistake is a silent semantic change. Deriving the calls from the
//!   parse makes structural fidelity automatic: the emitted code is the AST, so it cannot
//!   drift from what the parser produced.
//! - It PANICS on any node it cannot express. A silent skip would emit builder code that
//!   compiles and quietly drops a statement, which is the one failure mode a migration tool
//!   must not have. A panic names the node so the missing helper can be added.
//! - It does NOT reproduce PHP comments — the parser discards them. A converted prelude keeps
//!   its explanatory comments only if they are re-attached by hand, which is why the output is
//!   a starting point for review rather than a finished file.
//! - Two-word `else if` needs no special handling here: it reaches the AST as an `If` nested
//!   in the `else` body, so emitting the tree as-is reproduces it exactly.

use crate::parser::ast::{
    AttributeGroup, BinOp, CType, CastType, ClassMethod, Expr, ExprKind, Program, StaticReceiver,
    Stmt, StmtKind, TypeExpr, Visibility,
};

/// Emits builder calls for every declaration in `program`, one per line-group.
pub fn transcribe(program: &Program) -> String {
    let mut out = String::new();
    for stmt in program {
        out.push_str(&decl(stmt, 3));
        out.push_str(",\n");
    }
    out
}

/// Emits ONE FUNCTION PER DECLARATION plus an aggregator that calls them in order.
///
/// A large prelude cannot be one expression. Building six thousand lines of PHP as a single
/// nested builder expression OVERFLOWS THE STACK: every intermediate `Expr`/`Stmt` is a
/// temporary in one frame, and `image_prelude` aborts the process before it returns. Splitting
/// per declaration keeps each frame small, and it reads better besides — the aggregator is a
/// table of contents.
pub fn transcribe_split(program: &Program, aggregator: &str) -> String {
    transcribe_split_with_wrapper(program, aggregator, true)
}

/// Emits split builder functions without marking the resulting declarations as internal source.
pub fn transcribe_split_plain(program: &Program, aggregator: &str) -> String {
    transcribe_split_with_wrapper(program, aggregator, false)
}

/// Implements split transcription with an optional internal-declaration wrapper.
fn transcribe_split_with_wrapper(
    program: &Program,
    aggregator: &str,
    internal: bool,
) -> String {
    let mut helpers = String::new();
    let mut calls = String::new();
    let mut used: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

    // A prelude's top level is not necessarily all declarations: the web prelude's bootstrap
    // (superglobal population, session auto-start) is executable, and it must keep its POSITION
    // relative to the declarations around it. Such a statement gets a positional helper name.
    let mut executable = 0;
    for stmt in program {
        let (kind, raw_name) = match &stmt.kind {
            StmtKind::FunctionDecl { name, .. } => ("fn", name.clone()),
            StmtKind::ClassDecl { name, .. } => ("class", name.clone()),
            StmtKind::ExternFunctionDecl { name, .. } => ("extern", name.clone()),
            StmtKind::ConstDecl { name, .. } => ("const", name.clone()),
            _ => {
                executable += 1;
                ("stmt", format!("bootstrap {executable}"))
            }
        };
        let mut base = format!("decl_{}_{}", kind, sanitize(&raw_name));
        let seen = used.entry(base.clone()).or_insert(0);
        *seen += 1;
        if *seen > 1 {
            base = format!("{base}_{}", *seen);
        }

        let (method_helpers, declaration) = split_declaration_methods(stmt, &base, 1);
        helpers.push_str(&method_helpers);
        helpers.push_str(&format!(
            "/// `{}` — transcribed from the PHP form.\nfn {}() -> Stmt {{\n{}\n}}\n\n",
            raw_name,
            base,
            declaration
        ));
        calls.push_str(&format!("            {base}(),\n"));
    }

    let body = if internal {
        format!("    internal_declarations(|| {{\n        vec![\n{calls}        ]\n    }})")
    } else {
        format!("    vec![\n{calls}    ]")
    };
    format!(
        "{helpers}/// Builds the whole surface, one declaration per helper above.\n\
         pub(crate) fn {aggregator}() -> Program {{\n{body}\n}}\n"
    )
}

/// Splits class/interface methods into dedicated builders before emitting their owner.
///
/// One generated DateTime class used to reserve more than five MiB in a single debug-build
/// frame because every method body was nested in the class builder expression. Returning each
/// `MethodBuilder` from its own helper bounds the largest frame to one method while preserving
/// the exact direct-AST declaration assembled by the owner helper.
fn split_declaration_methods(stmt: &Stmt, base: &str, depth: usize) -> (String, String) {
    let (owner_name, methods, methods_have_bodies) = match &stmt.kind {
        StmtKind::ClassDecl { name, methods, .. } => (name.as_str(), methods.as_slice(), true),
        StmtKind::InterfaceDecl { name, methods, .. } => {
            (name.as_str(), methods.as_slice(), false)
        }
        _ => return (String::new(), decl(stmt, depth)),
    };
    if methods.is_empty() {
        return (String::new(), decl(stmt, depth));
    }

    let mut stripped = stmt.clone();
    match &mut stripped.kind {
        StmtKind::ClassDecl { methods, .. } | StmtKind::InterfaceDecl { methods, .. } => {
            methods.clear();
        }
        _ => unreachable!("the owner kind was matched above"),
    }

    let mut helpers = String::new();
    let mut calls = String::new();
    for (index, method_decl) in methods.iter().enumerate() {
        let helper = format!(
            "{base}_method_{index}_{}",
            sanitize(&method_decl.name)
        );
        helpers.push_str(&format!(
            "/// `{}::{}` — transcribed method builder.\nfn {}() -> MethodBuilder {{\n{}\n}}\n\n",
            owner_name,
            method_decl.name,
            helper,
            method_builder(method_decl, 1, methods_have_bodies),
        ));
        calls.push_str(&format!("\n{}.method({helper}())", pad(depth + 1)));
    }

    let mut declaration = decl(&stripped, depth);
    let build_suffix = format!("\n{}.build()", pad(depth + 1));
    assert!(
        declaration.ends_with(&build_suffix),
        "split declaration for {owner_name} has no terminal build()"
    );
    declaration.truncate(declaration.len() - build_suffix.len());
    declaration.push_str(&calls);
    declaration.push_str(&build_suffix);
    (helpers, declaration)
}

/// Turns a PHP name into a Rust identifier fragment.
///
/// Runs of non-alphanumerics collapse to ONE underscore and leading ones are dropped: PHP's
/// `__elephc_img_output` would otherwise become `decl_fn__elephc_img_output`, which trips the
/// `non_snake_case` lint on the doubled underscore.
fn sanitize(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if !out.is_empty() && !out.ends_with('_') {
            out.push('_');
        }
    }
    out.trim_end_matches('_').to_string()
}

/// Indentation helper.
fn pad(depth: usize) -> String {
    "    ".repeat(depth)
}

/// A Rust string literal for `value`, with Rust escaping.
fn lit(value: &str) -> String {
    format!("{:?}", value)
}

/// Emits one top-level declaration.
fn decl(stmt: &Stmt, depth: usize) -> String {
    match &stmt.kind {
        StmtKind::FunctionDecl {
            name,
            params,
            param_attributes,
            variadic,
            variadic_by_ref,
            variadic_type,
            return_type,
            by_ref_return,
            body,
        } => {
            assert!(!variadic_by_ref, "a by-ref variadic is not modelled: {name}");
            assert!(!by_ref_return, "a by-ref return is not modelled: {name}");
            let mut out = format!("{}function({})", pad(depth), lit(name));
            out.push_str(&params_calls(params, param_attributes, depth + 1));
            if let Some(tail) = variadic {
                let element = match variadic_type {
                    Some(ty) => format!("Some({})", ty_expr(ty)),
                    None => "None".to_string(),
                };
                out.push_str(&format!(
                    "\n{}.variadic({}, {})",
                    pad(depth + 1),
                    lit(tail),
                    element
                ));
            }
            if let Some(ty) = return_type {
                out.push_str(&format!("\n{}.returns({})", pad(depth + 1), ty_expr(ty)));
            }
            out.push_str(&body_call(body, depth + 1));
            out.push_str(&format!("\n{}.build()", pad(depth + 1)));
            out
        }
        StmtKind::ClassDecl {
            name,
            extends,
            implements,
            is_final,
            properties,
            methods,
            constants,
            trait_uses,
            is_abstract,
            is_readonly_class,
        } => {
            assert!(trait_uses.is_empty(), "trait uses are not modelled");
            assert!(!is_abstract, "abstract classes are not modelled");
            assert!(!is_readonly_class, "readonly classes are not modelled");
            let mut out = format!("{}class({})", pad(depth), lit(name));
            if *is_final {
                out.push_str(&format!("\n{}.final_()", pad(depth + 1)));
            }
            if let Some(parent) = extends {
                out.push_str(&format!(
                    "\n{}.extends({})",
                    pad(depth + 1),
                    lit(&name_source(parent))
                ));
            }
            for interface in implements {
                out.push_str(&format!(
                    "\n{}.implements({})",
                    pad(depth + 1),
                    lit(&name_source(interface))
                ));
            }
            for constant in constants {
                assert_eq!(
                    constant.visibility,
                    Visibility::Public,
                    "only public class constants are modelled"
                );
                assert!(!constant.is_final, "final class constants are not modelled");
                // A constant's ATTRIBUTES were being dropped here without a word — the same
                // shape that swallowed parameter attributes. `#[\Deprecated("…")]` on
                // `PDO::TRANSACTION_*` is what makes reference PHP warn about it, so losing it
                // silently changes what a program is told.
                if constant.type_expr.is_none() && constant.attributes.is_empty() {
                    out.push_str(&format!(
                        "\n{}.constant({}, {})",
                        pad(depth + 1),
                        lit(&constant.name),
                        expr(&constant.value, depth + 2)
                    ));
                } else {
                    let type_expr = constant
                        .type_expr
                        .as_ref()
                        .map(|ty| format!("Some({})", ty_expr(ty)))
                        .unwrap_or_else(|| "None".to_string());
                    out.push_str(&format!(
                        "\n{}.constant_full({}, {}, {}, vec![{}])",
                        pad(depth + 1),
                        lit(&constant.name),
                        expr(&constant.value, depth + 2),
                        type_expr,
                        attribute_groups(&constant.attributes, depth + 2)
                    ));
                }
            }
            for property in properties {
                assert!(
                    property.set_visibility.is_none(),
                    "asymmetric property visibility is not modelled: {}",
                    property.name
                );
                assert!(
                    !property.is_final,
                    "final properties are not modelled: {}",
                    property.name
                );
                assert!(
                    !property.is_abstract,
                    "abstract properties are not modelled: {}",
                    property.name
                );
                assert!(
                    !property.by_ref,
                    "by-ref properties are not modelled: {}",
                    property.name
                );
                assert!(
                    !property.is_promoted,
                    "promoted properties are not modelled: {}",
                    property.name
                );
                assert!(
                    property.attributes.is_empty(),
                    "property attributes are not modelled: {}",
                    property.name
                );
                let default = match &property.default {
                    Some(value) => format!("Some({})", expr(value, depth + 2)),
                    None => "None".to_string(),
                };
                let virtual_get_only = property.hooks.get
                    && !property.hooks.set
                    && !property.hooks.get_by_ref;
                let call = match (&property.visibility, &property.type_expr) {
                    (Visibility::Public, Some(ty))
                        if virtual_get_only
                            && !property.readonly
                            && !property.is_static
                            && property.default.is_none() =>
                    {
                        format!(".virtual_get_prop({}, {})", lit(&property.name), ty_expr(ty))
                    }
                    (Visibility::Public, Some(ty)) if property.readonly => format!(
                        ".readonly_prop({}, {})",
                        lit(&property.name),
                        ty_expr(ty)
                    ),
                    (Visibility::Private, Some(ty)) if property.is_static => format!(
                        ".private_static_prop({}, {}, {})",
                        lit(&property.name),
                        ty_expr(ty),
                        default
                    ),
                    (Visibility::Private, None) if property.is_static => format!(
                        ".private_static_untyped_prop({}, {})",
                        lit(&property.name),
                        default
                    ),
                    (Visibility::Protected, Some(ty)) if property.is_static => format!(
                        ".protected_static_prop({}, {}, {})",
                        lit(&property.name),
                        ty_expr(ty),
                        default
                    ),
                    (Visibility::Public, Some(ty)) if property.is_static => format!(
                        ".static_prop({}, {}, {})",
                        lit(&property.name),
                        ty_expr(ty),
                        default
                    ),
                    (Visibility::Public, None) if property.is_static => {
                        format!(".static_untyped_prop({}, {})", lit(&property.name), default)
                    }
                    (Visibility::Public, Some(ty)) => {
                        format!(".prop({}, {}, {})", lit(&property.name), ty_expr(ty), default)
                    }
                    (Visibility::Public, None) => {
                        format!(".untyped_prop({}, {})", lit(&property.name), default)
                    }
                    (Visibility::Private, Some(ty)) => format!(
                        ".private_prop({}, {}, {})",
                        lit(&property.name),
                        ty_expr(ty),
                        default
                    ),
                    (Visibility::Private, None) => {
                        format!(".private_untyped_prop({}, {})", lit(&property.name), default)
                    }
                    (Visibility::Protected, Some(ty)) => format!(
                        ".protected_prop({}, {}, {})",
                        lit(&property.name),
                        ty_expr(ty),
                        default
                    ),
                    (Visibility::Protected, None) => {
                        format!(".protected_untyped_prop({}, {})", lit(&property.name), default)
                    }
                };
                assert!(
                    virtual_get_only || property.hooks == crate::parser::ast::PropertyHooks::none(),
                    "unsupported property hooks on {}",
                    property.name
                );
                out.push_str(&format!("\n{}{}", pad(depth + 1), call));
            }
            for class_method in methods {
                let m = method_builder(class_method, depth + 3, true);
                out.push_str(&format!(
                    "\n{}.method(\n{}{},\n{})",
                    pad(depth + 1),
                    pad(depth + 2),
                    m,
                    pad(depth + 1)
                ));
            }
            out.push_str(&format!("\n{}.build()", pad(depth + 1)));
            out
        }
        StmtKind::ExternFunctionDecl {
            name,
            params,
            return_type,
            library,
        } => {
            let library = library
                .as_ref()
                .expect("an extern without a library is not modelled");
            let mut out = format!(
                "{}extern_fn({}, {})",
                pad(depth),
                lit(name),
                lit(library)
            );
            for param in params {
                out.push_str(&format!(
                    "\n{}.param({}, {})",
                    pad(depth + 1),
                    lit(&param.name),
                    c_type(&param.c_type)
                ));
            }
            out.push_str(&format!(
                "\n{}.returns({})\n{}.build()",
                pad(depth + 1),
                c_type(return_type),
                pad(depth + 1)
            ));
            out
        }
        StmtKind::InterfaceDecl {
            name,
            extends,
            properties,
            methods,
            constants,
        } => {
            assert!(
                properties.is_empty(),
                "interface properties are not modelled"
            );
            let mut out = format!("{}interface({})", pad(depth), lit(name));
            for parent in extends {
                out.push_str(&format!(
                    "\n{}.extends({})",
                    pad(depth + 1),
                    lit(&name_source(parent))
                ));
            }
            for constant in constants {
                assert_eq!(
                    constant.visibility,
                    Visibility::Public,
                    "only public interface constants are modelled"
                );
                assert!(!constant.is_final, "final interface constants are not modelled");
                if constant.type_expr.is_none() && constant.attributes.is_empty() {
                    out.push_str(&format!(
                        "\n{}.constant({}, {})",
                        pad(depth + 1),
                        lit(&constant.name),
                        expr(&constant.value, depth + 2)
                    ));
                } else {
                    let type_expr = constant
                        .type_expr
                        .as_ref()
                        .map(|ty| format!("Some({})", ty_expr(ty)))
                        .unwrap_or_else(|| "None".to_string());
                    out.push_str(&format!(
                        "\n{}.constant_full({}, {}, {}, vec![{}])",
                        pad(depth + 1),
                        lit(&constant.name),
                        expr(&constant.value, depth + 2),
                        type_expr,
                        attribute_groups(&constant.attributes, depth + 2)
                    ));
                }
            }
            for signature in methods {
                let m = method_builder(signature, depth + 3, false);
                out.push_str(&format!(
                    "\n{}.method(\n{}{},\n{})",
                    pad(depth + 1),
                    pad(depth + 2),
                    m,
                    pad(depth + 1)
                ));
            }
            out.push_str(&format!("\n{}.build()", pad(depth + 1)));
            out
        }
        StmtKind::ConstDecl { name, value } => format!(
            "{}s_const({}, {})",
            pad(depth),
            lit(name),
            expr(value, depth + 1)
        ),
        // A braced namespace holds DECLARATIONS, so its body recurses through `decl` rather
        // than the statement emitter — a class inside `namespace Pdo { }` is still a class.
        StmtKind::NamespaceBlock { name, body } => {
            let name = name
                .as_ref()
                .expect("an unnamed namespace block is not modelled");
            let inner = body
                .iter()
                .map(|stmt| decl(stmt, depth + 1))
                .collect::<Vec<_>>()
                .join(",\n");
            format!(
                "{}s_namespace({}, vec![\n{}\n{}])",
                pad(depth),
                lit(name.as_str()),
                inner,
                pad(depth)
            )
        }
        // An executable top-level statement — a prelude's bootstrap. It is a statement like any
        // other, so it goes through the ordinary statement emitter.
        _ => format!("{}{}", pad(depth), statement(stmt, depth)),
    }
}

/// Emits one method/signature builder expression for a split or inline owner declaration.
fn method_builder(method_decl: &ClassMethod, depth: usize, has_body: bool) -> String {
    assert_eq!(
        method_decl.has_body, has_body,
        "method body shape is not modelled: {}",
        method_decl.name
    );
    if !has_body {
        assert_eq!(
            method_decl.visibility,
            Visibility::Public,
            "only public interface methods are modelled"
        );
        assert!(!method_decl.is_final, "final interface methods are not modelled");
        assert!(
            method_decl.body.is_empty(),
            "an interface method is a signature: {}",
            method_decl.name
        );
    }
    assert!(
        !method_decl.variadic_by_ref,
        "a by-ref variadic is not modelled: {}",
        method_decl.name
    );
    // This helper reads fields off the struct by name rather than destructuring it, so an
    // unmodelled one could otherwise be dropped silently.
    assert!(
        !method_decl.by_ref_return,
        "a by-ref return is not modelled: {}",
        method_decl.name
    );

    let mut out = format!("method({})", lit(&method_decl.name));
    match method_decl.visibility {
        Visibility::Public => {}
        Visibility::Private => out.push_str(&format!("\n{}.private()", pad(depth))),
        Visibility::Protected => out.push_str(&format!("\n{}.protected()", pad(depth))),
    }
    if method_decl.is_static {
        out.push_str(&format!("\n{}.static_()", pad(depth)));
    }
    if method_decl.is_final {
        out.push_str(&format!("\n{}.final_()", pad(depth)));
    }
    for group in &method_decl.attributes {
        for attribute in &group.attributes {
            out.push_str(&format!(
                "\n{}.attr({}, {})",
                pad(depth),
                lit(&name_source(&attribute.name)),
                expr_vec(&attribute.args, depth),
            ));
        }
    }
    out.push_str(&params_calls(
        &method_decl.params,
        &method_decl.param_attributes,
        depth,
    ));
    if let Some(tail) = &method_decl.variadic {
        let element = match &method_decl.variadic_type {
            Some(ty) => format!("Some({})", ty_expr(ty)),
            None => "None".to_string(),
        };
        out.push_str(&format!(
            "\n{}.variadic({}, {})",
            pad(depth),
            lit(tail),
            element
        ));
    }
    if let Some(ty) = &method_decl.return_type {
        out.push_str(&format!("\n{}.returns({})", pad(depth), ty_expr(ty)));
    }
    if has_body {
        out.push_str(&body_call(&method_decl.body, depth));
    }
    out
}

/// Emits `attr("Name", vec![args…])` calls for one declaration's attribute groups.
fn attribute_groups(groups: &[AttributeGroup], depth: usize) -> String {
    groups
        .iter()
        .flat_map(|group| group.attributes.iter())
        .map(|attribute| {
            format!(
                "attr({}, {})",
                lit(&name_source(&attribute.name)),
                expr_vec(&attribute.args, depth)
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// A name AS WRITTEN, keeping the leading separator that marks it root-anchored.
///
/// `as_canonical()` renders the same text for `\Foo` and `Foo`, which are different nodes —
/// so a transcription that goes through it emits code that rebuilds the wrong one.
fn name_source(name: &crate::names::Name) -> String {
    match name.kind {
        crate::names::NameKind::FullyQualified => format!("\\{}", name.as_canonical()),
        _ => name.as_canonical(),
    }
}

/// Emits the `.param*` calls for a parameter list.
fn params_calls(
    params: &[(String, Option<TypeExpr>, Option<Expr>, bool)],
    param_attributes: &[Vec<AttributeGroup>],
    depth: usize,
) -> String {
    let mut out = String::new();
    for (index, (name, ty, default, by_ref)) in params.iter().enumerate() {
        // Emitted AFTER the parameter call below, because `param_attr` attaches to whichever
        // parameter was added last. Rendered here so both the by-ref and the plain branch get
        // it without repeating the loop tail.
        let attrs = param_attributes
            .get(index)
            .map(|groups| {
                groups
                    .iter()
                    .flat_map(|group| group.attributes.iter())
                    .map(|attribute| {
                        assert!(
                            attribute.args.is_empty(),
                            "a parameter attribute with arguments is not modelled: {}",
                            attribute.name.as_str()
                        );
                        // The SOURCE spelling, not the canonical text: `as_str()` drops the
                        // leading separator, and `\SensitiveParameter` reparses as an
                        // unqualified name once it is gone — a different node from the one
                        // that was written.
                        format!(
                            "\n{}.param_attr({})",
                            pad(depth),
                            lit(&name_source(&attribute.name))
                        )
                    })
                    .collect::<String>()
            })
            .unwrap_or_default();
        if *by_ref {
            let rendered_ty = match ty {
                Some(ty) => format!("Some({})", ty_expr(ty)),
                None => "None".to_string(),
            };
            let call = match default {
                None => format!(".param_by_ref({}, {})", lit(name), rendered_ty),
                Some(value) => format!(
                    ".param_by_ref_default({}, {}, {})",
                    lit(name),
                    rendered_ty,
                    expr(value, depth + 1)
                ),
            };
            out.push_str(&format!("\n{}{}{}", pad(depth), call, attrs));
            continue;
        }
        let call = match (ty, default) {
            (Some(ty), None) => format!(".param({}, {})", lit(name), ty_expr(ty)),
            (Some(ty), Some(value)) => format!(
                ".param_default({}, {}, {})",
                lit(name),
                ty_expr(ty),
                expr(value, depth + 1)
            ),
            (None, None) => format!(".param_untyped({})", lit(name)),
            (None, Some(value)) => format!(
                ".param_untyped_default({}, {})",
                lit(name),
                expr(value, depth + 1)
            ),
        };
        out.push_str(&format!("\n{}{}{}", pad(depth), call, attrs));
    }
    out
}

/// Emits `.body_exact(vec![…])`, or nothing for an empty body.
fn body_call(body: &[Stmt], depth: usize) -> String {
    if body.is_empty() {
        return String::new();
    }
    let mut out = format!("\n{}.body_exact(vec![", pad(depth));
    for stmt in body {
        out.push_str(&format!("\n{}{},", pad(depth + 1), statement(stmt, depth + 1)));
    }
    out.push_str(&format!("\n{}])", pad(depth)));
    out
}

/// Emits a `vec![…]` of statements.
fn stmt_vec(body: &[Stmt], depth: usize) -> String {
    if body.is_empty() {
        return "vec![]".to_string();
    }
    let mut out = String::from("vec![");
    for stmt in body {
        out.push_str(&format!("\n{}{},", pad(depth + 1), statement(stmt, depth + 1)));
    }
    out.push_str(&format!("\n{}]", pad(depth)));
    out
}

/// Emits one statement builder call.
fn statement(stmt: &Stmt, depth: usize) -> String {
    match &stmt.kind {
        StmtKind::Return(Some(value)) => format!("s_return({})", expr(value, depth)),
        StmtKind::Return(None) => "s_return_void()".to_string(),
        StmtKind::ExprStmt(value) => format!("s_expr({})", expr(value, depth)),
        StmtKind::Echo(value) => format!("s_echo({})", expr(value, depth)),
        StmtKind::Throw(value) => format!("s_throw({})", expr(value, depth)),
        StmtKind::Break(levels) => format!("s_break({})", levels),
        StmtKind::Continue(levels) => format!("s_continue({})", levels),
        StmtKind::Assign { name, value } => {
            format!("s_assign({}, {})", lit(name), expr(value, depth))
        }
        StmtKind::TypedAssign {
            type_expr,
            name,
            value,
        } => format!(
            "s_typed_assign({}, {}, {})",
            ty_expr(type_expr),
            lit(name),
            expr(value, depth)
        ),
        StmtKind::ArrayPush { array, value } => {
            format!("s_array_push({}, {})", lit(array), expr(value, depth))
        }
        StmtKind::ArrayAssign {
            array,
            index,
            value,
        } => format!(
            "s_array_assign({}, {}, {})",
            lit(array),
            expr(index, depth),
            expr(value, depth)
        ),
        StmtKind::PropertyAssign {
            object,
            property,
            value,
        } => format!(
            "s_prop_assign({}, {}, {})",
            expr(object, depth),
            lit(property),
            expr(value, depth)
        ),
        StmtKind::PropertyArrayPush {
            object,
            property,
            value,
        } => format!(
            "s_prop_array_push({}, {}, {})",
            expr(object, depth),
            lit(property),
            expr(value, depth)
        ),
        StmtKind::PropertyArrayAssign {
            object,
            property,
            index,
            value,
        } => format!(
            "s_prop_array_assign({}, {}, {}, {})",
            expr(object, depth),
            lit(property),
            expr(index, depth),
            expr(value, depth)
        ),
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            let clauses = if elseif_clauses.is_empty() {
                "vec![]".to_string()
            } else {
                let mut out = String::from("vec![");
                for (clause_condition, clause_body) in elseif_clauses {
                    out.push_str(&format!(
                        "\n{}({}, {}),",
                        pad(depth + 1),
                        expr(clause_condition, depth + 1),
                        stmt_vec(clause_body, depth + 1)
                    ));
                }
                out.push_str(&format!("\n{}]", pad(depth)));
                out
            };
            let otherwise = match else_body {
                Some(body) => format!("Some({})", stmt_vec(body, depth)),
                None => "None".to_string(),
            };
            format!(
                "s_if(\n{}{},\n{}{},\n{}{},\n{}{},\n{})",
                pad(depth + 1),
                expr(condition, depth + 1),
                pad(depth + 1),
                stmt_vec(then_body, depth + 1),
                pad(depth + 1),
                clauses,
                pad(depth + 1),
                otherwise,
                pad(depth)
            )
        }
        StmtKind::StaticPropertyAssign {
            receiver,
            property,
            value,
        } => match receiver {
            StaticReceiver::Self_ => format!(
                "s_self_static_prop_assign({}, {})",
                lit(property),
                expr(value, depth)
            ),
            other => format!(
                "s_static_prop_assign({}, {}, {})",
                lit(&static_receiver_name(other)),
                lit(property),
                expr(value, depth)
            ),
        },
        StmtKind::StaticPropertyArrayPush {
            receiver,
            property,
            value,
        } => format!(
            "s_static_prop_array_push({}, {}, {})",
            lit(&static_receiver_name(receiver)),
            lit(property),
            expr(value, depth)
        ),
        StmtKind::StaticPropertyArrayAssign {
            receiver,
            property,
            index,
            value,
        } => format!(
            "s_static_prop_array_assign({}, {}, {}, {})",
            lit(&static_receiver_name(receiver)),
            lit(property),
            expr(index, depth),
            expr(value, depth)
        ),
        StmtKind::DoWhile { body, condition } => format!(
            "s_do_while({}, {})",
            stmt_vec(body, depth),
            expr(condition, depth)
        ),
        StmtKind::While { condition, body } => format!(
            "s_while({}, {})",
            expr(condition, depth),
            stmt_vec(body, depth)
        ),
        StmtKind::StaticVar { name, init } => {
            format!("s_static({}, {})", lit(name), expr(init, depth))
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            let mut arms = String::from("vec![");
            for catch in catches {
                let types: Vec<String> = catch
                    .exception_types
                    .iter()
                    // `s_try` reads a leading `\` as root-anchored, so the spelling has to
                    // survive: `\Throwable` and `Throwable` are different nodes.
                    .map(|name| match name.kind {
                        crate::names::NameKind::FullyQualified => {
                            lit(&format!("\\{}", name.as_canonical()))
                        }
                        _ => lit(&name.as_canonical()),
                    })
                    .collect();
                let variable = match &catch.variable {
                    Some(name) => format!("Some({})", lit(name)),
                    None => "None".to_string(),
                };
                arms.push_str(&format!(
                    "\n{}(vec![{}], {}, {}),",
                    pad(depth + 1),
                    types.join(", "),
                    variable,
                    stmt_vec(&catch.body, depth + 1)
                ));
            }
            arms.push_str(&format!("\n{}]", pad(depth)));
            let finally = match finally_body {
                Some(body) => format!("Some({})", stmt_vec(body, depth)),
                None => "None".to_string(),
            };
            format!(
                "s_try({}, {}, {})",
                stmt_vec(try_body, depth),
                arms,
                finally
            )
        }
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => {
            let mut arms = String::from("vec![");
            for (labels, arm_body) in cases {
                arms.push_str(&format!(
                    "\n{}({}, {}),",
                    pad(depth + 1),
                    expr_vec(labels, depth + 1),
                    stmt_vec(arm_body, depth + 1)
                ));
            }
            arms.push_str(&format!("\n{}]", pad(depth)));
            let otherwise = match default {
                Some(body) => format!("Some({})", stmt_vec(body, depth)),
                None => "None".to_string(),
            };
            format!(
                "s_switch({}, {}, {})",
                expr(subject, depth),
                arms,
                otherwise
            )
        }
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => {
            let init = match init {
                Some(stmt) => format!("Some({})", statement(stmt, depth)),
                None => "None".to_string(),
            };
            let condition = match condition {
                Some(value) => format!("Some({})", expr(value, depth)),
                None => "None".to_string(),
            };
            let update = match update {
                Some(stmt) => format!("Some({})", statement(stmt, depth)),
                None => "None".to_string(),
            };
            format!(
                "s_for({}, {}, {}, {})",
                init,
                condition,
                update,
                stmt_vec(body, depth)
            )
        }
        StmtKind::Foreach {
            array,
            key_var,
            value_var,
            value_by_ref,
            body,
        } => {
            assert!(!value_by_ref, "by-ref foreach is not modelled");
            let key = match key_var {
                Some(name) => format!("Some({})", lit(name)),
                None => "None".to_string(),
            };
            format!(
                "s_foreach({}, {}, {}, {})",
                expr(array, depth),
                key,
                lit(value_var),
                stmt_vec(body, depth)
            )
        }
        other => panic!("transcribe: unmodelled statement {:?}", other),
    }
}

/// Emits a comma-separated `vec![…]` of expressions.
fn expr_vec(items: &[Expr], depth: usize) -> String {
    if items.is_empty() {
        return "vec![]".to_string();
    }
    let rendered: Vec<String> = items.iter().map(|item| expr(item, depth)).collect();
    format!("vec![{}]", rendered.join(", "))
}

/// Emits one expression builder call.
fn expr(value: &Expr, depth: usize) -> String {
    match &value.kind {
        ExprKind::Null => "e_null()".to_string(),
        ExprKind::This => "e_this()".to_string(),
        ExprKind::BoolLiteral(flag) => format!("e_bool({})", flag),
        ExprKind::IntLiteral(number) => format!("e_int({})", number),
        ExprKind::FloatLiteral(number) if number.is_infinite() && number.is_sign_positive() => {
            "e_float(f64::INFINITY)".to_string()
        }
        ExprKind::FloatLiteral(number) if number.is_infinite() => {
            "e_float(f64::NEG_INFINITY)".to_string()
        }
        ExprKind::FloatLiteral(number) if number.is_nan() => {
            "e_float(f64::NAN)".to_string()
        }
        ExprKind::FloatLiteral(number) => format!("e_float({:?})", number),
        ExprKind::StringLiteral(text) => format!("e_str({})", lit(text)),
        ExprKind::Variable(name) => format!("e_var({})", lit(name)),
        ExprKind::NamedArg { name, value } => {
            format!("e_named_arg({}, {})", lit(name), expr(value, depth))
        }
        ExprKind::ConstRef(name) => format!("e_const({})", lit(&name.as_canonical())),
        ExprKind::PostIncrement(name) => format!("e_post_inc({})", lit(name)),
        ExprKind::Negate(inner) => format!("e_neg({})", expr(inner, depth)),
        ExprKind::Not(inner) => format!("e_not({})", expr(inner, depth)),
        ExprKind::ErrorSuppress(inner) => {
            format!("e_error_suppress({})", expr(inner, depth))
        }
        ExprKind::Clone(inner) => format!("e_clone({})", expr(inner, depth)),
        ExprKind::Cast { target, expr: inner } => format!(
            "e_cast(CastType::{}, {})",
            cast_type(target),
            expr(inner, depth)
        ),
        ExprKind::BinaryOp { left, op, right } => format!(
            "e_binop({}, BinOp::{}, {})",
            expr(left, depth),
            bin_op(op),
            expr(right, depth)
        ),
        ExprKind::NullCoalesce { value, default } => format!(
            "e_null_coalesce({}, {})",
            expr(value, depth),
            expr(default, depth)
        ),
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => format!(
            "e_ternary({}, {}, {})",
            expr(condition, depth),
            expr(then_expr, depth),
            expr(else_expr, depth)
        ),
        ExprKind::ArrayAccess { array, index } => {
            format!("e_index({}, {})", expr(array, depth), expr(index, depth))
        }
        ExprKind::ArrayLiteral(items) => format!("e_array({})", expr_vec(items, depth)),
        ExprKind::ArrayLiteralAssoc(entries) => {
            let rendered: Vec<String> = entries
                .iter()
                .map(|(key, item)| format!("({}, {})", expr(key, depth), expr(item, depth)))
                .collect();
            format!("e_array_assoc(vec![{}])", rendered.join(", "))
        }
        ExprKind::ObjectClassName { object } => {
            format!("e_object_class_name({})", expr(object, depth))
        }
        ExprKind::StaticPropertyAccess { receiver, property } => match receiver {
            StaticReceiver::Self_ => format!("e_self_static_prop({})", lit(property)),
            other => format!(
                "e_static_prop({}, {})",
                lit(&static_receiver_name(other)),
                lit(property)
            ),
        },
        ExprKind::PropertyAccess { object, property } => {
            if matches!(object.kind, ExprKind::This) {
                format!("e_this_prop({})", lit(property))
            } else {
                format!("e_prop({}, {})", expr(object, depth), lit(property))
            }
        }
        ExprKind::DynamicPropertyAccess { object, property } => format!(
            "e_dyn_prop({}, {})",
            expr(object, depth),
            expr(property, depth)
        ),
        ExprKind::Assignment {
            target,
            value,
            result_target,
            prelude,
            conditional_value_temp,
        } => {
            assert!(
                result_target.is_none()
                    && prelude.is_empty()
                    && conditional_value_temp.is_none(),
                "only plain assignments are modelled"
            );
            format!(
                "e_assign({}, {})",
                expr(target, depth),
                expr(value, depth)
            )
        }
        ExprKind::FunctionCall { name, args } => format!(
            "e_call({}, {})",
            lit(&name_source(name)),
            expr_vec(args, depth)
        ),
        ExprKind::MethodCall {
            object,
            method,
            args,
        } => format!(
            "e_method_call({}, {}, {})",
            expr(object, depth),
            lit(method),
            expr_vec(args, depth)
        ),
        ExprKind::StaticMethodCall {
            receiver,
            method,
            args,
        } => match receiver {
            StaticReceiver::Named(name) => format!(
                "e_static_call({}, {}, {})",
                lit(&name_source(name)),
                lit(method),
                expr_vec(args, depth)
            ),
            StaticReceiver::Self_ => format!(
                "e_self_call({}, {})",
                lit(method),
                expr_vec(args, depth)
            ),
            StaticReceiver::Parent => format!(
                "e_parent_call({}, {})",
                lit(method),
                expr_vec(args, depth)
            ),
            other => panic!("transcribe: unmodelled static receiver {:?}", other),
        },
        ExprKind::NewObject { class_name, args } => {
            let rendered = expr_vec(args, depth);
            match class_name.kind {
                crate::names::NameKind::FullyQualified => format!(
                    "e_new_fq({}, {})",
                    lit(&class_name.as_canonical()),
                    rendered
                ),
                _ => format!("e_new({}, {})", lit(&class_name.as_canonical()), rendered),
            }
        }
        ExprKind::Spread(value) => format!("e_spread({})", expr(value, depth)),
        ExprKind::ClosureCall { var, args } => format!(
            "e_closure_call({}, {})",
            lit(var),
            expr_vec(args, depth)
        ),
        ExprKind::Closure {
            params,
            variadic,
            variadic_by_ref,
            variadic_type,
            return_type,
            body,
            is_arrow,
            is_static,
            by_ref_return,
            captures,
            capture_refs,
        } => {
            assert!(variadic.is_none(), "a variadic closure is not modelled");
            assert!(!variadic_by_ref, "a by-ref variadic is not modelled");
            assert!(variadic_type.is_none(), "a variadic type is not modelled");
            assert!(!is_arrow, "an arrow function is not modelled");
            assert!(!is_static, "a static closure is not modelled");
            assert!(!by_ref_return, "a by-ref closure return is not modelled");
            let mut out = "closure()".to_string();
            out.push_str(&params_calls(params, &[], depth + 1));
            // One call per captured name, not one per list: `capture_refs` marks which of
            // `captures` alias rather than naming a second set, so iterating both emits every
            // by-ref capture twice.
            for name in captures {
                let call = if capture_refs.contains(name) {
                    "captures_ref"
                } else {
                    "captures"
                };
                out.push_str(&format!("\n{}.{call}({})", pad(depth + 1), lit(name)));
            }
            assert!(
                capture_refs.iter().all(|name| captures.contains(name)),
                "a by-ref capture must also be listed as a capture"
            );
            if let Some(ty) = return_type {
                out.push_str(&format!("\n{}.returns({})", pad(depth + 1), ty_expr(ty)));
            }
            out.push_str(&format!(
                "\n{}.body({})\n{}.build()",
                pad(depth + 1),
                stmt_vec(body, depth + 1),
                pad(depth + 1)
            ));
            out
        }
        ExprKind::ClassConstant { receiver } => match receiver {
            StaticReceiver::Self_ => "e_self_class()".to_string(),
            StaticReceiver::Static => "e_static_class()".to_string(),
            StaticReceiver::Named(name) => {
                format!("e_named_class({})", lit(&name.as_canonical()))
            }
            other => panic!("transcribe: unmodelled ::class receiver {:?}", other),
        },
        ExprKind::ScopedConstantAccess { receiver, name } => match receiver {
            StaticReceiver::Named(class_name) => format!(
                "e_class_const({}, {})",
                lit(&name_source(class_name)),
                lit(name)
            ),
            StaticReceiver::Self_ => format!("e_self_const({})", lit(name)),
            other => panic!("transcribe: unmodelled constant receiver {:?}", other),
        },
        ExprKind::InstanceOf { value, target } => match target {
            crate::parser::ast::InstanceOfTarget::Name(name) => format!(
                "e_instance_of({}, {})",
                expr(value, depth),
                lit(&name_source(name))
            ),
            other => panic!("transcribe: unmodelled instanceof target {:?}", other),
        },
        ExprKind::NewDynamic { name_expr, args } => format!(
            "e_new_dynamic({}, {})",
            expr(name_expr, depth),
            expr_vec(args, depth)
        ),
        ExprKind::NewScopedObject { receiver, args } => match receiver {
            StaticReceiver::Self_ => format!("e_new_self({})", expr_vec(args, depth)),
            StaticReceiver::Static => format!("e_new_static({})", expr_vec(args, depth)),
            other => panic!("transcribe: unmodelled scoped receiver {:?}", other),
        },
        other => panic!("transcribe: unmodelled expression {:?}", other),
    }
}

/// The class name behind a `Class::` receiver. Only a NAMED receiver is modelled: `self::` and
/// `static::` inside a prelude would need the enclosing class, which the builders do not track.
fn static_receiver_name(receiver: &StaticReceiver) -> String {
    match receiver {
        StaticReceiver::Named(name) => name.as_canonical(),
        other => panic!("transcribe: unmodelled static property receiver {:?}", other),
    }
}

/// Emits a `TypeExpr` construction.
fn ty_expr(ty: &TypeExpr) -> String {
    match ty {
        TypeExpr::Int => "TypeExpr::Int".to_string(),
        TypeExpr::Float => "TypeExpr::Float".to_string(),
        TypeExpr::Bool => "TypeExpr::Bool".to_string(),
        TypeExpr::False => "TypeExpr::False".to_string(),
        TypeExpr::Str => "TypeExpr::Str".to_string(),
        TypeExpr::Void => "TypeExpr::Void".to_string(),
        TypeExpr::Ptr(None) => "t_ptr()".to_string(),
        TypeExpr::Never => "TypeExpr::Never".to_string(),
        TypeExpr::Iterable => "TypeExpr::Iterable".to_string(),
        TypeExpr::Nullable(inner) => format!("t_nullable({})", ty_expr(inner)),
        TypeExpr::Union(members) => {
            let rendered: Vec<String> = members.iter().map(ty_expr).collect();
            format!("t_union(vec![{}])", rendered.join(", "))
        }
        TypeExpr::Named(name) => {
            let text = name.as_canonical();
            match text.as_str() {
                "mixed" => "t_mixed()".to_string(),
                "array" => "t_array()".to_string(),
                // The SOURCE spelling: `\Iterator` and `Iterator` are different nodes, and a
                // hint rebuilt from the canonical text would quietly become the second one.
                _ => format!("t_class({})", lit(&name_source(name))),
            }
        }
        other => panic!("transcribe: unmodelled type {:?}", other),
    }
}

/// Emits a `CType` variant name.
fn c_type(ty: &CType) -> String {
    match ty {
        CType::Int => "CType::Int".to_string(),
        CType::Float => "CType::Float".to_string(),
        CType::Str => "CType::Str".to_string(),
        CType::Bool => "CType::Bool".to_string(),
        CType::Void => "CType::Void".to_string(),
        CType::Ptr => "CType::Ptr".to_string(),
        other => panic!("transcribe: unmodelled C type {:?}", other),
    }
}

/// Emits a `CastType` variant name.
fn cast_type(target: &CastType) -> &'static str {
    match target {
        CastType::Int => "Int",
        CastType::Float => "Float",
        CastType::String => "String",
        CastType::Bool => "Bool",
        CastType::Array => "Array",
        CastType::Void => "Void",
    }
}

/// Emits a `BinOp` variant name.
fn bin_op(op: &BinOp) -> &'static str {
    match op {
        BinOp::Add => "Add",
        BinOp::Sub => "Sub",
        BinOp::Mul => "Mul",
        BinOp::Div => "Div",
        BinOp::Mod => "Mod",
        BinOp::Concat => "Concat",
        BinOp::Eq => "Eq",
        BinOp::NotEq => "NotEq",
        BinOp::StrictEq => "StrictEq",
        BinOp::StrictNotEq => "StrictNotEq",
        BinOp::Lt => "Lt",
        BinOp::Gt => "Gt",
        BinOp::LtEq => "LtEq",
        BinOp::GtEq => "GtEq",
        BinOp::Pow => "Pow",
        BinOp::And => "And",
        BinOp::Or => "Or",
        BinOp::Xor => "Xor",
        BinOp::BitAnd => "BitAnd",
        BinOp::BitOr => "BitOr",
        BinOp::BitXor => "BitXor",
        BinOp::ShiftLeft => "ShiftLeft",
        BinOp::ShiftRight => "ShiftRight",
        BinOp::Spaceship => "Spaceship",
        BinOp::NullCoalesce => "NullCoalesce",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// MIGRATION ORACLE — run with `ELEPHC_ORACLE_PHP=<path.php>` and `ELEPHC_ORACLE_WHICH=<name>`
    /// to assert that a converted prelude rebuilds the same AST as the PHP text it replaced.
    /// Inert without the variables, so it costs nothing in normal runs.
    #[test]
    fn converted_prelude_matches_its_php_on_request() {
        let Ok(php_path) = std::env::var("ELEPHC_ORACLE_PHP") else {
            return;
        };
        let which = std::env::var("ELEPHC_ORACLE_WHICH").unwrap_or_default();
        let built = match which.as_str() {
            "pdo" => crate::pdo_prelude::build::pdo_declarations(
                crate::php_version::PhpVersion::default(),
                crate::pdo_prelude::OptionalDrivers::from_build_environment(),
            ),
            "image" => crate::image_prelude::image_declarations(),
            other => panic!("unknown prelude {other}"),
        };
        let php = std::fs::read_to_string(&php_path).expect("must read the PHP fixture");
        let tokens = crate::lexer::tokenize(&php).expect("fixture must tokenize");
        let parsed = crate::parser::parse_internal(&tokens).expect("fixture must parse");

        assert_eq!(built.len(), parsed.len(), "declaration count");
        for (built_stmt, parsed_stmt) in built.iter().zip(parsed.iter()) {
            let label = match &built_stmt.kind {
                StmtKind::FunctionDecl { name, .. }
                | StmtKind::ClassDecl { name, .. }
                | StmtKind::ExternFunctionDecl { name, .. } => name.clone(),
                _ => "<other>".to_string(),
            };
            let left = strip_spans(&format!("{:?}", built_stmt));
            let right = strip_spans(&format!("{:?}", parsed_stmt));
            if left != right {
                if let Ok(dir) = std::env::var("ELEPHC_ORACLE_DUMP") {
                    std::fs::write(format!("{dir}/built.txt"), left.replace("}, ", "},\n"))
                        .expect("dump built");
                    std::fs::write(format!("{dir}/parsed.txt"), right.replace("}, ", "},\n"))
                        .expect("dump parsed");
                }
                panic!("mismatch in {label}");
            }
        }
    }

    /// Removes span payloads so a synthetic node and a parsed node compare on structure.
    fn strip_spans(rendered: &str) -> String {
        let mut cleaned = String::with_capacity(rendered.len());
        let mut rest = rendered;
        while let Some(at) = rest.find("Span {") {
            cleaned.push_str(&rest[..at]);
            cleaned.push_str("Span");
            let after = &rest[at..];
            let close = after.find('}').map(|end| end + 1).unwrap_or(after.len());
            rest = &after[close..];
        }
        cleaned.push_str(rest);
        cleaned
    }

    /// MIGRATION DRIVER — run with `ELEPHC_TRANSCRIBE_OUT=<path>` to dump the builder calls
    /// for a prelude still held as PHP text. Does nothing without the variable, so it is inert
    /// in normal runs.
    #[test]
    fn dump_prelude_on_request() {
        let Ok(out) = std::env::var("ELEPHC_TRANSCRIBE_OUT") else {
            return;
        };
        let which = std::env::var("ELEPHC_TRANSCRIBE_WHICH").unwrap_or_default();
        let source = match which.as_str() {
            "image_old" => std::fs::read_to_string(
                std::env::var("ELEPHC_TRANSCRIBE_IN").expect("ELEPHC_TRANSCRIBE_IN"),
            )
            .expect("must read the source"),
            "file" => std::fs::read_to_string(
                std::env::var("ELEPHC_TRANSCRIBE_IN").expect("ELEPHC_TRANSCRIBE_IN"),
            )
            .expect("must read the source"),
            other => panic!("unknown prelude {other}"),
        };
        let tokens = crate::lexer::tokenize(&source).expect("prelude must tokenize");
        let parsed = crate::parser::parse_internal(&tokens).expect("prelude must parse");
        let rendered = match std::env::var("ELEPHC_TRANSCRIBE_SPLIT") {
            Ok(aggregator) => transcribe_split(&parsed, &aggregator),
            Err(_) => transcribe(&parsed),
        };
        std::fs::write(&out, rendered).expect("must write the transcription");
    }

    /// Transcribing a program and comparing the ROUND TRIP is what makes this tool
    /// trustworthy: the emitted builder calls must rebuild the same AST. This checks the
    /// property on a program using the constructs the preludes are made of.
    ///
    /// The round trip cannot be run automatically — the output is Rust source, and compiling it
    /// requires pasting it in — so what is asserted here is that transcription SUCCEEDS on every
    /// node (the emitter panics otherwise) and that the output mentions each construct.
    #[test]
    fn transcribes_the_prelude_vocabulary_without_panicking() {
        let source = r#"<?php
extern "elephc_demo" {
    function elephc_demo_open(string $dsn): int;
}
class Demo implements Iterator {
    const MODE = 2;
    private int $handle;
    private $target;
    public function __construct(int $handle) {
        $this->handle = $handle;
        $this->target = null;
    }
    private function fail(string $message): void {
        if ($this->handle == 2) {
            throw new DemoException($message);
        }
        fwrite(STDERR, "err: " . $message . "\n");
    }
    public function run(?string $name = null, $flag = 1): string|bool {
        $_v = $name ?? "";
        for ($_i = 0; $_i < 3; $_i++) {
            $_v = $_v . strval($_i);
        }
        foreach ([1, 2] as $_k => $_x) {
            $_v = $_v . strval($_k);
        }
        return ($flag == 1) ? $_v : false;
    }
}
function demo_make(int $h) {
    return new Demo($h);
}
"#;
        let tokens = crate::lexer::tokenize(source).expect("fixture must tokenize");
        let parsed = crate::parser::parse_internal(&tokens).expect("fixture must parse");
        let rendered = transcribe(&parsed);

        for needle in [
            "extern_fn(\"elephc_demo_open\", \"elephc_demo\")",
            ".implements(\"Iterator\")",
            ".constant(\"MODE\"",
            ".private_prop(\"handle\"",
            ".private_untyped_prop(\"target\"",
            ".private()",
            "s_prop_assign(e_this()",
            "s_throw(e_new(\"DemoException\"",
            "e_null_coalesce",
            "s_for(",
            "s_foreach(",
            "e_ternary(",
            "t_union(vec![TypeExpr::Str, TypeExpr::Bool])",
            "t_nullable(TypeExpr::Str)",
            ".param_untyped_default(\"flag\"",
            "e_new(\"Demo\"",
        ] {
            assert!(
                rendered.contains(needle),
                "transcription should mention {needle}\n--- output ---\n{rendered}"
            );
        }
    }
}
