use super::*;
use crate::codegen_support::platform::Target;

#[test]
fn pcntl_dispatch_queue_status_uses_c_int_width_on_every_target() {
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        let mut emitter = Emitter::new(target);
        match target.arch {
            Arch::AArch64 => emit_pcntl_dispatch_pending_aarch64(&mut emitter),
            Arch::X86_64 => emit_pcntl_dispatch_pending_x86_64(&mut emitter),
        }
        let asm = emitter.output();
        let expected = match target.arch {
            Arch::AArch64 => "cmp w0, #0",
            Arch::X86_64 => "test eax, eax",
        };
        let queue_call = match target.arch {
            Arch::AArch64 => asm.find("__rt_pcntl_dispatch_loop:").unwrap(),
            Arch::X86_64 => asm.find("__rt_pcntl_dispatch_loop_x86:").unwrap(),
        };
        let queue_read = &asm[queue_call..];
        let call = match target.arch {
            Arch::AArch64 => queue_read.find("blr x9").unwrap(),
            Arch::X86_64 => queue_read.find("call r9").unwrap(),
        };
        assert!(queue_read[call..].lines().nth(1).unwrap().contains(expected), "{name}: {queue_read}");
    }
}

/// The async safe point dispatches only once a handler raised the bridge's pending flag, as php
/// dispatches only behind its interrupt flag: the flag word is read before the branch to the
/// preserving dispatcher on AArch64, and before the complete spill on x86_64. Each safe point used
/// to dispatch unconditionally while async signals were on (two `sigprocmask` and a pipe read).
#[test]
fn pcntl_async_safe_point_reads_the_pending_flag_before_dispatching() {
    let mut emitter = Emitter::new(Target::parse("macos-aarch64").unwrap());
    emit_pcntl_safe_point_aarch64(&mut emitter);
    let asm = emitter.output();
    let flag = asm.find(PENDING_FLAG_SLOT).expect("the AArch64 safe point reads the pending flag");
    let dispatch = asm.find("b __rt_pcntl_async_dispatch_preserving").unwrap();
    assert!(flag < dispatch, "{asm}");

    let mut emitter = Emitter::new(Target::parse("linux-x86_64").unwrap());
    emit_pcntl_async_dispatch_preserving_x86_64(&mut emitter);
    let asm = emitter.output();
    let flag = asm.find(PENDING_FLAG_SLOT).expect("the x86_64 safe point reads the pending flag");
    let spill = asm.find("__rt_pcntl_async_slow_x86:").unwrap();
    assert!(flag < spill, "{asm}");
}

/// Execute the emitted boundary against a real C function returning int -1.
/// Both ABIs may leave the upper 32 bits zero; reading a 64-bit status would
/// therefore consume uninitialized siginfo instead of returning false.
#[test]
#[cfg(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "aarch64", target_arch = "x86_64"))
))]
fn pcntl_dispatch_queue_error_is_false_without_reading_siginfo() {
    use std::{fs, process::Command};
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    struct Scratch(std::path::PathBuf);
    impl Drop for Scratch {
        fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); }
    }
    let path = std::env::temp_dir().join(format!("elephc-pcntl-status-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
    fs::create_dir(&path).unwrap();
    let scratch = Scratch(path);
    let name = if cfg!(target_os = "macos") { "macos-aarch64" }
        else if cfg!(target_arch = "aarch64") { "linux-aarch64" }
        else { "linux-x86_64" };
    let target = Target::parse(name).unwrap();
    let mut emitter = Emitter::new(target);
    if target.arch == Arch::X86_64 { emitter.raw(".intel_syntax noprefix"); }
    emitter.raw(".text");
    match target.arch {
        Arch::AArch64 => emit_pcntl_dispatch_pending_aarch64(&mut emitter),
        Arch::X86_64 => emit_pcntl_dispatch_pending_x86_64(&mut emitter),
    }
    let assembly = scratch.0.join("dispatch.s");
    let harness = scratch.0.join("harness.c");
    let executable = scratch.0.join("probe");
    fs::write(&assembly, emitter.output()).unwrap();
    fs::write(&harness, r#"
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
uint64_t dispatching __asm__("__rt_pcntl_dispatching") = 0;
unsigned char mask[256] __asm__("__rt_pcntl_dispatch_mask");
uint64_t kinds[128] __asm__("__rt_pcntl_handler_kind");
uint64_t descriptors[128] __asm__("__rt_pcntl_handler_descriptor");
static volatile int mode = -1;
static int pops, ends;
static int begin(void *p) { (void)p; return 1; }
static int end(void *p) { (void)p; ++ends; return 1; }
static int next(void *p, int owner) {
    if (owner != 1) abort();
    ++pops;
    if (mode < 0) return -1; /* deliberately leave the record untouched */
    if (mode == 0 || pops > 1) return 0;
    *(int64_t *)p = 1;
    return 1;
}
int (*begin_fn)(void *) __asm__("__rt_pcntl_dispatch_begin_fn") = begin;
int (*end_fn)(void *) __asm__("__rt_pcntl_dispatch_end_fn") = end;
int (*next_fn)(void *, int) __asm__("__rt_pcntl_signal_next_fn") = next;
void invoke(void) __asm__("__rt_pcntl_invoke_descriptor");
void invoke(void) { abort(); }
long dispatch(void) __asm__("__rt_pcntl_dispatch_pending");
int main(void) {
    if (dispatch() != 0 || ends != 1 || pops != 1 || dispatching) return 11;
    mode = 0; pops = 0;
    if (dispatch() != 1 || ends != 2 || pops != 1 || dispatching) return 12;
    mode = 1; pops = 0;
    if (dispatch() != 1 || ends != 3 || pops != 2 || dispatching) return 13;
    dispatching = 1; pops = 0;
    if (dispatch() != 1 || ends != 3 || pops != 0 || dispatching != 1) return 14;
    puts("error|empty|record|reentrant");
    return 0;
}
"#).unwrap();
    let build = Command::new("cc").arg("-O2").arg(&harness).arg(&assembly).arg("-o").arg(&executable).output().unwrap();
    assert!(build.status.success(), "{}", String::from_utf8_lossy(&build.stderr));
    let run = Command::new(executable).output().unwrap();
    assert!(run.status.success(), "status {:?}, stderr {}", run.status, String::from_utf8_lossy(&run.stderr));
    assert_eq!(String::from_utf8_lossy(&run.stdout), "error|empty|record|reentrant\n");
}
