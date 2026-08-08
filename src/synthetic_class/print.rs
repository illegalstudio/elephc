//! Purpose:
//! Renders a built declaration back to PHP source. This is the transcriber's twin: `transcribe`
//! turns PHP into builder calls, `print` turns builder output back into PHP.
//!
//! Called from:
//! - Tests of the preludes that are now built in Rust, which assert reference-PHP facts about
//!   what a target bakes (`'kind' => 5`, `in_array($path, ['/a.php'], true)`, the presence or
//!   absence of a key). Those assertions were written against a rendered string, and are exactly
//!   as valid against this rendering — provided the rendering is faithful, which is what
//!   `printing_round_trips` pins.
//!
//! Key details:
//! - IT IS TEST-ONLY, and deliberately so. Nothing in the compiler needs a PHP printer: the AST
//!   is the artifact. Shipping one would invite a second source of truth of exactly the kind
//!   this campaign is removing.
//! - WHAT MAKES IT TRUSTWORTHY IS THE ROUND TRIP, not the code being obviously right: printing a
//!   program, re-parsing it, and comparing node for node is a property the tool can check on
//!   itself, and `printing_round_trips` runs it over every prelude built in Rust. A `contains`
//!   assertion on this output is then as strong as one on hand-written source.
//! - It PANICS on any node it does not model, the same rule `transcribe` follows: a printer that
//!   silently skipped a node would make a test assert about a program that is not the one built.
//! - Formatting follows the PHP the preludes were written in rather than any general style: a
//!   single-statement `if`/`while`/`foreach` with no `else` stays on ONE LINE, everything else
//!   breaks. That is what the prelude sources looked like, so assertions carried over from them
//!   keep matching.

use crate::parser::ast::{
    AttributeGroup, BinOp, CType, CastType, Expr, ExprKind, Program, StaticReceiver, Stmt,
    StmtKind, TypeExpr, Visibility,
};

/// Renders a whole program as PHP source, one declaration after another.
pub fn print_program(program: &Program) -> String {
    let mut out = String::new();
    for stmt in program {
        out.push_str(&statement(stmt, 0));
        out.push('\n');
    }
    out
}

/// Renders one expression as PHP source — for the baked LITERALS (a configuration array, a
/// manifest path list) that are values rather than declarations.
pub fn print_expr(value: &Expr) -> String {
    expression(value)
}

/// A NAME as it was written in source.
///
/// `as_canonical` gives the backslash-joined text used for lookup, which drops the LEADING
/// backslash of a fully-qualified name — and `\Exception` and `Exception` are different nodes
/// (`NameKind::FullyQualified` against `Unqualified`), so printing the canonical form for both
/// would not round-trip. The prelude builders spell some class names fully qualified precisely
/// so they cannot be captured by a namespace, and that distinction has to survive.
fn name_source(name: &crate::names::Name) -> String {
    match name.kind {
        crate::names::NameKind::FullyQualified => format!("\\{}", name.as_canonical()),
        _ => name.as_canonical(),
    }
}

/// One indent level.
fn pad(depth: usize) -> String {
    "    ".repeat(depth)
}

/// Renders a PHP string literal.
///
/// Single quotes unless the value carries a control character, in which case double quotes with
/// escapes — the same split the prelude sources used (`'Warning: …'` beside `"\n"`), and the one
/// that keeps a printed statement on a single line.
fn string_literal(value: &str) -> String {
    if value.chars().any(|character| character.is_control()) {
        let mut out = String::from("\"");
        for character in value.chars() {
            match character {
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                '\t' => out.push_str("\\t"),
                '\\' => out.push_str("\\\\"),
                '"' => out.push_str("\\\""),
                // Double-quoted PHP interpolates, so a literal `$` has to be escaped.
                '$' => out.push_str("\\$"),
                other => out.push(other),
            }
        }
        out.push('"');
        return out;
    }
    format!("'{}'", value.replace('\\', "\\\\").replace('\'', "\\'"))
}

