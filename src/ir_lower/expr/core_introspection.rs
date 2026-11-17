//! Purpose:
//! Lowers PHP Core introspection operations that require the current lexical scope.
//!
//! Called from:
//! - `super::function_calls::lower_function_call()` before registry lowering.
//!
//! Key details:
//! - `get_defined_vars()` supports direct calls and PHP's explicit `call_user_func*` forms.

use super::*;

/// Builds `get_defined_vars()` from the definitely visible locals at its direct call site.
pub(super) fn lower_get_defined_vars(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    if php_symbol_key(name.trim_start_matches('\\')) != "get_defined_vars" || !args.is_empty() {
        return None;
    }
    Some(lower_visible_defined_vars(ctx, expr))
}

/// Builds the associative array of locals visible at the current lexical call site.
pub(super) fn lower_visible_defined_vars(
    ctx: &mut LoweringContext<'_, '_>,
    expr: &Expr,
) -> LoweredValue {
    // `visible_local_names` already drops every compiler-generated frame and parser temporary,
    // which matters far beyond `func_*` users: once the backtrace gate fires, EVERY function in
    // the program receives the hidden argument collector.
    let names = ctx.visible_local_names();
    let hash_ty = PhpType::AssocArray {
        key: Box::new(PhpType::Str),
        value: Box::new(PhpType::Mixed),
    };
    let hash = ctx.emit_value(
        Op::HashNew,
        Vec::new(),
        Some(Immediate::Capacity(names.len() as u32)),
        hash_ty,
        Op::HashNew.default_effects(),
        Some(expr.span),
    );
    for name in names {
        let key = lower_string_literal(ctx, &name, expr);
        let value = ctx.load_local(&name, Some(expr.span));
        let value = box_value_as_mixed(ctx, value, expr.span);
        ctx.emit_void(
            Op::HashSet,
            vec![hash.value, key.value, value.value],
            None,
            Op::HashSet.default_effects(),
            Some(expr.span),
        );
    }
    // A closed-world `extract()` assigns only the locals the frame compiled; the keys naming no
    // compiled local stay in its kept source array. They are still variables of the frame for
    // php, so they join the list after the locals (`+` keeps a local over its source key, which
    // is right for EXTR_SKIP and EXTR_OVERWRITE alike: the local already holds the winner).
    let source = extract_seen_local();
    if crate::closed_world::is_enabled() && ctx.has_local_slot(&source) {
        use crate::synthetic_class::{e_array, e_binop, e_call, e_ternary, e_var};
        let listed = crate::names::generated_local_name("__elephc_defined_vars");
        ctx.store_local(&listed, hash, PhpType::Mixed, Some(expr.span));
        let merged = e_binop(
            e_var(&listed),
            BinOp::Add,
            e_ternary(
                e_call("is_array", vec![e_var(&source)]),
                e_var(&source),
                e_array(Vec::new()),
            ),
        );
        return lower_expr(ctx, &merged);
    }
    hash
}

/// The local a closed-world `extract()` keeps its source array in: generated, so neither
/// `get_defined_vars()` nor a later `extract()` sees it as a variable of the frame.
fn extract_source_local() -> String {
    crate::names::generated_local_name("__elephc_extract_source")
}

/// The local accumulating every key any closed-world `extract()` of the frame imported: each call
/// keeps its own array in [`extract_source_local`], and `get_defined_vars()` must list them all.
fn extract_seen_local() -> String {
    crate::names::generated_local_name("__elephc_extract_seen")
}

