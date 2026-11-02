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
/// Returns how many capture groups a compiled regex named.
pub(crate) type RegexNameCountFn = unsafe extern "C" fn(*mut c_void) -> u64;
/// Resolves a compiled regex's declared name for one capture group, if any.
pub(crate) type RegexGroupNameFn =
    unsafe extern "C" fn(*mut c_void, u64, *mut *const c_char, *mut u64) -> i32;
/// Returns the name set by the last MARK verb the most recent match passed, if any.
pub(crate) type RegexLastMarkFn =
    unsafe extern "C" fn(*mut c_void, *mut *const c_char, *mut u64) -> i32;

/// Returns php's `preg_last_error()` code for the most recent compile or match.
pub(crate) type RegexLastErrorFn = unsafe extern "C" fn() -> i32;

/// Registered callback table for the managed regex implementation.
#[derive(Clone, Copy)]
pub(crate) struct RegexProvider {
    pub(crate) compile: RegexCompileFn,
    pub(crate) exec: RegexExecFn,
    pub(crate) free: RegexFreeFn,
    pub(crate) name_count: RegexNameCountFn,
    pub(crate) group_name: RegexGroupNameFn,
    pub(crate) last_mark: RegexLastMarkFn,
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
    name_count: RegexNameCountFn,
    group_name: RegexGroupNameFn,
    last_mark: RegexLastMarkFn,
) -> i32 {
    let _ = REGEX_PROVIDER.set(RegexProvider {
        compile,
        exec,
        free,
        name_count,
        group_name,
        last_mark,
    });
    i32::from(regex_provider().is_some())
}

/// Process-wide `preg_last_error()` reader, registered next to the provider table.
///
/// A separate registration keeps `__elephc_eval_register_regex_provider` at six arguments, all in
/// registers on both SysV targets.
static REGEX_LAST_ERROR: OnceLock<RegexLastErrorFn> = OnceLock::new();

/// Registers the managed PCRE2 shim's `preg_last_error()` reader.
#[no_mangle]
pub extern "C" fn __elephc_eval_register_regex_last_error(last_error: RegexLastErrorFn) -> i32 {
    let _ = REGEX_LAST_ERROR.set(last_error);
    1
}

/// Returns php's `preg_last_error()` code for the most recent regex compile or match.
///
/// The code is shared with compiled `preg_*` calls, as php's is: the shim holds one value.
pub(crate) fn regex_last_error() -> i32 {
    #[cfg(test)]
    {
        return test_provider::last_error();
    }
    #[cfg(not(test))]
    {
        REGEX_LAST_ERROR.get().map_or(0, |last_error| unsafe { last_error() })
    }
}

/// Records a delimiter or modifier error for the next compile to report.
pub(crate) type RegexRecordPatternErrorFn = unsafe extern "C" fn(i32, u32);
/// Builds php's whole warning line for the last failed compile, under a PHP function name.
pub(crate) type RegexCompileWarningFn =
    unsafe extern "C" fn(*const c_char, u64, *mut *const c_char, *mut u64) -> i32;

/// The shim's pattern-failure reporting, registered next to the provider table.
#[derive(Clone, Copy)]
struct RegexDiagnostics {
    record_pattern_error: RegexRecordPatternErrorFn,
    compile_warning: RegexCompileWarningFn,
}

static REGEX_DIAGNOSTICS: OnceLock<RegexDiagnostics> = OnceLock::new();

/// Registers the managed PCRE2 shim's pattern-failure reporting.
///
/// php rejects a pattern with a warning and a false or null result; eval reports it the way
/// compiled code does, from the same shim state, so the two agree word for word.
#[no_mangle]
pub extern "C" fn __elephc_eval_register_regex_diagnostics(
    record_pattern_error: RegexRecordPatternErrorFn,
    compile_warning: RegexCompileWarningFn,
) -> i32 {
    let _ = REGEX_DIAGNOSTICS.set(RegexDiagnostics {
        record_pattern_error,
        compile_warning,
    });
    1
}

