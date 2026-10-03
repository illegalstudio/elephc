//! Purpose:
//! End-to-end tests for `print_r()` of OBJECTS: the `C Object\n(\n    [p] => v\n)\n`
//! layout, PHP's visibility-annotated keys, objects nested in arrays and in other
//! objects, enum cases, `print_r($o, true)` return mode, and the `*RECURSION*`
//! guard.
//!
//! Called from:
//! - `cargo test --test print_r_object_tests` through Rust's test harness.
//!
//! Key details:
//! - REGRESSION ANCHOR (issue C5): `print_r()` of an object was a HARD COMPILE
//!   ERROR — `unsupported EIR backend feature: print_r for PHP type Object("V")`
//!   — and an object nested inside an array rendered as NOTHING at all (tag 6 fell
//!   through to `__rt_pr_val_done`). `top_level_object` and `object_in_array` are
//!   those repros.
//! - EXPECTATIONS ARE REFERENCE PHP'S OUTPUT, BYTE FOR BYTE, taken from PHP
//!   8.4.20 (`php -d xdebug.mode=off`) on the same program.
//! - THE INDENT RULE PHP USES, and what these tests pin: `print_hash(indent)`
//!   writes `(` and `)` at `indent`, entry lines at `indent + 4`, and renders a
//!   value with `indent + 8`. A container value therefore closes with its own
//!   `)\n` and the OUTER walker adds the per-entry `\n`, which is where PHP's
//!   blank line after a nested `)` comes from. `nested_object_in_object` and
//!   `array_property` are the shape tests for that.
//! - Enum cases print `E Enum` / `E Enum:int` / `E Enum:string` — a DIFFERENT
//!   header from `var_dump`'s `enum(E::C)` — with `name` before `value`. elephc
//!   stores a backed enum with `value` first, so the display order is fixed in the
//!   descriptor (`hoist_enum_name_row`), which `backed_enum_case` pins.
//! - Tests invoke the elephc CLI (CARGO_BIN_EXE_elephc) as a subprocess in an
//!   isolated temp dir, compile a plain executable, run it, and assert stdout —
//!   the same harness style as `var_dump_object_tests`. Host-target only.
//! - Compile STDERR is filtered to elephc's OWN diagnostics: on Linux, GNU `ld`
//!   adds static-glibc and `.note.GNU-stack` warnings that Apple's linker never
//!   emits, so an unfiltered assertion would be non-portable.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static TEST_ID: AtomicUsize = AtomicUsize::new(0);

/// Creates an isolated temp dir unique across parallel test threads/processes.
fn make_test_dir(prefix: &str) -> PathBuf {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let tid = std::thread::current().id();
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("{}_{}_{:?}_{}", prefix, pid, tid, id));
    fs::create_dir_all(&dir).unwrap();
    dir
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

