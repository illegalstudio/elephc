//! Purpose:
//! Gives every `--web` request the process environment the worker started with, the way php
//! restores the environment at request shutdown so that a `putenv()` lasts one request.
//!
//! Called from:
//! - `crate::worker::run_handler` and the pool/request broker child's handler call, around each
//!   invocation of the compiled PHP handler.
//!
//! Key details:
//! - The baseline is captured before the first request a process serves, so variables a request
//!   sets (Laravel's Dotenv `putenv()`s every `.env` entry) are gone for the next one, as in php.
//! - The restore runs right after the handler returns, while that request's heap is still intact.
//!   Every change goes through `std::env::{set_var, remove_var}`, so the environment ends up
//!   holding only libc-owned strings.

use std::collections::HashMap;
use std::ffi::OsString;
use std::sync::OnceLock;

/// The environment as it was before the first request this process served.
static BASELINE: OnceLock<HashMap<OsString, OsString>> = OnceLock::new();

/// Records the baseline environment once, before the first request runs.
pub(crate) fn capture_baseline() {
    BASELINE.get_or_init(|| std::env::vars_os().collect());
}

/// Variables the runtime itself manages through the environment, which a request restore must
/// not undo.
///
/// `TZ` carries the PHP default timezone to libc: the runtime writes `TZ=<id>` and calls `tzset`
/// on the first date operation of a request (`__rt_date_default_timezone_set`). Removing it after
/// that request left the runtime still reporting `UTC` while libc parsed every later
/// `new DateTimeImmutable('2023-02-12 17:48:59')` in the machine's own zone -- an hour early here.
fn is_runtime_managed(name: &std::ffi::OsStr) -> bool {
    name == "TZ"
}

/// Undoes every environment change the request just made: added variables are removed, changed
/// or removed ones get their baseline value back.
pub(crate) fn restore_baseline() {
    let Some(baseline) = BASELINE.get() else {
        return;
    };
    for (name, value) in std::env::vars_os() {
        if is_runtime_managed(&name) {
            continue;
        }
        match baseline.get(&name) {
            None => std::env::remove_var(&name),
            Some(original) if *original != value => std::env::set_var(&name, original),
            Some(_) => {}
        }
    }
    for (name, original) in baseline {
        if is_runtime_managed(name) {
            continue;
        }
        if std::env::var_os(name).is_none() {
            std::env::set_var(name, original);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A request's additions, changes and removals are all undone; untouched variables stay.
    #[test]
    fn restore_returns_the_environment_to_its_baseline() {
        std::env::set_var("ELEPHC_WEB_ENV_KEPT", "kept");
        std::env::set_var("ELEPHC_WEB_ENV_CHANGED", "before");
        std::env::set_var("ELEPHC_WEB_ENV_REMOVED", "present");
        std::env::remove_var("ELEPHC_WEB_ENV_ADDED");
        capture_baseline();

        std::env::set_var("ELEPHC_WEB_ENV_CHANGED", "after");
        std::env::remove_var("ELEPHC_WEB_ENV_REMOVED");
        std::env::set_var("ELEPHC_WEB_ENV_ADDED", "new");
        restore_baseline();

        assert_eq!(std::env::var("ELEPHC_WEB_ENV_KEPT").as_deref(), Ok("kept"));
        assert_eq!(std::env::var("ELEPHC_WEB_ENV_CHANGED").as_deref(), Ok("before"));
        assert_eq!(std::env::var("ELEPHC_WEB_ENV_REMOVED").as_deref(), Ok("present"));
        assert!(std::env::var_os("ELEPHC_WEB_ENV_ADDED").is_none());
    }
}
