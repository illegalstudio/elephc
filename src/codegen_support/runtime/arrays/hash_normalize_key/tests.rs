//! Purpose:
//! Checks exact string normalization and key identity on every supported target.
//!
//! Called from:
//! - Focused native key-policy probes and optional cross-assembler validation.
//!
//! Key details:
//! - Real helper code executes canonical, binary and colliding integer/string keys.
//! - Native C shims adapt the internal helper ABI without replacing its semantics.

use super::*;
use crate::codegen_support::platform::{Platform, Target};
use elephc_semantics::{SemanticProfile, StrictFeature};
use std::{io::Write, path::PathBuf, process::{Command, Stdio}};

/// Emits real normalization, hashing and equality helpers plus C-compatible probes.
fn assembly(target: Target, exact: bool) -> String {
    let mut emitter = Emitter::new(target);
    emitter.semantic_profile = SemanticProfile::PERMISSIVE.with(StrictFeature::ArrayKeys, exact);
    emit_hash_normalize_key(&mut emitter);
    crate::codegen_support::runtime::arrays::emit_hash_fnv1a(&mut emitter);
    crate::codegen_support::runtime::arrays::emit_hash_key_hash(&mut emitter);
    crate::codegen_support::runtime::arrays::emit_hash_key_eq(&mut emitter);
    crate::codegen_support::runtime::strings::emit_str_eq(&mut emitter);
    probes(&mut emitter);
    let prefix = if target.arch == Arch::X86_64 { ".intel_syntax noprefix\n.text\n" } else { ".text\n" };
    let suffix = if target.platform == Platform::Linux { ".section .note.GNU-stack,\"\",@progbits\n" } else { "" };
    format!("{prefix}{}{suffix}", emitter.output())
}

/// Names probe entry points using the native C symbol convention.
fn c_symbol(target: Target, name: &str) -> String {
    if target.platform == Platform::MacOS { format!("_{name}") } else { name.into() }
}

/// Emits one reviewed instruction with its specific ABI explanation.
fn shim(emitter: &mut Emitter, instruction: &str, note: &str) {
    emitter.comment(note);
    emitter.instruction(instruction);                                           // adapt one probe operand or return register to the native C ABI
}

/// Adapts pointer/length and aggregate arguments to the existing internal key registers.
fn probes(emitter: &mut Emitter) {
    for (name, target) in [("key_normalize_probe", "__rt_hash_normalize_key"),
        ("key_hash_probe", "__rt_hash_key_hash"), ("key_equal_probe", "__rt_hash_key_eq")] {
        emitter.label_global(&c_symbol(emitter.target, name));
        if emitter.target.arch == Arch::X86_64 {
            if name == "key_normalize_probe" {
                shim(emitter, "mov rax, rdi", "move the C string pointer into the internal low key word");
                shim(emitter, "mov rdx, rsi", "move the C byte length into the internal high key word");
            }
            shim(emitter, &format!("jmp {target}"), "tail-call the helper with SysV aggregate return registers");
        } else {
            shim(emitter, "stp x29, x30, [sp, #-16]!", "preserve the C frame and return address through the internal call");
            shim(emitter, "mov x29, sp", "establish an aligned probe frame");
            if name == "key_equal_probe" {
                shim(emitter, "mov x4, x3", "shift the right key high word before its source is overwritten");
                shim(emitter, "mov x3, x2", "shift the right key low word into its internal argument register");
            }
            shim(emitter, "mov x2, x1", "shift the left key high word or string length");
            shim(emitter, "mov x1, x0", "shift the left key low word or string pointer");
            shim(emitter, &format!("bl {target}"), "execute the actual target key helper");
            if name == "key_normalize_probe" {
                shim(emitter, "mov x0, x1", "return the normalized low word in the first C aggregate register");
                shim(emitter, "mov x1, x2", "return the normalized high word in the second C aggregate register");
            }
            shim(emitter, "ldp x29, x30, [sp], #16", "restore the caller frame and return address");
            shim(emitter, "ret", "return the key aggregate, hash or equality result to C");
        }
    }
}

/// Owns one isolated probe directory and removes only those explicitly created files.
struct Scratch(PathBuf);

impl Scratch {
    /// Creates an isolated target directory for this one native probe invocation.
    fn new() -> Self {
        let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target")
            .join(format!("strict-array-key-probe-{}-{nonce}", std::process::id()));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for Scratch {
    /// Cleans the task-owned directory on passing and failing assertions alike.
    fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0); }
}

/// Real helpers preserve string bytes and distinguish keys even when full hashes collide.
#[test]
#[cfg(any(all(target_os = "linux", target_arch = "x86_64"),
    all(target_arch = "aarch64", any(target_os = "linux", target_os = "macos"))))]
