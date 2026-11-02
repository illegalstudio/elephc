//! Purpose:
//! End-to-end regressions for `highlight_string()`/`highlight_file()`: PHP's exact HTML output,
//! the echo/return contract, a runtime-evaluated call, and the file renderer.
//!
//! Called from:
//! - `cargo test --test codegen_tests` through Rust's test harness.
//!
//! Key details:
//! - The expected text was produced by reference PHP 8.5 (`php hl_test2.php`), so any drift in the
//!   highlighter's colours, escaping, span coalescing or whitespace handling fails here.

use crate::support::*;

/// Verifies the AOT and eval highlighters reproduce reference PHP byte for byte.
#[test]
fn test_highlight_builtins_match_reference_php() {
    let out = compile_and_run(SOURCE);
    assert_eq!(out, EXPECTED);
}

/// The program under test; the file path is `__DIR__`-relative so the test is location-independent.
const SOURCE: &str = r#"<?php
echo highlight_string("<?php\necho 1; // c\n", true), "\n";
var_export(highlight_string("a\tb", true)); echo "\n";
var_export(highlight_string("<?php echo 2;")); echo "\n";
var_export(highlight_string('pre {$a["k"]} post', true)); echo "\n";
$code = "echo highlight_string('<?php \$x = 1;', true);";
eval($argc > 0 ? $code : '');
echo "\n";
file_put_contents(__DIR__ . '/hl_target.php', "<?php\n// c\n\$a = 1;\n");
var_export(highlight_file(__DIR__ . '/hl_target.php', true)); echo "\n";
"#;

/// Reference PHP stdout for [`SOURCE`].
const EXPECTED: &str = r#"<pre><code style="color: #000000"><span style="color: #0000BB">&lt;?php
</span><span style="color: #007700">echo </span><span style="color: #0000BB">1</span><span style="color: #007700">; </span><span style="color: #FF8000">// c
</span></code></pre>
'<pre><code style="color: #000000">a    b</code></pre>'
<pre><code style="color: #000000"><span style="color: #0000BB">&lt;?php </span><span style="color: #007700">echo </span><span style="color: #0000BB">2</span><span style="color: #007700">;</span></code></pre>true
'<pre><code style="color: #000000">pre {$a["k"]} post</code></pre>'
<pre><code style="color: #000000"><span style="color: #0000BB">&lt;?php $x </span><span style="color: #007700">= </span><span style="color: #0000BB">1</span><span style="color: #007700">;</span></code></pre>
'<pre><code style="color: #000000"><span style="color: #0000BB">&lt;?php
</span><span style="color: #FF8000">// c
</span><span style="color: #0000BB">$a </span><span style="color: #007700">= </span><span style="color: #0000BB">1</span><span style="color: #007700">;
</span></code></pre>'
"#;
