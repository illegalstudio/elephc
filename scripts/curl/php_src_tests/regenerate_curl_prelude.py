#!/usr/bin/env python3
"""Regenerate `src/curl_prelude/build.rs` from the PHP migration oracle.

`src/curl_prelude.rs` keeps `CURL_PRELUDE_SRC` (test-only) as the migration oracle, and
the shipped declarations are built as Rust AST in `src/curl_prelude/build.rs`. The two
must stay in sync; the permanent oracle test
(`curl_built_declarations_match_the_php_for_every_profile`) enforces it.

Workflow for a prelude behavior change:

1. Edit the PHP in `src/curl_prelude.rs`'s `CURL_PRELUDE_SRC`.
2. Run this script to transcribe the new source into builder calls.
3. Run `cargo test -p elephc --lib curl_built_declarations_match_the_php_for_every_profile`.

The script re-applies the two things the transcriber cannot know about: the PHP-version
conditionals (8.5-only members, and the deprecation bodies of `curl_close` /
`curl_share_close`).
"""
import os
import re
import subprocess
import sys
import tempfile

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "..", ".."))
BUILD = os.path.join(ROOT, "src", "curl_prelude", "build.rs")

HEADER = '''//! Purpose:
//! Builds the curl standard-library surface as AST — the `CurlHandle`/`CurlMultiHandle`/
//! `CurlShareHandle`/`CurlSharePersistentHandle` classes and every `curl_*` function —
//! replacing the PHP source `crate::curl_prelude::CURL_PRELUDE_SRC` used to carry.
//!
//! Called from:
//! - `crate::curl_prelude::inject_if_used_for_version`, after include resolution and before
//!   name resolution.
//!
//! Key details:
//! - TRANSCRIBED, not rewritten: every declaration here was generated from the parse of the
//!   PHP it replaces (`synthetic_class::transcribe`), and the migration oracle
//!   (`ELEPHC_ORACLE_PHP` / `ELEPHC_ORACLE_WHICH=curl`) compares the built AST against that
//!   parse node by node. `CURL_PRELUDE_SRC` stays the migration oracle.
//! - The PHP-version fences become the `php_version >= PhpVersion::Php85` conditional in
//!   `curl_declarations`: `curl_multi_get_handles`, `CurlSharePersistentHandle` and
//!   `curl_share_init_persistent` exist only from 8.5.

#![allow(dead_code)]
#![allow(clippy::too_many_lines)]

use crate::parser::ast::{BinOp, CastType, Program, Stmt, TypeExpr};
use crate::php_version::PhpVersion;
use crate::synthetic_class::*;

'''

CLOSE_CLOSE = '''fn decl_fn_curl_close(php_version: PhpVersion) -> Stmt {
    let body = if php_version >= PhpVersion::Php85 {
        vec![
            s_if(
                e_binop(e_prop(e_var("handle"), "__elephc_handle"), BinOp::StrictNotEq, e_null()),
                vec![
                    s_expr(e_call("trigger_error", vec![e_str("Function curl_close() is deprecated since 8.5, as it has no effect since PHP 8.0"), e_const("E_USER_DEPRECATED")])),
                ],
                vec![],
                None,
            ),
        ]
    } else {
        vec![]
    };
    function("curl_close")
        .param("handle", t_class("CurlHandle"))
        .keep_unread_params()
        .returns(TypeExpr::Void)
        .body(body)
        .build()
}'''

CLOSE_SHARE = CLOSE_CLOSE.replace("curl_close", "curl_share_close").replace(
    'e_var("handle")', 'e_var("share_handle")'
).replace('"handle"', '"share_handle"').replace(
    "t_class(\"CurlHandle\")", "t_class(\"CurlShareHandle\")"
)

GATED = [
    "decl_fn_curl_multi_get_handles",
    "decl_class_curlsharepersistenthandle",
    "decl_fn_curl_share_init_persistent",
]


def main():
    tmp = tempfile.mkdtemp(prefix="curl-transcribe-")
    out = os.path.join(tmp, "gen.rs")
    env = dict(os.environ)
    env.update(
        ELEPHC_TRANSCRIBE_OUT=out,
        ELEPHC_TRANSCRIBE_WHICH="curl",
        ELEPHC_TRANSCRIBE_SPLIT="curl_declarations",
        ELEPHC_TRANSCRIBE_VERSION="8.5",
    )
    subprocess.run(
        ["cargo", "test", "-p", "elephc", "--lib", "dump_prelude_on_request"],
        cwd=ROOT, env=env, check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
    )
    gen = open(out).read().splitlines(keepends=True)
    agg_at = next(i for i, l in enumerate(gen) if l.startswith("/// Builds the whole surface"))
    helpers = "".join(gen[:agg_at])
    calls = [l.strip().rstrip(",") for l in gen[agg_at:] if l.strip().startswith("decl_")]
    base = [c for c in calls if c[:-2] not in GATED]
    assert {c[:-2] for c in calls} - {c[:-2] for c in base} == set(GATED), "gated set changed"

    # Re-apply the version-conditional bodies the transcriber cannot infer.
    helpers = re.sub(r"fn decl_fn_curl_close\(\) -> Stmt \{.*?\n\}", CLOSE_CLOSE, helpers, flags=re.S)
    helpers = re.sub(r"fn decl_fn_curl_share_close\(\) -> Stmt \{.*?\n\}", CLOSE_SHARE, helpers, flags=re.S)

    body = "\n".join(
        "            " + (
            c[:-2] + "(php_version)" if c[:-2] in ("decl_fn_curl_close", "decl_fn_curl_share_close") else c
        ) + "," for c in base
    )
    gated = "\n".join("            decls.push(" + g + "());" for g in GATED)
    aggregator = (
        "/// Builds the whole surface for one PHP compatibility profile.\n"
        "///\n"
        "/// The 8.5-only members are appended last, matching the order of the PHP source the oracle\n"
        "/// compares against.\n"
        "pub(crate) fn curl_declarations(php_version: PhpVersion) -> Program {\n"
        "    internal_declarations(|| {\n"
        "        let mut decls = vec![\n"
        f"{body}\n"
        "        ];\n"
        "        if php_version >= PhpVersion::Php85 {\n"
        f"{gated}\n"
        "        }\n"
        "        decls\n"
        "    })\n"
        "}\n"
    )
    open(BUILD, "w").write(HEADER + helpers + aggregator)
    print(f"wrote {BUILD}")


if __name__ == "__main__":
    sys.exit(main())
