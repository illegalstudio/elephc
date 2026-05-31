//! Purpose:
//! Defines target triples, architecture/platform enums, and derived codegen properties.
//! Maps user-facing target choices to assembly, object, and linker conventions.
//!
//! Called from:
//! - `crate::codegen::platform` and pipeline target selection
//!
//! Key details:
//! - Architecture and platform decisions here gate every downstream ABI helper.

use super::linux_transform::{map_syscall, needs_at_fdcwd, transform_for_linux};
use super::toolchain::host_has_native_aarch64_toolchain;

/// Target platform for code generation.
///
/// elephc emits target-specific assembly for the supported platform/architecture
/// pairs while keeping OS concerns separate from ISA concerns. Platform controls
/// syscall convention, relocation syntax, symbol naming, and struct layouts;
/// architecture controls registers, calling convention, and runtime slices.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    MacOS,
    Linux,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Target architecture.
pub enum Arch {
    AArch64,
    X86_64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Target representation.
pub struct Target {
    pub platform: Platform,
    pub arch: Arch,
}

impl Platform {
    /// Detects the host operating system from the Rust compile-time target OS.
    ///
    /// Returns `Platform::MacOS` when compiling on macOS, otherwise `Platform::Linux`.
    pub fn detect_host() -> Self {
        if cfg!(target_os = "macos") {
            Platform::MacOS
        } else {
            Platform::Linux
        }
    }

    /// Returns the PHP-compatible OS name string for this platform.
    ///
    /// macOS reports `"Darinux"` and Linux reports `"Linux"`, matching PHP's `PHP_OS` constant.
    pub fn php_os_name(&self) -> &'static str {
        match self {
            Platform::MacOS => "Darwin",
            Platform::Linux => "Linux",
        }
    }

    /// Returns the `O_WRONLY | O_CREAT | O_TRUNC` flag combination for `open()`.
    ///
    /// These flags open a file for writing, creating it if it does not exist,
    /// and truncating it to zero length if it does. Platform values differ in
    /// the high bits used for mode flags.
    pub fn o_wronly_creat_trunc(&self) -> u32 {
        match self {
            Platform::MacOS => 0x601,
            Platform::Linux => 0x241,
        }
    }

    /// Returns the `O_WRONLY | O_CREAT` flag combination for `open()`.
    ///
    /// Opens an existing file for writing or creates a new file; does not truncate.
    pub fn o_wronly_creat(&self) -> u32 {
        match self {
            Platform::MacOS => 0x201,
            Platform::Linux => 0x41,
        }
    }

    /// Returns the `O_WRONLY | O_CREAT | O_APPEND` flag combination for `open()`.
    ///
    /// Opens or creates a file for writing, with all writes appended to the end.
    pub fn o_wronly_creat_append(&self) -> u32 {
        match self {
            Platform::MacOS => 0x209,
            Platform::Linux => 0x441,
        }
    }

    /// Emits a conditional branch instruction that jumps to `label` on syscall success.
    ///
    /// macOS uses `b.cc` (conditional continue) and Linux uses `b.ge` (greater-than-or-equal),
    /// since Linux syscalls return 0 or positive on success, negative on error.
    pub fn branch_on_syscall_success(&self, label: &str) -> String {
        match self {
            Platform::MacOS => format!("b.cc {}", label),
            Platform::Linux => format!("b.ge {}", label),
        }
    }

    /// Returns `true` if the platform requires a `cmp` instruction before an error branch.
    ///
    /// Linux syscall results need a comparison against zero before branching on error,
    /// whereas macOS uses the condition flags set directly by `svc`.
    pub fn needs_cmp_before_error_branch(&self) -> bool {
        matches!(self, Platform::Linux)
    }

    /// Returns the size of `struct stat` for this platform in bytes.
    ///
    /// Used when allocating the stat buffer passed to `*at()` syscalls.
    pub fn stat_buf_size(&self) -> usize {
        match self {
            Platform::MacOS => 144,
            Platform::Linux => 128,
        }
    }

