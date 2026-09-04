//! Purpose:
//! Builds php's `gz*` stream surface and the four zlib string functions as AST, replacing the PHP
//! source this module used to carry as a raw string and reparse on every compile that touched it.
//!
//! Called from:
//! - `crate::gz_prelude::inject_if_used`, after include resolution and before name resolution.
//!
//! Key details:
//! - TRANSCRIBED, not rewritten: every declaration here was generated from the parse of the PHP it
//!   replaces (`synthetic_class::transcribe`), and the migration oracle
//!   (`ELEPHC_ORACLE_PHP` / `ELEPHC_ORACLE_WHICH=gz`) compares the built AST against that parse
//!   node by node. Edit the shape here only with the same comparison in hand.
//! - The PHP form stays under `#[cfg(test)]` in the parent module as that oracle's reference. It is
//!   no longer tokenized on any real compile.
//! - The transcriber PANICS on a node it cannot express rather than dropping it, so a declaration
//!   cannot go missing silently in the conversion.

use crate::parser::ast::{BinOp, Program, Stmt, TypeExpr};
use crate::synthetic_class::{
    e_binop, e_bool, e_call, e_const, e_index, e_int, e_neg, e_new_fq, e_null, e_str, e_ternary, e_var, function, internal_declarations, s_assign, s_if, s_return, s_throw, t_array, t_mixed, t_nullable, t_union,
};

/// `gzopen` — transcribed from the PHP form.
fn decl_fn_gzopen() -> Stmt {
    function("gzopen")
        .param("filename", TypeExpr::Str)
        .param("mode", TypeExpr::Str)
        .param_default("use_include_path", TypeExpr::Int, e_int(0))
        .body(vec![
            s_return(e_call("fopen", vec![e_binop(e_str("compress.zlib://"), BinOp::Concat, e_var("filename")), e_var("mode"), e_binop(e_var("use_include_path"), BinOp::StrictNotEq, e_int(0))])),
        ])
        .build()
}

/// `gzclose` — transcribed from the PHP form.
fn decl_fn_gzclose() -> Stmt {
    function("gzclose")
        .param("stream", t_mixed())
        .returns(TypeExpr::Bool)
        .body(vec![
            s_return(e_call("fclose", vec![e_var("stream")])),
        ])
        .build()
}

/// `gzeof` — transcribed from the PHP form.
fn decl_fn_gzeof() -> Stmt {
    function("gzeof")
        .param("stream", t_mixed())
        .returns(TypeExpr::Bool)
        .body(vec![
            s_return(e_call("feof", vec![e_var("stream")])),
        ])
        .build()
}

/// `gzgetc` — transcribed from the PHP form.
fn decl_fn_gzgetc() -> Stmt {
    function("gzgetc")
        .param("stream", t_mixed())
        .returns(t_union(vec![TypeExpr::Str, TypeExpr::False]))
        .body(vec![
            s_return(e_call("fgetc", vec![e_var("stream")])),
        ])
        .build()
}

/// `gzgets` — transcribed from the PHP form.
fn decl_fn_gzgets() -> Stmt {
    function("gzgets")
        .param("stream", t_mixed())
        .param_default("length", t_nullable(TypeExpr::Int), e_null())
        .returns(t_union(vec![TypeExpr::Str, TypeExpr::False]))
        .body(vec![
            s_if(
                e_binop(e_var("length"), BinOp::StrictEq, e_null()),
                vec![
                    s_return(e_call("fgets", vec![e_var("stream")])),
                ],
                vec![],
                None,
            ),
            s_return(e_call("fgets", vec![e_var("stream"), e_var("length")])),
        ])
        .build()
}

/// `gzread` — transcribed from the PHP form.
fn decl_fn_gzread() -> Stmt {
    function("gzread")
        .param("stream", t_mixed())
        .param("length", TypeExpr::Int)
        .returns(t_union(vec![TypeExpr::Str, TypeExpr::False]))
        .body(vec![
            s_return(e_call("fread", vec![e_var("stream"), e_var("length")])),
        ])
        .build()
}

