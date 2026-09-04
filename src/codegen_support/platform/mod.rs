//! Purpose:
//! Collects platform target descriptions, toolchain selection, and Linux assembly transforms.
//! Provides target metadata used before and after user assembly emission.
//!
//! Called from:
//! - `crate::cli`, `crate::codegen`, and pipeline linking support.
//!
//! Key details:
//! - Target identity controls assembly syntax, object format, and post-processing assumptions.

mod linux_transform;
mod target;
mod toolchain;

pub use target::{AppleVariant, Arch, Platform, Target, APPLE_IOS_MIN_OS};

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

    /// Both spellings of each iOS target resolve to the same Darwin target with
    /// the right Apple variant — the platform stays `MacOS` because the ABI is.
    #[test]
    fn test_target_parse_ios_variants() {
        for spelling in ["ios-arm64", "ios-aarch64", "aarch64-apple-ios"] {
            let target = Target::parse(spelling).unwrap_or_else(|e| panic!("{spelling}: {e}"));
            assert_eq!(target, Target::new_apple(Arch::AArch64, AppleVariant::IOS));
            assert_eq!(target.platform, Platform::MacOS, "{spelling} shares the Darwin ABI");
        }
        for spelling in [
            "ios-sim-arm64",
            "ios-simulator-arm64",
            "aarch64-apple-ios-simulator",
        ] {
            let target = Target::parse(spelling).unwrap_or_else(|e| panic!("{spelling}: {e}"));
            assert_eq!(
                target,
                Target::new_apple(Arch::AArch64, AppleVariant::IOSSimulator)
            );
        }
    }

    /// iOS has no x86_64 backend, so legacy-looking device and Simulator
    /// spellings fail during target parsing instead of reaching codegen.
    #[test]
    fn test_target_parse_rejects_ios_x86_64_with_arm64_guidance() {
        for spelling in [
            "ios-x86_64",
            "x86_64-apple-ios",
            "ios-sim-x86_64",
            "x86_64-apple-ios-simulator",
        ] {
            let error = Target::parse(spelling).expect_err("x86_64 iOS must be rejected");
            assert!(error.contains("only arm64 iOS targets"), "{spelling}: {error}");
            assert!(error.contains("ios-arm64") && error.contains("ios-sim-arm64"));
        }
    }

    /// Three persisted keys derive from `as_str()` — the runtime object cache
    /// filename, the native-dependency receipt JSON and the package catalog — so
    /// two targets sharing a string silently reuse each other's artifacts.
    /// Device, simulator and macOS must stay distinguishable on the same arch.
    #[test]
    fn test_apple_variants_do_not_collide_in_the_canonical_string() {
        let macos = Target::new(Platform::MacOS, Arch::AArch64);
        let device = Target::new_apple(Arch::AArch64, AppleVariant::IOS);
        let simulator = Target::new_apple(Arch::AArch64, AppleVariant::IOSSimulator);

        let names = [macos.as_str(), device.as_str(), simulator.as_str()];
        let mut unique = names.to_vec();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), names.len(), "collision among {names:?}");

        // And each canonical string must round-trip back to the same target.
        for target in [macos, device, simulator] {
            assert_eq!(Target::parse(target.as_str()).unwrap(), target);
        }
    }

    /// The Apple variant drives SDK selection, the ld64 platform token and the
    /// compiler triple. These three strings are what actually differ between
    /// macOS and iOS; everything else about the target is shared.
    #[test]
    fn test_apple_variant_drives_sdk_platform_and_triple() {
        let macos = Target::new(Platform::MacOS, Arch::AArch64);
        assert_eq!(macos.apple_sdk_name(), "macosx");
        assert_eq!(macos.apple_platform_name(), "macos");
        assert_eq!(macos.apple_triple_os(), "darwin");

        let device = Target::new_apple(Arch::AArch64, AppleVariant::IOS);
        assert_eq!(device.apple_sdk_name(), "iphoneos");
        assert_eq!(device.apple_platform_name(), "ios");
        assert_eq!(device.apple_triple_os(), "ios");

        let simulator = Target::new_apple(Arch::AArch64, AppleVariant::IOSSimulator);
        assert_eq!(simulator.apple_sdk_name(), "iphonesimulator");
        assert_eq!(simulator.apple_platform_name(), "ios-simulator");
        assert_eq!(simulator.apple_triple_os(), "ios");
    }

    /// A Mach-O object records the platform it was assembled for, and `ld`
    /// refuses to link a macOS-tagged object into an iOS image. `as` cannot
    /// express "iOS", so non-macOS Apple targets assemble through `clang` and
    /// this triple is what carries the platform.
    ///
    /// The deployment floor must be the same value the linker records in
    /// `-platform_version`; a mismatch between an object's `LC_BUILD_VERSION`
    /// and the image's is itself a link error, which is why both read
    /// `APPLE_IOS_MIN_OS`.
    #[test]
    fn test_apple_clang_triple_carries_the_platform() {
        assert_eq!(
            Target::new(Platform::MacOS, Arch::AArch64).apple_clang_triple(),
            "arm64-apple-macos"
        );
        assert_eq!(
            Target::new_apple(Arch::AArch64, AppleVariant::IOS).apple_clang_triple(),
            format!("arm64-apple-ios{APPLE_IOS_MIN_OS}")
        );
        assert_eq!(
            Target::new_apple(Arch::AArch64, AppleVariant::IOSSimulator).apple_clang_triple(),
            format!("arm64-apple-ios{APPLE_IOS_MIN_OS}-simulator")
        );

        // Device and simulator must not share a triple: clang stamps a distinct
        // Mach-O platform from each, and ld rejects the wrong one.
        assert_ne!(
            Target::new_apple(Arch::AArch64, AppleVariant::IOS).apple_clang_triple(),
            Target::new_apple(Arch::AArch64, AppleVariant::IOSSimulator).apple_clang_triple()
        );
    }

    /// The arm64 Darwin backend serves iOS unchanged, so the central gate must
    /// already accept it — that equivalence is the whole reason iOS is a variant
    /// rather than a fourth `Platform`.
    #[test]
    fn test_ios_reuses_the_existing_aarch64_darwin_backend() {
        assert!(Target::new_apple(Arch::AArch64, AppleVariant::IOS).supports_current_backend());
        assert!(
            Target::new_apple(Arch::AArch64, AppleVariant::IOSSimulator)
                .supports_current_backend()
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
    /// macOS syscall numbers 1, 4, 5, 128, 338 map to Linux aarch64 syscall numbers 94, 64, 56, 38, 79.
    fn test_map_syscall() {
        assert_eq!(map_syscall(1), 94);
        assert_eq!(map_syscall(4), 64);
        assert_eq!(map_syscall(5), 56);
        // The process-identity trio behind is_readable()/is_writable(): getuid,
        // getgid, getgroups. Their macOS and Linux numbers share no digits, so a
        // transcription slip here answers the permission question about the
        // wrong identity rather than failing.
        assert_eq!(map_syscall(24), 174);
        assert_eq!(map_syscall(47), 176);
        assert_eq!(map_syscall(79), 158);
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
    /// "bl _sin" becomes "bl sin".
    fn test_transform_c_call() {
        assert_eq!(
            transform_c_call("bl _snprintf"),
            Some("bl snprintf".to_string())
        );
        assert_eq!(transform_c_call("bl __rt_itoa"), None);
        assert_eq!(transform_c_call("bl _sin"), Some("bl sin".to_string()));
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
        assert!(linux_asm.contains("mov x8, #94\n"));
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
    /// Directory and glob layouts retain their platform-specific offsets.
    fn test_directory_layout_offsets() {
        assert_eq!(Platform::MacOS.dirent_name_offset(), 21);
        assert_eq!(Platform::Linux.dirent_name_offset(), 19);
        assert_eq!(Platform::MacOS.glob_pathv_offset(), 32);
        assert_eq!(Platform::Linux.glob_pathv_offset(), 8);
    }

    #[test]
    /// The libc glob bits are the ones the target's own headers define.
    ///
    /// The table cannot be verified by reading it, and getting one bit wrong does not fail — it
    /// silently selects a different flag, which is what makes php's `GLOB_NOESCAPE` dangerous: its
    /// value is `GLOB_LIMIT` in macOS's glob.h. `libc::GLOB_*` is the target's own header, so this
    /// checks the macOS column on a mac and the glibc column wherever CI builds for Linux.
    fn test_glob_libc_flags_match_the_target_headers() {
        let host = if cfg!(target_os = "macos") {
            Platform::MacOS
        } else {
            Platform::Linux
        };
        let bits = host.glob_libc_flags();
        assert_eq!(bits.err, i64::from(libc::GLOB_ERR), "GLOB_ERR");
        assert_eq!(bits.mark, i64::from(libc::GLOB_MARK), "GLOB_MARK");
        assert_eq!(bits.nocheck, i64::from(libc::GLOB_NOCHECK), "GLOB_NOCHECK");
        assert_eq!(bits.nosort, i64::from(libc::GLOB_NOSORT), "GLOB_NOSORT");
        assert_eq!(bits.noescape, i64::from(libc::GLOB_NOESCAPE), "GLOB_NOESCAPE");
        #[cfg(target_os = "linux")]
        assert_eq!(bits.brace, i64::from(libc::GLOB_BRACE), "GLOB_BRACE");
        #[cfg(target_os = "macos")]
        // The `libc` crate does not export GLOB_BRACE for Apple targets; this is the SDK's own
        // /usr/include/glob.h value, read there rather than recalled.
        assert_eq!(bits.brace, 0x0080, "GLOB_BRACE");
    }

    #[test]
    /// php's glob flags are php's, and they are NOT the bits either libc gives those names.
    ///
    /// This is the property the whole translation exists for. If a future edit ever made the two
    /// agree, the translation would be dead code and this test says so out loud.
    fn test_php_glob_flags_are_not_the_libc_bits() {
        let php_noescape = crate::types::stream_constants::STREAM_INT_CONSTANTS
            .iter()
            .find(|(name, _)| *name == "GLOB_NOESCAPE")
            .expect("GLOB_NOESCAPE is a declared php constant")
            .1;
        // 4096 is GLOB_LIMIT on macOS and unassigned on glibc — never GLOB_NOESCAPE.
        assert_eq!(php_noescape, 4096);
        assert_ne!(php_noescape, Platform::MacOS.glob_libc_flags().noescape);
        assert_ne!(php_noescape, Platform::Linux.glob_libc_flags().noescape);
    }
}