/// Records why php rejects a pattern before compiling it (see `PregPatternRejection`), for the
/// compile that follows to fail with.
pub(crate) fn record_pattern_error(kind: i32, byte: u8) {
    #[cfg(test)]
    {
        test_provider::record_pattern_error(kind, byte);
    }
    #[cfg(not(test))]
    {
        if let Some(diagnostics) = REGEX_DIAGNOSTICS.get() {
            unsafe { (diagnostics.record_pattern_error)(kind, u32::from(byte)) };
        }
    }
}

/// Returns php's warning line for the pattern the last compile rejected, or `None` when the
/// last compile did not fail on its pattern.
pub(crate) fn regex_compile_warning(function: &str) -> Option<String> {
    #[cfg(test)]
    {
        test_provider::compile_warning(function)
    }
    #[cfg(not(test))]
    {
        let diagnostics = REGEX_DIAGNOSTICS.get()?;
        let mut message: *const c_char = std::ptr::null();
        let mut length = 0u64;
        let failed = unsafe {
            (diagnostics.compile_warning)(
                function.as_ptr().cast(),
                function.len() as u64,
                &mut message,
                &mut length,
            )
        };
        if failed == 0 || message.is_null() {
            return None;
        }
        let bytes = unsafe { std::slice::from_raw_parts(message.cast::<u8>(), length as usize) };
        Some(String::from_utf8_lossy(bytes).into_owned())
    }
}

