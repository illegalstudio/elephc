use crate::parser::ast::{
    BinOp, CallableTarget, CastType, ClassMethod, ClassProperty, EnumCaseDecl, Expr, ExprKind,
    Program, Stmt, StmtKind,
};
use crate::termination::{block_terminal_effect, stmt_terminal_effect, TerminalEffect};
use std::cell::RefCell;
use std::collections::HashMap;

thread_local! {
    static ACTIVE_FUNCTION_EFFECTS: RefCell<Option<HashMap<String, Effect>>> = const { RefCell::new(None) };
    static ACTIVE_STATIC_METHOD_EFFECTS: RefCell<Option<HashMap<String, Effect>>> = const { RefCell::new(None) };
    static ACTIVE_CLASS_EFFECT_CONTEXT: RefCell<Option<ClassEffectContext>> = const { RefCell::new(None) };
    static ACTIVE_CALLABLE_ALIAS_EFFECTS: RefCell<Option<HashMap<String, Effect>>> = const { RefCell::new(None) };
}

pub fn fold_constants(program: Program) -> Program {
    program.into_iter().map(fold_stmt).collect()
}

pub fn prune_constant_control_flow(program: Program) -> Program {
    let (function_effects, static_method_effects) = compute_program_callable_effects(&program);
    with_callable_effects(function_effects, static_method_effects, || prune_block(program))
}

pub fn eliminate_dead_code(program: Program) -> Program {
    let (function_effects, static_method_effects) = compute_program_callable_effects(&program);
    with_callable_effects(function_effects, static_method_effects, || prune_block(program))
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Effect {
    has_side_effects: bool,
    may_throw: bool,
}

impl Effect {
    const PURE: Self = Self {
        has_side_effects: false,
        may_throw: false,
    };

    fn with_side_effects(mut self) -> Self {
        self.has_side_effects = true;
        self
    }

    fn with_may_throw(mut self) -> Self {
        self.may_throw = true;
        self
    }

    fn combine(self, other: Self) -> Self {
        Self {
            has_side_effects: self.has_side_effects || other.has_side_effects,
            may_throw: self.may_throw || other.may_throw,
        }
    }

    fn is_observable(self) -> bool {
        self.has_side_effects || self.may_throw
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ClassEffectContext {
    class_name: String,
    parent_name: Option<String>,
}

#[derive(Clone, Debug)]
struct StaticMethodBody {
    context: ClassEffectContext,
    body: Vec<Stmt>,
}

fn with_callable_effects<R>(
    function_effects: HashMap<String, Effect>,
    static_method_effects: HashMap<String, Effect>,
    f: impl FnOnce() -> R,
) -> R {
    ACTIVE_FUNCTION_EFFECTS.with(|function_slot| {
        ACTIVE_STATIC_METHOD_EFFECTS.with(|static_slot| {
            let previous_functions = function_slot.replace(Some(function_effects));
            let previous_static_methods = static_slot.replace(Some(static_method_effects));
            let result = f();
            static_slot.replace(previous_static_methods);
            function_slot.replace(previous_functions);
            result
        })
    })
}

fn with_class_effect_context<R>(context: Option<ClassEffectContext>, f: impl FnOnce() -> R) -> R {
    ACTIVE_CLASS_EFFECT_CONTEXT.with(|slot| {
        let previous = slot.replace(context);
        let result = f();
        slot.replace(previous);
        result
    })
}

fn with_callable_alias_effects<R>(
    alias_effects: HashMap<String, Effect>,
    f: impl FnOnce() -> R,
) -> R {
    ACTIVE_CALLABLE_ALIAS_EFFECTS.with(|slot| {
        let previous = slot.replace(Some(alias_effects));
        let result = f();
        slot.replace(previous);
        result
    })
}

fn current_callable_alias_effects() -> HashMap<String, Effect> {
    ACTIVE_CALLABLE_ALIAS_EFFECTS.with(|slot| slot.borrow().clone().unwrap_or_default())
}

fn compute_program_callable_effects(
    program: &[Stmt],
) -> (HashMap<String, Effect>, HashMap<String, Effect>) {
    let mut function_bodies = HashMap::new();
    collect_program_function_bodies(program, &mut function_bodies);
    let mut static_method_bodies = HashMap::new();
    collect_program_static_method_bodies(program, &mut static_method_bodies);

    let mut function_effects: HashMap<String, Effect> = function_bodies
        .keys()
        .cloned()
        .map(|name| (name, Effect::PURE))
        .collect();
    let mut static_method_effects: HashMap<String, Effect> = static_method_bodies
        .keys()
        .cloned()
        .map(|name| (name, Effect::PURE))
        .collect();

    loop {
        let function_snapshot = function_effects.clone();
        let static_method_snapshot = static_method_effects.clone();
        let mut changed = false;

        ACTIVE_FUNCTION_EFFECTS.with(|function_slot| {
            ACTIVE_STATIC_METHOD_EFFECTS.with(|static_slot| {
                let previous_functions = function_slot.replace(Some(function_snapshot));
                let previous_static_methods = static_slot.replace(Some(static_method_snapshot));

                for (name, body) in &function_bodies {
                    let effect = block_effect(body);
                    if function_effects.get(name).copied() != Some(effect) {
                        function_effects.insert(name.clone(), effect);
                        changed = true;
                    }
                }

                for (name, method) in &static_method_bodies {
                    let effect = with_class_effect_context(Some(method.context.clone()), || {
                        block_effect(&method.body)
                    });
                    if static_method_effects.get(name).copied() != Some(effect) {
                        static_method_effects.insert(name.clone(), effect);
                        changed = true;
                    }
                }

                static_slot.replace(previous_static_methods);
                function_slot.replace(previous_functions);
            });
        });

        if !changed {
            return (function_effects, static_method_effects);
        }
    }
}

fn collect_program_function_bodies(stmts: &[Stmt], out: &mut HashMap<String, Vec<Stmt>>) {
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::FunctionDecl { name, body, .. } => {
                out.insert(name.clone(), body.clone());
            }
            StmtKind::NamespaceBlock { body, .. } => collect_program_function_bodies(body, out),
            _ => {}
        }
    }
}

fn collect_program_static_method_bodies(
    stmts: &[Stmt],
    out: &mut HashMap<String, StaticMethodBody>,
) {
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::ClassDecl {
                name,
                extends,
                methods,
                ..
            } => {
                let context = ClassEffectContext {
                    class_name: name.clone(),
                    parent_name: extends.as_ref().map(|parent| parent.as_str().to_string()),
                };
                for method in methods {
                    if method.is_static && method.has_body {
                        out.insert(
                            method_effect_key(name, &method.name),
                            StaticMethodBody {
                                context: context.clone(),
                                body: method.body.clone(),
                            },
                        );
                    }
                }
            }
            StmtKind::NamespaceBlock { body, .. } => collect_program_static_method_bodies(body, out),
            _ => {}
        }
    }
}

fn method_effect_key(class_name: &str, method_name: &str) -> String {
    format!("{class_name}::{method_name}")
}

fn expr_to_effect_stmt(expr: Expr) -> Vec<Stmt> {
    let span = expr.span;
    if expr_is_observable(&expr) {
        vec![Stmt::new(StmtKind::ExprStmt(expr), span)]
    } else {
        Vec::new()
    }
}

fn normalize_optional_block(body: Option<Vec<Stmt>>) -> Option<Vec<Stmt>> {
    body.filter(|body| !body.is_empty())
}

fn invert_condition(condition: Expr) -> Expr {
    let span = condition.span;
    prune_expr(Expr::new(ExprKind::Not(Box::new(condition)), span))
}

fn build_if_chain_body(
    elseif_clauses: Vec<(Expr, Vec<Stmt>)>,
    else_body: Option<Vec<Stmt>>,
) -> Vec<Stmt> {
    if let Some(((condition, then_body), rest)) = elseif_clauses.split_first() {
        vec![Stmt::new(
            StmtKind::If {
                condition: condition.clone(),
                then_body: then_body.clone(),
                elseif_clauses: rest.to_vec(),
                else_body,
            },
            condition.span,
        )]
    } else {
        else_body.unwrap_or_default()
    }
}

fn materialize_switch_execution(
    cases: &[(Vec<Expr>, Vec<Stmt>)],
    default: &Option<Vec<Stmt>>,
    start_case_index: Option<usize>,
) -> Vec<Stmt> {
    let mut out = Vec::new();

    let push_body = |body: &[Stmt], out: &mut Vec<Stmt>| -> bool {
        for stmt in body.iter().cloned() {
            if matches!(stmt.kind, StmtKind::Break) {
                return true;
            }

            let stops_here = !matches!(stmt_terminal_effect(&stmt), TerminalEffect::FallsThrough);
            out.push(stmt);
            if stops_here {
                return true;
            }
        }

        false
    };

    if let Some(start_case_index) = start_case_index {
        for (_, body) in &cases[start_case_index..] {
            if push_body(body, &mut out) {
                return out;
            }
        }
    }

    if let Some(default_body) = default {
        let _ = push_body(default_body, &mut out);
    }

    out
}

fn split_hoistable_try_prefix(mut try_body: Vec<Stmt>) -> (Vec<Stmt>, Vec<Stmt>) {
    let hoist_len = try_body
        .iter()
        .take_while(|stmt| {
            !stmt_may_throw(stmt)
                && matches!(stmt_terminal_effect(stmt), TerminalEffect::FallsThrough)
        })
        .count();
    let tail = try_body.split_off(hoist_len);
    (try_body, tail)
}

fn combine_if_conditions(left: Expr, right: Expr) -> Expr {
    let span = left.span;
    prune_expr(Expr::new(
        ExprKind::BinaryOp {
            left: Box::new(left),
            op: BinOp::And,
            right: Box::new(right),
        },
        span,
    ))
}

fn build_if_stmt(
    condition: Expr,
    then_body: Vec<Stmt>,
    elseif_clauses: Vec<(Expr, Vec<Stmt>)>,
    else_body: Option<Vec<Stmt>>,
    span: crate::span::Span,
) -> Stmt {
    if elseif_clauses.is_empty() && else_body.is_none() && then_body.len() == 1 {
        if let StmtKind::If {
            condition: inner_condition,
            then_body: inner_then_body,
            elseif_clauses: inner_elseifs,
            else_body: inner_else,
        } = &then_body[0].kind
        {
            if inner_elseifs.is_empty() && inner_else.is_none() {
                return Stmt {
                    kind: StmtKind::If {
                        condition: combine_if_conditions(condition, inner_condition.clone()),
                        then_body: inner_then_body.clone(),
                        elseif_clauses: Vec::new(),
                        else_body: None,
                    },
                    span,
                };
            }
        }
    }

    Stmt {
        kind: StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        },
        span,
    }
}

fn block_may_throw(stmts: &[Stmt]) -> bool {
    block_effect(stmts).may_throw
}

fn stmt_may_throw(stmt: &Stmt) -> bool {
    stmt_effect(stmt).may_throw
}

fn stmt_effect(stmt: &Stmt) -> Effect {
    match &stmt.kind {
        StmtKind::Echo(expr) => expr_effect(expr).with_side_effects(),
        StmtKind::ExprStmt(expr)
        | StmtKind::ConstDecl { value: expr, .. }
        | StmtKind::StaticVar { init: expr, .. }
        | StmtKind::ListUnpack { value: expr, .. }
        | StmtKind::Return(Some(expr)) => expr_effect(expr),
        StmtKind::Throw(expr) => expr_effect(expr).with_side_effects().with_may_throw(),
        StmtKind::Assign { value, .. }
        | StmtKind::TypedAssign { value, .. }
        | StmtKind::ArrayPush { value, .. } => expr_effect(value).with_side_effects(),
        StmtKind::ArrayAssign { index, value, .. }
        | StmtKind::PropertyArrayAssign { index, value, .. } => {
            expr_effect(index)
                .combine(expr_effect(value))
                .with_side_effects()
        }
        StmtKind::PropertyAssign { object, value, .. }
        | StmtKind::PropertyArrayPush { object, value, .. } => {
            expr_effect(object)
                .combine(expr_effect(value))
                .with_side_effects()
        }
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => expr_effect(condition)
            .combine(block_effect(then_body))
            .combine(combine_effects(
                elseif_clauses.iter().map(|(condition, body)| {
                    expr_effect(condition).combine(block_effect(body))
                }),
            ))
            .combine(
                else_body
                    .as_ref()
                    .map(|body| block_effect(body))
                    .unwrap_or(Effect::PURE),
            ),
        StmtKind::IfDef {
            then_body,
            else_body,
            ..
        } => block_effect(then_body).combine(
            else_body
                .as_ref()
                .map(|body| block_effect(body))
                .unwrap_or(Effect::PURE),
        ),
        StmtKind::While { condition, body } | StmtKind::DoWhile { condition, body } => {
            expr_effect(condition).combine(block_effect(body))
        }
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => init
            .as_ref()
            .map(|stmt| stmt_effect(stmt))
            .unwrap_or(Effect::PURE)
            .combine(
                condition
                    .as_ref()
                    .map(|expr| expr_effect(expr))
                    .unwrap_or(Effect::PURE),
            )
            .combine(
                update
                    .as_ref()
                    .map(|stmt| stmt_effect(stmt))
                    .unwrap_or(Effect::PURE),
            )
            .combine(block_effect(body)),
        StmtKind::Foreach { array, body, .. } => expr_effect(array)
            .combine(block_effect(body))
            .with_side_effects(),
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => expr_effect(subject).combine(combine_effects(cases.iter().map(|(patterns, body)| {
            combine_effects(patterns.iter().map(expr_effect)).combine(block_effect(body))
        })))
        .combine(
            default
                .as_ref()
                .map(|body| block_effect(body))
                .unwrap_or(Effect::PURE),
        ),
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => block_effect(try_body)
            .combine(combine_effects(
                catches.iter().map(|catch| block_effect(&catch.body)),
            ))
            .combine(
                finally_body
                    .as_ref()
                    .map(|body| block_effect(body))
                    .unwrap_or(Effect::PURE),
            ),
        StmtKind::NamespaceBlock { body, .. } => block_effect(body),
        StmtKind::FunctionDecl { .. }
        | StmtKind::NamespaceDecl { .. }
        | StmtKind::UseDecl { .. }
        | StmtKind::ClassDecl { .. }
        | StmtKind::EnumDecl { .. }
        | StmtKind::PackedClassDecl { .. }
        | StmtKind::InterfaceDecl { .. }
        | StmtKind::TraitDecl { .. }
        | StmtKind::Global { .. }
        | StmtKind::Return(None)
        | StmtKind::Break
        | StmtKind::Continue
        | StmtKind::ExternFunctionDecl { .. }
        | StmtKind::ExternClassDecl { .. }
        | StmtKind::ExternGlobalDecl { .. } => Effect::PURE,
        StmtKind::Include { .. } => Effect::PURE.with_side_effects().with_may_throw(),
    }
}

fn expr_is_observable(expr: &Expr) -> bool {
    expr_effect(expr).is_observable()
}

