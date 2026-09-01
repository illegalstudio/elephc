//! Purpose:
//! A Rust builder for the synthetic PHP declarations elephc injects itself (`ext-dom`, PDO,
//! the hash context, the session handlers, the image extensions). It expresses those
//! declarations as Rust data instead of PHP source text held in a `&'static str`.
//!
//! Called from:
//! - The `*_prelude` modules, which build their class and function surfaces through it
//!   instead of embedding PHP source text.
//!
//! Key details:
//! - Produces `StmtKind::ClassDecl` / `StmtKind::FunctionDecl` nodes DIRECTLY, so the result
//!   enters the pipeline at exactly the point a parsed prelude did: collected, checked,
//!   lowered and emitted like user code. That property is the whole reason the PHP-text form
//!   existed. A class registered straight into the checker's class map (a `FlattenedClass`)
//!   type-checks but never reaches lowering, so `new C()` dies at the backend with
//!   "constructor call to C::__construct without an emitted EIR method body" — see
//!   `codegen/lower_inst/objects.rs`. Building the AST keeps the method bodies real while
//!   removing the embedded PHP.
//! - EVERY builder emits the SAME node the parser emits for the equivalent PHP. That is not
//!   pedantry: statement-position assignment parses to `StmtKind::Assign`, not to
//!   `ExprStmt(ExprKind::Assignment)`, and `array` parses to `TypeExpr::Named("array")`, not
//!   to `TypeExpr::Array`. Picking the other shape yields an AST no parse could produce and
//!   sends later passes down a path the PHP form never took. `parser_agreement` pins the
//!   correspondence by parsing PHP and comparing.
//! - Declarations are built under `SourceMode::Internal` so they are exempt from the user
//!   strict-PHP audit, matching `parser::parse_internal` which parsed the PHP form. The
//!   thread-local defaults to `Internal`, so this only matters when injection runs inside an
//!   enclosing parse — but it is the difference between an exemption that holds by
//!   construction and one that holds by luck. Use `internal_declarations` to get it.
//! - Unused PARAMETERS are consumed by a synthesized `$_unused = …;` statement. The `$_`
//!   prefix is what exempts a name from the unused-variable warning, and an unconditionally
//!   injected prelude warns on EVERY compile — `<?php echo "hi";` included — for each
//!   parameter no body reads. Which parameters those are is DERIVED by scanning the built
//!   body rather than declared by hand, so the hazard is removed rather than transcribed.
//!   Parameters are NOT renamed to `$_name`: PHP named arguments make a parameter's name part
//!   of the public API, and a shell has no licence to rename `setAttribute(name:, value:)`.
//! - The scan (`reads_of`) covers the vocabulary these builders can produce. A variant it does
//!   not know is read as "no variable in there", so a missed read can only ADD a redundant
//!   `$_unused = $p;` — never leave a parameter unconsumed. Extend it alongside any new helper.
//! - Method-local variables should stay `$_`-prefixed for the same reason `pdo_prelude` does
//!   it: the checker resolves a method-body variable's type against top-level variables of the
//!   same name, so a user global named `$node` would otherwise clash with a plain method-local
//!   `$node`.
//! - Most nodes use `Span::dummy()` because a synthetic declaration has no source location.
//!   Free-function call nodes and loops that serve as checker/lowering map keys use distinct
//!   `Span::synthetic()` identities instead; those spans still identify generated nodes rather
//!   than PHP source.

#[cfg(test)]
pub mod transcribe;

#[cfg(test)]
pub mod print;

use crate::names::{Name, NameKind};
use crate::parser::ast::{
    Attribute, AttributeGroup, BinOp, CType, CastType, CatchClause, ClassConst, ClassMethod,
    ClassProperty, Expr, ExprKind, ExternParam, Program, PropertyHooks, StaticReceiver, Stmt,
    StmtKind, TypeExpr, Visibility,
};
use crate::source::{SourceMode, SourceProfile};
use crate::span::Span;

/// Builds a prelude's declarations under the internal source mode.
///
/// Every prelude entry point should wrap its construction in this. See the module header: the
/// PHP form reached `SourceMode::Internal` through `parse_internal`, and statements carry the
/// mode they were built under into name resolution and type checking.
pub fn internal_declarations(build: impl FnOnce() -> Program) -> Program {
    // `SourceProfile::new` pairs the mode with PHP's default coercive typing, which is right
    // here: a built prelude declares no `strict_types=1`, so it must be stamped the way an
    // undeclared file is rather than inheriting whatever the caller was parsing.
    crate::source::with_parse_mode(SourceProfile::new(SourceMode::Internal), build)
}

// ---------------------------------------------------------------------------
// Type helpers
// ---------------------------------------------------------------------------

/// PHP's `mixed`, which the parser models as an unqualified named type rather than a
/// dedicated `TypeExpr` variant (see `parser/stmt/params.rs`).
pub fn t_mixed() -> TypeExpr {
    TypeExpr::Named(Name::unqualified("mixed"))
}

/// PHP's untyped `array`. The parser produces a NAMED type for the bare keyword and reserves
/// `TypeExpr::Array` for element-typed forms, so this must not be `TypeExpr::Array`.
pub fn t_array() -> TypeExpr {
    TypeExpr::Named(Name::unqualified("array"))
}

/// A class or interface type by name (`DOMNode`, `Iterator`, …).
pub fn t_class(name: &str) -> TypeExpr {
    // Through `class_name`, so a type hint keeps the spelling it was written with. A `\`-
    // anchored hint inside a namespace names a DIFFERENT class from the bare one, and
    // `PDOStatement::getIterator(): \Iterator` relies on exactly that.
    TypeExpr::Named(class_name(name))
}

/// The `ptr` type — the raw address an `extern` boundary hands back. It carries no class, which
/// is the bare `ptr` the preludes spell.
pub fn t_ptr() -> TypeExpr {
    TypeExpr::Ptr(None)
}

/// The nullable form `?T`.
pub fn t_nullable(inner: TypeExpr) -> TypeExpr {
    TypeExpr::Nullable(Box::new(inner))
}

/// The union form `A|B`.
pub fn t_union(members: Vec<TypeExpr>) -> TypeExpr {
    TypeExpr::Union(members)
}

/// A class name AS WRITTEN: a leading `\` marks it root-anchored, anything else is unqualified.
///
/// `\Throwable` and `Throwable` are different nodes, and a prelude spells the first when it must
/// not be captured by a namespace. Taking the spelling from the string keeps the two forms one
/// argument apart instead of one helper apart.
///
/// SEPARATORS SPLIT INTO PARTS, because that is what the parser produces: `\Pdo\Sqlite` is two
/// parts, not one part that happens to contain a backslash. The two render the same text and
/// compare UNEQUAL, so a name built the second way silently fails to match its own source.
pub fn class_name(name: &str) -> Name {
    match name.strip_prefix('\\') {
        Some(rest) => Name::from_parts(NameKind::FullyQualified, name_parts(rest)),
        None => {
            let parts = name_parts(name);
            if parts.len() > 1 {
                Name::from_parts(NameKind::Qualified, parts)
            } else {
                Name::unqualified(name)
            }
        }
    }
}

/// A root-anchored name (`\Exception`), as written with a leading separator.
pub fn fq_name(name: &str) -> Name {
    Name::from_parts(NameKind::FullyQualified, name_parts(name))
}

/// Any PHP name, spelled as written — class, interface or FUNCTION.
///
/// The rule is not specific to classes. Inside a namespace a call to an unqualified function
/// does NOT fall back to the global one the way a class reference does, so the PDO prelude
/// spells `\elephc_pdo_load_extension()` and dropping the `\` would resolve it to
/// `Pdo\elephc_pdo_load_extension` — a function that does not exist.
///
/// Named apart from `class_name` only because callers here hold a parameter of that name.
fn class_name_spelled(name: &str) -> Name {
    class_name(name)
}

/// `#[Name(args…)]` — one attribute group holding one attribute.
///
/// PHP allows several attributes per group and several groups per declaration; the preludes
/// only ever write one of each, so this covers them without inviting a shape they do not use.
pub fn attr(name: &str, args: Vec<Expr>) -> AttributeGroup {
    AttributeGroup {
        attributes: vec![Attribute {
            name: class_name(name),
            args,
            span: Span::dummy(),
        }],
        span: Span::dummy(),
    }
}

/// Splits a namespaced spelling into the parts the parser would produce.
fn name_parts(name: &str) -> Vec<String> {
    name.split('\\')
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect()
}

// ---------------------------------------------------------------------------
// Expression helpers
// ---------------------------------------------------------------------------

/// The `null` literal.
pub fn e_null() -> Expr {
    Expr::new(ExprKind::Null, Span::dummy())
}

/// A `true`/`false` literal.
pub fn e_bool(value: bool) -> Expr {
    Expr::new(ExprKind::BoolLiteral(value), Span::dummy())
}

/// An integer literal.
pub fn e_int(value: i64) -> Expr {
    Expr::int_lit(value)
}

/// A string literal.
pub fn e_str(value: &str) -> Expr {
    Expr::string_lit(value)
}

/// A variable read, written WITHOUT the leading `$`.
pub fn e_var(name: &str) -> Expr {
    Expr::var(name)
}

/// `$this`.
pub fn e_this() -> Expr {
    Expr::new(ExprKind::This, Span::dummy())
}

/// `new C(args)`.
pub fn e_new(class_name: &str, args: Vec<Expr>) -> Expr {
    Expr::new(
        ExprKind::NewObject {
            class_name: class_name_spelled(class_name),
            args,
        },
        Span::dummy(),
    )
}

/// `new \C(args)` for a root-anchored class such as `\Exception`.
pub fn e_new_fq(class_name: &str, args: Vec<Expr>) -> Expr {
    Expr::new(
        ExprKind::NewObject {
            class_name: fq_name(class_name),
            args,
        },
        Span::dummy(),
    )
}

/// `new $var(args)` — the class is named by a runtime string expression.
pub fn e_new_dynamic(name_expr: Expr, args: Vec<Expr>) -> Expr {
    Expr::new(
        ExprKind::NewDynamic {
            name_expr: Box::new(name_expr),
            args,
        },
        Span::dummy(),
    )
}

/// `new self(args)`. Distinct from `e_new`: late-bound receivers go through
/// `NewScopedObject`, which is what the parser produces for the `self` keyword.
pub fn e_new_self(args: Vec<Expr>) -> Expr {
    Expr::new(
        ExprKind::NewScopedObject {
            receiver: StaticReceiver::Self_,
            args,
        },
        Span::dummy(),
    )
}

/// `new static(<args>)` — late static binding: the CALLED class, not the declaring one.
///
/// `PDO::connect()` returns `static`, so a `Pdo\Sqlite::connect()` must produce a
/// `Pdo\Sqlite` from a constructor written in `PDO`. `new self()` there would build a `PDO`.
pub fn e_new_static(args: Vec<Expr>) -> Expr {
    Expr::new(
        ExprKind::NewScopedObject {
            receiver: StaticReceiver::Static,
            args,
        },
        Span::dummy(),
    )
}

/// `f(args)` — a free function call.
pub fn e_call(name: &str, args: Vec<Expr>) -> Expr {
    Expr::new(
        ExprKind::FunctionCall {
            name: class_name_spelled(name),
            args,
        },
        // A DISTINCT SPAN, because a span is a map key here: the checker files every builtin
        // call's inferred type in `CheckResult::builtin_call_types` keyed by the CALL's span,
        // and lowering reads it back. With `dummy()` on all of them, one key holds whichever
        // call wrote last, so lowering cannot identify a call and falls back to the builtin's
        // DECLARED return type.
        //
        // It is NOT what makes that safe — the declarations are. Six builtins declared `Mixed`
        // while their check hook returned `Pointer` or `Callable`, and the fallback handed
        // codegen a boxed cell for a raw descriptor. `TypeSpec::Ptr` and `TypeSpec::Callable`
        // let them declare what they return, and the difference was measured rather than
        // assumed: with `dummy()` put back, all 276 PDO tests pass. The first measurement,
        // taken with four of the six fixed, left 25 failures and made this look like a
        // precision mechanism; the 25 were the same defect in the two builtins not yet found.
        //
        // What it still buys is OBSERVABILITY. `resolve_registry_builtin_result_type` asserts
        // that a builtin's declared type is a representation-compatible stand-in for what the
        // checker inferred — and that assertion can only compare the two where the checked type
        // is findable. Under `dummy()` the map is skipped for every prelude call, so the next
        // builtin to declare a type of the wrong shape would go unwitnessed exactly here.
        //
        // Deliberately NOT applied to every builder. Three places read `span.line == 0` as
        // "synthetic": a `ConstNull` the fiber lowering recognises that way, the
        // statement-boundary concat reset it uses to skip prelude statements, and a
        // throwable's `getLine()`, which would start reporting a seven-digit line. Only the
        // call node keys the map, so only the call node moves.
        Span::synthetic(),
    )
}

