//! Purpose:
//! Builds `stream_select()` -- by-reference `$read`, `$write` and `$except` arrays of any shape --
//! as a prelude declaration over the native descriptor-list select.
//!
//! Called from:
//! - `crate::backend_gap_prelude::inject_if_used`, which prepends the declaration when the
//!   program calls `stream_select()`.
//! - `crate::ir_lower::expr::compat_preludes`, which routes every such call to it.
//!
//! Key details:
//! - The native `__rt_stream_select` reads each argument as a non-null PACKED array of
//!   descriptors and compacts it in place. A `null` `$write`/`$except` (the common shape) read
//!   address zero, and a hash -- `$r = $pipes; unset($r[0]);` leaves keys 1 and 2, which is what
//!   Symfony's `UnixPipes::readAndWrite()` passes -- or a boxed `mixed` array was misread.
//! - Each array is flattened into three fresh lists (descriptors handed to the native select,
//!   the original values, the original keys). The native call compacts only its own list, so a
//!   caller's array that shares storage with another variable is never mutated in place.
//! - The native compaction keeps the ready descriptors in their original order, so walking the
//!   original values against the compacted list recovers exactly which entries are ready; each
//!   one is stored back under its ORIGINAL KEY, as php does. A `null` array is skipped and stays
//!   `null`.
//! - php answers `false` when the call fails; the native helper answers a negative count.
//! - Built as AST, never parsed from PHP source.

use crate::parser::ast::{BinOp, Program, Stmt};
use crate::synthetic_class::{
    e_array, e_binop, e_bool, e_call, e_index, e_int, e_null, e_post_inc, e_ternary, e_var,
    function, php_semantics_declarations, s_array_assign, s_array_push, s_assign, s_expr,
    s_foreach, s_if, s_return, s_while,
};

/// Reserved helper every `stream_select()` call is routed to.
pub(crate) const STREAM_SELECT_NAME: &str = "__elephc_stream_select";

/// Returns the helper declaration for injection.
pub(crate) fn declarations() -> Program {
    php_semantics_declarations(|| vec![decl_fn_stream_select()])
}

/// Builds `__elephc_stream_select(&$read, &$write, &$except, $seconds, $microseconds = 0)`.
fn decl_fn_stream_select() -> Stmt {
    let sets = ["read", "write", "except"];
    let mut body = Vec::new();
    for set in sets {
        body.extend(flatten(set));
    }
    body.push(s_assign(
        "count",
        e_call(
            "stream_select",
            vec![
                e_var("read_fds"),
                e_var("write_fds"),
                e_var("except_fds"),
                e_var("seconds"),
                e_var("microseconds"),
            ],
        ),
    ));
    for set in sets {
        body.push(restore(set));
    }
    body.push(s_return(e_ternary(
        e_binop(e_var("count"), BinOp::Lt, e_int(0)),
        e_bool(false),
        e_var("count"),
    )));
    function(STREAM_SELECT_NAME)
        .param_by_ref("read", None)
        .param_by_ref("write", None)
        .param_by_ref("except", None)
        .param_untyped("seconds")
        .param_untyped_default("microseconds", e_int(0))
        .body(body)
        .build()
}

/// Flattens `$set` into `$set_fds` (handed to the native select), `$set_vals` and `$set_keys`.
fn flatten(set: &str) -> Vec<Stmt> {
    let fds = format!("{set}_fds");
    let vals = format!("{set}_vals");
    let keys = format!("{set}_keys");
    // Loop variables are per set: one name shared by sets of different element types widens its
    // slot to a boxed mixed, and the box the first loop left behind is never released.
    let entry_key = format!("{set}_entry_key");
    let entry_val = format!("{set}_entry_val");
    vec![
        s_assign(&fds, e_array(vec![])),
        s_assign(&vals, e_array(vec![])),
        s_assign(&keys, e_array(vec![])),
        s_if(
            e_binop(e_var(set), BinOp::StrictNotEq, e_null()),
            vec![s_foreach(
                e_var(set),
                Some(&entry_key),
                &entry_val,
                vec![
                    s_array_push(&fds, e_var(&entry_val)),
                    s_array_push(&vals, e_var(&entry_val)),
                    s_array_push(&keys, e_var(&entry_key)),
                ],
            )],
            vec![],
            None,
        ),
    ]
}

/// Rebuilds `$set` from the entries whose descriptor survived the native select, original keys kept.
fn restore(set: &str) -> Stmt {
    let fds = format!("{set}_fds");
    let vals = format!("{set}_vals");
    let keys = format!("{set}_keys");
    let ready = format!("{set}_ready");
    let total = format!("{set}_total");
    let out = format!("{set}_out");
    // The index is per set too: a counter shared by the three loops leaves a boxed temporary
    // behind (heap-debug reports one 48-byte block) when two of them run in one call.
    let index = format!("{set}_index");
    s_if(
        e_binop(e_var(set), BinOp::StrictNotEq, e_null()),
        vec![
            s_assign(&out, e_array(vec![])),
            s_assign(&ready, e_int(0)),
            s_assign(&index, e_int(0)),
            s_assign(&total, e_call("count", vec![e_var(&vals)])),
            s_while(
                e_binop(e_var(&index), BinOp::Lt, e_var(&total)),
                vec![
                    s_if(
                        e_binop(
                            e_binop(
                                e_var(&ready),
                                BinOp::Lt,
                                e_call("count", vec![e_var(&fds)]),
                            ),
                            BinOp::And,
                            e_binop(
                                e_index(e_var(&fds), e_var(&ready)),
                                BinOp::StrictEq,
                                e_index(e_var(&vals), e_var(&index)),
                            ),
                        ),
                        vec![
                            s_array_assign(
                                &out,
                                e_index(e_var(&keys), e_var(&index)),
                                e_index(e_var(&vals), e_var(&index)),
                            ),
                            s_expr(e_post_inc(&ready)),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_post_inc(&index)),
                ],
            ),
            s_assign(set, e_var(&out)),
        ],
        vec![],
        None,
    )
}