fn expr_effect(expr: &Expr) -> Effect {
    match &expr.kind {
        ExprKind::StringLiteral(_)
        | ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::Variable(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::Null
        | ExprKind::ConstRef(_)
        | ExprKind::EnumCase { .. }
        | ExprKind::This => Effect::PURE,
        ExprKind::Negate(inner)
        | ExprKind::Not(inner)
        | ExprKind::BitNot(inner)
        | ExprKind::Cast { expr: inner, .. }
        | ExprKind::PtrCast { expr: inner, .. }
        | ExprKind::Spread(inner) => expr_effect(inner),
        ExprKind::BinaryOp { left, right, .. } => expr_effect(left).combine(expr_effect(right)),
        ExprKind::Throw(inner) => expr_effect(inner).with_side_effects().with_may_throw(),
        ExprKind::NullCoalesce { value, default } => expr_effect(value).combine(expr_effect(default)),
        ExprKind::PreIncrement(_)
        | ExprKind::PostIncrement(_)
        | ExprKind::PreDecrement(_)
        | ExprKind::PostDecrement(_) => Effect::PURE.with_side_effects(),
        ExprKind::FunctionCall { name, args } => combine_effects(args.iter().map(expr_effect))
            .combine(function_call_effect(name.as_str())),
        ExprKind::ClosureCall { var, args } => combine_effects(args.iter().map(expr_effect))
            .combine(callable_alias_effect(var)),
        ExprKind::ExprCall { callee, args } => expr_effect(callee)
            .combine(combine_effects(args.iter().map(expr_effect)))
            .combine(expr_call_effect(callee)),
        ExprKind::NewObject { args, .. } => combine_effects(args.iter().map(expr_effect))
            .with_side_effects()
            .with_may_throw(),
        ExprKind::MethodCall { object, args, .. } => expr_effect(object)
            .combine(combine_effects(args.iter().map(expr_effect)))
            .with_side_effects()
            .with_may_throw(),
        ExprKind::StaticMethodCall {
            receiver,
            method,
            args,
        } => combine_effects(args.iter().map(expr_effect))
            .combine(static_method_call_effect(receiver, method)),
        ExprKind::ArrayLiteral(items) => combine_effects(items.iter().map(expr_effect)),
        ExprKind::ArrayLiteralAssoc(items) => combine_effects(
            items
                .iter()
                .map(|(key, value)| expr_effect(key).combine(expr_effect(value))),
        ),
        ExprKind::Match {
            subject,
            arms,
            default,
        } => expr_effect(subject)
            .combine(combine_effects(arms.iter().map(|(patterns, value)| {
                combine_effects(patterns.iter().map(expr_effect)).combine(expr_effect(value))
            })))
            .combine(
                default
                    .as_ref()
                    .map(|expr| expr_effect(expr))
                    .unwrap_or(Effect::PURE),
            ),
        ExprKind::ArrayAccess { array, index } => expr_effect(array).combine(expr_effect(index)),
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => expr_effect(condition)
            .combine(expr_effect(then_expr))
            .combine(expr_effect(else_expr)),
        ExprKind::Closure { .. } => Effect::PURE,
        ExprKind::NamedArg { value, .. } => expr_effect(value),
        ExprKind::PropertyAccess { object, .. } => expr_effect(object),
        ExprKind::FirstClassCallable(target) => callable_target_effect(target),
        ExprKind::BufferNew { len, .. } => expr_effect(len).with_side_effects(),
    }
}

fn block_effect(stmts: &[Stmt]) -> Effect {
    let mut aliases = current_callable_alias_effects();
    let mut effect = Effect::PURE;
    for stmt in stmts {
        let stmt_effect = with_callable_alias_effects(aliases.clone(), || stmt_effect(stmt));
        effect = effect.combine(stmt_effect);
        apply_stmt_callable_aliases(stmt, &mut aliases);
        if !matches!(stmt_terminal_effect(stmt), TerminalEffect::FallsThrough) {
            break;
        }
    }
    effect
}

fn combine_effects(effects: impl IntoIterator<Item = Effect>) -> Effect {
    effects
        .into_iter()
        .fold(Effect::PURE, |acc, effect| acc.combine(effect))
}

fn function_call_effect(name: &str) -> Effect {
    ACTIVE_FUNCTION_EFFECTS.with(|slot| {
        slot.borrow()
            .as_ref()
            .and_then(|effects| effects.get(name).copied())
    })
    .unwrap_or_else(|| {
        if is_pure_non_throwing_builtin(name) {
            Effect::PURE
        } else {
            Effect::PURE.with_side_effects().with_may_throw()
        }
    })
}

fn expr_call_effect(callee: &Expr) -> Effect {
    match &callee.kind {
        ExprKind::FirstClassCallable(target) => callable_target_call_effect(target),
        _ => Effect::PURE.with_side_effects().with_may_throw(),
    }
}

fn callable_alias_effect(name: &str) -> Effect {
    ACTIVE_CALLABLE_ALIAS_EFFECTS.with(|slot| {
        slot.borrow()
            .as_ref()
            .and_then(|effects| effects.get(name).copied())
    })
    .unwrap_or_else(|| Effect::PURE.with_side_effects().with_may_throw())
}

fn callable_target_call_effect(target: &CallableTarget) -> Effect {
    match target {
        CallableTarget::Function(name) => function_call_effect(name.as_str()),
        CallableTarget::StaticMethod { receiver, method } => static_method_call_effect(receiver, method),
        CallableTarget::Method { object, .. } => expr_effect(object)
            .with_side_effects()
            .with_may_throw(),
    }
}

fn static_method_call_effect(
    receiver: &crate::parser::ast::StaticReceiver,
    method_name: &str,
) -> Effect {
    let Some(class_name) = resolve_static_receiver_class(receiver) else {
        return Effect::PURE.with_side_effects().with_may_throw();
    };

    ACTIVE_STATIC_METHOD_EFFECTS.with(|slot| {
        slot.borrow()
            .as_ref()
            .and_then(|effects| effects.get(&method_effect_key(&class_name, method_name)).copied())
    })
    .unwrap_or_else(|| Effect::PURE.with_side_effects().with_may_throw())
}

fn resolve_static_receiver_class(receiver: &crate::parser::ast::StaticReceiver) -> Option<String> {
    match receiver {
        crate::parser::ast::StaticReceiver::Named(class_name) => Some(class_name.as_str().to_string()),
        crate::parser::ast::StaticReceiver::Self_ => ACTIVE_CLASS_EFFECT_CONTEXT
            .with(|slot| slot.borrow().as_ref().map(|context| context.class_name.clone())),
        crate::parser::ast::StaticReceiver::Parent => ACTIVE_CLASS_EFFECT_CONTEXT.with(|slot| {
            slot.borrow()
                .as_ref()
                .and_then(|context| context.parent_name.clone())
        }),
        crate::parser::ast::StaticReceiver::Static => None,
    }
}

fn is_pure_non_throwing_builtin(name: &str) -> bool {
    matches!(
        name,
        "strlen"
            | "count"
            | "intval"
            | "floatval"
            | "boolval"
            | "gettype"
            | "is_array"
            | "is_bool"
            | "is_float"
            | "is_int"
            | "is_null"
            | "is_numeric"
            | "is_string"
            | "abs"
            | "min"
            | "max"
            | "floor"
            | "ceil"
            | "round"
            | "sqrt"
            | "pow"
            | "fmod"
            | "fdiv"
            | "sin"
            | "cos"
            | "tan"
            | "asin"
            | "acos"
            | "atan"
            | "atan2"
            | "deg2rad"
            | "rad2deg"
            | "sinh"
            | "cosh"
            | "tanh"
            | "log"
            | "log2"
            | "log10"
            | "exp"
            | "hypot"
            | "pi"
            | "number_format"
            | "substr"
            | "strpos"
            | "strrpos"
            | "strstr"
            | "str_replace"
            | "str_ireplace"
            | "substr_replace"
            | "strtolower"
            | "strtoupper"
            | "ucfirst"
            | "lcfirst"
            | "ucwords"
            | "trim"
            | "ltrim"
            | "rtrim"
            | "str_repeat"
            | "strrev"
            | "str_pad"
            | "explode"
            | "implode"
            | "str_split"
            | "strcmp"
            | "strcasecmp"
            | "str_contains"
            | "str_starts_with"
            | "str_ends_with"
            | "ord"
            | "chr"
            | "nl2br"
            | "wordwrap"
            | "addslashes"
            | "stripslashes"
            | "htmlspecialchars"
            | "htmlentities"
            | "html_entity_decode"
            | "urlencode"
            | "urldecode"
            | "rawurlencode"
            | "rawurldecode"
            | "md5"
            | "sha1"
            | "hash"
            | "base64_encode"
            | "base64_decode"
            | "bin2hex"
            | "hex2bin"
            | "ctype_alpha"
            | "ctype_digit"
            | "ctype_alnum"
            | "ctype_space"
            | "array_key_exists"
            | "array_search"
            | "array_keys"
            | "array_values"
            | "array_merge"
            | "array_slice"
            | "array_combine"
            | "array_flip"
            | "array_reverse"
            | "array_unique"
            | "array_column"
            | "array_sum"
            | "array_product"
            | "array_chunk"
            | "array_pad"
            | "array_fill"
            | "array_fill_keys"
            | "array_diff"
            | "array_intersect"
            | "array_diff_key"
            | "array_intersect_key"
            | "range"
            | "json_encode"
            | "json_decode"
            | "json_last_error"
    )
}

fn fold_stmt(stmt: Stmt) -> Stmt {
    let span = stmt.span;
    let kind = match stmt.kind {
        StmtKind::Echo(expr) => StmtKind::Echo(fold_expr(expr)),
        StmtKind::Assign { name, value } => StmtKind::Assign {
            name,
            value: fold_expr(value),
        },
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => StmtKind::If {
            condition: fold_expr(condition),
            then_body: fold_block(then_body),
            elseif_clauses: elseif_clauses
                .into_iter()
                .map(|(condition, body)| (fold_expr(condition), fold_block(body)))
                .collect(),
            else_body: else_body.map(fold_block),
        },
        StmtKind::IfDef {
            symbol,
            then_body,
            else_body,
        } => StmtKind::IfDef {
            symbol,
            then_body: fold_block(then_body),
            else_body: else_body.map(fold_block),
        },
        StmtKind::While { condition, body } => StmtKind::While {
            condition: fold_expr(condition),
            body: fold_block(body),
        },
        StmtKind::DoWhile { body, condition } => StmtKind::DoWhile {
            body: fold_block(body),
            condition: fold_expr(condition),
        },
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => StmtKind::For {
            init: init.map(|stmt| Box::new(fold_stmt(*stmt))),
            condition: condition.map(fold_expr),
            update: update.map(|stmt| Box::new(fold_stmt(*stmt))),
            body: fold_block(body),
        },
        StmtKind::ArrayAssign {
            array,
            index,
            value,
        } => StmtKind::ArrayAssign {
            array,
            index: fold_expr(index),
            value: fold_expr(value),
        },
        StmtKind::ArrayPush { array, value } => StmtKind::ArrayPush {
            array,
            value: fold_expr(value),
        },
        StmtKind::TypedAssign {
            type_expr,
            name,
            value,
        } => StmtKind::TypedAssign {
            type_expr,
            name,
            value: fold_expr(value),
        },
        StmtKind::Foreach {
            array,
            key_var,
            value_var,
            body,
        } => StmtKind::Foreach {
            array: fold_expr(array),
            key_var,
            value_var,
            body: fold_block(body),
        },
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => StmtKind::Switch {
            subject: fold_expr(subject),
            cases: cases
                .into_iter()
                .map(|(exprs, body)| {
                    (
                        exprs.into_iter().map(fold_expr).collect(),
                        fold_block(body),
                    )
                })
                .collect(),
            default: default.map(fold_block),
        },
        StmtKind::Include {
            path,
            once,
            required,
        } => StmtKind::Include {
            path,
            once,
            required,
        },
        StmtKind::Throw(expr) => StmtKind::Throw(fold_expr(expr)),
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => StmtKind::Try {
            try_body: fold_block(try_body),
            catches: catches
                .into_iter()
                .map(|catch| crate::parser::ast::CatchClause {
                    exception_types: catch.exception_types,
                    variable: catch.variable,
                    body: fold_block(catch.body),
                })
                .collect(),
            finally_body: finally_body.map(fold_block),
        },
        StmtKind::Break => StmtKind::Break,
        StmtKind::Continue => StmtKind::Continue,
        StmtKind::ExprStmt(expr) => StmtKind::ExprStmt(fold_expr(expr)),
        StmtKind::NamespaceDecl { name } => StmtKind::NamespaceDecl { name },
        StmtKind::NamespaceBlock { name, body } => StmtKind::NamespaceBlock {
            name,
            body: fold_block(body),
        },
        StmtKind::UseDecl { imports } => StmtKind::UseDecl { imports },
        StmtKind::FunctionDecl {
            name,
            params,
            variadic,
            return_type,
            body,
        } => StmtKind::FunctionDecl {
            name,
            params: fold_params(params),
            variadic,
            return_type,
            body: fold_block(body),
        },
        StmtKind::Return(expr) => StmtKind::Return(expr.map(fold_expr)),
        StmtKind::ConstDecl { name, value } => StmtKind::ConstDecl {
            name,
            value: fold_expr(value),
        },
        StmtKind::ListUnpack { vars, value } => StmtKind::ListUnpack {
            vars,
            value: fold_expr(value),
        },
        StmtKind::Global { vars } => StmtKind::Global { vars },
        StmtKind::StaticVar { name, init } => StmtKind::StaticVar {
            name,
            init: fold_expr(init),
        },
        StmtKind::ClassDecl {
            name,
            extends,
            implements,
            is_abstract,
            is_readonly_class,
            trait_uses,
            properties,
            methods,
        } => StmtKind::ClassDecl {
            name,
            extends,
            implements,
            is_abstract,
            is_readonly_class,
            trait_uses,
            properties: properties.into_iter().map(fold_property).collect(),
            methods: methods.into_iter().map(fold_method).collect(),
        },
        StmtKind::EnumDecl {
            name,
            backing_type,
            cases,
        } => StmtKind::EnumDecl {
            name,
            backing_type,
            cases: cases.into_iter().map(fold_enum_case).collect(),
        },
        StmtKind::PackedClassDecl { name, fields } => StmtKind::PackedClassDecl { name, fields },
        StmtKind::InterfaceDecl {
            name,
            extends,
            methods,
        } => StmtKind::InterfaceDecl {
            name,
            extends,
            methods: methods.into_iter().map(fold_method).collect(),
        },
        StmtKind::TraitDecl {
            name,
            trait_uses,
            properties,
            methods,
        } => StmtKind::TraitDecl {
            name,
            trait_uses,
            properties: properties.into_iter().map(fold_property).collect(),
            methods: methods.into_iter().map(fold_method).collect(),
        },
        StmtKind::PropertyAssign {
            object,
            property,
            value,
        } => StmtKind::PropertyAssign {
            object: Box::new(fold_expr(*object)),
            property,
            value: fold_expr(value),
        },
        StmtKind::PropertyArrayPush {
            object,
            property,
            value,
        } => StmtKind::PropertyArrayPush {
            object: Box::new(fold_expr(*object)),
            property,
            value: fold_expr(value),
        },
        StmtKind::PropertyArrayAssign {
            object,
            property,
            index,
            value,
        } => StmtKind::PropertyArrayAssign {
            object: Box::new(fold_expr(*object)),
            property,
            index: fold_expr(index),
            value: fold_expr(value),
        },
        StmtKind::ExternFunctionDecl {
            name,
            params,
            return_type,
            library,
        } => StmtKind::ExternFunctionDecl {
            name,
            params,
            return_type,
            library,
        },
        StmtKind::ExternClassDecl { name, fields } => StmtKind::ExternClassDecl { name, fields },
        StmtKind::ExternGlobalDecl { name, c_type } => {
            StmtKind::ExternGlobalDecl { name, c_type }
        }
    };
    Stmt { kind, span }
}

fn fold_block(body: Vec<Stmt>) -> Vec<Stmt> {
    body.into_iter().map(fold_stmt).collect()
}

fn prune_block(body: Vec<Stmt>) -> Vec<Stmt> {
    let mut pruned = Vec::new();
    for stmt in body {
        let pruned_stmt = prune_stmt(stmt);
        let stops_here = pruned_stmt
            .last()
            .is_some_and(|stmt| !matches!(stmt_terminal_effect(stmt), TerminalEffect::FallsThrough));
        pruned.extend(pruned_stmt);
        if stops_here {
            break;
        }
    }
    pruned
}

fn prune_stmt(stmt: Stmt) -> Vec<Stmt> {
    let span = stmt.span;
    match stmt.kind {
        StmtKind::Echo(expr) => vec![Stmt {
            kind: StmtKind::Echo(prune_expr(expr)),
            span,
        }],
        StmtKind::Assign { name, value } => vec![Stmt {
            kind: StmtKind::Assign {
                name,
                value: prune_expr(value),
            },
            span,
        }],
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => prune_if_chain(condition, then_body, elseif_clauses, else_body),
        StmtKind::IfDef {
            symbol,
            then_body,
            else_body,
        } => {
            let then_body = prune_block(then_body);
            let else_body = normalize_optional_block(else_body.map(prune_block));
            if then_body.is_empty() && else_body.is_none() {
                Vec::new()
            } else {
                vec![Stmt {
                    kind: StmtKind::IfDef {
                        symbol,
                        then_body,
                        else_body,
                    },
                    span,
                }]
            }
        }
        StmtKind::While { condition, body } => {
            let condition = prune_expr(condition);
            match scalar_value(&condition) {
            Some(value) if !value.truthy() => Vec::new(),
            _ => vec![Stmt {
                kind: StmtKind::While {
                    condition,
                    body: prune_block(body),
                },
                span,
            }],
        }
        }
        StmtKind::DoWhile { body, condition } => {
            let condition = prune_expr(condition);
            match scalar_value(&condition) {
            Some(value) if !value.truthy() => prune_block(body),
            _ => vec![Stmt {
                kind: StmtKind::DoWhile {
                    body: prune_block(body),
                    condition,
                },
                span,
            }],
        }
        }
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => {
            let init = prune_for_clause(init);
            let condition = condition.map(prune_expr);
            let update = prune_for_clause(update);
            match condition.as_ref().and_then(scalar_value) {
                Some(value) if !value.truthy() => init.map(|stmt| vec![*stmt]).unwrap_or_default(),
                _ => vec![Stmt {
                    kind: StmtKind::For {
                        init,
                        condition,
                        update,
                        body: prune_block(body),
                    },
                    span,
                }],
            }
        }
        StmtKind::Foreach {
            array,
            key_var,
            value_var,
            body,
        } => vec![Stmt {
            kind: StmtKind::Foreach {
                array: prune_expr(array),
                key_var,
                value_var,
                body: prune_block(body),
            },
            span,
        }],
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => prune_switch_stmt(subject, cases, default, span),
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            let try_body = prune_block(try_body);
            let catches: Vec<_> = catches
                .into_iter()
                .map(|catch| crate::parser::ast::CatchClause {
                    exception_types: catch.exception_types,
                    variable: catch.variable,
                    body: prune_block(catch.body),
                })
                .collect();
            let finally_body = normalize_optional_block(finally_body.map(prune_block));
            let (mut hoisted_prefix, try_body) = split_hoistable_try_prefix(try_body);

            let mut remaining = if try_body.is_empty() {
                finally_body.unwrap_or_default()
            } else if !block_may_throw(&try_body) {
                if let Some(finally_body) = finally_body {
                    if matches!(block_terminal_effect(&try_body), TerminalEffect::FallsThrough) {
                        let mut stmts = try_body;
                        stmts.extend(finally_body);
                        stmts
                    } else {
                        vec![Stmt {
                            kind: StmtKind::Try {
                                try_body,
                                catches,
                                finally_body: Some(finally_body),
                            },
                            span,
                        }]
                    }
                } else {
                    try_body
                }
            } else if catches.is_empty() && finally_body.is_none() {
                try_body
            } else {
                vec![Stmt {
                    kind: StmtKind::Try {
                        try_body,
                        catches,
                        finally_body,
                    },
                    span,
                }]
            };
            hoisted_prefix.append(&mut remaining);
            hoisted_prefix
        }
        StmtKind::NamespaceBlock { name, body } => vec![Stmt {
            kind: StmtKind::NamespaceBlock {
                name,
                body: prune_block(body),
            },
            span,
        }],
        StmtKind::FunctionDecl {
            name,
            params,
            variadic,
            return_type,
            body,
        } => vec![Stmt {
            kind: StmtKind::FunctionDecl {
                name,
                params,
                variadic,
                return_type,
                body: prune_block(body),
            },
            span,
        }],
        StmtKind::Return(expr) => vec![Stmt {
            kind: StmtKind::Return(expr.map(prune_expr)),
            span,
        }],
        StmtKind::ClassDecl {
            name,
            extends,
            implements,
            is_abstract,
            is_readonly_class,
            trait_uses,
            properties,
            methods,
        } => {
            let parent_name = extends.as_ref().map(|parent| parent.as_str().to_string());
            let methods = methods
                .into_iter()
                .map(|method| prune_method(method, &name, parent_name.as_deref()))
                .collect();
            vec![Stmt {
                kind: StmtKind::ClassDecl {
                    name,
                    extends,
                    implements,
                    is_abstract,
                    is_readonly_class,
                    trait_uses,
                    properties,
                    methods,
                },
                span,
            }]
        }
        StmtKind::ExprStmt(expr) => {
            let expr = prune_expr(expr);
            if expr_has_side_effects(&expr) {
                vec![Stmt {
                    kind: StmtKind::ExprStmt(expr),
                    span,
                }]
            } else {
                Vec::new()
            }
        }
        StmtKind::EnumDecl {
            name,
            backing_type,
            cases,
        } => vec![Stmt {
            kind: StmtKind::EnumDecl {
                name,
                backing_type,
                cases,
            },
            span,
        }],
        StmtKind::PackedClassDecl { name, fields } => vec![Stmt {
            kind: StmtKind::PackedClassDecl { name, fields },
            span,
        }],
        StmtKind::InterfaceDecl {
            name,
            extends,
            methods,
        } => vec![Stmt {
            kind: StmtKind::InterfaceDecl {
                name,
                extends,
                methods: methods
                    .into_iter()
                    .map(prune_method_without_context)
                    .collect(),
            },
            span,
        }],
        StmtKind::TraitDecl {
            name,
            trait_uses,
            properties,
            methods,
        } => vec![Stmt {
            kind: StmtKind::TraitDecl {
                name,
                trait_uses,
                properties,
                methods: methods
                    .into_iter()
                    .map(prune_method_without_context)
                    .collect(),
            },
            span,
        }],
        kind => vec![Stmt { kind, span }],
    }
}

