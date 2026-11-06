//! Purpose:
//! Pins that a PHP file included from `eval()` runs as ONE script, as in PHP: template blocks
//! may span the tags, and every shape prints what reference PHP 8.5.10 prints.
//!
//! Called from:
//! - `cargo test --test eval_include_template_tests` through Rust's test harness.
//!
//! Key details:
//! - Each block of an included file used to be parsed on its own, so the most common template
//!   shape, `<?php foreach ($rows as $r) { ?><li><?= $r ?></li><?php } ?>`, was a parse error.
//!   The file is now parsed whole (`crates/elephc-magician/src/script_cache/segments.rs`).
//! - The include is made from `eval()`, with the driver code read at run time, so the
//!   compiler never sees it: this is the runtime include path, the one that path serves.
//! - Expected bytes are compared RAW: one template keeps Latin-1 bytes in its HTML.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

static TEST_ID: AtomicUsize = AtomicUsize::new(0);

/// Creates an isolated temp dir unique across parallel test threads/processes.
fn make_test_dir(prefix: &str) -> PathBuf {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!(
        "{}_{}_{:?}_{}",
        prefix,
        std::process::id(),
        std::thread::current().id(),
        id
    ));
    fs::create_dir_all(dir.join("tpl")).unwrap();
    dir.canonicalize().unwrap()
}

/// Resolves the elephc CLI binary path (cargo env var, fallback next to the test binary).
fn elephc_bin() -> String {
    std::env::var("CARGO_BIN_EXE_elephc").unwrap_or_else(|_| {
        let mut path = std::env::current_exe().expect("failed to resolve current test binary");
        path.pop();
        if path.ends_with("deps") {
            path.pop();
        }
        path.join("elephc").to_string_lossy().into_owned()
    })
}

/// The compiled program: it only evaluates the driver it reads at run time.
const HARNESS: &str = r#"<?php
$dir = __DIR__;
$code = file_get_contents($dir . "/driver.txt");
eval($code);
"#;

/// The eval'd driver: includes each listed template and prints its output and return value.
///
/// The value is echoed rather than `var_export`ed: every template returns an int, and eval
/// does not support `var_export()` yet.
const DRIVER: &str = r#"foreach (file($dir . "/list.txt", FILE_IGNORE_NEW_LINES) as $name) {
    echo "[", $name, ":";
    $r = include $dir . "/tpl/" . $name . ".php";
    echo "|", $r, "]\n";
}
echo "END\n";
"#;

/// Writes the templates, the list and the driver, compiles the harness and runs it.
fn run_templates(prefix: &str, templates: &[(&str, &[u8])]) -> Output {
    let dir = make_test_dir(prefix);
    for (name, body) in templates {
        fs::write(dir.join("tpl").join(format!("{name}.php")), body).unwrap();
    }
    let list: String = templates.iter().map(|(name, _)| format!("{name}\n")).collect();
    fs::write(dir.join("list.txt"), list).unwrap();
    fs::write(dir.join("driver.txt"), DRIVER).unwrap();
    fs::write(dir.join("main.php"), HARNESS).unwrap();
    compile(&dir);
    Command::new(dir.join("main")).output().expect("failed to run binary")
}

/// Compiles `main.php` in `dir`.
fn compile(dir: &Path) {
    let output = Command::new(elephc_bin())
        .env("XDG_CACHE_HOME", dir.join("cache-root"))
        .current_dir(dir)
        .arg(dir.join("main.php"))
        .output()
        .expect("failed to spawn elephc");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
}

/// Every template shape prints what reference PHP 8.5.10 prints, MEASURED with this harness:
/// blocks, loops, alternative syntax and a function body spanning the tags, `<?=`, the one
/// newline `?>` swallows (LF, CRLF and CR), `__LINE__` counted across the file, Latin-1 HTML,
/// `<?phpX` and a bare `<?` staying HTML, a `return` mid-template, a file ending in code or on
/// a bare `<?php`, `<?PHP`, and an empty file.
#[test]
fn an_included_template_runs_as_one_script_as_php_does() {
    let templates: &[(&str, &[u8])] = &[
        ("foreach_short_echo", b"<ul><?php foreach ([\"a\", \"b\"] as $r) { ?><li><?= $r ?></li><?php } ?></ul>\n"),
        ("alt_syntax", b"<?php $x = 1; if ($x): ?>yes<?php else: ?>no<?php endif; ?>|<?php for ($i = 0; $i < 2; $i++): ?>[<?= $i ?>]<?php endfor ?>\n"),
        ("newline_after_close", b"<?php echo 'a'; ?>\nB\n<?php echo 'c'; ?>\r\nD\n<?php echo 'e'; ?>\rF\n"),
        ("line_numbers", b"<p>\n<?php echo __LINE__; ?>\n</p>\n<?php\n\necho __LINE__, \"\\n\";\n"),
        ("latin1_html", b"caf\xe9 <?php echo 'ok'; ?> \xff\n"),
        ("php_x_is_html", b"<?phpX not code ?>\n<?php echo 'code';\n"),
        ("return_midway", b"one\n<?php if (true) { ?>two\n<?php return 42; } ?>\nthree\n"),
        ("ends_in_code", b"head <?php echo 'x';"),
        ("open_tag_at_eof", b"html only <?php"),
        ("upper_open_tag", b"a<?PHP echo 'b'; ?>c\n"),
        ("short_tag_is_html", b"<? echo 'x'; ?>|<?php echo 'y'; ?>\n"),
        ("function_across_blocks", b"<?php function row($v) { ?><td><?= $v ?></td><?php } row(1); row(2); ?>\n"),
        ("empty_file", b""),
    ];
    let run = run_templates("eval_include_template", templates);
    let expected: &[u8] = b"[foreach_short_echo:<ul><li>a</li><li>b</li></ul>\n|1]\n\
[alt_syntax:yes|[0][1]|1]\n\
[newline_after_close:aB\ncD\neF\n|1]\n\
[line_numbers:<p>\n2</p>\n6\n|1]\n\
[latin1_html:caf\xe9 ok \xff\n|1]\n\
[php_x_is_html:<?phpX not code ?>\ncode|1]\n\
[return_midway:one\ntwo\n|42]\n\
[ends_in_code:head x|1]\n\
[open_tag_at_eof:html only |1]\n\
[upper_open_tag:abc\n|1]\n\
[short_tag_is_html:<? echo 'x'; ?>|y|1]\n\
[function_across_blocks:<td>1</td><td>2</td>|1]\n\
[empty_file:|1]\n\
END\n";
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(expected),
        "stderr: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(run.stdout, expected, "the Latin-1 bytes must come through unchanged");
}

/// A parse error anywhere in an included file stops it before ANY of it runs, as in PHP,
/// which compiles a whole script first. MEASURED: reference prints only the parse error for
/// this template; elephc used to print `before`, `ran` and `middle` first.
#[test]
fn a_late_parse_error_runs_none_of_the_included_file() {
    let templates: &[(&str, &[u8])] = &[(
        "late_parse_error",
        b"before\n<?php echo 'ran'; ?>\nmiddle\n<?php echo ; ?>\nafter\n",
    )];
    let run = run_templates("eval_include_parse_error", templates);
    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(!run.status.success(), "the include must fail: {stdout}");
    assert_eq!(stdout, "[late_parse_error:", "nothing of the file may run");
}
