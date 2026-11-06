//! Purpose:
//! Owns the optional regex-provider ABI used by dynamic eval builtins.
//! Keeps libelephc_magician independent from PCRE2 until generated code
//! explicitly registers the managed native shim.
//!
//! Called from:
//! - Generated eval setup through `__elephc_eval_register_regex_provider()`.
//! - `crate::interpreter::builtins::regex` for opaque compile/execute/free calls.
//! - Eval builtin lookup when deciding whether `preg_*` names are available.
//!
//! Key details:
//! - Provider callbacks use the versioned managed PCRE2 shim ABI.
//! - Test builds install an equivalent host-PCRE2 provider without changing
//!   production staticlib dependencies.

use std::ffi::{c_char, c_void};
use std::sync::OnceLock;

/// Compiles one null-terminated regex and returns an opaque provider handle.
pub(crate) type RegexCompileFn =
    unsafe extern "C" fn(*mut *mut c_void, *const c_char, u32, *mut u64) -> i32;
/// Executes one compiled regex and fills signed start/end offset pairs.
pub(crate) type RegexExecFn =
    unsafe extern "C" fn(*mut c_void, *const c_char, u64, *mut i64, u32) -> i32;
/// Releases one opaque regex handle.
pub(crate) type RegexFreeFn = unsafe extern "C" fn(*mut c_void);

/// Registered callback table for the managed regex implementation.
#[derive(Clone, Copy)]
pub(crate) struct RegexProvider {
    pub(crate) compile: RegexCompileFn,
    pub(crate) exec: RegexExecFn,
    pub(crate) free: RegexFreeFn,
}

/// Process-wide provider selected before the first dynamic eval executes.
static REGEX_PROVIDER: OnceLock<RegexProvider> = OnceLock::new();

/// Registers the managed PCRE2 shim callbacks used by dynamic eval regex builtins.
///
/// Registration is idempotent because generated code may initialize more than
/// one eval call site. Returns `1` once a provider is available.
#[no_mangle]
pub extern "C" fn __elephc_eval_register_regex_provider(
    compile: RegexCompileFn,
    exec: RegexExecFn,
    free: RegexFreeFn,
) -> i32 {
    let _ = REGEX_PROVIDER.set(RegexProvider {
        compile,
        exec,
        free,
    });
    i32::from(regex_provider().is_some())
}

/// Returns whether dynamic eval may expose its regex builtin family.
pub(crate) fn regex_provider_available() -> bool {
    regex_provider().is_some()
}

/// Returns the registered provider, or the host-backed unit-test provider.
pub(crate) fn regex_provider() -> Option<RegexProvider> {
    #[cfg(test)]
    {
        return Some(test_provider::provider());
    }
    #[cfg(not(test))]
    {
        REGEX_PROVIDER.get().copied()
    }
}

#[cfg(test)]
mod test_provider {
    //! Purpose:
    //! Adapts host PCRE2 into the same opaque ABI used by managed native builds.
    //!
    //! Called from:
    //! - `super::regex_provider()` in elephc-magician unit tests.
    //!
    //! Key details:
    //! - Native link arguments are test-only directives from `build.rs`.
    //! - REG_STARTEND preserves the caller-supplied first offset pair.

    use std::ffi::{c_char, c_int, c_void};

    use libc::size_t;

    use super::RegexProvider;

    const REG_BADPAT: i32 = 3;
    const REG_ESPACE: i32 = 12;
    const REG_STARTEND: u32 = 0x0080;
    const ELEPHC_PCRE2_GLOBAL_NEXT: u32 = 1 << 16;
    const PCRE2_NOTEMPTY_ATSTART: u32 = 0x00000008;

    /// PCRE2 POSIX `regex_t` layout for the supported host wrapper ABI.
    #[repr(C)]
    struct Pcre2Regex {
        re_pcre2_code: *mut c_void,
        re_match_data: *mut c_void,
        re_endp: *const c_char,
        re_nsub: size_t,
        re_erroffset: size_t,
        re_cflags: c_int,
    }

    /// PCRE2 POSIX `regmatch_t` capture offset pair.
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct Pcre2Regmatch {
        rm_so: c_int,
        rm_eo: c_int,
    }

    /// Opaque test handle mirroring the managed shim's owned state.
    struct TestRegexHandle {
        regex: Pcre2Regex,
        slots: usize,
    }

    unsafe extern "C" {
        /// Compiles through the host PCRE2 POSIX wrapper.
        fn pcre2_regcomp(regex: *mut Pcre2Regex, pattern: *const c_char, flags: c_int) -> c_int;
        /// Executes through the host PCRE2 POSIX wrapper.
        fn pcre2_regexec(
            regex: *const Pcre2Regex,
            subject: *const c_char,
            nmatch: size_t,
            matches: *mut Pcre2Regmatch,
            flags: c_int,
        ) -> c_int;
        /// Releases host PCRE2 resources.
        fn pcre2_regfree(regex: *mut Pcre2Regex);
        /// Executes against the original subject instead of the POSIX wrapper's sliced range.
        fn pcre2_match_8(
            code: *const c_void, subject: *const u8, length: usize, start: usize,
            options: u32, data: *mut c_void, context: *mut c_void,
        ) -> c_int;
        /// Returns native absolute capture pairs from the retained match data.
        fn pcre2_get_ovector_pointer_8(data: *mut c_void) -> *mut usize;
    }

    /// Returns the test-only provider callback table.
    pub(super) fn provider() -> RegexProvider {
        RegexProvider {
            compile,
            exec,
            free,
        }
    }

