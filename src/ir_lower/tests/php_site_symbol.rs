//! Purpose:
//! Pins the C symbol of the published source-site record on every target.
//!
//! Called from:
//! - The AST-to-EIR regression test module.
//!
//! Key details:
//! - `elephc-magician` reads the record as the Rust extern `static php_site`, so the runtime's
//!   definition and every assembly reference must spell what the target's C ABI makes of
//!   `php_site`: `_php_site` on Mach-O, `php_site` on ELF. A literal `_php_site` linked on Apple
//!   only, and every Linux build that linked Magician failed on an undefined `php_site`.

/// Every spelling of the site record (not `_php_site_none` / `_php_site_eval`) in `asm`.
fn site_record_spellings(asm: &str) -> std::collections::BTreeSet<String> {
    let bytes = asm.as_bytes();
    let is_ident = |b: u8| b.is_ascii_alphanumeric() || b == b'_' || b == b'.' || b == b'$';
    let mut found = std::collections::BTreeSet::new();
    let mut search = 0;
    while let Some(offset) = asm[search..].find("php_site") {
        let start = search + offset;
        let end = start + "php_site".len();
        search = end;
        if end < bytes.len() && is_ident(bytes[end]) {
            continue;
        }
        let mut begin = start;
        while begin > 0 && bytes[begin - 1] == b'_' {
            begin -= 1;
        }
        if begin > 0 && is_ident(bytes[begin - 1]) {
            continue;
        }
        found.insert(asm[begin..end].to_string());
    }
    found
}

/// Checks that the runtime defines, and user code references, one target-mangled name.
fn verify(target_name: &str, expected: &str) {
    let target = crate::codegen_support::platform::Target::parse(target_name).unwrap();
    assert_eq!(crate::codegen_support::abi::php_site_symbol(&target), expected);
    let runtime = crate::codegen_support::generate_runtime(8_388_608, target);
    assert!(
        runtime.contains(&format!(".globl {expected}\n")) && runtime.contains(&format!("\n{expected}:\n")),
        "{target_name}: the runtime must define `{expected}`"
    );
    // A call to a user function publishes its site before entering the callee.
    let module = super::lower_source_at_for_target(
        r#"<?php
function callee(int $n): int { if ($n > 3) { throw new Exception("x"); } return $n + 1; }
try { echo callee(1), callee(9); } catch (Exception $e) { echo $e->getLine(); }
"#,
        std::path::Path::new("main.php"),
        std::path::Path::new("."),
        target,
    );
    let user = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    let spellings: std::collections::BTreeSet<String> = site_record_spellings(&runtime)
        .into_iter()
        .chain(site_record_spellings(&user))
        .collect();
    assert_eq!(
        spellings,
        std::collections::BTreeSet::from([expected.to_string()]),
        "{target_name}: every reference to the site record must use the target's C name"
    );
    assert!(
        site_record_spellings(&user).contains(expected),
        "{target_name}: the fixture must publish a call site, or this test checks nothing"
    );
}

/// Keeps every target independently scheduled under the CI timeout.
macro_rules! target_test {
    ($name:ident, $target:literal, $symbol:literal) => {
        /// Checks the site record's C symbol for this target.
        #[test]
        fn $name() { verify($target, $symbol); }
    };
}

target_test!(php_site_symbol_macos, "macos-aarch64", "_php_site");
target_test!(php_site_symbol_ios, "ios-arm64", "_php_site");
target_test!(php_site_symbol_ios_sim, "ios-sim-arm64", "_php_site");
target_test!(php_site_symbol_linux_arm, "linux-aarch64", "php_site");
target_test!(php_site_symbol_linux_x86, "linux-x86_64", "php_site");
