use std::process::Command;
use std::sync::OnceLock;

/// Target platform for code generation.
///
/// elephc always emits ARM64 assembly but the syscall convention, relocation
/// syntax, symbol naming, and struct layouts differ between macOS and Linux.
/// Rather than threading the platform through every emitter call, we generate
/// macOS-flavoured assembly and then post-process it into Linux assembly when
/// the target is Linux.  Only the handful of places where struct layouts
/// differ (e.g. `stat`) need to consult the platform at emit time.
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
    /// Auto-detect the host operating system.
    pub fn detect_host() -> Self {
        if cfg!(target_os = "macos") {
            Platform::MacOS
        } else {
            Platform::Linux
        }
    }
}

impl Arch {
    /// Auto-detect the host architecture.
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

    /// Auto-detect the platform + architecture we are compiling on.
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
        self.arch == Arch::AArch64
    }

    pub fn ensure_aarch64_backend(&self, feature: &str) {
        assert!(
            self.arch == Arch::AArch64,
            "{} is not implemented yet for target {}",
            feature,
            self.as_str()
        );
    }

    /// Transform macOS-flavoured AArch64 assembly into Linux AArch64 (ELF) assembly.
    ///
    /// This remains a same-ISA post-processing step. Cross-ISA targets such as
    /// Linux x86_64 must use native emission rather than textual rewriting.
    #[allow(dead_code)]
    pub fn transform_assembly(&self, asm: &str) -> String {
        match (self.platform, self.arch) {
            (Platform::MacOS, Arch::AArch64) => asm.to_string(),
            (Platform::Linux, Arch::AArch64) => transform_for_linux(asm),
            _ => asm.to_string(),
        }
    }
}