/// `C::m(args)` — a static call on a named class.
pub fn e_static_call(class_name: &str, method_name: &str, args: Vec<Expr>) -> Expr {
    Expr::new(
        ExprKind::StaticMethodCall {
            receiver: StaticReceiver::Named(class_name_spelled(class_name)),
            method: method_name.to_string(),
            args,
        },
        Span::dummy(),
    )
}

/// `self::m(args)`.
pub fn e_self_call(method_name: &str, args: Vec<Expr>) -> Expr {
    Expr::new(
        ExprKind::StaticMethodCall {
            receiver: StaticReceiver::Self_,
            method: method_name.to_string(),
            args,
        },
        Span::dummy(),
    )
}

/// `$object->m(args)`.
pub fn e_method_call(object: Expr, method_name: &str, args: Vec<Expr>) -> Expr {
    Expr::new(
        ExprKind::MethodCall {
            object: Box::new(object),
            method: method_name.to_string(),
            args,
        },
        Span::dummy(),
    )
}

/// `name: value` inside an argument list or attribute argument list.
pub fn e_named_arg(name: &str, value: Expr) -> Expr {
    Expr::new(
        ExprKind::NamedArg {
            name: name.to_string(),
            value: Box::new(value),
        },
        Span::dummy(),
    )
}

/// `left ?? right`.
pub fn e_null_coalesce(value: Expr, default: Expr) -> Expr {
    Expr::new(
        ExprKind::NullCoalesce {
            value: Box::new(value),
            default: Box::new(default),
        },
        Span::dummy(),
    )
}

/// `$object->prop`.
pub fn e_prop(object: Expr, property: &str) -> Expr {
    Expr::new(
        ExprKind::PropertyAccess {
            object: Box::new(object),
            property: property.to_string(),
        },
        Span::dummy(),
    )
}

/// `$object::class`.
pub fn e_object_class_name(object: Expr) -> Expr {
    Expr::new(
        ExprKind::ObjectClassName {
            object: Box::new(object),
        },
        Span::dummy(),
    )
}

/// `$this->prop`.
pub fn e_this_prop(property: &str) -> Expr {
    e_prop(e_this(), property)
}

/// `$object->$name` — a property named by a runtime expression.
pub fn e_dyn_prop(object: Expr, property: Expr) -> Expr {
    Expr::new(
        ExprKind::DynamicPropertyAccess {
            object: Box::new(object),
            property: Box::new(property),
        },
        Span::dummy(),
    )
}

/// `target = value` in EXPRESSION position.
///
/// Statement-position assignment to a plain variable or a named property has its own
/// `StmtKind` (`s_assign`, `s_prop_assign`); this is the general form the parser falls back to
/// for targets those do not cover, such as a dynamic property.
pub fn e_assign(target: Expr, value: Expr) -> Expr {
    Expr::new(
        ExprKind::Assignment {
            target: Box::new(target),
            value: Box::new(value),
            result_target: None,
            prelude: Vec::new(),
            conditional_value_temp: None,
        },
        Span::dummy(),
    )
}

/// A reference to a global constant (`PHP_INT_MIN`, `STDERR`, …).
pub fn e_const(name: &str) -> Expr {
    Expr::new(
        ExprKind::ConstRef(Name::unqualified(name)),
        Span::dummy(),
    )
}

/// A float literal.
pub fn e_float(value: f64) -> Expr {
    Expr::float_lit(value)
}

/// A binary operation `left op right`.
pub fn e_binop(left: Expr, op: BinOp, right: Expr) -> Expr {
    Expr::binop(left, op, right)
}

/// `left . right` — PHP string concatenation.
///
/// A DOUBLE-QUOTED INTERPOLATED STRING IS BUILT WITH THIS. Interpolation is resolved by the
/// LEXER (`lexer/literals/strings.rs`), which emits the parts as an ordinary concatenation
/// token stream, so `"a{$b}c"` reaches the AST as `'a' . $b . 'c'` and has no node of its own.
pub fn e_concat(left: Expr, right: Expr) -> Expr {
    e_binop(left, BinOp::Concat, right)
}

/// Folds `parts` left-to-right with `.`, the shape the lexer produces for an interpolated
/// string. Panics on an empty list, which would have no expression to stand for.
pub fn e_concat_all(parts: Vec<Expr>) -> Expr {
    let mut parts = parts.into_iter();
    let first = parts.next().expect("a concatenation needs at least one part");
    parts.fold(first, e_concat)
}

/// `C::NAME` — a user-declared class constant read.
pub fn e_class_const(class_name: &str, name: &str) -> Expr {
    Expr::new(
        ExprKind::ScopedConstantAccess {
            receiver: StaticReceiver::Named(class_name_spelled(class_name)),
            name: name.to_string(),
        },
        Span::dummy(),
    )
}

/// `self::NAME`.
pub fn e_self_const(name: &str) -> Expr {
    Expr::new(
        ExprKind::ScopedConstantAccess {
            receiver: StaticReceiver::Self_,
            name: name.to_string(),
        },
        Span::dummy(),
    )
}

/// `value instanceof C`.
pub fn e_instance_of(value: Expr, class_name: &str) -> Expr {
    Expr::instance_of(value, class_name_spelled(class_name))
}

/// `!value`.
pub fn e_not(value: Expr) -> Expr {
    Expr::new(ExprKind::Not(Box::new(value)), Span::dummy())
}

/// `@value` error suppression.
pub fn e_error_suppress(value: Expr) -> Expr {
    Expr::new(ExprKind::ErrorSuppress(Box::new(value)), Span::dummy())
}

/// `clone value`.
pub fn e_clone(value: Expr) -> Expr {
    Expr::new(ExprKind::Clone(Box::new(value)), Span::dummy())
}

/// `-value`, the unary minus.
///
/// A NEGATIVE LITERAL IS THIS TOO. The parser does NOT fold `-3` into `IntLiteral(-3)`; it
/// emits `Negate(IntLiteral(3))`, so a negative literal in a converted prelude must be
/// `e_neg(e_int(3))`. The exception is `PHP_INT_MIN`/`PHP_INT_MAX`, which are dedicated lexer
/// tokens carrying the value directly and really are plain `e_int` literals.
pub fn e_neg(value: Expr) -> Expr {
    Expr::negate(value)
}

/// `condition ? then_value : else_value`.
pub fn e_ternary(condition: Expr, then_value: Expr, else_value: Expr) -> Expr {
    Expr::new(
        ExprKind::Ternary {
            condition: Box::new(condition),
            then_expr: Box::new(then_value),
            else_expr: Box::new(else_value),
        },
        Span::dummy(),
    )
}

/// `(int) value`, `(string) value`, … — a scalar cast.
pub fn e_cast(target: CastType, value: Expr) -> Expr {
    Expr::new(
        ExprKind::Cast {
            target,
            expr: Box::new(value),
        },
        Span::dummy(),
    )
}

/// `$array[$index]`. Also the form for indexing a STRING (`$s[0]`), which PHP spells the same.
pub fn e_index(array: Expr, index: Expr) -> Expr {
    Expr::new(
        ExprKind::ArrayAccess {
            array: Box::new(array),
            index: Box::new(index),
        },
        Span::dummy(),
    )
}

/// `$name++` in expression position.
pub fn e_post_inc(name: &str) -> Expr {
    Expr::new(ExprKind::PostIncrement(name.to_string()), Span::dummy())
}

/// A list array literal `[a, b, c]`.
pub fn e_array(items: Vec<Expr>) -> Expr {
    Expr::new(ExprKind::ArrayLiteral(items), Span::dummy())
}

/// A keyed array literal `['k' => v, …]`.
pub fn e_array_assoc(entries: Vec<(Expr, Expr)>) -> Expr {
    Expr::new(ExprKind::ArrayLiteralAssoc(entries), Span::dummy())
}

// ---------------------------------------------------------------------------
// Statement helpers
// ---------------------------------------------------------------------------

/// `namespace <name> { <body> }` — the BRACED form, not the `namespace X;` declaration.
///
/// The two are different nodes and the PDO prelude needs this one specifically: a
/// `namespace Pdo;` declaration would apply to every statement after it, including the global
/// classes the prelude goes on to declare. The braced form scopes to its own body.
///
/// Inside it, a call to a global function does NOT fall back to the global namespace the way
/// a class reference does, so bodies built here spell their builtins fully qualified
/// (`\is_callable`, `\elephc_pdo_open`) exactly as the PHP source does.
pub fn s_namespace(name: &str, body: Vec<Stmt>) -> Stmt {
    Stmt::new(
        StmtKind::NamespaceBlock {
            name: Some(class_name(name)),
            body,
        },
        Span::dummy(),
    )
}

/// `return <value>;`
pub fn s_return(value: Expr) -> Stmt {
    Stmt::new(StmtKind::Return(Some(value)), Span::dummy())
}

/// A bare `return;`.
pub fn s_return_void() -> Stmt {
    Stmt::new(StmtKind::Return(None), Span::dummy())
}

/// `$name = <value>;` in statement position.
///
/// This is `StmtKind::Assign`, which is what the parser produces — statement-position
/// assignment does NOT go through `ExprStmt(ExprKind::Assignment)`.
pub fn s_assign(name: &str, value: Expr) -> Stmt {
    Stmt::new(
        StmtKind::Assign {
            name: name.to_string(),
            value,
        },
        Span::dummy(),
    )
}

/// `callable $name = <value>;` — an assignment carrying an explicit type on the LOCAL.
///
/// elephc-PHP only; reference PHP has no typed local. The PDO prelude uses it to pin a
/// closure-valued local as `callable` so the checker keeps it callable across the assignment
/// instead of widening it, which is what makes the property-hook setters dispatch.
pub fn s_typed_assign(ty: TypeExpr, name: &str, value: Expr) -> Stmt {
    Stmt::new(
        StmtKind::TypedAssign {
            type_expr: ty,
            name: name.to_string(),
            value,
        },
        Span::dummy(),
    )
}

/// `$object->prop[] = <value>;` in statement position.
pub fn s_prop_array_push(object: Expr, property: &str, value: Expr) -> Stmt {
    Stmt::new(
        StmtKind::PropertyArrayPush {
            object: Box::new(object),
            property: property.to_string(),
            value,
        },
        Span::dummy(),
    )
}

/// `$object->prop[$index] = <value>;` in statement position.
pub fn s_prop_array_assign(object: Expr, property: &str, index: Expr, value: Expr) -> Stmt {
    Stmt::new(
        StmtKind::PropertyArrayAssign {
            object: Box::new(object),
            property: property.to_string(),
            index,
            value,
        },
        Span::dummy(),
    )
}

/// `Class::$prop` — a static property read.
pub fn e_static_prop(class_name: &str, property: &str) -> Expr {
    Expr::new(
        ExprKind::StaticPropertyAccess {
            receiver: StaticReceiver::Named(class_name_spelled(class_name)),
            property: property.to_string(),
        },
        Span::dummy(),
    )
}

/// `self::$prop` — a static property read through the ENCLOSING class.
///
/// Not interchangeable with `e_static_prop` naming the same class: `self::` binds at compile
/// time to where the method is written, so a subclass that inherits the method still reads the
/// declaring class's slot. The PDO prelude relies on that for its per-class registration
/// latches, which is why the receiver is preserved rather than spelled out.
pub fn e_self_static_prop(property: &str) -> Expr {
    Expr::new(
        ExprKind::StaticPropertyAccess {
            receiver: StaticReceiver::Self_,
            property: property.to_string(),
        },
        Span::dummy(),
    )
}

/// `self::$prop = <value>;` in statement position.
pub fn s_self_static_prop_assign(property: &str, value: Expr) -> Stmt {
    Stmt::new(
        StmtKind::StaticPropertyAssign {
            receiver: StaticReceiver::Self_,
            property: property.to_string(),
            value,
        },
        Span::dummy(),
    )
}

/// `...$args` in an argument list — unpacking an array into positional arguments.
///
/// An argument-position node, not an operator on the value: it says how the callee receives
/// the array, which is why it wraps the expression rather than transforming it.
pub fn e_spread(value: Expr) -> Expr {
    Expr::new(ExprKind::Spread(Box::new(value)), Span::dummy())
}