    /// Compiles one test regex into an opaque owned handle.
    unsafe extern "C" fn compile(
        handle_out: *mut *mut c_void,
        pattern: *const c_char,
        flags: u32,
        slot_count_out: *mut u64,
    ) -> i32 {
        if handle_out.is_null()
            || pattern.is_null()
            || slot_count_out.is_null()
            || flags > c_int::MAX as u32
        {
            return REG_BADPAT;
        }
        unsafe {
            *handle_out = std::ptr::null_mut();
            *slot_count_out = 0;
        }
        let mut regex = Pcre2Regex {
            re_pcre2_code: std::ptr::null_mut(),
            re_match_data: std::ptr::null_mut(),
            re_endp: std::ptr::null(),
            re_nsub: 0,
            re_erroffset: 0,
            re_cflags: 0,
        };
        let status = unsafe { pcre2_regcomp(&mut regex, pattern, flags as c_int) };
        if status != 0 {
            return status;
        }
        let Some(slots) = regex.re_nsub.checked_add(1) else {
            unsafe { pcre2_regfree(&mut regex) };
            return REG_ESPACE;
        };
        let Ok(slots_u64) = u64::try_from(slots) else {
            unsafe { pcre2_regfree(&mut regex) };
            return REG_ESPACE;
        };
        let handle = Box::new(TestRegexHandle { regex, slots });
        unsafe {
            *slot_count_out = slots_u64;
            *handle_out = Box::into_raw(handle).cast();
        }
        0
    }

    /// Executes one test regex with the managed shim's fixed offset-pair ABI.
    unsafe extern "C" fn exec(
        opaque_handle: *mut c_void,
        subject: *const c_char,
        requested_slots: u64,
        offset_pairs: *mut i64,
        flags: u32,
    ) -> i32 {
        if opaque_handle.is_null()
            || subject.is_null()
            || (requested_slots != 0 && offset_pairs.is_null())
            || flags > c_int::MAX as u32
        {
            return REG_BADPAT;
        }
        let Ok(requested_slots) = usize::try_from(requested_slots) else {
            return REG_ESPACE;
        };
        if requested_slots > usize::MAX / 2 {
            return REG_ESPACE;
        }
        let handle = unsafe { &mut *opaque_handle.cast::<TestRegexHandle>() };
        let effective_slots = requested_slots.min(handle.slots);
        let input_range = if flags & REG_STARTEND != 0 && requested_slots > 0 {
            Some(unsafe { (*offset_pairs, *offset_pairs.add(1)) })
        } else {
            None
        };
        for index in 0..requested_slots.saturating_mul(2) {
            unsafe { *offset_pairs.add(index) = -1 };
        }
        if let Some((start, end)) = input_range {
            let (Ok(mut start), Ok(end)) = (usize::try_from(start), usize::try_from(end)) else {
                return REG_BADPAT;
            };
            if start > end { return REG_BADPAT; }
            let mut options = 0;
            if flags & ELEPHC_PCRE2_GLOBAL_NEXT != 0 {
                let vector = unsafe { pcre2_get_ovector_pointer_8(handle.regex.re_match_data) };
                let (previous_start, previous_end) = unsafe { (*vector, *vector.add(1)) };
                start = previous_end;
                if previous_start == previous_end {
                    if start >= end { return 17; }
                    options = PCRE2_NOTEMPTY_ATSTART;
                }
            }
            let result = unsafe { pcre2_match_8(handle.regex.re_pcre2_code, subject.cast(), end,
                start, options, handle.regex.re_match_data, std::ptr::null_mut()) };
            if result < 0 { return if result == -1 { 17 } else { REG_BADPAT }; }
            let vector = unsafe { pcre2_get_ovector_pointer_8(handle.regex.re_match_data) };
            let returned = if result == 0 { handle.slots } else { result as usize };
            for index in 0..returned.min(effective_slots) {
                unsafe {
                    let start = *vector.add(index * 2);
                    let end = *vector.add(index * 2 + 1);
                    *offset_pairs.add(index * 2) = if start == usize::MAX { -1 } else { start as i64 };
                    *offset_pairs.add(index * 2 + 1) = if end == usize::MAX { -1 } else { end as i64 };
                }
            }
            return 0;
        }
        let mut matches = vec![
            Pcre2Regmatch {
                rm_so: -1,
                rm_eo: -1,
            };
            effective_slots
        ];
        if let (Some((start, end)), Some(full_match)) = (input_range, matches.first_mut()) {
            let (Ok(start), Ok(end)) = (c_int::try_from(start), c_int::try_from(end)) else {
                return REG_BADPAT;
            };
            full_match.rm_so = start;
            full_match.rm_eo = end;
        }
        let status = unsafe {
            pcre2_regexec(
                &handle.regex,
                subject,
                effective_slots,
                matches.as_mut_ptr(),
                flags as c_int,
            )
        };
        if status == 0 {
            for (index, matched) in matches.into_iter().enumerate() {
                unsafe {
                    *offset_pairs.add(index * 2) = i64::from(matched.rm_so);
                    *offset_pairs.add(index * 2 + 1) = i64::from(matched.rm_eo);
                }
            }
        }
        status
    }

    /// Releases one test regex handle and its PCRE2 allocation.
    unsafe extern "C" fn free(opaque_handle: *mut c_void) {
        if opaque_handle.is_null() {
            return;
        }
        let mut handle = unsafe { Box::from_raw(opaque_handle.cast::<TestRegexHandle>()) };
        unsafe { pcre2_regfree(&mut handle.regex) };
    }
}
