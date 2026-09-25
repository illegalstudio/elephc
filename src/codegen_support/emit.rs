//! Purpose:
//! Owns the assembly text builder and target-aware syntax helpers used by all emitters.
//! Centralizes labels, directives, relocation forms, comments, and raw text output.
//!
//! Called from:
//! - `crate::codegen` and all shared `crate::codegen_support::*` emitters.
//!
//! Key details:
//! - Instruction comments are emitted by callers; this module preserves target syntax and output ordering.

use std::collections::HashSet;
use std::fmt::Write;

use super::platform::{Arch, Platform, Target};

/// Assembly emitter.
pub struct Emitter {
    buf: String,
    pub target: Target,
    pub platform: Platform,
    /// When `true`, the `emit_*_symbol_*` helpers in `codegen::abi::symbols`
    /// route global-symbol references through the GOT (`@GOTPCREL` on x86_64,
    /// `:got:` + `:got_lo12:` on AArch64) instead of using direct PC-relative
    /// addressing. Required for shared-library output, where the loader cannot
    /// resolve cross-object `R_X86_64_PC32` relocations at dlopen time.
    pub pic_data_refs: bool,
    /// When `true`, user frames publish exceptional cleanup activations and
    /// process-fatal runtime paths may unwind through the cdylib host boundary.
    /// This is deliberately independent from position-independent addressing:
    /// PIC is a relocation choice, while boundary recovery is an ABI contract.
    pub cdylib_boundary: bool,
    /// When `true`, macOS runtime emission is prepared for per-symbol dead
    /// stripping: `label()` records each internal label name in
    /// `internal_labels` so the final assembly can rename them to Mach-O
    /// assembler-local (`L`-prefixed) labels. Under the `.subsections_via_symbols`
    /// footer that keeps each `__rt_*` helper a single atom (local labels never
    /// start an atom) while remaining valid conditional-branch targets on every
    /// toolchain, so the linker's `-dead_strip` drops whole unreferenced helpers.
    /// Only set for the macOS executable runtime object; Linux uses per-section
    /// `--gc-sections` and cdylibs never dead-strip.
    pub dead_strip: bool,
    /// Names of internal (`label()`) labels recorded while `dead_strip` is set,
    /// used by `localize_internal_labels` to rewrite them `L`-prefixed.
    internal_labels: HashSet<String>,
    /// Name of the last `.text.<name>` section opened, so a caller that emits a
    /// global entry *inside* a function body can put the body back where it was.
    /// Only ELF splits text per symbol; Mach-O keeps one flat `__text`, so this
    /// stays `None` there and reopening is a no-op.
    current_text_section: Option<String>,
}

impl Emitter {
    /// Creates an emitter for the specified target platform.
    pub fn new(target: Target) -> Self {
        Self {
            buf: String::with_capacity(4096),
            target,
            platform: target.platform,
            pic_data_refs: false,
            cdylib_boundary: false,
            dead_strip: false,
            internal_labels: HashSet::new(),
            current_text_section: None,
        }
    }

    /// Returns a new emitter configured for position-independent data
    /// references. Used by `--emit cdylib` so global symbol accesses survive
    /// dynamic loading as a shared object.
    pub fn new_pic(target: Target) -> Self {
        let mut emitter = Self::new(target);
        emitter.pic_data_refs = true;
        emitter
    }

    /// Returns an emitter configured for the recoverable cdylib ABI.
    pub fn new_cdylib(target: Target) -> Self {
        let mut emitter = Self::new_pic(target);
        emitter.cdylib_boundary = true;
        emitter
    }

    /// Returns a non-PIC emitter configured for the recoverable library ABI.
    ///
    /// Static archives are relocated once by the consuming application's linker,
    /// so they keep direct PC-relative data references while publishing the same
    /// fatal-recovery boundary as a dynamically loaded library.
    pub fn new_staticlib(target: Target) -> Self {
        let mut emitter = Self::new(target);
        emitter.cdylib_boundary = true;
        emitter
    }

    /// Emits a single assembly instruction with standard indentation.
    pub fn instruction(&mut self, instr: &str) {
        let _ = writeln!(self.buf, "    {}", instr);
    }

    /// Emits a local label (name:).
    /// Under macOS per-symbol dead stripping (`dead_strip`), a named identifier
    /// label is recorded so the final assembly can rename it to a Mach-O
    /// assembler-local (`L`-prefixed) label: those never start an atom under
    /// `.subsections_via_symbols` (keeping each helper one strippable unit) yet
    /// stay valid conditional-branch targets on every toolchain — unlike
    /// `.alt_entry`, which older assemblers reject as "external" for conditional
    /// branches. Numeric (`1:`/`2:`) and already-`L` labels are assembler-local
    /// already, so they are left untouched.
    pub fn label(&mut self, name: &str) {
        if self.dead_strip
            && self.platform == Platform::MacOS
            && !name.starts_with('L')
            && !name.bytes().all(|b| b.is_ascii_digit())
        {
            self.internal_labels.insert(name.to_string());
        }
        let _ = writeln!(self.buf, "{}:", name);
    }