    /// Returns the byte offset of `st_mode` within `struct stat`.
    pub fn stat_mode_offset(&self) -> usize {
        match self {
            Platform::MacOS => 4,
            Platform::Linux => 16,
        }
    }

    /// Returns the ARM64 load instruction to read `st_mode` from a stat buffer.
    ///
    /// macOS uses `ldrh` (unsigned halfword) since `st_mode` is 16 bits;
    /// Linux uses `ldr` (word) since `st_mode` is 32 bits.
    pub fn stat_mode_load_instr(&self, dest: &str, base: &str, offset: usize) -> String {
        match self {
            Platform::MacOS => format!("ldrh {}, [{}, #{}]", dest, base, offset),
            Platform::Linux => format!("ldr {}, [{}, #{}]", dest, base, offset),
        }
    }

    /// Returns the byte offset of `st_size` within `struct stat`.
    pub fn stat_size_offset(&self) -> usize {
        match self {
            Platform::MacOS => 96,
            Platform::Linux => 48,
        }
    }

    /// Returns the byte offset of `st_mtime` within `struct stat`.
    pub fn stat_mtime_offset(&self) -> usize {
        match self {
            Platform::MacOS => 48,
            Platform::Linux => 88,
        }
    }

    /// Returns the byte offset of `st_atime` within `struct stat`.
    pub fn stat_atime_offset(&self) -> usize {
        match self {
            Platform::MacOS => 32,
            Platform::Linux => 72,
        }
    }

    /// Returns the byte offset of `st_ctime` within `struct stat`.
    pub fn stat_ctime_offset(&self) -> usize {
        match self {
            Platform::MacOS => 64,
            Platform::Linux => 104,
        }
    }

    /// Returns the byte offset of `st_ino` within `struct stat`.
    pub fn stat_ino_offset(&self) -> usize {
        match self {
            Platform::MacOS => 8,
            Platform::Linux => 8,
        }
    }

    /// Returns the byte offset of `st_uid` within `struct stat`.
    pub fn stat_uid_offset(&self) -> usize {
        match self {
            Platform::MacOS => 16,
            Platform::Linux => 24,
        }
    }

    /// Returns the byte offset of `st_gid` within `struct stat`.
    pub fn stat_gid_offset(&self) -> usize {
        match self {
            Platform::MacOS => 20,
            Platform::Linux => 28,
        }
    }

    /// Returns the byte offset of `st_dev` within `struct stat`.
    ///
    /// Darwin stores `st_dev` as a signed 32-bit `int32_t`; Linux uses `__dev_t` (64-bit).
    /// Both platforms place `st_dev` at offset 0.
    pub fn stat_dev_offset(&self) -> usize {
        match self {
            Platform::MacOS => 0,
            Platform::Linux => 0,
        }
    }

    /// Returns the byte offset of `st_rdev` within `struct stat`.
    pub fn stat_rdev_offset(&self) -> usize {
        match self {
            Platform::MacOS => 24,
            Platform::Linux => 32,
        }
    }

    /// Returns the byte offset of `st_nlink` within `struct stat`.
    pub fn stat_nlink_offset(&self) -> usize {
        match self {
            Platform::MacOS => 6,
            Platform::Linux => 20,
        }
    }

    /// Returns the byte offset of `st_blksize` within `struct stat`.
    pub fn stat_blksize_offset(&self) -> usize {
        match self {
            Platform::MacOS => 112,
            Platform::Linux => 56,
        }
    }

    /// Returns the byte offset of `st_blocks` within `struct stat`.
    pub fn stat_blocks_offset(&self) -> usize {
        match self {
            Platform::MacOS => 104,
            Platform::Linux => 64,
        }
    }

