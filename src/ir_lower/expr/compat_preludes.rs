//! Purpose:
//! Routes PHP builtin call shapes through narrow compatibility preludes when the native EIR
//! representation cannot preserve their semantics.
//!
//! Called from:
//! - `crate::ir_lower::expr::function_calls::lower_function_call()` before registry lowering.
//!
//! Key details:
//! - Routing remains arity- and representation-sensitive; native packed-array paths stay native.

use super::*;

/// Routes `assert($assertion)` to the prelude while the richer description form remains native.
pub(super) fn lower_single_arg_assert(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    if php_symbol_key(name.trim_start_matches('\\')) != "assert"
        || args.len() != 1
        || args.iter().any(is_spread_arg)
    {
        return None;
    }
    Some(lower_function_call(
        ctx,
        &crate::names::Name::unqualified(crate::assert_prelude::ASSERT_ONE_ARG_NAME),
        args,
        expr,
    ))
}

/// Lowers `unpack('H*name', $data)` as `['name' => bin2hex($data)]`, boxed like `unpack()`'s
/// `array|false` result.
///
/// `H*` reads the whole string as high-nibble-first hexadecimal, which is exactly `bin2hex()`, and
/// it cannot fail. The backend's `unpack()` covers the integer directives only, so league/uri's
/// `IPv6\Converter::expandIp()` (`unpack('H*hex', inet_pton($ip))`) was refused, and that one
/// refusal sent its file to the interpreter and cost the Symfony demo a whole second compile.
/// An unnamed `H*` keys the value `1`, as php does.
pub(super) fn lower_unpack_whole_hex(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    if php_symbol_key(name.trim_start_matches('\\')) != "unpack"
        || args.len() != 2
        || crate::types::call_args::has_named_args(args)
        || args.iter().any(is_spread_arg)
    {
        return None;
    }
    let ExprKind::StringLiteral(format) = &args[0].kind else {
        return None;
    };
    let field = format.strip_prefix("H*")?;
    if !field.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'_') {
        return None;
    }
    let key = if field.is_empty() {
        Expr::new(ExprKind::IntLiteral(1), expr.span)
    } else {
        Expr::new(ExprKind::StringLiteral(field.to_string()), expr.span)
    };
    let hex = Expr::new(
        ExprKind::FunctionCall {
            name: crate::names::Name::unqualified("bin2hex"),
            args: vec![Expr::new(
                ExprKind::Cast {
                    target: crate::parser::ast::CastType::String,
                    expr: Box::new(args[1].clone()),
                },
                args[1].span,
            )],
        },
        expr.span,
    );
    let array = Expr::new(ExprKind::ArrayLiteralAssoc(vec![(key, hex)]), expr.span);
    let value = lower_expr(ctx, &array);
    Some(ctx.box_value_as_mixed(value, PhpType::Mixed, Some(expr.span)))
}

/// Routes `is_callable($value, $syntax_only[, &$name])` to the prelude, keeping the one-argument
/// form on the backend predicate.
///
/// The backend answers one question -- can this value be called -- and `$syntax_only` asks another
/// one, whether it merely has callable SHAPE. `symfony/http-kernel`'s `ServiceValueResolver` uses
/// the second form to decide whether `[$class, $method]` can be flattened to `Class::method`.
pub(super) fn lower_is_callable_with_options(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    if php_symbol_key(name.trim_start_matches('\\')) != "is_callable"
        || !matches!(args.len(), 2 | 3)
        || args.iter().any(is_spread_arg)
    {
        return None;
    }
    // Named arguments (`is_callable($value, syntax_only: true)`, nette/utils `Validators`) are
    // left to the user-call path: the helper's parameters carry php's own names.
    Some(lower_function_call(
        ctx,
        &crate::names::Name::unqualified(crate::is_callable_prelude::IS_CALLABLE_EXT_NAME),
        args,
        expr,
    ))
}