    /// Takes ownership of the recorded internal-label names, clearing the set.
    /// Called once after runtime emission to drive `localize_internal_labels`.
    pub fn take_internal_labels(&mut self) -> HashSet<String> {
        std::mem::take(&mut self.internal_labels)
    }

    /// Emits a label for an internal helper that is reached from *another* helper
    /// via an unconditional `b`/`bl` (never a conditional branch). Under macOS
    /// dead stripping it is marked `.alt_entry`: it stays inside its defining
    /// helper's atom (so that helper is not split and its own conditional
    /// branches remain intra-atom) yet remains a real symbol, so the cross-helper
    /// `b`/`bl` keeps the atom alive under `-dead_strip`. Unlike `label()` it is
    /// NOT recorded for `L`-localization, so the bare name still resolves. Only
    /// valid for `b`/`bl` targets — older assemblers reject conditional branches
    /// to `.alt_entry` labels.
    pub fn label_shared(&mut self, name: &str) {
        if self.dead_strip && self.platform == Platform::MacOS {
            let _ = writeln!(self.buf, ".alt_entry {}", name);
        }
        let _ = writeln!(self.buf, "{}:", name);
    }

    /// Emit a label that is visible across object files (for two-object linking).
    /// On Linux, places each global symbol in its own `.text.<name>` section so
    /// that `--gc-sections` can eliminate unreachable helpers at link time.
    /// On Windows, emits `.globl` only (PE/COFF does not support per-function sections via GAS).
    pub fn label_global(&mut self, name: &str) {
        // An assembler-local name cannot be `.globl` ("non-local symbol required"). Catch
        // it here with the offending label instead of an assembler error thousands of
        // lines into the stream — entry points must mint via `next_global_label()`.
        debug_assert!(
            !name.starts_with(self.platform.local_label_prefix()),
            "label_global on assembler-local name {name}"
        );
        if self.platform == Platform::Linux {
            let _ = writeln!(self.buf, ".section .text.{},\"ax\",@progbits", name);
            let _ = writeln!(self.buf, ".globl {}", name);
            let _ = writeln!(self.buf, ".type {}, %function", name);
            let _ = writeln!(self.buf, "{}:", name);
            self.current_text_section = Some(name.to_string());
        } else {
            let _ = writeln!(self.buf, ".globl {}", name);
            let _ = writeln!(self.buf, "{}:", name);
        }
    }

    /// The section a later [`Self::reopen_text_section`] should return to.
    pub fn current_text_section(&self) -> Option<String> {
        self.current_text_section.clone()
    }

    /// Reopens a section previously captured with [`Self::current_text_section`].
    ///
    /// Needed because [`Self::label_global`] opens a fresh `.text.<name>` on ELF: code
    /// emitted after an inline global thunk would otherwise continue inside *that*
    /// thunk's section, leaving the enclosing function's tail — epilogue and `ret`
    /// included — in another function's section. That is invisible on Mach-O and inert
    /// on ELF until something needs a cross-section difference, at which point
    /// `--debug-info`'s function extents stop assembling.
    pub fn reopen_text_section(&mut self, section: Option<String>) {
        if self.platform != Platform::Linux {
            return;
        }
        let Some(name) = section else { return };
        if self.current_text_section.as_deref() == Some(name.as_str()) {
            return;
        }
        let _ = writeln!(self.buf, ".section .text.{},\"ax\",@progbits", name);
        self.current_text_section = Some(name);
    }

    /// Emits a line comment using the target's comment prefix.
    pub fn comment(&mut self, text: &str) {
        let _ = writeln!(
            self.buf,
            "    {} {}",
            self.target.line_comment_prefix(),
            text
        );
    }

    /// Emits a blank line for visual separation.
    pub fn blank(&mut self) {
        self.buf.push('\n');
    }

    /// Emits raw text directly to the output buffer without formatting.
    pub fn raw(&mut self, text: &str) {
        // `raw` is the escape hatch every hand-written directive goes through, so a
        // section change can arrive here rather than via `label_global`. Track it, or
        // `reopen_text_section` would restore a section that is no longer the one the
        // caller was in — a stale restore is worse than none.
        let line = text.trim_start();
        if let Some(rest) = line.strip_prefix(".section .text.") {
            self.current_text_section =
                Some(rest.split(&[',', ' '][..]).next().unwrap_or_default().to_string());
        } else if line.starts_with(".section") || line == ".text" || line.starts_with(".data") {
            self.current_text_section = None;
        }
        self.buf.push_str(text);
        self.buf.push('\n');
    }