/// `$closure(<args>)` — calling a closure held in a variable.
///
/// A distinct node from a named call and from `call_user_func`: the callee is the variable
/// itself, so nothing resolves a function name here.
pub fn e_closure_call(var: &str, args: Vec<Expr>) -> Expr {
    Expr::new(
        ExprKind::ClosureCall {
            var: var.to_string(),
            args,
        },
        Span::dummy(),
    )
}

/// Builder for a closure literal (`function (int $x) use (&$y): void { … }`).
///
/// Parameters and body mirror `MethodBuilder`, but captures do not: PHP distinguishes
/// `use ($v)` from `use (&$v)` and the AST keeps them in separate lists, because the first
/// copies at closure-creation time and the second aliases the caller's variable for the
/// closure's whole life. The PDO prelude's property-hook getters and setters depend on the
/// by-reference form, so the two are separate calls rather than one with a flag.
pub struct ClosureBuilder {
    params: Vec<(String, Option<TypeExpr>, Option<Expr>, bool)>,
    return_type: Option<TypeExpr>,
    body: Vec<Stmt>,
    captures: Vec<String>,
    capture_refs: Vec<String>,
}

/// Starts a closure literal with no parameters, no captures and an empty body.
pub fn closure() -> ClosureBuilder {
    ClosureBuilder {
        params: Vec::new(),
        return_type: None,
        body: Vec::new(),
        captures: Vec::new(),
        capture_refs: Vec::new(),
    }
}

impl ClosureBuilder {
    /// Appends a typed parameter.
    pub fn param(mut self, name: &str, ty: TypeExpr) -> Self {
        self.params.push((name.to_string(), Some(ty), None, false));
        self
    }

    /// Appends an UNTYPED parameter.
    pub fn param_untyped(mut self, name: &str) -> Self {
        self.params.push((name.to_string(), None, None, false));
        self
    }

    /// Appends a typed parameter carrying a default.
    pub fn param_default(mut self, name: &str, ty: TypeExpr, default: Expr) -> Self {
        self.params
            .push((name.to_string(), Some(ty), Some(default), false));
        self
    }

    /// Captures a variable BY VALUE (`use ($v)`).
    pub fn captures(mut self, name: &str) -> Self {
        self.captures.push(name.to_string());
        self
    }

    /// Captures a variable BY REFERENCE (`use (&$v)`).
    ///
    /// Lands in BOTH lists, because that is the shape the parser produces: `captures` is every
    /// captured name and `capture_refs` marks which of them alias. Pushing only to the second
    /// builds a closure the source could not have written.
    pub fn captures_ref(mut self, name: &str) -> Self {
        self.captures.push(name.to_string());
        self.capture_refs.push(name.to_string());
        self
    }

    /// Sets the declared return type.
    pub fn returns(mut self, ty: TypeExpr) -> Self {
        self.return_type = Some(ty);
        self
    }

    /// Sets the body.
    pub fn body(mut self, body: Vec<Stmt>) -> Self {
        self.body = body;
        self
    }

    /// Builds the closure expression.
    pub fn build(self) -> Expr {
        Expr::new(
            ExprKind::Closure {
                params: self.params,
                variadic: None,
                variadic_by_ref: false,
                variadic_type: None,
                return_type: self.return_type,
                body: self.body,
                is_arrow: false,
                is_static: false,
                by_ref_return: false,
                captures: self.captures,
                capture_refs: self.capture_refs,
            },
            Span::dummy(),
        )
    }
}

/// `self::class` / `static::class` / `Foo::class` — the class NAME as a string.
///
/// The three are genuinely different values, not spellings of one: `self::class` is the class
/// the method is written in, `static::class` is the class the call was made on (late static
/// binding), and a named receiver is neither. The PDO prelude uses all three within a few
/// lines of each other to build its "called class" diagnostics, so the receiver is a parameter
/// rather than three helpers.
pub fn e_class_name_of(receiver: StaticReceiver) -> Expr {
    Expr::new(ExprKind::ClassConstant { receiver }, Span::dummy())
}

/// `self::class`.
pub fn e_self_class() -> Expr {
    e_class_name_of(StaticReceiver::Self_)
}

/// `static::class` — the CALLED class, not the declaring one.
pub fn e_static_class() -> Expr {
    e_class_name_of(StaticReceiver::Static)
}

/// `Foo::class`.
pub fn e_named_class(class_name_text: &str) -> Expr {
    e_class_name_of(StaticReceiver::Named(class_name(class_name_text)))
}

/// `parent::<method>(<args>)` — an explicit call up the inheritance chain.
///
/// The whole point is that it does NOT dispatch: `parent::__construct()` runs the parent's
/// body even when the object is a subclass that overrides it, which is how the namespaced
/// `Pdo\Sqlite` drivers reuse `PDO`'s constructor.
pub fn e_parent_call(method_name: &str, args: Vec<Expr>) -> Expr {
    Expr::new(
        ExprKind::StaticMethodCall {
            receiver: StaticReceiver::Parent,
            method: method_name.to_string(),
            args,
        },
        Span::dummy(),
    )
}

/// `Class::$prop = <value>;` in statement position.
pub fn s_static_prop_assign(class_name: &str, property: &str, value: Expr) -> Stmt {
    Stmt::new(
        StmtKind::StaticPropertyAssign {
            receiver: StaticReceiver::Named(class_name_spelled(class_name)),
            property: property.to_string(),
            value,
        },
        Span::dummy(),
    )
}

/// `Class::$prop[<index>] = <value>;` in statement position.
pub fn s_static_prop_array_assign(
    class_name: &str,
    property: &str,
    index: Expr,
    value: Expr,
) -> Stmt {
    Stmt::new(
        StmtKind::StaticPropertyArrayAssign {
            receiver: StaticReceiver::Named(class_name_spelled(class_name)),
            property: property.to_string(),
            index,
            value,
        },
        Span::dummy(),
    )
}

/// `Class::$prop[] = <value>;` in statement position.
pub fn s_static_prop_array_push(class_name: &str, property: &str, value: Expr) -> Stmt {
    Stmt::new(
        StmtKind::StaticPropertyArrayPush {
            receiver: StaticReceiver::Named(class_name_spelled(class_name)),
            property: property.to_string(),
            value,
        },
        Span::dummy(),
    )
}

/// `$object->prop = <value>;` in statement position.
pub fn s_prop_assign(object: Expr, property: &str, value: Expr) -> Stmt {
    Stmt::new(
        StmtKind::PropertyAssign {
            object: Box::new(object),
            property: property.to_string(),
            value,
        },
        Span::dummy(),
    )
}

/// `static $name = <init>;` — a function-local whose value survives across calls.
///
/// The initializer runs once, on the first call, so `init` must be a constant expression;
/// that is a PHP rule, not a builder one, and the parser accepts the same shapes.
pub fn s_static(name: &str, init: Expr) -> Stmt {
    Stmt::new(
        StmtKind::StaticVar {
            name: name.to_string(),
            init,
        },
        Span::dummy(),
    )
}

/// A top-level `const NAME = value;` declaration.
pub fn s_const(name: &str, value: Expr) -> Stmt {
    Stmt::new(
        StmtKind::ConstDecl {
            name: name.to_string(),
            value,
        },
        Span::dummy(),
    )
}

/// `throw <value>;`
pub fn s_throw(value: Expr) -> Stmt {
    Stmt::new(StmtKind::Throw(value), Span::dummy())
}

/// An expression evaluated for its effect.
pub fn s_expr(value: Expr) -> Stmt {
    Stmt::new(StmtKind::ExprStmt(value), Span::dummy())
}

/// `echo <value>;`
pub fn s_echo(value: Expr) -> Stmt {
    Stmt::new(StmtKind::Echo(value), Span::dummy())
}

/// `$array[] = <value>;` where `array` is a plain variable name.
pub fn s_array_push(array: &str, value: Expr) -> Stmt {
    Stmt::new(
        StmtKind::ArrayPush {
            array: array.to_string(),
            value,
        },
        Span::dummy(),
    )
}

/// `$array[$index] = <value>;` where `array` is a plain variable name.
pub fn s_array_assign(array: &str, index: Expr, value: Expr) -> Stmt {
    Stmt::new(
        StmtKind::ArrayAssign {
            array: array.to_string(),
            index,
            value,
        },
        Span::dummy(),
    )
}

/// `if (…) { … } elseif (…) { … } else { … }`.
///
/// `elseif_clauses` is for the ONE-WORD `elseif` keyword. PHP's two-word `else if` is a
/// different tree: it parses as an `if` nested inside the `else` body, so transcribing an
/// `else if` chain as `elseif_clauses` builds an AST the original source never produced.
/// `s_else_if` writes the nested form.
pub fn s_if(
    condition: Expr,
    then_body: Vec<Stmt>,
    elseif_clauses: Vec<(Expr, Vec<Stmt>)>,
    else_body: Option<Vec<Stmt>>,
) -> Stmt {
    Stmt::new(
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        },
        Span::dummy(),
    )
}

/// A two-word `else if` chain: `if (c0) { b0 } else if (c1) { b1 } … else { fallback }`.
///
/// Each arm after the first nests inside the previous `else`, which is what the parser
/// produces for `else if` — unlike the one-word `elseif`, which fills `elseif_clauses`.
/// Panics on an empty arm list, which would have no condition to test.
pub fn s_else_if(arms: Vec<(Expr, Vec<Stmt>)>, fallback: Option<Vec<Stmt>>) -> Stmt {
    let mut arms = arms;
    let (condition, body) = arms.remove(0);
    let tail = if arms.is_empty() {
        fallback
    } else {
        Some(vec![s_else_if(arms, fallback)])
    };
    s_if(condition, body, vec![], tail)
}

/// `for (init; condition; update) { body }`.
///
/// The parser canonicalizes simple local-variable assignment clauses to `StmtKind::Assign`,
/// while preserving genuinely expression-shaped clauses such as post-increment.
pub fn s_for(
    init: Option<Stmt>,
    condition: Option<Expr>,
    update: Option<Stmt>,
    body: Vec<Stmt>,
) -> Stmt {
    Stmt::new(
        StmtKind::For {
            init: init.map(Box::new),
            condition,
            update: update.map(Box::new),
            body,
        },
        // A loop span is a KEY: `loop_storage_types` is `(scope, loop span) -> contracts`, filed
        // by the checker and read back by lowering. Measured on one `new PDO(...)` program, the
        // shared `dummy()` made 26 loops inherit another loop's contracts, and all 26 differed
        // from what they would have computed — one got two contracts where it needed none.
        Span::synthetic(),
    )
}

/// `foreach ($array as [$key =>] $value) { body }`.
pub fn s_foreach(array: Expr, key_var: Option<&str>, value_var: &str, body: Vec<Stmt>) -> Stmt {
    Stmt::new(
        StmtKind::Foreach {
            array,
            key_var: key_var.map(str::to_string),
            value_var: value_var.to_string(),
            value_by_ref: false,
            body,
        },
        Span::synthetic(), // a loop span keys `loop_storage_types`; see `s_for`
    )
}

/// `try { … } catch (T $e) { … } finally { … }`.
///
/// Each catch names its exception types and, optionally, the variable they bind to — PHP 8
/// allows `catch (T)` with no variable.
pub fn s_try(
    try_body: Vec<Stmt>,
    catches: Vec<(Vec<&str>, Option<&str>, Vec<Stmt>)>,
    finally_body: Option<Vec<Stmt>>,
) -> Stmt {
    let catches = catches
        .into_iter()
        .map(|(types, variable, body)| CatchClause {
            exception_types: types.into_iter().map(class_name).collect(),
            variable: variable.map(str::to_string),
            body,
        })
        .collect();
    Stmt::new(
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        },
        Span::dummy(),
    )
}

/// `switch (subject) { case …: … default: … }`.
///
/// Each case carries its own list of labels, matching PHP's fallthrough grouping
/// (`case A: case B: …` is one entry with two labels).
pub fn s_switch(
    subject: Expr,
    cases: Vec<(Vec<Expr>, Vec<Stmt>)>,
    default: Option<Vec<Stmt>>,
) -> Stmt {
    Stmt::new(
        StmtKind::Switch {
            subject,
            cases,
            default,
        },
        Span::dummy(),
    )
}

/// `while (condition) { body }`.
/// `do { <body> } while (<condition>);` — the body runs before the first test.
pub fn s_do_while(body: Vec<Stmt>, condition: Expr) -> Stmt {
    Stmt::new(StmtKind::DoWhile { body, condition }, Span::synthetic())
}