/// Routes narrowly supported builtin arities to their elephc-PHP compatibility helpers.
pub(super) fn lower_backend_gap_builtin_shape(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    let builtin = php_symbol_key(name.trim_start_matches('\\'));
    let single_value_arg = match args {
        [arg] if !is_spread_arg(arg) => Some(match &arg.kind {
            ExprKind::NamedArg { value, .. } => value.as_ref(),
            _ => arg,
        }),
        _ => None,
    };
    // The helper the redirect targets hands an all-scalar list to the native sort, from inside
    // `SORT_SCALAR_VALUES_NAME`; redirecting that call too would recurse forever.
    let inside_scalar_sort = ctx
        .builder
        .function()
        .name
        .eq_ignore_ascii_case(crate::backend_gap_prelude::SORT_SCALAR_VALUES_NAME);
    if matches!(builtin.as_str(), "sort" | "rsort")
        && !inside_scalar_sort
        && single_value_arg.is_some_and(|arg| {
            match materialized_expr_type_for_merge(ctx, arg).codegen_repr() {
                PhpType::Array(element) => element.codegen_repr() == PhpType::Mixed,
                PhpType::AssocArray { value, .. } => {
                    matches!(value.codegen_repr(), PhpType::Mixed | PhpType::Union(_))
                }
                PhpType::Mixed | PhpType::Union(_) => true,
                _ => false,
            }
        })
    {
        return Some(lower_function_call(
            ctx,
            &crate::names::Name::unqualified(if builtin == "sort" {
                crate::backend_gap_prelude::SORT_MIXED_NAME
            } else {
                crate::backend_gap_prelude::RSORT_MIXED_NAME
            }),
            args,
            expr,
        ));
    }
    // A one-argument `min()` / `max()` over a gradual operand goes to `crate::min_max_prelude`:
    // the native form reads a typed array. The helper's own body never calls either builtin.
    if matches!(builtin.as_str(), "min" | "max")
        && single_value_arg.is_some_and(|arg| {
            matches!(
                materialized_expr_type_for_merge(ctx, arg).codegen_repr(),
                PhpType::Mixed | PhpType::Union(_)
            )
        })
    {
        let mut helper_args = args.to_vec();
        helper_args.push(Expr::new(ExprKind::BoolLiteral(builtin == "max"), expr.span));
        return Some(lower_function_call(
            ctx,
            &crate::names::Name::unqualified(crate::min_max_prelude::MIN_MAX_MIXED_NAME),
            &helper_args,
            expr,
        ));
    }
    // `max(...$values)` / `min(...$values)` over a list of boxed or gradual elements -- doctrine/dbal's
    // `max(...array_map(mb_strlen(...), $column['values']))` -- has no native lowering: the spread
    // reaches the array form with an `array<mixed>` operand. The helper takes the spread flag so php's
    // argument-count rules still hold (an empty spread has no argument at all; a single element is
    // that one argument, which must itself be an array).
    if matches!(builtin.as_str(), "min" | "max") && args.len() == 1 {
        if let ExprKind::Spread(inner) = &args[0].kind {
            if matches!(
                materialized_expr_type_for_merge(ctx, inner).codegen_repr(),
                PhpType::Mixed | PhpType::Union(_)
            ) || matches!(
                materialized_expr_type_for_merge(ctx, inner).codegen_repr(),
                PhpType::Array(element) if element.codegen_repr() == PhpType::Mixed
            ) {
                let helper_args = vec![
                    inner.as_ref().clone(),
                    Expr::new(ExprKind::BoolLiteral(builtin == "max"), expr.span),
                    Expr::new(ExprKind::BoolLiteral(true), expr.span),
                ];
                return Some(lower_function_call(
                    ctx,
                    &crate::names::Name::unqualified(crate::min_max_prelude::MIN_MAX_MIXED_NAME),
                    &helper_args,
                    expr,
                ));
            }
        }
    }
    // `count($value, COUNT_RECURSIVE)` over a receiver that can hold nested arrays goes to
    // `crate::count_recursive_prelude`: indexed storage is untagged, so the native total cannot
    // tell a nested array from a scalar slot and the lowering refuses such a receiver.
    if builtin == "count"
        && args.len() == 2
        && !args.iter().any(is_spread_arg)
        && !args.iter().any(|arg| matches!(arg.kind, ExprKind::NamedArg { .. }))
        && count_receiver_can_nest(&materialized_expr_type_for_merge(ctx, &args[0]).codegen_repr())
    {
        return Some(lower_function_call(
            ctx,
            &crate::names::Name::unqualified(crate::count_recursive_prelude::COUNT_RECURSIVE_NAME),
            args,
            expr,
        ));
    }
    // `array_fill_keys()` over keys known only at run time goes to `crate::array_fill_keys_prelude`:
    // the native lowering reads int or string keys. The helper's own body never calls it.
    if builtin == "array_fill_keys"
        && args.len() == 2
        && !args.iter().any(is_spread_arg)
        && !args.iter().any(|arg| matches!(arg.kind, ExprKind::NamedArg { .. }))
        && match materialized_expr_type_for_merge(ctx, &args[0]).codegen_repr() {
            PhpType::Array(element) => {
                matches!(element.codegen_repr(), PhpType::Mixed | PhpType::Union(_))
            }
            PhpType::AssocArray { value, .. } => {
                matches!(value.codegen_repr(), PhpType::Mixed | PhpType::Union(_))
            }
            PhpType::Mixed | PhpType::Union(_) => true,
            _ => false,
        }
    {
        return Some(lower_function_call(
            ctx,
            &crate::names::Name::unqualified(
                crate::array_fill_keys_prelude::ARRAY_FILL_KEYS_MIXED_NAME,
            ),
            args,
            expr,
        ));
    }
    // A `string` first argument of a math builtin goes through `crate::numeric_string_arg_prelude`
    // first: the native lowerings read an int or a float. The helper returns the number, so the
    // re-entered builtin call no longer matches this arm.
    if crate::numeric_string_arg_prelude::NUMERIC_ARG_BUILTINS.contains(&builtin.as_str())
        && !args.is_empty()
        && !args.iter().any(is_spread_arg)
        && !matches!(args[0].kind, ExprKind::NamedArg { .. })
        && materialized_expr_type_for_merge(ctx, &args[0]).codegen_repr() == PhpType::Str
    {
        let mut converted_args = args.to_vec();
        converted_args[0] = Expr::new(
            ExprKind::FunctionCall {
                name: crate::names::Name::unqualified(
                    crate::numeric_string_arg_prelude::NUMERIC_STRING_ARG_NAME,
                ),
                args: vec![
                    args[0].clone(),
                    Expr::new(ExprKind::StringLiteral(builtin.clone()), Span::dummy()),
                ],
            },
            Span::dummy(),
        );
        return Some(lower_function_call(
            ctx,
            &crate::names::Name::unqualified(builtin.as_str()),
            &converted_args,
            expr,
        ));
    }
    // `preg_replace()` / `preg_replace_callback()` with an operand that may be an array, or with
    // `$flags`, go to `crate::preg_array_prelude`: the native runtime replaces one pattern in one
    // string. The checker typed exactly those calls `mixed`
    // (`types::checker::builtins::preg_operand_may_be_array`), and redirecting on the type
    // it FILED for this call keeps the two in agreement -- a call it typed `string` must return
    // one. The prelude's own calls pass strings, so they are filed `?string` and stay native. Named arguments
    // pass through as they are: the helpers' parameters carry php's names, which is why this
    // runs before the named-argument bail-out below.
    if (builtin == "preg_replace" || builtin == "preg_replace_callback")
        && !args.iter().any(is_spread_arg)
        && expr.span.line != 0
        && ctx
            .builtin_call_types
            .get(&(ctx.loop_storage_scope.clone(), expr.span))
            .is_some_and(|checked| *checked == PhpType::Mixed)
    {
        let helper = if builtin == "preg_replace" {
            crate::preg_array_prelude::PREG_REPLACE_FULL_NAME
        } else {
            crate::preg_array_prelude::PREG_REPLACE_CALLBACK_FULL_NAME
        };
        return Some(lower_function_call(
            ctx,
            &crate::names::Name::unqualified(helper),
            args,
            expr,
        ));
    }
    if crate::types::call_args::has_named_args(args)
        || (builtin != "array_unshift" && args.iter().any(is_spread_arg))
    {
        return None;
    }
    // `implode()` over elements the native joiners cannot render: objects (php calls
    // `__toString()`) and floats were refused at compile time, and a boxed object joined as "".
    // The single-argument form joins with an empty separator.
    if matches!(builtin.as_str(), "implode" | "join")
        && matches!(args.len(), 1 | 2)
        && implode_elements_need_prelude(&materialized_expr_type_for_merge(
            ctx,
            &args[args.len() - 1],
        ))
    {
        let call_args = if args.len() == 2 {
            args.to_vec()
        } else {
            vec![Expr::new(ExprKind::StringLiteral(String::new()), expr.span), args[0].clone()]
        };
        return Some(lower_function_call(
            ctx,
            &crate::names::Name::unqualified(crate::implode_prelude::IMPLODE_VALUES_NAME),
            &call_args,
            expr,
        ));
    }
    if builtin == "array_unique"
        && args.len() == 1
        && match materialized_expr_type_for_merge(ctx, &args[0]).codegen_repr() {
            PhpType::Array(element) => element.codegen_repr() == PhpType::Mixed,
            PhpType::AssocArray { value, .. } => {
                matches!(value.codegen_repr(), PhpType::Mixed | PhpType::Union(_))
            }
            PhpType::Mixed | PhpType::Union(_) => true,
            _ => false,
        }
    {
        return Some(lower_function_call(
            ctx,
            &crate::names::Name::unqualified(
                crate::backend_gap_prelude::ARRAY_UNIQUE_ASSOC_MIXED_NAME,
            ),
            args,
            expr,
        ));
    }
    if builtin == "array_diff" && args.len() == 2 {
        return Some(lower_function_call(
            ctx,
            &crate::names::Name::unqualified(
                crate::backend_gap_prelude::ARRAY_DIFF_STRING_NAME,
            ),
            args,
            expr,
        ));
    }
    if builtin == "array_intersect" && args.len() == 2 {
        return Some(lower_function_call(
            ctx,
            &crate::names::Name::unqualified(crate::backend_gap_prelude::ARRAY_INTERSECT_NAME),
            args,
            expr,
        ));
    }
    if builtin == "array_combine" && args.len() == 2 {
        return Some(lower_function_call(
            ctx,
            &crate::names::Name::unqualified(
                crate::backend_gap_prelude::ARRAY_COMBINE_GRADUAL_NAME,
            ),
            args,
            expr,
        ));
    }
    // Only the two-argument form: `preserve_keys` changes the result SHAPE and a gradual
    // source's keys are not known statically, so the three-argument form stays on its own path.
    //
    // `array<mixed>` is included deliberately. It is what a declared `array` parameter and an
    // untyped property both get, and it does NOT promise indexed storage -- the same reason
    // `lower_array_union` picks its gradual helper for that pair. Leaving it native ran the dense
    // indexed walk over hash storage and produced a short, malformed result. Only a NARROWER
    // element type guarantees the dense representation the native path needs.
    if builtin == "array_chunk"
        && (2..=3).contains(&args.len())
        && match materialized_expr_type_for_merge(ctx, &args[0]).codegen_repr() {
            // A DENSE source keeps its native path in the three-argument form: both arms are
            // lowerable there, and redirecting it to the prelude handed the call site a result
            // shaped like the helper's rather than its own, which printed empty chunks.
            PhpType::Array(element) => args.len() == 2 && element.codegen_repr() == PhpType::Mixed,
            PhpType::Mixed | PhpType::Union(_) | PhpType::AssocArray { .. } => true,
            _ => false,
        }
    {
        // The three-argument form takes its own helper: its chunks carry the source keys and are
        // therefore hashes, where the two-argument helper's are dense arrays.
        let helper = if args.len() == 3 {
            crate::backend_gap_prelude::ARRAY_CHUNK_GRADUAL_FLAGGED_NAME
        } else {
            crate::backend_gap_prelude::ARRAY_CHUNK_GRADUAL_NAME
        };
        return Some(lower_function_call(
            ctx,
            &crate::names::Name::unqualified(helper),
            args,
            expr,
        ));
    }
    // php's multi-array `array_map()` walks the arrays in parallel by position; the native
    // runtime maps one array, so two or more go to the prelude helper.
    if builtin == "array_map"
        && args.len() > 2
        && !args.iter().any(|arg| {
            matches!(arg.kind, ExprKind::NamedArg { .. } | ExprKind::Spread(_))
        })
    {
        return Some(lower_function_call(
            ctx,
            &crate::names::Name::unqualified(crate::array_map_prelude::ARRAY_MAP_MULTI_NAME),
            args,
            expr,
        ));
    }
    // A single-array map whose callback is read from storage and only known at run time (a
    // `?Closure` property) has no static binding for the native runtime; the prelude helper calls
    // it the way php does. `ast_usage` injected the helper for exactly these callback shapes.
    if builtin == "array_map"
        && args.len() == 2
        && matches!(
            args[0].kind,
            ExprKind::PropertyAccess { .. }
                | ExprKind::NullsafePropertyAccess { .. }
                | ExprKind::StaticPropertyAccess { .. }
                | ExprKind::ArrayAccess { .. }
        )
        && matches!(
            materialized_expr_type_for_merge(ctx, &args[0]).codegen_repr(),
            PhpType::Mixed | PhpType::Union(_)
        )
    {
        return Some(lower_function_call(
            ctx,
            &crate::names::Name::unqualified(crate::array_map_prelude::ARRAY_MAP_GRADUAL_NAME),
            args,
            expr,
        ));
    }
    // php's `array_keys($array, $filter_value[, $strict])` lists only the matching keys; the
    // native runtime lists all of them, so the search form goes to the prelude helper.
    if builtin == "array_keys"
        && args.len() > 1
        && !args.iter().any(|arg| {
            matches!(arg.kind, ExprKind::NamedArg { .. } | ExprKind::Spread(_))
        })
    {
        return Some(lower_function_call(
            ctx,
            &crate::names::Name::unqualified(crate::array_keys_prelude::ARRAY_KEYS_SEARCH_NAME),
            args,
            expr,
        ));
    }
    // `array_walk()` and `array_walk_recursive()` go to the PHP helpers WHOLESALE, not just for
    // the shapes the native runtimes refuse. The refusals and the silent divergences come from the
    // same place -- a native callback pointer, a fixed argument list the callback cannot write
    // through, and an element read as a raw integer -- and the helpers are defined in terms the
    // compiler already lowers correctly. `src/array_walk_prelude.rs` carries the measurements.
    if (builtin == "array_walk" || builtin == "array_walk_recursive")
        && (2..=3).contains(&args.len())
    {
        let has_arg = args.len() == 3;
        let mut helper_args = vec![args[0].clone(), args[1].clone()];
        helper_args.push(Expr::new(ExprKind::BoolLiteral(has_arg), expr.span));
        helper_args.push(match args.get(2) {
            Some(arg) => arg.clone(),
            None => Expr::new(ExprKind::Null, expr.span),
        });
        let helper = if builtin == "array_walk" {
            crate::array_walk_prelude::ARRAY_WALK_NAME
        } else {
            crate::array_walk_prelude::ARRAY_WALK_RECURSIVE_NAME
        };
        return Some(lower_function_call(
            ctx,
            &crate::names::Name::unqualified(helper),
            &helper_args,
            expr,
        ));
    }
    // A gradual or hash `array_rand()`: the native helper only walks a dense indexed array, and a
    // hash's key is as likely to be a string. The prelude helper collects the keys first.
    if builtin == "array_rand"
        && (1..=2).contains(&args.len())
        && !matches!(
            materialized_expr_type_for_merge(ctx, &args[0]).codegen_repr(),
            PhpType::Array(_)
        )
    {
        return Some(lower_function_call(
            ctx,
            &crate::names::Name::unqualified(
                crate::backend_gap_prelude::ARRAY_RAND_GRADUAL_NAME,
            ),
            &args[..1],
            expr,
        ));
    }
    // The three-argument `array_column()` re-keys its result from the data, which is a hash the
    // backend has no lowering for. The prelude helper answers it in PHP.
    // Object rows and list rows have no native column reader either; the same helper answers
    // the two-argument form with a null index key, which keeps php's sequential keys.
    // So do an int column key (league/commonmark's `array_column($match, 0)` over preg offset
    // pairs) and rows the native reader cannot walk (lists, or rows typed only as empty arrays),
    // which `codegen::lower_inst::builtins::arrays::column` refuses.
    if builtin == "array_column"
        && args.len() == 2
        && !args.iter().any(is_spread_arg)
        && !args.iter().any(|arg| matches!(arg.kind, ExprKind::NamedArg { .. }))
        && (crate::builtins::array_column_rows_need_generic_helper(
            &materialized_expr_type_for_merge(ctx, &args[0]),
        ) || materialized_expr_type_for_merge(ctx, &args[1]).codegen_repr() != PhpType::Str
            || !native_array_column_reads_rows(&materialized_expr_type_for_merge(ctx, &args[0])))
    {
        let mut with_index = args.to_vec();
        with_index.push(crate::parser::ast::Expr::new(
            crate::parser::ast::ExprKind::Null,
            expr.span,
        ));
        return Some(lower_function_call(
            ctx,
            &crate::names::Name::unqualified(
                crate::backend_gap_prelude::ARRAY_COLUMN_INDEXED_NAME,
            ),
            &with_index,
            expr,
        ));
    }
    if builtin == "array_column" && args.len() == 3 {
        return Some(lower_function_call(
            ctx,
            &crate::names::Name::unqualified(
                crate::backend_gap_prelude::ARRAY_COLUMN_INDEXED_NAME,
            ),
            args,
            expr,
        ));
    }
    // The native pad helper copies 8-byte slots of a list and fills with a value of the list's
    // own representation; anything else goes through `crate::array_pad_prelude`.
    if builtin == "array_pad"
        && args.len() == 3
        && !args.iter().any(is_spread_arg)
        && !crate::types::call_args::has_named_args(args)
    {
        let source = materialized_expr_type_for_merge(ctx, &args[0]).codegen_repr();
        let pad = materialized_expr_type_for_merge(ctx, &args[2]).codegen_repr();
        let native = match &source {
            PhpType::Array(element) => {
                let element = element.codegen_repr();
                element == pad
                    && (matches!(
                        element,
                        PhpType::Int | PhpType::Bool | PhpType::Float | PhpType::Callable
                    ) || element.is_refcounted())
            }
            _ => false,
        };
        if !native {
            return Some(lower_function_call(
                ctx,
                &crate::names::Name::unqualified(crate::array_pad_prelude::ARRAY_PAD_VALUES_NAME),
                args,
                expr,
            ));
        }
    }
    if builtin == "array_reverse" && args.len() == 1 {
        return Some(lower_function_call(
            ctx,
            &crate::names::Name::unqualified(
                crate::backend_gap_prelude::ARRAY_REVERSE_GRADUAL_NAME,
            ),
            args,
            expr,
        ));
    }
    // A gradual needle is routed whatever the haystack's static type: the native search compares a
    // gradual needle against nothing (`array_search($parameter, array_keys($this->parameters))` in
    // illuminate/routing was refused "array_search needle PHP type Mixed"), and a call result such
    // as `array_keys()` is typed `mixed` by the merge view even though it lowers to `array<mixed>`.
    if builtin == "array_search"
        && (2..=3).contains(&args.len())
        && !args.iter().any(is_spread_arg)
        && (matches!(
            materialized_expr_type_for_merge(ctx, &args[0]).codegen_repr(),
            PhpType::Mixed | PhpType::Union(_)
        ) || !matches!(
            materialized_expr_type_for_merge(ctx, &args[1]).codegen_repr(),
            PhpType::Mixed | PhpType::Union(_)
        ))
        && (args.len() == 3
            || matches!(
                materialized_expr_type_for_merge(ctx, &args[0]).codegen_repr(),
                PhpType::Mixed | PhpType::Union(_)
            )
            || matches!(
                materialized_expr_type_for_merge(ctx, &args[1]).codegen_repr(),
                PhpType::Array(element) if element.codegen_repr() == PhpType::Mixed
            ))
    {
        return Some(lower_function_call(
            ctx,
            &crate::names::Name::unqualified(
                crate::backend_gap_prelude::ARRAY_SEARCH_MIXED_NAME,
            ),
            args,
            expr,
        ));
    }
    if builtin == "array_splice" && !args.iter().any(is_spread_arg) {
        // A hash or a boxed receiver is spliced by the prelude walk: the native path splices a
        // dense slot array, refusing a hash at compile time and dropping a boxed one silently.
        let receiver = args.iter().enumerate().find_map(|(index, arg)| match &arg.kind {
            ExprKind::NamedArg { name, value } if php_symbol_key(name) == "array" => {
                Some(value.as_ref())
            }
            ExprKind::NamedArg { .. } => None,
            _ if index == 0 => Some(arg),
            _ => None,
        });
        // Only a plain variable: the helper binds its receiver as `mixed &$array`, and a variable
        // is the one place the checker has boxed (`widen_hash_splice_receiver`) so the result
        // can be published back. A property or element keeps the native path.
        if receiver.is_some_and(|receiver| {
            matches!(receiver.kind, ExprKind::Variable(_))
                && matches!(
                    materialized_expr_type_for_merge(ctx, receiver).codegen_repr(),
                    PhpType::AssocArray { .. } | PhpType::Mixed | PhpType::Union(_)
                )
        }) {
            return Some(lower_function_call(
                ctx,
                &crate::names::Name::unqualified(
                    crate::array_splice_prelude::ARRAY_SPLICE_GRADUAL_NAME,
                ),
                args,
                expr,
            ));
        }
        let replacement = args.iter().enumerate().find_map(|(index, arg)| match &arg.kind {
            ExprKind::NamedArg { name, value } if php_symbol_key(name) == "replacement" => {
                Some(value.as_ref())
            }
            ExprKind::NamedArg { .. } => None,
            _ if index == 3 => Some(arg),
            _ => None,
        });
        if replacement.is_some_and(|replacement| {
            matches!(
                materialized_expr_type_for_merge(ctx, replacement).codegen_repr(),
                PhpType::Mixed | PhpType::Union(_)
            )
        }) {
            return Some(lower_function_call(
                ctx,
                &crate::names::Name::unqualified(
                    crate::backend_gap_prelude::ARRAY_SPLICE_MIXED_REPLACEMENT_NAME,
                ),
                args,
                expr,
            ));
        }
    }
    if builtin == "array_unshift" {
        let receiver = args.iter().enumerate().find_map(|(index, arg)| match &arg.kind {
            ExprKind::NamedArg { name, value } if php_symbol_key(name) == "array" => {
                Some(value.as_ref())
            }
            ExprKind::NamedArg { .. } => None,
            _ if index == 0 => Some(arg),
            _ => None,
        });
        if let (Some(receiver), Some(last)) = (receiver, args.last()) {
            if let ExprKind::Spread(spread) = &last.kind {
                // A property or element receiver reaches the helper's `&$array` through the
                // by-reference place rewrite (`ref_place_args`), which reads it once and writes
                // the result back: `array_unshift($box->values, 'first', ...$extra)`.
                if matches!(
                    receiver.kind,
                    ExprKind::Variable(_)
                        | ExprKind::PropertyAccess { .. }
                        | ExprKind::StaticPropertyAccess { .. }
                        | ExprKind::ArrayAccess { .. }
                ) && args[..args.len() - 1].iter().all(|arg| !is_spread_arg(arg))
                {
                    let leading = Expr::new(
                        ExprKind::ArrayLiteral(args[1..args.len() - 1].to_vec()),
                        expr.span,
                    );
                    let helper_args = vec![receiver.clone(), leading, spread.as_ref().clone()];
                    return Some(lower_function_call(
                        ctx,
                        &crate::names::Name::unqualified(
                            crate::backend_gap_prelude::ARRAY_UNSHIFT_TRAILING_SPREAD_NAME,
                        ),
                        &helper_args,
                        expr,
                    ));
                }
            }
        }
    }
    if builtin == "pack" {
        let Some(Expr {
            kind: ExprKind::StringLiteral(format),
            ..
        }) = args.first()
        else {
            return None;
        };
        if format.is_empty()
            || !format.chars().all(|code| matches!(code, 'V' | 'N' | 'n'))
            || args.len() != format.chars().count() + 1
        {
            return None;
        }
        return Some(lower_function_call(
            ctx,
            &crate::names::Name::unqualified(crate::backend_gap_prelude::PACK_INTEGER_NAME),
            args,
            expr,
        ));
    }
    if builtin == "http_build_query" && (1..=4).contains(&args.len()) {
        return Some(lower_function_call(
            ctx,
            &crate::names::Name::unqualified(
                crate::backend_gap_prelude::HTTP_BUILD_QUERY_NAME,
            ),
            args,
            expr,
        ));
    }
    if builtin == "file_put_contents"
        && (2..=4).contains(&args.len())
        && matches!(
            materialized_expr_type_for_merge(ctx, &args[1]).codegen_repr(),
            PhpType::Array(_) | PhpType::AssocArray { .. }
        )
    {
        return Some(lower_function_call(
            ctx,
            &crate::names::Name::unqualified(
                crate::backend_gap_prelude::FILE_PUT_CONTENTS_ARRAY_NAME,
            ),
            args,
            expr,
        ));
    }
    // `uasort`/`uksort` keep the keys, which a list has no storage for: the slot permuter behind a
    // list renumbered them, and `uksort` even compared the values (`[1, 3, 2]` came out `321`
    // instead of php's `2 => 2, 1 => 3, 0 => 1`). Promote the local to a hash first, as
    // `asort` does; the hash then takes the key-preserving prelude helper below.
    if matches!(builtin.as_str(), "uasort" | "uksort") && args.len() == 2 {
        super::array_builtin_args::promote_indexed_local_to_hash(ctx, &args[0]);
    }
    let gradual_sort_operand = || {
        matches!(
            materialized_expr_type_for_merge(ctx, &args[0]).codegen_repr(),
            PhpType::Mixed | PhpType::Union(_) | PhpType::AssocArray { .. }
        ) || matches!(
            materialized_expr_type_for_merge(ctx, &args[0]).codegen_repr(),
            PhpType::Array(element)
                if matches!(
                    element.codegen_repr(),
                    PhpType::Mixed | PhpType::Union(_) | PhpType::Array(_)
                )
        )
    };
    let gradual_sort_helper = match (builtin.as_str(), args.len()) {
        ("uasort", 2) if gradual_sort_operand() => {
            Some(crate::backend_gap_prelude::UASORT_MIXED_NAME)
        }
        ("usort", 2) if gradual_sort_operand() => {
            Some(crate::backend_gap_prelude::USORT_MIXED_NAME)
        }
        ("uksort", 2) if gradual_sort_operand() => {
            Some(crate::backend_gap_prelude::UKSORT_MIXED_NAME)
        }
        ("asort", 1) if gradual_sort_operand() => {
            Some(crate::backend_gap_prelude::ASORT_MIXED_NAME)
        }
        ("natsort" | "natcasesort", 1) if gradual_sort_operand() => {
            let call = crate::natsort_gradual_prelude::uasort_call(
                &args[0],
                builtin == "natcasesort",
                expr.span,
            );
            return Some(lower_expr(ctx, &call));
        }
        _ => None,
    };
    if let Some(helper) = gradual_sort_helper {
        let wrapped;
        let args = match runtime_named_comparator_wrapper(ctx, args) {
            Some(with_closure) => {
                wrapped = with_closure;
                &wrapped[..]
            }
            None => args,
        };
        return Some(lower_function_call(
            ctx,
            &crate::names::Name::unqualified(helper),
            args,
            expr,
        ));
    }
    // `htmlspecialchars()`'s fourth argument, `$double_encode`: the native helper always
    // double-encodes, which is php's answer for `true`, so only a flag that may be false goes to
    // the prelude loop that keeps valid character references. Laravel's `e()` passes it on every
    // Blade `{{ }}`, and the four-operand call stopped EIR lowering.
    if builtin == "htmlspecialchars"
        && args.len() == 4
        && !crate::types::call_args::has_named_args(args)
        && !args.iter().any(is_spread_arg)
        && !matches!(args[3].kind, ExprKind::BoolLiteral(true))
    {
        return Some(lower_function_call(
            ctx,
            &crate::names::Name::unqualified(
                crate::htmlspecialchars_prelude::HTMLSPECIALCHARS_DOUBLE_ENCODE_NAME,
            ),
            args,
            expr,
        ));
    }
    // `str_replace()` over a gradual search or replace: see `crate::str_replace_count_prelude`.
    if builtin == "str_replace"
        && args.len() == 3
        && !crate::types::call_args::has_named_args(args)
        && !args.iter().any(is_spread_arg)
        && args[..2].iter().any(|arg| {
            match materialized_expr_type_for_merge(ctx, arg).codegen_repr() {
                PhpType::Mixed | PhpType::Union(_) | PhpType::AssocArray { .. } => true,
                PhpType::Array(element) => element.codegen_repr() != PhpType::Str,
                _ => false,
            }
        })
    {
        return Some(lower_function_call(
            ctx,
            &crate::names::Name::unqualified(
                crate::str_replace_count_prelude::STR_REPLACE_COUNT_NAME,
            ),
            args,
            expr,
        ));
    }
    // `fputcsv()` with a field list the native writer cannot render -- anything but an array of
    // strings: ints, floats, bools and null fields, or a gradual array -- takes the same prelude
    // as the long forms, which converts each field as php does.
    if builtin == "fputcsv"
        && (2..=4).contains(&args.len())
        && !crate::types::call_args::has_named_args(args)
        && !args.iter().any(is_spread_arg)
        && !matches!(
            materialized_expr_type_for_merge(ctx, &args[1]).codegen_repr(),
            PhpType::Array(element) if element.codegen_repr() == PhpType::Str
        )
        && !ctx
            .builder
            .function()
            .name
            .eq_ignore_ascii_case(crate::csv_prelude::FPUTCSV_FULL_NAME)
    {
        return Some(lower_function_call(
            ctx,
            &crate::names::Name::unqualified(crate::csv_prelude::FPUTCSV_FULL_NAME),
            args,
            expr,
        ));
    }
    // `stream_select()` goes to `crate::stream_select_prelude`; the helper's own call of the native
    // select (over flattened descriptor lists) must stay native or it would recurse forever.
    let inside_stream_select = ctx
        .builder
        .function()
        .name
        .eq_ignore_ascii_case(crate::stream_select_prelude::STREAM_SELECT_NAME);
    // The closed-world autoload chain: `crate::backend_gap_prelude::AUTOLOAD_CHAIN_SRC` holds the
    // loaders instead of the interpreter. Declared exactly when the program names the chain.
    if crate::closed_world::is_enabled()
        && !crate::types::call_args::has_named_args(args)
        && !args.iter().any(is_spread_arg)
        && ctx.module_declares_function(crate::backend_gap_prelude::AUTOLOAD_CALL_NAME)
    {
        let chain_helper = match (builtin.as_str(), args.len()) {
            ("spl_autoload_register", 0..=3) => Some(crate::backend_gap_prelude::AUTOLOAD_REGISTER_NAME),
            ("spl_autoload_unregister", 1) => Some(crate::backend_gap_prelude::AUTOLOAD_UNREGISTER_NAME),
            ("spl_autoload_functions", 0) => Some(crate::backend_gap_prelude::AUTOLOAD_FUNCTIONS_NAME),
            ("spl_autoload_call", 1) => Some(crate::backend_gap_prelude::AUTOLOAD_CALL_NAME),
            _ => None,
        };
        if let Some(helper) = chain_helper {
            return Some(lower_function_call(
                ctx,
                &crate::names::Name::unqualified(helper),
                args,
                expr,
            ));
        }
        // `class_exists($name)` and its siblings load through the chain on a miss: ask natively
        // first (compiled classes and the probe-only ones answer there), then run the loaders
        // and ask again. The helper's own probes must stay native or it would recurse.
        let kind = match builtin.as_str() {
            "class_exists" => Some(0),
            "interface_exists" => Some(1),
            "trait_exists" => Some(2),
            "enum_exists" => Some(3),
            _ => None,
        };
        let inside_chain = ctx
            .builder
            .function()
            .name
            .to_ascii_lowercase()
            .starts_with("__elephc_autoload");
        if let Some(kind) = kind {
            let simple_name = args.first().is_some_and(|name| {
                matches!(
                    name.kind,
                    ExprKind::Variable(_) | ExprKind::StringLiteral(_) | ExprKind::ClassConstant { .. }
                )
            });
            let autoloads = match args.len() {
                1 => true,
                2 => matches!(args[1].kind, ExprKind::BoolLiteral(true)),
                _ => false,
            };
            if !inside_chain && !ctx.autoload_hook_active() && simple_name && autoloads {
                let retry = Expr::new(
                    ExprKind::FunctionCall {
                        name: crate::names::Name::unqualified(
                            crate::backend_gap_prelude::AUTOLOAD_RETRY_NAME,
                        ),
                        args: vec![
                            args[0].clone(),
                            Expr::new(ExprKind::IntLiteral(kind), expr.span),
                        ],
                    },
                    expr.span,
                );
                let either = Expr::new(
                    ExprKind::BinaryOp {
                        left: Box::new(expr.clone()),
                        op: crate::parser::ast::BinOp::Or,
                        right: Box::new(retry),
                    },
                    expr.span,
                );
                let previous = ctx.set_autoload_hook_active(true);
                let value = lower_expr(ctx, &either);
                ctx.set_autoload_hook_active(previous);
                return Some(value);
            }
        }
    }
    let helper = match (builtin.as_str(), args.len()) {
        ("stream_select", 4..=5) if !inside_stream_select => {
            crate::stream_select_prelude::STREAM_SELECT_NAME
        }
        ("levenshtein", 2) => crate::backend_gap_prelude::LEVENSHTEIN_TWO_ARG_NAME,
        ("random_bytes", 1) => crate::backend_gap_prelude::RANDOM_BYTES_NAME,
        ("str_replace", 4) => crate::str_replace_count_prelude::STR_REPLACE_COUNT_NAME,
        ("exec", 1..=3) => crate::exec_prelude::EXEC_FULL_NAME,
        ("passthru", 2) => crate::exec_prelude::PASSTHRU_FULL_NAME,
        ("fputcsv", 5..=6) => crate::csv_prelude::FPUTCSV_FULL_NAME,
        ("fgetcsv", 4..=5) => crate::csv_prelude::FGETCSV_FULL_NAME,
        ("escapeshellarg", 1) => crate::backend_gap_prelude::ESCAPESHELLARG_NAME,
        ("str_getcsv", 1..=4) => crate::backend_gap_prelude::STR_GETCSV_NAME,
        ("cli_set_process_title" | "setproctitle", 1) => {
            crate::backend_gap_prelude::CLI_SET_PROCESS_TITLE_NAME
        }
        _ => match crate::stream_args_prelude::helper_for(builtin.as_str(), args.len()) {
            Some(helper) => helper,
            None => return None,
        },
    };
    Some(lower_function_call(
        ctx,
        &crate::names::Name::unqualified(helper),
        args,
        expr,
    ))
}
/// Folds a variadic `array_replace()` into the nested two-argument form it is defined as.
///
/// php's `array_replace($a, $b, $c)` is `array_replace(array_replace($a, $b), $c)`: each array
/// overwrites the keys of the ones before it, left to right. Every lowering below takes exactly
/// two hash operands, so the fold reuses them all unchanged and needs no new runtime helper and
/// no new prelude -- it is a pure AST rewrite of the call site.
///
/// Symfony's `UrlGenerator` writes the three-argument form
/// (`array_replace($defaults, $this->context->getParameters(), $parameters)`), and refusing it
/// with `array_replace() takes exactly 2 arguments` kept the whole routing component out of the
/// closed world -- which is what made the runtime autoloader re-interpret `UrlMatcher` and
/// `CompiledUrlMatcher` on every request.
pub(super) fn lower_variadic_array_replace(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    let key = php_symbol_key(name.trim_start_matches('\\'));
    if key != "array_replace" && key != "array_replace_recursive" {
        return None;
    }
    if args.len() == 2 || args.is_empty() || crate::types::call_args::has_named_args(args) {
        return None;
    }
    if args.iter().any(is_spread_arg) {
        return None;
    }
    // One array replaces nothing: php returns it as it is. The two-operand backend helpers have
    // no one-operand form, so the call failed to build ("array_replace expected 2 args, got 1").
    if args.len() == 1 {
        return Some(lower_expr(ctx, &args[0]));
    }
    let callee = crate::names::Name::unqualified(name.trim_start_matches('\\'));
    let mut folded = args[0].clone();
    for next in &args[1..] {
        folded = Expr::new(
            ExprKind::FunctionCall {
                name: callee.clone(),
                args: vec![folded, next.clone()],
            },
            expr.span,
        );
    }
    Some(lower_expr(ctx, &folded))
}