    /// Returns the ARM64 load instruction for `st_dev`, accounting for its width on each platform.
    ///
    /// Darwin stores `st_dev` as a signed 32-bit field loaded with `ldrsw` (sign-extending word);
    /// Linux stores it as a 64-bit value loaded with `ldr` (zero-extending word).
    pub fn stat_dev_load_instr(&self, dest_x: &str, base: &str, offset: usize) -> String {
        match self {
            Platform::MacOS => format!("ldrsw {}, [{}, #{}]", dest_x, base, offset),
            Platform::Linux => format!("ldr {}, [{}, #{}]", dest_x, base, offset),
        }
    }

    /// Returns the ARM64 load instruction for `st_rdev`, accounting for its width on each platform.
    ///
    /// Same semantics as `stat_dev_load_instr`: Darwin uses `ldrsw`, Linux uses `ldr`.
    pub fn stat_rdev_load_instr(&self, dest_x: &str, base: &str, offset: usize) -> String {
        match self {
            Platform::MacOS => format!("ldrsw {}, [{}, #{}]", dest_x, base, offset),
            Platform::Linux => format!("ldr {}, [{}, #{}]", dest_x, base, offset),
        }
    }

    /// Returns the ARM64 load instruction for `st_nlink`, accounting for its width on each platform.
    ///
    /// Darwin packs `st_nlink` into 16 bits (loaded with `ldrh`); Linux uses 32 bits (loaded with `ldr`).
    pub fn stat_nlink_load_instr(&self, dest_w: &str, base: &str, offset: usize) -> String {
        match self {
            Platform::MacOS => format!("ldrh {}, [{}, #{}]", dest_w, base, offset),
            Platform::Linux => format!("ldr {}, [{}, #{}]", dest_w, base, offset),
        }
    }

    /// Returns the platform-native value of `AT_FDCWD` for `*at()` syscalls.
    ///
    /// macOS uses `-2` and Linux uses `-100`. The libc `*at()` family functions
    /// consume this platform-native value directly.
    pub fn at_fdcwd(&self) -> i64 {
        match self {
            Platform::MacOS => -2,
            Platform::Linux => -100,
        }
    }

    /// Returns the `tv_nsec` value used to set file access/modify times to "now".
    ///
    /// macOS uses `-1` which preserves the existing timestamp; Linux uses `0x3FFF_FFFF`
    /// as a sentinel meaning "current time".
    pub fn utime_now_nsec(&self) -> i64 {
        match self {
            Platform::MacOS => -1,
            Platform::Linux => 0x3FFF_FFFF,
        }
    }

    /// Returns the byte offset of `d_name` within `struct dirent`.
    pub fn dirent_name_offset(&self) -> usize {
        match self {
            Platform::MacOS => 21,
            Platform::Linux => 19,
        }
    }

    /// Returns the byte offset of `gl_pathv` within `struct glob`.
    pub fn glob_pathv_offset(&self) -> usize {
        match self {
            Platform::MacOS => 32,
            Platform::Linux => 8,
        }
    }

    /// Returns the size of PCRE2 POSIX-wrapper `struct regex_t` in bytes.
    pub fn regex_t_size(&self) -> usize {
        48
    }

    /// Returns the byte offset of `re_nsub` within PCRE2 POSIX-wrapper `struct regex_t`.
    pub fn regex_re_nsub_offset(&self) -> usize {
        24
    }

    /// Returns the value of `LC_CTYPE` for `setlocale()`.
    pub fn lc_ctype(&self) -> u32 {
        match self {
            Platform::MacOS => 2,
            Platform::Linux => 0,
        }
    }

    /// Returns the size of PCRE2 POSIX-wrapper `struct regmatch_t` in bytes.
    pub fn regmatch_t_size(&self) -> usize {
        8
    }

    /// Returns the byte offset of `rm_eo` within PCRE2 POSIX-wrapper `struct regmatch_t`.
    pub fn regmatch_rm_eo_offset(&self) -> usize {
        4
    }

