//! Purpose:
//! Provides the small target-aware emission vocabulary used by float sorting.
//!
//! Called from:
//! - The sibling float-sort entry, comparison, insertion and partition emitters.
//!
//! Key details:
//! - All persistent state lives in aligned ABI frames, including across recursive calls.
//! - Registers are caller-saved argument registers; stack slots are positive frame offsets.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Branch relation shared by the two supported instruction sets.
#[derive(Clone, Copy)]
pub(super) enum Cond { Eq, Ne, Lt, Le, Gt, Ge }

/// Emits the float sort's pointer-word operations through target-aware ABI helpers.
pub(super) struct Asm<'a> { pub emitter: &'a mut Emitter }

impl Asm<'_> {
    /// Returns one caller-saved integer argument register.
    pub fn reg(&self, index: usize) -> &'static str {
        abi::int_arg_reg_name(self.emitter.target, index)
    }

    /// Starts an aligned frame with enough private slots for every float-sort helper.
    pub fn begin(&mut self, label: &str) {
        self.emitter.blank();
        self.emitter.label_global(label);
        abi::emit_frame_prologue(self.emitter, 208);
    }

    /// Restores the caller frame and returns without disturbing the result.
    pub fn ret(&mut self) {
        abi::emit_frame_restore(self.emitter, 208);
        abi::emit_return(self.emitter);
    }

    /// Introduces one local control-flow destination.
    pub fn label(&mut self, label: &str) { self.emitter.label(label); }

    /// Emits an unconditional local transfer.
    pub fn jump(&mut self, label: &str) { abi::emit_jump(self.emitter, label); }

    /// Loads a private frame slot into an argument register.
    pub fn load(&mut self, reg: usize, slot: usize) {
        let r = self.reg(reg);
        abi::load_at_offset(self.emitter, r, slot);
    }

    /// Preserves an argument register in a private frame slot.
    pub fn save(&mut self, slot: usize, reg: usize) {
        let r = self.reg(reg);
        abi::store_at_offset(self.emitter, r, slot);
    }

    /// Preserves a nested call's return value in a private frame slot.
    pub fn save_result(&mut self, slot: usize) {
        let result = abi::int_result_reg(self.emitter);
        abi::store_at_offset(self.emitter, result, slot);
    }

    /// Materializes an immediate in an argument register.
    pub fn imm(&mut self, reg: usize, value: i64) {
        let r = self.reg(reg);
        abi::emit_load_int_immediate(self.emitter, r, value);
    }

    /// Loads a pointer word at a constant byte offset.
    pub fn read(&mut self, dst: usize, base: usize, offset: usize) {
        let (d, b) = (self.reg(dst), self.reg(base));
        abi::emit_load_from_address(self.emitter, d, b, offset);
    }

    /// Stores a pointer word at a constant byte offset.
    pub fn write(&mut self, src: usize, base: usize, offset: usize) {
        let (s, b) = (self.reg(src), self.reg(base));
        abi::emit_store_to_address(self.emitter, s, b, offset);
    }

    /// Addresses one eight-byte payload or ordinal slot.
    pub fn index(&mut self, dst: usize, base: usize, index: usize) {
        // -- compute a pointer-word element address --
        let (d, b, i) = (self.reg(dst), self.reg(base), self.reg(index));
        match self.emitter.target.arch {
            Arch::AArch64 => self.emitter.instruction(&format!("add {d}, {b}, {i}, lsl #3")), // address one pointer-sized element
            Arch::X86_64 => self.emitter.instruction(&format!("lea {d}, [{b} + {i} * 8]")), // address one pointer-sized element
        }
    }

    /// Adds a small signed constant to an argument register.
    pub fn offset(&mut self, reg: usize, amount: i64) {
        // -- advance a bounded sort cursor --
        let r = self.reg(reg);
        match self.emitter.target.arch {
            Arch::AArch64 => {
                let op = if amount < 0 { "sub" } else { "add" };
                self.emitter.instruction(&format!("{op} {r}, {r}, #{}", amount.abs())); // advance the sort cursor by a fixed amount
            }
            Arch::X86_64 => self.emitter.instruction(&format!("add {r}, {amount}")), // advance the sort cursor by a fixed amount
        }
    }

    /// Adds or subtracts another register without clobbering it.
    pub fn combine(&mut self, dst: usize, rhs: usize, subtract: bool) {
        // -- combine partition bounds --
        let (d, r) = (self.reg(dst), self.reg(rhs));
        let op = if subtract { "sub" } else { "add" };
        match self.emitter.target.arch {
            Arch::AArch64 => self.emitter.instruction(&format!("{op} {d}, {d}, {r}")), // combine the range bounds or cursor offsets
            Arch::X86_64 => self.emitter.instruction(&format!("{op} {d}, {r}")), // combine the range bounds or cursor offsets
        }
    }

    /// Scales a nonnegative element count or computes a partition midpoint.
    pub fn shift(&mut self, reg: usize, amount: u8, left: bool) {
        // -- scale a nonnegative count --
        let r = self.reg(reg);
        match self.emitter.target.arch {
            Arch::AArch64 => {
                let op = if left { "lsl" } else { "lsr" };
                self.emitter.instruction(&format!("{op} {r}, {r}, #{amount}")); // scale a count by a power of two
            }
            Arch::X86_64 => {
                let op = if left { "shl" } else { "shr" };
                self.emitter.instruction(&format!("{op} {r}, {amount}"));       // scale a count by a power of two
            }
        }
    }

    /// Branches using the flags set by a preceding integer comparison.
    fn branch(&mut self, cond: Cond, label: &str) {
        // -- select the target conditional branch --
        let op = match (self.emitter.target.arch, cond) {
            (Arch::AArch64, Cond::Eq) => "b.eq", (Arch::X86_64, Cond::Eq) => "je",
            (Arch::AArch64, Cond::Ne) => "b.ne", (Arch::X86_64, Cond::Ne) => "jne",
            (Arch::AArch64, Cond::Lt) => "b.lt", (Arch::X86_64, Cond::Lt) => "jl",
            (Arch::AArch64, Cond::Le) => "b.le", (Arch::X86_64, Cond::Le) => "jle",
            (Arch::AArch64, Cond::Gt) => "b.gt", (Arch::X86_64, Cond::Gt) => "jg",
            (Arch::AArch64, Cond::Ge) => "b.ge", (Arch::X86_64, Cond::Ge) => "jge",
        };
        self.emitter.instruction(&format!("{op} {label}"));                     // take the selected sort control-flow edge
    }

    /// Compares two argument registers and branches on their signed relation.
    pub fn cmp(&mut self, lhs: usize, rhs: usize, cond: Cond, label: &str) {
        // -- compare two bounded positions --
        self.emitter.instruction(&format!("cmp {}, {}", self.reg(lhs), self.reg(rhs))); // compare nonnegative range positions
        self.branch(cond, label);
    }

    /// Tests a register against a small constant without changing its value.
    pub fn cmp_imm(&mut self, reg: usize, value: i64, cond: Cond, label: &str) {
        // -- compare a count or flag with a constant --
        let prefix = if self.emitter.target.arch == Arch::AArch64 { "#" } else { "" };
        self.emitter.instruction(&format!("cmp {}, {prefix}{value}", self.reg(reg))); // test a length, cursor or direction flag
        self.branch(cond, label);
    }

    /// Tests the nested comparator's result without assuming result/argument register aliasing.
    pub fn cmp_result(&mut self, cond: Cond, label: &str) {
        // -- inspect the shared comparator result --
        let result = abi::int_result_reg(self.emitter);
        let prefix = if self.emitter.target.arch == Arch::AArch64 { "#" } else { "" };
        self.emitter.instruction(&format!("cmp {result}, {prefix}0"));          // inspect the shared PHP comparator result
        self.branch(cond, label);
    }

    /// Loads caller-visible arguments from frame slots and calls a runtime label.
    pub fn call(&mut self, label: &str, slots: &[usize]) {
        for (reg, slot) in slots.iter().enumerate() { self.load(reg, *slot); }
        abi::emit_call_label(self.emitter, label);
    }
}