pub fn s_while(condition: Expr, body: Vec<Stmt>) -> Stmt {
    Stmt::new(StmtKind::While { condition, body }, Span::synthetic())
}

/// `break;` (one level) or `break N;`.
pub fn s_break(levels: usize) -> Stmt {
    Stmt::new(StmtKind::Break(levels), Span::dummy())
}

/// `continue;` (one level) or `continue N;`.
pub fn s_continue(levels: usize) -> Stmt {
    Stmt::new(StmtKind::Continue(levels), Span::dummy())
}

// ---------------------------------------------------------------------------
// Unused-parameter consumption
// ---------------------------------------------------------------------------

/// Collects the variable names read anywhere in `body`.
///
/// Deliberately conservative: an unrecognized node contributes no names, so a gap here can
/// only add a redundant `$_unused = $p;` — it can never leave a parameter unconsumed and
/// warning on every compile. Extend it whenever a new expression helper is added above.
fn reads_of(body: &[Stmt]) -> Vec<String> {
    let mut names = Vec::new();
    for stmt in body {
        walk_stmt(stmt, &mut names);
    }
    names
}

/// Accumulates variable reads from one statement.
fn walk_stmt(stmt: &Stmt, out: &mut Vec<String>) {
    match &stmt.kind {
        StmtKind::Return(Some(expr))
        | StmtKind::ExprStmt(expr)
        | StmtKind::Throw(expr)
        | StmtKind::Echo(expr) => walk_expr(expr, out),
        StmtKind::Return(None) | StmtKind::Break(_) | StmtKind::Continue(_) => {}
        // The declared name is a fresh local, not a use of an enclosing one, so only the
        // initializer contributes reads.
        StmtKind::StaticVar { init, .. } => walk_expr(init, out),
        // The TARGET counts as a use, not just the value. PHP does not warn about a parameter
        // a body assigns to, and an out-parameter is used precisely by being written.
        StmtKind::Assign { name, value } => {
            out.push(name.clone());
            walk_expr(value, out);
        }
        StmtKind::StaticPropertyAssign { value, .. }
        | StmtKind::StaticPropertyArrayPush { value, .. } => walk_expr(value, out),
        StmtKind::StaticPropertyArrayAssign { index, value, .. } => {
            walk_expr(index, out);
            walk_expr(value, out);
        }
        StmtKind::PropertyAssign { object, value, .. } => {
            walk_expr(object, out);
            walk_expr(value, out);
        }
        StmtKind::ArrayAssign { array, index, value } => {
            out.push(array.clone());
            walk_expr(index, out);
            walk_expr(value, out);
        }
        StmtKind::ArrayPush { array, value } => {
            out.push(array.clone());
            walk_expr(value, out);
        }
        StmtKind::PropertyArrayPush { object, value, .. } => {
            walk_expr(object, out);
            walk_expr(value, out);
        }
        StmtKind::PropertyArrayAssign {
            object,
            index,
            value,
            ..
        } => {
            walk_expr(object, out);
            walk_expr(index, out);
            walk_expr(value, out);
        }
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            walk_expr(condition, out);
            for inner in then_body {
                walk_stmt(inner, out);
            }
            for (clause_condition, clause_body) in elseif_clauses {
                walk_expr(clause_condition, out);
                for inner in clause_body {
                    walk_stmt(inner, out);
                }
            }
            for inner in else_body.iter().flatten() {
                walk_stmt(inner, out);
            }
        }
        StmtKind::While { condition, body } | StmtKind::DoWhile { body, condition } => {
            walk_expr(condition, out);
            for inner in body {
                walk_stmt(inner, out);
            }
        }
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => {
            for inner in init.iter().chain(update.iter()) {
                walk_stmt(inner, out);
            }
            if let Some(expr) = condition {
                walk_expr(expr, out);
            }
            for inner in body {
                walk_stmt(inner, out);
            }
        }
        StmtKind::Foreach { array, body, .. } => {
            walk_expr(array, out);
            for inner in body {
                walk_stmt(inner, out);
            }
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            for inner in try_body {
                walk_stmt(inner, out);
            }
            for catch in catches {
                for inner in &catch.body {
                    walk_stmt(inner, out);
                }
            }
            for inner in finally_body.iter().flatten() {
                walk_stmt(inner, out);
            }
        }
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => {
            walk_expr(subject, out);
            for (labels, body) in cases {
                for label in labels {
                    walk_expr(label, out);
                }
                for inner in body {
                    walk_stmt(inner, out);
                }
            }
            for inner in default.iter().flatten() {
                walk_stmt(inner, out);
            }
        }
        _ => {}
    }
}

/// Accumulates variable reads from one expression.
fn walk_expr(expr: &Expr, out: &mut Vec<String>) {
    match &expr.kind {
        ExprKind::Variable(name)
        | ExprKind::PostIncrement(name)
        | ExprKind::PreIncrement(name)
        | ExprKind::PostDecrement(name)
        | ExprKind::PreDecrement(name) => out.push(name.clone()),
        ExprKind::ArrayLiteral(items) => {
            for item in items {
                walk_expr(item, out);
            }
        }
        ExprKind::ArrayLiteralAssoc(entries) => {
            for (key, value) in entries {
                walk_expr(key, out);
                walk_expr(value, out);
            }
        }
        ExprKind::Assignment { target, value, .. } => {
            walk_expr(target, out);
            walk_expr(value, out);
        }
        ExprKind::StaticPropertyAccess { .. } => {}
        ExprKind::PropertyAccess { object, .. } => walk_expr(object, out),
        ExprKind::DynamicPropertyAccess { object, property } => {
            walk_expr(object, out);
            walk_expr(property, out);
        }
        ExprKind::NullCoalesce { value, default } => {
            walk_expr(value, out);
            walk_expr(default, out);
        }
        ExprKind::MethodCall { object, args, .. } => {
            walk_expr(object, out);
            for arg in args {
                walk_expr(arg, out);
            }
        }
        ExprKind::FunctionCall { args, .. }
        | ExprKind::StaticMethodCall { args, .. }
        | ExprKind::NewObject { args, .. }
        | ExprKind::NewScopedObject { args, .. } => {
            for arg in args {
                walk_expr(arg, out);
            }
        }
        ExprKind::NewDynamic { name_expr, args } => {
            walk_expr(name_expr, out);
            for arg in args {
                walk_expr(arg, out);
            }
        }
        ExprKind::BinaryOp { left, right, .. } => {
            walk_expr(left, out);
            walk_expr(right, out);
        }
        ExprKind::ArrayAccess { array, index } => {
            walk_expr(array, out);
            walk_expr(index, out);
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            walk_expr(condition, out);
            walk_expr(then_expr, out);
            walk_expr(else_expr, out);
        }
        ExprKind::Cast { expr: inner, .. }
        | ExprKind::Not(inner)
        | ExprKind::Negate(inner) => walk_expr(inner, out),
        ExprKind::InstanceOf { value, .. } => walk_expr(value, out),
        _ => {}
    }
}

/// Prepends the `$_unused` consumption for every parameter `body` does not read.
///
/// One unread parameter becomes `$_unused = $p;`; several become `$_unused = [$a, $b];`,
/// matching what the hand-written PHP did.
fn consume_unread_params(
    params: &[(String, Option<TypeExpr>, Option<Expr>, bool)],
    body: Vec<Stmt>,
) -> Vec<Stmt> {
    let read = reads_of(&body);
    let unread: Vec<Expr> = params
        .iter()
        // A BY-REF parameter is never consumed. It is an OUT-parameter: the caller reads what
        // the body writes into it, so writing is its use. Adding `$_unused = $width;` to
        // `exif_thumbnail(&$width, …)` would be dead code the PHP form never had.
        .filter(|(_, _, _, by_ref)| !by_ref)
        .filter(|(name, _, _, _)| !read.iter().any(|seen| seen == name))
        .map(|(name, _, _, _)| Expr::var(name.clone()))
        .collect();

    if unread.is_empty() {
        return body;
    }

    let consumed = if unread.len() == 1 {
        unread.into_iter().next().expect("checked non-empty")
    } else {
        e_array(unread)
    };
    let mut consumed_body = Vec::with_capacity(body.len() + 1);
    consumed_body.push(s_assign("_unused", consumed));
    consumed_body.extend(body);
    consumed_body
}

// ---------------------------------------------------------------------------
// Callables
// ---------------------------------------------------------------------------

/// Shared parameter/return/body state for methods and free functions.
#[derive(Default)]
struct Signature {
    params: Vec<(String, Option<TypeExpr>, Option<Expr>, bool)>,
    /// The variadic tail (`mixed ...$args`) — its name and its declared element type.
    variadic: Option<(String, Option<TypeExpr>)>,
    return_type: Option<TypeExpr>,
    body: Vec<Stmt>,
}

impl Signature {
    /// Consumes the signature into its parts, with unread parameters consumed.
    fn finish(
        self,
    ) -> (
        Vec<(String, Option<TypeExpr>, Option<Expr>, bool)>,
        Option<TypeExpr>,
        Vec<Stmt>,
    ) {
        let body = consume_unread_params(&self.params, self.body);
        (self.params, self.return_type, body)
    }
}

/// Builder for one method of a synthetic class.
///
/// The default shape is a `public` instance method with no parameters, no declared return
/// type and an empty body — the smallest thing that still lowers to a real EIR body.
pub struct MethodBuilder {
    name: String,
    visibility: Visibility,
    is_static: bool,
    is_final: bool,
    attributes: Vec<AttributeGroup>,
    consume_unread_params: bool,
    signature: Signature,
    /// Attribute groups per parameter, aligned with `signature.params` and grown lazily by
    /// `param_attr`. Left empty when no parameter carries one, so the common method pays
    /// nothing; `build` pads it back out to the parameter count the parser would produce.
    param_attributes: Vec<Vec<AttributeGroup>>,
}

/// Starts a `public` instance method named `name`.
pub fn method(name: &str) -> MethodBuilder {
    MethodBuilder {
        name: name.to_string(),
        visibility: Visibility::Public,
        is_static: false,
        is_final: false,
        attributes: Vec::new(),
        consume_unread_params: true,
        signature: Signature::default(),
        param_attributes: Vec::new(),
    }
}

impl MethodBuilder {
    /// Attaches one PHP attribute group to the method declaration.
    pub fn attr(mut self, name: &str, args: Vec<Expr>) -> Self {
        self.attributes.push(attr(name, args));
        self
    }

    /// Attaches a PHP 8 attribute to the parameter added LAST (`#[\SensitiveParameter] $pw`).
    ///
    /// Written as a separate call rather than an argument to every `param*` because a
    /// parameter attribute is rare — `#[\SensitiveParameter]` on a password is nearly the
    /// whole population — and threading an `attrs` argument through nine `param*` overloads
    /// would put an empty vector at every call site to serve a handful.
    ///
    /// `name` is spelled as it appears in source, leading `\` included: the parser keeps the
    /// qualification (`NameKind`), and `\SensitiveParameter` is a different node from
    /// `SensitiveParameter` — inside a `namespace Pdo` block it is also a different CLASS.
    pub fn param_attr(mut self, name: &str) -> Self {
        assert!(
            !self.signature.params.is_empty(),
            "param_attr on {}: an attribute attaches to the parameter added before it",
            self.name
        );
        let index = self.signature.params.len() - 1;
        if self.param_attributes.len() <= index {
            self.param_attributes.resize_with(index + 1, Vec::new);
        }
        self.param_attributes[index].push(AttributeGroup {
            attributes: vec![Attribute {
                name: class_name(name),
                args: Vec::new(),
                span: Span::dummy(),
            }],
            span: Span::dummy(),
        });
        self
    }

    /// Applies `edit` only when `condition` holds.
    ///
    /// For the places a transcribed prelude differs between PHP profiles. Written as a
    /// combinator rather than an `if` around the whole builder so the chain stays ONE
    /// expression: duplicating a three-thousand-line method to vary one parameter attribute is
    /// how two copies drift apart.
    pub fn when(self, condition: bool, edit: impl FnOnce(Self) -> Self) -> Self {
        if condition {
            edit(self)
        } else {
            self
        }
    }

    /// Makes the method `private`.
    pub fn private(mut self) -> Self {
        self.visibility = Visibility::Private;
        self
    }

    /// Makes the method `protected`.
    pub fn protected(mut self) -> Self {
        self.visibility = Visibility::Protected;
        self
    }