    /// Emits the .text section prelude, including Intel syntax switch for x86_64.
    pub fn emit_text_prelude(&mut self) {
        if self.target.arch == Arch::X86_64 {
            self.raw(".intel_syntax noprefix");
        }
        self.raw(".text");
    }

    /// Returns the accumulated assembly output as a String.
    pub fn output(self) -> String {
        self.buf
    }

    // ── Platform-aware relocation helpers ─────────────────────────────

    /// Emit `adrp reg, sym@PAGE` (macOS) or `adrp reg, sym` (Linux).
    pub fn adrp(&mut self, reg: &str, sym: &str) {
        self.target
            .ensure_aarch64_backend("adrp relocation emission");
        match self.platform {
            Platform::MacOS => self.instruction(&format!("adrp {}, {}@PAGE", reg, sym)),
            Platform::Linux => self.instruction(&format!("adrp {}, {}", reg, sym)),
            Platform::Windows => panic!("Windows target is not yet supported (see issue #379)"),
        }
    }

    /// Emit `add dst, src, sym@PAGEOFF` (macOS) or `add dst, src, :lo12:sym` (Linux).
    pub fn add_lo12(&mut self, dst: &str, src: &str, sym: &str) {
        self.target
            .ensure_aarch64_backend("lo12 relocation emission");
        match self.platform {
            Platform::MacOS => self.instruction(&format!("add {}, {}, {}@PAGEOFF", dst, src, sym)),
            Platform::Linux => self.instruction(&format!("add {}, {}, :lo12:{}", dst, src, sym)),
            Platform::Windows => panic!("Windows target is not yet supported (see issue #379)"),
        }
    }

    /// Emit `ldr reg, [base, sym@PAGEOFF]` (macOS) or `ldr reg, [base, :lo12:sym]` (Linux).
    pub fn ldr_lo12(&mut self, reg: &str, base: &str, sym: &str) {
        self.target.ensure_aarch64_backend("lo12 load emission");
        match self.platform {
            Platform::MacOS => {
                self.instruction(&format!("ldr {}, [{}, {}@PAGEOFF]", reg, base, sym))
            }
            Platform::Linux => self.instruction(&format!("ldr {}, [{}, :lo12:{}]", reg, base, sym)),
            Platform::Windows => panic!("Windows target is not yet supported (see issue #379)"),
        }
    }

    /// Emit `adrp reg, sym@GOTPAGE` (macOS) or `adrp reg, :got:sym` (Linux).
    pub fn adrp_got(&mut self, reg: &str, sym: &str) {
        self.target
            .ensure_aarch64_backend("GOT page relocation emission");
        match self.platform {
            Platform::MacOS => self.instruction(&format!("adrp {}, {}@GOTPAGE", reg, sym)),
            Platform::Linux => self.instruction(&format!("adrp {}, :got:{}", reg, sym)),
            Platform::Windows => panic!("Windows target is not yet supported (see issue #379)"),
        }
    }

    /// Emit `ldr reg, [base, sym@GOTPAGEOFF]` (macOS) or `ldr reg, [base, :got_lo12:sym]` (Linux).
    pub fn ldr_got_lo12(&mut self, reg: &str, base: &str, sym: &str) {
        self.target.ensure_aarch64_backend("GOT lo12 load emission");
        match self.platform {
            Platform::MacOS => {
                self.instruction(&format!("ldr {}, [{}, {}@GOTPAGEOFF]", reg, base, sym))
            }
            Platform::Linux => {
                self.instruction(&format!("ldr {}, [{}, :got_lo12:{}]", reg, base, sym))
            }
            Platform::Windows => panic!("Windows target is not yet supported (see issue #379)"),
        }
    }

    // ── Platform-aware syscall helper ─────────────────────────────────

    /// Emit a complete syscall sequence: sets the syscall register and traps.
    /// On macOS: `mov x16, #N` + `svc #0x80`.
    /// On Linux: optional AT_FDCWD arg shift + `mov x8, #M` + `svc #0`.
    pub fn syscall(&mut self, macos_num: u32) {
        self.target.ensure_aarch64_backend("syscall emission");
        match self.platform {
            Platform::MacOS => {
                self.instruction(&format!("mov x16, #{}", macos_num));
                self.instruction("svc #0x80");
            }
            Platform::Linux => {
                let target = self.target;
                target.emit_linux_syscall(self, macos_num);
            }
            Platform::Windows => panic!("Windows does not use AArch64 syscalls (see issue #379)"),
        }
    }

    // ── Platform-aware C symbol call ─────────────────────────────────