    /// Returns the ARM64 load instruction for a `regoff_t` field (regex match offset).
    ///
    /// PCRE2's POSIX wrapper uses signed 32-bit offsets on all supported targets.
    pub fn regoff_load_instr(&self, dest: &str, base: &str, offset: usize) -> String {
        format!("ldrsw {}, [{}, #{}]", dest, base, offset)
    }
}

impl Arch {
    /// Detects the host architecture from the Rust compile-time target architecture.
    ///
    /// Returns `Arch::AArch64` on ARM64 hosts and `Arch::X86_64` on x86_64 hosts.
    /// Panics if running on an unsupported architecture.
    pub fn detect_host() -> Self {
        if cfg!(target_arch = "aarch64") {
            Arch::AArch64
        } else if cfg!(target_arch = "x86_64") {
            Arch::X86_64
        } else {
            panic!("unsupported host architecture for elephc")
        }
    }
}

impl Target {
    /// Constructs a `Target` from a `Platform` and `Arch`.
    pub const fn new(platform: Platform, arch: Arch) -> Self {
        Self { platform, arch }
    }

    /// Detects the host platform and architecture from the Rust compile-time target.
    ///
    /// Combines `Platform::detect_host()` and `Arch::detect_host()` into a single `Target`.
    pub fn detect_host() -> Self {
        Self::new(Platform::detect_host(), Arch::detect_host())
    }