    /// Declares the VARIADIC tail (`function query(string $sql, ...$args)`).
    ///
    /// `element_type` is the per-argument hint, which is a different thing from the type of
    /// the array the tail collects into — `int ...$xs` hints each argument, not the array.
    pub fn variadic(mut self, name: &str, element_type: Option<TypeExpr>) -> Self {
        self.signature.variadic = Some((name.to_string(), element_type));
        self
    }

    /// Makes the method `static`.
    pub fn static_(mut self) -> Self {
        self.is_static = true;
        self
    }

    /// Makes the method `final`.
    pub fn final_(mut self) -> Self {
        self.is_final = true;
        self
    }

    /// Appends a required parameter.
    pub fn param(mut self, name: &str, ty: TypeExpr) -> Self {
        self.signature
            .params
            .push((name.to_string(), Some(ty), None, false));
        self
    }

    /// Appends an optional parameter with a default value.
    pub fn param_default(mut self, name: &str, ty: TypeExpr, default: Expr) -> Self {
        self.signature
            .params
            .push((name.to_string(), Some(ty), Some(default), false));
        self
    }

    /// Appends an UNTYPED required parameter (`function m($x)`).
    pub fn param_untyped(mut self, name: &str) -> Self {
        self.signature
            .params
            .push((name.to_string(), None, None, false));
        self
    }

    /// Appends a BY-REFERENCE parameter (`function m(int &$x)`), typed or not.
    pub fn param_by_ref(mut self, name: &str, ty: Option<TypeExpr>) -> Self {
        self.signature
            .params
            .push((name.to_string(), ty, None, true));
        self
    }

    /// Appends a BY-REFERENCE parameter carrying a default (`function m(int &$w = 0)`).
    pub fn param_by_ref_default(mut self, name: &str, ty: Option<TypeExpr>, default: Expr) -> Self {
        self.signature
            .params
            .push((name.to_string(), ty, Some(default), true));
        self
    }

    /// Appends an UNTYPED optional parameter (`function m($x = 1)`).
    pub fn param_untyped_default(mut self, name: &str, default: Expr) -> Self {
        self.signature
            .params
            .push((name.to_string(), None, Some(default), false));
        self
    }

    /// Declares the return type.
    pub fn returns(mut self, ty: TypeExpr) -> Self {
        self.signature.return_type = Some(ty);
        self
    }

    /// Sets the body to a single `return <value>;`.
    pub fn returning(mut self, value: Expr) -> Self {
        self.signature.body = vec![s_return(value)];
        self
    }

    /// Sets the full body.
    pub fn body(mut self, body: Vec<Stmt>) -> Self {
        self.signature.body = body;
        self
    }

    /// Sets an already-complete body without synthesizing unread-parameter consumption.
    pub fn body_exact(mut self, body: Vec<Stmt>) -> Self {
        self.consume_unread_params = false;
        self.signature.body = body;
        self
    }

    /// Builds the method as a bodiless SIGNATURE, for an interface.
    ///
    /// The parameters are NOT consumed: `$_unused = $p;` is a statement, and a signature has no
    /// statements. Nothing warns about an unused parameter of a method that has no body.
    fn build_abstract(self) -> ClassMethod {
        assert!(
            self.signature.body.is_empty(),
            "an interface method is a signature: {} must have no body",
            self.name
        );
        // NOT `Signature::finish()`: that appends the `$_unused = …;` consumption, and a
        // signature has no statements to append it to. Nothing warns about an unused parameter
        // of a method that has no body.
        let param_attributes = vec![Vec::new(); self.signature.params.len()];
        ClassMethod {
            name: self.name,
            visibility: self.visibility,
            is_static: self.is_static,
            is_abstract: true,
            is_final: self.is_final,
            has_body: false,
            params: self.signature.params,
            param_attributes,
            variadic: None,
            variadic_by_ref: false,
            variadic_type: None,
            return_type: self.signature.return_type,
            by_ref_return: false,
            body: Vec::new(),
            span: Span::dummy(),
            attributes: self.attributes,
        }
    }

    /// Builds the method, prepending the `$_unused` consumption when it is needed.
    fn build(mut self) -> ClassMethod {
        let param_attributes = std::mem::take(&mut self.param_attributes);
        let variadic = self.signature.variadic.take();
        let (params, return_type, body) = if self.consume_unread_params {
            self.signature.finish()
        } else {
            (
                self.signature.params,
                self.signature.return_type,
                self.signature.body,
            )
        };
        // The parser emits one group list per parameter, so pad rather than leave the vector
        // short: a comparison against the parsed AST is length-sensitive, and a builder that
        // only ever attributed parameter 0 would otherwise never equal its own source.
        let mut param_attributes = param_attributes;
        // The parser attributes the VARIADIC tail too, so the list runs one past the declared
        // parameters when there is one — pad to the length the parser would produce, not to
        // `params.len()`, or an equality check against the parsed AST fails on length alone.
        let (variadic_name, variadic_type) = match variadic {
            Some((name, element_type)) => (Some(name), element_type),
            None => (None, None),
        };
        param_attributes
            .resize_with(params.len() + usize::from(variadic_name.is_some()), Vec::new);
        ClassMethod {
            name: self.name,
            visibility: self.visibility,
            is_static: self.is_static,
            is_abstract: false,
            is_final: self.is_final,
            has_body: true,
            params,
            param_attributes,
            variadic: variadic_name,
            variadic_by_ref: false,
            variadic_type,
            return_type,
            by_ref_return: false,
            body,
            span: Span::dummy(),
            attributes: self.attributes,
        }
    }
}

/// Builder for one synthetic free function.
pub struct FunctionBuilder {
    name: String,
    consume_unread_params: bool,
    signature: Signature,
}

/// Starts a free function named `name`.
pub fn function(name: &str) -> FunctionBuilder {
    FunctionBuilder {
        name: name.to_string(),
        consume_unread_params: true,
        signature: Signature::default(),
    }
}

impl FunctionBuilder {
    /// Appends a required parameter.
    pub fn param(mut self, name: &str, ty: TypeExpr) -> Self {
        self.signature
            .params
            .push((name.to_string(), Some(ty), None, false));
        self
    }

    /// Appends an optional parameter with a default value.
    pub fn param_default(mut self, name: &str, ty: TypeExpr, default: Expr) -> Self {
        self.signature
            .params
            .push((name.to_string(), Some(ty), Some(default), false));
        self
    }

    /// Appends an UNTYPED optional parameter (`function f($x = 1)`).
    ///
    /// Untyped is not the same as `mixed`: the checker infers an untyped parameter from its
    /// call sites, so writing `t_mixed()` where the PHP form had no hint would widen the
    /// signature rather than transcribe it.
    pub fn param_untyped_default(mut self, name: &str, default: Expr) -> Self {
        self.signature
            .params
            .push((name.to_string(), None, Some(default), false));
        self
    }

    /// Appends an UNTYPED required parameter (`function f($x)`).
    pub fn param_untyped(mut self, name: &str) -> Self {
        self.signature
            .params
            .push((name.to_string(), None, None, false));
        self
    }

    /// Appends a BY-REFERENCE parameter (`function f(int &$x)`), typed or not.
    ///
    /// By-ref is part of the signature, not a detail: `getimagesize($file, &$info)` writes
    /// through it, so transcribing it by value would silently drop the out-parameter.
    pub fn param_by_ref(mut self, name: &str, ty: Option<TypeExpr>) -> Self {
        self.signature
            .params
            .push((name.to_string(), ty, None, true));
        self
    }

    /// Appends a BY-REFERENCE parameter carrying a default (`function f(int &$w = 0)`).
    pub fn param_by_ref_default(mut self, name: &str, ty: Option<TypeExpr>, default: Expr) -> Self {
        self.signature
            .params
            .push((name.to_string(), ty, Some(default), true));
        self
    }

    /// Declares the return type.
    pub fn returns(mut self, ty: TypeExpr) -> Self {
        self.signature.return_type = Some(ty);
        self
    }

    /// Declares the VARIADIC tail (`mixed ...$args`). `element_type` is the per-argument hint,
    /// which is a different thing from the collected array's type.
    pub fn variadic(mut self, name: &str, element_type: Option<TypeExpr>) -> Self {
        self.signature.variadic = Some((name.to_string(), element_type));
        self
    }

    /// Sets the body to a single `return <value>;`.
    pub fn returning(mut self, value: Expr) -> Self {
        self.signature.body = vec![s_return(value)];
        self
    }

    /// Sets the full body.
    pub fn body(mut self, body: Vec<Stmt>) -> Self {
        self.signature.body = body;
        self
    }

    /// Sets an already-complete body without synthesizing unread-parameter consumption.
    pub fn body_exact(mut self, body: Vec<Stmt>) -> Self {
        self.consume_unread_params = false;
        self.signature.body = body;
        self
    }

    /// Emits the function declaration statement.
    pub fn build(self) -> Stmt {
        let (variadic_name, variadic_type) = match self.signature.variadic.clone() {
            Some((name, ty)) => (Some(name), ty),
            None => (None, None),
        };
        let (params, return_type, body) = if self.consume_unread_params {
            self.signature.finish()
        } else {
            (
                self.signature.params,
                self.signature.return_type,
                self.signature.body,
            )
        };
        // The parser aligns `param_attributes` with the declared parameters PLUS the variadic
        // tail when there is one, so the vector is one longer than `params` for a variadic.
        let param_attributes =
            vec![Vec::new(); params.len() + usize::from(variadic_name.is_some())];
        Stmt::new(
            StmtKind::FunctionDecl {
                name: self.name,
                params,
                param_attributes,
                variadic: variadic_name,
                variadic_by_ref: false,
                variadic_type,
                return_type,
                by_ref_return: false,
                body,
            },
            Span::dummy(),
        )
    }
}

// ---------------------------------------------------------------------------
// Call-site audit (test support)
// ---------------------------------------------------------------------------

/// Collects every free-function name CALLED anywhere in a built prelude.
///
/// `builtins::parity_tests` uses this to enforce that no injected prelude calls a
/// PHP-visible extension builtin instead of its `internal: true` `__elephc_*` alias —
/// a gate the PHP-text preludes get from a textual scan of their source.
///
/// UNLIKE `reads_of`, THIS MUST NOT BE CONSERVATIVE. A node it silently skipped would hide
/// a forbidden call and turn the gate green while the prelude breaks `--strict-php`
/// compiles. So it panics on any node these builders are not known to produce: adding a
/// helper above without teaching this function about it fails loudly instead of quietly
/// narrowing the audit.
#[cfg(test)]
pub(crate) fn called_function_names(program: &Program) -> Vec<String> {
    let mut names = Vec::new();
    for stmt in program {
        audit_stmt(stmt, &mut names);
    }
    names
}