/// Renders a statement, indented to `depth`.
fn statement(stmt: &Stmt, depth: usize) -> String {
    let indent = pad(depth);
    match &stmt.kind {
        StmtKind::FunctionDecl {
            name,
            params,
            param_attributes,
            variadic,
            variadic_type,
            return_type,
            body,
            ..
        } => {
            let mut list = parameter_list(params, param_attributes);
            if let Some(tail) = variadic {
                if !list.is_empty() {
                    list.push_str(", ");
                }
                let hint = match variadic_type {
                    Some(ty) => format!("{} ", type_expr(ty)),
                    None => String::new(),
                };
                list.push_str(&format!("{hint}...${tail}"));
            }
            let signature = format!(
                "{indent}function {name}({list}){}",
                return_hint(return_type)
            );
            format!("{signature} {}", block(body, depth))
        }
        StmtKind::ClassDecl {
            name,
            extends,
            implements,
            constants,
            properties,
            methods,
            is_final,
            ..
        } => {
            let mut head = String::from(indent.as_str());
            if *is_final {
                head.push_str("final ");
            }
            head.push_str(&format!("class {name}"));
            if let Some(parent) = extends {
                head.push_str(&format!(" extends {}", name_source(parent)));
            }
            if !implements.is_empty() {
                let names: Vec<String> = implements
                    .iter()
                    .map(name_source)
                    .collect();
                head.push_str(&format!(" implements {}", names.join(", ")));
            }
            let mut out = format!("{head} {{\n");
            for constant in constants {
                // Attributes print on their own line above the constant, which is where the
                // prelude sources put them and what the parser reads back.
                for group in &constant.attributes {
                    let rendered: Vec<String> = group
                        .attributes
                        .iter()
                        .map(|attribute| {
                            if attribute.args.is_empty() {
                                name_source(&attribute.name)
                            } else {
                                format!(
                                    "{}({})",
                                    name_source(&attribute.name),
                                    argument_list(&attribute.args)
                                )
                            }
                        })
                        .collect();
                    out.push_str(&format!("{}#[{}]\n", pad(depth + 1), rendered.join(", ")));
                }
                out.push_str(&format!(
                    "{}const {} = {};\n",
                    pad(depth + 1),
                    constant.name,
                    expression(&constant.value)
                ));
            }
            for property in properties {
                let visibility = match property.visibility {
                    Visibility::Public => "public",
                    Visibility::Private => "private",
                    Visibility::Protected => "protected",
                };
                let type_hint = match &property.type_expr {
                    Some(ty) => format!("{} ", type_expr(ty)),
                    None => String::new(),
                };
                let default = match &property.default {
                    Some(value) => format!(" = {}", expression(value)),
                    None => String::new(),
                };
                // `readonly` sits between the visibility and the type, and PHP rejects it on a
                // static property — which is why the two modifiers are separate strings here
                // rather than one `modifiers` field: they cannot both appear.
                out.push_str(&format!(
                    "{}{visibility} {}{}{type_hint}${}{default};\n",
                    pad(depth + 1),
                    if property.is_static { "static " } else { "" },
                    if property.readonly { "readonly " } else { "" },
                    property.name
                ));
            }
            for method in methods {
                let visibility = match method.visibility {
                    Visibility::Public => "public ",
                    Visibility::Private => "private ",
                    Visibility::Protected => "protected ",
                };
                let modifiers = format!(
                    "{}{}",
                    if method.is_final { "final " } else { "" },
                    if method.is_static { "static " } else { "" }
                );
                let mut list = parameter_list(&method.params, &method.param_attributes);
                if let Some(tail) = &method.variadic {
                    if !list.is_empty() {
                        list.push_str(", ");
                    }
                    let hint = match &method.variadic_type {
                        Some(ty) => format!("{} ", type_expr(ty)),
                        None => String::new(),
                    };
                    list.push_str(&format!("{hint}...${tail}"));
                }
                out.push_str(&format!(
                    "{}{visibility}{modifiers}function {}({list}){} {}\n",
                    pad(depth + 1),
                    method.name,
                    return_hint(&method.return_type),
                    block(&method.body, depth + 1)
                ));
            }
            out.push_str(&format!("{indent}}}"));
            out
        }
        StmtKind::InterfaceDecl {
            name,
            extends,
            properties,
            methods,
            constants,
        } => {
            assert!(properties.is_empty(), "interface properties are not modelled");
            let mut head = format!("{indent}interface {name}");
            if !extends.is_empty() {
                let names: Vec<String> = extends.iter().map(name_source).collect();
                head.push_str(&format!(" extends {}", names.join(", ")));
            }
            let mut out = format!("{head} {{\n");
            for constant in constants {
                out.push_str(&format!(
                    "{}const {} = {};\n",
                    pad(depth + 1),
                    constant.name,
                    expression(&constant.value)
                ));
            }
            for signature in methods {
                assert!(
                    !signature.has_body,
                    "an interface method is a signature: {}",
                    signature.name
                );
                out.push_str(&format!(
                    "{}public function {}({}){};\n",
                    pad(depth + 1),
                    signature.name,
                    parameter_list(&signature.params, &signature.param_attributes),
                    return_hint(&signature.return_type)
                ));
            }
            out.push_str(&format!("{indent}}}"));
            out
        }
        // The BRACED form, and it has to stay braced: `namespace Pdo;` would capture every
        // declaration after it, which for a prelude means the global classes it goes on to
        // declare. Printing it with braces is what makes the reparse match the built AST.
        StmtKind::NamespaceBlock { name, body } => {
            let name = name
                .as_ref()
                .expect("an unnamed namespace block is not modelled");
            let inner: Vec<String> = body
                .iter()
                .map(|stmt| statement(stmt, depth + 1))
                .collect();
            format!(
                "{indent}namespace {} {{\n{}\n{indent}}}",
                name_source(name),
                inner.join("\n")
            )
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
            let rendered: Vec<String> = params
                .iter()
                .map(|param| format!("{} ${}", c_type(&param.c_type), param.name))
                .collect();
            format!(
                "{indent}extern \"{library}\" {{\n{}function {name}({}): {};\n{indent}}}",
                pad(depth + 1),
                rendered.join(", "),
                c_type(return_type)
            )
        }
        StmtKind::ConstDecl { name, value } => {
            format!("{indent}const {name} = {};", expression(value))
        }
        StmtKind::StaticVar { name, init } => {
            format!("{indent}static ${name} = {};", expression(init))
        }
        StmtKind::Return(Some(value)) => format!("{indent}return {};", expression(value)),
        StmtKind::Return(None) => format!("{indent}return;"),
        StmtKind::Break(levels) => format!("{indent}break {levels};"),
        StmtKind::Continue(levels) => format!("{indent}continue {levels};"),
        StmtKind::Throw(value) => format!("{indent}throw {};", expression(value)),
        StmtKind::Echo(value) => format!("{indent}echo {};", expression(value)),
        // An assignment evaluated for its EFFECT is written bare. Wrapping it in parentheses
        // would make the parser treat its value as used and attach a `result_target`, which is a
        // different node from the one built.
        StmtKind::ExprStmt(Expr {
            kind: ExprKind::Assignment { target, value, .. },
            ..
        }) => format!(
            "{indent}{} = {};",
            expression(target),
            expression(value)
        ),
        StmtKind::ExprStmt(value) => format!("{indent}{};", expression(value)),
        StmtKind::Assign { name, value } => {
            format!("{indent}${name} = {};", expression(value))
        }
        StmtKind::TypedAssign {
            type_expr: declared,
            name,
            value,
        } => format!(
            "{indent}{} ${name} = {};",
            type_expr(declared),
            expression(value)
        ),
        StmtKind::ArrayAssign {
            array,
            index,
            value,
        } => format!(
            "{indent}${array}[{}] = {};",
            expression(index),
            expression(value)
        ),
        StmtKind::ArrayPush { array, value } => {
            format!("{indent}${array}[] = {};", expression(value))
        }
        StmtKind::StaticPropertyAssign {
            receiver,
            property,
            value,
        } => format!(
            "{indent}{}::${property} = {};",
            static_receiver(receiver),
            expression(value)
        ),
        StmtKind::StaticPropertyArrayPush {
            receiver,
            property,
            value,
        } => format!(
            "{indent}{}::${property}[] = {};",
            static_receiver(receiver),
            expression(value)
        ),
        StmtKind::StaticPropertyArrayAssign {
            receiver,
            property,
            index,
            value,
        } => format!(
            "{indent}{}::${property}[{}] = {};",
            static_receiver(receiver),
            expression(index),
            expression(value)
        ),
        StmtKind::PropertyAssign {
            object,
            property,
            value,
        } => format!(
            "{indent}{}->{property} = {};",
            expression(object),
            expression(value)
        ),
        StmtKind::PropertyArrayPush {
            object,
            property,
            value,
        } => format!(
            "{indent}{}->{property}[] = {};",
            expression(object),
            expression(value)
        ),
        StmtKind::PropertyArrayAssign {
            object,
            property,
            index,
            value,
        } => format!(
            "{indent}{}->{property}[{}] = {};",
            expression(object),
            expression(index),
            expression(value)
        ),
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            let simple = elseif_clauses.is_empty() && else_body.is_none();
            let mut out = format!(
                "{indent}if ({}) {}",
                expression(condition),
                if simple {
                    inline_or_block(then_body, depth)
                } else {
                    block(then_body, depth)
                }
            );
            for (clause_condition, clause_body) in elseif_clauses {
                out.push_str(&format!(
                    " elseif ({}) {}",
                    expression(clause_condition),
                    block(clause_body, depth)
                ));
            }
            if let Some(body) = else_body {
                out.push_str(&format!(" else {}", block(body, depth)));
            }
            out
        }
        StmtKind::DoWhile { body, condition } => format!(
            "{indent}do {} while ({});",
            block(body, depth),
            expression(condition)
        ),
        StmtKind::While { condition, body } => format!(
            "{indent}while ({}) {}",
            expression(condition),
            inline_or_block(body, depth)
        ),
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => {
            let head = |stmt: &Option<Box<Stmt>>| match stmt {
                Some(stmt) => statement(stmt, 0).trim_end_matches(';').to_string(),
                None => String::new(),
            };
            format!(
                "{indent}for ({}; {}; {}) {}",
                head(init),
                condition.as_ref().map(expression).unwrap_or_default(),
                head(update),
                block(body, depth)
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
            let binding = match key_var {
                Some(key) => format!("${key} => ${value_var}"),
                None => format!("${value_var}"),
            };
            format!(
                "{indent}foreach ({} as {binding}) {}",
                expression(array),
                inline_or_block(body, depth)
            )
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            let mut out = format!("{indent}try {}", block(try_body, depth));
            for catch in catches {
                let types: Vec<String> = catch
                    .exception_types
                    .iter()
                    .map(name_source)
                    .collect();
                let binding = match &catch.variable {
                    Some(name) => format!(" ${name}"),
                    None => String::new(),
                };
                out.push_str(&format!(
                    " catch ({}{binding}) {}",
                    types.join(" | "),
                    block(&catch.body, depth)
                ));
            }
            if let Some(body) = finally_body {
                out.push_str(&format!(" finally {}", block(body, depth)));
            }
            out
        }
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => {
            let mut out = format!("{indent}switch ({}) {{\n", expression(subject));
            for (labels, body) in cases {
                for label in labels {
                    out.push_str(&format!("{}case {}:\n", pad(depth + 1), expression(label)));
                }
                for inner in body {
                    out.push_str(&format!("{}\n", statement(inner, depth + 2)));
                }
            }
            if let Some(body) = default {
                out.push_str(&format!("{}default:\n", pad(depth + 1)));
                for inner in body {
                    out.push_str(&format!("{}\n", statement(inner, depth + 2)));
                }
            }
            out.push_str(&format!("{indent}}}"));
            out
        }
        other => panic!("print: unmodelled statement {:?}", truncate(other)),
    }
}

