//! Purpose:
//! Reads fileinfo wrapper inputs whose names are known only at runtime.
//!
//! Called from:
//! - The private finfo stream reader, including recursive php://filter resources.
//!
//! Key details:
//! - Known filter names use literal builtin calls, sharing existing EIR filter semantics.
//! - Each filter stage owns a temporary stream and closes it even when a handler throws.
//! - Standard descriptors are borrowed and never closed by a fileinfo query.
//! - Prepared filter references keep an explicit mixed element contract across recursive calls.

use crate::parser::ast::{BinOp, CastType, Expr, Stmt, TypeExpr};
use crate::synthetic_class::*;

/// Compares two PHP values without coercion.
fn eq(left: Expr, right: Expr) -> Expr { e_binop(left, BinOp::StrictEq, right) }

/// Tests a wrapper prefix without changing its filename bytes.
fn prefix(value: &str) -> Expr {
    e_call("str_starts_with", vec![e_call("strtolower", vec![e_var("filename")]), e_str(value)])
}

/// Reads a wrapper recursively before applying its read-direction filters.
fn filtered_url() -> Stmt {
    s_if(prefix("php://filter/"), vec![
        s_assign("_split", e_call("strpos", vec![e_var("filename"), e_str("/resource=")])),
        s_if(eq(e_var("_split"), e_bool(false)), vec![s_throw(e_new_fq("Error", vec![e_str("No URL resource specified")]))], vec![], None),
        s_assign("_resource", e_call("substr", vec![e_var("filename"), e_binop(e_var("_split"), BinOp::Add, e_int(10))])),
        s_if(eq(e_var("_resource"), e_str("")), super::streams::failure(e_str("operation failed")), vec![], None),
        s_assign("_data", e_method_call(e_this(), "__elephc_read_stream", vec![e_var("_resource"), e_var("context"), e_var("_function"), e_bool(false), e_var("_directory"), e_var("_pending"), e_var("_stages")])),
        s_if(eq(e_var("_data"), e_bool(false)), super::streams::failure(e_str("operation failed")), vec![], None),
        s_assign("_bytes", e_cast(CastType::String, e_var("_data"))),
        s_expr(e_call("unset", vec![e_var("_data")])),
        // PHP's filter wrapper replaces the network wrapper's failing stat with socket stat.
        s_if(eq(e_var("_pending"), e_int(-2)), vec![s_assign("_directory", e_bool(true))], vec![], None),
        s_assign("_spec", e_str("")),
        s_if(e_binop(e_var("_split"), BinOp::Gt, e_int(13)), vec![
            s_assign("_spec", e_call("substr", vec![e_var("filename"), e_int(13), e_binop(e_var("_split"), BinOp::Sub, e_int(13))])),
        ], vec![], None),
        // PHP tokenizes beyond the delimiter's NUL when /resource= directly follows filter.
        s_if(eq(e_var("_split"), e_int(12)), vec![s_assign("_spec", e_call("substr", vec![e_var("filename"), e_int(13)]))], vec![], None),
        s_foreach(e_call("explode", vec![e_str("/"), e_var("_spec")]), None, "_part", vec![
            s_assign("_write", e_call("str_starts_with", vec![e_call("strtolower", vec![e_var("_part")]), e_str("write=")])),
            s_if(e_var("_write"), vec![s_assign("_part", e_call("substr", vec![e_var("_part"), e_int(6)]))], vec![], None),
            s_if(e_binop(e_not(e_var("_write")), BinOp::And, e_call("str_starts_with", vec![e_call("strtolower", vec![e_var("_part")]), e_str("read=")])), vec![
                s_assign("_part", e_call("substr", vec![e_var("_part"), e_int(5)])),
            ], vec![], None),
            s_foreach(e_call("explode", vec![e_str("|"), e_var("_part")]), None, "_filter", vec![
                s_if(eq(e_var("_filter"), e_str("")), vec![s_continue(1)], vec![], None),
                s_assign("_filter", e_index(e_call("explode", vec![e_str("\0"), e_call("urldecode", vec![e_var("_filter")])]), e_int(0))),
                s_if(e_binop(e_var("_write"), BinOp::Or, e_binop(e_var("_pending"), BinOp::StrictNotEq, e_null())), vec![
                    s_assign("_prepared", e_method_call(e_this(), "__elephc_prepare_filter", vec![e_var("_filter"), e_var("_function"), e_var("_write")])),
                    s_if(eq(e_var("_prepared"), e_bool(false)), vec![s_return(e_bool(false))], vec![], None),
                    s_expr(e_call("array_push", vec![e_var("_stages"), e_var("_prepared")])),
                    s_continue(1),
                ], vec![], None),
                // Keep the fallible result boxed while the loop's byte accumulator stays string-typed.
                s_assign("_stage", e_array(vec![
                    e_method_call(e_this(), "__elephc_filter_bytes", vec![e_var("_bytes"), e_var("_filter"), e_var("_function"), e_var("_directory")]), e_bool(true),
                ])),
                s_if(eq(e_index(e_var("_stage"), e_int(0)), e_bool(false)), vec![s_return(e_bool(false))], vec![], None),
                s_assign("_bytes", e_cast(CastType::String, e_index(e_var("_stage"), e_int(0)))),
            ]),
        ]),
        s_return(e_var("_bytes")),
    ], vec![], None)
}