/// Collects call sites from one statement, rejecting anything unmodelled.
#[cfg(test)]
fn audit_stmt(stmt: &Stmt, out: &mut Vec<String>) {
    match &stmt.kind {
        StmtKind::Return(Some(expr))
        | StmtKind::ExprStmt(expr)
        | StmtKind::Throw(expr)
        | StmtKind::Echo(expr) => audit_expr(expr, out),
        StmtKind::Return(None) | StmtKind::Break(_) | StmtKind::Continue(_) => {}
        StmtKind::StaticVar { init, .. } => audit_expr(init, out),
        StmtKind::Assign { value, .. } | StmtKind::TypedAssign { value, .. } => {
            audit_expr(value, out)
        }
        StmtKind::StaticPropertyAssign { value, .. }
        | StmtKind::StaticPropertyArrayPush { value, .. } => audit_expr(value, out),
        StmtKind::StaticPropertyArrayAssign { index, value, .. } => {
            audit_expr(index, out);
            audit_expr(value, out);
        }
        StmtKind::PropertyAssign { object, value, .. } => {
            audit_expr(object, out);
            audit_expr(value, out);
        }
        StmtKind::ArrayAssign { index, value, .. } => {
            audit_expr(index, out);
            audit_expr(value, out);
        }
        StmtKind::ArrayPush { value, .. } => audit_expr(value, out),
        StmtKind::PropertyArrayPush { object, value, .. } => {
            audit_expr(object, out);
            audit_expr(value, out);
        }
        StmtKind::PropertyArrayAssign {
            object,
            index,
            value,
            ..
        } => {
            audit_expr(object, out);
            audit_expr(index, out);
            audit_expr(value, out);
        }
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            audit_expr(condition, out);
            for inner in then_body {
                audit_stmt(inner, out);
            }
            for (clause_condition, clause_body) in elseif_clauses {
                audit_expr(clause_condition, out);
                for inner in clause_body {
                    audit_stmt(inner, out);
                }
            }
            for inner in else_body.iter().flatten() {
                audit_stmt(inner, out);
            }
        }
        StmtKind::While { condition, body } | StmtKind::DoWhile { body, condition } => {
            audit_expr(condition, out);
            for inner in body {
                audit_stmt(inner, out);
            }
        }
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => {
            for inner in init.iter().chain(update.iter()) {
                audit_stmt(inner, out);
            }
            if let Some(expr) = condition {
                audit_expr(expr, out);
            }
            for inner in body {
                audit_stmt(inner, out);
            }
        }
        StmtKind::Foreach { array, body, .. } => {
            audit_expr(array, out);
            for inner in body {
                audit_stmt(inner, out);
            }
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            for inner in try_body {
                audit_stmt(inner, out);
            }
            for catch in catches {
                for inner in &catch.body {
                    audit_stmt(inner, out);
                }
            }
            for inner in finally_body.iter().flatten() {
                audit_stmt(inner, out);
            }
        }
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => {
            audit_expr(subject, out);
            for (labels, body) in cases {
                for label in labels {
                    audit_expr(label, out);
                }
                for inner in body {
                    audit_stmt(inner, out);
                }
            }
            for inner in default.iter().flatten() {
                audit_stmt(inner, out);
            }
        }
        // Declarations: walk defaults and bodies, where a call can hide.
        StmtKind::FunctionDecl { params, body, .. } => {
            audit_params(params, out);
            for inner in body {
                audit_stmt(inner, out);
            }
        }
        StmtKind::ClassDecl {
            properties,
            methods,
            constants,
            ..
        } => {
            for property in properties {
                if let Some(default) = &property.default {
                    audit_expr(default, out);
                }
            }
            for constant in constants {
                audit_expr(&constant.value, out);
            }
            for class_method in methods {
                audit_params(&class_method.params, out);
                for inner in &class_method.body {
                    audit_stmt(inner, out);
                }
            }
        }
        // An interface declares SIGNATURES, which carry no call sites; only a constant's
        // initializer can hold one.
        StmtKind::InterfaceDecl {
            methods, constants, ..
        } => {
            for constant in constants {
                audit_expr(&constant.value, out);
            }
            for signature in methods {
                audit_params(&signature.params, out);
            }
        }
        StmtKind::ConstDecl { value, .. } => audit_expr(value, out),
        // A namespace body is just declarations, and the audit has to reach them: the PDO
        // driver subclasses all live inside one, and stopping here would exempt them.
        StmtKind::NamespaceBlock { body, .. } => {
            for inner in body {
                audit_stmt(inner, out);
            }
        }
        // Extern declarations carry types and no expressions.
        StmtKind::ExternFunctionDecl { .. } => {}
        other => panic!(
            "synthetic_class::called_function_names met an unmodelled statement: {}",
            format!("{:?}", other).chars().take(120).collect::<String>()
        ),
    }
}

/// Collects call sites from parameter DEFAULTS, which are ordinary expressions.
#[cfg(test)]
fn audit_params(params: &[(String, Option<TypeExpr>, Option<Expr>, bool)], out: &mut Vec<String>) {
    for (_, _, default, _) in params {
        if let Some(expr) = default {
            audit_expr(expr, out);
        }
    }
}

/// Collects call sites from one expression, rejecting anything unmodelled.
#[cfg(test)]
fn audit_expr(expr: &Expr, out: &mut Vec<String>) {
    match &expr.kind {
        ExprKind::FunctionCall { name, args } => {
            out.push(name.as_canonical());
            for arg in args {
                audit_expr(arg, out);
            }
        }
        ExprKind::StaticMethodCall { args, .. }
        | ExprKind::NewObject { args, .. }
        | ExprKind::NewScopedObject { args, .. } => {
            for arg in args {
                audit_expr(arg, out);
            }
        }
        ExprKind::NewDynamic { name_expr, args } => {
            audit_expr(name_expr, out);
            for arg in args {
                audit_expr(arg, out);
            }
        }
        ExprKind::MethodCall { object, args, .. } => {
            audit_expr(object, out);
            for arg in args {
                audit_expr(arg, out);
            }
        }
        ExprKind::StaticPropertyAccess { .. } => {}
        ExprKind::PropertyAccess { object, .. } => audit_expr(object, out),
        ExprKind::DynamicPropertyAccess { object, property } => {
            audit_expr(object, out);
            audit_expr(property, out);
        }
        ExprKind::NullCoalesce { value, default } => {
            audit_expr(value, out);
            audit_expr(default, out);
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            audit_expr(condition, out);
            audit_expr(then_expr, out);
            audit_expr(else_expr, out);
        }
        ExprKind::ArrayLiteral(items) => {
            for item in items {
                audit_expr(item, out);
            }
        }
        ExprKind::ArrayLiteralAssoc(entries) => {
            for (key, value) in entries {
                audit_expr(key, out);
                audit_expr(value, out);
            }
        }
        ExprKind::Assignment { target, value, .. } => {
            audit_expr(target, out);
            audit_expr(value, out);
        }
        ExprKind::BinaryOp { left, right, .. } => {
            audit_expr(left, out);
            audit_expr(right, out);
        }
        ExprKind::ArrayAccess { array, index } => {
            audit_expr(array, out);
            audit_expr(index, out);
        }
        ExprKind::Cast { expr: inner, .. }
        | ExprKind::Not(inner)
        | ExprKind::Negate(inner) => audit_expr(inner, out),
        ExprKind::InstanceOf { value, .. } => audit_expr(value, out),
        // A closure body is ordinary code and can call anything, so it has to be walked — a
        // leaf arm here would exempt every call written inside one from this whole audit.
        ExprKind::Closure { body, params, .. } => {
            for (_, _, default, _) in params {
                if let Some(default) = default {
                    audit_expr(default, out);
                }
            }
            for stmt in body {
                audit_stmt(stmt, out);
            }
        }
        // The callee is a variable, so nothing is NAMED here — but the arguments are code.
        ExprKind::ClosureCall { args, .. } => {
            for arg in args {
                audit_expr(arg, out);
            }
        }
        ExprKind::Spread(inner) => audit_expr(inner, out),
        // Leaves: nothing to descend into. A class-constant read names no function.
        ExprKind::ScopedConstantAccess { .. } | ExprKind::ClassConstant { .. } => {}
        ExprKind::Variable(_)
        | ExprKind::This
        | ExprKind::Null
        | ExprKind::BoolLiteral(_)
        | ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::StringLiteral(_)
        | ExprKind::ConstRef(_)
        | ExprKind::PostIncrement(_)
        | ExprKind::PreIncrement(_)
        | ExprKind::PostDecrement(_)
        | ExprKind::PreDecrement(_) => {}
        other => panic!(
            "synthetic_class::called_function_names met an unmodelled expression: {}",
            format!("{:?}", other).chars().take(120).collect::<String>()
        ),
    }
}

// ---------------------------------------------------------------------------
// Extern declarations
// ---------------------------------------------------------------------------

/// Builder for one `extern "lib" { function … }` entry.
///
/// A block declaring several functions has no node of its own: the parser emits one
/// `ExternFunctionDecl` per function, each carrying the library name, so a block is written
/// here as several `extern_fn` calls sharing a library.
pub struct ExternFnBuilder {
    name: String,
    library: String,
    params: Vec<ExternParam>,
    return_type: CType,
}

/// Starts an extern function bound to `library`, returning `void` until `returns` says otherwise.
pub fn extern_fn(name: &str, library: &str) -> ExternFnBuilder {
    ExternFnBuilder {
        name: name.to_string(),
        library: library.to_string(),
        params: Vec::new(),
        return_type: CType::Void,
    }
}

impl ExternFnBuilder {
    /// Appends a parameter with its C type.
    pub fn param(mut self, name: &str, c_type: CType) -> Self {
        self.params.push(ExternParam {
            name: name.to_string(),
            c_type,
        });
        self
    }

    /// Declares the C return type.
    pub fn returns(mut self, c_type: CType) -> Self {
        self.return_type = c_type;
        self
    }

    /// Emits the extern declaration statement.
    pub fn build(self) -> Stmt {
        Stmt::new(
            StmtKind::ExternFunctionDecl {
                name: self.name,
                params: self.params,
                return_type: self.return_type,
                library: Some(self.library),
            },
            Span::dummy(),
        )
    }
}

// ---------------------------------------------------------------------------
// Classes
// ---------------------------------------------------------------------------

/// Builder for one synthetic class declaration.
pub struct ClassBuilder {
    name: String,
    is_final: bool,
    extends: Option<Name>,
    implements: Vec<Name>,
    constants: Vec<ClassConst>,
    properties: Vec<ClassProperty>,
    methods: Vec<ClassMethod>,
}

/// Starts a concrete (non-abstract, non-final) class named `name`.
pub fn class(name: &str) -> ClassBuilder {
    ClassBuilder {
        name: name.to_string(),
        is_final: false,
        extends: None,
        implements: Vec::new(),
        constants: Vec::new(),
        properties: Vec::new(),
        methods: Vec::new(),
    }
}

impl ClassBuilder {
    /// Makes the class `final`.
    pub fn final_(mut self) -> Self {
        self.is_final = true;
        self
    }

    /// Applies `edit` only when `condition` holds — see `MethodBuilder::when`.
    pub fn when(self, condition: bool, edit: impl FnOnce(Self) -> Self) -> Self {
        if condition {
            edit(self)
        } else {
            self
        }
    }

    /// Sets the parent class, AS SPELLED.
    ///
    /// The leading `\` is not decoration inside a namespace: `namespace Pdo { class Sqlite
    /// extends PDO }` resolves the parent to `Pdo\PDO`, which does not exist — the driver
    /// subclasses spell `\PDO` and have to keep spelling it.
    pub fn extends(mut self, parent: &str) -> Self {
        self.extends = Some(class_name(parent));
        self
    }

    /// Adds an implemented interface, AS SPELLED (see `extends` for why the `\` matters).
    pub fn implements(mut self, interface: &str) -> Self {
        self.implements.push(class_name(interface));
        self
    }

    /// Adds a `public` class constant carrying PHP 8 attributes.
    ///
    /// `#[\Deprecated("as it has no effect")]` on `PDO::TRANSACTION_*` is the whole population
    /// today, and it matters: the attribute is what makes reference PHP warn on the constant,
    /// so dropping it silently changes what a program is told.
    pub fn constant_attributed(
        self,
        name: &str,
        value: Expr,
        attributes: Vec<AttributeGroup>,
    ) -> Self {
        self.constant_full(name, value, None, attributes)
    }

    /// Adds a `public` class constant with its complete reflection-visible metadata.
    pub fn constant_full(
        mut self,
        name: &str,
        value: Expr,
        type_expr: Option<TypeExpr>,
        attributes: Vec<AttributeGroup>,
    ) -> Self {
        self.constants.push(ClassConst {
            name: name.to_string(),
            visibility: Visibility::Public,
            is_final: false,
            type_expr,
            value,
            span: Span::dummy(),
            attributes,
        });
        self
    }

    /// Adds a `public` class constant.
    pub fn constant(self, name: &str, value: Expr) -> Self {
        self.constant_full(name, value, None, Vec::new())
    }

    /// Adds a `public` property with a declared type and an optional default.
    pub fn prop(self, name: &str, ty: TypeExpr, default: Option<Expr>) -> Self {
        self.property(name, Some(ty), default, Visibility::Public)
    }

    /// Adds a public virtual get-only property backed by the class's synthetic getter method.
    pub fn virtual_get_prop(mut self, name: &str, ty: TypeExpr) -> Self {
        self.properties.push(ClassProperty {
            name: name.to_string(),
            visibility: Visibility::Public,
            set_visibility: None,
            type_expr: Some(ty),
            hooks: PropertyHooks {
                get: true,
                set: false,
                get_by_ref: false,
            },
            readonly: false,
            is_final: false,
            is_static: false,
            is_abstract: false,
            by_ref: false,
            is_promoted: false,
            default: None,
            span: Span::dummy(),
            attributes: Vec::new(),
        });
        self
    }

    /// Adds a `private` property with a declared type and an optional default.
    pub fn private_prop(self, name: &str, ty: TypeExpr, default: Option<Expr>) -> Self {
        self.property(name, Some(ty), default, Visibility::Private)
    }