/// Routes `array_merge(...$arrays)` to the elephc-PHP prelude for gradual or keyed operands.
///
/// The six `__rt_array_merge*` helpers copy 8- or 16-byte slots out of INDEXED arrays; none
/// walks a hash. A boxed `mixed` operand therefore has no native path, and the gradual trick used
/// for `array_chunk` — rewriting through `array_values` — is not available here: `array_merge`
/// renumbers integer keys but PRESERVES string keys, so `array_values` would silently drop them.
/// `crate::array_merge_prelude` writes PHP's rule in PHP instead, which needs no new runtime helper.
///
/// Gradual values require a runtime shape check, while statically associative arrays require PHP's
/// string-key preservation and integer-key renumbering. Packed arrays keep the native slot-copy
/// lowering. Any other shape keeps its loud refusal rather than silently changing semantics.
pub(super) fn lower_gradual_array_merge(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    if php_symbol_key(name.trim_start_matches('\\')) != "array_merge" {
        return None;
    }
    if crate::types::call_args::has_named_args(args) {
        return None;
    }
    let has_spread = args.iter().any(is_spread_arg);
    if !has_spread
        && !args
            .iter()
            .any(|arg| array_merge_operand_requires_prelude(ctx, arg))
    {
        return None;
    }
    if has_spread {
        if args.iter().filter(|arg| is_spread_arg(arg)).count() != 1 {
            return None;
        }
        let ExprKind::Spread(rest) = &args.last()?.kind else {
            return None;
        };
        let (helper, helper_args) = match args.len() {
            1 => (
                crate::array_merge_prelude::GRADUAL_ARRAY_MERGE_ONLY_SPREAD_NAME,
                vec![rest.as_ref().clone()],
            ),
            2 => (
                crate::array_merge_prelude::GRADUAL_ARRAY_MERGE_SPREAD_NAME,
                vec![args[0].clone(), rest.as_ref().clone()],
            ),
            _ => return None,
        };
        return Some(lower_function_call(
            ctx,
            &crate::names::Name::unqualified(helper),
            &helper_args,
            expr,
        ));
    }
    let helper = match args.len() {
        2 => crate::array_merge_prelude::GRADUAL_ARRAY_MERGE_NAME,
        3 => crate::array_merge_prelude::GRADUAL_ARRAY_MERGE_THREE_NAME,
        5 => crate::array_merge_prelude::GRADUAL_ARRAY_MERGE_FIVE_NAME,
        6 => crate::array_merge_prelude::GRADUAL_ARRAY_MERGE_SIX_NAME,
        // Four operands (illuminate/queue's `array_merge([get_class($q)], class_parents($q) ?: [],
        // class_implements($q) ?: [], class_uses_recursive($q))`) and seven or more have no
        // fixed-arity wrapper. `array_merge` is associative -- integer keys renumber in order and
        // string keys keep the last value either way -- so the call folds left through the
        // two-operand helper, evaluating every operand once and in source order.
        count if count >= 4 => {
            let pair = crate::names::Name::unqualified(
                crate::array_merge_prelude::GRADUAL_ARRAY_MERGE_NAME,
            );
            let mut folded = Expr::new(
                ExprKind::FunctionCall {
                    name: pair.clone(),
                    args: vec![args[0].clone(), args[1].clone()],
                },
                expr.span,
            );
            for next in &args[2..] {
                folded = Expr::new(
                    ExprKind::FunctionCall {
                        name: pair.clone(),
                        args: vec![folded, next.clone()],
                    },
                    expr.span,
                );
            }
            return Some(lower_expr(ctx, &folded));
        }
        _ => return None,
    };
    Some(lower_function_call(
        ctx,
        &crate::names::Name::unqualified(helper),
        args,
        expr,
    ))
}