/// A `{ … }` body, one statement per line.
fn block(body: &[Stmt], depth: usize) -> String {
    if body.is_empty() {
        return "{}".to_string();
    }
    let mut out = String::from("{\n");
    for stmt in body {
        out.push_str(&format!("{}\n", statement(stmt, depth + 1)));
    }
    out.push_str(&format!("{}}}", pad(depth)));
    out
}

/// A body that stays on one line when it is a single simple statement — the shape the prelude
/// sources used for their guard clauses (`if ($v === '') { return $def; }`).
fn inline_or_block(body: &[Stmt], depth: usize) -> String {
    if body.len() == 1 && !matches!(body[0].kind, StmtKind::If { .. } | StmtKind::Try { .. }) {
        return format!("{{ {} }}", statement(&body[0], 0));
    }
    block(body, depth)
}

/// A parameter list, by-ref parameters carrying their `&`.
fn parameter_list(
    params: &[(String, Option<TypeExpr>, Option<Expr>, bool)],
    param_attributes: &[Vec<AttributeGroup>],
) -> String {
    let rendered: Vec<String> = params
        .iter()
        .enumerate()
        .map(|(index, (name, ty, default, by_ref))| {
            // `#[\SensitiveParameter] ?string $pw = null` — the attribute precedes the type,
            // and its name is printed as written so a leading `\` survives the round trip.
            let attrs = param_attributes
                .get(index)
                .map(|groups| {
                    groups
                        .iter()
                        .map(|group| {
                            let names: Vec<String> =
                                group.attributes.iter().map(|a| name_source(&a.name)).collect();
                            format!("#[{}] ", names.join(", "))
                        })
                        .collect::<String>()
                })
                .unwrap_or_default();
            let type_hint = match ty {
                Some(ty) => format!("{} ", type_expr(ty)),
                None => String::new(),
            };
            let default = match default {
                Some(value) => format!(" = {}", expression(value)),
                None => String::new(),
            };
            format!(
                "{attrs}{type_hint}{}${name}{default}",
                if *by_ref { "&" } else { "" }
            )
        })
        .collect();
    rendered.join(", ")
}