/// `gzwrite` — transcribed from the PHP form.
fn decl_fn_gzwrite() -> Stmt {
    function("gzwrite")
        .param("stream", t_mixed())
        .param("data", TypeExpr::Str)
        .param_default("length", t_nullable(TypeExpr::Int), e_null())
        .returns(t_union(vec![TypeExpr::Int, TypeExpr::False]))
        .body(vec![
            s_if(
                e_binop(e_var("length"), BinOp::StrictEq, e_null()),
                vec![
                    s_return(e_call("fwrite", vec![e_var("stream"), e_var("data")])),
                ],
                vec![],
                None,
            ),
            s_return(e_call("fwrite", vec![e_var("stream"), e_var("data"), e_var("length")])),
        ])
        .build()
}

/// `gzputs` — transcribed from the PHP form.
fn decl_fn_gzputs() -> Stmt {
    function("gzputs")
        .param("stream", t_mixed())
        .param("data", TypeExpr::Str)
        .param_default("length", t_nullable(TypeExpr::Int), e_null())
        .returns(t_union(vec![TypeExpr::Int, TypeExpr::False]))
        .body(vec![
            s_return(e_call("gzwrite", vec![e_var("stream"), e_var("data"), e_var("length")])),
        ])
        .build()
}

/// `gzpassthru` — transcribed from the PHP form.
fn decl_fn_gzpassthru() -> Stmt {
    function("gzpassthru")
        .param("stream", t_mixed())
        .returns(TypeExpr::Int)
        .body(vec![
            s_return(e_call("fpassthru", vec![e_var("stream")])),
        ])
        .build()
}

/// `gzrewind` — transcribed from the PHP form.
fn decl_fn_gzrewind() -> Stmt {
    function("gzrewind")
        .param("stream", t_mixed())
        .returns(TypeExpr::Bool)
        .body(vec![
            s_return(e_call("rewind", vec![e_var("stream")])),
        ])
        .build()
}

/// `gzseek` — transcribed from the PHP form.
fn decl_fn_gzseek() -> Stmt {
    function("gzseek")
        .param("stream", t_mixed())
        .param("offset", TypeExpr::Int)
        .param_default("whence", TypeExpr::Int, e_const("SEEK_SET"))
        .returns(TypeExpr::Int)
        .body(vec![
            s_return(e_call("fseek", vec![e_var("stream"), e_var("offset"), e_var("whence")])),
        ])
        .build()
}

/// `gztell` — transcribed from the PHP form.
fn decl_fn_gztell() -> Stmt {
    function("gztell")
        .param("stream", t_mixed())
        .returns(t_union(vec![TypeExpr::Int, TypeExpr::False]))
        .body(vec![
            s_return(e_call("ftell", vec![e_var("stream")])),
        ])
        .build()
}