    /// Emit `bl _func` (macOS) or `bl func` (Linux) for C library calls.
    ///
    /// For a symbol that may be an imported Windows API function
    /// (msvcrt/ws2_32 — anything a `WIN32_IMPORTS` entry could name), prefer
    /// [`Emitter::emit_call_c`]: on windows-x86_64 `bl_c` always emits a bare
    /// `call func`, which is only correct for internal/toolchain symbols —
    /// a bare msvcrt/ws2_32 import expects the MSx64 calling convention, not
    /// SysV, so calling it through `bl_c` is the Class-1 ABI bug. `bl_c`
    /// remains the right choice for symbols that never resolve to a Windows
    /// import.
    pub fn bl_c(&mut self, func: &str) {
        match (self.platform, self.target.arch) {
            (Platform::MacOS, Arch::AArch64) => self.instruction(&format!("bl _{}", func)),
            (Platform::Linux, Arch::AArch64) => self.instruction(&format!("bl {}", func)),
            (Platform::Linux, Arch::X86_64) => self.instruction(&format!("call {}", func)),
            (Platform::Windows, Arch::X86_64) => {
                debug_assert!(
                    super::runtime::windows_c_shim_name(func).is_none(),
                    "bl_c(\"{func}\") on windows-x86_64 targets a symbol with a registered \
                     __rt_sys_* shim; call emit_call_c(\"{func}\") instead so the shim's \
                     SysV->MSx64 ABI conversion is applied rather than a raw SysV-staged import call"
                );
                // MinGW's C-library setjmp/longjmp are SEH-based and read their
                // arguments MSx64-style; elephc's SysV-staged, SEH-free replacements
                // (see runtime::exceptions::setjmp) are used on this target instead.
                let name = match func {
                    "setjmp" => "__rt_setjmp",
                    "longjmp" => "__rt_longjmp",
                    other => other,
                };
                self.instruction(&format!("call {}", name));
            }
            (Platform::MacOS, Arch::X86_64) => {
                panic!("C symbol calls are not implemented yet for target macos-x86_64");
            }
            (Platform::Windows, Arch::AArch64) => {
                panic!("Windows ARM64 target is not yet supported (see issue #379)");
            }
        }
    }