fn prune_if_chain(
    condition: Expr,
    then_body: Vec<Stmt>,
    elseif_clauses: Vec<(Expr, Vec<Stmt>)>,
    else_body: Option<Vec<Stmt>>,
) -> Vec<Stmt> {
    let condition = prune_expr(condition);
    match scalar_value(&condition) {
        Some(value) if value.truthy() => prune_block(then_body),
        Some(_) => prune_else_if_chain(elseif_clauses, else_body),
        None => {
            let span = condition.span;
            let then_body = prune_block(then_body);
            let (kept_elseifs, kept_else) = prune_remaining_elseif_chain(elseif_clauses, else_body);
            let kept_else = normalize_optional_block(kept_else);

            if then_body.is_empty() && kept_elseifs.is_empty() && kept_else.is_none() {
                return expr_to_effect_stmt(condition);
            }

            if then_body.is_empty() {
                let fallback_body =
                    build_if_chain_body(kept_elseifs.clone(), kept_else.clone());
                if !fallback_body.is_empty() {
                    return vec![build_if_stmt(
                        invert_condition(condition),
                        fallback_body,
                        Vec::new(),
                        None,
                        span,
                    )];
                }
            }

            vec![build_if_stmt(
                condition,
                then_body,
                kept_elseifs,
                kept_else,
                span,
            )]
        }
    }
}

fn prune_else_if_chain(
    elseif_clauses: Vec<(Expr, Vec<Stmt>)>,
    else_body: Option<Vec<Stmt>>,
) -> Vec<Stmt> {
    let mut clauses = elseif_clauses.into_iter();
    while let Some((condition, body)) = clauses.next() {
        let condition = prune_expr(condition);
        match scalar_value(&condition) {
            Some(value) if value.truthy() => return prune_block(body),
            Some(_) => continue,
            None => {
                let span = condition.span;
                let remaining: Vec<_> = clauses.collect();
                let (kept_elseifs, kept_else) = prune_remaining_elseif_chain(remaining, else_body);
                return vec![build_if_stmt(
                    condition,
                    prune_block(body),
                    kept_elseifs,
                    kept_else,
                    span,
                )];
            }
        }
    }
    else_body.map(prune_block).unwrap_or_default()
}

fn prune_remaining_elseif_chain(
    elseif_clauses: Vec<(Expr, Vec<Stmt>)>,
    else_body: Option<Vec<Stmt>>,
) -> (Vec<(Expr, Vec<Stmt>)>, Option<Vec<Stmt>>) {
    let mut kept = Vec::new();
    for (condition, body) in elseif_clauses {
        let condition = prune_expr(condition);
        match scalar_value(&condition) {
            Some(value) if value.truthy() => return (kept, Some(prune_block(body))),
            Some(_) => {}
            None => kept.push((condition, prune_block(body))),
        }
    }
    (kept, normalize_optional_block(else_body.map(prune_block)))
}

fn prune_method(
    method: ClassMethod,
    class_name: &str,
    parent_name: Option<&str>,
) -> ClassMethod {
    let context = ClassEffectContext {
        class_name: class_name.to_string(),
        parent_name: parent_name.map(str::to_string),
    };
    ClassMethod {
        body: with_class_effect_context(Some(context), || prune_block(method.body)),
        ..method
    }
}

fn prune_method_without_context(method: ClassMethod) -> ClassMethod {
    ClassMethod {
        body: with_class_effect_context(None, || prune_block(method.body)),
        ..method
    }
}

fn callable_alias_from_expr(expr: &Expr) -> Option<Effect> {
    match &expr.kind {
        ExprKind::FirstClassCallable(target) => Some(callable_target_call_effect(target)),
        ExprKind::Variable(name) => ACTIVE_CALLABLE_ALIAS_EFFECTS.with(|slot| {
            slot.borrow()
                .as_ref()
                .and_then(|effects| effects.get(name).copied())
        }),
        _ => None,
    }
}

fn update_callable_alias(aliases: &mut HashMap<String, Effect>, name: &str, value: &Expr) {
    if let Some(effect) = callable_alias_from_expr(value) {
        aliases.insert(name.to_string(), effect);
    } else {
        aliases.remove(name);
    }
}

fn simulate_catch_callable_aliases(
    catch: &crate::parser::ast::CatchClause,
    mut aliases: HashMap<String, Effect>,
) -> HashMap<String, Effect> {
    if let Some(name) = &catch.variable {
        aliases.remove(name);
    }
    simulate_block_callable_aliases(&catch.body, aliases)
}

fn merge_try_callable_alias_paths(
    try_body: &[Stmt],
    catches: &[crate::parser::ast::CatchClause],
    finally_body: Option<&[Stmt]>,
    incoming_aliases: &HashMap<String, Effect>,
) -> HashMap<String, Effect> {
    let mut fallthrough_paths = Vec::new();

    if matches!(block_terminal_effect(try_body), TerminalEffect::FallsThrough) {
        fallthrough_paths.push(simulate_block_callable_aliases(try_body, incoming_aliases.clone()));
    }

    for catch in catches {
        if matches!(block_terminal_effect(&catch.body), TerminalEffect::FallsThrough) {
            fallthrough_paths.push(simulate_catch_callable_aliases(catch, incoming_aliases.clone()));
        }
    }

    if let Some(finally_body) = finally_body {
        fallthrough_paths = fallthrough_paths
            .into_iter()
            .map(|aliases| simulate_block_callable_aliases(finally_body, aliases))
            .collect();
    }

    merge_callable_alias_paths(fallthrough_paths)
}

enum SwitchAliasPathOutcome {
    FallsThrough(HashMap<String, Effect>),
    Breaks(HashMap<String, Effect>),
    ExitsCurrentBlock,
}

fn simulate_switch_body_callable_aliases(
    body: &[Stmt],
    mut aliases: HashMap<String, Effect>,
) -> SwitchAliasPathOutcome {
    for stmt in body {
        apply_stmt_callable_aliases(stmt, &mut aliases);
        match stmt_terminal_effect(stmt) {
            TerminalEffect::FallsThrough => {}
            TerminalEffect::Breaks => return SwitchAliasPathOutcome::Breaks(aliases),
            TerminalEffect::ExitsCurrentBlock | TerminalEffect::TerminatesMixed => {
                return SwitchAliasPathOutcome::ExitsCurrentBlock;
            }
        }
    }

    SwitchAliasPathOutcome::FallsThrough(aliases)
}

fn simulate_switch_entry_callable_aliases(
    cases: &[(Vec<Expr>, Vec<Stmt>)],
    default: Option<&[Stmt]>,
    entry_case: Option<usize>,
    incoming_aliases: &HashMap<String, Effect>,
) -> Option<HashMap<String, Effect>> {
    let mut aliases = incoming_aliases.clone();

    if let Some(start_index) = entry_case {
        for (_, body) in cases.iter().skip(start_index) {
            match simulate_switch_body_callable_aliases(body, aliases) {
                SwitchAliasPathOutcome::FallsThrough(updated) => aliases = updated,
                SwitchAliasPathOutcome::Breaks(updated) => return Some(updated),
                SwitchAliasPathOutcome::ExitsCurrentBlock => return None,
            }
        }
    }

    match default {
        Some(default_body) => match simulate_switch_body_callable_aliases(default_body, aliases) {
            SwitchAliasPathOutcome::FallsThrough(updated)
            | SwitchAliasPathOutcome::Breaks(updated) => Some(updated),
            SwitchAliasPathOutcome::ExitsCurrentBlock => None,
        },
        None => Some(aliases),
    }
}

fn merge_switch_callable_alias_paths(
    cases: &[(Vec<Expr>, Vec<Stmt>)],
    default: Option<&[Stmt]>,
    incoming_aliases: &HashMap<String, Effect>,
) -> HashMap<String, Effect> {
    let mut fallthrough_paths = Vec::new();

    for case_index in 0..cases.len() {
        if let Some(aliases) =
            simulate_switch_entry_callable_aliases(cases, default, Some(case_index), incoming_aliases)
        {
            fallthrough_paths.push(aliases);
        }
    }

    if let Some(aliases) = simulate_switch_entry_callable_aliases(cases, default, None, incoming_aliases)
    {
        fallthrough_paths.push(aliases);
    }

    merge_callable_alias_paths(fallthrough_paths)
}

fn apply_stmt_callable_aliases(stmt: &Stmt, aliases: &mut HashMap<String, Effect>) {
    match &stmt.kind {
        StmtKind::Assign { name, value } | StmtKind::TypedAssign { name, value, .. } => {
            let effect = with_callable_alias_effects(aliases.clone(), || callable_alias_from_expr(value));
            if let Some(effect) = effect {
                aliases.insert(name.clone(), effect);
            } else {
                aliases.remove(name);
            }
        }
        StmtKind::StaticVar { name, init } => update_callable_alias(aliases, name, init),
        StmtKind::Global { vars } => {
            for var in vars {
                aliases.remove(var);
            }
        }
        StmtKind::ArrayAssign { array, .. } | StmtKind::ArrayPush { array, .. } => {
            aliases.remove(array);
        }
        StmtKind::ListUnpack { vars, .. } => {
            for var in vars {
                aliases.remove(var);
            }
        }
        StmtKind::If {
            then_body,
            elseif_clauses,
            else_body,
            ..
        } => {
            let mut fallthrough_paths = Vec::new();
            if matches!(block_terminal_effect(then_body), TerminalEffect::FallsThrough) {
                fallthrough_paths.push(simulate_block_callable_aliases(then_body, aliases.clone()));
            }
            for (_, body) in elseif_clauses {
                if matches!(block_terminal_effect(body), TerminalEffect::FallsThrough) {
                    fallthrough_paths.push(simulate_block_callable_aliases(body, aliases.clone()));
                }
            }
            if let Some(body) = else_body {
                if matches!(block_terminal_effect(body), TerminalEffect::FallsThrough) {
                    fallthrough_paths.push(simulate_block_callable_aliases(body, aliases.clone()));
                }
            } else {
                fallthrough_paths.push(aliases.clone());
            }
            *aliases = merge_callable_alias_paths(fallthrough_paths);
        }
        StmtKind::IfDef {
            then_body, else_body, ..
        } => {
            let mut fallthrough_paths = Vec::new();
            if matches!(block_terminal_effect(then_body), TerminalEffect::FallsThrough) {
                fallthrough_paths.push(simulate_block_callable_aliases(then_body, aliases.clone()));
            }
            match else_body {
                Some(body) if matches!(block_terminal_effect(body), TerminalEffect::FallsThrough) => {
                    fallthrough_paths.push(simulate_block_callable_aliases(body, aliases.clone()));
                }
                None => fallthrough_paths.push(aliases.clone()),
                _ => {}
            }
            *aliases = merge_callable_alias_paths(fallthrough_paths);
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            *aliases = merge_try_callable_alias_paths(
                try_body,
                catches,
                finally_body.as_deref(),
                aliases,
            );
        }
        StmtKind::Switch { cases, default, .. } => {
            *aliases = merge_switch_callable_alias_paths(cases, default.as_deref(), aliases);
        }
        StmtKind::While { .. }
        | StmtKind::DoWhile { .. }
        | StmtKind::For { .. }
        | StmtKind::Foreach { .. }
        | StmtKind::Include { .. } => aliases.clear(),
        _ => {}
    }
}

fn simulate_block_callable_aliases(
    body: &[Stmt],
    mut aliases: HashMap<String, Effect>,
) -> HashMap<String, Effect> {
    for stmt in body {
        apply_stmt_callable_aliases(stmt, &mut aliases);
        if !matches!(stmt_terminal_effect(stmt), TerminalEffect::FallsThrough) {
            break;
        }
    }
    aliases
}

fn merge_callable_alias_paths(
    mut paths: Vec<HashMap<String, Effect>>,
) -> HashMap<String, Effect> {
    let Some(first) = paths.pop() else {
        return HashMap::new();
    };
    first
        .into_iter()
        .filter(|(name, effect)| {
            paths.iter()
                .all(|path| path.get(name).copied() == Some(*effect))
        })
        .collect()
}