/// `__elephc_gzip_frame` — transcribed from the PHP form.
fn decl_fn_elephc_gzip_frame() -> Stmt {
    function("__elephc_gzip_frame")
        .param("data", t_mixed())
        .param("level", TypeExpr::Int)
        .returns(TypeExpr::Str)
        .body(vec![
            s_assign("xfl", e_int(0)),
            s_if(
                e_binop(e_var("level"), BinOp::StrictEq, e_int(9)),
                vec![
                    s_assign("xfl", e_int(2)),
                ],
                vec![
                (e_binop(e_binop(e_var("level"), BinOp::StrictEq, e_int(0)), BinOp::Or, e_binop(e_var("level"), BinOp::StrictEq, e_int(1))), vec![
                    s_assign("xfl", e_int(4)),
                ]),
            ],
                None,
            ),
            s_assign("os", e_ternary(e_binop(e_const("PHP_OS"), BinOp::StrictEq, e_str("Darwin")), e_int(19), e_int(3))),
            s_assign("crc", e_call("crc32", vec![e_var("data")])),
            s_assign("len", e_call("strlen", vec![e_var("data")])),
            s_return(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_str("\u{1f}\u{e08b}\u{8}\0\0\0\0\0"), BinOp::Concat, e_call("chr", vec![e_var("xfl")])), BinOp::Concat, e_call("chr", vec![e_var("os")])), BinOp::Concat, e_call("gzdeflate", vec![e_var("data"), e_var("level")])), BinOp::Concat, e_call("chr", vec![e_binop(e_var("crc"), BinOp::BitAnd, e_int(255))])), BinOp::Concat, e_call("chr", vec![e_binop(e_binop(e_var("crc"), BinOp::ShiftRight, e_int(8)), BinOp::BitAnd, e_int(255))])), BinOp::Concat, e_call("chr", vec![e_binop(e_binop(e_var("crc"), BinOp::ShiftRight, e_int(16)), BinOp::BitAnd, e_int(255))])), BinOp::Concat, e_call("chr", vec![e_binop(e_binop(e_var("crc"), BinOp::ShiftRight, e_int(24)), BinOp::BitAnd, e_int(255))])), BinOp::Concat, e_call("chr", vec![e_binop(e_var("len"), BinOp::BitAnd, e_int(255))])), BinOp::Concat, e_call("chr", vec![e_binop(e_binop(e_var("len"), BinOp::ShiftRight, e_int(8)), BinOp::BitAnd, e_int(255))])), BinOp::Concat, e_call("chr", vec![e_binop(e_binop(e_var("len"), BinOp::ShiftRight, e_int(16)), BinOp::BitAnd, e_int(255))])), BinOp::Concat, e_call("chr", vec![e_binop(e_binop(e_var("len"), BinOp::ShiftRight, e_int(24)), BinOp::BitAnd, e_int(255))]))),
        ])
        .build()
}