    /// Symbols for which, on windows-x86_64, `emit_shim_c_symbols` /
    /// `emit_shim_c_symbol_delegates` (`codegen_support::runtime::win32`)
    /// emit an internal label of the identical name that performs its own
    /// SysV→MSx64 ABI conversion internally (e.g. `write`, `read`, `stat`).
    /// A bare `call <symbol>` for one of these reaches that internal label,
    /// not the msvcrt/ws2_32 import of the same name, so it is safe for
    /// [`Emitter::emit_call_c`] to emit it unchanged. Keep this list in sync
    /// with the labels those two functions emit.
    const EMIT_CALL_C_SYSV_STUB_DELEGATES: &'static [&'static str] = &[
        "accept4", "access", "brk", "chdir", "chmod", "clock_gettime", "close", "dirfd",
        "execve", "exit", "fcntl", "fgetc", "fileno", "flock", "fnmatch", "fstat", "fsync",
        "ftruncate", "futex", "getcwd", "getpid", "getrandom", "glob", "globfree", "h_errno",
        "hstrerror", "ioctl", "kill", "link", "lseek", "lstat", "main", "mkdir", "mmap",
        "mprotect", "munmap", "open", "pclose", "popen", "read", "readlink", "realpath",
        "rename", "rmdir", "sleep", "stat", "symlink", "sysinfo", "system", "timegm", "umask",
        "uname", "unlink", "usleep", "utimensat", "write", "writev",
    ];

    /// Emit a call to a C-library symbol that may be an imported Windows API
    /// function (msvcrt/ws2_32). Contrast with [`Emitter::bl_c`]: `bl_c`
    /// emits a bare `call func` on windows-x86_64 unconditionally, which is
    /// wrong whenever `func` is a bare msvcrt/ws2_32 import (entered with
    /// SysV registers instead of the MSx64 ABI it expects — the Class-1 ABI
    /// bug). `emit_call_c` routes `symbol` correctly on windows-x86_64:
    /// - if `symbol` has a registered `__rt_sys_<symbol>` shim (the registry
    ///   is `codegen_support::runtime::win32::windows_c_shim_name`, the
    ///   single source of truth for Windows C shims), emits
    ///   `call __rt_sys_<symbol>`;
    /// - else, if `symbol` is a known SysV stub-delegate (see
    ///   [`Self::EMIT_CALL_C_SYSV_STUB_DELEGATES`]), emits a bare
    ///   `call <symbol>` — correct because the call target is the internal
    ///   stub-delegate label, not the msvcrt/ws2_32 import;
    /// - else PANICS with a message naming the missing shim/stub. This is a
    ///   build-time exhaustiveness guard: it only fires for symbols actually
    ///   passed to `emit_call_c`, so a future call site added for a symbol
    ///   with neither a shim nor a stub blows up the first test that
    ///   exercises it, instead of silently reintroducing the Class-1 bug.
    ///
    /// On every other target, emits exactly what `bl_c` emits (`call symbol`
    /// on Linux x86_64; `bl _symbol`/`bl symbol` on AArch64) — byte-identical.
    pub fn emit_call_c(&mut self, symbol: &str) {
        if (self.platform, self.target.arch) != (Platform::Windows, Arch::X86_64) {
            self.bl_c(symbol);
            return;
        }
        if let Some(shim) = super::runtime::windows_c_shim_name(symbol) {
            self.instruction(&format!("call {}", shim));
        } else if Self::EMIT_CALL_C_SYSV_STUB_DELEGATES.contains(&symbol) {
            self.instruction(&format!("call {}", symbol));
        } else {
            panic!(
                "emit_call_c(\"{symbol}\"): no Windows shim and not a SysV stub-delegate — \
                 add a __rt_sys_{symbol} shim or register {symbol} as stub-covered"
            );
        }
    }

    /// Emits, on windows-x86_64 ONLY, the reverse-index-order remap of the SysV
    /// integer argument registers into the MSx64 registers a GENERATED callback
    /// (closure/descriptor invoker, user method, magic method) reads, immediately
    /// before an indirect `call` into that generated code. Mirror of
    /// `remap_platform_args_to_runtime_helper_regs` (generated→`__rt_*`, SysV): here
    /// a hand-written runtime helper calls BACK into generated code, so the
    /// SysV-staged args must be moved into the MSx64 argument registers. Reverse
    /// index order (r9←rcx, r8←rdx, rdx←rsi, rcx←rdi) avoids clobbering a source
    /// register a later index still reads (rcx is SysV arg3 AND MSx64 arg0; rdx is
    /// SysV arg2 AND MSx64 arg1). No-op (nothing emitted) on every
    /// non-windows-x86_64 target, so output is byte-identical there. `int_reg_count`
    /// MUST be ≤4: a callback consuming >4 integer registers passes its 5th+ arg on
    /// the MSx64 stack and needs bespoke stack+shadow staging, not this helper.
    pub fn remap_sysv_args_to_platform_for_callback(&mut self, int_reg_count: usize) {
        assert!(
            int_reg_count <= 4,
            "reverse-ABI callback remap covers <=4 register args; {int_reg_count} needs MSx64 stack staging"
        );
        if (self.platform, self.target.arch) != (Platform::Windows, Arch::X86_64) {
            return;
        }
        for idx in (0..int_reg_count).rev() {
            let src = crate::codegen_support::abi::runtime_helper_int_arg_reg(self, idx);
            let dst = crate::codegen_support::abi::int_arg_reg_name(self.target, idx);
            if src != dst {
                self.instruction(&format!("mov {}, {}", dst, src));             // SysV callback arg -> MSx64 reg the generated callee reads
            }
        }
    }

    /// Calls generated PHP-ABI code from a hand-written SysV runtime helper.
    ///
    /// Runtime helpers always stage integer arguments in SysV registers. On
    /// Windows x86_64, generated functions instead consume MSx64 registers and
    /// require 32 bytes of caller shadow space. This adapter relocates the call
    /// target away from argument registers, stages the fifth and sixth SysV
    /// arguments in MSx64 stack slots, remaps the first four registers without
    /// collisions, and restores the stack after the indirect call. Other
    /// targets retain the original bare indirect-call sequence byte-for-byte.
    pub fn emit_platform_callback_call(&mut self, fnptr_reg: &str, int_arg_count: usize) {
        self.emit_sysv_to_msx64_indirect_call(fnptr_reg, int_arg_count, "generated PHP callback");
    }

    /// Emits a call to a NATIVE (real C/Rust, MSx64-ABI) function through the
    /// pointer in `fnptr_reg`, correcting the ABI on windows-x86_64. The
    /// hand-written runtime helpers stage arguments in the SysV registers; a
    /// genuine native callee on windows reads the MSx64 registers, needs
    /// 32-byte caller shadow space, and (at >=4 args) would collide with a
    /// fn-ptr parked in r9. This relocates the fn-ptr off the MSx64 argument
    /// registers into r11, reserves the shadow space (plus 8-byte slots for
    /// the 5th/6th integer args, 16-byte aligned), moves the SysV 5th/6th
    /// args (r8/r9) onto the stack BEFORE the register remap clobbers them,
    /// remaps the first <=4 SysV integer args into the MSx64 registers
    /// (reverse index order, via `remap_sysv_args_to_platform_for_callback`),
    /// then calls the relocated pointer. Byte-identical to a bare
    /// `call {fnptr_reg}` on every other target. Arguments must already be in
    /// the SysV integer registers (rdi/rsi/rdx/rcx/r8/r9); no float args
    /// (assert-free but undefined if present); `int_arg_count` must be <= 6
    /// (a 7th+ arg needs bespoke staging). The caller frame must be 16-byte
    /// aligned at this point.
    pub fn emit_native_bridge_call(&mut self, fnptr_reg: &str, int_arg_count: usize) {
        self.emit_sysv_to_msx64_indirect_call(fnptr_reg, int_arg_count, "native bridge");
    }

    /// Calls a statically linked native C/Rust bridge symbol from a hand-written
    /// runtime helper whose integer arguments use the compiler's SysV runtime ABI.
    ///
    /// Windows x86_64 materializes the platform-mangled symbol address and routes
    /// it through [`Emitter::emit_native_bridge_call`] so arguments are remapped
    /// and the mandatory MSx64 shadow space is reserved. Other targets retain a
    /// direct platform-mangled C-symbol call.
    pub fn emit_native_bridge_symbol_call(&mut self, symbol: &str, int_arg_count: usize) {
        if (self.platform, self.target.arch) != (Platform::Windows, Arch::X86_64) {
            self.bl_c(symbol);
            return;
        }

        let extern_symbol = self.target.extern_symbol(symbol);
        crate::codegen_support::abi::emit_symbol_address(self, "r11", &extern_symbol);
        self.emit_native_bridge_call("r11", int_arg_count);
    }

    /// Calls an indirect bridge entry whose published pointer already exposes
    /// the compiler runtime ABI, including through a Windows ABI adapter.
    pub fn emit_published_bridge_call(&mut self, fnptr_reg: &str) {
        match self.target.arch {
            Arch::AArch64 => self.instruction(&format!("blr {fnptr_reg}")),
            Arch::X86_64 => self.instruction(&format!("call {fnptr_reg}")),
        }
    }

    /// Emits the shared SysV-runtime to MSx64 indirect-call transition.
    fn emit_sysv_to_msx64_indirect_call(
        &mut self,
        fnptr_reg: &str,
        int_arg_count: usize,
        callee_kind: &str,
    ) {
        if (self.platform, self.target.arch) != (Platform::Windows, Arch::X86_64) {
            self.instruction(&format!("call {}", fnptr_reg));                   // native call, SysV/AAPCS ABI — byte-identical off windows
            return;
        }
        assert!(
            int_arg_count <= 6,
            "{callee_kind} call remap covers <=6 integer args; {int_arg_count} needs bespoke staging"
        );
        if fnptr_reg != "r11" {
            self.instruction(&format!("mov r11, {}", fnptr_reg));               // relocate the fn-ptr off the MSx64 argument registers
        }
        // 32-byte MSx64 shadow space + one 8-byte slot per 5th/6th SysV int arg, 16-byte aligned
        let stack_args = int_arg_count.saturating_sub(4);
        let frame = (32 + stack_args * 8 + 15) / 16 * 16;
        self.instruction(&format!("sub rsp, {}", frame));                       // reserve MSx64 shadow space and stack-arg slots
        for idx in 4..int_arg_count {
            let src = crate::codegen_support::abi::runtime_helper_int_arg_reg(self, idx);
            let offset = 32 + (idx - 4) * 8;
            self.instruction(&format!("mov QWORD PTR [rsp + {}], {}", offset, src)); // 5th+ SysV int arg -> MSx64 stack slot (before the remap clobbers r8/r9)
        }
        self.remap_sysv_args_to_platform_for_callback(int_arg_count.min(4));
        self.instruction("call r11");                                           // invoke the MSx64 callee via the relocated pointer
        self.instruction(&format!("add rsp, {}", frame));                       // release the shadow + stack-arg scratch
    }

    // ── Platform-aware entry point ───────────────────────────────────

    /// Returns the program entry point symbol: `_main` (macOS), `main` (Linux),
    /// or `__elephc_main` (Windows x86_64 — the Win32 shim emits the real `main`
    /// wrapper that calls into `__elephc_main`).
    pub fn entry_symbol(&self) -> &'static str {
        match self.target.arch {
            Arch::AArch64 => match self.platform {
                Platform::MacOS => "_main",
                Platform::Linux => "main",
                Platform::Windows => {
                    panic!("Windows ARM64 target is not yet supported (see issue #379)")
                }
            },
            Arch::X86_64 => match self.platform {
                Platform::Windows => "__elephc_main",
                _ => "main",
            },
        }
    }

    /// Emit the program entry point label: `_main` (macOS), `main` (Linux),
    /// or `__elephc_main` (Windows — the Win32 shim emits the real `main` wrapper).
    pub fn entry_label(&mut self) {
        match self.target.arch {
            Arch::AArch64 => match self.platform {
                Platform::MacOS => self.label_global("_main"),
                Platform::Linux => self.label_global("main"),
                Platform::Windows => {
                    panic!("Windows ARM64 target is not yet supported (see issue #379)");
                }
            },
            Arch::X86_64 => match self.platform {
                Platform::Windows => self.label_global("__elephc_main"),
                _ => self.label_global("main"),
            },
        }
    }
}