fn strict_exact_key_runtime_preserves_bytes_kinds_and_hash_collisions() {
    let name = if cfg!(target_os = "macos") { "macos-aarch64" }
        else if cfg!(target_arch = "aarch64") { "linux-aarch64" } else { "linux-x86_64" };
    let target = Target::parse(name).unwrap();
    let scratch = Scratch::new();
    let driver = scratch.0.join("probe.c");
    std::fs::write(&driver, DRIVER).unwrap();
    for exact in [false, true] {
        let source = scratch.0.join(format!("keys-{exact}.s"));
        let binary = scratch.0.join(format!("keys-{exact}"));
        std::fs::write(&source, assembly(target, exact)).unwrap();
        let output = Command::new("cc").args(["-std=c11", "-Wall", "-Wextra", "-Werror"])
            .arg(&driver).arg(&source).arg("-o").arg(&binary).output().unwrap();
        assert!(output.status.success(), "{name}, exact={exact}: {}", String::from_utf8_lossy(&output.stderr));
        let output = Command::new(&binary).arg(if exact { "1" } else { "0" }).output().unwrap();
        assert!(output.status.success(), "{name}, exact={exact}: {}", String::from_utf8_lossy(&output.stderr));
        assert_eq!(output.stdout, b"keys:15;collision:distinct;bytes:exact\n");
    }
}

/// All supported ABIs assemble both PHP normalization and exact typed-key helpers.
#[test]
#[ignore = "requires clang with ELF and Apple AArch64 assembler support"]
fn strict_exact_key_runtime_assembles_on_every_target() {
    for (name, triple) in [("linux-x86_64", "x86_64-linux-gnu"), ("linux-aarch64", "aarch64-linux-gnu"),
        ("macos-aarch64", "arm64-apple-macos11"), ("ios-arm64", "arm64-apple-ios13"),
        ("ios-sim-arm64", "arm64-apple-ios13-simulator")] {
        for exact in [false, true] {
            let source = assembly(Target::parse(name).unwrap(), exact);
            let mut child = Command::new("clang").args(["-target", triple, "-x", "assembler", "-c", "-o", "/dev/null", "-"])
                .stdin(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
            child.stdin.take().unwrap().write_all(source.as_bytes()).unwrap();
            let output = child.wait_with_output().unwrap();
            assert!(output.status.success(), "{name}, exact={exact}: {}", String::from_utf8_lossy(&output.stderr));
        }
    }
}

/// Independent C assertions verify observable key identity through the real helper ABI.
const DRIVER: &str = r#"
#include <assert.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
struct key {uint64_t lo,hi;};
struct key key_normalize_probe(const unsigned char *,uint64_t);
uint64_t key_hash_probe(struct key);
uint64_t key_equal_probe(struct key,struct key);
struct sample {const unsigned char *bytes;uint64_t len;int integer;int64_t value;};
#define S(text,len,integer,value) {(const unsigned char *)text,len,integer,value}
int main(int argc,char **argv) {
    assert(argc==2);
    int exact=strcmp(argv[1],"1")==0;
    const struct sample samples[]={
        S("",0,0,0),S("0",1,1,0),S("1",1,1,1),S("-1",2,1,-1),
        S("01",2,0,0),S("-0",2,0,0),S("+1",2,0,0),S("1 ",2,0,0),
        S("9223372036854775807",19,1,INT64_MAX),S("-9223372036854775808",20,1,INT64_MIN),
        S("9223372036854775808",19,0,0),S("-9223372036854775809",20,0,0),
        S("a",1,0,0),S("\xff",1,0,0),S("1\0",2,0,0)
    };
    for(unsigned i=0;i<sizeof(samples)/sizeof(samples[0]);i++) {
        struct key result=key_normalize_probe(samples[i].bytes,samples[i].len);
        if(!exact && samples[i].integer) {
            assert(result.hi==UINT64_MAX && result.lo==(uint64_t)samples[i].value);
        } else {
            assert(result.hi==samples[i].len && result.lo==(uintptr_t)samples[i].bytes);
        }
    }
    struct key one=key_normalize_probe((const unsigned char *)"1",1);
    struct key integer_one={1,UINT64_MAX};
    assert((key_equal_probe(one,integer_one)!=0)==!exact);
    unsigned char a[]={97},other_a[]={97},embedded_b[]={97,0,98},embedded_c[]={97,0,99};
    struct key text={(uintptr_t)a,1},copy={(uintptr_t)other_a,1};
    assert(key_equal_probe(text,copy));
    assert(!key_equal_probe((struct key){(uintptr_t)embedded_b,3},(struct key){(uintptr_t)embedded_c,3}));
    uint64_t hash=key_hash_probe(text),multiplier=UINT64_C(0x9e3779b97f4a7c15),inverse=1;
    /* Invert the current integer-key mix to create a full hash collision. */
    for(unsigned i=0;i<6;i++) inverse*=2-multiplier*inverse;
    uint64_t payload=hash*inverse;payload^=payload>>33;
    struct key colliding_integer={payload,UINT64_MAX};
    assert(key_hash_probe(colliding_integer)==hash);
    assert(!key_equal_probe(text,colliding_integer));
    assert(!key_equal_probe(colliding_integer,text));
    puts("keys:15;collision:distinct;bytes:exact");
    return 0;
}
"#;
