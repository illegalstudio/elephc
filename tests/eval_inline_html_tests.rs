//! Purpose:
//! Pins that a `?>` in eval'd code switches to inline HTML, as in a PHP file, with every shape
//! measured on reference PHP 8.5.10.
//!
//! Called from:
//! - `cargo test --test eval_inline_html_tests` through Rust's test harness.
//!
//! Key details:
//! - The eval lexer lexed `?>` as `?` then `>`, and `parse_fragment` refused any fragment
//!   containing `<?`, strings included: every eval'd close tag was a parse error, and so was
//!   `echo "<?xml";`. The lexer now turns `?>` into `;`, the HTML into `echo "…";` and `<?=`
//!   into `echo` (`crates/elephc-magician/src/lexer/inline_html.rs`), and the parser accepts
//!   the empty statement that makes.
//! - The sources are read from a file at run time, so the compiler never sees them and every
//!   case goes through the eval interpreter.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
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
    fs::create_dir_all(&dir).unwrap();
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

/// One eval source per line; `\n` and `\r` stand for the newline bytes a case needs.
const CASES: &[&str] = &[
        "echo \"a\"; ?>HTML<?php echo \"b\";",
        "?>X<?php echo 1;",
        "echo 1; ?>tail",
        "echo 1; ?>\\nnext",
        "echo 1; ?>\\r\\nnext",
        "echo 1; ?>\\n\\nnext",
        "if (1) { ?>IN<?php } echo \"!\";",
        "if (0) { ?>NO<?php } else { ?>YES<?php }",
        "foreach ([1, 2] as $i) { ?>[<?= $i ?>]<?php }",
        "echo 1 ?>X",
        "// c ?>X",
        "# c ?>Y<?php echo \"Z\";",
        "$s = \"?>\"; echo $s;",
        "$s = '?>'; echo $s;",
        "?>",
        "?>only",
        "echo \"a\"; ?>B<?PHP echo \"c\";",
        "echo \"a\"; ?>B<?php\\necho \"c\";",
        "return 5; ?>after",
        "?>A<?php return 7;",
        "?>A<?= 1 + 2 ?>B",
        "echo \"x\"; /* ?> */ echo \"y\";",
        "echo \"<?xml\";",
        "echo 'a<?b';",
        "echo 1;; echo 2;",
        "while (false); echo \"w\";",
        "echo \"one\\n\"; // note ?> between <?php echo \"two\\n\";",
        "echo \"one\\n\"; # note ?> between <?php echo \"two\\n\";",
        "echo \"a\"; ?>B<?php\u{a0}echo \"c\";",
];

/// The harness: evaluates each line and prints its output and return value, delimited.
const HARNESS: &str = r#"<?php
$cases = file(__DIR__ . "/cases.txt", FILE_IGNORE_NEW_LINES);
foreach ($cases as $i => $case) {
    $code = str_replace("\\r", "\r", str_replace("\\n", "\n", $case));
    echo "[$i:";
    $r = eval($code);
    echo "|", var_export($r, true), "]\n";
}
echo "END\n";
"#;

/// Every close-tag shape prints what reference PHP 8.5.10 prints (MEASURED with this harness):
/// HTML between and around code, the one newline `?>` swallows (LF, CRLF, not a second one),
/// blocks and loops spanning the tags, `<?=`, `?>` standing for `;`, comments ending at `?>`,
/// `?>` inside strings staying text, `<?PHP`, a `return` before trailing HTML, and `<?` inside
/// a string, which the old blanket refusal rejected. `<?php` before a no-break space (U+00A0)
/// stays HTML: the scanner takes only an ASCII space, tab or line break as the separator
/// (MEASURED with `short_open_tag=0`, the production setting).
#[test]
fn a_close_tag_in_eval_switches_to_inline_html_as_php_does() {
    let dir = make_test_dir("eval_inline_html");
    fs::write(dir.join("main.php"), HARNESS).unwrap();
    fs::write(dir.join("cases.txt"), CASES.join("\n") + "\n").unwrap();
    let output = Command::new(elephc_bin())
        .env("XDG_CACHE_HOME", dir.join("cache-root"))
        .current_dir(&dir)
        .arg(dir.join("main.php"))
        .output()
        .expect("failed to spawn elephc");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let run = Command::new(dir.join("main")).output().expect("failed to run binary");
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        concat!(
            "[0:aHTMLb|NULL]\n",
            "[1:X1|NULL]\n",
            "[2:1tail|NULL]\n",
            "[3:1next|NULL]\n",
            "[4:1next|NULL]\n",
            "[5:1\n",
            "next|NULL]\n",
            "[6:IN!|NULL]\n",
            "[7:YES|NULL]\n",
            "[8:[1][2]|NULL]\n",
            "[9:1X|NULL]\n",
            "[10:X|NULL]\n",
            "[11:YZ|NULL]\n",
            "[12:?>|NULL]\n",
            "[13:?>|NULL]\n",
            "[14:|NULL]\n",
            "[15:only|NULL]\n",
            "[16:aBc|NULL]\n",
            "[17:aBc|NULL]\n",
            "[18:|5]\n",
            "[19:A|7]\n",
            "[20:A3B|NULL]\n",
            "[21:xy|NULL]\n",
            "[22:<?xml|NULL]\n",
            "[23:a<?b|NULL]\n",
            "[24:12|NULL]\n",
            "[25:w|NULL]\n",
            "[26:one\n",
            " between two\n",
            "|NULL]\n",
            "[27:one\n",
            " between two\n",
            "|NULL]\n",
            "[28:aB<?php\u{a0}echo \"c\";|NULL]\n",
            "END\n",
        )
    );
}