/// Rewrites every whole-token occurrence of an internal label name to its
/// Mach-O assembler-local (`L`-prefixed) form, covering both the `name:`
/// definition and every branch/reference to it. Used by the macOS dead-strip
/// path: under `.subsections_via_symbols`, conditional branches may only target
/// assembler-local labels, and `L`-prefixed labels also do not start a new atom,
/// so each `__rt_*` helper stays a single dead-strippable unit. Matching is
/// whole-token (identifier runs of `[A-Za-z0-9_$]`), so a name is never rewritten
/// inside a longer identifier. Quoted assembly strings are copied verbatim within
/// their physical line, so user string constants cannot be changed when user
/// metadata references one of the localized labels and an unmatched quote cannot
/// suppress localization on later lines.
pub fn localize_internal_labels(asm: &str, internal: &HashSet<String>) -> String {
    if internal.is_empty() {
        return asm.to_string();
    }
    let bytes = asm.as_bytes();
    let is_ident = |b: u8| b.is_ascii_alphanumeric() || b == b'_' || b == b'$';
    let mut out = String::with_capacity(asm.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'"' {
            let start = i;
            i += 1;
            let mut escaped = false;
            while i < bytes.len() {
                let byte = bytes[i];
                i += 1;
                if byte == b'\n' {
                    break;
                } else if escaped {
                    escaped = false;
                } else if byte == b'\\' {
                    escaped = true;
                } else if byte == b'"' {
                    break;
                }
            }
            out.push_str(&asm[start..i]);
        } else if is_ident(bytes[i]) {
            let start = i;
            while i < bytes.len() && is_ident(bytes[i]) {
                i += 1;
            }
            let token = &asm[start..i];
            if internal.contains(token) {
                out.push('L');
            }
            out.push_str(token);
        } else {
            let start = i;
            while i < bytes.len() && !is_ident(bytes[i]) && bytes[i] != b'"' {
                i += 1;
            }
            out.push_str(&asm[start..i]);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::fs;
    use std::path::{Path, PathBuf};

    use super::*;

    /// Collects Rust source files below one runtime directory for structural ABI checks.
    fn collect_runtime_rust_files(dir: &Path, files: &mut Vec<PathBuf>) {
        for entry in fs::read_dir(dir).expect("runtime source directory must be readable") {
            let path = entry.expect("runtime source entry must be readable").path();
            if path.is_dir() {
                collect_runtime_rust_files(&path, files);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                files.push(path);
            }
        }
    }

    /// Verifies comment prefix is platform aware.
    #[test]
    fn test_comment_prefix_is_platform_aware() {
        let mut mac = Emitter::new(Target::new(Platform::MacOS, Arch::AArch64));
        mac.comment("-- block --");
        assert_eq!(mac.output(), "    ; -- block --\n");

        let mut linux = Emitter::new(Target::new(Platform::Linux, Arch::AArch64));
        linux.comment("-- block --");
        assert_eq!(linux.output(), "    // -- block --\n");

        let mut linux_x86 = Emitter::new(Target::new(Platform::Linux, Arch::X86_64));
        linux_x86.comment("-- block --");
        assert_eq!(linux_x86.output(), "    # -- block --\n");
    }

    /// Verifies text prelude switches x86 to intel syntax.
    #[test]
    fn test_text_prelude_switches_x86_to_intel_syntax() {
        let mut mac = Emitter::new(Target::new(Platform::MacOS, Arch::AArch64));
        mac.emit_text_prelude();
        assert_eq!(mac.output(), ".text\n");

        let mut linux_x86 = Emitter::new(Target::new(Platform::Linux, Arch::X86_64));
        linux_x86.emit_text_prelude();
        assert_eq!(linux_x86.output(), ".intel_syntax noprefix\n.text\n");
    }

    /// Verifies internal symbol references are localized without rewriting quoted user bytes.
    #[test]
    fn test_localize_internal_labels_preserves_assembly_strings() {
        let internal = HashSet::from(["_eir_branch_1".to_string()]);
        let asm = "    b _eir_branch_1\n_eir_branch_1:\n    .ascii \"_eir_branch_1\"\n";
        assert_eq!(
            localize_internal_labels(asm, &internal),
            "    b L_eir_branch_1\nL_eir_branch_1:\n    .ascii \"_eir_branch_1\"\n"
        );
    }

    /// Verifies an unmatched quote in one assembly line cannot hide labels on later lines.
    #[test]
    fn test_localize_internal_labels_bounds_unmatched_quotes_to_one_line() {
        let internal = HashSet::from(["_eir_branch_1".to_string()]);
        let asm = "    ; unmatched \" in comment\n    b _eir_branch_1\n_eir_branch_1:\n";
        assert_eq!(
            localize_internal_labels(asm, &internal),
            "    ; unmatched \" in comment\n    b L_eir_branch_1\nL_eir_branch_1:\n"
        );
    }

    /// A function that emits a global thunk mid-body must resume in its own section.
    ///
    /// On ELF `label_global` opens `.text.<name>`, so without restoring it the code
    /// after an inline thunk continues inside the *thunk's* section — the enclosing
    /// function's epilogue and `ret` end up filed under another symbol. Nothing
    /// complains until something needs a cross-section difference, and then
    /// `--debug-info`'s `Lelephc_fend_N - <fn>` extents stop assembling ("can't
    /// resolve"), which is how every PDO program failed to build on Linux.
    #[test]
    fn test_a_global_thunk_does_not_swallow_the_rest_of_its_caller() {
        let mut e = Emitter::new(Target::new(Platform::Linux, Arch::AArch64));
        e.label_global("_php_outer");
        let enclosing = e.current_text_section();
        assert_eq!(enclosing.as_deref(), Some("_php_outer"));

        e.label_global("_thunk");
        assert_eq!(e.current_text_section().as_deref(), Some("_thunk"));

        e.reopen_text_section(enclosing);
        assert_eq!(e.current_text_section().as_deref(), Some("_php_outer"));

        // The tail of the caller must sit after a reopening of its own section, and
        // the reopen must not redeclare the symbol (that would be a duplicate).
        let out = e.output();
        let tail = &out[out.rfind(".section .text._php_outer").expect("section reopened")..];
        assert!(!tail.contains(".globl"), "reopening redeclared the symbol: {tail}");
        assert!(
            out.matches(".section .text._php_outer").count() == 2,
            "expected the caller's section opened once and reopened once:\n{out}"
        );
    }

    /// A plain `.text` (or any raw section directive) invalidates the tracked section,
    /// so a later restore cannot resurrect a section the caller has already left.
    #[test]
    fn test_a_raw_section_directive_retracks_or_clears() {
        let mut e = Emitter::new(Target::new(Platform::Linux, Arch::AArch64));
        e.label_global("_php_outer");
        assert_eq!(e.current_text_section().as_deref(), Some("_php_outer"));

        e.emit_text_prelude(); // plain `.text` — no longer in a per-symbol section
        assert_eq!(e.current_text_section(), None);

        e.raw(".section .text._other,\"ax\",@progbits");
        assert_eq!(e.current_text_section().as_deref(), Some("_other"));

        e.raw(".section .rodata");
        assert_eq!(e.current_text_section(), None);

        // Restoring nothing must emit nothing: the three directives above and no more.
        e.reopen_text_section(None);
        let out = e.output();
        assert_eq!(out.matches(".section").count(), 3, "{out}");
    }

    /// Mach-O keeps one flat `__text`, so there is no section to restore there.
    #[test]
    fn test_reopening_a_section_is_inert_on_mach_o() {
        let mut e = Emitter::new(Target::new(Platform::MacOS, Arch::AArch64));
        e.label_global("_php_outer");
        let enclosing = e.current_text_section();
        assert_eq!(enclosing, None);
        e.label_global("_thunk");
        e.reopen_text_section(enclosing);
        let out = e.output();
        assert!(!out.contains(".section"), "{out}");
    }

    /// Rejects new raw runtime indirect calls unless their ABI family is explicitly reviewed.
    #[test]
    fn runtime_indirect_calls_are_structurally_allowlisted() {
        let runtime_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/codegen_support/runtime");
        let allowed = BTreeMap::from([
            // `stream_open` has seven integer arguments. Its source-side SysV
            // seventh stack argument requires bespoke Windows stack staging;
            // see the target-specific invariant and regression in `io/fopen.rs`.
            (("io/fopen.rs", "r11"), 1usize),
            (("io/fwrite.rs", "r9"), 3usize),
            (("io/ob_handler.rs", "r10"), 1usize),
            (("strings/hash_hmac.rs", "r11"), 1usize),
        ]);
        let mut files = Vec::new();
        collect_runtime_rust_files(&runtime_root, &mut files);
        let mut observed = BTreeMap::<(String, String), usize>::new();

        for path in files {
            let relative = path
                .strip_prefix(&runtime_root)
                .expect("runtime source remains below its root")
                .to_string_lossy()
                .replace('\\', "/");
            let source = fs::read_to_string(&path).expect("runtime source is readable");
            for line in source.lines() {
                let Some(call) = line.split("emitter.instruction(\"call ").nth(1) else { // inspect generated call sites for ABI validation
                    continue;
                };
                let Some(register) = call.split('"').next() else {
                    continue;
                };
                if !matches!(
                    register,
                    "rax" | "rbx" | "rcx" | "rdx" | "rsi" | "rdi" | "r8" | "r9"
                        | "r10" | "r11" | "r12" | "r13" | "r14" | "r15"
                ) {
                    continue;
                }
                *observed
                    .entry((relative.clone(), register.to_string()))
                    .or_default() += 1;
            }
        }

        let allowed = allowed
            .into_iter()
            .map(|((path, register), count)| ((path.to_string(), register.to_string()), count))
            .collect::<BTreeMap<_, _>>();
        assert_eq!(
            observed, allowed,
            "new runtime indirect calls need an ABI helper or explicit classification"
        );
    }
}