/// Rewrites a sort's comparator held in a string variable (`usort($a, $name)`) into
/// `fn ($l, $r) => $name($l, $r)` for the prelude sort helpers.
///
/// Their `callable $callback` parameter expects a callable descriptor, and the synthesized call
/// skips the checker that refuses a runtime string there for user functions: the string went in
/// as is and the helper's `$callback(...)` jumped into the string's bytes (SIGBUS). A variable
/// call on the string is supported, so the closure resolves the name at runtime exactly as
/// `$name($l, $r)` would. Returns `None` when the comparator is not a string variable.
fn runtime_named_comparator_wrapper(
    ctx: &LoweringContext<'_, '_>,
    args: &[Expr],
) -> Option<Vec<Expr>> {
    use crate::synthetic_class::{closure, e_var, s_return};
    let callback = args.get(1)?;
    let ExprKind::Variable(name) = &callback.kind else {
        return None;
    };
    if ctx.local_type(name).codegen_repr() != PhpType::Str {
        return None;
    }
    let call = Expr::new(
        ExprKind::ExprCall {
            callee: Box::new(e_var(name)),
            args: vec![e_var("__elephc_left"), e_var("__elephc_right")],
        },
        callback.span,
    );
    let mut comparator = closure()
        .param_untyped("__elephc_left")
        .param_untyped("__elephc_right")
        .captures(name)
        .body(vec![s_return(call)])
        .build();
    comparator.span = callback.span;
    let mut rewritten = args.to_vec();
    rewritten[1] = comparator;
    Some(rewritten)
}