/// Keeps only elephc's own diagnostics from a compile's stderr.
///
/// Linking also surfaces the HOST linker's warnings, which are environmental
/// rather than anything elephc emitted: GNU `ld` reports static-glibc notes and
/// the `.note.GNU-stack` deprecation, while Apple's linker stays silent.
fn elephc_diagnostics(stderr: &str) -> String {
    stderr
        .lines()
        .filter(|line| {
            line.starts_with("Warning: ")
                || line.starts_with("warning:")
                || line.starts_with("warning[")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Compiles `source`, runs the executable and returns its STDOUT.
///
/// Asserts a clean compile and a clean exit first: an object walker that reads a
/// wrong-shaped slot shows up as a signal rather than as bad text, and an
/// unguarded recursive walker blows the stack instead of producing a wrong string,
/// so the status assertions are load-bearing.
fn run_php(stem: &str, source: &str) -> String {
    let dir = make_test_dir("elephc_print_r_object");
    let php = dir.join(format!("{}.php", stem));
    fs::write(&php, source).unwrap();

    let mut cmd = Command::new(elephc_bin());
    cmd.arg("build");
    cmd.env("XDG_CACHE_HOME", dir.join("cache-root"));
    cmd.current_dir(&dir);
    cmd.arg("-q");
    cmd.arg(&php);
    let compile = cmd.output().expect("failed to spawn elephc");
    let raw_stderr = String::from_utf8_lossy(&compile.stderr).into_owned();
    assert!(
        compile.status.success(),
        "elephc compile failed:\n{raw_stderr}"
    );
    let diagnostics = elephc_diagnostics(&raw_stderr);
    assert!(
        diagnostics.is_empty(),
        "unexpected elephc diagnostics:\n{diagnostics}"
    );

    let out = run_binary(&dir.join(stem));
    let _ = fs::remove_dir_all(&dir);
    out
}

/// Runs a compiled executable and returns its STDOUT, asserting a clean exit.
fn run_binary(bin: &Path) -> String {
    let output = Command::new(bin).output().expect("failed to run compiled binary");
    assert!(
        output.status.success(),
        "compiled binary exited non-zero ({:?}):\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// The headline repro: `print_r(new V())` was a hard compile error.
///
/// Also pins the mixed-scalar body — a null-valued property renders as the empty
/// string after `=> `, which leaves a TRAILING SPACE on that line in PHP.
#[test]
fn top_level_object() {
    let out = run_php(
        "pr_top_level_object",
        concat!(
            "<?php\n",
            "class V { public $x = 1; public $s = 'hi'; public $f = 1.5; ",
            "public $b = true; public $n = null; }\n",
            "print_r(new V());\n",
        ),
    );
    assert_eq!(
        out,
        concat!(
            "V Object\n",
            "(\n",
            "    [x] => 1\n",
            "    [s] => hi\n",
            "    [f] => 1.5\n",
            "    [b] => 1\n",
            "    [n] => \n",
            ")\n",
        )
    );
}

/// An object with NO declared properties. Guards the zero-row walk: PHP still
/// writes both paren lines.
#[test]
fn empty_object() {
    let out = run_php(
        "pr_empty_object",
        "<?php class E {}\nprint_r(new E());\n",
    );
    assert_eq!(out, "E Object\n(\n)\n");
}

/// `new stdClass` with no properties — the same empty body under the built-in
/// class name.
#[test]
fn empty_stdclass() {
    let out = run_php("pr_stdclass", "<?php print_r(new stdClass);\n");
    assert_eq!(out, "stdClass Object\n(\n)\n");
}

/// PHP annotates non-public keys: `[b:protected]` and `[c:DeclaringClass:private]`
/// — unquoted, unlike `var_dump`'s `["b":protected]`.
#[test]
fn visibility_annotated_keys() {
    let out = run_php(
        "pr_visibility",
        "<?php class P { public $a = 1; protected $b = 2; private $c = 3; }\nprint_r(new P());\n",
    );
    assert_eq!(
        out,
        concat!(
            "P Object\n",
            "(\n",
            "    [a] => 1\n",
            "    [b:protected] => 2\n",
            "    [c:P:private] => 3\n",
            ")\n",
        )
    );
}

/// An UNINITIALIZED typed property is OMITTED from `print_r` entirely — unlike
/// `var_dump`, which lists it as `uninitialized(int)`.
#[test]
fn uninitialized_typed_property_is_omitted() {
    let out = run_php(
        "pr_uninitialized",
        "<?php class U { public int $t; public $d = 1; }\nprint_r(new U());\n",
    );
    assert_eq!(out, "U Object\n(\n    [d] => 1\n)\n");
}

/// The second repro: an object nested in an INDEXED array rendered as nothing.
///
/// Pins PHP's nesting layout — the header follows `=> ` on the entry line, the
/// parens sit at `entry indent + 4`, and the entry's own `\n` after the nested
/// `)\n` produces the blank line.
#[test]
fn object_in_array() {
    let out = run_php(
        "pr_object_in_array",
        concat!(
            "<?php\n",
            "class V { public $x = 1; }\n",
            "print_r([0, 'k' => new V()]);\n",
        ),
    );
    assert_eq!(
        out,
        concat!(
            "Array\n",
            "(\n",
            "    [0] => 0\n",
            "    [k] => V Object\n",
            "        (\n",
            "            [x] => 1\n",
            "        )\n",
            "\n",
            ")\n",
        )
    );
}

/// An object property holding another OBJECT: two levels of the same frame.
#[test]
fn nested_object_in_object() {
    let out = run_php(
        "pr_nested_object",
        concat!(
            "<?php\n",
            "class V { public $x = 1; }\n",
            "class Outer { public $inner; function __construct() { $this->inner = new V(); } }\n",
            "print_r(new Outer());\n",
        ),
    );
    assert_eq!(
        out,
        concat!(
            "Outer Object\n",
            "(\n",
            "    [inner] => V Object\n",
            "        (\n",
            "            [x] => 1\n",
            "        )\n",
            "\n",
            ")\n",
        )
    );
}

/// An object property holding an ARRAY: the other direction of the same nesting,
/// proving the object walker hands the array walker the right base indent.
#[test]
fn array_property() {
    let out = run_php(
        "pr_array_property",
        "<?php class W { public $arr = [1, 2, 3]; }\nprint_r(new W());\n",
    );
    assert_eq!(
        out,
        concat!(
            "W Object\n",
            "(\n",
            "    [arr] => Array\n",
            "        (\n",
            "            [0] => 1\n",
            "            [1] => 2\n",
            "            [2] => 3\n",
            "        )\n",
            "\n",
            ")\n",
        )
    );
}

/// A PURE enum case prints `E Enum` with only its `name`.
#[test]
fn pure_enum_case() {
    let out = run_php(
        "pr_enum_pure",
        "<?php enum Status { case Active; }\nprint_r(Status::Active);\n",
    );
    assert_eq!(out, "Status Enum\n(\n    [name] => Active\n)\n");
}

/// A BACKED enum case prints the backing type in the header and `name` BEFORE
/// `value`. elephc's storage order is the opposite, so this pins the display
/// reordering rather than an accident of layout.
#[test]
fn backed_enum_case() {
    let out = run_php(
        "pr_enum_backed",
        concat!(
            "<?php\n",
            "enum Suit: string { case Hearts = 'H'; }\n",
            "enum Lvl: int { case Low = 3; }\n",
            "print_r(Suit::Hearts);\n",
            "print_r(Lvl::Low);\n",
        ),
    );
    assert_eq!(
        out,
        concat!(
            "Suit Enum:string\n",
            "(\n",
            "    [name] => Hearts\n",
            "    [value] => H\n",
            ")\n",
            "Lvl Enum:int\n",
            "(\n",
            "    [name] => Low\n",
            "    [value] => 3\n",
            ")\n",
        )
    );
}

/// `print_r($o, true)` return mode: the same bytes, handed back as a string
/// instead of written to stdout. The length is asserted too, so a capture buffer
/// that truncated or over-copied could not pass on the text alone.
#[test]
fn return_mode_captures_the_object_body() {
    let out = run_php(
        "pr_object_return_mode",
        concat!(
            "<?php\n",
            "class P { public $a = 1; protected $b = 2; private $c = 3; }\n",
            "$s = print_r(new P(), true);\n",
            "echo strlen($s), \"\\n\";\n",
            "echo $s;\n",
        ),
    );
    assert_eq!(
        out,
        concat!(
            "72\n",
            "P Object\n",
            "(\n",
            "    [a] => 1\n",
            "    [b:protected] => 2\n",
            "    [c:P:private] => 3\n",
            ")\n",
        )
    );
}

/// A SELF-REFERENTIAL object renders ` *RECURSION*` after the header instead of
/// recursing forever. PHP writes the class name and ` Object\n` FIRST, then the
/// marker, and the entry line's own `\n` terminates it.
#[test]
fn self_reference_renders_the_recursion_marker() {
    let out = run_php(
        "pr_recursion",
        concat!(
            "<?php\n",
            "class R { public $a = 1; public ?R $self = null; }\n",
            "$r = new R();\n",
            "$r->self = $r;\n",
            "print_r($r);\n",
        ),
    );
    assert_eq!(
        out,
        concat!(
            "R Object\n",
            "(\n",
            "    [a] => 1\n",
            "    [self] => R Object\n",
            " *RECURSION*\n",
            ")\n",
        )
    );
}

/// TWO SIBLING references to the same instance must BOTH render in full: PHP
/// marks an object only for the duration of its own body, so a guard that pushed
/// and never popped would turn the second one into `*RECURSION*`.
#[test]
fn sibling_references_both_render_in_full() {
    let out = run_php(
        "pr_siblings",
        concat!(
            "<?php\n",
            "class V { public $x = 1; }\n",
            "$v = new V();\n",
            "print_r([$v, $v]);\n",
        ),
    );
    assert_eq!(
        out,
        concat!(
            "Array\n",
            "(\n",
            "    [0] => V Object\n",
            "        (\n",
            "            [x] => 1\n",
            "        )\n",
            "\n",
            "    [1] => V Object\n",
            "        (\n",
            "            [x] => 1\n",
            "        )\n",
            "\n",
            ")\n",
        )
    );
}
