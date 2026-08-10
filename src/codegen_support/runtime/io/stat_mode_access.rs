//! Purpose:
//! Emits `__rt_stat_mode_access`, the permission-triad predicate behind
//! `is_readable()`, `is_writable()` and `is_executable()` on a path a userspace
//! stream wrapper answered for.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via
//!   `crate::codegen_support::runtime::io`.
//! - `__rt_user_wrapper_url_stat_field` for its boolean selectors (3/4/5).
//!
//! Key details:
//! - PHP does not test the mode bits directly: it first picks ONE permission
//!   triad and then ignores the other two. The selection was read off reference
//!   PHP with a wrapper returning a synthetic `url_stat` mode/uid/gid, not
//!   inferred: `mode 0044 uid=me` answers `is_readable() === false` even though
//!   the group and world read bits are set, and `mode 0004 gid=<a supplementary
//!   group>` answers false even though the world read bit is set. The triad is
//!   the owner one when the reported uid is the process uid, else the group one
//!   when the reported gid is the process gid **or any of its supplementary
//!   groups**, else the world one.
//! - The identity compared against is the process's, from `getuid`/`getgid`/
//!   `getgroups` — not `getmyuid()`/`getmygid()`, which report the running
//!   script FILE's owner and silently differ (a script in `/tmp` is group 0).
//! - Raw syscalls rather than libc calls, so a program that never reaches this
//!   helper still imports nothing on its account.

use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Supplementary groups read in one `getgroups` call. macOS caps a process at
/// 16 (`NGROUPS_MAX`) and Linux at 65536; 64 covers every practical process
/// while keeping the scratch buffer a single stack frame.
const SUPPLEMENTARY_GROUP_CAPACITY: usize = 64;
/// Bytes of stack scratch for the supplementary-group list (`gid_t` is 32-bit).
const GROUP_BUFFER_BYTES: usize = SUPPLEMENTARY_GROUP_CAPACITY * 4;

/// Emits `__rt_stat_mode_access(mode, uid, gid, want)`.
///
/// `want` is the permission bit inside a triad: 4 read, 2 write, 1 execute.
/// Returns 1 when the selected triad grants it, 0 otherwise. Dispatches by
/// target.
pub fn emit_stat_mode_access(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_stat_mode_access_linux_x86_64(emitter);
        return;
    }
    emit_stat_mode_access_aarch64(emitter);
}