    /// Adds an UNTYPED `private` property (`private $fetchTarget;`).
    ///
    /// Untyped is not `mixed`: the checker infers an untyped property's type from what is
    /// assigned to it, which is exactly why some prelude properties are left unhinted.
    pub fn private_untyped_prop(self, name: &str, default: Option<Expr>) -> Self {
        self.property(name, None, default, Visibility::Private)
    }

    /// Adds an UNTYPED `public` property.
    pub fn untyped_prop(self, name: &str, default: Option<Expr>) -> Self {
        self.property(name, None, default, Visibility::Public)
    }

    /// Adds a `public static` property with a declared type and an optional default.
    ///
    /// A static property is CLASS state, read and written through `Class::$name` — a different
    /// node from an instance property, and the reason `e_static_prop` exists.
    pub fn static_prop(self, name: &str, ty: TypeExpr, default: Option<Expr>) -> Self {
        self.property_at(name, Some(ty), default, Visibility::Public, true)
    }

    /// Adds an UNTYPED `public static` property.
    pub fn static_untyped_prop(self, name: &str, default: Option<Expr>) -> Self {
        self.property_at(name, None, default, Visibility::Public, true)
    }

    /// Adds a `protected` property with a declared type and an optional default.
    ///
    /// `protected` is not a stylistic middle ground between the other two here: a subclass in
    /// the same prelude reads it, and `private` would make that read a fatal at the subclass.
    pub fn protected_prop(self, name: &str, ty: TypeExpr, default: Option<Expr>) -> Self {
        self.property(name, Some(ty), default, Visibility::Protected)
    }

    /// Adds an UNTYPED `protected` property.
    pub fn protected_untyped_prop(self, name: &str, default: Option<Expr>) -> Self {
        self.property(name, None, default, Visibility::Protected)
    }

    /// Adds a `private static` property with a declared type and an optional default.
    ///
    /// Class state that only the declaring class touches — a per-class registration latch or a
    /// pending-value slot handed between two calls, which is what the PDO prelude uses it for.
    pub fn private_static_prop(self, name: &str, ty: TypeExpr, default: Option<Expr>) -> Self {
        self.property_at(name, Some(ty), default, Visibility::Private, true)
    }

    /// Adds an UNTYPED `private static` property.
    pub fn private_static_untyped_prop(self, name: &str, default: Option<Expr>) -> Self {
        self.property_at(name, None, default, Visibility::Private, true)
    }

    /// Adds a `protected static` property with a declared type and an optional default.
    pub fn protected_static_prop(self, name: &str, ty: TypeExpr, default: Option<Expr>) -> Self {
        self.property_at(name, Some(ty), default, Visibility::Protected, true)
    }

    /// Adds a `public readonly` property with a declared type.
    ///
    /// No default, and that is the language rather than a simplification: a readonly property
    /// cannot carry an initializer, it is written exactly once from inside the declaring class.
    /// PHP also requires a declared type on one, so there is no untyped variant to add.
    pub fn readonly_prop(mut self, name: &str, ty: TypeExpr) -> Self {
        self = self.property(name, Some(ty), None, Visibility::Public);
        if let Some(property) = self.properties.last_mut() {
            property.readonly = true;
        }
        self
    }

    /// Adds an instance property at an explicit visibility.
    fn property(
        self,
        name: &str,
        ty: Option<TypeExpr>,
        default: Option<Expr>,
        visibility: Visibility,
    ) -> Self {
        self.property_at(name, ty, default, visibility, false)
    }

    /// Adds a property at an explicit visibility and instance/static kind.
    fn property_at(
        mut self,
        name: &str,
        ty: Option<TypeExpr>,
        default: Option<Expr>,
        visibility: Visibility,
        is_static: bool,
    ) -> Self {
        self.properties.push(ClassProperty {
            name: name.to_string(),
            visibility,
            set_visibility: None,
            type_expr: ty,
            hooks: PropertyHooks::none(),
            readonly: false,
            is_final: false,
            is_static,
            is_abstract: false,
            by_ref: false,
            is_promoted: false,
            default,
            span: Span::dummy(),
            attributes: Vec::new(),
        });
        self
    }

    /// Adds a method.
    pub fn method(mut self, builder: MethodBuilder) -> Self {
        self.methods.push(builder.build());
        self
    }

    /// Emits the class declaration statement.
    pub fn build(self) -> Stmt {
        Stmt::new(
            StmtKind::ClassDecl {
                name: self.name,
                extends: self.extends,
                implements: self.implements,
                is_abstract: false,
                is_final: self.is_final,
                is_readonly_class: false,
                trait_uses: Vec::new(),
                properties: self.properties,
                methods: self.methods,
                constants: self.constants,
            },
            Span::dummy(),
        )
    }
}

/// Builder for one synthetic interface.
///
/// An interface's methods are SIGNATURES: `has_body: false` and `is_abstract: true`, which is a
/// different node from a method with an empty body — the checker uses the flag to decide whether
/// an implementor must supply one. `abstract_method` is the only way to build that shape, so a
/// `MethodBuilder` cannot accidentally produce a bodiless method with a body flag set.
pub struct InterfaceBuilder {
    name: String,
    extends: Vec<Name>,
    constants: Vec<ClassConst>,
    methods: Vec<ClassMethod>,
}

/// Starts an interface named `name`.
pub fn interface(name: &str) -> InterfaceBuilder {
    InterfaceBuilder {
        name: name.to_string(),
        extends: Vec::new(),
        constants: Vec::new(),
        methods: Vec::new(),
    }
}

impl InterfaceBuilder {
    /// Adds a parent interface (`interface A extends B`), AS SPELLED.
    pub fn extends(mut self, parent: &str) -> Self {
        self.extends.push(class_name(parent));
        self
    }

    /// Adds an interface constant.
    pub fn constant(self, name: &str, value: Expr) -> Self {
        self.constant_full(name, value, None, Vec::new())
    }

    /// Adds an interface constant with its complete reflection-visible metadata.
    pub fn constant_full(
        mut self,
        name: &str,
        value: Expr,
        type_expr: Option<TypeExpr>,
        attributes: Vec<AttributeGroup>,
    ) -> Self {
        self.constants.push(ClassConst {
            name: name.to_string(),
            value,
            visibility: Visibility::Public,
            is_final: false,
            type_expr,
            span: Span::dummy(),
            attributes,
        });
        self
    }

    /// Adds one method SIGNATURE.
    pub fn method(mut self, builder: MethodBuilder) -> Self {
        self.methods.push(builder.build_abstract());
        self
    }