/// Guards the archive component of a `phar://` URL before the generic stream reader opens it.
///
/// Phar determines its archive by the first `.phar`/`.tar`/`.zip` suffix in the URL. The guard
/// confines that component with the same canonical-path handoff as `zip://`; a URL without a
/// recognizable suffix falls through to the wrapper's own diagnostic.
fn phar_guard(open_basedir: Option<&str>) -> Option<Stmt> {
    let roots = open_basedir?;
    Some(s_if(prefix("phar://"), vec![
        s_assign("_phar_rest", e_call("substr", vec![e_var("filename"), e_int(7)])),
        s_assign("_phar_lower", e_call("strtolower", vec![e_var("_phar_rest")])),
        s_assign("_phar_end", e_bool(false)),
        s_foreach(
            e_array(vec![e_str(".phar"), e_str(".tar"), e_str(".zip")]),
            None,
            "_phar_ext",
            vec![
                s_assign(
                    "_phar_pos",
                    e_call("strpos", vec![e_var("_phar_lower"), e_var("_phar_ext")]),
                ),
                s_if(
                    e_binop(
                        e_binop(e_var("_phar_pos"), BinOp::StrictNotEq, e_bool(false)),
                        BinOp::And,
                        e_binop(
                            e_binop(e_var("_phar_end"), BinOp::StrictEq, e_bool(false)),
                            BinOp::Or,
                            e_binop(e_var("_phar_pos"), BinOp::Lt, e_var("_phar_end")),
                        ),
                    ),
                    vec![s_assign(
                        "_phar_end",
                        e_binop(
                            e_var("_phar_pos"),
                            BinOp::Add,
                            e_call("strlen", vec![e_var("_phar_ext")]),
                        ),
                    )],
                    vec![],
                    None,
                ),
            ],
        ),
        s_if(
            e_binop(e_var("_phar_end"), BinOp::StrictNotEq, e_bool(false)),
            vec![
                s_assign(
                    "_phar_archive",
                    e_call("substr", vec![e_var("_phar_rest"), e_int(0), e_var("_phar_end")]),
                ),
                basedir_guard("_phar_archive", roots, Some("_report_errors")),
            ],
            vec![],
            None,
        ),
    ], vec![], None))
}