/// AArch64 implementation of `__rt_stat_mode_access`.
///
/// Inputs: x0 = mode, x1 = uid, x2 = gid, x3 = wanted permission bit.
/// Output: x0 = 1 when the selected triad grants the bit, else 0.
fn emit_stat_mode_access_aarch64(emitter: &mut Emitter) {
    // Frame: [sp,#0..16] x29/x30, [sp,#16] mode, [sp,#24] uid, [sp,#32] gid,
    //   [sp,#40] wanted bit, [sp,#48..] supplementary-group list.
    let group_buf = 48;
    let frame = (group_buf + GROUP_BUFFER_BYTES + 15) & !15;

    emitter.blank();
    emitter.comment("--- runtime: stat_mode_access ---");
    emitter.label_global("__rt_stat_mode_access");
    emitter.instruction(&format!("sub sp, sp, #{}", frame));                    // helper frame plus the supplementary-group scratch list
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #16]");                                   // save the stat mode across the identity syscalls
    emitter.instruction("str x1, [sp, #24]");                                   // save the reported owner uid
    emitter.instruction("str x2, [sp, #32]");                                   // save the reported owning gid
    emitter.instruction("str x3, [sp, #40]");                                   // save the requested permission bit (4 read, 2 write, 1 execute)

    // -- owner triad when the reported uid is the process uid --
    emitter.syscall(24);                                                        // getuid() → x0
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the reported owner uid
    emitter.instruction("cmp x0, x9");                                          // does the file claim to be ours?
    emitter.instruction("b.eq __rt_sma_owner");                                 // → the owner triad, and only it

    // -- group triad when the reported gid is the process gid --
    emitter.syscall(47);                                                        // getgid() → x0
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload the reported owning gid
    emitter.instruction("cmp x0, x9");                                          // does it match our primary group?
    emitter.instruction("b.eq __rt_sma_group");                                 // → the group triad

    // -- group triad when the reported gid is one of our supplementary groups --
    emitter.instruction(&format!("mov x0, #{}", SUPPLEMENTARY_GROUP_CAPACITY)); // entries the scratch list can hold
    emitter.instruction(&format!("add x1, sp, #{}", group_buf));                // scratch list for the supplementary groups
    emitter.syscall(79);                                                        // getgroups() → x0 = count, or -1 on failure
    emitter.instruction("cmp x0, #0");                                          // did we get any supplementary group?
    emitter.instruction("b.le __rt_sma_world");                                 // none (or the call failed) → the world triad
    emitter.instruction("mov x10, x0");                                         // supplementary-group count
    emitter.instruction("mov x11, #0");                                         // scan index
    emitter.instruction("ldr x12, [sp, #32]");                                  // the gid the wrapper reported
    emitter.instruction(&format!("add x13, sp, #{}", group_buf));               // base of the supplementary-group list
    emitter.label("__rt_sma_supp");
    emitter.instruction("cmp x11, x10");                                        // scanned every supplementary group?
    emitter.instruction("b.ge __rt_sma_world");                                 // no supplementary group matched → the world triad
    emitter.instruction("ldr w14, [x13, x11, lsl #2]");                         // load one supplementary gid (gid_t is 32-bit)
    emitter.instruction("cmp x14, x12");                                        // does it match the reported gid?
    emitter.instruction("b.eq __rt_sma_group");                                 // → the group triad
    emitter.instruction("add x11, x11, #1");                                    // advance the scan index
    emitter.instruction("b __rt_sma_supp");                                     // keep scanning

    // -- test the wanted bit inside the selected triad --
    emitter.label("__rt_sma_owner");
    emitter.instruction("mov x9, #6");                                          // the owner triad sits in bits 6..8
    emitter.instruction("b __rt_sma_test");
    emitter.label("__rt_sma_group");
    emitter.instruction("mov x9, #3");                                          // the group triad sits in bits 3..5
    emitter.instruction("b __rt_sma_test");
    emitter.label("__rt_sma_world");
    emitter.instruction("mov x9, #0");                                          // the world triad sits in bits 0..2
    emitter.label("__rt_sma_test");
    emitter.instruction("ldr x10, [sp, #16]");                                  // reload the stat mode
    emitter.instruction("lsr x10, x10, x9");                                    // shift the selected triad down to the low three bits
    emitter.instruction("ldr x11, [sp, #40]");                                  // reload the requested permission bit
    emitter.instruction("and x10, x10, x11");                                   // isolate it inside the selected triad
    emitter.instruction("cmp x10, #0");                                         // is the permission granted?
    emitter.instruction("cset x0, ne");                                         // materialize the PHP boolean

    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction(&format!("add sp, sp, #{}", frame));                    // release the helper frame
    emitter.instruction("ret");                                                 // return the permission predicate
}