fn prune_for_clause(stmt: Option<Box<Stmt>>) -> Option<Box<Stmt>> {
    let stmt = stmt?;
    prune_stmt(*stmt).into_iter().next().map(Box::new)
}

fn prune_switch_stmt(
    subject: Expr,
    cases: Vec<(Vec<Expr>, Vec<Stmt>)>,
    default: Option<Vec<Stmt>>,
    span: crate::span::Span,
) -> Vec<Stmt> {
    let subject = prune_expr(subject);
    let cases: Vec<(Vec<Expr>, Vec<Stmt>)> = cases
        .into_iter()
        .map(|(patterns, body)| (patterns.into_iter().map(prune_expr).collect(), prune_block(body)))
        .collect();
    let default = normalize_optional_block(default.map(prune_block));

    if cases.iter().all(|(_, body)| body.is_empty()) && default.is_none() {
        return expr_to_effect_stmt(subject);
    }

    if cases.is_empty() {
        let mut stmts = expr_to_effect_stmt(subject);
        if let Some(default_body) = default {
            stmts.extend(default_body);
        }
        return stmts;
    }

    let Some(subject_value) = scalar_value(&subject) else {
        return vec![Stmt {
            kind: StmtKind::Switch {
                subject,
                cases,
                default,
            },
            span,
        }];
    };

    for (index, (patterns, _)) in cases.iter().enumerate() {
        match classify_case_patterns(&subject_value, patterns, CaseComparison::LooseSwitch) {
            CaseMatch::Matches => {
                return materialize_switch_execution(&cases, &default, Some(index));
            }
            CaseMatch::Unknown => {
                return vec![Stmt {
                    kind: StmtKind::Switch {
                        subject,
                        cases: cases[index..].to_vec(),
                        default,
                    },
                    span,
                }];
            }
            CaseMatch::NoMatch => {}
        }
    }

    if default.is_some() {
        materialize_switch_execution(&cases, &default, None)
    } else {
        Vec::new()
    }
}

fn prune_expr(expr: Expr) -> Expr {
    let span = expr.span;
    let kind = match expr.kind {
        ExprKind::StringLiteral(value) => ExprKind::StringLiteral(value),
        ExprKind::IntLiteral(value) => ExprKind::IntLiteral(value),
        ExprKind::FloatLiteral(value) => ExprKind::FloatLiteral(value),
        ExprKind::Variable(name) => ExprKind::Variable(name),
        ExprKind::BinaryOp { left, op, right } => ExprKind::BinaryOp {
            left: Box::new(prune_expr(*left)),
            op,
            right: Box::new(prune_expr(*right)),
        },
        ExprKind::BoolLiteral(value) => ExprKind::BoolLiteral(value),
        ExprKind::Null => ExprKind::Null,
        ExprKind::Negate(inner) => ExprKind::Negate(Box::new(prune_expr(*inner))),
        ExprKind::Not(inner) => ExprKind::Not(Box::new(prune_expr(*inner))),
        ExprKind::BitNot(inner) => ExprKind::BitNot(Box::new(prune_expr(*inner))),
        ExprKind::Throw(inner) => ExprKind::Throw(Box::new(prune_expr(*inner))),
        ExprKind::NullCoalesce { value, default } => ExprKind::NullCoalesce {
            value: Box::new(prune_expr(*value)),
            default: Box::new(prune_expr(*default)),
        },
        ExprKind::PreIncrement(name) => ExprKind::PreIncrement(name),
        ExprKind::PostIncrement(name) => ExprKind::PostIncrement(name),
        ExprKind::PreDecrement(name) => ExprKind::PreDecrement(name),
        ExprKind::PostDecrement(name) => ExprKind::PostDecrement(name),
        ExprKind::FunctionCall { name, args } => ExprKind::FunctionCall {
            name,
            args: args.into_iter().map(prune_expr).collect(),
        },
        ExprKind::ArrayLiteral(items) => {
            ExprKind::ArrayLiteral(items.into_iter().map(prune_expr).collect())
        }
        ExprKind::ArrayLiteralAssoc(items) => ExprKind::ArrayLiteralAssoc(
            items.into_iter()
                .map(|(key, value)| (prune_expr(key), prune_expr(value)))
                .collect(),
        ),
        ExprKind::Match {
            subject,
            arms,
            default,
        } => {
            let subject = prune_expr(*subject);
            let arms: Vec<(Vec<Expr>, Expr)> = arms
                .into_iter()
                .map(|(patterns, value)| {
                    (
                        patterns.into_iter().map(prune_expr).collect(),
                        prune_expr(value),
                    )
                })
                .collect();
            let default = default.map(|expr| Box::new(prune_expr(*expr)));
            try_prune_match_expr(subject, arms, default)
        }
        ExprKind::ArrayAccess { array, index } => ExprKind::ArrayAccess {
            array: Box::new(prune_expr(*array)),
            index: Box::new(prune_expr(*index)),
        },
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => ExprKind::Ternary {
            condition: Box::new(prune_expr(*condition)),
            then_expr: Box::new(prune_expr(*then_expr)),
            else_expr: Box::new(prune_expr(*else_expr)),
        },
        ExprKind::Cast { target, expr } => ExprKind::Cast {
            target,
            expr: Box::new(prune_expr(*expr)),
        },
        ExprKind::Closure {
            params,
            variadic,
            body,
            is_arrow,
            captures,
        } => ExprKind::Closure {
            params,
            variadic,
            body: prune_block(body),
            is_arrow,
            captures,
        },
        ExprKind::NamedArg { name, value } => ExprKind::NamedArg {
            name,
            value: Box::new(prune_expr(*value)),
        },
        ExprKind::Spread(inner) => ExprKind::Spread(Box::new(prune_expr(*inner))),
        ExprKind::ClosureCall { var, args } => ExprKind::ClosureCall {
            var,
            args: args.into_iter().map(prune_expr).collect(),
        },
        ExprKind::ExprCall { callee, args } => ExprKind::ExprCall {
            callee: Box::new(prune_expr(*callee)),
            args: args.into_iter().map(prune_expr).collect(),
        },
        ExprKind::ConstRef(name) => ExprKind::ConstRef(name),
        ExprKind::EnumCase {
            enum_name,
            case_name,
        } => ExprKind::EnumCase {
            enum_name,
            case_name,
        },
        ExprKind::NewObject { class_name, args } => ExprKind::NewObject {
            class_name,
            args: args.into_iter().map(prune_expr).collect(),
        },
        ExprKind::PropertyAccess { object, property } => ExprKind::PropertyAccess {
            object: Box::new(prune_expr(*object)),
            property,
        },
        ExprKind::MethodCall {
            object,
            method,
            args,
        } => ExprKind::MethodCall {
            object: Box::new(prune_expr(*object)),
            method,
            args: args.into_iter().map(prune_expr).collect(),
        },
        ExprKind::StaticMethodCall {
            receiver,
            method,
            args,
        } => ExprKind::StaticMethodCall {
            receiver,
            method,
            args: args.into_iter().map(prune_expr).collect(),
        },
        ExprKind::FirstClassCallable(target) => {
            ExprKind::FirstClassCallable(prune_callable_target(target))
        }
        ExprKind::This => ExprKind::This,
        ExprKind::PtrCast { target_type, expr } => ExprKind::PtrCast {
            target_type,
            expr: Box::new(prune_expr(*expr)),
        },
        ExprKind::BufferNew { element_type, len } => ExprKind::BufferNew {
            element_type,
            len: Box::new(prune_expr(*len)),
        },
    };
    let kind = prune_unused_pure_subexpressions(kind);
    Expr { kind, span }
}

fn expr_has_side_effects(expr: &Expr) -> bool {
    expr_effect(expr).has_side_effects
}

fn callable_target_effect(target: &CallableTarget) -> Effect {
    match target {
        CallableTarget::Function(_) | CallableTarget::StaticMethod { .. } => Effect::PURE,
        CallableTarget::Method { object, .. } => expr_effect(object),
    }
}

fn prune_unused_pure_subexpressions(kind: ExprKind) -> ExprKind {
    match kind {
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => match scalar_value(&condition) {
            Some(value) if value.truthy() && !expr_has_side_effects(&else_expr) => then_expr.kind,
            Some(value) if !value.truthy() && !expr_has_side_effects(&then_expr) => else_expr.kind,
            _ => ExprKind::Ternary {
                condition,
                then_expr,
                else_expr,
            },
        },
        ExprKind::NullCoalesce { value, default } => match scalar_value(&value) {
            Some(ScalarValue::Null) => default.kind,
            Some(_) if !expr_has_side_effects(&default) => value.kind,
            _ => ExprKind::NullCoalesce { value, default },
        },
        ExprKind::BinaryOp { left, op, right } => match op {
            BinOp::And => match scalar_value(&left) {
                Some(value) if !value.truthy() && !expr_has_side_effects(&right) => {
                    ExprKind::BoolLiteral(false)
                }
                _ => ExprKind::BinaryOp { left, op, right },
            },
            BinOp::Or => match scalar_value(&left) {
                Some(value) if value.truthy() && !expr_has_side_effects(&right) => {
                    ExprKind::BoolLiteral(true)
                }
                _ => ExprKind::BinaryOp { left, op, right },
            },
            _ => ExprKind::BinaryOp { left, op, right },
        },
        other => other,
    }
}

fn prune_callable_target(target: CallableTarget) -> CallableTarget {
    match target {
        CallableTarget::Function(name) => CallableTarget::Function(name),
        CallableTarget::StaticMethod { receiver, method } => {
            CallableTarget::StaticMethod { receiver, method }
        }
        CallableTarget::Method { object, method } => CallableTarget::Method {
            object: Box::new(prune_expr(*object)),
            method,
        },
    }
}