/// Builds the runtime reader used by both object and procedural fileinfo calls.
pub(super) fn read_method(open_basedir: Option<&str>) -> MethodBuilder {
    let mut body = vec![super::streams::data_uri(), super::streams::file_uri(),
        super::streams::network_uri(), filtered_url()];
    // PHP's `zip://archive#entry` is a read-only view over the same zip parser the
    // `phar://` wrapper already reaches. Translate it, and preserve the directory
    // entry's trailing slash so identification answers `directory`.
    let mut zip_body = vec![
        s_assign("_zip_rest", e_call("substr", vec![e_var("filename"), e_int(6)])),
        s_assign("_zip_split", e_call("strpos", vec![e_var("_zip_rest"), e_str("#")])),
        s_if(eq(e_var("_zip_split"), e_bool(false)), super::streams::failure(e_str("Invalid zip URL specified")), vec![], None),
        s_assign("_zip_archive", e_call("substr", vec![e_var("_zip_rest"), e_int(0), e_var("_zip_split")])),
        s_assign("_zip_entry", e_call("substr", vec![e_var("_zip_rest"), e_binop(e_var("_zip_split"), BinOp::Add, e_int(1))])),
    ];
    // PHP's phar wrapper applies the configured open_basedir to the archive path too.
    if let Some(roots) = open_basedir {
        zip_body.push(basedir_guard("_zip_archive", roots, Some("_report_errors")));
    }
    zip_body.extend([
        // Keep the entry inside the archive's member namespace. Only a `..` SEGMENT escapes
        // it: `a..b.txt` is an ordinary member name, while `../x`, `a/../x`, `..` and `a/..`
        // are not.
        s_if(e_binop(e_binop(eq(e_var("_zip_entry"), e_str("")), BinOp::Or,
            e_binop(
                e_binop(eq(e_var("_zip_entry"), e_str("..")), BinOp::Or,
                    e_call("str_starts_with", vec![e_var("_zip_entry"), e_str("../")])),
                BinOp::Or,
                e_binop(e_call("str_ends_with", vec![e_var("_zip_entry"), e_str("/..")]), BinOp::Or,
                    e_call("str_contains", vec![e_var("_zip_entry"), e_str("/../")])),
            )), BinOp::Or,
            e_call("str_starts_with", vec![e_var("_zip_entry"), e_str("/")])),
            super::streams::failure(e_str("Invalid zip entry")), vec![], None),
        s_if(e_call("str_ends_with", vec![e_var("_zip_entry"), e_str("/")]), vec![
            // Only claim a directory after the archive/entry actually opens. This keeps a
            // hostile `../` or nonexistent entry from being classified as a directory.
            s_assign("_zip_probe", e_call("fopen", vec![
                e_binop(e_binop(e_str("phar://"), BinOp::Concat, e_var("_zip_archive")),
                    BinOp::Concat, e_binop(e_str("/"), BinOp::Concat, e_var("_zip_entry"))),
                e_str("rb"),
            ])),
            s_if(eq(e_var("_zip_probe"), e_bool(false)), super::streams::failure(e_str("operation failed")), vec![], None),
            s_expr(e_call("fclose", vec![e_var("_zip_probe")])),
            s_assign("_directory", e_bool(true)),
            s_assign("_pending", e_int(-2)),
            s_return(e_str("")),
        ], vec![], None),
        s_assign("filename", e_binop(
            e_binop(e_str("phar://"), BinOp::Concat, e_var("_zip_archive")),
            BinOp::Concat,
            e_binop(e_str("/"), BinOp::Concat, e_var("_zip_entry")),
        )),
    ]);
    body.push(s_if(prefix("zip://"), zip_body, vec![], None));
    if let Some(guard) = phar_guard(open_basedir) {
        body.push(guard);
    }
    for scheme in ["compress.zlib://", "compress.bzip2://"] {
        body.push(s_if(prefix(scheme), vec![
            // PHP opens the wrapper but cannot identify it when stream stat is unavailable.
            s_assign("_stream", e_call("fopen", vec![
                e_call("substr", vec![e_var("filename"), e_int(scheme.len() as i64)]), e_str("rb"), e_bool(false), e_var("context")])),
            s_if(eq(e_var("_stream"), e_bool(false)), vec![s_return(e_bool(false))], vec![], None),
            s_expr(e_call("fclose", vec![e_var("_stream")])),
            s_assign("_pending", e_bool(false)), s_return(e_str("")),
        ], vec![], None));
    }
    for url in ["php://input", "php://output"] {
        body.push(s_if(eq(e_call("strtolower", vec![e_var("filename")]), e_str(url)),
            vec![s_assign("_pending", e_bool(false)), s_return(e_str(""))], vec![], None));
    }
    for (url, resource) in [("php://stdin", "STDIN"), ("php://stdout", "STDOUT"), ("php://stderr", "STDERR")] {
        body.push(s_if(eq(e_call("strtolower", vec![e_var("filename")]), e_str(url)), vec![
            s_return(e_call("stream_get_contents", vec![e_const(resource), e_int(7340032)])),
        ], vec![], None));
    }
    body.push(s_if(e_binop(eq(e_call("strtolower", vec![e_var("filename")]), e_str("php://memory")), BinOp::Or,
        e_binop(eq(e_call("strtolower", vec![e_var("filename")]), e_str("php://temp")), BinOp::Or, prefix("php://temp/"))),
        vec![s_return(e_str(""))], vec![], None));
    body.push(s_if(prefix("php://"), vec![
        super::build::warning(e_binop(e_var("_function"), BinOp::Concat, e_str("(): Invalid php:// URL specified"))),
    ].into_iter().chain(super::streams::failure(e_str("operation failed"))).collect(), vec![], None));
    body.push(s_if(prefix("glob://"), super::streams::failure(e_str("wrapper does not support stream open")), vec![], None));
    body.push(s_if(e_not(e_method_call(e_this(), "__elephc_is_stream", vec![e_var("filename"), e_var("_function"), e_int(1)])), local_bytes(open_basedir), vec![], None));
    body.extend([
        s_assign("_stream", e_call("fopen", vec![e_var("filename"), e_str("rb"), e_bool(false), e_var("context")])),
        s_if(eq(e_var("_stream"), e_bool(false)), vec![s_return(e_bool(false))], vec![], None),
        s_assign("_pending", e_var("_stream")), s_return(e_str("")),
    ]);
    method("__elephc_read_stream").private().param("filename", TypeExpr::Str)
        .param("context", t_mixed()).param("_function", TypeExpr::Str).param("_report_errors", TypeExpr::Bool)
        .param_by_ref("_directory", Some(TypeExpr::Bool)).param_by_ref("_pending", None).param_by_ref("_stages", Some(TypeExpr::Array(Box::new(t_mixed()))))
        .returns(t_union(vec![TypeExpr::Str, TypeExpr::False])).body(body)
}

