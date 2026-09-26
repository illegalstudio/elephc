//! Purpose:
//! Reproduces php-src's startup validation of `opcache.file_cache` and, on an 8.5
//! target, `opcache.file_cache_read_only`. elephc has no on-disk opcode cache — the
//! directives stay inert — but reference PHP REFUSES TO START on a bad setting, and
//! that refusal is observable behaviour a binary should not silently skip.
//!
//! Called from:
//! - `crate::ffi::context::__elephc_eval_configure_opcache_file_cache()` (generated
//!   code), once, while the eval context is being initialized.
//!
//! Key details:
//! - THE VALIDATION IS GATED ON THE CACHE BEING ENABLED, and that is not a shortcut:
//!   VERIFIED on reference PHP 8.5.10 that `-d opcache.enable=0` and the CLI default
//!   `-d opcache.enable_cli=0` both run the script with NO validation at all, however
//!   broken the path. The gate is therefore the same `ScriptCacheConfig::enabled`
//!   predicate the rest of this module already reads.
//! - An EMPTY `opcache.file_cache` means "unset" and validates nothing, matching the
//!   C `NULL` default php-src registers it with (VERIFIED: `-d opcache.file_cache=`
//!   runs).
//! - The access mode DEPENDS ON `read_only`: php-src asks for `R_OK | W_OK` normally
//!   and `R_OK` alone when `opcache.file_cache_read_only=1`. VERIFIED both ways
//!   against `/`, which is readable but not writable: read-only runs, read-write
//!   fatals.
//! - `opcache.file_cache_read_only` WITHOUT a path is its own distinct fatal, with a
//!   different message. It exists only on an 8.5 target, so older profiles never
//!   report `true` for it and never reach that branch.

use super::accel_log::{accel_error, AccelLogLevel};
use std::cell::RefCell;
use std::ffi::CString;
use std::sync::atomic::{AtomicBool, Ordering};
use std::path::Path;

/// The `opcache.*` pair that describes the on-disk file cache php-src validates.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct FileCacheConfig {
    /// `opcache.file_cache`. EMPTY means unset — php-src's C `NULL` default.
    pub(crate) path: String,
    /// `opcache.file_cache_read_only`, an 8.5-only directive. Always `false` below it.
    pub(crate) read_only: bool,
}

impl FileCacheConfig {
    /// The unconfigured pair: no directory, so the whole file cache is inert.
    pub(crate) const fn new() -> Self {
        Self {
            path: String::new(),
            read_only: false,
        }
    }
}

thread_local! {
    /// The file-cache directives this binary was compiled with.
    ///
    /// Installed beside the validation below rather than derived again later: the pair is
    /// what decides where entries live and whether they may be written, and both readers
    /// must see exactly the values the validation approved.
    static FILE_CACHE_CONFIG: RefCell<FileCacheConfig> =
        const { RefCell::new(FileCacheConfig::new()) };
}

/// Installs the file-cache directives for the current thread.
pub(crate) fn set_file_cache_config(config: FileCacheConfig) {
    FILE_CACHE_CONFIG.with(|cell| *cell.borrow_mut() = config);
}

/// Returns the file-cache directives active on the current thread.
pub(crate) fn file_cache_config() -> FileCacheConfig {
    FILE_CACHE_CONFIG.with(|cell| cell.borrow().clone())
}

/// php-src's message for a `opcache.file_cache` that is not a usable directory.
const BAD_DIRECTORY: &str = "opcache.file_cache must be a full path of an accessible directory";

/// php-src's message for `opcache.file_cache_read_only` with no path to read from.
const READ_ONLY_WITHOUT_PATH: &str =
    "opcache.file_cache_read_only is set without a proper setting of opcache.file_cache";

/// Applies php-src's startup validation, terminating the process exactly as it does.
///
/// Does nothing at all unless `cache_enabled`, which is what reference PHP does: the
/// directory is never looked at in a build whose cache is off.
pub(crate) fn validate_file_cache_directives(config: &FileCacheConfig, cache_enabled: bool) {
    if !cache_enabled {
        return;
    }
    if config.path.is_empty() {
        if config.read_only {
            accel_error(AccelLogLevel::Fatal, READ_ONLY_WITHOUT_PATH);
        }
        return;
    }
    if !is_usable_cache_directory(&config.path, config.read_only) {
        accel_error(AccelLogLevel::Fatal, BAD_DIRECTORY);
    }
}

