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
}

impl Emitter {
    /// Creates an emitter for the specified target platform.
    pub fn new(target: Target) -> Self {
        Self {
            buf: String::with_capacity(4096),
            target,
            platform: target.platform,
            pic_data_refs: false,
            dead_strip: false,
            internal_labels: HashSet::new(),
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
    pub fn label_global(&mut self, name: &str) {
        if self.platform == Platform::Linux {
            let _ = writeln!(self.buf, ".section .text.{},\"ax\",@progbits", name);
            let _ = writeln!(self.buf, ".globl {}", name);
            let _ = writeln!(self.buf, ".type {}, %function", name);
            let _ = writeln!(self.buf, "{}:", name);
        } else {
            let _ = writeln!(self.buf, ".globl {}", name);
            let _ = writeln!(self.buf, "{}:", name);
        }
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
            Platform::Windows => panic!("Windows target is not yet supported (see issue #379)"),
        }
    }

    // ── Platform-aware C symbol call ─────────────────────────────────

    /// Emit `bl _func` (macOS) or `bl func` (Linux) for C library calls.
    pub fn bl_c(&mut self, func: &str) {
        match (self.platform, self.target.arch) {
            (Platform::MacOS, Arch::AArch64) => self.instruction(&format!("bl _{}", func)),
            (Platform::Linux, Arch::AArch64) => self.instruction(&format!("bl {}", func)),
            (Platform::Linux, Arch::X86_64) => self.instruction(&format!("call {}", func)),
            (Platform::MacOS, Arch::X86_64) => {
                panic!("C symbol calls are not implemented yet for target macos-x86_64");
            }
            (Platform::Windows, _) => panic!("Windows target is not yet supported (see issue #379)"),
        }
    }

    // ── Platform-aware entry point ───────────────────────────────────

    /// Returns the program entry point symbol: `_main` (macOS) or `main` (Linux).
    pub fn entry_symbol(&self) -> &'static str {
        match self.target.arch {
            Arch::AArch64 => match self.platform {
                Platform::MacOS => "_main",
                Platform::Linux => "main",
                Platform::Windows => panic!("Windows target is not yet supported (see issue #379)"),
            },
            Arch::X86_64 => "main",
        }
    }

    /// Emit the program entry point label: `_main` (macOS) or `main` (Linux).
    pub fn entry_label(&mut self) {
        let symbol = self.entry_symbol();
        self.label_global(symbol);
    }
}

/// Rewrites every whole-token occurrence of an internal label name to its
/// Mach-O assembler-local (`L`-prefixed) form, covering both the `name:`
/// definition and every branch/reference to it. Used by the macOS dead-strip
/// path: under `.subsections_via_symbols`, conditional branches may only target
/// assembler-local labels, and `L`-prefixed labels also do not start a new atom,
/// so each `__rt_*` helper stays a single dead-strippable unit. Matching is
/// whole-token (identifier runs of `[A-Za-z0-9_$]`), so a name is never rewritten
/// inside a longer identifier; non-identifier text (including UTF-8 in comments)
/// is copied verbatim. Apply to the runtime text only — the runtime `.data` never
/// references internal labels, and skipping it avoids touching string literals.
pub fn localize_internal_labels(asm: &str, internal: &HashSet<String>) -> String {
    if internal.is_empty() {
        return asm.to_string();
    }
    let bytes = asm.as_bytes();
    let is_ident = |b: u8| b.is_ascii_alphanumeric() || b == b'_' || b == b'$';
    let mut out = String::with_capacity(asm.len());
    let mut i = 0;
    while i < bytes.len() {
        if is_ident(bytes[i]) {
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
            while i < bytes.len() && !is_ident(bytes[i]) {
                i += 1;
            }
            out.push_str(&asm[start..i]);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