/// Routes registered wrappers while preserving PHP's warned plain-file fallback for unknown schemes.
pub(super) fn routing_method() -> MethodBuilder {
    let mut body = vec![
        s_if(e_call("str_starts_with", vec![e_var("filename"), e_str("data:")]), vec![s_return(e_bool(true))], vec![], None),
        s_assign("_separator", e_call("strpos", vec![e_var("filename"), e_str("://")])),
        s_if(eq(e_var("_separator"), e_bool(false)), vec![s_return(e_bool(false))], vec![], None),
        s_assign("_scheme", e_call("substr", vec![e_var("filename"), e_int(0), e_var("_separator")])),
        s_if(e_binop(e_call("strlen", vec![e_var("_scheme")]), BinOp::Lt, e_int(2)), vec![s_return(e_bool(false))], vec![], None),
        s_assign("_scheme_index", e_int(0)),
        s_while(e_binop(e_var("_scheme_index"), BinOp::Lt, e_call("strlen", vec![e_var("_scheme")])), vec![
            s_if(eq(e_call("strpos", vec![e_str("abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789+.-"),
                e_call("substr", vec![e_var("_scheme"), e_var("_scheme_index"), e_int(1)])]), e_bool(false)), vec![s_return(e_bool(false))], vec![], None),
            s_assign("_scheme_index", e_binop(e_var("_scheme_index"), BinOp::Add, e_int(1))),
        ]),
    ];
    for scheme in ["data", "php", "http", "https", "ftp", "ftps", "compress.zlib", "compress.bzip2", "phar", "zip", "glob"] {
        body.push(s_if(eq(e_call("strtolower", vec![e_var("_scheme")]), e_str(scheme)), vec![s_return(e_bool(true))], vec![], None));
    }
    body.push(s_if(eq(e_call("elephc_runtime_stream_has_wrapper", vec![e_var("_scheme")]), e_int(1)), vec![s_return(e_bool(true))], vec![], None));
    let message = e_binop(e_binop(e_binop(e_var("_function"), BinOp::Concat, e_str("(): Unable to find the wrapper \"")),
        BinOp::Concat, e_call("substr", vec![e_var("_scheme"), e_int(0), e_int(31)])), BinOp::Concat, e_str("\" - did you forget to enable it when you configured PHP?"));
    body.push(s_assign("_lookup", e_int(0)));
    body.push(s_while(e_binop(e_var("_lookup"), BinOp::Lt, e_var("_lookups")), vec![
        super::build::warning(message),
        s_assign("_lookup", e_binop(e_var("_lookup"), BinOp::Add, e_int(1))),
    ]));
    body.push(s_return(e_bool(false)));
    method("__elephc_is_stream").private().param("filename", TypeExpr::Str).param("_function", TypeExpr::Str)
        .param_default("_lookups", TypeExpr::Int, e_int(2)).returns(TypeExpr::Bool).body(body)
}