impl std::fmt::Display for Target {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Linux assembly transform (used by test harness for raw assembly strings)
// ────────────────────────────────────────────────────────────────────────────

/// macOS syscall number → Linux aarch64 syscall number.
#[allow(dead_code)]
fn map_syscall(macos_num: u32) -> u32 {
    match macos_num {
        1 => 93,    // exit
        3 => 63,    // read
        4 => 64,    // write
        5 => 56,    // openat (Linux aarch64 has no `open`; we use openat with AT_FDCWD)
        6 => 57,    // close
        10 => 35,   // unlinkat (Linux aarch64 has no `unlink`; we use unlinkat)
        12 => 49,   // chdir
        15 => 52,   // fchmod (placeholder — chmod maps to fchmodat on Linux)
        33 => 48,   // faccessat (Linux aarch64 has no `access`)
        116 => 169, // gettimeofday
        128 => 38,  // renameat (Linux aarch64 has no `rename`)
        136 => 34,  // mkdirat (Linux aarch64 has no `mkdir`)
        137 => 35,  // unlinkat (rmdir uses unlinkat with AT_REMOVEDIR flag)
        199 => 62,  // lseek
        338 => 79,  // fstatat (Linux aarch64 has no `stat`; uses fstatat/newfstatat)
        _ => panic!(
            "unknown macOS syscall number {} — cannot map to Linux",
            macos_num
        ),
    }
}

/// Known C library symbols that use Mach-O underscore prefix on macOS.
/// On Linux ELF, these are referenced without the leading underscore.
#[allow(dead_code)]
const C_SYMBOLS: &[&str] = &[
    "abs",
    "acos",
    "arc4random",
    "arc4random_uniform",
    "asin",
    "atan",
    "atan2",
    "atof",
    "atoi",
    "closedir",
    "cos",
    "cosh",
    "exp",
    "fgetc",
    "free",
    "getcwd",
    "getenv",
    "glob",
    "globfree",
    "hypot",
    "localtime",
    "log",
    "log10",
    "log2",
    "longjmp",
    "malloc",
    "memcpy",
    "memset",
    "mkstemp",
    "mktime",
    "opendir",
    "pclose",
    "popen",
    "pow",
    "putenv",
    "readdir",
    "regcomp",
    "regexec",
    "regfree",
    "setjmp",
    "sin",
    "sinh",
    "sleep",
    "snprintf",
    "system",
    "tan",
    "tanh",
    "time",
    "usleep",
];

#[allow(dead_code)]
fn is_c_symbol(name: &str) -> bool {
    C_SYMBOLS.binary_search(&name).is_ok()
}

/// Syscalls that on Linux need an AT_FDCWD (-100) prefix in x0 because
/// Linux aarch64 only provides `*at` variants (openat, mkdirat, etc.).
/// macOS syscall → needs AT_FDCWD setup before svc.
#[allow(dead_code)]
fn needs_at_fdcwd(macos_num: u32) -> bool {
    matches!(macos_num, 5 | 10 | 33 | 128 | 136 | 137 | 338)
}

#[allow(dead_code)]
fn transform_for_linux(asm: &str) -> String {
    let mut result = String::with_capacity(asm.len());
    let lines: Vec<&str> = asm.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim();

        // ── @PAGE / @PAGEOFF relocations ──────────────────────────────
        if let Some(transformed) = transform_relocation(line) {
            result.push_str(&transformed);
            result.push('\n');
            i += 1;
            continue;
        }

        // ── Syscall: mov x16, #N → mov x8, #M (only when followed by svc) ─
        if let Some(macos_num) = parse_syscall_mov(trimmed) {
            // Only transform if the next non-empty line is svc #0x80
            let next_line = lines.get(i + 1).map(|l| l.trim()).unwrap_or("");
            if !next_line.starts_with("svc #0x80") {
                // Not a syscall — x16 used as a general register, pass through
                result.push_str(line);
                result.push('\n');
                i += 1;
                continue;
            }

            let linux_num = map_syscall(macos_num);
            let indent = &line[..line.len() - trimmed.len()];

            // For *at syscalls, we need to set x0 = AT_FDCWD (-100) first.
            // But the original macOS code has x0 = path pointer in most cases.
            // The *at variants shift args: x0=dirfd, x1=path, x2=...
            // This requires per-syscall argument fixup.
            //
            // For openat:  x0=AT_FDCWD, x1=path, x2=flags, x3=mode
            //   macOS open: x0=path, x1=flags, x2=mode
            // For mkdirat: x0=AT_FDCWD, x1=path, x2=mode
            //   macOS mkdir: x0=path, x1=mode
            // For unlinkat: x0=AT_FDCWD, x1=path, x2=flags
            //   macOS unlink: x0=path (no flags)
            //   macOS rmdir:  x0=path → unlinkat with AT_REMOVEDIR
            // For faccessat: x0=AT_FDCWD, x1=path, x2=mode, x3=flags
            //   macOS access: x0=path, x1=mode
            // For renameat: x0=AT_FDCWD, x1=old, x2=AT_FDCWD, x3=new
            //   macOS rename: x0=old, x1=new
            // For fstatat:  x0=AT_FDCWD, x1=path, x2=buf, x3=flags
            //   macOS stat64: x0=path, x1=buf

            if needs_at_fdcwd(macos_num) {
                // Shift arguments right and insert AT_FDCWD in x0
                match macos_num {
                    128 => {
                        // rename(old, new) → renameat(AT_FDCWD, old, AT_FDCWD, new)
                        // x0=old, x1=new → x0=AT_FDCWD, x1=old, x2=AT_FDCWD, x3=new
                        result.push_str(&format!("{}mov x3, x1\n", indent)); // shift new path to x3
                        result.push_str(&format!("{}mov x1, x0\n", indent)); // shift old path to x1
                        result.push_str(&format!("{}mov x2, #-100\n", indent)); // AT_FDCWD for new path dir
                        result.push_str(&format!("{}mov x0, #-100\n", indent)); // AT_FDCWD for old path dir
                    }
                    338 => {
                        // stat64(path, buf) → fstatat(AT_FDCWD, path, buf, 0)
                        // x0=path, x1=buf → x0=AT_FDCWD, x1=path, x2=buf, x3=0
                        result.push_str(&format!("{}mov x2, x1\n", indent)); // shift buf to x2
                        result.push_str(&format!("{}mov x1, x0\n", indent)); // shift path to x1
                        result.push_str(&format!("{}mov x0, #-100\n", indent)); // AT_FDCWD
                        result.push_str(&format!("{}mov x3, #0\n", indent)); // flags = 0
                    }
                    5 => {
                        // open(path, flags, mode) → openat(AT_FDCWD, path, flags, mode)
                        // x0=path, x1=flags, x2=mode → x0=AT_FDCWD, x1=path, x2=flags, x3=mode
                        result.push_str(&format!("{}mov x3, x2\n", indent)); // shift mode to x3
                        result.push_str(&format!("{}mov x2, x1\n", indent)); // shift flags to x2
                        result.push_str(&format!("{}mov x1, x0\n", indent)); // shift path to x1
                        result.push_str(&format!("{}mov x0, #-100\n", indent)); // AT_FDCWD
                    }
                    136 => {
                        // mkdir(path, mode) → mkdirat(AT_FDCWD, path, mode)
                        // x0=path, x1=mode → x0=AT_FDCWD, x1=path, x2=mode
                        result.push_str(&format!("{}mov x2, x1\n", indent)); // shift mode to x2
                        result.push_str(&format!("{}mov x1, x0\n", indent)); // shift path to x1
                        result.push_str(&format!("{}mov x0, #-100\n", indent)); // AT_FDCWD
                    }
                    10 => {
                        // unlink(path) → unlinkat(AT_FDCWD, path, 0)
                        // x0=path → x0=AT_FDCWD, x1=path, x2=0
                        result.push_str(&format!("{}mov x1, x0\n", indent)); // shift path to x1
                        result.push_str(&format!("{}mov x0, #-100\n", indent)); // AT_FDCWD
                        result.push_str(&format!("{}mov x2, #0\n", indent)); // flags = 0
                    }
                    137 => {
                        // rmdir(path) → unlinkat(AT_FDCWD, path, AT_REMOVEDIR)
                        // x0=path → x0=AT_FDCWD, x1=path, x2=0x200
                        result.push_str(&format!("{}mov x1, x0\n", indent)); // shift path to x1
                        result.push_str(&format!("{}mov x0, #-100\n", indent)); // AT_FDCWD
                        result.push_str(&format!("{}mov x2, #0x200\n", indent));
                        // AT_REMOVEDIR
                    }
                    33 => {
                        // access(path, mode) → faccessat(AT_FDCWD, path, mode, 0)
                        // x0=path, x1=mode → x0=AT_FDCWD, x1=path, x2=mode, x3=0
                        result.push_str(&format!("{}mov x2, x1\n", indent)); // shift mode to x2
                        result.push_str(&format!("{}mov x1, x0\n", indent)); // shift path to x1
                        result.push_str(&format!("{}mov x0, #-100\n", indent)); // AT_FDCWD
                        result.push_str(&format!("{}mov x3, #0\n", indent)); // flags = 0
                    }
                    _ => unreachable!(),
                }
            }

            result.push_str(&format!("{}mov x8, #{}\n", indent, linux_num));
            i += 1;
            continue;
        }

        // ── svc #0x80 → svc #0 ───────────────────────────────────────
        if trimmed == "svc #0x80"
            || trimmed.starts_with("svc #0x80 ")
            || trimmed.starts_with("svc #0x80\t")
        {
            let indent = &line[..line.len() - trimmed.len()];
            result.push_str(indent);
            result.push_str("svc #0\n");
            i += 1;
            continue;
        }

        // ── Entry point: _main → main ────────────────────────────────
        if trimmed == "_main:" {
            result.push_str("main:\n");
            i += 1;
            continue;
        }
        if trimmed == ".globl _main" {
            result.push_str(".globl main\n");
            i += 1;
            continue;
        }

        // ── C library calls: bl _func → bl func ─────────────────────
        if let Some(transformed) = transform_c_call(trimmed) {
            let indent = &line[..line.len() - trimmed.len()];
            result.push_str(indent);
            result.push_str(&transformed);
            result.push('\n');
            i += 1;
            continue;
        }

        // ── Assembly comments: ; → // (GNU as uses // for ARM64) ─────
        if let Some(pos) = trimmed.find("; ") {
            let indent = &line[..line.len() - trimmed.len()];
            let before = &trimmed[..pos];
            let after = &trimmed[pos + 2..];
            result.push_str(indent);
            result.push_str(before);
            result.push_str("// ");
            result.push_str(after);
            result.push('\n');
            i += 1;
            continue;
        }

        // ── Default: pass through unchanged ──────────────────────────
        result.push_str(line);
        result.push('\n');
        i += 1;
    }