/// `: <type>` when the declaration has a return hint.
fn return_hint(return_type: &Option<TypeExpr>) -> String {
    match return_type {
        Some(ty) => format!(": {}", type_expr(ty)),
        None => String::new(),
    }
}

/// A type expression as PHP source.
fn type_expr(ty: &TypeExpr) -> String {
    match ty {
        TypeExpr::Int => "int".to_string(),
        TypeExpr::Float => "float".to_string(),
        TypeExpr::Bool => "bool".to_string(),
        TypeExpr::False => "false".to_string(),
        TypeExpr::Str => "string".to_string(),
        TypeExpr::Void => "void".to_string(),
        TypeExpr::Never => "never".to_string(),
        TypeExpr::Iterable => "iterable".to_string(),
        TypeExpr::Nullable(inner) => format!("?{}", type_expr(inner)),
        TypeExpr::Union(members) => members
            .iter()
            .map(type_expr)
            .collect::<Vec<_>>()
            .join("|"),
        TypeExpr::Ptr(None) => "ptr".to_string(),
        TypeExpr::Named(name) => name_source(name),
        other => panic!("print: unmodelled type {:?}", other),
    }
}

/// A C type in an `extern` block.
fn c_type(ty: &CType) -> &'static str {
    match ty {
        CType::Int => "int",
        CType::Float => "float",
        CType::Str => "string",
        CType::Bool => "bool",
        CType::Void => "void",
        CType::Ptr => "ptr",
        other => panic!("print: unmodelled C type {:?}", other),
    }
}