/// x86_64 implementation of `__rt_stat_mode_access`.
///
/// Inputs: rdi = mode, rsi = uid, rdx = gid, rcx = wanted permission bit.
/// Output: rax = 1 when the selected triad grants the bit, else 0.
fn emit_stat_mode_access_linux_x86_64(emitter: &mut Emitter) {
    // Frame: [rbp-8] mode, [rbp-16] uid, [rbp-24] gid, [rbp-32] wanted bit,
    //   [rbp-<group_buf>] supplementary-group list.
    let group_buf = 40 + GROUP_BUFFER_BYTES;
    let frame = (group_buf + 15) & !15;

    emitter.blank();
    emitter.comment("--- runtime: stat_mode_access ---");
    emitter.label_global("__rt_stat_mode_access");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction(&format!("sub rsp, {}", frame));                        // helper frame plus the supplementary-group scratch list
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the stat mode across the identity syscalls
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the reported owner uid
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save the reported owning gid
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // save the requested permission bit (4 read, 2 write, 1 execute)

    // -- owner triad when the reported uid is the process uid --
    emitter.instruction("mov eax, 102");                                        // Linux getuid
    emitter.instruction("syscall");                                             // getuid() → rax
    emitter.instruction("cmp rax, QWORD PTR [rbp - 16]");                       // does the file claim to be ours?
    emitter.instruction("je __rt_sma_owner_x86");                               // → the owner triad, and only it

    // -- group triad when the reported gid is the process gid --
    emitter.instruction("mov eax, 104");                                        // Linux getgid
    emitter.instruction("syscall");                                             // getgid() → rax
    emitter.instruction("cmp rax, QWORD PTR [rbp - 24]");                       // does it match our primary group?
    emitter.instruction("je __rt_sma_group_x86");                               // → the group triad

    // -- group triad when the reported gid is one of our supplementary groups --
    emitter.instruction(&format!("mov edi, {}", SUPPLEMENTARY_GROUP_CAPACITY)); // entries the scratch list can hold
    emitter.instruction(&format!("lea rsi, [rbp - {}]", group_buf));            // scratch list for the supplementary groups
    emitter.instruction("mov eax, 115");                                        // Linux getgroups
    emitter.instruction("syscall");                                             // getgroups() → rax = count, or negative on failure
    emitter.instruction("cmp rax, 0");                                          // did we get any supplementary group?
    emitter.instruction("jle __rt_sma_world_x86");                              // none (or the call failed) → the world triad
    emitter.instruction("mov r10, rax");                                        // supplementary-group count
    emitter.instruction("xor r11, r11");                                        // scan index
    emitter.instruction("mov r8, QWORD PTR [rbp - 24]");                        // the gid the wrapper reported
    emitter.instruction(&format!("lea r9, [rbp - {}]", group_buf));             // base of the supplementary-group list
    emitter.label("__rt_sma_supp_x86");
    emitter.instruction("cmp r11, r10");                                        // scanned every supplementary group?
    emitter.instruction("jge __rt_sma_world_x86");                              // no supplementary group matched → the world triad
    emitter.instruction("mov ecx, DWORD PTR [r9 + r11 * 4]");                   // load one supplementary gid (gid_t is 32-bit)
    emitter.instruction("cmp rcx, r8");                                         // does it match the reported gid?
    emitter.instruction("je __rt_sma_group_x86");                               // → the group triad
    emitter.instruction("inc r11");                                             // advance the scan index
    emitter.instruction("jmp __rt_sma_supp_x86");                               // keep scanning

    // -- test the wanted bit inside the selected triad --
    emitter.label("__rt_sma_owner_x86");
    emitter.instruction("mov ecx, 6");                                          // the owner triad sits in bits 6..8
    emitter.instruction("jmp __rt_sma_test_x86");
    emitter.label("__rt_sma_group_x86");
    emitter.instruction("mov ecx, 3");                                          // the group triad sits in bits 3..5
    emitter.instruction("jmp __rt_sma_test_x86");
    emitter.label("__rt_sma_world_x86");
    emitter.instruction("xor ecx, ecx");                                        // the world triad sits in bits 0..2
    emitter.label("__rt_sma_test_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the stat mode
    emitter.instruction("shr rax, cl");                                         // shift the selected triad down to the low three bits
    emitter.instruction("and rax, QWORD PTR [rbp - 32]");                       // isolate the requested bit inside the selected triad
    emitter.instruction("test rax, rax");                                       // is the permission granted?
    emitter.instruction("setne al");                                            // materialize the PHP boolean
    emitter.instruction("movzx eax, al");                                       // widen it into the result register

    emitter.instruction(&format!("add rsp, {}", frame));                        // release the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the permission predicate
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::emit::Emitter;
    use crate::codegen_support::platform::{Platform, Target};

    /// The two targets this helper is hand-written for.
    const TARGETS: [(Platform, Arch); 2] = [
        (Platform::MacOS, Arch::AArch64),
        (Platform::Linux, Arch::X86_64),
    ];

    /// The triad selection is exclusive: once the owner or group triad is chosen PHP
    /// never looks at the remaining bits. That is only true if the helper SHIFTS the
    /// mode and masks the low three bits — masking `S_IRUSR|S_IRGRP|S_IROTH` at once
    /// would answer true for `mode 0044 uid=me`, where reference PHP answers false.
    #[test]
    fn the_permission_test_shifts_one_triad_instead_of_masking_all_three() {
        for (platform, arch) in TARGETS {
            let mut emitter = Emitter::new(Target::new(platform, arch));
            emit_stat_mode_access(&mut emitter);
            let asm = emitter.output();
            let shift = if arch == Arch::X86_64 {
                "shr rax, cl"
            } else {
                "lsr x10, x10, x9"
            };
            assert!(
                asm.contains(shift),
                "{arch:?}: the triad must be shifted into the low bits, not masked in place:\n{asm}"
            );
        }
    }

    /// Each triad is reached by its own shift, and the identities are consulted in PHP's
    /// order: uid, then primary gid, then supplementary groups. Swapping the owner and
    /// group shifts, or testing the gid before the uid, changes the answer for any file
    /// whose uid and gid both match — which is the common case for a file we own.
    #[test]
    fn the_triads_shift_by_six_three_and_zero_and_are_chosen_in_phps_order() {
        for (platform, arch) in TARGETS {
            let mut emitter = Emitter::new(Target::new(platform, arch));
            emit_stat_mode_access(&mut emitter);
            let asm = emitter.output();
            let (owner, group, world, shift_reg) = if arch == Arch::X86_64 {
                (
                    "__rt_sma_owner_x86",
                    "__rt_sma_group_x86",
                    "__rt_sma_world_x86",
                    "ecx",
                )
            } else {
                ("__rt_sma_owner", "__rt_sma_group", "__rt_sma_world", "x9")
            };
            for (label, shift) in [(owner, 6), (group, 3)] {
                let body = asm
                    .split(&format!("{label}:"))
                    .nth(1)
                    .unwrap_or_else(|| panic!("{arch:?}: {label} must exist:\n{asm}"));
                let materialized = if arch == Arch::X86_64 {
                    format!("mov {shift_reg}, {shift}")
                } else {
                    format!("mov {shift_reg}, #{shift}")
                };
                assert!(
                    body.lines().take(2).any(|l| l.contains(&materialized)),
                    "{arch:?}: {label} must shift the mode by {shift}:\n{asm}"
                );
            }
            let world_body = asm
                .split(&format!("{world}:"))
                .nth(1)
                .unwrap_or_else(|| panic!("{arch:?}: {world} must exist:\n{asm}"));
            let zeroed = if arch == Arch::X86_64 {
                world_body.contains(&format!("xor {shift_reg}, {shift_reg}"))
            } else {
                world_body.contains(&format!("mov {shift_reg}, #0"))
            };
            assert!(
                zeroed,
                "{arch:?}: the world triad must shift the mode by 0:\n{asm}"
            );
            let uid_at = asm.find(owner).expect("the uid test branches to the owner triad");
            let gid_at = asm.find(group).expect("the gid test branches to the group triad");
            assert!(
                uid_at < gid_at,
                "{arch:?}: the uid must be tested before the gid, or a file we own picks the group triad:\n{asm}"
            );
        }
    }

    /// A gid that is only a supplementary group still selects the group triad — measured
    /// against reference PHP, which scans `getgroups()` before falling back to the world
    /// bits. Skipping the scan is invisible on a primary-group file and wrong on any other.
    #[test]
    fn a_supplementary_group_is_scanned_before_the_world_triad() {
        for (platform, arch) in TARGETS {
            let mut emitter = Emitter::new(Target::new(platform, arch));
            emit_stat_mode_access(&mut emitter);
            let asm = emitter.output();
            let (scan_label, group_label, world_label) = if arch == Arch::X86_64 {
                (
                    "__rt_sma_supp_x86",
                    "__rt_sma_group_x86",
                    "__rt_sma_world_x86",
                )
            } else {
                ("__rt_sma_supp", "__rt_sma_group", "__rt_sma_world")
            };
            assert!(
                asm.contains(scan_label),
                "{arch:?}: the supplementary-group scan must exist:\n{asm}"
            );
            let scan_start = asm.find(&format!("{scan_label}:")).expect("scan label");
            let scan_body: Vec<&str> = asm[scan_start..]
                .lines()
                .take_while(|line| !line.contains(&format!("{group_label}:")))
                .collect();
            assert!(
                scan_body.iter().any(|line| line.contains(group_label)),
                "{arch:?}: a matching supplementary group must select the group triad:\n{asm}"
            );
            assert!(
                scan_body.iter().any(|line| line.contains(world_label)),
                "{arch:?}: an exhausted scan must fall through to the world triad:\n{asm}"
            );
        }
    }
}