    result
}

/// Transform @PAGE / @PAGEOFF relocations to Linux ELF syntax.
///
/// macOS:  `adrp x9, symbol@PAGE`       → Linux: `adrp x9, symbol`
/// macOS:  `add x9, x9, symbol@PAGEOFF` → Linux: `add x9, x9, :lo12:symbol`
/// macOS:  `ldr d0, [x9, symbol@PAGEOFF]`→ Linux: `ldr d0, [x9, :lo12:symbol]`
#[allow(dead_code)]
fn transform_relocation(line: &str) -> Option<String> {
    // Quick check — avoids allocations for the 99% of lines without relocations.
    if !line.contains("@PAGE") && !line.contains("@GOT") {
        return None;
    }

    let mut result = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();

    while let Some(&ch) = chars.peek() {
        // Look for @PAGE, @PAGEOFF, @GOTPAGE, @GOTPAGEOFF
        if ch == '@' {
            let rest: String = chars.clone().collect();
            if rest.starts_with("@GOTPAGEOFF") {
                // Replace "symbol@GOTPAGEOFF" with ":got_lo12:symbol"
                let symbol_start = result
                    .rfind(|c: char| !c.is_alphanumeric() && c != '_')
                    .map(|i| i + 1)
                    .unwrap_or(0);
                let symbol = result[symbol_start..].to_string();
                result.truncate(symbol_start);
                result.push_str(&format!(":got_lo12:{}", symbol));
                for _ in 0..11 {
                    chars.next();
                } // skip "@GOTPAGEOFF"
            } else if rest.starts_with("@GOTPAGE") {
                // Replace "symbol@GOTPAGE" with ":got:symbol"
                let symbol_start = result
                    .rfind(|c: char| !c.is_alphanumeric() && c != '_')
                    .map(|i| i + 1)
                    .unwrap_or(0);
                let symbol = result[symbol_start..].to_string();
                result.truncate(symbol_start);
                result.push_str(&format!(":got:{}", symbol));
                for _ in 0..8 {
                    chars.next();
                } // skip "@GOTPAGE"
            } else if rest.starts_with("@PAGEOFF") {
                // Replace "symbol@PAGEOFF" with ":lo12:symbol"
                let symbol_start = result
                    .rfind(|c: char| !c.is_alphanumeric() && c != '_')
                    .map(|i| i + 1)
                    .unwrap_or(0);
                let symbol = result[symbol_start..].to_string();
                result.truncate(symbol_start);
                result.push_str(&format!(":lo12:{}", symbol));
                for _ in 0..8 {
                    chars.next();
                } // skip "@PAGEOFF"
            } else if rest.starts_with("@PAGE") {
                // Just remove "@PAGE"
                for _ in 0..5 {
                    chars.next();
                }
            } else {
                result.push(ch);
                chars.next();
            }
        } else {
            result.push(ch);
            chars.next();
        }
    }

    Some(result)
}