/// An expression as PHP source, parenthesized only where PHP's own precedence would otherwise
/// regroup it.
///
/// The builders carry precedence in the TREE, not in the text, so a printer that ignored it
/// would emit source that re-parses into a different program — `(a || b) && c` as
/// `a || (b && c)`. Parenthesizing everything is correct but unreadable, and it stops the
/// assertions these tests carry over from the prelude sources from matching. So the printer
/// asks each position what it binds at, and brackets only what binds looser.
///
/// The table below is asserted, not assumed: `printing_round_trips` re-parses everything this
/// prints, so a wrong entry fails loudly rather than producing a plausible wrong program.
fn expression(value: &Expr) -> String {
    let (text, _) = render(value);
    text
}

/// Renders `value` bracketed if it binds looser than `required`.
fn at(value: &Expr, required: u8) -> String {
    let (text, precedence) = render(value);
    if precedence < required {
        return format!("({text})");
    }
    text
}

/// A primary expression: nothing binds tighter, so anything else in this position brackets.
const ATOM: u8 = 100;

/// PHP's binding strength for one binary operator. Higher binds tighter.
fn operator_precedence(op: &BinOp) -> u8 {
    match op {
        BinOp::Pow => 90,
        BinOp::Mul | BinOp::Div | BinOp::Mod => 70,
        BinOp::Add | BinOp::Sub => 65,
        BinOp::ShiftLeft | BinOp::ShiftRight => 60,
        // PHP 8 moved `.` BELOW `+`/`-` and the shifts.
        BinOp::Concat => 55,
        BinOp::Lt | BinOp::LtEq | BinOp::Gt | BinOp::GtEq => 50,
        BinOp::Eq | BinOp::NotEq | BinOp::StrictEq | BinOp::StrictNotEq | BinOp::Spaceship => 45,
        BinOp::BitAnd => 40,
        BinOp::BitXor => 38,
        BinOp::BitOr => 36,
        BinOp::And => 34,
        BinOp::Or => 32,
        BinOp::NullCoalesce => 30,
        BinOp::Xor => 28,
    }
}