fn try_prune_match_expr(
    subject: Expr,
    arms: Vec<(Vec<Expr>, Expr)>,
    default: Option<Box<Expr>>,
) -> ExprKind {
    let Some(subject_value) = scalar_value(&subject) else {
        return ExprKind::Match {
            subject: Box::new(subject),
            arms,
            default,
        };
    };

    for (index, (patterns, result)) in arms.iter().enumerate() {
        match classify_case_patterns(&subject_value, patterns, CaseComparison::Strict) {
            CaseMatch::Matches => return result.kind.clone(),
            CaseMatch::NoMatch => {}
            CaseMatch::Unknown => {
                return ExprKind::Match {
                    subject: Box::new(subject),
                    arms: arms[index..].to_vec(),
                    default,
                };
            }
        }
    }

    if let Some(default) = default {
        default.kind
    } else {
        ExprKind::Match {
            subject: Box::new(subject),
            arms: Vec::new(),
            default: None,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CaseMatch {
    Matches,
    NoMatch,
    Unknown,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CaseComparison {
    Strict,
    LooseSwitch,
}

fn classify_case_patterns(
    subject: &ScalarValue,
    patterns: &[Expr],
    comparison: CaseComparison,
) -> CaseMatch {
    let mut has_unknown = false;
    for pattern in patterns {
        match pattern_matches_scalar(subject, pattern, comparison) {
            Some(true) => return CaseMatch::Matches,
            Some(false) => {}
            None => has_unknown = true,
        }
    }
    if has_unknown {
        CaseMatch::Unknown
    } else {
        CaseMatch::NoMatch
    }
}

fn pattern_matches_scalar(
    subject: &ScalarValue,
    pattern: &Expr,
    comparison: CaseComparison,
) -> Option<bool> {
    let pattern = scalar_value(pattern)?;
    match comparison {
        CaseComparison::Strict => compare_scalar_strict(subject, &pattern),
        CaseComparison::LooseSwitch => compare_scalar_switch(subject, &pattern),
    }
}

fn compare_scalar_strict(left: &ScalarValue, right: &ScalarValue) -> Option<bool> {
    match (left, right) {
        (ScalarValue::Null, ScalarValue::Null) => Some(true),
        (ScalarValue::Bool(left), ScalarValue::Bool(right)) => Some(left == right),
        (ScalarValue::Int(left), ScalarValue::Int(right)) => Some(left == right),
        (ScalarValue::String(left), ScalarValue::String(right)) => Some(left == right),
        (ScalarValue::Float(left), ScalarValue::Float(right)) => Some(left == right),
        _ => Some(false),
    }
}

fn compare_scalar_switch(left: &ScalarValue, right: &ScalarValue) -> Option<bool> {
    match (left, right) {
        (ScalarValue::String(left), ScalarValue::String(right)) => Some(left == right),
        (ScalarValue::Float(left), ScalarValue::Float(right)) => Some(left == right),
        (ScalarValue::String(_), _) | (_, ScalarValue::String(_)) => None,
        (ScalarValue::Float(_), _) | (_, ScalarValue::Float(_)) => None,
        _ => Some(scalar_dispatch_int(left)? == scalar_dispatch_int(right)?),
    }
}

fn scalar_dispatch_int(value: &ScalarValue) -> Option<i64> {
    match value {
        ScalarValue::Null => Some(0),
        ScalarValue::Bool(value) => Some(i64::from(*value)),
        ScalarValue::Int(value) => Some(*value),
        ScalarValue::Float(_) | ScalarValue::String(_) => None,
    }
}

fn fold_params(
    params: Vec<(String, Option<crate::parser::ast::TypeExpr>, Option<Expr>, bool)>,
) -> Vec<(String, Option<crate::parser::ast::TypeExpr>, Option<Expr>, bool)> {
    params
        .into_iter()
        .map(|(name, type_expr, default, is_ref)| {
            (name, type_expr, default.map(fold_expr), is_ref)
        })
        .collect()
}

fn fold_property(property: ClassProperty) -> ClassProperty {
    ClassProperty {
        name: property.name,
        visibility: property.visibility,
        readonly: property.readonly,
        default: property.default.map(fold_expr),
        span: property.span,
    }
}

fn fold_method(method: ClassMethod) -> ClassMethod {
    ClassMethod {
        name: method.name,
        visibility: method.visibility,
        is_static: method.is_static,
        is_abstract: method.is_abstract,
        has_body: method.has_body,
        params: fold_params(method.params),
        variadic: method.variadic,
        return_type: method.return_type,
        body: fold_block(method.body),
        span: method.span,
    }
}

fn fold_enum_case(case: EnumCaseDecl) -> EnumCaseDecl {
    EnumCaseDecl {
        name: case.name,
        value: case.value.map(fold_expr),
        span: case.span,
    }
}

fn fold_expr(expr: Expr) -> Expr {
    let span = expr.span;
    let kind = match expr.kind {
        ExprKind::StringLiteral(value) => ExprKind::StringLiteral(value),
        ExprKind::IntLiteral(value) => ExprKind::IntLiteral(value),
        ExprKind::FloatLiteral(value) => ExprKind::FloatLiteral(value),
        ExprKind::Variable(name) => ExprKind::Variable(name),
        ExprKind::BinaryOp { left, op, right } => {
            let left = fold_expr(*left);
            let right = fold_expr(*right);
            try_fold_binary_op(&op, &left, &right).unwrap_or_else(|| ExprKind::BinaryOp {
                left: Box::new(left),
                op,
                right: Box::new(right),
            })
        }
        ExprKind::BoolLiteral(value) => ExprKind::BoolLiteral(value),
        ExprKind::Null => ExprKind::Null,
        ExprKind::Negate(inner) => {
            let inner = fold_expr(*inner);
            try_fold_negate(&inner).unwrap_or_else(|| ExprKind::Negate(Box::new(inner)))
        }
        ExprKind::Not(inner) => {
            let inner = fold_expr(*inner);
            try_fold_not(&inner).unwrap_or_else(|| ExprKind::Not(Box::new(inner)))
        }
        ExprKind::BitNot(inner) => {
            let inner = fold_expr(*inner);
            try_fold_bit_not(&inner).unwrap_or_else(|| ExprKind::BitNot(Box::new(inner)))
        }
        ExprKind::Throw(inner) => ExprKind::Throw(Box::new(fold_expr(*inner))),
        ExprKind::NullCoalesce { value, default } => {
            let value = fold_expr(*value);
            let default = fold_expr(*default);
            try_fold_null_coalesce(&value, &default).unwrap_or_else(|| ExprKind::NullCoalesce {
                value: Box::new(value),
                default: Box::new(default),
            })
        }
        ExprKind::PreIncrement(name) => ExprKind::PreIncrement(name),
        ExprKind::PostIncrement(name) => ExprKind::PostIncrement(name),
        ExprKind::PreDecrement(name) => ExprKind::PreDecrement(name),
        ExprKind::PostDecrement(name) => ExprKind::PostDecrement(name),
        ExprKind::FunctionCall { name, args } => ExprKind::FunctionCall {
            name,
            args: args.into_iter().map(fold_expr).collect(),
        },
        ExprKind::ArrayLiteral(items) => {
            ExprKind::ArrayLiteral(items.into_iter().map(fold_expr).collect())
        }
        ExprKind::ArrayLiteralAssoc(items) => ExprKind::ArrayLiteralAssoc(
            items.into_iter()
                .map(|(key, value)| (fold_expr(key), fold_expr(value)))
                .collect(),
        ),
        ExprKind::Match {
            subject,
            arms,
            default,
        } => ExprKind::Match {
            subject: Box::new(fold_expr(*subject)),
            arms: arms
                .into_iter()
                .map(|(patterns, value)| {
                    (
                        patterns.into_iter().map(fold_expr).collect(),
                        fold_expr(value),
                    )
                })
                .collect(),
            default: default.map(|expr| Box::new(fold_expr(*expr))),
        },
        ExprKind::ArrayAccess { array, index } => ExprKind::ArrayAccess {
            array: Box::new(fold_expr(*array)),
            index: Box::new(fold_expr(*index)),
        },
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            let condition = fold_expr(*condition);
            let then_expr = fold_expr(*then_expr);
            let else_expr = fold_expr(*else_expr);
            try_fold_ternary(&condition, &then_expr, &else_expr).unwrap_or_else(|| {
                ExprKind::Ternary {
                    condition: Box::new(condition),
                    then_expr: Box::new(then_expr),
                    else_expr: Box::new(else_expr),
                }
            })
        }
        ExprKind::Cast { target, expr } => {
            let expr = fold_expr(*expr);
            try_fold_cast(&target, &expr).unwrap_or_else(|| ExprKind::Cast {
                target,
                expr: Box::new(expr),
            })
        }
        ExprKind::Closure {
            params,
            variadic,
            body,
            is_arrow,
            captures,
        } => ExprKind::Closure {
            params: fold_params(params),
            variadic,
            body: fold_block(body),
            is_arrow,
            captures,
        },
        ExprKind::NamedArg { name, value } => ExprKind::NamedArg {
            name,
            value: Box::new(fold_expr(*value)),
        },
        ExprKind::Spread(inner) => ExprKind::Spread(Box::new(fold_expr(*inner))),
        ExprKind::ClosureCall { var, args } => ExprKind::ClosureCall {
            var,
            args: args.into_iter().map(fold_expr).collect(),
        },
        ExprKind::ExprCall { callee, args } => ExprKind::ExprCall {
            callee: Box::new(fold_expr(*callee)),
            args: args.into_iter().map(fold_expr).collect(),
        },
        ExprKind::ConstRef(name) => ExprKind::ConstRef(name),
        ExprKind::EnumCase {
            enum_name,
            case_name,
        } => ExprKind::EnumCase {
            enum_name,
            case_name,
        },
        ExprKind::NewObject { class_name, args } => ExprKind::NewObject {
            class_name,
            args: args.into_iter().map(fold_expr).collect(),
        },
        ExprKind::PropertyAccess { object, property } => ExprKind::PropertyAccess {
            object: Box::new(fold_expr(*object)),
            property,
        },
        ExprKind::MethodCall {
            object,
            method,
            args,
        } => ExprKind::MethodCall {
            object: Box::new(fold_expr(*object)),
            method,
            args: args.into_iter().map(fold_expr).collect(),
        },
        ExprKind::StaticMethodCall {
            receiver,
            method,
            args,
        } => ExprKind::StaticMethodCall {
            receiver,
            method,
            args: args.into_iter().map(fold_expr).collect(),
        },
        ExprKind::FirstClassCallable(target) => {
            ExprKind::FirstClassCallable(fold_callable_target(target))
        }
        ExprKind::This => ExprKind::This,
        ExprKind::PtrCast { target_type, expr } => ExprKind::PtrCast {
            target_type,
            expr: Box::new(fold_expr(*expr)),
        },
        ExprKind::BufferNew { element_type, len } => ExprKind::BufferNew {
            element_type,
            len: Box::new(fold_expr(*len)),
        },
    };
    Expr { kind, span }
}

fn fold_callable_target(target: CallableTarget) -> CallableTarget {
    match target {
        CallableTarget::Function(name) => CallableTarget::Function(name),
        CallableTarget::StaticMethod { receiver, method } => {
            CallableTarget::StaticMethod { receiver, method }
        }
        CallableTarget::Method { object, method } => CallableTarget::Method {
            object: Box::new(fold_expr(*object)),
            method,
        },
    }
}

fn try_fold_negate(expr: &Expr) -> Option<ExprKind> {
    match &expr.kind {
        ExprKind::IntLiteral(value) => value.checked_neg().map(ExprKind::IntLiteral),
        ExprKind::FloatLiteral(value) => Some(ExprKind::FloatLiteral(-value)),
        _ => None,
    }
}

fn try_fold_not(expr: &Expr) -> Option<ExprKind> {
    Some(ExprKind::BoolLiteral(!scalar_value(expr)?.truthy()))
}

fn try_fold_bit_not(expr: &Expr) -> Option<ExprKind> {
    match &expr.kind {
        ExprKind::IntLiteral(value) => Some(ExprKind::IntLiteral(!value)),
        _ => None,
    }
}

fn try_fold_binary_op(op: &BinOp, left: &Expr, right: &Expr) -> Option<ExprKind> {
    match op {
        BinOp::Concat => try_fold_concat(left, right),
        BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Pow => {
            try_fold_numeric_binop(op, left, right)
        }
        BinOp::Mod => try_fold_int_mod(left, right),
        BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor | BinOp::ShiftLeft | BinOp::ShiftRight => {
            try_fold_bitwise_binop(op, left, right)
        }
        BinOp::And | BinOp::Or => try_fold_logical_binop(op, left, right),
        BinOp::Eq
        | BinOp::NotEq
        | BinOp::StrictEq
        | BinOp::StrictNotEq
        | BinOp::Lt
        | BinOp::Gt
        | BinOp::LtEq
        | BinOp::GtEq
        | BinOp::Spaceship => try_fold_compare_binop(op, left, right),
        _ => None,
    }
}

fn try_fold_concat(left: &Expr, right: &Expr) -> Option<ExprKind> {
    let ExprKind::StringLiteral(left) = &left.kind else {
        return None;
    };
    let ExprKind::StringLiteral(right) = &right.kind else {
        return None;
    };
    Some(ExprKind::StringLiteral(format!("{left}{right}")))
}

fn try_fold_numeric_binop(op: &BinOp, left: &Expr, right: &Expr) -> Option<ExprKind> {
    if let (Some(left), Some(right)) = (int_literal(left), int_literal(right)) {
        return try_fold_int_numeric_binop(op, left, right);
    }

    let (left, right) = (numeric_literal(left)?, numeric_literal(right)?);
    if matches!(op, BinOp::Div) && right == 0.0 {
        return None;
    }
    let result = match op {
        BinOp::Add => left + right,
        BinOp::Sub => left - right,
        BinOp::Mul => left * right,
        BinOp::Div => left / right,
        BinOp::Pow => left.powf(right),
        _ => return None,
    };
    if result.is_finite() {
        Some(ExprKind::FloatLiteral(result))
    } else {
        None
    }
}

fn try_fold_int_numeric_binop(op: &BinOp, left: i64, right: i64) -> Option<ExprKind> {
    match op {
        BinOp::Add => left.checked_add(right).map(ExprKind::IntLiteral),
        BinOp::Sub => left.checked_sub(right).map(ExprKind::IntLiteral),
        BinOp::Mul => left.checked_mul(right).map(ExprKind::IntLiteral),
        BinOp::Div => {
            if right == 0 {
                None
            } else {
                Some(ExprKind::FloatLiteral(left as f64 / right as f64))
            }
        }
        BinOp::Pow => {
            let result = (left as f64).powf(right as f64);
            if result.is_finite() {
                Some(ExprKind::FloatLiteral(result))
            } else {
                None
            }
        }
        _ => None,
    }
}

fn try_fold_int_mod(left: &Expr, right: &Expr) -> Option<ExprKind> {
    let (left, right) = (int_literal(left)?, int_literal(right)?);
    if right == 0 {
        None
    } else {
        Some(ExprKind::IntLiteral(left % right))
    }
}

fn try_fold_bitwise_binop(op: &BinOp, left: &Expr, right: &Expr) -> Option<ExprKind> {
    let (left, right) = (int_literal(left)?, int_literal(right)?);
    match op {
        BinOp::BitAnd => Some(ExprKind::IntLiteral(left & right)),
        BinOp::BitOr => Some(ExprKind::IntLiteral(left | right)),
        BinOp::BitXor => Some(ExprKind::IntLiteral(left ^ right)),
        BinOp::ShiftLeft => {
            let shift = u32::try_from(right).ok()?;
            left.checked_shl(shift).map(ExprKind::IntLiteral)
        }
        BinOp::ShiftRight => {
            let shift = u32::try_from(right).ok()?;
            left.checked_shr(shift).map(ExprKind::IntLiteral)
        }
        _ => None,
    }
}

fn try_fold_logical_binop(op: &BinOp, left: &Expr, right: &Expr) -> Option<ExprKind> {
    let left = scalar_value(left)?;
    let right = scalar_value(right)?;
    let result = match op {
        BinOp::And => left.truthy() && right.truthy(),
        BinOp::Or => left.truthy() || right.truthy(),
        _ => return None,
    };
    Some(ExprKind::BoolLiteral(result))
}

fn try_fold_compare_binop(op: &BinOp, left: &Expr, right: &Expr) -> Option<ExprKind> {
    match op {
        BinOp::Eq => Some(ExprKind::BoolLiteral(loose_eq(left, right)?)),
        BinOp::NotEq => Some(ExprKind::BoolLiteral(!loose_eq(left, right)?)),
        BinOp::StrictEq => Some(ExprKind::BoolLiteral(strict_eq(left, right)?)),
        BinOp::StrictNotEq => Some(ExprKind::BoolLiteral(!strict_eq(left, right)?)),
        BinOp::Lt => Some(ExprKind::BoolLiteral(compare_numeric(left, right, |l, r| l < r)?)),
        BinOp::Gt => Some(ExprKind::BoolLiteral(compare_numeric(left, right, |l, r| l > r)?)),
        BinOp::LtEq => Some(ExprKind::BoolLiteral(compare_numeric(left, right, |l, r| l <= r)?)),
        BinOp::GtEq => Some(ExprKind::BoolLiteral(compare_numeric(left, right, |l, r| l >= r)?)),
        BinOp::Spaceship => Some(ExprKind::IntLiteral(spaceship_numeric(left, right)?)),
        _ => None,
    }
}

fn try_fold_null_coalesce(value: &Expr, default: &Expr) -> Option<ExprKind> {
    let value = scalar_value(value)?;
    let default = scalar_value(default)?;
    if matches!(value, ScalarValue::Null) {
        Some(default.into_expr_kind())
    } else {
        Some(value.into_expr_kind())
    }
}

fn try_fold_ternary(condition: &Expr, then_expr: &Expr, else_expr: &Expr) -> Option<ExprKind> {
    let condition = scalar_value(condition)?;
    let then_expr = scalar_value(then_expr)?;
    let else_expr = scalar_value(else_expr)?;
    if condition.truthy() {
        Some(then_expr.into_expr_kind())
    } else {
        Some(else_expr.into_expr_kind())
    }
}

fn try_fold_cast(target: &CastType, expr: &Expr) -> Option<ExprKind> {
    let value = scalar_value(expr)?;
    match target {
        CastType::Int => try_fold_cast_int(value),
        CastType::Float => try_fold_cast_float(value),
        CastType::String => try_fold_cast_string(value),
        CastType::Bool => Some(ExprKind::BoolLiteral(value.truthy())),
        CastType::Array => None,
    }
}

fn try_fold_cast_int(value: ScalarValue) -> Option<ExprKind> {
    match value {
        ScalarValue::Null => Some(ExprKind::IntLiteral(0)),
        ScalarValue::Bool(value) => Some(ExprKind::IntLiteral(i64::from(value))),
        ScalarValue::Int(value) => Some(ExprKind::IntLiteral(value)),
        ScalarValue::Float(value) => truncate_float_to_i64(value).map(ExprKind::IntLiteral),
        ScalarValue::String(value) => parse_string_cast_int(&value).map(ExprKind::IntLiteral),
    }
}

fn try_fold_cast_float(value: ScalarValue) -> Option<ExprKind> {
    match value {
        ScalarValue::Null => Some(ExprKind::FloatLiteral(0.0)),
        ScalarValue::Bool(value) => Some(ExprKind::FloatLiteral(if value { 1.0 } else { 0.0 })),
        ScalarValue::Int(value) => Some(ExprKind::FloatLiteral(value as f64)),
        ScalarValue::Float(value) => Some(ExprKind::FloatLiteral(value)),
        ScalarValue::String(value) => parse_string_cast_float(&value).map(ExprKind::FloatLiteral),
    }
}

fn try_fold_cast_string(value: ScalarValue) -> Option<ExprKind> {
    match value {
        ScalarValue::Null => Some(ExprKind::StringLiteral(String::new())),
        ScalarValue::Bool(value) => Some(ExprKind::StringLiteral(if value {
            "1".to_string()
        } else {
            String::new()
        })),
        ScalarValue::Int(value) => Some(ExprKind::StringLiteral(value.to_string())),
        ScalarValue::Float(_value) => None,
        ScalarValue::String(value) => Some(ExprKind::StringLiteral(value)),
    }
}

fn int_literal(expr: &Expr) -> Option<i64> {
    match expr.kind {
        ExprKind::IntLiteral(value) => Some(value),
        _ => None,
    }
}

fn numeric_literal(expr: &Expr) -> Option<f64> {
    match expr.kind {
        ExprKind::IntLiteral(value) => Some(value as f64),
        ExprKind::FloatLiteral(value) => Some(value),
        _ => None,
    }
}

fn scalar_value(expr: &Expr) -> Option<ScalarValue> {
    match &expr.kind {
        ExprKind::Null => Some(ScalarValue::Null),
        ExprKind::BoolLiteral(value) => Some(ScalarValue::Bool(*value)),
        ExprKind::IntLiteral(value) => Some(ScalarValue::Int(*value)),
        ExprKind::FloatLiteral(value) => Some(ScalarValue::Float(*value)),
        ExprKind::StringLiteral(value) => Some(ScalarValue::String(value.clone())),
        _ => None,
    }
}

fn strict_eq(left: &Expr, right: &Expr) -> Option<bool> {
    let left = scalar_value(left)?;
    let right = scalar_value(right)?;
    Some(left == right)
}

fn loose_eq(left: &Expr, right: &Expr) -> Option<bool> {
    let left = scalar_value(left)?;
    let right = scalar_value(right)?;
    match (&left, &right) {
        (ScalarValue::Null, ScalarValue::Null) => Some(true),
        (ScalarValue::Bool(left), ScalarValue::Bool(right)) => Some(left == right),
        (ScalarValue::String(left), ScalarValue::String(right)) => Some(left == right),
        (ScalarValue::Int(left), ScalarValue::Int(right)) => Some(left == right),
        (ScalarValue::Float(left), ScalarValue::Float(right)) => Some(left == right),
        (ScalarValue::Int(left), ScalarValue::Float(right)) => Some(*left as f64 == *right),
        (ScalarValue::Float(left), ScalarValue::Int(right)) => Some(*left == *right as f64),
        _ => None,
    }
}

fn compare_numeric(left: &Expr, right: &Expr, cmp: impl FnOnce(f64, f64) -> bool) -> Option<bool> {
    let left = numeric_literal(left)?;
    let right = numeric_literal(right)?;
    Some(cmp(left, right))
}

fn spaceship_numeric(left: &Expr, right: &Expr) -> Option<i64> {
    let left = numeric_literal(left)?;
    let right = numeric_literal(right)?;
    Some(if left < right {
        -1
    } else if left > right {
        1
    } else {
        0
    })
}

#[derive(Debug, Clone, PartialEq)]
enum ScalarValue {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
}

impl ScalarValue {
    fn truthy(&self) -> bool {
        match self {
            ScalarValue::Null => false,
            ScalarValue::Bool(value) => *value,
            ScalarValue::Int(value) => *value != 0,
            ScalarValue::Float(value) => *value != 0.0,
            ScalarValue::String(value) => !value.is_empty() && value != "0",
        }
    }

    fn into_expr_kind(self) -> ExprKind {
        match self {
            ScalarValue::Null => ExprKind::Null,
            ScalarValue::Bool(value) => ExprKind::BoolLiteral(value),
            ScalarValue::Int(value) => ExprKind::IntLiteral(value),
            ScalarValue::Float(value) => ExprKind::FloatLiteral(value),
            ScalarValue::String(value) => ExprKind::StringLiteral(value),
        }
    }
}

fn truncate_float_to_i64(value: f64) -> Option<i64> {
    if !value.is_finite() {
        return None;
    }
    let truncated = value.trunc();
    if truncated < i64::MIN as f64 || truncated > i64::MAX as f64 {
        return None;
    }
    Some(truncated as i64)
}

fn parse_string_cast_int(value: &str) -> Option<i64> {
    if let Ok(parsed) = value.parse::<i64>() {
        return Some(parsed);
    }
    if let Ok(parsed) = value.parse::<f64>() {
        return truncate_float_to_i64(parsed);
    }
    if value.chars().all(|ch| ch.is_ascii_alphabetic()) {
        return Some(0);
    }
    None
}

fn parse_string_cast_float(value: &str) -> Option<f64> {
    if let Ok(parsed) = value.parse::<f64>() {
        return Some(parsed);
    }
    if value.chars().all(|ch| ch.is_ascii_alphabetic()) {
        return Some(0.0);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::names::Name;
    use crate::parser::ast::{ClassProperty, StaticReceiver, Visibility};
    use crate::span::Span;

    #[test]
    fn test_effect_analysis_recognizes_pure_builtin_calls() {
        let expr = Expr::new(
            ExprKind::FunctionCall {
                name: Name::from("strlen"),
                args: vec![Expr::string_lit("abc")],
            },
            Span::dummy(),
        );

        assert!(!expr_has_side_effects(&expr));
        assert!(!expr_effect(&expr).may_throw);
        assert!(!expr_is_observable(&expr));
    }

    #[test]
    fn test_effect_analysis_treats_property_and_array_reads_as_pure() {
        let property = Expr::new(
            ExprKind::PropertyAccess {
                object: Box::new(Expr::var("entry")),
                property: "name".to_string(),
            },
            Span::dummy(),
        );
        let array = Expr::new(
            ExprKind::ArrayAccess {
                array: Box::new(Expr::var("items")),
                index: Box::new(Expr::int_lit(0)),
            },
            Span::dummy(),
        );

        assert!(!expr_has_side_effects(&property));
        assert!(!expr_effect(&property).may_throw);
        assert!(!expr_has_side_effects(&array));
        assert!(!expr_effect(&array).may_throw);
    }

    #[test]
    fn test_program_function_effects_recognize_pure_user_functions() {
        let program = vec![Stmt::new(
            StmtKind::FunctionDecl {
                name: "len3".to_string(),
                params: Vec::new(),
                variadic: None,
                return_type: None,
                body: vec![Stmt::new(
                    StmtKind::Return(Some(Expr::new(
                        ExprKind::FunctionCall {
                            name: Name::from("strlen"),
                            args: vec![Expr::string_lit("abc")],
                        },
                        Span::dummy(),
                    ))),
                    Span::dummy(),
                )],
            },
            Span::dummy(),
        )];

        let (function_effects, _) = compute_program_callable_effects(&program);

        assert_eq!(function_effects.get("len3"), Some(&Effect::PURE));
    }

    #[test]
    fn test_program_function_effects_propagate_throwing_calls() {
        let program = vec![
            Stmt::new(
                StmtKind::FunctionDecl {
                    name: "boom".to_string(),
                    params: Vec::new(),
                    variadic: None,
                    return_type: None,
                    body: vec![Stmt::new(
                        StmtKind::Throw(Expr::new(
                            ExprKind::NewObject {
                                class_name: Name::from("Exception"),
                                args: Vec::new(),
                            },
                            Span::dummy(),
                        )),
                        Span::dummy(),
                    )],
                },
                Span::dummy(),
            ),
            Stmt::new(
                StmtKind::FunctionDecl {
                    name: "wrapper".to_string(),
                    params: Vec::new(),
                    variadic: None,
                    return_type: None,
                    body: vec![Stmt::new(
                        StmtKind::Return(Some(Expr::new(
                            ExprKind::FunctionCall {
                                name: Name::from("boom"),
                                args: Vec::new(),
                            },
                            Span::dummy(),
                        ))),
                        Span::dummy(),
                    )],
                },
                Span::dummy(),
            ),
        ];

        let (function_effects, _) = compute_program_callable_effects(&program);

        assert_eq!(
            function_effects.get("wrapper"),
            Some(&Effect::PURE.with_side_effects().with_may_throw())
        );
    }

    #[test]
    fn test_program_static_method_effects_recognize_pure_static_methods() {
        let program = vec![Stmt::new(
            StmtKind::ClassDecl {
                name: "Util".to_string(),
                extends: None,
                implements: Vec::new(),
                is_abstract: false,
                is_readonly_class: false,
                trait_uses: Vec::new(),
                properties: Vec::new(),
                methods: vec![ClassMethod {
                    name: "len3".to_string(),
                    visibility: Visibility::Public,
                    is_static: true,
                    is_abstract: false,
                    has_body: true,
                    params: Vec::new(),
                    variadic: None,
                    return_type: None,
                    body: vec![Stmt::new(
                        StmtKind::Return(Some(Expr::new(
                            ExprKind::FunctionCall {
                                name: Name::from("strlen"),
                                args: vec![Expr::string_lit("abc")],
                            },
                            Span::dummy(),
                        ))),
                        Span::dummy(),
                    )],
                    span: Span::dummy(),
                }],
            },
            Span::dummy(),
        )];

        let (_, static_method_effects) = compute_program_callable_effects(&program);

        assert_eq!(
            static_method_effects.get("Util::len3"),
            Some(&Effect::PURE)
        );
    }

    #[test]
    fn test_program_static_method_effects_resolve_self_receiver() {
        let program = vec![Stmt::new(
            StmtKind::ClassDecl {
                name: "Util".to_string(),
                extends: None,
                implements: Vec::new(),
                is_abstract: false,
                is_readonly_class: false,
                trait_uses: Vec::new(),
                properties: Vec::new(),
                methods: vec![
                    ClassMethod {
                        name: "len3".to_string(),
                        visibility: Visibility::Public,
                        is_static: true,
                        is_abstract: false,
                        has_body: true,
                        params: Vec::new(),
                        variadic: None,
                        return_type: None,
                        body: vec![Stmt::new(
                            StmtKind::Return(Some(Expr::new(
                                ExprKind::FunctionCall {
                                    name: Name::from("strlen"),
                                    args: vec![Expr::string_lit("abc")],
                                },
                                Span::dummy(),
                            ))),
                            Span::dummy(),
                        )],
                        span: Span::dummy(),
                    },
                    ClassMethod {
                        name: "relay".to_string(),
                        visibility: Visibility::Public,
                        is_static: true,
                        is_abstract: false,
                        has_body: true,
                        params: Vec::new(),
                        variadic: None,
                        return_type: None,
                        body: vec![Stmt::new(
                            StmtKind::Return(Some(Expr::new(
                                ExprKind::StaticMethodCall {
                                    receiver: StaticReceiver::Self_,
                                    method: "len3".to_string(),
                                    args: Vec::new(),
                                },
                                Span::dummy(),
                            ))),
                            Span::dummy(),
                        )],
                        span: Span::dummy(),
                    },
                ],
            },
            Span::dummy(),
        )];

        let (_, static_method_effects) = compute_program_callable_effects(&program);

        assert_eq!(
            static_method_effects.get("Util::relay"),
            Some(&Effect::PURE)
        );
    }

    #[test]
    fn test_program_static_method_effects_resolve_parent_receiver() {
        let program = vec![
            Stmt::new(
                StmtKind::ClassDecl {
                    name: "Base".to_string(),
                    extends: None,
                    implements: Vec::new(),
                    is_abstract: false,
                    is_readonly_class: false,
                    trait_uses: Vec::new(),
                    properties: Vec::new(),
                    methods: vec![ClassMethod {
                        name: "len3".to_string(),
                        visibility: Visibility::Public,
                        is_static: true,
                        is_abstract: false,
                        has_body: true,
                        params: Vec::new(),
                        variadic: None,
                        return_type: None,
                        body: vec![Stmt::new(
                            StmtKind::Return(Some(Expr::new(
                                ExprKind::FunctionCall {
                                    name: Name::from("strlen"),
                                    args: vec![Expr::string_lit("abc")],
                                },
                                Span::dummy(),
                            ))),
                            Span::dummy(),
                        )],
                        span: Span::dummy(),
                    }],
                },
                Span::dummy(),
            ),
            Stmt::new(
                StmtKind::ClassDecl {
                    name: "Child".to_string(),
                    extends: Some(Name::from("Base")),
                    implements: Vec::new(),
                    is_abstract: false,
                    is_readonly_class: false,
                    trait_uses: Vec::new(),
                    properties: Vec::new(),
                    methods: vec![ClassMethod {
                        name: "relay".to_string(),
                        visibility: Visibility::Public,
                        is_static: true,
                        is_abstract: false,
                        has_body: true,
                        params: Vec::new(),
                        variadic: None,
                        return_type: None,
                        body: vec![Stmt::new(
                            StmtKind::Return(Some(Expr::new(
                                ExprKind::StaticMethodCall {
                                    receiver: StaticReceiver::Parent,
                                    method: "len3".to_string(),
                                    args: Vec::new(),
                                },
                                Span::dummy(),
                            ))),
                            Span::dummy(),
                        )],
                        span: Span::dummy(),
                    }],
                },
                Span::dummy(),
            ),
        ];

        let (_, static_method_effects) = compute_program_callable_effects(&program);

        assert_eq!(
            static_method_effects.get("Child::relay"),
            Some(&Effect::PURE)
        );
    }

    #[test]
    fn test_effect_analysis_tracks_named_first_class_callable_expr_calls() {
        let expr = Expr::new(
            ExprKind::ExprCall {
                callee: Box::new(Expr::new(
                    ExprKind::FirstClassCallable(CallableTarget::Function(Name::from("strlen"))),
                    Span::dummy(),
                )),
                args: vec![Expr::string_lit("abc")],
            },
            Span::dummy(),
        );

        assert!(!expr_has_side_effects(&expr));
        assert!(!expr_effect(&expr).may_throw);
        assert!(!expr_is_observable(&expr));
    }

    #[test]
    fn test_program_function_effects_track_callable_alias_locals() {
        let program = vec![Stmt::new(
            StmtKind::FunctionDecl {
                name: "relay".to_string(),
                params: Vec::new(),
                variadic: None,
                return_type: None,
                body: vec![
                    Stmt::new(
                        StmtKind::Assign {
                            name: "f".to_string(),
                            value: Expr::new(
                                ExprKind::FirstClassCallable(CallableTarget::Function(Name::from(
                                    "strlen",
                                ))),
                                Span::dummy(),
                            ),
                        },
                        Span::dummy(),
                    ),
                    Stmt::new(
                        StmtKind::Assign {
                            name: "g".to_string(),
                            value: Expr::var("f"),
                        },
                        Span::dummy(),
                    ),
                    Stmt::new(
                        StmtKind::Return(Some(Expr::new(
                            ExprKind::ClosureCall {
                                var: "g".to_string(),
                                args: vec![Expr::string_lit("abc")],
                            },
                            Span::dummy(),
                        ))),
                        Span::dummy(),
                    ),
                ],
            },
            Span::dummy(),
        )];

        let (function_effects, _) = compute_program_callable_effects(&program);

        assert_eq!(
            function_effects.get("relay"),
            Some(&Effect::PURE.with_side_effects())
        );
    }

    #[test]
    fn test_program_function_effects_merge_callable_aliases_across_if_paths() {
        let program = vec![Stmt::new(
            StmtKind::FunctionDecl {
                name: "relay".to_string(),
                params: vec![("flag".to_string(), None, None, false)],
                variadic: None,
                return_type: None,
                body: vec![
                    Stmt::new(
                        StmtKind::If {
                            condition: Expr::var("flag"),
                            then_body: vec![Stmt::new(
                                StmtKind::Assign {
                                    name: "g".to_string(),
                                    value: Expr::new(
                                        ExprKind::FirstClassCallable(CallableTarget::Function(
                                            Name::from("strlen"),
                                        )),
                                        Span::dummy(),
                                    ),
                                },
                                Span::dummy(),
                            )],
                            elseif_clauses: Vec::new(),
                            else_body: Some(vec![Stmt::new(
                                StmtKind::Assign {
                                    name: "g".to_string(),
                                    value: Expr::new(
                                        ExprKind::FirstClassCallable(CallableTarget::Function(
                                            Name::from("strlen"),
                                        )),
                                        Span::dummy(),
                                    ),
                                },
                                Span::dummy(),
                            )]),
                        },
                        Span::dummy(),
                    ),
                    Stmt::new(
                        StmtKind::Return(Some(Expr::new(
                            ExprKind::ClosureCall {
                                var: "g".to_string(),
                                args: vec![Expr::string_lit("abc")],
                            },
                            Span::dummy(),
                        ))),
                        Span::dummy(),
                    ),
                ],
            },
            Span::dummy(),
        )];

        let (function_effects, _) = compute_program_callable_effects(&program);

        assert_eq!(
            function_effects.get("relay"),
            Some(&Effect::PURE.with_side_effects())
        );
    }

    #[test]
    fn test_program_function_effects_merge_callable_aliases_across_try_paths() {
        let program = vec![Stmt::new(
            StmtKind::FunctionDecl {
                name: "relay".to_string(),
                params: Vec::new(),
                variadic: None,
                return_type: None,
                body: vec![
                    Stmt::new(
                        StmtKind::Try {
                            try_body: vec![Stmt::new(
                                StmtKind::Assign {
                                    name: "g".to_string(),
                                    value: Expr::new(
                                        ExprKind::FirstClassCallable(CallableTarget::Function(
                                            Name::from("strlen"),
                                        )),
                                        Span::dummy(),
                                    ),
                                },
                                Span::dummy(),
                            )],
                            catches: vec![crate::parser::ast::CatchClause {
                                exception_types: vec![Name::from("Exception")],
                                variable: Some("e".to_string()),
                                body: vec![Stmt::new(
                                    StmtKind::Assign {
                                        name: "g".to_string(),
                                        value: Expr::new(
                                            ExprKind::FirstClassCallable(CallableTarget::Function(
                                                Name::from("strlen"),
                                            )),
                                            Span::dummy(),
                                        ),
                                    },
                                    Span::dummy(),
                                )],
                            }],
                            finally_body: Some(vec![Stmt::new(
                                StmtKind::ExprStmt(Expr::string_lit("done")),
                                Span::dummy(),
                            )]),
                        },
                        Span::dummy(),
                    ),
                    Stmt::new(
                        StmtKind::Return(Some(Expr::new(
                            ExprKind::ClosureCall {
                                var: "g".to_string(),
                                args: vec![Expr::string_lit("abc")],
                            },
                            Span::dummy(),
                        ))),
                        Span::dummy(),
                    ),
                ],
            },
            Span::dummy(),
        )];

        let (function_effects, _) = compute_program_callable_effects(&program);

        assert_eq!(
            function_effects.get("relay"),
            Some(&Effect::PURE.with_side_effects())
        );
    }

    #[test]
    fn test_program_function_effects_merge_callable_aliases_across_switch_paths() {
        let program = vec![Stmt::new(
            StmtKind::FunctionDecl {
                name: "relay".to_string(),
                params: vec![("flag".to_string(), None, None, false)],
                variadic: None,
                return_type: None,
                body: vec![
                    Stmt::new(
                        StmtKind::Switch {
                            subject: Expr::var("flag"),
                            cases: vec![
                                (
                                    vec![Expr::int_lit(1)],
                                    vec![
                                        Stmt::new(
                                            StmtKind::Assign {
                                                name: "g".to_string(),
                                                value: Expr::new(
                                                    ExprKind::FirstClassCallable(
                                                        CallableTarget::Function(Name::from("strlen")),
                                                    ),
                                                    Span::dummy(),
                                                ),
                                            },
                                            Span::dummy(),
                                        ),
                                        Stmt::new(StmtKind::Break, Span::dummy()),
                                    ],
                                ),
                                (vec![Expr::int_lit(2)], Vec::new()),
                            ],
                            default: Some(vec![Stmt::new(
                                StmtKind::Assign {
                                    name: "g".to_string(),
                                    value: Expr::new(
                                        ExprKind::FirstClassCallable(CallableTarget::Function(
                                            Name::from("strlen"),
                                        )),
                                        Span::dummy(),
                                    ),
                                },
                                Span::dummy(),
                            )]),
                        },
                        Span::dummy(),
                    ),
                    Stmt::new(
                        StmtKind::Return(Some(Expr::new(
                            ExprKind::ClosureCall {
                                var: "g".to_string(),
                                args: vec![Expr::string_lit("abc")],
                            },
                            Span::dummy(),
                        ))),
                        Span::dummy(),
                    ),
                ],
            },
            Span::dummy(),
        )];

        let (function_effects, _) = compute_program_callable_effects(&program);

        assert_eq!(
            function_effects.get("relay"),
            Some(&Effect::PURE.with_side_effects())
        );
    }

    #[test]
    fn test_fold_nested_integer_arithmetic() {
        let program = vec![Stmt::new(
            StmtKind::Echo(Expr::new(
                ExprKind::BinaryOp {
                    left: Box::new(Expr::new(
                        ExprKind::BinaryOp {
                            left: Box::new(Expr::int_lit(2)),
                            op: BinOp::Add,
                            right: Box::new(Expr::int_lit(3)),
                        },
                        Span::dummy(),
                    )),
                    op: BinOp::Mul,
                    right: Box::new(Expr::int_lit(4)),
                },
                Span::dummy(),
            )),
            Span::dummy(),
        )];

        let folded = fold_constants(program);

        assert_eq!(folded, vec![Stmt::echo(Expr::int_lit(20))]);
    }

    #[test]
    fn test_fold_constant_pow_to_float_literal() {
        let program = vec![Stmt::echo(Expr::new(
            ExprKind::BinaryOp {
                left: Box::new(Expr::int_lit(2)),
                op: BinOp::Pow,
                right: Box::new(Expr::int_lit(3)),
            },
            Span::dummy(),
        ))];

        let folded = fold_constants(program);

        assert_eq!(
            folded,
            vec![Stmt::echo(Expr::new(
                ExprKind::FloatLiteral(8.0),
                Span::dummy(),
            ))]
        );
    }

    #[test]
    fn test_skip_division_by_zero_fold() {
        let expr = Expr::new(
            ExprKind::BinaryOp {
                left: Box::new(Expr::int_lit(5)),
                op: BinOp::Div,
                right: Box::new(Expr::int_lit(0)),
            },
            Span::dummy(),
        );

        let folded = fold_constants(vec![Stmt::echo(expr.clone())]);

        assert_eq!(folded, vec![Stmt::echo(expr)]);
    }

    #[test]
    fn test_fold_string_concat_and_property_default() {
        let property = ClassProperty {
            name: "label".to_string(),
            visibility: Visibility::Public,
            readonly: false,
            default: Some(Expr::new(
                ExprKind::BinaryOp {
                    left: Box::new(Expr::string_lit("hello ")),
                    op: BinOp::Concat,
                    right: Box::new(Expr::string_lit("world")),
                },
                Span::dummy(),
            )),
            span: Span::dummy(),
        };

        let folded = fold_constants(vec![Stmt::new(
            StmtKind::ClassDecl {
                name: "Greeter".to_string(),
                extends: None,
                implements: Vec::new(),
                is_abstract: false,
                is_readonly_class: false,
                trait_uses: Vec::new(),
                properties: vec![property],
                methods: Vec::new(),
            },
            Span::dummy(),
        )]);

        let StmtKind::ClassDecl { properties, .. } = &folded[0].kind else {
            panic!("expected class declaration");
        };
        assert_eq!(
            properties[0].default,
            Some(Expr::string_lit("hello world"))
        );
    }

    #[test]
    fn test_fold_strict_and_numeric_comparisons() {
        let program = vec![
            Stmt::echo(Expr::new(
                ExprKind::BinaryOp {
                    left: Box::new(Expr::int_lit(2)),
                    op: BinOp::StrictEq,
                    right: Box::new(Expr::int_lit(2)),
                },
                Span::dummy(),
            )),
            Stmt::echo(Expr::new(
                ExprKind::BinaryOp {
                    left: Box::new(Expr::float_lit(2.5)),
                    op: BinOp::Lt,
                    right: Box::new(Expr::float_lit(3.0)),
                },
                Span::dummy(),
            )),
            Stmt::echo(Expr::new(
                ExprKind::BinaryOp {
                    left: Box::new(Expr::int_lit(2)),
                    op: BinOp::Spaceship,
                    right: Box::new(Expr::int_lit(3)),
                },
                Span::dummy(),
            )),
        ];

        let folded = fold_constants(program);

        assert_eq!(
            folded,
            vec![
                Stmt::echo(Expr::new(ExprKind::BoolLiteral(true), Span::dummy())),
                Stmt::echo(Expr::new(ExprKind::BoolLiteral(true), Span::dummy())),
                Stmt::echo(Expr::int_lit(-1)),
            ]
        );
    }

    #[test]
    fn test_fold_null_coalesce_and_ternary_only_for_scalar_constants() {
        let program = vec![
            Stmt::echo(Expr::new(
                ExprKind::NullCoalesce {
                    value: Box::new(Expr::new(ExprKind::Null, Span::dummy())),
                    default: Box::new(Expr::string_lit("fallback")),
                },
                Span::dummy(),
            )),
            Stmt::echo(Expr::new(
                ExprKind::Ternary {
                    condition: Box::new(Expr::string_lit("0")),
                    then_expr: Box::new(Expr::int_lit(10)),
                    else_expr: Box::new(Expr::int_lit(20)),
                },
                Span::dummy(),
            )),
        ];

        let folded = fold_constants(program);

        assert_eq!(
            folded,
            vec![
                Stmt::echo(Expr::string_lit("fallback")),
                Stmt::echo(Expr::int_lit(20)),
            ]
        );
    }

    #[test]
    fn test_fold_logical_ops_and_not_using_php_truthiness() {
        let program = vec![
            Stmt::echo(Expr::new(
                ExprKind::BinaryOp {
                    left: Box::new(Expr::string_lit("0")),
                    op: BinOp::Or,
                    right: Box::new(Expr::string_lit("hello")),
                },
                Span::dummy(),
            )),
            Stmt::echo(Expr::new(
                ExprKind::Not(Box::new(Expr::string_lit("0"))),
                Span::dummy(),
            )),
        ];

        let folded = fold_constants(program);

        assert_eq!(
            folded,
            vec![
                Stmt::echo(Expr::new(ExprKind::BoolLiteral(true), Span::dummy())),
                Stmt::echo(Expr::new(ExprKind::BoolLiteral(true), Span::dummy())),
            ]
        );
    }

    #[test]
    fn test_fold_scalar_casts_when_result_is_unambiguous() {
        let program = vec![
            Stmt::echo(Expr::new(
                ExprKind::Cast {
                    target: CastType::Int,
                    expr: Box::new(Expr::float_lit(3.7)),
                },
                Span::dummy(),
            )),
            Stmt::echo(Expr::new(
                ExprKind::Cast {
                    target: CastType::Float,
                    expr: Box::new(Expr::string_lit("3.14")),
                },
                Span::dummy(),
            )),
            Stmt::echo(Expr::new(
                ExprKind::Cast {
                    target: CastType::Bool,
                    expr: Box::new(Expr::string_lit("0")),
                },
                Span::dummy(),
            )),
            Stmt::echo(Expr::new(
                ExprKind::Cast {
                    target: CastType::String,
                    expr: Box::new(Expr::int_lit(42)),
                },
                Span::dummy(),
            )),
        ];

        let folded = fold_constants(program);

        assert_eq!(
            folded,
            vec![
                Stmt::echo(Expr::int_lit(3)),
                Stmt::echo(Expr::float_lit(3.14)),
                Stmt::echo(Expr::new(ExprKind::BoolLiteral(false), Span::dummy())),
                Stmt::echo(Expr::string_lit("42")),
            ]
        );
    }

    #[test]
    fn test_keep_ambiguous_string_casts_unfolded() {
        let expr = Expr::new(
            ExprKind::Cast {
                target: CastType::Int,
                expr: Box::new(Expr::string_lit("42abc")),
            },
            Span::dummy(),
        );

        let folded = fold_constants(vec![Stmt::echo(expr.clone())]);

        assert_eq!(folded, vec![Stmt::echo(expr)]);
    }

    #[test]
    fn test_prune_constant_if_chain() {
        let program = vec![Stmt::new(
            StmtKind::If {
                condition: Expr::new(ExprKind::BoolLiteral(false), Span::dummy()),
                then_body: vec![Stmt::echo(Expr::int_lit(1))],
                elseif_clauses: vec![
                    (
                        Expr::new(ExprKind::BoolLiteral(false), Span::dummy()),
                        vec![Stmt::echo(Expr::int_lit(2))],
                    ),
                    (
                        Expr::new(ExprKind::BoolLiteral(true), Span::dummy()),
                        vec![Stmt::echo(Expr::int_lit(3))],
                    ),
                ],
                else_body: Some(vec![Stmt::echo(Expr::int_lit(4))]),
            },
            Span::dummy(),
        )];

        let pruned = prune_constant_control_flow(program);

        assert_eq!(pruned, vec![Stmt::echo(Expr::int_lit(3))]);
    }

    #[test]
    fn test_prune_while_false_and_do_while_false() {
        let program = vec![
            Stmt::new(
                StmtKind::While {
                    condition: Expr::new(ExprKind::BoolLiteral(false), Span::dummy()),
                    body: vec![Stmt::echo(Expr::int_lit(1))],
                },
                Span::dummy(),
            ),
            Stmt::new(
                StmtKind::DoWhile {
                    body: vec![Stmt::echo(Expr::int_lit(2))],
                    condition: Expr::new(ExprKind::BoolLiteral(false), Span::dummy()),
                },
                Span::dummy(),
            ),
        ];

        let pruned = prune_constant_control_flow(program);

        assert_eq!(pruned, vec![Stmt::echo(Expr::int_lit(2))]);
    }

    #[test]
    fn test_prune_for_false_keeps_init_only() {
        let program = vec![Stmt::new(
            StmtKind::For {
                init: Some(Box::new(Stmt::assign("i", Expr::int_lit(1)))),
                condition: Some(Expr::new(ExprKind::BoolLiteral(false), Span::dummy())),
                update: Some(Box::new(Stmt::assign("i", Expr::int_lit(2)))),
                body: vec![Stmt::echo(Expr::int_lit(3))],
            },
            Span::dummy(),
        )];

        let pruned = prune_constant_control_flow(program);

        assert_eq!(pruned, vec![Stmt::assign("i", Expr::int_lit(1))]);
    }

    #[test]
    fn test_prune_block_drops_statements_after_return() {
        let program = vec![Stmt::new(
            StmtKind::FunctionDecl {
                name: "answer".into(),
                params: Vec::new(),
                variadic: None,
                return_type: None,
                body: vec![
                    Stmt::new(StmtKind::Return(Some(Expr::int_lit(7))), Span::dummy()),
                    Stmt::echo(Expr::int_lit(9)),
                ],
            },
            Span::dummy(),
        )];

        let pruned = prune_constant_control_flow(program);

        let StmtKind::FunctionDecl { body, .. } = &pruned[0].kind else {
            panic!("expected function");
        };
        assert_eq!(body.len(), 1);
        assert!(matches!(body[0].kind, StmtKind::Return(_)));
    }

    #[test]
    fn test_prune_drops_pure_expr_stmt() {
        let program = vec![Stmt::new(
            StmtKind::FunctionDecl {
                name: "answer".into(),
                params: Vec::new(),
                variadic: None,
                return_type: None,
                body: vec![
                    Stmt::new(
                        StmtKind::ExprStmt(Expr::binop(Expr::int_lit(2), BinOp::Pow, Expr::int_lit(8))),
                        Span::dummy(),
                    ),
                    Stmt::echo(Expr::int_lit(7)),
                ],
            },
            Span::dummy(),
        )];

        let pruned = prune_constant_control_flow(program);

        let StmtKind::FunctionDecl { body, .. } = &pruned[0].kind else {
            panic!("expected function");
        };
        assert_eq!(body.len(), 1);
        assert_eq!(body[0], Stmt::echo(Expr::int_lit(7)));
    }

    #[test]
    fn test_prune_ternary_drops_unused_pure_branch() {
        let program = vec![Stmt::assign(
            "x",
            Expr::new(
                ExprKind::Ternary {
                    condition: Box::new(Expr::new(ExprKind::BoolLiteral(true), Span::dummy())),
                    then_expr: Box::new(Expr::var("answer")),
                    else_expr: Box::new(Expr::binop(Expr::int_lit(2), BinOp::Pow, Expr::int_lit(8))),
                },
                Span::dummy(),
            ),
        )];

        let pruned = prune_constant_control_flow(program);

        assert_eq!(pruned, vec![Stmt::assign("x", Expr::var("answer"))]);
    }

    #[test]
    fn test_prune_short_circuit_drops_unused_pure_rhs() {
        let program = vec![Stmt::echo(Expr::new(
            ExprKind::BinaryOp {
                left: Box::new(Expr::new(ExprKind::BoolLiteral(true), Span::dummy())),
                op: BinOp::Or,
                right: Box::new(Expr::binop(Expr::int_lit(2), BinOp::Pow, Expr::int_lit(8))),
            },
            Span::dummy(),
        ))];

        let pruned = prune_constant_control_flow(program);

        assert_eq!(
            pruned,
            vec![Stmt::echo(Expr::new(ExprKind::BoolLiteral(true), Span::dummy()))]
        );
    }

    #[test]
    fn test_prune_block_drops_statements_after_exhaustive_if() {
        let program = vec![Stmt::new(
            StmtKind::FunctionDecl {
                name: "answer".into(),
                params: Vec::new(),
                variadic: None,
                return_type: None,
                body: vec![
                    Stmt::new(
                        StmtKind::If {
                            condition: Expr::var("flag"),
                            then_body: vec![Stmt::new(
                                StmtKind::Return(Some(Expr::int_lit(7))),
                                Span::dummy(),
                            )],
                            elseif_clauses: Vec::new(),
                            else_body: Some(vec![Stmt::new(
                                StmtKind::Return(Some(Expr::int_lit(8))),
                                Span::dummy(),
                            )]),
                        },
                        Span::dummy(),
                    ),
                    Stmt::echo(Expr::int_lit(9)),
                ],
            },
            Span::dummy(),
        )];

        let pruned = prune_constant_control_flow(program);

        let StmtKind::FunctionDecl { body, .. } = &pruned[0].kind else {
            panic!("expected function");
        };
        assert_eq!(body.len(), 1);
        let StmtKind::If { .. } = &body[0].kind else {
            panic!("expected if");
        };
    }

    #[test]
    fn test_prune_block_drops_statements_after_exhaustive_switch() {
        let program = vec![Stmt::new(
            StmtKind::FunctionDecl {
                name: "answer".into(),
                params: Vec::new(),
                variadic: None,
                return_type: None,
                body: vec![
                    Stmt::new(
                        StmtKind::Switch {
                            subject: Expr::var("flag"),
                            cases: vec![(
                                vec![Expr::int_lit(1)],
                                vec![Stmt::new(
                                    StmtKind::Return(Some(Expr::int_lit(7))),
                                    Span::dummy(),
                                )],
                            )],
                            default: Some(vec![Stmt::new(
                                StmtKind::Return(Some(Expr::int_lit(8))),
                                Span::dummy(),
                            )]),
                        },
                        Span::dummy(),
                    ),
                    Stmt::echo(Expr::int_lit(9)),
                ],
            },
            Span::dummy(),
        )];

        let pruned = prune_constant_control_flow(program);

        let StmtKind::FunctionDecl { body, .. } = &pruned[0].kind else {
            panic!("expected function");
        };
        assert_eq!(body.len(), 1);
        let StmtKind::Switch { .. } = &body[0].kind else {
            panic!("expected switch");
        };
    }

    #[test]
    fn test_prune_switch_case_body_drops_statements_after_break() {
        let program = vec![Stmt::new(
            StmtKind::Switch {
                subject: Expr::int_lit(1),
                cases: vec![(
                    vec![Expr::int_lit(1)],
                    vec![
                        Stmt::new(StmtKind::Break, Span::dummy()),
                        Stmt::echo(Expr::int_lit(9)),
                    ],
                )],
                default: None,
            },
            Span::dummy(),
        )];

        let pruned = prune_constant_control_flow(program);

        assert!(pruned.is_empty());
    }

    #[test]
    fn test_prune_match_expr_to_selected_arm() {
        let program = vec![Stmt::assign(
            "x",
            Expr::new(
                ExprKind::Match {
                    subject: Box::new(Expr::int_lit(3)),
                    arms: vec![
                        (vec![Expr::int_lit(1)], Expr::int_lit(10)),
                        (vec![Expr::int_lit(3)], Expr::int_lit(20)),
                    ],
                    default: Some(Box::new(Expr::int_lit(30))),
                },
                Span::dummy(),
            ),
        )];

        let pruned = prune_constant_control_flow(program);

        assert_eq!(pruned, vec![Stmt::assign("x", Expr::int_lit(20))]);
    }

    #[test]
    fn test_prune_match_uses_strict_case_comparison() {
        let program = vec![Stmt::assign(
            "x",
            Expr::new(
                ExprKind::Match {
                    subject: Box::new(Expr::new(ExprKind::BoolLiteral(true), Span::dummy())),
                    arms: vec![(vec![Expr::int_lit(1)], Expr::int_lit(10))],
                    default: Some(Box::new(Expr::int_lit(20))),
                },
                Span::dummy(),
            ),
        )];

        let pruned = prune_constant_control_flow(program);

        assert_eq!(pruned, vec![Stmt::assign("x", Expr::int_lit(20))]);
    }

    #[test]
    fn test_prune_switch_drops_leading_non_matching_cases() {
        let program = vec![Stmt::new(
            StmtKind::Switch {
                subject: Expr::int_lit(3),
                cases: vec![
                    (vec![Expr::int_lit(1)], vec![Stmt::echo(Expr::int_lit(10))]),
                    (
                        vec![Expr::int_lit(3)],
                        vec![Stmt::echo(Expr::int_lit(20)), Stmt::new(StmtKind::Break, Span::dummy())],
                    ),
                ],
                default: Some(vec![Stmt::echo(Expr::int_lit(30))]),
            },
            Span::dummy(),
        )];

        let pruned = prune_constant_control_flow(program);

        assert_eq!(pruned, vec![Stmt::echo(Expr::int_lit(20))]);
    }

    #[test]
    fn test_eliminate_dead_code_drops_statements_after_exhaustive_try_catch() {
        let program = vec![Stmt::new(
            StmtKind::FunctionDecl {
                name: "answer".into(),
                params: Vec::new(),
                variadic: None,
                return_type: None,
                body: vec![
                    Stmt::new(
                        StmtKind::Try {
                            try_body: vec![Stmt::new(
                                StmtKind::Return(Some(Expr::int_lit(7))),
                                Span::dummy(),
                            )],
                            catches: vec![crate::parser::ast::CatchClause {
                                exception_types: vec!["Exception".into()],
                                variable: Some("e".into()),
                                body: vec![Stmt::new(
                                    StmtKind::Return(Some(Expr::int_lit(8))),
                                    Span::dummy(),
                                )],
                            }],
                            finally_body: None,
                        },
                        Span::dummy(),
                    ),
                    Stmt::echo(Expr::int_lit(9)),
                ],
            },
            Span::dummy(),
        )];

        let eliminated = eliminate_dead_code(program);

        let StmtKind::FunctionDecl { body, .. } = &eliminated[0].kind else {
            panic!("expected function");
        };
        assert_eq!(body.len(), 1);
        assert!(matches!(body[0].kind, StmtKind::Return(_)));
    }

    #[test]
    fn test_eliminate_dead_code_drops_statements_after_try_finally_exit() {
        let program = vec![Stmt::new(
            StmtKind::FunctionDecl {
                name: "answer".into(),
                params: Vec::new(),
                variadic: None,
                return_type: None,
                body: vec![
                    Stmt::new(
                        StmtKind::Try {
                            try_body: vec![Stmt::new(
                                StmtKind::Return(Some(Expr::int_lit(7))),
                                Span::dummy(),
                            )],
                            catches: Vec::new(),
                            finally_body: Some(vec![Stmt::new(
                                StmtKind::Return(Some(Expr::int_lit(8))),
                                Span::dummy(),
                            )]),
                        },
                        Span::dummy(),
                    ),
                    Stmt::echo(Expr::int_lit(9)),
                ],
            },
            Span::dummy(),
        )];

        let eliminated = eliminate_dead_code(program);

        let StmtKind::FunctionDecl { body, .. } = &eliminated[0].kind else {
            panic!("expected function");
        };
        assert_eq!(body.len(), 1);
        assert!(matches!(body[0].kind, StmtKind::Try { .. }));
    }

    #[test]
    fn test_eliminate_dead_code_keeps_statements_after_fallthrough_try() {
        let program = vec![Stmt::new(
            StmtKind::FunctionDecl {
                name: "answer".into(),
                params: Vec::new(),
                variadic: None,
                return_type: None,
                body: vec![
                    Stmt::new(
                        StmtKind::Try {
                            try_body: vec![Stmt::echo(Expr::int_lit(7))],
                            catches: vec![crate::parser::ast::CatchClause {
                                exception_types: vec!["Exception".into()],
                                variable: Some("e".into()),
                                body: vec![Stmt::new(
                                    StmtKind::Return(Some(Expr::int_lit(8))),
                                    Span::dummy(),
                                )],
                            }],
                            finally_body: None,
                        },
                        Span::dummy(),
                    ),
                    Stmt::echo(Expr::int_lit(9)),
                ],
            },
            Span::dummy(),
        )];

        let eliminated = eliminate_dead_code(program);

        let StmtKind::FunctionDecl { body, .. } = &eliminated[0].kind else {
            panic!("expected function");
        };
        assert_eq!(body.len(), 2);
        assert_eq!(body[1], Stmt::echo(Expr::int_lit(9)));
    }

    #[test]
    fn test_eliminate_dead_code_replaces_empty_if_with_effectful_condition_eval() {
        let call = Expr::new(
            ExprKind::FunctionCall {
                name: Name::unqualified("touch"),
                args: Vec::new(),
            },
            Span::dummy(),
        );
        let program = vec![Stmt::new(
            StmtKind::If {
                condition: call.clone(),
                then_body: Vec::new(),
                elseif_clauses: Vec::new(),
                else_body: None,
            },
            Span::dummy(),
        )];

        let pruned = eliminate_dead_code(program);

        assert_eq!(
            pruned,
            vec![Stmt::new(StmtKind::ExprStmt(call), Span::dummy())]
        );
    }

    #[test]
    fn test_eliminate_dead_code_drops_empty_ifdef_shell() {
        let program = vec![Stmt::new(
            StmtKind::IfDef {
                symbol: "DEBUG".into(),
                then_body: Vec::new(),
                else_body: Some(Vec::new()),
            },
            Span::dummy(),
        )];

        let pruned = eliminate_dead_code(program);

        assert!(pruned.is_empty());
    }

    #[test]
    fn test_eliminate_dead_code_replaces_empty_switch_with_subject_eval() {
        let call = Expr::new(
            ExprKind::FunctionCall {
                name: Name::unqualified("touch"),
                args: Vec::new(),
            },
            Span::dummy(),
        );
        let program = vec![Stmt::new(
            StmtKind::Switch {
                subject: call.clone(),
                cases: Vec::new(),
                default: None,
            },
            Span::dummy(),
        )];

        let pruned = eliminate_dead_code(program);

        assert_eq!(
            pruned,
            vec![Stmt::new(StmtKind::ExprStmt(call), Span::dummy())]
        );
    }

    #[test]
    fn test_eliminate_dead_code_inlines_empty_try_finally_body() {
        let program = vec![Stmt::new(
            StmtKind::Try {
                try_body: Vec::new(),
                catches: vec![crate::parser::ast::CatchClause {
                    exception_types: vec![Name::unqualified("Exception")],
                    variable: Some("e".into()),
                    body: vec![Stmt::echo(Expr::int_lit(7))],
                }],
                finally_body: Some(vec![Stmt::echo(Expr::int_lit(9))]),
            },
            Span::dummy(),
        )];

        let pruned = eliminate_dead_code(program);

        assert_eq!(pruned, vec![Stmt::echo(Expr::int_lit(9))]);
    }

    #[test]
    fn test_eliminate_dead_code_inverts_single_live_else_branch() {
        let program = vec![Stmt::new(
            StmtKind::If {
                condition: Expr::var("flag"),
                then_body: Vec::new(),
                elseif_clauses: Vec::new(),
                else_body: Some(vec![Stmt::echo(Expr::int_lit(7))]),
            },
            Span::dummy(),
        )];

        let pruned = eliminate_dead_code(program);

        assert_eq!(
            pruned,
            vec![Stmt::new(
                StmtKind::If {
                    condition: Expr::new(
                        ExprKind::Not(Box::new(Expr::var("flag"))),
                        Span::dummy(),
                    ),
                    then_body: vec![Stmt::echo(Expr::int_lit(7))],
                    elseif_clauses: Vec::new(),
                    else_body: None,
                },
                Span::dummy(),
            )]
        );
    }

    #[test]
    fn test_eliminate_dead_code_inlines_default_only_switch() {
        let program = vec![Stmt::new(
            StmtKind::Switch {
                subject: Expr::var("flag"),
                cases: Vec::new(),
                default: Some(vec![Stmt::echo(Expr::int_lit(9))]),
            },
            Span::dummy(),
        )];

        let pruned = eliminate_dead_code(program);

        assert_eq!(pruned, vec![Stmt::echo(Expr::int_lit(9))]);
    }

    #[test]
    fn test_eliminate_dead_code_nests_elseif_chain_after_empty_head() {
        let program = vec![Stmt::new(
            StmtKind::If {
                condition: Expr::var("a"),
                then_body: Vec::new(),
                elseif_clauses: vec![(
                    Expr::var("b"),
                    vec![Stmt::echo(Expr::int_lit(7))],
                )],
                else_body: Some(vec![Stmt::echo(Expr::int_lit(9))]),
            },
            Span::dummy(),
        )];

        let pruned = eliminate_dead_code(program);

        assert_eq!(
            pruned,
            vec![Stmt::new(
                StmtKind::If {
                    condition: Expr::new(
                        ExprKind::Not(Box::new(Expr::var("a"))),
                        Span::dummy(),
                    ),
                    then_body: vec![Stmt::new(
                        StmtKind::If {
                            condition: Expr::var("b"),
                            then_body: vec![Stmt::echo(Expr::int_lit(7))],
                            elseif_clauses: Vec::new(),
                            else_body: Some(vec![Stmt::echo(Expr::int_lit(9))]),
                        },
                        Span::dummy(),
                    )],
                    elseif_clauses: Vec::new(),
                    else_body: None,
                },
                Span::dummy(),
            )]
        );
    }

    #[test]
    fn test_eliminate_dead_code_materializes_constant_switch_match() {
        let program = vec![Stmt::new(
            StmtKind::Switch {
                subject: Expr::int_lit(2),
                cases: vec![
                    (
                        vec![Expr::int_lit(1)],
                        vec![Stmt::echo(Expr::int_lit(5)), Stmt::new(StmtKind::Break, Span::dummy())],
                    ),
                    (
                        vec![Expr::int_lit(2)],
                        vec![Stmt::echo(Expr::int_lit(7)), Stmt::new(StmtKind::Break, Span::dummy())],
                    ),
                ],
                default: Some(vec![Stmt::echo(Expr::int_lit(9))]),
            },
            Span::dummy(),
        )];

        let pruned = eliminate_dead_code(program);

        assert_eq!(pruned, vec![Stmt::echo(Expr::int_lit(7))]);
    }

    #[test]
    fn test_eliminate_dead_code_materializes_constant_switch_fallthrough() {
        let program = vec![Stmt::new(
            StmtKind::Switch {
                subject: Expr::int_lit(1),
                cases: vec![
                    (vec![Expr::int_lit(1)], Vec::new()),
                    (
                        vec![Expr::int_lit(2)],
                        vec![Stmt::echo(Expr::int_lit(7)), Stmt::new(StmtKind::Break, Span::dummy())],
                    ),
                ],
                default: Some(vec![Stmt::echo(Expr::int_lit(9))]),
            },
            Span::dummy(),
        )];

        let pruned = eliminate_dead_code(program);

        assert_eq!(pruned, vec![Stmt::echo(Expr::int_lit(7))]);
    }

    #[test]
    fn test_eliminate_dead_code_materializes_constant_switch_default() {
        let program = vec![Stmt::new(
            StmtKind::Switch {
                subject: Expr::int_lit(3),
                cases: vec![(
                    vec![Expr::int_lit(1)],
                    vec![Stmt::echo(Expr::int_lit(5)), Stmt::new(StmtKind::Break, Span::dummy())],
                )],
                default: Some(vec![Stmt::echo(Expr::int_lit(9))]),
            },
            Span::dummy(),
        )];

        let pruned = eliminate_dead_code(program);

        assert_eq!(pruned, vec![Stmt::echo(Expr::int_lit(9))]);
    }

    #[test]
    fn test_eliminate_dead_code_inlines_non_throwing_try_catch() {
        let program = vec![Stmt::new(
            StmtKind::Try {
                try_body: vec![Stmt::echo(Expr::int_lit(7))],
                catches: vec![crate::parser::ast::CatchClause {
                    exception_types: vec![Name::unqualified("Exception")],
                    variable: Some("e".into()),
                    body: vec![Stmt::echo(Expr::int_lit(9))],
                }],
                finally_body: None,
            },
            Span::dummy(),
        )];

        let pruned = eliminate_dead_code(program);

        assert_eq!(pruned, vec![Stmt::echo(Expr::int_lit(7))]);
    }

    #[test]
    fn test_eliminate_dead_code_inlines_non_throwing_try_finally_fallthrough() {
        let program = vec![Stmt::new(
            StmtKind::Try {
                try_body: vec![Stmt::echo(Expr::int_lit(7))],
                catches: Vec::new(),
                finally_body: Some(vec![Stmt::echo(Expr::int_lit(9))]),
            },
            Span::dummy(),
        )];

        let pruned = eliminate_dead_code(program);

        assert_eq!(pruned, vec![Stmt::echo(Expr::int_lit(7)), Stmt::echo(Expr::int_lit(9))]);
    }

    #[test]
    fn test_eliminate_dead_code_keeps_non_throwing_try_finally_with_return() {
        let program = vec![Stmt::new(
            StmtKind::Try {
                try_body: vec![Stmt::new(
                    StmtKind::Return(Some(Expr::int_lit(7))),
                    Span::dummy(),
                )],
                catches: Vec::new(),
                finally_body: Some(vec![Stmt::echo(Expr::int_lit(9))]),
            },
            Span::dummy(),
        )];

        let pruned = eliminate_dead_code(program);

        assert_eq!(pruned.len(), 1);
        assert!(matches!(pruned[0].kind, StmtKind::Try { .. }));
    }

    #[test]
    fn test_eliminate_dead_code_hoists_non_throwing_try_prefix() {
        let program = vec![Stmt::new(
            StmtKind::Try {
                try_body: vec![
                    Stmt::echo(Expr::int_lit(7)),
                    Stmt::new(StmtKind::Throw(Expr::var("boom")), Span::dummy()),
                ],
                catches: vec![crate::parser::ast::CatchClause {
                    exception_types: vec![Name::unqualified("Exception")],
                    variable: Some("e".into()),
                    body: vec![Stmt::echo(Expr::int_lit(9))],
                }],
                finally_body: None,
            },
            Span::dummy(),
        )];

        let pruned = eliminate_dead_code(program);

        assert_eq!(pruned.len(), 2);
        assert_eq!(pruned[0], Stmt::echo(Expr::int_lit(7)));
        assert!(matches!(pruned[1].kind, StmtKind::Try { .. }));
    }

    #[test]
    fn test_eliminate_dead_code_flattens_nested_single_path_ifs() {
        let program = vec![Stmt::new(
            StmtKind::If {
                condition: Expr::var("a"),
                then_body: vec![Stmt::new(
                    StmtKind::If {
                        condition: Expr::var("b"),
                        then_body: vec![Stmt::echo(Expr::int_lit(7))],
                        elseif_clauses: Vec::new(),
                        else_body: None,
                    },
                    Span::dummy(),
                )],
                elseif_clauses: Vec::new(),
                else_body: None,
            },
            Span::dummy(),
        )];

        let pruned = eliminate_dead_code(program);

        assert_eq!(pruned.len(), 1);
        match &pruned[0].kind {
            StmtKind::If {
                condition,
                then_body,
                elseif_clauses,
                else_body,
            } => {
                assert!(elseif_clauses.is_empty());
                assert!(else_body.is_none());
                assert_eq!(then_body, &vec![Stmt::echo(Expr::int_lit(7))]);
                assert_eq!(
                    *condition,
                    Expr::new(
                        ExprKind::BinaryOp {
                            left: Box::new(Expr::var("a")),
                            op: BinOp::And,
                            right: Box::new(Expr::var("b")),
                        },
                        Span::dummy(),
                    )
                );
            }
            other => panic!("expected flattened if, got {:?}", other),
        }
    }
}