/// Returns whether a `count($value, $mode)` receiver can hold nested arrays the native total
/// cannot see: a gradual value, or a collection whose element is a container, a gradual value or
/// an object.
fn count_receiver_can_nest(receiver: &PhpType) -> bool {
    let element = match receiver {
        PhpType::Mixed | PhpType::Union(_) => return true,
        PhpType::Array(element) => element,
        PhpType::AssocArray { value, .. } => value,
        _ => return false,
    };
    matches!(
        element.codegen_repr(),
        PhpType::Array(_)
            | PhpType::AssocArray { .. }
            | PhpType::Mixed
            | PhpType::Union(_)
            | PhpType::Iterable
            | PhpType::Object(_)
    )
}

/// Returns whether the native `array_column()` reader walks rows of this collection type.
///
/// It reads associative rows, or boxed ones it checks at run time, from an indexed or associative
/// outer collection, or a boxed collection; this is
/// `codegen::lower_inst::builtins::arrays::column::array_column_source_value_type`'s acceptance.
fn native_array_column_reads_rows(array_ty: &PhpType) -> bool {
    let row = match array_ty.codegen_repr() {
        PhpType::Mixed | PhpType::Union(_) => return true,
        PhpType::Array(row) => row,
        PhpType::AssocArray { value, .. } => value,
        _ => return true,
    };
    matches!(
        row.codegen_repr(),
        PhpType::AssocArray { .. } | PhpType::Mixed | PhpType::Union(_)
    )
}

/// Returns whether `implode()`'s array operand holds elements only the prelude joins correctly.
///
/// The native renderers take string, int and bool slots, and boxed ones (objects included, through
/// `__toString()`). An unboxed object, float or nested-array slot has no renderer.
fn implode_elements_need_prelude(array_ty: &PhpType) -> bool {
    let element = match array_ty {
        PhpType::Array(element) => element,
        PhpType::AssocArray { value, .. } => value,
        _ => return false,
    };
    !matches!(
        element.codegen_repr(),
        PhpType::Bool
            | PhpType::Int
            | PhpType::Str
            | PhpType::Mixed
            | PhpType::Union(_)
            | PhpType::Never
            | PhpType::Void
    )
}

/// Returns whether an `array_merge` argument needs key-aware PHP-level iteration.
fn array_merge_operand_requires_prelude(ctx: &LoweringContext<'_, '_>, expr: &Expr) -> bool {
    match materialized_expr_type_for_merge(ctx, expr).codegen_repr() {
        PhpType::Mixed | PhpType::Union(_) | PhpType::AssocArray { .. } => true,
        PhpType::Array(element) => element.codegen_repr() == PhpType::Mixed,
        _ => false,
    }
}
