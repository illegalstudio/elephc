//! Purpose:
//! Pins the instruction density of function frames: prologue length, slot addressing and symbol
//! access, so a return to the longer sequences fails a test instead of only growing the binary.
//!
//! Called from:
//! - `crate::ir_lower::tests` through the Rust test harness.
//!
//! Key details:
//! - Counts emitted AArch64 instructions in the user assembly of small fixtures; no timing.

fn aarch64_asm(source: &str) -> String {
    let module = super::lower_source_at_for_target(
        source,
        std::path::Path::new("main.php"),
        std::path::Path::new("."),
        crate::codegen::platform::Target::parse("macos-aarch64").unwrap(),
    );
    crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap()
}

/// The text of one emitted function, from its label to the next function label.
fn function_asm<'a>(asm: &'a str, label: &str) -> &'a str {
    let start = asm
        .find(&format!("\n{label}:\n"))
        .unwrap_or_else(|| panic!("{label} was not emitted"));
    let rest = &asm[start + 1..];
    let end = rest[label.len()..]
        .find("\n_fn_")
        .map(|at| at + label.len())
        .unwrap_or(rest.len());
    &rest[..end]
}

fn instructions(text: &str) -> Vec<&str> {
    text.lines()
        .map(str::trim)
        .filter(|line| {
            !line.is_empty() && !line.starts_with(';') && !line.starts_with('.') && !line.ends_with(':')
        })
        .collect()
}

/// A function that can throw and keeps several owned strings in its frame.
const STRING_FRAME: &str = r#"<?php
function build(string $a, string $b): string {
    $c = $a . "1"; $d = $b . "2"; $e = $c . $d; $f = $e . "3"; $g = $f . $a; $h = $g . $b;
    if ($h === "") { throw new Exception("empty"); }
    return $h . $e;
}
try { echo build("x", "y"); } catch (Exception $e) { echo "caught"; }
"#;

/// The activation record is published by one shared helper, not by twenty-odd inline
/// instructions per function.
#[test]
fn php_frame_activation_is_published_out_of_line() {
    let asm = aarch64_asm(STRING_FRAME);
    let body = function_asm(&asm, "_fn_build");
    assert!(
        body.contains("bl __rt_php_frame_publish"),
        "the prologue no longer calls the shared publish helper"
    );
    let prologue_end = body.find("; @block").expect("entry block marker");
    assert!(
        !body[..prologue_end].contains("_exc_call_frame_top") && !body[..prologue_end].contains("_php_site"),
        "the activation record is being written inline again"
    );
    let prologue = instructions(&body[..prologue_end]);
    assert!(
        prologue.len() <= 40,
        "prologue grew to {} instructions:\n{}",
        prologue.len(),
        prologue.join("\n")
    );
}

/// Adjacent cleanup slots are cleared two words at a time.
#[test]
fn cleanup_slots_are_zeroed_with_pair_stores() {
    let asm = aarch64_asm(STRING_FRAME);
    let body = function_asm(&asm, "_fn_build");
    let prologue_end = body.find("; @block").expect("entry block marker");
    let prologue = &body[..prologue_end];
    assert!(prologue.contains("stp xzr, xzr"), "no pair zeroing in:\n{prologue}");
    let single_zero_stores = prologue
        .lines()
        .filter(|line| line.trim().starts_with("stur xzr") || line.trim().starts_with("str xzr"))
        .count();
    assert!(single_zero_stores <= 2, "{single_zero_stores} single zero stores remain:\n{prologue}");
}

/// A global read or write is `adrp` plus one access with the page offset folded in, not
/// `adrp` + `add` + access.
#[test]
fn global_symbol_access_folds_the_page_offset() {
    let asm = aarch64_asm(STRING_FRAME);
    let body = function_asm(&asm, "_fn_build");
    assert!(
        !body.contains("add x9, x9, _concat_off@PAGEOFF"),
        "_concat_off is addressed with a separate add again"
    );
    assert!(body.contains("[x9, _concat_off@PAGEOFF]"));
}