/// `gzencode` — transcribed from the PHP form.
fn decl_fn_gzencode() -> Stmt {
    function("gzencode")
        .param("data", t_mixed())
        .param_default("level", TypeExpr::Int, e_neg(e_int(1)))
        .param_default("encoding", TypeExpr::Int, e_int(31))
        .returns(t_union(vec![TypeExpr::Str, TypeExpr::False]))
        .body(vec![
            s_if(
                e_binop(e_binop(e_var("level"), BinOp::Lt, e_neg(e_int(1))), BinOp::Or, e_binop(e_var("level"), BinOp::Gt, e_int(9))),
                vec![
                    s_throw(e_new_fq("ValueError", vec![e_str("gzencode(): Argument #2 ($level) must be between -1 and 9")])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("encoding"), BinOp::StrictEq, e_neg(e_int(15))),
                vec![
                    s_return(e_call("gzdeflate", vec![e_var("data"), e_var("level")])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("encoding"), BinOp::StrictEq, e_int(15)),
                vec![
                    s_return(e_call("gzcompress", vec![e_var("data"), e_var("level")])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("encoding"), BinOp::StrictNotEq, e_int(31)),
                vec![
                    s_throw(e_new_fq("ValueError", vec![e_str("gzencode(): Argument #3 ($encoding) must be one of ZLIB_ENCODING_RAW, ZLIB_ENCODING_GZIP, or ZLIB_ENCODING_DEFLATE")])),
                ],
                vec![],
                None,
            ),
            s_return(e_call("__elephc_gzip_frame", vec![e_var("data"), e_var("level")])),
        ])
        .build()
}

/// `zlib_encode` — transcribed from the PHP form.
fn decl_fn_zlib_encode() -> Stmt {
    function("zlib_encode")
        .param("data", t_mixed())
        .param("encoding", TypeExpr::Int)
        .param_default("level", TypeExpr::Int, e_neg(e_int(1)))
        .returns(t_union(vec![TypeExpr::Str, TypeExpr::False]))
        .body(vec![
            s_if(
                e_binop(e_binop(e_var("level"), BinOp::Lt, e_neg(e_int(1))), BinOp::Or, e_binop(e_var("level"), BinOp::Gt, e_int(9))),
                vec![
                    s_throw(e_new_fq("ValueError", vec![e_str("zlib_encode(): Argument #3 ($level) must be between -1 and 9")])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("encoding"), BinOp::StrictEq, e_neg(e_int(15))),
                vec![
                    s_return(e_call("gzdeflate", vec![e_var("data"), e_var("level")])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("encoding"), BinOp::StrictEq, e_int(15)),
                vec![
                    s_return(e_call("gzcompress", vec![e_var("data"), e_var("level")])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("encoding"), BinOp::StrictNotEq, e_int(31)),
                vec![
                    s_throw(e_new_fq("ValueError", vec![e_str("zlib_encode(): Argument #2 ($encoding) must be one of ZLIB_ENCODING_RAW, ZLIB_ENCODING_GZIP, or ZLIB_ENCODING_DEFLATE")])),
                ],
                vec![],
                None,
            ),
            s_return(e_call("__elephc_gzip_frame", vec![e_var("data"), e_var("level")])),
        ])
        .build()
}

/// `__elephc_gzip_body` — transcribed from the PHP form.
fn decl_fn_elephc_gzip_body() -> Stmt {
    function("__elephc_gzip_body")
        .param("data", t_mixed())
        .returns(t_union(vec![TypeExpr::Str, TypeExpr::False]))
        .body(vec![
            s_if(
                e_binop(e_call("strlen", vec![e_var("data")]), BinOp::Lt, e_int(18)),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_binop(e_index(e_var("data"), e_int(0)), BinOp::StrictNotEq, e_str("\u{1f}")), BinOp::Or, e_binop(e_index(e_var("data"), e_int(1)), BinOp::StrictNotEq, e_str("\u{e08b}"))), BinOp::Or, e_binop(e_call("ord", vec![e_index(e_var("data"), e_int(2))]), BinOp::StrictNotEq, e_int(8))),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_assign("flg", e_call("ord", vec![e_index(e_var("data"), e_int(3))])),
            s_assign("pos", e_int(10)),
            s_if(
                e_binop(e_binop(e_var("flg"), BinOp::BitAnd, e_int(4)), BinOp::StrictNotEq, e_int(0)),
                vec![
                    s_assign("pos", e_binop(e_binop(e_var("pos"), BinOp::Add, e_int(2)), BinOp::Add, e_binop(e_call("ord", vec![e_index(e_var("data"), e_var("pos"))]), BinOp::BitOr, e_binop(e_call("ord", vec![e_index(e_var("data"), e_binop(e_var("pos"), BinOp::Add, e_int(1)))]), BinOp::ShiftLeft, e_int(8))))),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_var("flg"), BinOp::BitAnd, e_int(8)), BinOp::StrictNotEq, e_int(0)),
                vec![
                    s_assign("nameEnd", e_call("strpos", vec![e_var("data"), e_str("\0"), e_var("pos")])),
                    s_if(
                        e_binop(e_var("nameEnd"), BinOp::StrictEq, e_bool(false)),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("pos", e_binop(e_var("nameEnd"), BinOp::Add, e_int(1))),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_var("flg"), BinOp::BitAnd, e_int(16)), BinOp::StrictNotEq, e_int(0)),
                vec![
                    s_assign("commentEnd", e_call("strpos", vec![e_var("data"), e_str("\0"), e_var("pos")])),
                    s_if(
                        e_binop(e_var("commentEnd"), BinOp::StrictEq, e_bool(false)),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("pos", e_binop(e_var("commentEnd"), BinOp::Add, e_int(1))),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_var("flg"), BinOp::BitAnd, e_int(2)), BinOp::StrictNotEq, e_int(0)),
                vec![
                    s_assign("pos", e_binop(e_var("pos"), BinOp::Add, e_int(2))),
                ],
                vec![],
                None,
            ),
            s_assign("length", e_binop(e_binop(e_call("strlen", vec![e_var("data")]), BinOp::Sub, e_var("pos")), BinOp::Sub, e_int(8))),
            s_if(
                e_binop(e_var("length"), BinOp::Lt, e_int(0)),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_return(e_call("substr", vec![e_var("data"), e_var("pos"), e_var("length")])),
        ])
        .build()
}

/// `gzdecode` — transcribed from the PHP form.
fn decl_fn_gzdecode() -> Stmt {
    function("gzdecode")
        .param("data", t_mixed())
        .param_default("max_length", TypeExpr::Int, e_int(0))
        .returns(t_union(vec![TypeExpr::Str, TypeExpr::False]))
        .body(vec![
            s_assign("body", e_call("__elephc_gzip_body", vec![e_var("data")])),
            s_if(
                e_binop(e_var("body"), BinOp::StrictEq, e_bool(false)),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_return(e_call("gzinflate", vec![e_var("body"), e_var("max_length")])),
        ])
        .build()
}

/// `zlib_decode` — transcribed from the PHP form.
fn decl_fn_zlib_decode() -> Stmt {
    function("zlib_decode")
        .param("data", t_mixed())
        .param_default("max_length", TypeExpr::Int, e_int(0))
        .returns(t_union(vec![TypeExpr::Str, TypeExpr::False]))
        .body(vec![
            s_if(
                e_binop(e_call("strlen", vec![e_var("data")]), BinOp::Lt, e_int(2)),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_index(e_var("data"), e_int(0)), BinOp::StrictEq, e_str("\u{1f}")), BinOp::And, e_binop(e_index(e_var("data"), e_int(1)), BinOp::StrictEq, e_str("\u{e08b}"))),
                vec![
                    s_return(e_call("gzdecode", vec![e_var("data"), e_var("max_length")])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_call("ord", vec![e_index(e_var("data"), e_int(0))]), BinOp::BitAnd, e_int(15)), BinOp::StrictEq, e_int(8)),
                vec![
                    s_return(e_call("gzuncompress", vec![e_var("data"), e_var("max_length")])),
                ],
                vec![],
                None,
            ),
            s_return(e_call("gzinflate", vec![e_var("data"), e_var("max_length")])),
        ])
        .build()
}

/// `zlib_get_coding_type` — transcribed from the PHP form.
fn decl_fn_zlib_get_coding_type() -> Stmt {
    function("zlib_get_coding_type")
        .returns(t_union(vec![TypeExpr::Str, TypeExpr::False]))
        .body(vec![
            s_return(e_bool(false)),
        ])
        .build()
}

/// `gzfile` — transcribed from the PHP form.
fn decl_fn_gzfile() -> Stmt {
    function("gzfile")
        .param("filename", TypeExpr::Str)
        .param_default("use_include_path", TypeExpr::Int, e_int(0))
        .returns(t_union(vec![t_array(), TypeExpr::False]))
        .body(vec![
            s_return(e_call("file", vec![e_binop(e_str("compress.zlib://"), BinOp::Concat, e_var("filename")), e_ternary(e_binop(e_var("use_include_path"), BinOp::StrictNotEq, e_int(0)), e_const("FILE_USE_INCLUDE_PATH"), e_int(0))])),
        ])
        .build()
}

/// `readgzfile` — transcribed from the PHP form.
fn decl_fn_readgzfile() -> Stmt {
    function("readgzfile")
        .param("filename", TypeExpr::Str)
        .param_default("use_include_path", TypeExpr::Int, e_int(0))
        .returns(t_union(vec![TypeExpr::Int, TypeExpr::False]))
        .body(vec![
            s_return(e_call("readfile", vec![e_binop(e_str("compress.zlib://"), BinOp::Concat, e_var("filename")), e_binop(e_var("use_include_path"), BinOp::StrictNotEq, e_int(0))])),
        ])
        .build()
}

/// Builds the whole surface, one declaration per helper above.
pub(crate) fn gz_declarations() -> Program {
    internal_declarations(|| {
        vec![
            decl_fn_gzopen(),
            decl_fn_gzclose(),
            decl_fn_gzeof(),
            decl_fn_gzgetc(),
            decl_fn_gzgets(),
            decl_fn_gzread(),
            decl_fn_gzwrite(),
            decl_fn_gzputs(),
            decl_fn_gzpassthru(),
            decl_fn_gzrewind(),
            decl_fn_gzseek(),
            decl_fn_gztell(),
            decl_fn_elephc_gzip_frame(),
            decl_fn_gzencode(),
            decl_fn_zlib_encode(),
            decl_fn_elephc_gzip_body(),
            decl_fn_gzdecode(),
            decl_fn_zlib_decode(),
            decl_fn_zlib_get_coding_type(),
            decl_fn_gzfile(),
            decl_fn_readgzfile(),
        ]
    })
}
