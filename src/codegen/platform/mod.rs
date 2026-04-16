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
        assert_eq!(map_syscall(1), 93);
        assert_eq!(map_syscall(4), 64);
        assert_eq!(map_syscall(5), 56);
        assert_eq!(map_syscall(128), 38);
        assert_eq!(map_syscall(338), 79);
    }

    #[test]
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
    fn test_non_syscall_x16_preserved() {
        let macos_asm = "    mov x16, #0\n    str x16, [sp]\n";
        let linux_asm = transform_for_linux(macos_asm);
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
        assert!(linux_asm.contains("mov x8, #64\n"));
        assert!(linux_asm.contains("svc #0\n"));
        assert!(linux_asm.contains("bl snprintf\n"));
        assert!(linux_asm.contains("mov x8, #93\n"));
        assert!(!linux_asm.contains("x16"));
        assert!(!linux_asm.contains("@PAGE"));
    }

    #[test]
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