/// Parse `mov x16, #N` and return the macOS syscall number.
#[allow(dead_code)]
fn parse_syscall_mov(trimmed: &str) -> Option<u32> {
    let rest = trimmed.strip_prefix("mov x16, #")?;
    // The number may be followed by whitespace or end of line
    let num_str = rest.split_whitespace().next().unwrap_or(rest);
    num_str.parse::<u32>().ok()
}

/// Remap macOS-specific library symbols to their Linux equivalents.
#[allow(dead_code)]
fn remap_symbol(name: &str) -> &str {
    match name {
        // CommonCrypto → OpenSSL/libcrypto names on Linux
        "CC_MD5" => "MD5",
        "CC_SHA1" => "SHA1",
        "CC_SHA256" => "SHA256",
        _ => name,
    }
}

/// Transform C library calls: `bl _func` → `bl func` for known C symbols only.
/// Internal compiler-generated symbols (like `_method_*`, `_str_*`) are NOT touched.
#[allow(dead_code)]
fn transform_c_call(trimmed: &str) -> Option<String> {
    let rest = trimmed.strip_prefix("bl _")?;
    // Don't touch internal runtime labels (they start with another underscore)
    if rest.starts_with('_') {
        return None;
    }
    // Extract function name (up to whitespace or end)
    let func_name = rest.split_whitespace().next().unwrap_or(rest);
    // Check for platform-specific symbol remapping first (e.g., CC_MD5 → MD5)
    let remapped = remap_symbol(func_name);
    if remapped != func_name {
        return Some(format!("bl {}", remapped));
    }
    // Only strip underscore for known C library symbols
    if is_c_symbol(func_name) {
        return Some(format!("bl {}", rest));
    }
    // Everything else (internal labels, FFI externs) — leave unchanged
    None
}