/// The unparenthesized source for `value`, with the strength it binds at.
fn render(value: &Expr) -> (String, u8) {
    match &value.kind {
        ExprKind::Null => ("null".to_string(), ATOM),
        ExprKind::This => ("$this".to_string(), ATOM),
        ExprKind::BoolLiteral(flag) => (flag.to_string(), ATOM),
        // `i64::MIN` has no decimal spelling: PHP has no negative integer TOKEN, so
        // `-9223372036854775808` lexes as a negation of a magnitude that does not fit an int and
        // comes back a float. `PHP_INT_MIN` is the spelling the lexer folds to this very literal.
        ExprKind::IntLiteral(i64::MIN) => ("PHP_INT_MIN".to_string(), ATOM),
        ExprKind::IntLiteral(number) => (number.to_string(), ATOM),
        ExprKind::FloatLiteral(number) => {
            let rendered = format!("{:?}", number);
            let text =
                if rendered.contains('.') || rendered.contains('e') || rendered.contains("inf") {
                    rendered
                } else {
                    format!("{rendered}.0")
                };
            (text, ATOM)
        }
        ExprKind::StringLiteral(text) => (string_literal(text), ATOM),
        ExprKind::Variable(name) => (format!("${name}"), ATOM),
        ExprKind::ConstRef(name) => (name_source(name), ATOM),
        ExprKind::PostIncrement(name) => (format!("${name}++"), ATOM),
        ExprKind::Negate(inner) => (format!("-{}", at(inner, 85)), 85),
        ExprKind::Not(inner) => (format!("!{}", at(inner, 75)), 75),
        ExprKind::Cast { target, expr } => {
            (format!("({}) {}", cast_type(target), at(expr, 85)), 85)
        }
        ExprKind::BinaryOp { left, op, right } => {
            let precedence = operator_precedence(op);
            // `**` and `??` associate to the RIGHT; everything else to the left.
            let right_associative = matches!(op, BinOp::Pow | BinOp::NullCoalesce);
            let (left_required, right_required) = if right_associative {
                (precedence + 1, precedence)
            } else {
                (precedence, precedence + 1)
            };
            (
                format!(
                    "{} {} {}",
                    at(left, left_required),
                    bin_op(op),
                    at(right, right_required)
                ),
                precedence,
            )
        }
        ExprKind::NullCoalesce { value, default } => {
            (format!("{} ?? {}", at(value, 31), at(default, 30)), 30)
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => (
            format!(
                "{} ? {} : {}",
                at(condition, 26),
                at(then_expr, 26),
                at(else_expr, 26)
            ),
            25,
        ),
        ExprKind::ArrayAccess { array, index } => {
            (format!("{}[{}]", at(array, ATOM), expression(index)), ATOM)
        }
        ExprKind::ArrayLiteral(items) => {
            let rendered: Vec<String> = items.iter().map(expression).collect();
            (format!("[{}]", rendered.join(", ")), ATOM)
        }
        ExprKind::ArrayLiteralAssoc(entries) => {
            let rendered: Vec<String> = entries
                .iter()
                .map(|(key, item)| format!("{} => {}", expression(key), expression(item)))
                .collect();
            (format!("[{}]", rendered.join(", ")), ATOM)
        }
        ExprKind::StaticPropertyAccess { receiver, property } => {
            (format!("{}::${property}", static_receiver(receiver)), ATOM)
        }
        ExprKind::PropertyAccess { object, property } => {
            (format!("{}->{property}", at(object, ATOM)), ATOM)
        }
        ExprKind::DynamicPropertyAccess { object, property } => (
            format!("{}->{{{}}}", at(object, ATOM), expression(property)),
            ATOM,
        ),
        ExprKind::Assignment { target, value, .. } => {
            (format!("{} = {}", at(target, ATOM), at(value, 20)), 20)
        }
        ExprKind::FunctionCall { name, args } => (
            format!("{}({})", name_source(name), argument_list(args)),
            ATOM,
        ),
        ExprKind::MethodCall {
            object,
            method,
            args,
        } => (
            format!("{}->{method}({})", at(object, ATOM), argument_list(args)),
            ATOM,
        ),
        ExprKind::StaticMethodCall {
            receiver,
            method,
            args,
        } => (
            format!(
                "{}::{method}({})",
                static_receiver(receiver),
                argument_list(args)
            ),
            ATOM,
        ),
        ExprKind::NewObject { class_name, args } => (
            format!("new {}({})", name_source(class_name), argument_list(args)),
            ATOM,
        ),
        ExprKind::NewScopedObject { receiver, args } => (
            format!("new {}({})", static_receiver(receiver), argument_list(args)),
            ATOM,
        ),
        ExprKind::NewDynamic { name_expr, args } => (
            format!("new {}({})", at(name_expr, ATOM), argument_list(args)),
            ATOM,
        ),
        ExprKind::ScopedConstantAccess { receiver, name } => {
            (format!("{}::{name}", static_receiver(receiver)), ATOM)
        }
        // `::class` is its own node, not a `ScopedConstantAccess` named "class" — the parser
        // never produces the latter for it, so printing it that way would reparse differently.
        ExprKind::ClassConstant { receiver } => {
            (format!("{}::class", static_receiver(receiver)), ATOM)
        }
        ExprKind::ClosureCall { var, args } => {
            (format!("${var}({})", argument_list(args)), ATOM)
        }
        ExprKind::Spread(value) => (format!("...{}", at(value, ATOM)), ATOM),
        ExprKind::Closure {
            params,
            return_type,
            body,
            captures,
            capture_refs,
            ..
        } => {
            // `captures` holds EVERY captured name and `capture_refs` only the by-reference
            // ones — a `use (&$v)` appears in both. Concatenating the two lists prints `$v`
            // twice, which reparses as two captures; the round-trip is what said so.
            let uses: Vec<String> = captures
                .iter()
                .map(|name| {
                    if capture_refs.contains(name) {
                        format!("&${name}")
                    } else {
                        format!("${name}")
                    }
                })
                .collect();
            let use_clause = if uses.is_empty() {
                String::new()
            } else {
                format!(" use ({})", uses.join(", "))
            };
            (
                format!(
                    "function ({}){use_clause}{} {}",
                    parameter_list(params, &[]),
                    return_hint(return_type),
                    block(body, 0)
                ),
                ATOM,
            )
        }
        ExprKind::InstanceOf { value, target } => match target {
            crate::parser::ast::InstanceOfTarget::Name(name) => (
                format!("{} instanceof {}", at(value, 80), name_source(name)),
                80,
            ),
            other => panic!("print: unmodelled instanceof target {:?}", other),
        },
        other => panic!("print: unmodelled expression {:?}", truncate(other)),
    }
}

/// A `self::` / `Class::` receiver.
fn static_receiver(receiver: &StaticReceiver) -> String {
    match receiver {
        StaticReceiver::Named(name) => name_source(name),
        StaticReceiver::Self_ => "self".to_string(),
        StaticReceiver::Parent => "parent".to_string(),
        StaticReceiver::Static => "static".to_string(),
    }
}

/// A comma-separated argument list.
fn argument_list(args: &[Expr]) -> String {
    let rendered: Vec<String> = args.iter().map(expression).collect();
    rendered.join(", ")
}

/// A cast target as it is spelled in source.
fn cast_type(target: &CastType) -> &'static str {
    match target {
        CastType::Int => "int",
        CastType::Float => "float",
        CastType::String => "string",
        CastType::Bool => "bool",
        CastType::Array => "array",
    }
}