/// The slots every statement touches sit within the one-instruction addressing range even in a
/// large function: the statement-reset base is read through `ldur`, never through a computed
/// address.
#[test]
fn statement_reset_base_stays_within_unscaled_offset_range() {
    let mut source = String::from("<?php\nfunction wide(string $s): string {\n");
    for index in 0..60 {
        source.push_str(&format!("    $v{index} = $s . \"{index}\";\n"));
    }
    source.push_str("    return $v0 . $v59;\n}\necho wide(\"a\"), \"\\n\";\n");
    let asm = aarch64_asm(&source);
    let body = function_asm(&asm, "_fn_wide");
    let resets: Vec<&str> = body
        .lines()
        .skip_while(|line| !line.contains("; @block"))
        .filter(|line| line.contains("op=concat_reset"))
        .collect();
    assert!(!resets.is_empty(), "the fixture emits no statement resets");
    let mut lines = body.lines().peekable();
    while let Some(line) = lines.next() {
        if line.contains("op=concat_reset") {
            let next = lines.peek().copied().unwrap_or("").trim();
            assert!(
                next.starts_with("ldur x10, [x29, #-"),
                "statement reset reads its base through a computed address: {next}"
            );
        }
    }
}

/// A call-operand scope opens and closes with one call each on AArch64: the entries reserve and
/// release the record themselves, so the caller emits no `sub sp` / `mov x0, sp` / `add sp`.
#[test]
fn call_operand_scope_is_one_call_to_open_and_one_to_close() {
    let source = r#"<?php
function take(array $a, string $s): int { return count($a) + strlen($s); }
function run(string $s): int {
    $parts = [$s, $s . "x"];
    return take($parts, $s . "y") + take([$s], $s);
}
echo run("a"), "\n";
"#;
    let asm = aarch64_asm(source);
    let body = function_asm(&asm, "_fn_run");
    assert!(
        body.contains("bl __rt_call_operand_push_owner") || body.contains("bl __rt_call_operand_push_descriptor"),
        "no call-operand scope in the fixture:\n{body}"
    );
    assert!(body.contains("bl __rt_call_operand_pop"));
    assert!(!body.contains("bl __rt_call_operand_unlink"), "the unlink form is back");
    assert!(!body.contains("sub sp, sp, #48"), "the caller reserves the record inline again");
    assert!(!body.contains("add sp, sp, #48"), "the caller releases the record inline again");
}

/// A function's epilogue cleanup is a slot-description table walked by one shared routine: the
/// cleanup point is an address plus one call whatever the slot count, and no per-slot release
/// code is emitted at the exit. Counted on a function with many owned locals and two returns.
#[test]
fn epilogue_cleanup_is_one_table_walk_per_exit() {
    let source = r#"<?php
function many(string $s, bool $early): string {
    $a = $s . "1"; $b = $s . "2"; $c = [$s, "x"]; $d = [$s => 1]; $e = $a . $b;
    if ($early) { return $e; }
    $f = $c[0] . $e;
    return $f . $a;
}
echo many("q", true), many("q", false), "\n";
"#;
    let asm = aarch64_asm(source);
    let body = function_asm(&asm, "_fn_many");
    let walks = body.matches("bl __rt_release_slot_table").count();
    assert!(walks >= 2, "each exit walks the table ({walks} found):\n{body}");
    // Every table walk is followed by the frame teardown, not by per-slot release calls.
    for (at, _) in body.match_indices("epilogue cleanup table of") {
        let after_walk: Vec<&str> = body[at..].lines().skip(1).take(8).collect();
        let released_inline = after_walk.iter().any(|line| {
            let line = line.trim();
            line.starts_with("bl __rt_heap_free_safe") || line.starts_with("bl __rt_decref_")
        });
        assert!(!released_inline, "release code follows a table walk:\n{}", after_walk.join("\n"));
    }
    assert!(
        !body.contains("epilogue cleanup run of") && !body.contains("bl __rt_release_slot_run"),
        "the same-kind run form is back:\n{body}"
    );
}
