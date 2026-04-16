use super::linux_transform::{map_syscall, needs_at_fdcwd, transform_for_linux};
use super::toolchain::host_has_native_aarch64_toolchain;

/// Target platform for code generation.
///
/// elephc currently emits real AArch64 assembly only, but target selection is
/// already split into platform and architecture so backend-specific work can
/// grow without reintroducing "Linux => ARM64" assumptions. Platform still
/// controls the syscall convention, relocation syntax, symbol naming, and
/// struct layouts. On AArch64 Linux we currently reuse the existing same-ISA
/// post-processing path from the macOS-flavoured backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    MacOS,
    Linux,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arch {
    AArch64,
    X86_64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Target {
    pub platform: Platform,
    pub arch: Arch,
}

impl Platform {
    pub fn detect_host() -> Self {
        if cfg!(target_os = "macos") {
            Platform::MacOS
        } else {
            Platform::Linux
        }
    }

    pub fn o_wronly_creat_trunc(&self) -> u32 {
        match self {
            Platform::MacOS => 0x601,
            Platform::Linux => 0x241,
        }
    }

    pub fn o_wronly_creat_append(&self) -> u32 {
        match self {
            Platform::MacOS => 0x209,
            Platform::Linux => 0x441,
        }
    }

    pub fn branch_on_syscall_success(&self, label: &str) -> String {
        match self {
            Platform::MacOS => format!("b.cc {}", label),
            Platform::Linux => format!("b.ge {}", label),
        }
    }

    pub fn needs_cmp_before_error_branch(&self) -> bool {
        matches!(self, Platform::Linux)
    }

    pub fn stat_buf_size(&self) -> usize {
        match self {
            Platform::MacOS => 144,
            Platform::Linux => 128,
        }
    }

    pub fn stat_mode_offset(&self) -> usize {
        match self {
            Platform::MacOS => 4,
            Platform::Linux => 16,
        }
    }

    pub fn stat_mode_load_instr(&self, dest: &str, base: &str, offset: usize) -> String {
        match self {
            Platform::MacOS => format!("ldrh {}, [{}, #{}]", dest, base, offset),
            Platform::Linux => format!("ldr {}, [{}, #{}]", dest, base, offset),
        }
    }

    pub fn stat_size_offset(&self) -> usize {
        match self {
            Platform::MacOS => 96,
            Platform::Linux => 48,
        }
    }

    pub fn stat_mtime_offset(&self) -> usize {
        match self {
            Platform::MacOS => 48,
            Platform::Linux => 88,
        }
    }

    pub fn dirent_name_offset(&self) -> usize {
        match self {
            Platform::MacOS => 21,
            Platform::Linux => 19,
        }
    }

    pub fn glob_pathv_offset(&self) -> usize {
        match self {
            Platform::MacOS => 32,
            Platform::Linux => 8,
        }
    }

    pub fn regex_t_size(&self) -> usize {
        match self {
            Platform::MacOS => 32,
            Platform::Linux => 64,
        }
    }

    pub fn regmatch_t_size(&self) -> usize {
        match self {
            Platform::MacOS => 16,
            Platform::Linux => {
                if cfg!(target_env = "musl") {
                    16
                } else {
                    8
                }
            }
        }
    }

    pub fn regmatch_rm_eo_offset(&self) -> usize {
        match self {
            Platform::MacOS => 8,
            Platform::Linux => {
                if cfg!(target_env = "musl") {
                    8
                } else {
                    4
                }
            }
        }
    }

    pub fn regoff_load_instr(&self, dest: &str, base: &str, offset: usize) -> String {
        match self {
            Platform::MacOS => format!("ldr {}, [{}, #{}]", dest, base, offset),
            Platform::Linux => {
                if cfg!(target_env = "musl") {
                    format!("ldr {}, [{}, #{}]", dest, base, offset)
                } else {
                    format!("ldrsw {}, [{}, #{}]", dest, base, offset)
                }
            }
        }
    }
}

impl Arch {
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
    pub const fn new(platform: Platform, arch: Arch) -> Self {
        Self { platform, arch }
    }

    pub fn detect_host() -> Self {
        Self::new(Platform::detect_host(), Arch::detect_host())
    }

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

    pub fn as_str(&self) -> &'static str {
        match (self.platform, self.arch) {
            (Platform::MacOS, Arch::AArch64) => "macos-aarch64",
            (Platform::MacOS, Arch::X86_64) => "macos-x86_64",
            (Platform::Linux, Arch::AArch64) => "linux-aarch64",
            (Platform::Linux, Arch::X86_64) => "linux-x86_64",
        }
    }

    pub fn supports_current_backend(&self) -> bool {
        matches!(
            (self.platform, self.arch),
            (Platform::MacOS, Arch::AArch64)
                | (Platform::Linux, Arch::AArch64)
                | (Platform::Linux, Arch::X86_64)
        )
    }

    pub fn darwin_arch_name(&self) -> &'static str {
        match self.arch {
            Arch::AArch64 => "arm64",
            Arch::X86_64 => "x86_64",
        }
    }

    pub fn ensure_aarch64_backend(&self, feature: &str) {
        assert!(
            self.arch == Arch::AArch64,
            "{} is not implemented yet for target {}",
            feature,
            self.as_str()
        );
    }

    #[allow(dead_code)]
    pub fn transform_assembly(&self, asm: &str) -> String {
        match (self.platform, self.arch) {
            (Platform::MacOS, Arch::AArch64) => asm.to_string(),
            (Platform::Linux, Arch::AArch64) => transform_for_linux(asm),
            _ => asm.to_string(),
        }
    }

    pub fn line_comment_prefix(&self) -> &'static str {
        match (self.platform, self.arch) {
            (Platform::MacOS, Arch::AArch64) => ";",
            (Platform::Linux, Arch::AArch64) => "//",
            (_, Arch::X86_64) => "#",
        }
    }

    pub fn emit_linux_syscall(&self, emitter: &mut super::super::emit::Emitter, macos_num: u32) {
        self.ensure_aarch64_backend("linux syscall emission");
        let linux_num = map_syscall(macos_num);

        if needs_at_fdcwd(macos_num) {
            match macos_num {
                128 => {
                    emitter.instruction("mov x3, x1"); // shift new path to x3
                    emitter.instruction("mov x1, x0"); // shift old path to x1
                    emitter.instruction("mov x2, #-100"); // AT_FDCWD for new path dir
                    emitter.instruction("mov x0, #-100"); // AT_FDCWD for old path dir
                }
                338 => {
                    emitter.instruction("mov x2, x1"); // shift buf to x2
                    emitter.instruction("mov x1, x0"); // shift path to x1
                    emitter.instruction("mov x0, #-100"); // AT_FDCWD
                    emitter.instruction("mov x3, #0"); // flags = 0
                }
                5 => {
                    emitter.instruction("mov x3, x2"); // shift mode to x3
                    emitter.instruction("mov x2, x1"); // shift flags to x2
                    emitter.instruction("mov x1, x0"); // shift path to x1
                    emitter.instruction("mov x0, #-100"); // AT_FDCWD
                }
                136 => {
                    emitter.instruction("mov x2, x1"); // shift mode to x2
                    emitter.instruction("mov x1, x0"); // shift path to x1
                    emitter.instruction("mov x0, #-100"); // AT_FDCWD
                }
                10 => {
                    emitter.instruction("mov x1, x0"); // shift path to x1
                    emitter.instruction("mov x0, #-100"); // AT_FDCWD
                    emitter.instruction("mov x2, #0"); // flags = 0
                }
                137 => {
                    emitter.instruction("mov x1, x0"); // shift path to x1
                    emitter.instruction("mov x0, #-100"); // AT_FDCWD
                    emitter.instruction("mov x2, #0x200"); // AT_REMOVEDIR
                }
                33 => {
                    emitter.instruction("mov x2, x1"); // shift mode to x2
                    emitter.instruction("mov x1, x0"); // shift path to x1
                    emitter.instruction("mov x0, #-100"); // AT_FDCWD
                    emitter.instruction("mov x3, #0"); // flags = 0
                }
                _ => unreachable!(),
            }
        }

        emitter.instruction(&format!("mov x8, #{}", linux_num));
        emitter.instruction("svc #0");
    }

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

    pub fn extern_symbol(&self, name: &str) -> String {
        match self.platform {
            Platform::MacOS => format!("_{}", name),
            Platform::Linux => name.to_string(),
        }
    }

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
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}