    /// Finishes the interface declaration.
    pub fn build(self) -> Stmt {
        Stmt::new(
            StmtKind::InterfaceDecl {
                name: self.name,
                extends: self.extends,
                properties: Vec::new(),
                methods: self.methods,
                constants: self.constants,
            },
            Span::dummy(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parses PHP the way a prelude used to be parsed.
    fn parse_php(source: &str) -> Program {
        let tokens = crate::lexer::tokenize(source).expect("fixture must tokenize");
        crate::parser::parse_internal(&tokens).expect("fixture must parse")
    }

    /// The builders must emit the SAME nodes the parser emits for the equivalent PHP.
    ///
    /// This is the property that makes a conversion a delivery-mechanism change rather than a
    /// semantic one, and it is not obvious: statement-position assignment parses to
    /// `StmtKind::Assign` rather than `ExprStmt(ExprKind::Assignment)`, and `array` parses to
    /// `TypeExpr::Named("array")` rather than `TypeExpr::Array`. Comparing against a real parse
    /// is what catches a builder that picks the other shape.
    #[test]
    fn parser_agreement() {
        let parsed = parse_php(
            r#"<?php
final class Widget {
    public string $label = '';
    public mixed $slot = null;

    private function __construct() {}

    public static function make(string $label, mixed $slot): Widget {
        $w = new self();
        $w->label = $label;
        $w->slot = $slot;
        return $w;
    }

    public function describe(): array {
        return ['label' => $this->label];
    }
}

function widget_make(string $label, mixed $slot): Widget {
    return Widget::make($label, $slot);
}
"#,
        );

        let built = internal_declarations(|| {
            vec![
                class("Widget")
                    .final_()
                    .prop("label", TypeExpr::Str, Some(e_str("")))
                    .prop("slot", t_mixed(), Some(e_null()))
                    .method(method("__construct").private())
                    .method(
                        method("make")
                            .static_()
                            .param("label", TypeExpr::Str)
                            .param("slot", t_mixed())
                            .returns(t_class("Widget"))
                            .body(vec![
                                s_assign("w", e_new_self(vec![])),
                                s_prop_assign(e_var("w"), "label", e_var("label")),
                                s_prop_assign(e_var("w"), "slot", e_var("slot")),
                                s_return(e_var("w")),
                            ]),
                    )
                    .method(
                        method("describe")
                            .returns(t_array())
                            .returning(e_array_assoc(vec![(
                                e_str("label"),
                                e_this_prop("label"),
                            )])),
                    )
                    .build(),
                function("widget_make")
                    .param("label", TypeExpr::Str)
                    .param("slot", t_mixed())
                    .returns(t_class("Widget"))
                    .returning(e_static_call(
                        "Widget",
                        "make",
                        vec![e_var("label"), e_var("slot")],
                    ))
                    .build(),
            ]
        });

        assert_eq!(built.len(), parsed.len());
        for (built_stmt, parsed_stmt) in built.iter().zip(parsed.iter()) {
            assert_eq!(
                format!("{:?}", strip_spans(built_stmt)),
                format!("{:?}", strip_spans(parsed_stmt)),
            );
        }
    }

    /// The same agreement for extern declarations, constant references, method calls and a
    /// function with no declared return type — the vocabulary the bridge-backed preludes use.
    ///
    /// Two shapes here are not guessable and are the reason this test exists:
    /// - `extern "lib" { … }` has no node of its own. It parses to one `ExternFunctionDecl`
    ///   per function, each carrying the library, so a builder that invented a block node
    ///   would produce an AST no parse can yield.
    /// - NOT EVERY CONSTANT IS A `ConstRef`. `PHP_INT_MIN`/`PHP_INT_MAX` are dedicated LEXER
    ///   tokens (`lexer/literals/identifiers.rs`) folded to integer literals before the parser
    ///   ever sees a name, while `STDERR`/`PHP_OS` do reach it as `ConstRef`. Writing
    ///   `e_const("PHP_INT_MIN")` builds a node the PHP form never produced.
    #[test]
    fn parser_agreement_extern_and_constants() {
        let parsed = parse_php(
            r#"<?php

extern "elephc_demo" {
    function elephc_demo_load(string $zone): string;
    function elephc_demo_reset(): void;
}

function demo_location_get(DemoZone $object) {
    return $object->getLocation();
}

function demo_window(DemoZone $object, int $begin = PHP_INT_MIN, int $end = PHP_INT_MAX) {
    return $object->getWindow($begin, $end);
}

function demo_complain(string $message): void {
    fwrite(STDERR, $message);
}
"#,
        );

        let built = internal_declarations(|| {
            vec![
                extern_fn("elephc_demo_load", "elephc_demo")
                    .param("zone", CType::Str)
                    .returns(CType::Str)
                    .build(),
                extern_fn("elephc_demo_reset", "elephc_demo")
                    .returns(CType::Void)
                    .build(),
                function("demo_location_get")
                    .param("object", t_class("DemoZone"))
                    .returning(e_method_call(e_var("object"), "getLocation", vec![]))
                    .build(),
                function("demo_window")
                    .param("object", t_class("DemoZone"))
                    .param_default("begin", TypeExpr::Int, e_int(i64::MIN))
                    .param_default("end", TypeExpr::Int, e_int(i64::MAX))
                    .returning(e_method_call(
                        e_var("object"),
                        "getWindow",
                        vec![e_var("begin"), e_var("end")],
                    ))
                    .build(),
                function("demo_complain")
                    .param("message", TypeExpr::Str)
                    .returns(TypeExpr::Void)
                    .body(vec![s_expr(e_call(
                        "fwrite",
                        vec![e_const("STDERR"), e_var("message")],
                    ))])
                    .build(),
            ]
        });

        assert_eq!(built.len(), parsed.len());
        for (built_stmt, parsed_stmt) in built.iter().zip(parsed.iter()) {
            assert_eq!(
                format!("{:?}", strip_spans(built_stmt)),
                format!("{:?}", strip_spans(parsed_stmt)),
            );
        }
    }

    /// The same agreement for control flow and the full expression vocabulary — what the
    /// algorithmic prelude bodies (`var_export`, the image helpers) are actually made of.
    ///
    /// The shape worth pinning here is INTERPOLATION. `"%.{$p}e"` has no AST node: the lexer
    /// resolves it into an ordinary concatenation token stream, so it must be built as
    /// `'%.' . $p . 'e'`, folded left-to-right. Building anything else silently diverges from
    /// every PHP prelude that used a double-quoted string.
    #[test]
    fn parser_agreement_control_flow_and_operators() {
        let parsed = parse_php(
            r#"<?php
function demo_render(mixed $value, int $indent): string {
    $out = '';
    for ($p = 0; $p <= 16; $p++) {
        $out = sprintf("%.{$p}e", $value);
        if ((float) $out === $value) {
            break;
        }
    }
    if (is_array($value)) {
        $pad = str_repeat(' ', $indent);
        foreach ($value as $k => $v) {
            $out = $out . $pad . $k . $v;
        }
    } elseif (is_string($value)) {
        $out = ($value[0] === '-') ? '-' : '+';
    } else {
        $out = (string) $value;
    }
    echo $out;
    return $out;
}
"#,
        );

        let built = internal_declarations(|| {
            vec![function("demo_render")
                .param("value", t_mixed())
                .param("indent", TypeExpr::Int)
                .returns(TypeExpr::Str)
                .body(vec![
                    s_assign("out", e_str("")),
                    s_for(
                        Some(s_assign("p", e_int(0))),
                        Some(e_binop(e_var("p"), BinOp::LtEq, e_int(16))),
                        Some(s_expr(e_post_inc("p"))),
                        vec![
                            s_assign(
                                "out",
                                e_call(
                                    "sprintf",
                                    vec![
                                        e_concat_all(vec![e_str("%."), e_var("p"), e_str("e")]),
                                        e_var("value"),
                                    ],
                                ),
                            ),
                            s_if(
                                e_binop(
                                    e_cast(CastType::Float, e_var("out")),
                                    BinOp::StrictEq,
                                    e_var("value"),
                                ),
                                vec![s_break(1)],
                                vec![],
                                None,
                            ),
                        ],
                    ),
                    s_if(
                        e_call("is_array", vec![e_var("value")]),
                        vec![
                            s_assign("pad", e_call("str_repeat", vec![e_str(" "), e_var("indent")])),
                            s_foreach(
                                e_var("value"),
                                Some("k"),
                                "v",
                                vec![s_assign(
                                    "out",
                                    e_concat_all(vec![
                                        e_var("out"),
                                        e_var("pad"),
                                        e_var("k"),
                                        e_var("v"),
                                    ]),
                                )],
                            ),
                        ],
                        vec![(
                            e_call("is_string", vec![e_var("value")]),
                            vec![s_assign(
                                "out",
                                e_ternary(
                                    e_binop(
                                        e_index(e_var("value"), e_int(0)),
                                        BinOp::StrictEq,
                                        e_str("-"),
                                    ),
                                    e_str("-"),
                                    e_str("+"),
                                ),
                            )],
                        )],
                        Some(vec![s_assign(
                            "out",
                            e_cast(CastType::String, e_var("value")),
                        )]),
                    ),
                    s_echo(e_var("out")),
                    s_return(e_var("out")),
                ])
                .build()]
        });

        assert_eq!(built.len(), parsed.len());
        for (built_stmt, parsed_stmt) in built.iter().zip(parsed.iter()) {
            assert_eq!(
                format!("{:?}", strip_spans(built_stmt)),
                format!("{:?}", strip_spans(parsed_stmt)),
            );
        }
    }

    /// Two shapes that look identical in PHP source but are DIFFERENT trees.
    ///
    /// - `else if` (two words) nests an `if` inside the `else` body; `elseif` (one word) fills
    ///   `elseif_clauses`. Transcribing one as the other builds an AST the source never
    ///   produced.
    /// - A negative literal is `Negate(IntLiteral(3))`, not `IntLiteral(-3)`. The parser does
    ///   not fold the sign in, so `e_int(-3)` diverges from every PHP prelude that wrote `-3`.
    ///
    /// Both were caught by an oracle while converting `var_export_prelude`, whose float layout
    /// cascade uses `else if` and compares against `-3`.
    #[test]
    fn parser_agreement_else_if_chain_and_negative_literals() {
        let parsed = parse_php(
            r#"<?php
function two_word(int $n): string {
    if ($n < -3) {
        return 'low';
    } else if ($n > 17) {
        return 'high';
    } else {
        return 'mid';
    }
}
function one_word(int $n): string {
    if ($n < -3) {
        return 'low';
    } elseif ($n > 17) {
        return 'high';
    } else {
        return 'mid';
    }
}
"#,
        );

        let built = internal_declarations(|| {
            vec![
                function("two_word")
                    .param("n", TypeExpr::Int)
                    .returns(TypeExpr::Str)
                    .body(vec![s_else_if(
                        vec![
                            (
                                e_binop(e_var("n"), BinOp::Lt, e_neg(e_int(3))),
                                vec![s_return(e_str("low"))],
                            ),
                            (
                                e_binop(e_var("n"), BinOp::Gt, e_int(17)),
                                vec![s_return(e_str("high"))],
                            ),
                        ],
                        Some(vec![s_return(e_str("mid"))]),
                    )])
                    .build(),
                function("one_word")
                    .param("n", TypeExpr::Int)
                    .returns(TypeExpr::Str)
                    .body(vec![s_if(
                        e_binop(e_var("n"), BinOp::Lt, e_neg(e_int(3))),
                        vec![s_return(e_str("low"))],
                        vec![(
                            e_binop(e_var("n"), BinOp::Gt, e_int(17)),
                            vec![s_return(e_str("high"))],
                        )],
                        Some(vec![s_return(e_str("mid"))]),
                    )])
                    .build(),
            ]
        });

        assert_eq!(built.len(), parsed.len());
        for (built_stmt, parsed_stmt) in built.iter().zip(parsed.iter()) {
            assert_eq!(
                format!("{:?}", strip_spans(built_stmt)),
                format!("{:?}", strip_spans(parsed_stmt)),
            );
        }

        // And the two forms really are different trees, so the distinction is not cosmetic.
        assert_ne!(
            format!("{:?}", strip_spans(&built[0])).replace("two_word", "f"),
            format!("{:?}", strip_spans(&built[1])).replace("one_word", "f"),
        );
    }

    /// The vocabulary the `--web` and OPcache preludes added: an INTERFACE (bodiless signatures),
    /// STATIC class properties and their `Class::$p` access, a function-local `static`, a
    /// `do`/`while`, a VARIADIC tail, and the `ptr` type.
    ///
    /// Each of these is a shape a hand-written builder would plausibly get wrong in a way nothing
    /// else notices: an interface method with `has_body: true` and an empty body type-checks
    /// differently from a signature, a `static` property read through `$this->` is a different
    /// node from `Class::$p`, and a dropped variadic tail silently changes an arity.
    #[test]
    fn parser_agreement_interfaces_statics_and_variadics() {
        let parsed = parse_php(
            r#"<?php
interface Sink {
    public function open(string $path, int $mode): bool;
    public function close(): bool;
}
class Registry {
    public static ?Sink $sink = null;
    public static int $count = 0;
    public function attach(Sink $sink): void {
        Registry::$sink = $sink;
        Registry::$count = 1;
    }
}
function tally(mixed ...$values): int {
    static $seen = 0;
    $i = 0;
    do {
        $seen = $seen + 1;
        $i = $i + 1;
    } while ($i < 3);
    if (Registry::$count === 0) { return 0; }
    return $seen;
}
function stage(string $data): ptr {
    return __elephc_stage($data);
}
"#,
        );

        let built = internal_declarations(|| {
            vec![
                interface("Sink")
                    .method(
                        method("open")
                            .param("path", TypeExpr::Str)
                            .param("mode", TypeExpr::Int)
                            .returns(TypeExpr::Bool),
                    )
                    .method(method("close").returns(TypeExpr::Bool))
                    .build(),
                class("Registry")
                    .static_prop("sink", t_nullable(t_class("Sink")), Some(e_null()))
                    .static_prop("count", TypeExpr::Int, Some(e_int(0)))
                    .method(
                        method("attach")
                            .param("sink", t_class("Sink"))
                            .returns(TypeExpr::Void)
                            .body(vec![
                                s_static_prop_assign("Registry", "sink", e_var("sink")),
                                s_static_prop_assign("Registry", "count", e_int(1)),
                            ]),
                    )
                    .build(),
                function("tally")
                    .variadic("values", Some(t_mixed()))
                    .returns(TypeExpr::Int)
                    .body(vec![
                        s_static("seen", e_int(0)),
                        s_assign("i", e_int(0)),
                        s_do_while(
                            vec![
                                s_assign("seen", e_binop(e_var("seen"), BinOp::Add, e_int(1))),
                                s_assign("i", e_binop(e_var("i"), BinOp::Add, e_int(1))),
                            ],
                            e_binop(e_var("i"), BinOp::Lt, e_int(3)),
                        ),
                        s_if(
                            e_binop(
                                e_static_prop("Registry", "count"),
                                BinOp::StrictEq,
                                e_int(0),
                            ),
                            vec![s_return(e_int(0))],
                            vec![],
                            None,
                        ),
                        s_return(e_var("seen")),
                    ])
                    .build(),
                function("stage")
                    .param("data", TypeExpr::Str)
                    .returns(t_ptr())
                    .returning(e_call("__elephc_stage", vec![e_var("data")]))
                    .build(),
            ]
        });

        assert_eq!(built.len(), parsed.len());
        for (built_stmt, parsed_stmt) in built.iter().zip(parsed.iter()) {
            assert_eq!(
                format!("{:?}", strip_spans(built_stmt)),
                format!("{:?}", strip_spans(parsed_stmt)),
            );
        }
    }

    /// Renders a statement without its spans so a synthetic node (`Span::dummy()`) and a parsed
    /// node (real line/column) compare on structure alone.
    ///
    /// `PartialEq` on `Stmt` already ignores spans, but it also ignores method PARAMS, RETURN
    /// TYPES and BODIES — the very things a prelude conversion must not drift on — so the
    /// comparison is done on a span-free debug rendering instead.
    fn strip_spans(stmt: &Stmt) -> String {
        let rendered = format!("{:?}", stmt);
        let mut cleaned = String::with_capacity(rendered.len());
        let mut rest = rendered.as_str();
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

    /// An unread parameter must be consumed, or the injection warns on every compile.
    #[test]
    fn unread_parameters_are_consumed() {
        let built = function("f")
            .param("used", TypeExpr::Str)
            .param("ignored", TypeExpr::Int)
            .returns(TypeExpr::Str)
            .returning(e_var("used"))
            .build();

        let StmtKind::FunctionDecl { body, .. } = &built.kind else {
            panic!("expected a function declaration");
        };
        assert_eq!(
            body.len(),
            2,
            "the unread parameter should have gained a consumption statement"
        );
        assert!(matches!(
            &body[0].kind,
            StmtKind::Assign { name, .. } if name == "_unused"
        ));
        assert!(reads_of(body).iter().any(|name| name == "ignored"));
    }

    /// Several unread parameters collapse into one `$_unused = [$a, $b];`.
    #[test]
    fn several_unread_parameters_share_one_consumption() {
        let built = method("m")
            .param("a", TypeExpr::Int)
            .param("b", TypeExpr::Int)
            .build();

        assert_eq!(built.body.len(), 1);
        let StmtKind::Assign { name, value } = &built.body[0].kind else {
            panic!("expected an assignment");
        };
        assert_eq!(name, "_unused");
        assert!(matches!(&value.kind, ExprKind::ArrayLiteral(items) if items.len() == 2));
    }

    /// A parameter read from inside a nested body still counts as read.
    #[test]
    fn reads_reach_into_call_arguments() {
        let built = function("f")
            .param("value", t_mixed())
            .returning(e_call("strlen", vec![e_var("value")]))
            .build();

        let StmtKind::FunctionDecl { body, .. } = &built.kind else {
            panic!("expected a function declaration");
        };
        assert_eq!(body.len(), 1, "a read parameter needs no consumption");
    }

    /// Declarations are built under the internal source mode, which exempts them from the
    /// user strict-PHP audit exactly as `parse_internal` did for the PHP form.
    #[test]
    fn declarations_are_internal_source_mode() {
        let built = crate::source::with_parse_mode(SourceProfile::new(SourceMode::Php), || {
            internal_declarations(|| vec![class("C").build()])
        });
        assert_eq!(built[0].source_mode, SourceMode::Internal);
    }
}
