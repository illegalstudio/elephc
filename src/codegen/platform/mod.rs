//! Purpose:
//! Collects platform target descriptions, toolchain selection, and Linux assembly transforms.
//! Provides target metadata used before and after user assembly emission.
//!
//! Called from:
//! - `crate::codegen::generate()` and pipeline linking support
//!
//! Key details:
//! - Target identity controls assembly syntax, object format, and post-processing assumptions.

mod linux_transform;
mod target;
mod toolchain;

pub use target::{Arch, Platform, Target};

#[cfg(test)]
mod tests {
    use super::linux_transform::{
        map_syscall, parse_syscall_mov, transform_c_call, transform_for_linux,
        transform_relocation,
    };
    use super::*;

    #[test]
    /// Parsing "linux-aarch64", "linux-x86_64", and "aarch64-apple-darwin" returns the correct Platform+Arch pair.
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
    /// "adrp x9, _global_argc@PAGE" strips the @PAGE suffix (Linux has no PAGE relocations).
    fn test_transform_relocation_page() {
        let input = "    adrp x9, _global_argc@PAGE";
        let result = transform_relocation(input).unwrap();
        assert_eq!(result, "    adrp x9, _global_argc");
    }

    #[test]
    /// "add x9, x9, _global_argc@PAGEOFF" becomes ":lo12:" form on Linux.
    fn test_transform_relocation_pageoff() {
        let input = "    add x9, x9, _global_argc@PAGEOFF";
        let result = transform_relocation(input).unwrap();
        assert_eq!(result, "    add x9, x9, :lo12:_global_argc");
    }

    #[test]
    /// "ldr d0, [x9, _pi_const@PAGEOFF]" converts to :lo12: form for Linux.
    fn test_transform_relocation_ldr_pageoff() {
        let input = "    ldr d0, [x9, _pi_const@PAGEOFF]";
        let result = transform_relocation(input).unwrap();
        assert_eq!(result, "    ldr d0, [x9, :lo12:_pi_const]");
    }

    #[test]
    /// "mov x0, #1" contains no relocation markers so returns None unchanged.
    fn test_no_relocation() {
        let input = "    mov x0, #1";
        assert!(transform_relocation(input).is_none());
    }

    #[test]
    /// "mov x16, #4" extracts 4; "mov x0, #1" returns None (wrong register).
    fn test_parse_syscall_mov() {
        assert_eq!(parse_syscall_mov("mov x16, #4"), Some(4));
        assert_eq!(parse_syscall_mov("mov x16, #338"), Some(338));
        assert_eq!(parse_syscall_mov("mov x0, #1"), None);
    }

    #[test]
    /// macOS syscall numbers 1, 4, 5, 128, 338 map to Linux aarch64 syscall numbers 93, 64, 56, 38, 79.
    fn test_map_syscall() {
        assert_eq!(map_syscall(1), 93);
        assert_eq!(map_syscall(4), 64);
        assert_eq!(map_syscall(5), 56);
        assert_eq!(map_syscall(29), 207);
        assert_eq!(map_syscall(30), 202);
        assert_eq!(map_syscall(31), 205);
        assert_eq!(map_syscall(32), 204);
        assert_eq!(map_syscall(54), 29);
        assert_eq!(map_syscall(92), 25);
        assert_eq!(map_syscall(93), 72);
        assert_eq!(map_syscall(97), 198);
        assert_eq!(map_syscall(98), 203);
        assert_eq!(map_syscall(104), 200);
        assert_eq!(map_syscall(105), 208);
        assert_eq!(map_syscall(106), 201);
        assert_eq!(map_syscall(128), 38);
        assert_eq!(map_syscall(133), 206);
        assert_eq!(map_syscall(134), 210);
        assert_eq!(map_syscall(135), 199);
        assert_eq!(map_syscall(160), 160);
        assert_eq!(map_syscall(338), 79);
        assert_eq!(map_syscall(345), 43);
    }

    #[test]
    /// "bl _snprintf" becomes "bl snprintf" (known C symbol, underscore stripped).
    /// "bl __rt_itoa" returns None (runtime internal, not a C symbol).
    /// "bl _sin" becomes "bl sin". "bl _CC_MD5" becomes "bl MD5" (CommonCrypto remap).
    fn test_transform_c_call() {
        assert_eq!(
            transform_c_call("bl _snprintf"),
            Some("bl snprintf".to_string())
        );
        assert_eq!(transform_c_call("bl __rt_itoa"), None);
        assert_eq!(transform_c_call("bl _sin"), Some("bl sin".to_string()));
        assert_eq!(transform_c_call("bl _CC_MD5"), Some("bl MD5".to_string()));
    }

    #[test]
    /// Non-syscall "mov x16, #0" is preserved as-is when transforming to Linux.
    fn test_non_syscall_x16_preserved() {
        let macos_asm = "    mov x16, #0\n    str x16, [sp]\n";
        let linux_asm = transform_for_linux(macos_asm);
        assert!(linux_asm.contains("mov x16, #0"));
    }

    #[test]
    /// Full end-to-end transform: _main → main, @PAGE/@PAGEOFF → :lo12:, svc #0x80 → svc #0,
    /// x16 syscall numbers → x8 Linux numbers, bl _snprintf → bl snprintf.
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
        assert!(linux_asm.contains("mov x8, #64\n"));
        assert!(linux_asm.contains("svc #0\n"));
        assert!(linux_asm.contains("bl snprintf\n"));
        assert!(linux_asm.contains("mov x8, #93\n"));
        assert!(!linux_asm.contains("x16"));
        assert!(!linux_asm.contains("@PAGE"));
    }

    #[test]
    /// "mov x16, #5" (macOS openat) emits argument shifts to reorder path/flags/dirfd for Linux's newfstatat.
    fn test_openat_arg_shift() {
        let macos_asm = "    mov x16, #5\n    svc #0x80\n";
        let linux_asm = transform_for_linux(macos_asm);
        assert!(linux_asm.contains("mov x3, x2"));
        assert!(linux_asm.contains("mov x2, x1"));
        assert!(linux_asm.contains("mov x1, x0"));
        assert!(linux_asm.contains("mov x0, #-100"));
        assert!(linux_asm.contains("mov x8, #56"));
    }

    #[test]
    /// PCRE2 POSIX-wrapper regex structs use one layout across supported targets.
    fn test_regex_layout_offsets() {
        assert_eq!(Platform::MacOS.dirent_name_offset(), 21);
        assert_eq!(Platform::Linux.dirent_name_offset(), 19);
        assert_eq!(Platform::MacOS.glob_pathv_offset(), 32);
        assert_eq!(Platform::Linux.glob_pathv_offset(), 8);
        assert_eq!(Platform::MacOS.regex_t_size(), 48);
        assert_eq!(Platform::Linux.regex_t_size(), 48);
        assert_eq!(Platform::MacOS.regex_re_nsub_offset(), 24);
        assert_eq!(Platform::Linux.regex_re_nsub_offset(), 24);
        assert_eq!(Platform::MacOS.regmatch_t_size(), 8);
        assert_eq!(Platform::Linux.regmatch_t_size(), 8);
        assert_eq!(Platform::MacOS.regmatch_rm_eo_offset(), 4);
        assert_eq!(Platform::Linux.regmatch_rm_eo_offset(), 4);
        assert_eq!(
            Platform::MacOS.regoff_load_instr("x9", "sp", 32),
            "ldrsw x9, [sp, #32]"
        );
        assert_eq!(
            Platform::Linux.regoff_load_instr("x9", "sp", 32),
            "ldrsw x9, [sp, #32]"
        );
    }
}