// ────────────────────────────────────────────────────────────────────────────
// Platform-dependent struct layout constants
// ────────────────────────────────────────────────────────────────────────────

fn host_has_native_aarch64_toolchain() -> bool {
    static NATIVE_AARCH64: OnceLock<bool> = OnceLock::new();
    *NATIVE_AARCH64.get_or_init(|| {
        Command::new("gcc")
            .arg("-dumpmachine")
            .output()
            .map(|output| String::from_utf8_lossy(&output.stdout).contains("aarch64"))
            .unwrap_or(false)
    })
}

impl Platform {
    // File open flags (differ between macOS and Linux)
    pub fn o_wronly_creat_trunc(&self) -> u32 {
        match self {
            Platform::MacOS => 0x601, // O_WRONLY(1)|O_CREAT(0x200)|O_TRUNC(0x400)
            Platform::Linux => 0x241, // O_WRONLY(1)|O_CREAT(0x40)|O_TRUNC(0x200)
        }
    }

    pub fn o_wronly_creat_append(&self) -> u32 {
        match self {
            Platform::MacOS => 0x209, // O_WRONLY(1)|O_CREAT(0x200)|O_APPEND(8)
            Platform::Linux => 0x441, // O_WRONLY(1)|O_CREAT(0x40)|O_APPEND(0x400)
        }
    }

    /// Emit a branch-on-syscall-success instruction.
    /// macOS uses carry flag; Linux uses negative return value.
    pub fn branch_on_syscall_success(&self, label: &str) -> String {
        match self {
            Platform::MacOS => format!("b.cc {}", label), // carry clear = success
            Platform::Linux => format!("b.ge {}", label), // x0 >= 0 = success (after cmp x0, #0)
        }
    }

    /// Whether an explicit `cmp x0, #0` is needed before branching on syscall error.
    /// macOS checks carry flag (no cmp needed); Linux checks x0 sign.
    pub fn needs_cmp_before_error_branch(&self) -> bool {
        matches!(self, Platform::Linux)
    }