/// Set once this process has run the startup validation. A `--web` master sets it before it
/// forks, so every worker inherits it; see `validate_file_cache_directives_at_startup`.
static STARTUP_VALIDATED: AtomicBool = AtomicBool::new(false);

/// [`validate_file_cache_directives`], ONCE per process — at startup, as php-src does it.
///
/// The configure bridge runs in the CLI prologue and, under `--web`, in the master before it
/// forks AND at the top of every request, which is how a recycled worker starts configured.
/// Validating there on every request re-ran a STARTUP check against a directory that may
/// legitimately change afterwards: removing the cache directory after the first request made
/// the next one exit the worker with the startup fatal, and the server stopped accepting
/// connections. MEASURED. php-src validates in `zend_accel_startup`, once, and keeps serving.
///
/// Returns whether this call performed the validation, so the guard itself can be tested
/// without reaching the fatal.
pub(crate) fn validate_file_cache_directives_at_startup(
    config: &FileCacheConfig,
    cache_enabled: bool,
) -> bool {
    if STARTUP_VALIDATED.swap(true, Ordering::SeqCst) {
        return false;
    }
    validate_file_cache_directives(config, cache_enabled);
    true
}

/// Returns whether `path` satisfies every condition php-src's `zend_accel_startup` tests.
///
/// All four are required, in php-src's own order: an ABSOLUTE path (a bare `.` is
/// refused even though it exists and is a directory — VERIFIED), an existing entry,
/// that entry being a directory, and `access()` granting the mode `read_only` selects.
fn is_usable_cache_directory(path: &str, read_only: bool) -> bool {
    if !Path::new(path).is_absolute() {
        return false;
    }
    if !Path::new(path).is_dir() {
        return false;
    }
    // A NUL byte cannot reach `access()` as a C string; php-src's own INI value is
    // NUL-terminated and so could never carry one either. Refusing is the same answer
    // its `zend_stat` would give.
    let Ok(c_path) = CString::new(path) else {
        return false;
    };
    // SEARCH PERMISSION IN BOTH MODES. Entries live INSIDE the directory, so a directory that
    // can be read and written but not traversed holds nothing usable, and php-src asks
    // `access()` for `X_OK` alongside the rest. MEASURED as a non-root user: a `0600`
    // directory is a startup fatal in reference and was accepted here; under
    // `file_cache_read_only=1`, `0400` is fatal and `0500` runs.
    let mode = if read_only {
        libc::R_OK | libc::X_OK
    } else {
        libc::R_OK | libc::W_OK | libc::X_OK
    };
    // SAFETY: `c_path` is a live NUL-terminated C string for the duration of the call
    // and `mode` is a valid `access(2)` mode mask.
    unsafe { libc::access(c_path.as_ptr(), mode) == 0 }
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Pins the directory predicate against the exact shapes reference PHP 8.5.10 was
    //! probed with. The fatals themselves exit the process, so they are covered by the
    //! end-to-end binary tests rather than here.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.
    //!
    //! Key details:
    //! - `/` is the load-bearing fixture: readable but not writable on every unix, so
    //!   it separates the two access modes without creating anything.

    use super::*;

    /// Verifies a disabled cache validates nothing, however broken the path.
    ///
    /// This is the branch that keeps a default CLI binary running: it must not consult
    /// the filesystem, let alone exit.
    #[test]
    fn a_disabled_cache_validates_nothing() {
        let config = FileCacheConfig {
            path: "/no/such/directory".to_string(),
            read_only: true,
        };

        validate_file_cache_directives(&config, false);
    }

    /// Verifies an unset path is accepted, matching php-src's `NULL` default.
    #[test]
    fn an_empty_path_is_accepted() {
        validate_file_cache_directives(&FileCacheConfig::default(), true);
    }

    /// Verifies the startup validation runs ONCE per process, whatever follows.
    ///
    /// Under `--web` the configure bridge runs again at the top of every request. The second
    /// call here is handed a directory that does not exist with the cache ENABLED: if the guard
    /// let it through, the process would exit with the startup fatal and take this test binary
    /// with it. MEASURED end to end: removing the cache directory after one request made the
    /// next one kill the worker, and the server stopped accepting connections.
    #[test]
    fn startup_validation_runs_once_per_process() {
        validate_file_cache_directives_at_startup(&FileCacheConfig::default(), true);
        let again = validate_file_cache_directives_at_startup(
            &FileCacheConfig {
                path: "/no/such/directory".to_string(),
                read_only: false,
            },
            true,
        );

        assert!(!again, "a second configure call must not re-run the startup validation");
    }

    /// Returns whether this process bypasses permission checks, which makes the mode fixtures
    /// below meaningless: root reads and writes a `0600` directory regardless.
    fn bypasses_permission_checks() -> bool {
        // SAFETY: `geteuid` has no preconditions.
        unsafe { libc::geteuid() == 0 }
    }

    /// Verifies a directory without SEARCH permission is refused, in both access modes.
    ///
    /// Entries live inside the directory, so one that can be read and written but not
    /// traversed holds nothing usable; php-src asks `access()` for `X_OK` as well. MEASURED as
    /// a non-root user: `0600` is fatal in reference, and under `file_cache_read_only=1`,
    /// `0400` is fatal while `0500` runs.
    #[test]
    fn a_directory_without_search_permission_is_refused() {
        use std::os::unix::fs::PermissionsExt;
        if bypasses_permission_checks() {
            return;
        }
        let dir = std::env::temp_dir().join(format!(
            "elephc-file-cache-modes-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&dir).expect("fixture directory");
        let path = dir.to_string_lossy().into_owned();
        let set_mode = |mode: u32| {
            std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(mode))
                .expect("mode should be settable");
        };

        set_mode(0o600);
        let writable_without_search = is_usable_cache_directory(&path, false);
        set_mode(0o400);
        let read_only_without_search = is_usable_cache_directory(&path, true);
        set_mode(0o500);
        let read_only_with_search = is_usable_cache_directory(&path, true);
        set_mode(0o700);

        assert!(!writable_without_search, "0600 cannot be traversed");
        assert!(!read_only_without_search, "0400 cannot be traversed either");
        assert!(read_only_with_search, "0500 is readable and traversable");
    }

    /// Verifies a relative path is refused even when it exists and is a directory.
    #[test]
    fn a_relative_directory_is_refused() {
        assert!(!is_usable_cache_directory(".", false));
    }

    /// Verifies a missing path is refused.
    #[test]
    fn a_missing_directory_is_refused() {
        assert!(!is_usable_cache_directory("/no/such/directory", false));
    }

    /// Verifies an existing FILE is refused, since php-src requires `S_ISDIR`.
    #[test]
    fn a_file_is_refused() {
        assert!(!is_usable_cache_directory("/etc/hosts", false));
    }

    /// Verifies the access mode follows `read_only`, the one rule that needs both modes.
    ///
    /// `/` is the fixture because it is readable and, FOR AN ORDINARY USER, not writable —
    /// so it separates the two modes without creating anything. Reference PHP refuses it
    /// read-write and accepts it read-only with `-d opcache.file_cache=/`.
    ///
    /// ROOT IS THE EXCEPTION, and it is not a special case in this code: root bypasses
    /// permission checks, so `access("/", R_OK|W_OK)` succeeds and the directory genuinely IS
    /// usable — reference PHP accepts it there too. Asserting the refusal unconditionally
    /// made this fail on CI, which runs as root on the Linux runners. The read-only half is
    /// the half that discriminates the directive, and it is asserted either way.
    #[test]
    fn the_access_mode_follows_read_only() {
        assert!(is_usable_cache_directory("/", true));

        // MEASURED, not inferred from the uid: root is the common way to gain write access
        // to `/`, but not the only one — a non-root process holding CAP_DAC_OVERRIDE has it
        // too, and a `geteuid() == 0` test would then assert the wrong side. Ask the
        // filesystem the question the code under test asks.
        let probe = std::path::Path::new("/").join(format!(
            ".elephc-write-probe-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let writable = match std::fs::File::create(&probe) {
            Ok(_) => {
                let _ = std::fs::remove_file(&probe);
                true
            }
            Err(_) => false,
        };
        assert_eq!(
            is_usable_cache_directory("/", false),
            writable,
            "read-write acceptance of `/` must follow whether this process may write to it"
        );
    }

    /// Verifies a writable absolute directory is accepted.
    #[test]
    fn a_writable_absolute_directory_is_accepted() {
        let path = std::env::temp_dir();
        let path = path.to_str().expect("the temp dir should be UTF-8");

        assert!(is_usable_cache_directory(path, false));
    }
}
