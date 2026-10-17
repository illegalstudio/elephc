//! Purpose:
//! Checks dynamic-property hashes with one owner outside the managed graph.
//!
//! Called from:
//! - The incoming-edge cache runtime GC integration suite.
//!
//! Key details:
//! - Native shims retain the actual property hash, without creating another heap edge.
//! - PHP's array cast supplies the equivalent external root for the output oracle.
//! - Both optimizer modes run real collection and require balanced heap ownership.
//! - A conditional throw keeps the replaceable drop entry from being inlined.

use crate::support::*;

const SOURCE: &str = r#"<?php
#[AllowDynamicProperties]
class CacheDynamicNode {
    public mixed $fixed = null;
    public static int $freed = 0;
    public function __destruct() { self::$freed++; }
}
class CacheDynamicRoot { public static array $root = []; }
function cacheHashHold(CacheDynamicNode $value): int {
    CacheDynamicRoot::$root = (array)$value;
    return 1;
}
function cacheHashDrop(int $marker): int {
    if ($marker < 0) { throw new RuntimeException('fixture drop'); }
    CacheDynamicRoot::$root = [];
    return 0;
}
$node = new CacheDynamicNode();
$node->self = $node;
$marker = cacheHashHold($node);
unset($node);
gc_collect_cycles();
echo CacheDynamicNode::$freed, ':';
$marker = cacheHashDrop($marker);
gc_collect_cycles();
echo CacheDynamicNode::$freed;
"#;

/// An external hash owner roots its object's cycle until that owner is explicitly released.
#[test]
fn test_gc_incoming_cache_dynamic_hash_external_owner() {
    for mode in ["--ir-opt=off", "--ir-opt=on"] {
        let dir = make_cli_test_dir("gc_cache_dynamic_hash");
        let php = dir.join("main.php");
        fs::write(&php, SOURCE).unwrap();
        let compile = elephc_cli_command(&dir).args([mode, "--heap-debug", "--emit-asm"])
            .arg(&php).output().unwrap();
        assert!(compile.status.success(), "{}", String::from_utf8_lossy(&compile.stderr));
        let user = fs::read_to_string(dir.join("main.s")).unwrap();
        let drop_call = if target().arch == Arch::AArch64 {
            "bl _fn_cacheHashDrop"
        } else {
            "call _fn_cacheHashDrop"
        };
        assert!(user.contains(drop_call), "{mode}: the native drop fixture must not be inlined");
        let (_, runtime, requirements) = compile_source_to_asm_with_options(
            SOURCE, &dir, 8_388_608, false, true);
        let user = replace_body(&user, "cacheHashHold", &hold_hash());
        let mut user = replace_body(&user, "cacheHashDrop", &drop_hash());
        user.push_str("\n.data\n.p2align 3\n_gc_cache_test_hash:\n.quad 0\n");
        let output = assemble_and_run_capture(&user, &runtime_obj_for_asm(&runtime), &dir,
            &requirements, &default_link_paths(), &[]);
        let oracle = std::env::var_os("ELEPHC_PHP_CHECK").map(|_| {
            Command::new("php").arg("-n").arg(&php).output().unwrap()
        });
        fs::remove_dir_all(&dir).unwrap();
        assert!(output.success, "{mode}: {}\n{}", output.stdout, output.stderr);
        assert_eq!(output.stdout, "0:1", "{mode}: {}", output.stderr);
        assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{mode}: {}", output.stderr);
        if let Some(php) = oracle {
            assert!(php.status.success(), "{}", String::from_utf8_lossy(&php.stderr));
            assert_eq!(output.stdout.as_bytes(), php.stdout, "PHP mismatch with {mode}");
        }
    }
}

/// Replaces only one entry body, preserving its generated exceptional-cleanup callback.
fn replace_body(assembly: &str, name: &str, body: &str) -> String {
    let marker = format!("@fn name={name} symbol=");
    let begin = assembly.find(&marker).unwrap();
    let start = assembly[..begin].rfind('\n').map_or(0, |n| n + 1);
    let symbol = assembly[begin + marker.len()..].split_whitespace().next().unwrap();
    let end_marker = format!("@endfn name={name}");
    let end_begin = begin + assembly[begin..].find(&end_marker).unwrap();
    let end = end_begin + assembly[end_begin..].find('\n').unwrap() + 1;
    let cleanup = assembly[begin..end_begin].find("# exceptional PHP frame cleanup callback")
        .map(|n| assembly[..begin + n].rfind('\n').unwrap() + 1);
    format!("{}.text\n.globl {symbol}\n{symbol}:\n{body}{}{}", &assembly[..start],
        cleanup.map_or("", |n| &assembly[n..end_begin]), &assembly[end..])
}

/// Materializes a fixture data address with the host target's object-format relocations.
fn arm_address(reg: &str, symbol: &str) -> String {
    if target().platform == Platform::Linux {
        format!("    adrp {reg}, {symbol}\n    add {reg}, {reg}, :lo12:{symbol}\n")
    } else {
        format!("    adrp {reg}, {symbol}@PAGE\n    add {reg}, {reg}, {symbol}@PAGEOFF\n")
    }
}

/// Retains the object's actual tail hash once in non-heap fixture storage.
fn hold_hash() -> String {
    if target().arch == Arch::AArch64 {
        return format!(r#"
    // -- retain one external owner of the actual dynamic-property hash --
    stp x29, x30, [sp, #-32]!
    mov x29, sp
    ldr x10, [x0]
{}    ldr x10, [x11, x10, lsl #3]
    add x0, x0, x10
    ldr x0, [x0, #-8]
    str x0, [sp, #16]
    bl __rt_incref
    ldr x0, [sp, #16]
{}    str x0, [x11]
    mov x0, #1
    ldp x29, x30, [sp], #32
    ret
"#, arm_address("x11", "_class_object_payload_sizes"), arm_address("x11", "_gc_cache_test_hash"));
    }
    r#"
    # -- retain one external owner of the actual dynamic-property hash --
    push rbp
    mov rbp, rsp
    sub rsp, 16
    mov r10, QWORD PTR [rdi]
    lea r11, [rip + _class_object_payload_sizes]
    mov r10, QWORD PTR [r11 + r10 * 8]
    mov rax, QWORD PTR [rdi + r10 - 8]
    mov QWORD PTR [rsp], rax
    call __rt_incref
    mov rax, QWORD PTR [rsp]
    mov QWORD PTR [rip + _gc_cache_test_hash], rax
    mov eax, 1
    leave
    ret
"#.into()
}

/// Releases exactly the external owner, leaving cycle reclamation to the real collector.
fn drop_hash() -> String {
    if target().arch == Arch::AArch64 {
        return format!(r#"
    // -- release the external owner without adding a managed heap edge --
    stp x29, x30, [sp, #-16]!
    mov x29, sp
{}    ldr x0, [x11]
    str xzr, [x11]
    bl __rt_decref_hash
    mov x0, #0
    ldp x29, x30, [sp], #16
    ret
"#, arm_address("x11", "_gc_cache_test_hash"));
    }
    r#"
    # -- release the external owner without adding a managed heap edge --
    push rbp
    mov rbp, rsp
    mov rax, QWORD PTR [rip + _gc_cache_test_hash]
    mov QWORD PTR [rip + _gc_cache_test_hash], 0
    call __rt_decref_hash
    xor eax, eax
    pop rbp
    ret
"#.into()
}