    // stat struct offsets
    pub fn stat_buf_size(&self) -> usize {
        match self {
            Platform::MacOS => 144,
            Platform::Linux => 128,
        }
    }

    /// Offset of st_mode in the stat struct.
    pub fn stat_mode_offset(&self) -> usize {
        match self {
            Platform::MacOS => 4,
            Platform::Linux => 16,
        }
    }

    /// Width of st_mode field: macOS uses uint16, Linux uses uint32.
    pub fn stat_mode_load_instr(&self, dest: &str, base: &str, offset: usize) -> String {
        match self {
            Platform::MacOS => format!("ldrh {}, [{}, #{}]", dest, base, offset),
            Platform::Linux => format!("ldr {}, [{}, #{}]", dest, base, offset),
        }
    }

    /// Offset of st_size in the stat struct.
    pub fn stat_size_offset(&self) -> usize {
        match self {
            Platform::MacOS => 96,
            Platform::Linux => 48,
        }
    }

    /// Offset of st_mtime (tv_sec) in the stat struct.
    pub fn stat_mtime_offset(&self) -> usize {
        match self {
            Platform::MacOS => 48,
            Platform::Linux => 88,
        }
    }

    /// Offset of `d_name` inside libc's `struct dirent`.
    pub fn dirent_name_offset(&self) -> usize {
        match self {
            Platform::MacOS => 21,
            Platform::Linux => 19,
        }
    }

    /// Offset of `gl_pathv` inside libc's `glob_t`.
    pub fn glob_pathv_offset(&self) -> usize {
        match self {
            Platform::MacOS => 32,
            Platform::Linux => 8,
        }
    }

    /// Size of libc's opaque `regex_t`.
    pub fn regex_t_size(&self) -> usize {
        match self {
            Platform::MacOS => 32,
            Platform::Linux => 64,
        }
    }

    /// Size of libc's `regmatch_t`.
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

    /// Offset of `rm_eo` inside `regmatch_t`.
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

    /// Load a signed `regoff_t` from `regmatch_t`.
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

impl Target {
    pub fn line_comment_prefix(&self) -> &'static str {
        match (self.platform, self.arch) {
            (Platform::MacOS, Arch::AArch64) => ";",
            (Platform::Linux, Arch::AArch64) => "//",
            (_, Arch::X86_64) => "#",
        }
    }

