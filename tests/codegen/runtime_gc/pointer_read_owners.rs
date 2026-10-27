//! Purpose:
//! Checks owning raw-memory string copies through the public and internal builtin aliases.
//!
//! Called from:
//! - The runtime GC integration suite and the dedicated fileinfo CI gates.
//!
//! Key details:
//! - Fileinfo returns internal pointer-read copies through fallible PHP methods.
//! - Returned bytes must survive source-buffer release and retire without a second copy leak.

use crate::support::*;

/// Pointer reads transfer independent copies while pointer-write counts retire borrowed inputs.
#[test]
fn test_fileinfo_pointer_read_string_alias_owners() {
    let source = r#"<?php
extern function malloc(int $size): ptr;
extern function free(ptr $p): void;
function fileinfoPublicCopy(ptr $p): string|false { return ptr_read_string($p, 3); }
class FileinfoPointerCopy {
    public function direct(ptr $p): string|false { return __elephc_ptr_read_string($p, 3); }
    public function local(ptr $p): string { $s = __elephc_ptr_read_string($p, 3); return $s; }
}
$p = malloc(3); ptr_write_string($p, "A\0B");
$reader = new FileinfoPointerCopy();
for ($i = 0; $i < 40; $i++) {
    $written = ptr_write_string($p, "A\0B");
    $internalWritten = __elephc_ptr_write_string($p, "A\0B");
    $a = fileinfoPublicCopy($p);
    $b = $reader->direct($p);
    $c = $reader->local($p);
    __elephc_ptr_read_string($p, 3);
}
free($p);
echo strlen($a), ":", strlen($b), ":", strlen($c), ":", ord($b[1]), ":", ord($c[2]), ":", $written + $internalWritten, "\n";
unset($a, $b, $c, $reader);
"#;
    let (out, _) = compile_and_run_with_heap_debug_and_asm(source);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "3:3:3:0:66:6\n", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