/// A binary operator as it is spelled in source.
fn bin_op(op: &BinOp) -> &'static str {
    match op {
        BinOp::Add => "+",
        BinOp::Sub => "-",
        BinOp::Mul => "*",
        BinOp::Div => "/",
        BinOp::Mod => "%",
        BinOp::Concat => ".",
        BinOp::Eq => "==",
        BinOp::NotEq => "!=",
        BinOp::StrictEq => "===",
        BinOp::StrictNotEq => "!==",
        BinOp::Lt => "<",
        BinOp::Gt => ">",
        BinOp::LtEq => "<=",
        BinOp::GtEq => ">=",
        BinOp::Pow => "**",
        BinOp::And => "&&",
        BinOp::Or => "||",
        BinOp::Xor => "xor",
        BinOp::BitAnd => "&",
        BinOp::BitOr => "|",
        BinOp::BitXor => "^",
        BinOp::ShiftLeft => "<<",
        BinOp::ShiftRight => ">>",
        BinOp::Spaceship => "<=>",
        BinOp::NullCoalesce => "??",
    }
}

/// Shortens a Debug rendering so a panic NAMES the offending node instead of dumping a whole
/// subtree — the same courtesy `transcribe` extends.
fn truncate(node: impl std::fmt::Debug) -> String {
    let rendered = format!("{:?}", node);
    if rendered.len() <= 200 {
        return rendered;
    }
    format!("{}…", &rendered[..200])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::web_prelude::PhpVersion;

    /// Removes span payloads so a printed-and-reparsed node compares against the built one on
    /// structure. Source positions necessarily differ — that is the whole point of printing.
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

    /// Prints `program`, re-parses the result, and asserts the two ASTs are the same one.
    fn round_trip(label: &str, program: &Program) {
        let source = format!("<?php\n{}", print_program(program));
        let tokens = crate::lexer::tokenize(&source)
            .unwrap_or_else(|error| panic!("{label}: printed source must tokenize: {error:?}"));
        let reparsed = crate::parser::parse_internal(&tokens)
            .unwrap_or_else(|error| panic!("{label}: printed source must parse: {error:?}"));
        assert_eq!(
            program.len(),
            reparsed.len(),
            "{label}: declaration count after the round trip"
        );
        for (built, parsed) in program.iter().zip(reparsed.iter()) {
            let name = match &built.kind {
                StmtKind::FunctionDecl { name, .. }
                | StmtKind::ClassDecl { name, .. }
                | StmtKind::ExternFunctionDecl { name, .. } => name.clone(),
                _ => "<statement>".to_string(),
            };
            assert_eq!(
                strip_spans(&format!("{:?}", built)),
                strip_spans(&format!("{:?}", parsed)),
                "{label}: {name} changed across the round trip"
            );
        }
    }

    /// THE PROPERTY THAT MAKES THIS TOOL USABLE AS AN ORACLE: printing a built prelude and
    /// parsing the result yields the SAME AST. Run over every prelude built in Rust, so a
    /// printer gap shows up as a failing round trip rather than as a test asserting about a
    /// program that was never built.
    #[test]
    fn printing_round_trips() {
        round_trip("hash", &crate::hash_prelude::hash_declarations());
        round_trip("tz", &crate::tz_prelude::tz_declarations());
        round_trip(
            "var_export",
            &crate::var_export_prelude::var_export_declarations(),
        );
        round_trip(
            "version",
            &crate::version_prelude::version_declarations(
                &["phpversion", "php_uname", "php_sapi_name"],
                PhpVersion::Php85,
            ),
        );
        round_trip("list_id", &crate::list_id_prelude::list_id_declarations());
        for version in PhpVersion::ALL {
            round_trip(
                "pdo",
                &crate::pdo_prelude::build::pdo_declarations(
                    version,
                    crate::pdo_prelude::OptionalDrivers::from_build_environment(),
                ),
            );
        }
        round_trip("image", &crate::image_prelude::image_declarations());
        round_trip(
            "opcache env helpers",
            &crate::opcache_prelude::env_override_declarations(),
        );
        round_trip(
            "opcache state helpers",
            &crate::opcache_prelude::build::state_helper_decls(),
        );
        // `PhpVersion::ALL` is an array of values upstream, not a slice of references, and it
        // now spans seven profiles rather than four — so this loop widened on its own.
        for version in PhpVersion::ALL {
            round_trip(
                "opcache ini helpers",
                &crate::opcache_prelude::ini_helper_declarations(version, &[]),
            );
            round_trip(
                "web",
                &crate::web_prelude::build::web_declarations(version, &[]),
            );
        }
        round_trip("web wrapper", &vec![crate::web_prelude::web_wrap_stmt()]);
    }
}