/// php's `preg_last_error_msg()` text for a `preg_last_error()` code.
pub(crate) fn regex_last_error_msg(code: i32) -> &'static str {
    match code {
        0 => "No error",
        2 => "Backtrack limit exhausted",
        3 => "Recursion limit exhausted",
        4 => "Malformed UTF-8 characters, possibly incorrectly encoded",
        5 => "The offset did not correspond to the beginning of a valid UTF-8 code point",
        6 => "JIT stack limit exhausted",
        _ => "Internal error",
    }
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

    /// Mirrors the production shim's `elephc_pcre2_v1_error`.
    static LAST_ERROR: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);

    /// Returns the code recorded by the last test compile or match.
    pub(super) fn last_error() -> i32 {
        LAST_ERROR.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Mirrors the shim's pending pattern error and last-failure record.
    static PATTERN_ERROR: std::sync::Mutex<(i32, u8)> = std::sync::Mutex::new((0, 0));
    static FAILURE: std::sync::Mutex<Option<(i32, u8, c_int, size_t)>> = std::sync::Mutex::new(None);

    /// Mirrors `elephc_pcre2_v1_record_pattern_error`.
    pub(super) fn record_pattern_error(kind: i32, byte: u8) {
        *PATTERN_ERROR.lock().unwrap() = (kind, byte);
    }

    /// Mirrors `elephc_pcre2_v1_compile_warning`.
    pub(super) fn compile_warning(function: &str) -> Option<String> {
        let (kind, byte, errorcode, erroffset) = FAILURE.lock().unwrap().take()?;
        let byte = char::from(byte);
        let reason = match kind {
            1 => "Empty regular expression".to_string(),
            2 => "Delimiter must not be alphanumeric, backslash, or NUL byte".to_string(),
            3 => format!("No ending delimiter '{byte}' found"),
            4 => format!("Unknown modifier '{byte}'"),
            _ => {
                let mut buffer = [0u8; 256];
                let written = unsafe {
                    pcre2_get_error_message(errorcode, buffer.as_mut_ptr(), buffer.len())
                };
                let length = usize::try_from(written).unwrap_or(0);
                format!(
                    "Compilation failed: {} at offset {erroffset}",
                    String::from_utf8_lossy(&buffer[..length])
                )
            }
        };
        Some(format!("Warning: {function}(): {reason}\n"))
    }

    /// Maps a `pcre2_match` status to php's `preg_last_error()` code, as the shim does.
    fn error_from_match(status: c_int) -> i32 {
        const PCRE2_ERROR_UTF8_ERR21: c_int = -23;
        const PCRE2_ERROR_UTF8_ERR1: c_int = -3;
        const PCRE2_ERROR_BADUTFOFFSET: c_int = -36;
        const PCRE2_ERROR_MATCHLIMIT: c_int = -47;
        const PCRE2_ERROR_DEPTHLIMIT: c_int = -53;
        const PCRE2_ERROR_HEAPLIMIT: c_int = -63;
        match status {
            status if status >= 0 || status == PCRE2_ERROR_NOMATCH => 0,
            PCRE2_ERROR_UTF8_ERR21..=PCRE2_ERROR_UTF8_ERR1 => 4,
            PCRE2_ERROR_BADUTFOFFSET => 5,
            PCRE2_ERROR_MATCHLIMIT => 2,
            PCRE2_ERROR_DEPTHLIMIT | PCRE2_ERROR_HEAPLIMIT => 3,
            _ => 1,
        }
    }

    const REG_BADPAT: i32 = 3;
    const REG_ESPACE: i32 = 12;
    const REG_NOMATCH: i32 = 17;
    const REG_STARTEND: u32 = 0x0080;
    const ELEPHC_PCRE2_GLOBAL_NEXT: u32 = 1 << 16;
    const PCRE2_NOTEMPTY_ATSTART: u32 = 0x00000008;

    // POSIX-style cflag bits `crate::interpreter::builtins::regex::engine`
    // sets on `EvalPregModifiers::flags()`. The first six mirror
    // `pcre2posix.h`'s REG_* bits (kept numerically identical so this test
    // provider and the production `elephc_pcre2_v1_compile` shim agree); the
    // last four are Elephc-owned bits above POSIX's own range because
    // pcre2posix.h has no REG_* bit for PCRE2_EXTENDED, PCRE2_DOLLAR_ENDONLY,
    // PCRE2_DUPNAMES, or PCRE2_NO_AUTO_CAPTURE.
    const REG_ICASE: u32 = 0x0001;
    const REG_NEWLINE: u32 = 0x0002;
    const REG_DOTALL: u32 = 0x0010;
    const REG_UTF: u32 = 0x0040;
    const REG_UNGREEDY: u32 = 0x0200;
    const REG_UCP: u32 = 0x0400;
    const ELEPHC_PCRE2_CFLAG_ANCHORED: u32 = 0x2000;
    const ELEPHC_PCRE2_CFLAG_EXTENDED: u32 = 0x4000;
    const ELEPHC_PCRE2_CFLAG_DOLLAR_ENDONLY: u32 = 0x8000;
    const ELEPHC_PCRE2_CFLAG_DUPNAMES: u32 = 0x10000;
    const ELEPHC_PCRE2_CFLAG_NO_AUTO_CAPTURE: u32 = 0x20000;

    // Native PCRE2 compile options (from pcre2.h), used directly since the
    // POSIX cflag set above cannot express all of them.
    const PCRE2_CASELESS: u32 = 0x0000_0008;
    const PCRE2_DOLLAR_ENDONLY: u32 = 0x0000_0010;
    const PCRE2_DOTALL: u32 = 0x0000_0020;
    const PCRE2_DUPNAMES: u32 = 0x0000_0040;
    const PCRE2_EXTENDED: u32 = 0x0000_0080;
    const PCRE2_MULTILINE: u32 = 0x0000_0400;
    const PCRE2_NO_AUTO_CAPTURE: u32 = 0x0000_2000;
    const PCRE2_UCP: u32 = 0x0002_0000;
    const PCRE2_UNGREEDY: u32 = 0x0004_0000;
    const PCRE2_UTF: u32 = 0x0008_0000;
    const PCRE2_INFO_CAPTURECOUNT: u32 = 4;
    const PCRE2_INFO_NAMECOUNT: u32 = 17;
    const PCRE2_INFO_NAMEENTRYSIZE: u32 = 18;
    const PCRE2_INFO_NAMETABLE: u32 = 19;
    const PCRE2_ZERO_TERMINATED: size_t = usize::MAX;
    const PCRE2_UNSET: size_t = usize::MAX;
    const PCRE2_ERROR_NOMATCH: c_int = -1;
    const PCRE2_NOTBOL: u32 = 0x0000_0001;
    const PCRE2_NOTEOL: u32 = 0x0000_0002;
    const PCRE2_NOTEMPTY: u32 = 0x0000_0004;
    const REG_NOTBOL: u32 = 0x0004;
    const REG_NOTEOL: u32 = 0x0008;
    const REG_NOTEMPTY: u32 = 0x0100;

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
        anchored: bool,
    }

    unsafe extern "C" {
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
        /// Renders a PCRE2 compile error code as PCRE2's own message.
        #[link_name = "pcre2_get_error_message_8"]
        fn pcre2_get_error_message(errorcode: c_int, buffer: *mut u8, size: size_t) -> c_int;
        // PCRE2's core (non-POSIX) API is compiled for multiple code-unit
        // widths in the same library, so `pcre2.h`'s C preprocessor renames
        // every one of these to an `_8` suffix when `PCRE2_CODE_UNIT_WIDTH 8`
        // is defined before the include (exactly what the production shim in
        // `src/native_deps/recipes/pcre2_shim.c` does). Rust never sees that
        // macro, so it must name the real exported symbol explicitly — unlike
        // `pcre2_regcomp`/`pcre2_regexec`/`pcre2_regfree` above, which come
        // from the separate always-8-bit POSIX wrapper library and are never
        // suffixed.
        #[link_name = "pcre2_compile_8"]
        fn pcre2_compile(
            pattern: *const u8,
            length: size_t,
            options: u32,
            errorcode: *mut c_int,
            erroroffset: *mut size_t,
            ccontext: *const c_void,
        ) -> *mut c_void;
        /// Allocates match-data sized for a compiled pattern's own capture count.
        #[link_name = "pcre2_match_data_create_from_pattern_8"]
        fn pcre2_match_data_create_from_pattern(
            code: *const c_void,
            gcontext: *const c_void,
        ) -> *mut c_void;
        /// Releases match-data allocated by `pcre2_match_data_create_from_pattern`.
        #[link_name = "pcre2_match_data_free_8"]
        fn pcre2_match_data_free(match_data: *mut c_void);
        /// Releases a compiled pattern allocated by `pcre2_compile`.
        #[link_name = "pcre2_code_free_8"]
        fn pcre2_code_free(code: *mut c_void);
        /// Reads compile-time metadata (capture count, name table) off a compiled pattern.
        #[link_name = "pcre2_pattern_info_8"]
        fn pcre2_pattern_info(code: *const c_void, what: u32, where_: *mut c_void) -> c_int;
        /// Matches a compiled pattern against the WHOLE subject from a start offset.
        #[link_name = "pcre2_match_8"]
        fn pcre2_match(
            code: *const c_void,
            subject: *const c_char,
            length: size_t,
            start_offset: size_t,
            options: u32,
            match_data: *mut c_void,
            mcontext: *const c_void,
        ) -> c_int;
        /// Returns the ovector of a match-data block filled by `pcre2_match`.
        #[link_name = "pcre2_get_ovector_pointer_8"]
        fn pcre2_get_ovector_pointer(match_data: *mut c_void) -> *mut size_t;

        /// Returns the last MARK name the most recent match passed, or null.
        #[link_name = "pcre2_get_mark_8"]
        fn pcre2_get_mark(match_data: *mut c_void) -> *const c_char;
    }

    /// Returns the test-only provider callback table.
    pub(super) fn provider() -> RegexProvider {
        RegexProvider {
            compile,
            exec,
            free,
            name_count,
            group_name,
            last_mark,
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
        LAST_ERROR.store(0, std::sync::atomic::Ordering::Relaxed);
        *FAILURE.lock().unwrap() = None;
        let pending = std::mem::take(&mut *PATTERN_ERROR.lock().unwrap());
        if pending.0 != 0 {
            LAST_ERROR.store(1, std::sync::atomic::Ordering::Relaxed);
            *FAILURE.lock().unwrap() = Some((pending.0, pending.1, 0, 0));
            return REG_BADPAT;
        }
        let anchored = flags & ELEPHC_PCRE2_CFLAG_ANCHORED != 0;

        // Translate the POSIX-style cflags into native PCRE2 compile options
        // ourselves (mirrors `elephc_pcre2_v1_compile` in the production
        // shim): pcre2_regcomp()'s fixed cflag set cannot express PHP's
        // x/D/J/n modifiers at all, so this test provider must also bypass
        // it to keep unit-test behavior identical to the real shim.
        let mut native_options = 0u32;
        if flags & REG_ICASE != 0 {
            native_options |= PCRE2_CASELESS;
        }
        if flags & REG_NEWLINE != 0 {
            native_options |= PCRE2_MULTILINE;
        }
        if flags & REG_DOTALL != 0 {
            native_options |= PCRE2_DOTALL;
        }
        if flags & REG_UTF != 0 {
            native_options |= PCRE2_UTF;
        }
        if flags & REG_UNGREEDY != 0 {
            native_options |= PCRE2_UNGREEDY;
        }
        if flags & REG_UCP != 0 {
            native_options |= PCRE2_UCP;
        }
        if flags & ELEPHC_PCRE2_CFLAG_EXTENDED != 0 {
            native_options |= PCRE2_EXTENDED;
        }
        if flags & ELEPHC_PCRE2_CFLAG_DOLLAR_ENDONLY != 0 {
            native_options |= PCRE2_DOLLAR_ENDONLY;
        }
        if flags & ELEPHC_PCRE2_CFLAG_DUPNAMES != 0 {
            native_options |= PCRE2_DUPNAMES;
        }
        if flags & ELEPHC_PCRE2_CFLAG_NO_AUTO_CAPTURE != 0 {
            native_options |= PCRE2_NO_AUTO_CAPTURE;
        }

        let mut errorcode: c_int = 0;
        let mut erroffset: size_t = 0;
        let code = unsafe {
            pcre2_compile(
                pattern.cast::<u8>(),
                PCRE2_ZERO_TERMINATED,
                native_options,
                &mut errorcode,
                &mut erroffset,
                std::ptr::null(),
            )
        };
        if code.is_null() {
            LAST_ERROR.store(1, std::sync::atomic::Ordering::Relaxed);
            *FAILURE.lock().unwrap() = Some((0, 0, errorcode, erroffset));
            return REG_BADPAT;
        }
        let match_data = unsafe { pcre2_match_data_create_from_pattern(code, std::ptr::null()) };
        if match_data.is_null() {
            unsafe { pcre2_code_free(code) };
            return REG_ESPACE;
        }
        let mut capture_count: u32 = 0;
        let info_status = unsafe {
            pcre2_pattern_info(
                code,
                PCRE2_INFO_CAPTURECOUNT,
                (&mut capture_count as *mut u32).cast(),
            )
        };
        if info_status != 0 {
            unsafe {
                pcre2_match_data_free(match_data);
                pcre2_code_free(code);
            }
            return REG_ESPACE;
        }

        let mut regex = Pcre2Regex {
            re_pcre2_code: code,
            re_match_data: match_data,
            re_endp: std::ptr::null(),
            re_nsub: capture_count as size_t,
            re_erroffset: 0,
            re_cflags: 0,
        };
        let Some(slots) = regex.re_nsub.checked_add(1) else {
            unsafe { pcre2_regfree(&mut regex) };
            return REG_ESPACE;
        };
        let Ok(slots_u64) = u64::try_from(slots) else {
            unsafe { pcre2_regfree(&mut regex) };
            return REG_ESPACE;
        };
        let handle = Box::new(TestRegexHandle {
            regex,
            slots,
            anchored,
        });
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
        let input_range = if flags & REG_STARTEND != 0 && requested_slots > 0 {
            Some(unsafe { (*offset_pairs, *offset_pairs.add(1)) })
        } else {
            None
        };
        for index in 0..requested_slots.saturating_mul(2) {
            unsafe { *offset_pairs.add(index) = -1 };
        }
        // Match through `pcre2_match` on the WHOLE subject, mirroring `elephc_pcre2_v1_exec`:
        // `pcre2_regexec`'s REG_STARTEND advances the subject POINTER, which makes `^` under the
        // `m` modifier match at every continuation of a global match rather than at line starts.
        let (subject_length, match_offset) = match input_range {
            Some((start, end)) => {
                if start < 0 || end < start {
                    return REG_BADPAT;
                }
                let (Ok(start), Ok(end)) = (usize::try_from(start), usize::try_from(end)) else {
                    return REG_BADPAT;
                };
                (end, start)
            }
            None => (unsafe { std::ffi::CStr::from_ptr(subject) }.to_bytes().len(), 0),
        };
        let mut match_options = 0u32;
        if flags & REG_NOTBOL != 0 {
            match_options |= PCRE2_NOTBOL;
        }
        if flags & REG_NOTEOL != 0 {
            match_options |= PCRE2_NOTEOL;
        }
        if flags & REG_NOTEMPTY != 0 {
            match_options |= PCRE2_NOTEMPTY;
        }
        if handle.regex.re_pcre2_code.is_null() || handle.regex.re_match_data.is_null() {
            return REG_BADPAT;
        }
        // A global continuation resumes at the end of the previous match held in the retained
        // match data; after an empty match it must not match empty at that same offset again.
        let mut match_offset = match_offset;
        if flags & ELEPHC_PCRE2_GLOBAL_NEXT != 0 && input_range.is_some() {
            let vector = unsafe { pcre2_get_ovector_pointer(handle.regex.re_match_data) };
            if !vector.is_null() {
                let (previous_start, previous_end) = unsafe { (*vector, *vector.add(1)) };
                match_offset = previous_end;
                if previous_start == previous_end {
                    if match_offset >= subject_length {
                        return REG_NOMATCH;
                    }
                    match_options |= PCRE2_NOTEMPTY_ATSTART;
                }
            }
        }
        let status = unsafe {
            pcre2_match(
                handle.regex.re_pcre2_code,
                subject,
                subject_length,
                match_offset,
                match_options,
                handle.regex.re_match_data,
                std::ptr::null(),
            )
        };
        LAST_ERROR.store(error_from_match(status), std::sync::atomic::Ordering::Relaxed);
        if status == PCRE2_ERROR_NOMATCH {
            return REG_NOMATCH;
        }
        if status < 0 {
            return REG_BADPAT;
        }
        let ovector = unsafe { pcre2_get_ovector_pointer(handle.regex.re_match_data) };
        if ovector.is_null() {
            return REG_BADPAT;
        }
        // `0` means the ovector could not hold every capture; the pairs it did fill are valid and
        // the pattern's own slot count bounds them.
        let pair_count = if status == 0 { handle.slots } else { status as usize };
        if handle.anchored && unsafe { *ovector } != match_offset {
            return REG_NOMATCH;
        }
        for index in 0..requested_slots.min(pair_count) {
            let start = unsafe { *ovector.add(index * 2) };
            if start == PCRE2_UNSET {
                continue;
            }
            let end = unsafe { *ovector.add(index * 2 + 1) };
            unsafe {
                *offset_pairs.add(index * 2) = start as i64;
                *offset_pairs.add(index * 2 + 1) = end as i64;
            }
        }
        0
    }

    /// Releases one test regex handle and its PCRE2 allocation.
    unsafe extern "C" fn free(opaque_handle: *mut c_void) {
        if opaque_handle.is_null() {
            return;
        }
        let mut handle = unsafe { Box::from_raw(opaque_handle.cast::<TestRegexHandle>()) };
        unsafe { pcre2_regfree(&mut handle.regex) };
    }

    /// Returns how many capture groups this test regex named, mirroring
    /// `elephc_pcre2_v1_name_count` in the production shim.
    unsafe extern "C" fn name_count(opaque_handle: *mut c_void) -> u64 {
        if opaque_handle.is_null() {
            return 0;
        }
        let handle = unsafe { &*opaque_handle.cast::<TestRegexHandle>() };
        let code = handle.regex.re_pcre2_code;
        if code.is_null() {
            return 0;
        }
        let mut count: u32 = 0;
        let status = unsafe { pcre2_pattern_info(code, PCRE2_INFO_NAMECOUNT, (&mut count as *mut u32).cast()) };
        if status != 0 {
            return 0;
        }
        u64::from(count)
    }

    /// Returns the last MARK name this test regex's most recent match passed, mirroring
    /// `elephc_pcre2_v1_last_mark` in the production shim.
    unsafe extern "C" fn last_mark(
        opaque_handle: *mut c_void,
        mark_out: *mut *const c_char,
        mark_len_out: *mut u64,
    ) -> i32 {
        if !mark_out.is_null() {
            unsafe { *mark_out = std::ptr::null() };
        }
        if !mark_len_out.is_null() {
            unsafe { *mark_len_out = 0 };
        }
        if opaque_handle.is_null() || mark_out.is_null() || mark_len_out.is_null() {
            return 1;
        }
        let handle = unsafe { &*opaque_handle.cast::<TestRegexHandle>() };
        let match_data = handle.regex.re_match_data;
        if match_data.is_null() {
            return 1;
        }
        let mark = unsafe { pcre2_get_mark(match_data) };
        if mark.is_null() {
            return 1;
        }
        let mut length = 0u64;
        while unsafe { *mark.add(length as usize) } != 0 {
            length += 1;
        }
        unsafe { *mark_out = mark };
        unsafe { *mark_len_out = length };
        0
    }

    /// Resolves the declared name for one capture group index, mirroring
    /// `elephc_pcre2_v1_group_name` in the production shim byte for byte.
    unsafe extern "C" fn group_name(
        opaque_handle: *mut c_void,
        group: u64,
        name_out: *mut *const c_char,
        name_len_out: *mut u64,
    ) -> i32 {
        if !name_out.is_null() {
            unsafe { *name_out = std::ptr::null() };
        }
        if !name_len_out.is_null() {
            unsafe { *name_len_out = 0 };
        }
        if opaque_handle.is_null() || name_out.is_null() || name_len_out.is_null() || group > 0xFFFF {
            return 1;
        }
        let handle = unsafe { &*opaque_handle.cast::<TestRegexHandle>() };
        let code = handle.regex.re_pcre2_code;
        if code.is_null() {
            return 1;
        }
        let mut count: u32 = 0;
        let mut entry_size: u32 = 0;
        let mut table: *const u8 = std::ptr::null();
        unsafe {
            if pcre2_pattern_info(code, PCRE2_INFO_NAMECOUNT, (&mut count as *mut u32).cast()) != 0 {
                return 1;
            }
            if pcre2_pattern_info(code, PCRE2_INFO_NAMEENTRYSIZE, (&mut entry_size as *mut u32).cast()) != 0 {
                return 1;
            }
            if pcre2_pattern_info(code, PCRE2_INFO_NAMETABLE, (&mut table as *mut *const u8).cast()) != 0 {
                return 1;
            }
        }
        if count == 0 || entry_size < 3 || table.is_null() {
            return 1;
        }
        for index in 0..count {
            let entry = unsafe { table.add(index as usize * entry_size as usize) };
            let entry_group = (u32::from(unsafe { *entry }) << 8) | u32::from(unsafe { *entry.add(1) });
            if entry_group != group as u32 {
                continue;
            }
            let name_ptr = unsafe { entry.add(2) };
            let limit = entry_size as usize - 2;
            let mut length = 0usize;
            while length < limit && unsafe { *name_ptr.add(length) } != 0 {
                length += 1;
            }
            unsafe {
                *name_out = name_ptr.cast::<c_char>();
                *name_len_out = length as u64;
            }
            return 0;
        }
        1
    }
}
