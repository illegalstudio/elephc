use std::fmt::Write;

use super::platform::Platform;

pub struct Emitter {
    buf: String,
    pub platform: Platform,
}

impl Emitter {
    pub fn new() -> Self {
        Self {
            buf: String::with_capacity(4096),
            platform: Platform::detect(),
        }
    }

    pub fn instruction(&mut self, instr: &str) {
        let _ = writeln!(self.buf, "    {}", instr);
    }

    pub fn label(&mut self, name: &str) {
        let _ = writeln!(self.buf, "{}:", name);
    }

    /// Emit a label that is visible across object files (for two-object linking).
    pub fn label_global(&mut self, name: &str) {
        let _ = writeln!(self.buf, ".globl {}", name);
        let _ = writeln!(self.buf, "{}:", name);
    }

    pub fn comment(&mut self, text: &str) {
        let _ = writeln!(self.buf, "    {} {}", self.platform.line_comment_prefix(), text);
    }

    pub fn blank(&mut self) {
        self.buf.push('\n');
    }

    pub fn raw(&mut self, text: &str) {
        self.buf.push_str(text);
        self.buf.push('\n');
    }

    pub fn output(self) -> String {
        self.buf
    }

    // ── Platform-aware relocation helpers ─────────────────────────────

    /// Emit `adrp reg, sym@PAGE` (macOS) or `adrp reg, sym` (Linux).
    pub fn adrp(&mut self, reg: &str, sym: &str) {
        match self.platform {
            Platform::MacOS => self.instruction(&format!("adrp {}, {}@PAGE", reg, sym)),
            Platform::Linux => self.instruction(&format!("adrp {}, {}", reg, sym)),
        }
    }

    /// Emit `add dst, src, sym@PAGEOFF` (macOS) or `add dst, src, :lo12:sym` (Linux).
    pub fn add_lo12(&mut self, dst: &str, src: &str, sym: &str) {
        match self.platform {
            Platform::MacOS => self.instruction(&format!("add {}, {}, {}@PAGEOFF", dst, src, sym)),
            Platform::Linux => self.instruction(&format!("add {}, {}, :lo12:{}", dst, src, sym)),
        }
    }

    /// Emit `ldr reg, [base, sym@PAGEOFF]` (macOS) or `ldr reg, [base, :lo12:sym]` (Linux).
    pub fn ldr_lo12(&mut self, reg: &str, base: &str, sym: &str) {
        match self.platform {
            Platform::MacOS => {
                self.instruction(&format!("ldr {}, [{}, {}@PAGEOFF]", reg, base, sym))
            }
            Platform::Linux => {
                self.instruction(&format!("ldr {}, [{}, :lo12:{}]", reg, base, sym))
            }
        }
    }

    /// Emit `adrp reg, sym@GOTPAGE` (macOS) or `adrp reg, :got:sym` (Linux).
    pub fn adrp_got(&mut self, reg: &str, sym: &str) {
        match self.platform {
            Platform::MacOS => self.instruction(&format!("adrp {}, {}@GOTPAGE", reg, sym)),
            Platform::Linux => self.instruction(&format!("adrp {}, :got:{}", reg, sym)),
        }
    }

    /// Emit `ldr reg, [base, sym@GOTPAGEOFF]` (macOS) or `ldr reg, [base, :got_lo12:sym]` (Linux).
    pub fn ldr_got_lo12(&mut self, reg: &str, base: &str, sym: &str) {
        match self.platform {
            Platform::MacOS => {
                self.instruction(&format!("ldr {}, [{}, {}@GOTPAGEOFF]", reg, base, sym))
            }
            Platform::Linux => {
                self.instruction(&format!("ldr {}, [{}, :got_lo12:{}]", reg, base, sym))
            }
        }
    }

    // ── Platform-aware syscall helper ─────────────────────────────────

    /// Emit a complete syscall sequence: sets the syscall register and traps.
    /// On macOS: `mov x16, #N` + `svc #0x80`.
    /// On Linux: optional AT_FDCWD arg shift + `mov x8, #M` + `svc #0`.
    pub fn syscall(&mut self, macos_num: u32) {
        match self.platform {
            Platform::MacOS => {
                self.instruction(&format!("mov x16, #{}", macos_num));
                self.instruction("svc #0x80");
            }
            Platform::Linux => {
                Platform::emit_linux_syscall(self, macos_num);
            }
        }
    }

    // ── Platform-aware C symbol call ─────────────────────────────────

    /// Emit `bl _func` (macOS) or `bl func` (Linux) for C library calls.
    pub fn bl_c(&mut self, func: &str) {
        match self.platform {
            Platform::MacOS => self.instruction(&format!("bl _{}", func)),
            Platform::Linux => {
                let remapped = Platform::remap_c_symbol(func);
                self.instruction(&format!("bl {}", remapped));
            }
        }
    }

    // ── Platform-aware entry point ───────────────────────────────────

    /// Emit the program entry point label: `_main` (macOS) or `main` (Linux).
    pub fn entry_label(&mut self) {
        match self.platform {
            Platform::MacOS => self.label_global("_main"),
            Platform::Linux => self.label_global("main"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_comment_prefix_is_platform_aware() {
        let mut mac = Emitter::new();
        mac.platform = Platform::MacOS;
        mac.comment("-- block --");
        assert_eq!(mac.output(), "    ; -- block --\n");

        let mut linux = Emitter::new();
        linux.platform = Platform::Linux;
        linux.comment("-- block --");
        assert_eq!(linux.output(), "    // -- block --\n");
    }
}