/// Reports PHP's two-warning `open_basedir` refusal and returns `false` for `variable`.
///
/// The first warning names the configured roots; the second comes from `failure()` and names
/// the public fileinfo call and target exactly like a failed stream open.
pub(super) fn basedir_guard(
    variable: &str,
    roots: &str,
    report_errors: Option<&str>,
) -> Stmt {
    let message = e_binop(
        e_binop(e_var("_function"), BinOp::Concat,
            e_str("(): open_basedir restriction in effect. File(")),
        BinOp::Concat,
        e_binop(e_var(variable), BinOp::Concat,
            e_binop(e_str(") is not within the allowed path(s): ("), BinOp::Concat,
                e_binop(e_str(roots), BinOp::Concat, e_str(")")))),
    );
    let outside = eq(
        e_call("elephc_fileinfo_path_allowed", vec![e_var(variable), e_str(roots)]),
        e_int(0),
    );
    let mut body = if let Some(flag) = report_errors {
        vec![s_if(e_var(flag), vec![super::build::warning(message)], vec![], None)]
    } else {
        vec![super::build::warning(message)]
    };
    body.extend(if report_errors.is_some() {
        super::streams::failure(e_str("Operation not permitted"))
    } else {
        super::streams::forced_failure(e_str("Operation not permitted"))
    });
    // The allowed branch opens the canonical spelling the check just resolved, so a symlink
    // swapped between the check and the native open cannot redirect the read outside the root.
    s_if(
        outside,
        body,
        vec![],
        Some(vec![
            // Publish the roots before the native open so it can traverse with `openat`
            // and re-check every descriptor-relative component.
            s_expr(e_call("elephc_fileinfo_set_basedir", vec![e_str(roots)])),
            s_assign(
                variable,
                e_call("elephc_fileinfo_confined_path", vec![]),
            ),
        ]),
    )
}

/// Reads nested plain-file resources without reporting an inner fopen warning, then retires the owned bytes.
fn local_bytes(open_basedir: Option<&str>) -> Vec<Stmt> {
    let mut body = Vec::new();
    if let Some(roots) = open_basedir {
        body.push(basedir_guard("filename", roots, Some("_report_errors")));
    }
    body.extend([
        s_assign("_read_id", e_call("elephc_fileinfo_read", vec![e_var("filename")])),
        s_if(eq(e_var("_read_id"), e_int(0)), super::streams::failure(e_call("elephc_fileinfo_error", vec![])), vec![], None),
        s_if(eq(e_var("_read_id"), e_int(-1)), vec![s_assign("_directory", e_bool(true)), s_return(e_str(""))], vec![], None),
        s_try(vec![s_return(e_call("__elephc_ptr_read_string", vec![
            e_call("elephc_fileinfo_read_data", vec![e_var("_read_id")]),
            e_call("elephc_fileinfo_read_length", vec![e_var("_read_id")]),
        ]))], vec![], Some(vec![s_expr(e_call("elephc_fileinfo_read_release", vec![e_var("_read_id")]))])),
    ]);
    body
}

