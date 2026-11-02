//! Purpose:
//! Guards address-taken frame-reader entries against Mach-O relocation addend limits.
//!
//! Called from:
//! - The EIR lowering unit suite on every supported target.
//!
//! Key details:
//! - Each function's reader (`<entry>__bt_reader`, published by its prologue) is a file-local
//!   real symbol, distinct from temporary branch labels.
//! - Native Apple assembly also checks an entry beyond the signed 24-bit addend range.

use crate::codegen::platform::Target;

/// Frame-reader addresses remain representable even after a very large enclosing PHP frame.
#[test]
fn backtrace_reader_entries_have_real_local_symbols_on_all_targets() {
    let source = r#"<?php
        function tracedFrame(int $value): array { return debug_backtrace(); }
        echo count(tracedFrame($argc));
    "#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        let module = super::lower_source_at_for_target(source,
            std::path::Path::new("main.php"), std::path::Path::new("."), target);
        let asm = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        let entries: Vec<_> = asm.lines().filter_map(|line| line.trim().strip_suffix(':'))
            .filter(|label| label.ends_with("__bt_reader")).collect();
        assert!(!entries.is_empty(), "{name}");
        for entry in entries {
            assert!(!entry.starts_with(target.platform.local_label_prefix()), "{name}: {entry}");
            assert!(!asm.contains(&format!(".globl {entry}\n")), "{name}: {entry}");
            if cfg!(target_os = "macos")
                && target.platform == crate::codegen::platform::Platform::MacOS
            {
                assemble_distant_reader(entry);
            }
        }
    }
}

/// The Apple assembler must address a generated reader more than eight MiB into its caller.
fn assemble_distant_reader(entry: &str) {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let source = format!(
        ".text\n.globl _large_reader_test\n_large_reader_test:\n.space 9000000\n\
         adrp x0, {entry}@PAGE\nadd x0, x0, {entry}@PAGEOFF\n\
         b Lreader_done\n{entry}:\nret\nLreader_done:\nret\n"
    );
    let mut child = Command::new("as").args(["-arch", "arm64", "-o", "/dev/null", "-"])
        .stdin(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
    child.stdin.take().unwrap().write_all(source.as_bytes()).unwrap();
    let result = child.wait_with_output().unwrap();
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
}