/// Lowers `extract($array [, EXTR_OVERWRITE | EXTR_SKIP])` onto the frame's compiled locals.
///
/// A closed-world build has no interpreter scope to publish variables into, but the frame's
/// locals are all compiled, so the symbol table `extract()` writes is known: every gradual
/// local whose name is a key of the array is assigned (EXTR_OVERWRITE, the default) or assigned
/// only while it is still unset (EXTR_SKIP), and the call evaluates to the number imported.
/// Keys naming no compiled local create nothing, exactly as a variable no code ever reads would.
///
/// Only the two literal flags are handled; any other flag keeps the interpreter-backed call,
/// which the closed-world check then reports.
pub(super) fn lower_closed_world_extract(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    use crate::synthetic_class::{e_binop, e_bool, e_call, e_index, e_int, e_str, e_var, s_assign, s_if};
    if !crate::closed_world::is_enabled()
        || php_symbol_key(name.trim_start_matches('\\')) != "extract"
        || args.is_empty()
        || args.len() > 2
        || args.iter().any(|arg| matches!(arg.kind, ExprKind::NamedArg { .. } | ExprKind::Spread(_)))
    {
        return None;
    }
    let skip = match args.get(1).map(|flags| &flags.kind) {
        None => false,
        Some(ExprKind::IntLiteral(0)) => false,
        Some(ExprKind::IntLiteral(1)) => true,
        Some(ExprKind::ConstRef(constant)) => match constant.as_str().trim_start_matches('\\') {
            "EXTR_OVERWRITE" => false,
            "EXTR_SKIP" => true,
            _ => return None,
        },
        Some(_) => return None,
    };
    let source_local = extract_source_local();
    let count_local = crate::names::generated_local_name("__elephc_extract_count");
    let targets = ctx.extractable_local_names();
    // EXTR_SKIP leaves every variable that already exists alone, a null one included, which the
    // locals the frame definitely initialized here are known to be.
    let initialized = ctx.visible_local_names();
    let source = Stmt::new(
        StmtKind::Assign { name: source_local.clone(), value: args[0].clone() },
        expr.span,
    );
    super::super::stmt::lower_stmt(ctx, &source);
    super::super::stmt::lower_stmt(ctx, &s_assign(&count_local, e_int(0)));
    // php answers the number of variables imported, so a key naming no local of this frame (a
    // variable nothing reads) still counts when it is a valid variable name.
    super::super::stmt::lower_stmt(
        ctx,
        &count_unread_extract_keys(&source_local, &count_local, &targets),
    );
    for target in targets {
        if skip && initialized.contains(&target) {
            continue;
        }
        let mut present = e_binop(
            e_call("is_array", vec![e_var(&source_local)]),
            BinOp::And,
            e_call("array_key_exists", vec![e_str(&target), e_var(&source_local)]),
        );
        if skip {
            present = e_binop(present, BinOp::And, crate::synthetic_class::e_not(e_call("isset", vec![e_var(&target)])));
        }
        let import = s_if(
            present,
            vec![
                s_assign(&target, e_index(e_var(&source_local), e_str(&target))),
                s_assign(&count_local, e_binop(e_var(&count_local), BinOp::Add, e_int(1))),
            ],
            vec![],
            None,
        );
        super::super::stmt::lower_stmt(ctx, &import);
    }
    let _ = e_bool;
    // Fold this call's array into the frame's record of extracted keys: a later EXTR_OVERWRITE
    // call wins a shared key, an EXTR_SKIP one does not.
    {
        use crate::synthetic_class::{e_array, e_ternary};
        let seen_local = extract_seen_local();
        let previous = if ctx.has_local_slot(&seen_local) {
            e_ternary(
                e_call("is_array", vec![e_var(&seen_local)]),
                e_var(&seen_local),
                e_array(Vec::new()),
            )
        } else {
            e_array(Vec::new())
        };
        let current = e_ternary(
            e_call("is_array", vec![e_var(&source_local)]),
            e_var(&source_local),
            e_array(Vec::new()),
        );
        let merged = if skip {
            e_binop(previous, BinOp::Add, current)
        } else {
            e_call("array_replace", vec![previous, current])
        };
        super::super::stmt::lower_stmt(ctx, &s_assign(&seen_local, merged));
    }
    Some(lower_expr(ctx, &e_var(&count_local)))
}

/// Builds the statement that adds to `count_local` every key of the extracted array that is a
/// valid variable name (`[A-Za-z_\x80-\xff][A-Za-z0-9_\x80-\xff]*`, not `this`) and names none
/// of `handled`, the locals the import loop assigns and counts itself.
fn count_unread_extract_keys(source_local: &str, count_local: &str, handled: &[String]) -> Stmt {
    use crate::synthetic_class::{e_array, e_binop, e_call, e_index, e_int, e_not, e_str, e_var, s_assign, s_foreach, s_if};
    // Every ASCII byte that cannot appear in a variable name; bytes 0x80-0xff all can.
    let invalid: String = (0u8..0x80)
        .filter(|byte| !byte.is_ascii_alphanumeric() && *byte != b'_')
        .map(char::from)
        .collect();
    let key_local = crate::names::generated_local_name("__elephc_extract_key");
    let key = || e_var(&key_local);
    let and = |left, right| e_binop(left, BinOp::And, right);
    let valid = and(
        and(
            and(
                e_call("is_string", vec![key()]),
                e_binop(key(), BinOp::StrictNotEq, e_str("")),
            ),
            and(
                e_binop(key(), BinOp::StrictNotEq, e_str("this")),
                e_not(e_call(
                    "in_array",
                    vec![
                        key(),
                        e_array(handled.iter().map(|name| e_str(name)).collect()),
                        crate::synthetic_class::e_bool(true),
                    ],
                )),
            ),
        ),
        and(
            e_binop(
                e_call("strcspn", vec![key(), e_str(&invalid)]),
                BinOp::StrictEq,
                e_call("strlen", vec![key()]),
            ),
            e_binop(
                e_call("strpos", vec![e_str("0123456789"), e_index(key(), e_int(0))]),
                BinOp::StrictEq,
                crate::synthetic_class::e_bool(false),
            ),
        ),
    );
    s_if(
        e_call("is_array", vec![e_var(source_local)]),
        vec![s_foreach(
            e_call("array_keys", vec![e_var(source_local)]),
            None,
            &key_local,
            vec![s_if(
                valid,
                vec![s_assign(count_local, e_binop(e_var(count_local), BinOp::Add, e_int(1)))],
                vec![],
                None,
            )],
        )],
        vec![],
        None,
    )
}