    /// Parses a target string into a `Target`.
    ///
    /// Supported values: `macos-aarch64`, `macos-arm64`, `aarch64-apple-darwin`,
    /// `macos-x86_64`, `x86_64-apple-darwin`, `linux-aarch64`, `linux-arm64`,
    /// `aarch64-unknown-linux-gnu`, `linux-x86_64`, `x86_64-unknown-linux-gnu`.
    /// Returns an error for any unrecognized string.
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "macos-aarch64" | "macos-arm64" | "aarch64-apple-darwin" => {
                Ok(Self::new(Platform::MacOS, Arch::AArch64))
            }
            "macos-x86_64" | "x86_64-apple-darwin" => {
                Ok(Self::new(Platform::MacOS, Arch::X86_64))
            }
            "linux-aarch64" | "linux-arm64" | "aarch64-unknown-linux-gnu" => {
                Ok(Self::new(Platform::Linux, Arch::AArch64))
            }
            "linux-x86_64" | "x86_64-unknown-linux-gnu" => {
                Ok(Self::new(Platform::Linux, Arch::X86_64))
            }
            _ => Err(format!(
                "unsupported target '{}'; expected one of: macos-aarch64, macos-x86_64, linux-aarch64, linux-x86_64",
                value
            )),
        }
    }

    /// Returns the canonical string representation of this target.
    ///
    /// Returns one of: `"macos-aarch64"`, `"macos-x86_64"`, `"linux-aarch64"`, `"linux-x86_64"`.
    pub fn as_str(&self) -> &'static str {
        match (self.platform, self.arch) {
            (Platform::MacOS, Arch::AArch64) => "macos-aarch64",
            (Platform::MacOS, Arch::X86_64) => "macos-x86_64",
            (Platform::Linux, Arch::AArch64) => "linux-aarch64",
            (Platform::Linux, Arch::X86_64) => "linux-x86_64",
        }
    }

    /// Returns `true` if this target has a working codegen backend.
    ///
    /// Currently returns `true` for all targets except `macos-x86_64`, which is not yet implemented.
    pub fn supports_current_backend(&self) -> bool {
        matches!(
            (self.platform, self.arch),
            (Platform::MacOS, Arch::AArch64)
                | (Platform::Linux, Arch::AArch64)
                | (Platform::Linux, Arch::X86_64)
        )
    }

    /// Returns the Darwin architecture name used in Mach-O files and `AS`/`LD` flags.
    ///
    /// Returns `"arm64"` for `AArch64` and `"x86_64"` for `X86_64`.
    pub fn darwin_arch_name(&self) -> &'static str {
        match self.arch {
            Arch::AArch64 => "arm64",
            Arch::X86_64 => "x86_64",
        }
    }

    /// Panics with a descriptive message if the target is not AArch64.
    ///
    /// Used to gate AArch64-only codegen paths. The `feature` argument is included
    /// in the assertion message to identify which codegen feature is missing.
    pub fn ensure_aarch64_backend(&self, feature: &str) {
        assert!(
            self.arch == Arch::AArch64,
            "{} is not implemented yet for target {}",
            feature,
            self.as_str()
        );
    }

    /// Applies platform-specific assembly transforms to the input string.
    ///
    /// On macOS, returns the input unchanged. On Linux ARM64, applies the transform
    /// from `super::linux_transform` to convert macOS-style assembly to Linux style.
    #[allow(dead_code)]
    pub fn transform_assembly(&self, asm: &str) -> String {
        match (self.platform, self.arch) {
            (Platform::MacOS, Arch::AArch64) => asm.to_string(),
            (Platform::Linux, Arch::AArch64) => transform_for_linux(asm),
            _ => asm.to_string(),
        }
    }

    /// Returns the line comment prefix used by the assembler for this target.
    ///
    /// macOS ARM64 uses `;`, Linux ARM64 uses `//`, and x86_64 (both platforms) uses `#`.
    pub fn line_comment_prefix(&self) -> &'static str {
        match (self.platform, self.arch) {
            (Platform::MacOS, Arch::AArch64) => ";",
            (Platform::Linux, Arch::AArch64) => "//",
            (_, Arch::X86_64) => "#",
        }
    }

    /// Emits Linux ARM64 syscall code for a macOS syscall number.
    ///
    /// Transforms the macOS syscall number to its Linux equivalent using `map_syscall`,
    /// emits `AT_FDCWD` setup for `*at()` family syscalls when required, then loads
    /// the Linux syscall number into `x8` and invokes `svc #0`. Panics if the target
    /// is not ARM64 or if the syscall number is not recognized.
    pub fn emit_linux_syscall(&self, emitter: &mut super::super::emit::Emitter, macos_num: u32) {
        self.ensure_aarch64_backend("linux syscall emission");
        let linux_num = map_syscall(macos_num);

        if needs_at_fdcwd(macos_num) {
            match macos_num {
                128 => {
                    emitter.instruction("mov x3, x1");                          // shift new path to x3
                    emitter.instruction("mov x1, x0");                          // shift old path to x1
                    emitter.instruction("mov x2, #-100");                       // AT_FDCWD for new path dir
                    emitter.instruction("mov x0, #-100");                       // AT_FDCWD for old path dir
                }
                338 => {
                    emitter.instruction("mov x2, x1");                          // shift buf to x2
                    emitter.instruction("mov x1, x0");                          // shift path to x1
                    emitter.instruction("mov x0, #-100");                       // AT_FDCWD
                    emitter.instruction("mov x3, #0");                          // flags = 0
                }
                340 => {
                    emitter.instruction("mov x2, x1");                          // shift buf to x2
                    emitter.instruction("mov x1, x0");                          // shift path to x1
                    emitter.instruction("mov x0, #-100");                       // AT_FDCWD
                    emitter.instruction("mov x3, #0x100");                      // AT_SYMLINK_NOFOLLOW (lstat semantics)
                }
                5 => {
                    emitter.instruction("mov x3, x2");                          // shift mode to x3
                    emitter.instruction("mov x2, x1");                          // shift flags to x2
                    emitter.instruction("mov x1, x0");                          // shift path to x1
                    emitter.instruction("mov x0, #-100");                       // AT_FDCWD
                }
                136 => {
                    emitter.instruction("mov x2, x1");                          // shift mode to x2
                    emitter.instruction("mov x1, x0");                          // shift path to x1
                    emitter.instruction("mov x0, #-100");                       // AT_FDCWD
                }
                10 => {
                    emitter.instruction("mov x1, x0");                          // shift path to x1
                    emitter.instruction("mov x0, #-100");                       // AT_FDCWD
                    emitter.instruction("mov x2, #0");                          // flags = 0
                }
                137 => {
                    emitter.instruction("mov x1, x0");                          // shift path to x1
                    emitter.instruction("mov x0, #-100");                       // AT_FDCWD
                    emitter.instruction("mov x2, #0x200");                      // AT_REMOVEDIR
                }
                33 => {
                    emitter.instruction("mov x2, x1");                          // shift mode to x2
                    emitter.instruction("mov x1, x0");                          // shift path to x1
                    emitter.instruction("mov x0, #-100");                       // AT_FDCWD
                    emitter.instruction("mov x3, #0");                          // flags = 0
                }
                _ => unreachable!(),
            }
        }

        emitter.instruction(&format!("mov x8, #{}", linux_num));                // load the Linux syscall number into x8
        emitter.instruction("svc #0");                                          // invoke the Linux kernel supervisor call
    }

    /// Remaps CommonCrypto symbol names to their Linux OpenSSL equivalents on Linux.
    ///
    /// On macOS returns the name unchanged. On Linux maps `CC_MD5` → `MD5`,
    /// `CC_SHA1` → `SHA1`, `CC_SHA256` → `SHA256`, and passes through all other names.
    pub fn remap_c_symbol<'a>(&self, name: &'a str) -> &'a str {
        match self.platform {
            Platform::MacOS => name,
            Platform::Linux => match name {
                "CC_MD5" => "MD5",
                "CC_SHA1" => "SHA1",
                "CC_SHA256" => "SHA256",
                _ => name,
            },
        }
    }

    /// Returns the platform-mangled extern symbol name.
    ///
    /// macOS prefixes C symbols with `_` (e.g., `"printf"` → `"_printf"`);
    /// Linux returns the name unchanged.
    pub fn extern_symbol(&self, name: &str) -> String {
        match self.platform {
            Platform::MacOS => format!("_{}", name),
            Platform::Linux => name.to_string(),
        }
    }

    /// Returns the assembler command used to assemble `.s` files for this target.
    ///
    /// On macOS always uses `as`. On Linux ARM64 uses `as` if a native toolchain
    /// is available, otherwise `aarch64-linux-gnu-as`. On Linux x86_64 uses `as`.
    pub fn assembler_cmd(&self) -> &'static str {
        match (self.platform, self.arch) {
            (Platform::MacOS, Arch::AArch64 | Arch::X86_64) => "as",
            (Platform::Linux, Arch::AArch64) => {
                if host_has_native_aarch64_toolchain() {
                    "as"
                } else {
                    "aarch64-linux-gnu-as"
                }
            }
            (Platform::Linux, Arch::X86_64) => "as",
        }
    }

    /// Returns the linker command used to link object files into a final binary for this target.
    ///
    /// On macOS always uses `ld`. On Linux ARM64 uses `gcc` if a native toolchain
    /// is available, otherwise `aarch64-linux-gnu-gcc`. On Linux x86_64 uses `gcc`.
    pub fn linker_cmd(&self) -> &'static str {
        match (self.platform, self.arch) {
            (Platform::MacOS, Arch::AArch64 | Arch::X86_64) => "ld",
            (Platform::Linux, Arch::AArch64) => {
                if host_has_native_aarch64_toolchain() {
                    "gcc"
                } else {
                    "aarch64-linux-gnu-gcc"
                }
            }
            (Platform::Linux, Arch::X86_64) => "gcc",
        }
    }
}

impl std::fmt::Display for Target {
    /// Formats the target as its canonical string representation.
    ///
    /// Equivalent to calling `as_str()`, returning one of:
    /// `"macos-aarch64"`, `"macos-x86_64"`, `"linux-aarch64"`, `"linux-x86_64"`.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}
