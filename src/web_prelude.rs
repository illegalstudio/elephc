//! Purpose:
//! The `--web` request prelude: under `--web`, prepends an `extern "elephc_web"`
//! declaration block (Phase 2 Task 2) and executable statements that build the
//! request superglobals ($_SERVER/$_GET/$_POST) on every request (Task 5+).
//!
//! Called from:
//! - `crate::pipeline::compile`, after the other preludes and before name
//!   resolution, gated on `CliConfig.web` (NOT usage detection — it is the only
//!   flag-gated prelude).
//!
//! Key details:
//! - The injected statements run before user top-level code each request because
//!   the prelude statements are prepended and the whole top-level body re-runs
//!   per request.

use crate::parser::ast::Program;

/// The PHP source prepended under `--web`. Phase 2 Task 2: extern declarations;
/// Task 5: $_SERVER; Task 6: $_GET parsed from the query string. The query
/// parser is built inline (element-by-element into the $_GET superglobal),
/// mirroring the $_SERVER pattern, to stay within the type checker's proven
/// capabilities (a helper function returning a freshly-built assoc array trips
/// return-type inference / union widening).
const WEB_PRELUDE_SRC: &str = r#"<?php
extern "elephc_web" {
    function elephc_web_method(): string;
    function elephc_web_uri(): string;
    function elephc_web_path(): string;
    function elephc_web_query_string(): string;
    function elephc_web_header_count(): int;
    function elephc_web_header_name(int $i): string;
    function elephc_web_header_value(int $i): string;
    function elephc_web_body_ptr(): ptr;
    function elephc_web_body_len(): int;
}
$_SERVER = [];
$_SERVER['REQUEST_METHOD'] = elephc_web_method();
$_SERVER['REQUEST_URI']    = elephc_web_uri();
$_SERVER['QUERY_STRING']   = elephc_web_query_string();
$__elephc_hc = elephc_web_header_count();
for ($__elephc_i = 0; $__elephc_i < $__elephc_hc; $__elephc_i++) {
    $__elephc_hn = elephc_web_header_name($__elephc_i);
    $__elephc_hv = elephc_web_header_value($__elephc_i);
    $_SERVER['HTTP_' . strtoupper(str_replace('-', '_', $__elephc_hn))] = $__elephc_hv;
    $__elephc_up = strtoupper($__elephc_hn);
    if ($__elephc_up === 'CONTENT-TYPE') { $_SERVER['CONTENT_TYPE'] = $__elephc_hv; }
    if ($__elephc_up === 'CONTENT-LENGTH') { $_SERVER['CONTENT_LENGTH'] = $__elephc_hv; }
}
$_GET = [];
$__elephc_qs = elephc_web_query_string();
if ($__elephc_qs !== '') {
    $__elephc_pairs = explode('&', $__elephc_qs);
    foreach ($__elephc_pairs as $__elephc_pair) {
        $__elephc_eq = strpos($__elephc_pair, '=');
        if ($__elephc_eq === false) {
            if ($__elephc_pair !== '') {
                $_GET[rawurldecode($__elephc_pair)] = '';
            }
        } else {
            $__elephc_gk = rawurldecode(substr($__elephc_pair, 0, $__elephc_eq));
            $__elephc_gv = rawurldecode(substr($__elephc_pair, $__elephc_eq + 1));
            $_GET[$__elephc_gk] = $__elephc_gv;
        }
    }
}
"#;

/// Prepends the web prelude when compiling with `--web`. Returns the program
/// unchanged otherwise.
pub fn inject_if_web(program: Program, web: bool) -> Program {
    if !web {
        return program;
    }
    let tokens = crate::lexer::tokenize(WEB_PRELUDE_SRC).expect("web prelude must tokenize");
    let mut combined = crate::parser::parse(&tokens).expect("web prelude must parse");
    combined.extend(program);
    combined
}