/// Applies one filter through the existing stream implementation and its PHP callbacks.
pub(super) fn filter_method() -> MethodBuilder {
    let mut attach = vec![s_assign("_attached", e_call("stream_filter_append", vec![e_var("_stream"), e_var("name"), e_const("STREAM_FILTER_READ")]))];
    for name in ["string.toupper", "string.tolower", "string.rot13", "string.strip_tags", "dechunk",
        "convert.base64-encode", "convert.base64-decode", "convert.quoted-printable-encode",
        "convert.quoted-printable-decode", "bzip2.decompress"].into_iter().rev() {
        attach = vec![s_if(eq(e_var("name"), e_str(name)), vec![
            s_if(e_var("_directory"), vec![s_return(e_var("data"))], vec![], None),
            s_assign("_attached", e_call("stream_filter_append", vec![e_var("_stream"), e_str(name), e_const("STREAM_FILTER_READ")])),
        ], vec![], Some(attach))];
    }
    let mut work = vec![s_expr(e_call("fwrite", vec![e_var("_stream"), e_var("data")])),
        s_expr(e_call("rewind", vec![e_var("_stream")]))];
    work.extend(attach);
    work.push(s_if(eq(e_var("_attached"), e_bool(false)), vec![
        super::build::warning(e_binop(e_binop(e_var("_function"), BinOp::Concat,
            e_str("(): Unable to locate filter \"")), BinOp::Concat, e_binop(e_var("name"), BinOp::Concat, e_str("\"")))),
        super::build::warning(e_binop(e_binop(e_var("_function"), BinOp::Concat,
            e_str("(): Unable to create filter (")), BinOp::Concat, e_binop(e_var("name"), BinOp::Concat, e_str(")")))),
        // PHP warns about an unknown filter and leaves its input bytes unchanged.
        s_return(e_var("data")),
    ], vec![], None));
    work.push(s_if(e_var("_directory"), vec![s_return(e_var("data"))], vec![], None));
    work.push(s_return(e_call("stream_get_contents", vec![e_var("_stream"), e_int(7340032)])));
    method("__elephc_filter_bytes").private().param("data", TypeExpr::Str).param("name", TypeExpr::Str)
        .param("_function", TypeExpr::Str).param("_directory", TypeExpr::Bool)
        .returns(t_union(vec![TypeExpr::Str, TypeExpr::False])).body(vec![
            // PHP's filter default is raw DEFLATE, unlike the auto-detecting fopen helper.
            s_if(eq(e_var("name"), e_str("zlib.inflate")), vec![s_if(e_var("_directory"), vec![s_return(e_var("data"))], vec![], None), s_return(e_call("gzinflate", vec![e_var("data")]))], vec![], None),
            s_if(eq(e_var("name"), e_str("zlib.deflate")), vec![s_if(e_var("_directory"), vec![s_return(e_var("data"))], vec![], None), s_return(e_call("gzdeflate", vec![e_var("data")]))], vec![], None),
            s_assign("_stream", e_call("fopen", vec![e_str("php://temp"), e_str("w+b")])),
            s_if(eq(e_var("_stream"), e_bool(false)), vec![s_return(e_bool(false))], vec![], None),
            s_assign("_filter_unwind", e_bool(false)),
            s_try(work, vec![(vec!["Throwable"], Some("_filter_error"), vec![s_assign("_filter_unwind", e_bool(true)), s_throw(e_var("_filter_error"))])],
                Some(vec![s_expr(e_method_call(e_this(), "__elephc_close_streams", vec![e_var("_stream"), e_array(vec![]), e_var("_filter_unwind")]))])),
        ])
}
