//! Purpose:
//! Interpreter tests for a TEMPLATE included at run time: leading HTML, `<?=`, an alternative
//! `foreach:`/`endforeach;` body with HTML inside it, and the newline a closing tag swallows.
//!
//! Called from:
//! - `cargo test -p elephc-magician interpreter::tests::interleaved_html_include`.
//!
//! Key details:
//! - The FILE entry point is the one under test. `parse_fragment` is right to refuse a `<?php`
//!   token -- a fragment is already inside PHP -- so a template can only be measured the way it
//!   is actually reached, through `include`, which goes to `parse_source_file`.
//! - Every expected byte is `php -n` 8.5.6's output on the same two files, captured the same way
//!   the test does, with `ob_start()` / `ob_get_clean()`.
//! - Symfony's `error-handler/Resources/views` templates are all written in this shape, and the
//!   dumped container reaches them through exactly this include path.

use super::super::*;
use super::support::*;

/// Runs one template through `include` and returns what the caller captured.
///
/// The fragment plays the part of the compiled entry point; the template is read from disk at
/// run time, which is the only way the file-shaped parser is reached.
fn run_template_fixture(tag: &str, files: &[(&str, &str)], fragment: &[u8]) -> String {
    let dir = std::env::temp_dir().join(format!(
        "elephc-magician-interleaved-html-{}-{}",
        std::process::id(),
        tag
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create template fixture directory");
    for (name, contents) in files {
        std::fs::write(dir.join(name), contents).expect("write template fixture");
    }
    let program = parse_fragment(fragment).expect("parse template caller fragment");
    let mut context = ElephcEvalContext::new();
    context.set_call_site(
        dir.join("main.php").to_string_lossy().into_owned(),
        dir.to_string_lossy().into_owned(),
        1,
    );
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect("execute template caller fragment");
    let _ = std::fs::remove_dir_all(&dir);
    values.output.clone()
}

/// The template: every interleaving a Symfony view uses, in one file.
///
/// Leading HTML, a `<?=` short echo, an alternative-syntax `foreach` whose body is HTML with two
/// more short echoes in it, a `<?php … ?>` block, and trailing HTML. Five closing tags, each
/// followed by a newline that php swallows.
const TEMPLATE: &str = "<h1>Title</h1>\n\
                        <?= $x ?>\n\
                        <ul>\n\
                        <?php foreach ($rows as $k => $row): ?>\n\
                        \x20 <li><?= $k ?>:<?= $row ?></li>\n\
                        <?php endforeach; ?>\n\
                        </ul>\n\
                        <?php $done = 1; ?>\n\
                        <footer><?= $done ?></footer>\n";

/// Verifies the whole template renders byte for byte the way php renders it.
///
/// `php -n` 8.5.6 captures
/// `<h1>Title</h1>\nhello<ul>\n  <li>a:1</li>\n  <li>b:2</li>\n</ul>\n<footer>1</footer>\n`.
///
/// Two things in that string are the measurement rather than decoration. `hello<ul>` has no
/// newline between it because the `?>` after `$x` swallowed one -- php swallows exactly one, and
/// the file has exactly one there. And `</ul>` follows the last `</li>\n` directly, because the
/// `?>` closing `endforeach;` swallowed that newline too, once, on the pass that ended the loop.
#[test]
fn a_template_include_renders_html_short_echoes_and_an_alternative_loop() {
    assert_eq!(
        run_template_fixture(
            "template",
            &[("tpl.php", TEMPLATE)],
            br#"$x = "hello";
$rows = ["a" => 1, "b" => 2];
ob_start();
include "tpl.php";
echo ob_get_clean();"#,
        ),
        "<h1>Title</h1>\nhello<ul>\n  <li>a:1</li>\n  <li>b:2</li>\n</ul>\n<footer>1</footer>\n",
    );
}

/// Verifies a template body that runs zero times still emits the HTML around it.
///
/// `php -n` 8.5.6 captures `<ul>\n</ul>\n`. An empty loop is the case where a swallowed newline
/// and a body that never ran can be confused for each other, so it is pinned on its own.
#[test]
fn an_alternative_loop_over_nothing_still_emits_the_html_around_it() {
    assert_eq!(
        run_template_fixture(
            "empty",
            &[(
                "tpl.php",
                "<ul>\n<?php foreach ($rows as $row): ?>\n  <li><?= $row ?></li>\n<?php endforeach; ?>\n</ul>\n",
            )],
            br#"$rows = [];
ob_start();
include "tpl.php";
echo ob_get_clean();"#,
        ),
        "<ul>\n</ul>\n",
    );
}