    /// Emit the Linux syscall sequence for a given macOS syscall number.
    /// Handles AT_FDCWD argument shifting for *at syscall variants.
    pub fn emit_linux_syscall(&self, emitter: &mut super::emit::Emitter, macos_num: u32) {
        self.ensure_aarch64_backend("linux syscall emission");
        let linux_num = map_syscall(macos_num);

        if needs_at_fdcwd(macos_num) {
            match macos_num {
                128 => {
                    // rename(old, new) → renameat(AT_FDCWD, old, AT_FDCWD, new)
                    emitter.instruction("mov x3, x1"); // shift new path to x3
                    emitter.instruction("mov x1, x0"); // shift old path to x1
                    emitter.instruction("mov x2, #-100"); // AT_FDCWD for new path dir
                    emitter.instruction("mov x0, #-100"); // AT_FDCWD for old path dir
                }
                338 => {
                    // stat64(path, buf) → fstatat(AT_FDCWD, path, buf, 0)
                    emitter.instruction("mov x2, x1"); // shift buf to x2
                    emitter.instruction("mov x1, x0"); // shift path to x1
                    emitter.instruction("mov x0, #-100"); // AT_FDCWD
                    emitter.instruction("mov x3, #0"); // flags = 0
                }
                5 => {
                    // open(path, flags, mode) → openat(AT_FDCWD, path, flags, mode)
                    emitter.instruction("mov x3, x2"); // shift mode to x3
                    emitter.instruction("mov x2, x1"); // shift flags to x2
                    emitter.instruction("mov x1, x0"); // shift path to x1
                    emitter.instruction("mov x0, #-100"); // AT_FDCWD
                }
                136 => {
                    // mkdir(path, mode) → mkdirat(AT_FDCWD, path, mode)
                    emitter.instruction("mov x2, x1"); // shift mode to x2
                    emitter.instruction("mov x1, x0"); // shift path to x1
                    emitter.instruction("mov x0, #-100"); // AT_FDCWD
                }
                10 => {
                    // unlink(path) → unlinkat(AT_FDCWD, path, 0)
                    emitter.instruction("mov x1, x0"); // shift path to x1
                    emitter.instruction("mov x0, #-100"); // AT_FDCWD
                    emitter.instruction("mov x2, #0"); // flags = 0
                }
                137 => {
                    // rmdir(path) → unlinkat(AT_FDCWD, path, AT_REMOVEDIR)
                    emitter.instruction("mov x1, x0"); // shift path to x1
                    emitter.instruction("mov x0, #-100"); // AT_FDCWD
                    emitter.instruction("mov x2, #0x200"); // AT_REMOVEDIR
                }
                33 => {
                    // access(path, mode) → faccessat(AT_FDCWD, path, mode, 0)
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

    /// Remap a C library symbol name for the target when needed.
    /// E.g. CommonCrypto functions → OpenSSL/libcrypto names on Linux.
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

    /// Apply the platform's external symbol naming convention.
    pub fn extern_symbol(&self, name: &str) -> String {
        match self.platform {
            Platform::MacOS => format!("_{}", name),
            Platform::Linux => name.to_string(),
        }
    }

    /// Get the assembler command for this target.
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

    /// Get the linker/gcc command for this target.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_target_parse() {
        assert_eq!(
            Target::parse("linux-aarch64").unwrap(),
            Target::new(Platform::Linux, Arch::AArch64)
        );
        assert_eq!(
            Target::parse("linux-x86_64").unwrap(),
            Target::new(Platform::Linux, Arch::X86_64)
        );
        assert_eq!(
            Target::parse("aarch64-apple-darwin").unwrap(),
            Target::new(Platform::MacOS, Arch::AArch64)
        );
    }

    #[test]
    fn test_transform_relocation_page() {
        let input = "    adrp x9, _global_argc@PAGE";
        let result = transform_relocation(input).unwrap();
        assert_eq!(result, "    adrp x9, _global_argc");
    }

    #[test]
    fn test_transform_relocation_pageoff() {
        let input = "    add x9, x9, _global_argc@PAGEOFF";
        let result = transform_relocation(input).unwrap();
        assert_eq!(result, "    add x9, x9, :lo12:_global_argc");
    }

    #[test]
    fn test_transform_relocation_ldr_pageoff() {
        let input = "    ldr d0, [x9, _pi_const@PAGEOFF]";
        let result = transform_relocation(input).unwrap();
        assert_eq!(result, "    ldr d0, [x9, :lo12:_pi_const]");
    }

    #[test]
    fn test_no_relocation() {
        let input = "    mov x0, #1";
        assert!(transform_relocation(input).is_none());
    }

    #[test]
    fn test_parse_syscall_mov() {
        assert_eq!(parse_syscall_mov("mov x16, #4"), Some(4));
        assert_eq!(parse_syscall_mov("mov x16, #338"), Some(338));
        assert_eq!(parse_syscall_mov("mov x0, #1"), None);
    }

    #[test]
    fn test_map_syscall() {
        assert_eq!(map_syscall(1), 93); // exit
        assert_eq!(map_syscall(4), 64); // write
        assert_eq!(map_syscall(5), 56); // open → openat
        assert_eq!(map_syscall(338), 79); // stat64 → fstatat
    }

    #[test]
    fn test_transform_c_call() {
        assert_eq!(
            transform_c_call("bl _snprintf"),
            Some("bl snprintf".to_string())
        );
        assert_eq!(transform_c_call("bl __rt_itoa"), None); // internal, not touched
        assert_eq!(transform_c_call("bl _sin"), Some("bl sin".to_string()));
        assert_eq!(transform_c_call("bl _CC_MD5"), Some("bl MD5".to_string()));
    }

    #[test]
    fn test_non_syscall_x16_preserved() {
        let macos_asm = "    mov x16, #0\n    str x16, [sp]\n";
        let linux_asm = transform_for_linux(macos_asm);
        // x16 used as general register, not followed by svc — should be unchanged
        assert!(linux_asm.contains("mov x16, #0"));
    }

    #[test]
    fn test_full_linux_transform() {
        let macos_asm = "\
.globl _main
_main:
    adrp x9, _global_argc@PAGE
    add x9, x9, _global_argc@PAGEOFF
    mov x0, #1
    mov x16, #4
    svc #0x80
    bl _snprintf
    mov x16, #1
    svc #0x80
";
        let linux_asm = transform_for_linux(macos_asm);
        assert!(linux_asm.contains(".globl main\n"));
        assert!(linux_asm.contains("main:\n"));
        assert!(linux_asm.contains("adrp x9, _global_argc\n"));
        assert!(linux_asm.contains("add x9, x9, :lo12:_global_argc\n"));
        assert!(linux_asm.contains("mov x8, #64\n")); // write
        assert!(linux_asm.contains("svc #0\n"));
        assert!(linux_asm.contains("bl snprintf\n"));
        assert!(linux_asm.contains("mov x8, #93\n")); // exit
        assert!(!linux_asm.contains("x16"));
        assert!(!linux_asm.contains("@PAGE"));
    }

    #[test]
    fn test_openat_arg_shift() {
        // open(path, flags, mode) → openat(AT_FDCWD, path, flags, mode)
        let macos_asm = "    mov x16, #5\n    svc #0x80\n";
        let linux_asm = transform_for_linux(macos_asm);
        assert!(linux_asm.contains("mov x3, x2")); // shift mode
        assert!(linux_asm.contains("mov x2, x1")); // shift flags
        assert!(linux_asm.contains("mov x1, x0")); // shift path
        assert!(linux_asm.contains("mov x0, #-100")); // AT_FDCWD
        assert!(linux_asm.contains("mov x8, #56")); // openat
    }

    #[test]
    fn test_linux_libc_layout_offsets() {
        assert_eq!(Platform::MacOS.dirent_name_offset(), 21);
        assert_eq!(Platform::Linux.dirent_name_offset(), 19);
        assert_eq!(Platform::MacOS.glob_pathv_offset(), 32);
        assert_eq!(Platform::Linux.glob_pathv_offset(), 8);
        assert_eq!(Platform::MacOS.regex_t_size(), 32);
        assert_eq!(Platform::Linux.regex_t_size(), 64);
        assert_eq!(Platform::MacOS.regmatch_t_size(), 16);
        assert_eq!(
            Platform::Linux.regmatch_t_size(),
            if cfg!(target_env = "musl") { 16 } else { 8 }
        );
        assert_eq!(Platform::MacOS.regmatch_rm_eo_offset(), 8);
        assert_eq!(
            Platform::Linux.regmatch_rm_eo_offset(),
            if cfg!(target_env = "musl") { 8 } else { 4 }
        );
        assert_eq!(
            Platform::MacOS.regoff_load_instr("x9", "sp", 32),
            "ldr x9, [sp, #32]"
        );
        assert_eq!(
            Platform::Linux.regoff_load_instr("x9", "sp", 32),
            if cfg!(target_env = "musl") {
                "ldr x9, [sp, #32]"
            } else {
                "ldrsw x9, [sp, #32]"
            }
        );
    }
}
