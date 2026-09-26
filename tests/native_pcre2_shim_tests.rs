//! Purpose:
//! Host-platform ABI tests for Elephc's embedded opaque PCRE2 shim.
//!
//! Called from:
//! - `cargo test --test native_pcre2_shim_tests` through Rust's integration-test harness.
//!
//! Key details:
//! - Compiles and executes a C harness against one aligned, network-free system provider.
//! - Missing PCRE2 development headers or libraries are an explicit test failure, never a skip.

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

const SHIM_SOURCE: &str = include_str!("../src/native_deps/recipes/pcre2_shim.c");

const HARNESS_SOURCE: &str = r#"
#include <limits.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

int32_t elephc_pcre2_v1_compile(
    void **handle_out,
    const char *pattern_z,
    uint32_t cflags,
    uint64_t *match_slot_count_out
);
int32_t elephc_pcre2_v1_exec(
    void *opaque_handle,
    const char *subject_z,
    uint64_t requested_slots,
    int64_t *offset_pairs,
    uint32_t eflags
);
void elephc_pcre2_v1_free(void *opaque_handle);
int32_t elephc_pcre2_v1_mime_compile(void **handle, const uint8_t *pattern, uint64_t length, uint64_t *offset);
int32_t elephc_pcre2_v1_mime_match(void *handle, const uint8_t *subject, uint64_t length);
void elephc_pcre2_v1_mime_free(void *handle);
int32_t elephc_pcre2_v1_error_message(int32_t error, uint8_t *buffer, uint64_t capacity);

#define CHECK(condition, message)                                                \
    do {                                                                         \
        if (!(condition)) {                                                      \
            fprintf(stderr, "contract failure at line %d: %s\n", __LINE__, message); \
            return __LINE__;                                                     \
        }                                                                        \
    } while (0)

static int check_optional_capture_and_surplus(void) {
    void *handle = (void *)(uintptr_t)1;
    uint64_t slots = UINT64_MAX;
    int64_t pairs[200];
    size_t index;
    int32_t result;

    result = elephc_pcre2_v1_compile(&handle, "(a)?(b)", 0, &slots);
    CHECK(result == 0, "valid optional-capture pattern must compile");
    CHECK(handle != NULL, "successful compilation must publish a handle");
    CHECK(slots == 3, "full match plus two captures must publish three slots");

    for (index = 0; index < 200; ++index) {
        pairs[index] = 77;
    }
    result = elephc_pcre2_v1_exec(handle, "b", 3, pairs, 0);
    CHECK(result == 0, "optional-capture pattern must match b");
    CHECK(pairs[0] == 0 && pairs[1] == 1, "full match must be [0,1]");
    CHECK(pairs[2] == -1 && pairs[3] == -1, "unmatched capture must be [-1,-1]");
    CHECK(pairs[4] == 0 && pairs[5] == 1, "second capture must be [0,1]");

    for (index = 0; index < 200; ++index) {
        pairs[index] = 88;
    }
    result = elephc_pcre2_v1_exec(handle, "b", 100, pairs, 0);
    CHECK(result == 0, "requesting surplus slots must still match");
    CHECK(pairs[0] == 0 && pairs[1] == 1, "surplus request must preserve full match");
    CHECK(pairs[2] == -1 && pairs[3] == -1, "surplus request must preserve unmatched capture");
    CHECK(pairs[4] == 0 && pairs[5] == 1, "surplus request must preserve second capture");
    for (index = 6; index < 200; ++index) {
        CHECK(pairs[index] == -1, "every surplus offset field must be initialized to -1");
    }

    result = elephc_pcre2_v1_exec(handle, "b", 0, NULL, 0);
    CHECK(result == 0, "zero requested slots with a null buffer must be accepted");
    elephc_pcre2_v1_free(handle);
    return 0;
}

static int check_invalid_pattern_and_empty_match(void) {
    void *handle = (void *)(uintptr_t)1;
    uint64_t slots = UINT64_MAX;
    int64_t pair[2] = {91, 92};
    int32_t result;

    result = elephc_pcre2_v1_compile(&handle, "(", 0, &slots);
    CHECK(result != 0, "invalid pattern must return a non-zero status");
    CHECK(handle == NULL, "invalid pattern must clear the handle output");
    CHECK(slots == 0, "invalid pattern must clear the slot-count output");

    result = elephc_pcre2_v1_compile(&handle, "a*", 0, &slots);
    CHECK(result == 0 && handle != NULL && slots == 1, "empty-match pattern must compile");
    result = elephc_pcre2_v1_exec(handle, "bbb", 1, pair, 0);
    CHECK(result == 0, "a* must produce an empty match at the subject start");
    CHECK(pair[0] == 0 && pair[1] == 0, "empty match must return [0,0]");
    elephc_pcre2_v1_free(handle);
    return 0;
}

static int check_startend_offsets(void) {
    void *handle = NULL;
    uint64_t slots = 0;
    int64_t pairs[2] = {4, 8};
    int32_t result;

    result = elephc_pcre2_v1_compile(&handle, "[a-z]+", 0, &slots);
    CHECK(result == 0 && handle != NULL && slots == 1, "start/end fixture must compile");
    result = elephc_pcre2_v1_exec(handle, "one two", 1, pairs, 0x0080);
    CHECK(result == 0, "REG_STARTEND must search inside the supplied byte range");
    CHECK(pairs[0] == 4 && pairs[1] == 7, "REG_STARTEND must preserve absolute offsets");

    pairs[0] = -1;
    pairs[1] = 8;
    CHECK(
        elephc_pcre2_v1_exec(handle, "one two", 1, pairs, 0x0080) != 0,
        "REG_STARTEND must reject a negative start offset"
    );
    elephc_pcre2_v1_free(handle);
    return 0;
}

static int check_guard_contracts(void) {
    void *handle = (void *)(uintptr_t)1;
    uint64_t slots = UINT64_MAX;
    int64_t guard_pair[2] = {123, 456};
    int32_t result;

    result = elephc_pcre2_v1_compile(NULL, "a", 0, &slots);
    CHECK(result != 0 && slots == 0, "null handle output must fail and clear slots");

    slots = UINT64_MAX;
    result = elephc_pcre2_v1_compile(&handle, NULL, 0, &slots);
    CHECK(result != 0 && handle == NULL && slots == 0, "null pattern must clear outputs");

    handle = (void *)(uintptr_t)1;
    result = elephc_pcre2_v1_compile(&handle, "a", 0, NULL);
    CHECK(result != 0 && handle == NULL, "null slot-count output must fail and clear handle");

    handle = (void *)(uintptr_t)1;
    slots = UINT64_MAX;
    result = elephc_pcre2_v1_compile(&handle, "a", (uint32_t)INT_MAX + 1U, &slots);
    CHECK(result != 0 && handle == NULL && slots == 0, "oversized compile flags must fail safely");

    result = elephc_pcre2_v1_compile(&handle, "a", 0, &slots);
    CHECK(result == 0 && handle != NULL && slots == 1, "guard fixture pattern must compile");
    CHECK(elephc_pcre2_v1_exec(NULL, "a", 1, guard_pair, 0) != 0, "null handle must fail");
    CHECK(elephc_pcre2_v1_exec(handle, NULL, 1, guard_pair, 0) != 0, "null subject must fail");
    CHECK(elephc_pcre2_v1_exec(handle, "a", 1, NULL, 0) != 0, "null non-empty output must fail");
    CHECK(
        elephc_pcre2_v1_exec(handle, "a", 1, guard_pair, (uint32_t)INT_MAX + 1U) != 0,
        "oversized execution flags must fail safely"
    );
    CHECK(
        elephc_pcre2_v1_exec(handle, "a", UINT64_MAX, guard_pair, 0) != 0,
        "overflowing requested-slot count must fail before writing"
    );
    CHECK(guard_pair[0] == 123 && guard_pair[1] == 456, "overflow guard must not touch output");
    elephc_pcre2_v1_free(handle);
    elephc_pcre2_v1_free(NULL);
    return 0;
}

/* Exercises PCRE2 syntax, caseless matching, explicit byte lengths, and diagnostic ownership. */
static int check_mime_native_contract(void) {
    void *handle = (void *)(uintptr_t)1;
    uint64_t offset = UINT64_MAX;
    uint8_t message[256];
    uint8_t small[2] = {77, 88};
    const uint8_t subject[] = {'X', 0, 'y'};
    const char *pattern = "(?<=^application/)json";
    int32_t error;
    int32_t length;

    error = elephc_pcre2_v1_mime_compile(&handle, (const uint8_t *)"[", 1, &offset);
    CHECK(error > 0 && handle == NULL && offset == 1, "invalid MIME pattern must preserve PCRE2 error and byte offset");
    length = elephc_pcre2_v1_error_message(error, message, sizeof(message));
    CHECK(length > 0 && (size_t)length == strlen((char *)message), "MIME diagnostic length excludes the terminator");
    CHECK(strcmp((char *)message, "missing terminating ] for character class") == 0, "MIME diagnostic uses native PCRE2 wording");
    CHECK(elephc_pcre2_v1_error_message(error, small, sizeof(small)) < 0 && small[1] == 0, "short diagnostic buffer must terminate safely");
    CHECK(elephc_pcre2_v1_error_message(error, small, 0) < 0, "zero diagnostic capacity must fail");
    CHECK(elephc_pcre2_v1_error_message(error, NULL, 1) < 0, "null diagnostic buffer must fail");

    CHECK(elephc_pcre2_v1_mime_compile(&handle, (const uint8_t *)pattern, strlen(pattern), &offset) == 0,
        "native lookbehind MIME syntax must compile");
    CHECK(handle != NULL && offset == 0, "successful MIME compile clears the error offset");
    CHECK(elephc_pcre2_v1_mime_match(handle, (const uint8_t *)"APPLICATION/JSON", 16) == 1, "MIME matching is caseless");
    CHECK(elephc_pcre2_v1_mime_match(handle, (const uint8_t *)"text/plain", 10) == 0, "valid MIME nonmatch returns zero");
    CHECK(elephc_pcre2_v1_mime_match(handle, NULL, 1) < 0, "null nonempty MIME subject must fail");
    CHECK(elephc_pcre2_v1_mime_match(handle, subject, UINT64_MAX) < 0, "oversized MIME subject must fail before reading");
    elephc_pcre2_v1_mime_free(handle);

    pattern = "x\\x00y";
    CHECK(elephc_pcre2_v1_mime_compile(&handle, (const uint8_t *)pattern, strlen(pattern), &offset) == 0, "binary subject fixture compiles");
    CHECK(elephc_pcre2_v1_mime_match(handle, subject, sizeof(subject)) == 1, "MIME shim preserves explicit binary subject length");
    elephc_pcre2_v1_mime_free(handle);

    CHECK(elephc_pcre2_v1_mime_compile(&handle, NULL, 0, &offset) == 0, "null empty pattern is safe");
    CHECK(elephc_pcre2_v1_mime_match(handle, NULL, 0) == 1, "null empty subject is safe");
    elephc_pcre2_v1_mime_free(handle);
    CHECK(elephc_pcre2_v1_mime_compile(&handle, NULL, 1, &offset) < 0 && handle == NULL && offset == 0, "invalid MIME arguments clear outputs");
    CHECK(elephc_pcre2_v1_mime_compile(&handle, subject, UINT64_MAX, &offset) < 0, "oversized pattern must fail before reading");
    CHECK(elephc_pcre2_v1_mime_compile(NULL, subject, sizeof(subject), &offset) < 0, "null handle output must fail");
    CHECK(elephc_pcre2_v1_mime_compile(&handle, subject, sizeof(subject), NULL) < 0 && handle == NULL, "null offset output clears handle");
    CHECK(elephc_pcre2_v1_mime_match(NULL, subject, sizeof(subject)) < 0, "null MIME handle must fail");
    elephc_pcre2_v1_mime_free(NULL);
    return 0;
}

int main(void) {
    int result;

    result = check_optional_capture_and_surplus();
    if (result != 0) {
        return result;
    }
    result = check_invalid_pattern_and_empty_match();
    if (result != 0) {
        return result;
    }
    result = check_startend_offsets();
    if (result != 0) {
        return result;
    }
    result = check_guard_contracts();
    if (result != 0) {
        return result;
    }
    result = check_mime_native_contract();
    if (result != 0) {
        return result;
    }
    puts("opaque PCRE2 shim contract: ok");
    return 0;
}
"#;

#[derive(Debug)]
struct Pcre2Provider {
    include_dir: PathBuf,
    posix_library: PathBuf,
    core_library: PathBuf,
}

struct TempDir(PathBuf);

impl Drop for TempDir {
    /// Removes only the unique process-and-time-scoped directory created by this test.
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Compiles the embedded shim and proves its complete opaque offset-pair contract.
#[test]
fn embedded_pcre2_shim_obeys_versioned_abi_contract() {
    let provider = discover_provider();
    let directory = create_temp_dir();
    let shim = directory.0.join("elephc_pcre2_shim.c");
    let harness = directory.0.join("shim_contract_harness.c");
    let executable = directory.0.join("shim_contract_harness");
    fs::write(&shim, SHIM_SOURCE).expect("failed to write embedded PCRE2 shim fixture");
    fs::write(&harness, HARNESS_SOURCE).expect("failed to write PCRE2 shim harness fixture");

    let compiler = std::env::var_os("CC").unwrap_or_else(|| OsString::from("cc"));
    let mut command = Command::new(&compiler);
    command
        .args(["-std=c11", "-Wall", "-Wextra", "-DPCRE2_STATIC"])
        .arg(format!("-I{}", provider.include_dir.display()))
        .arg(&shim)
        .arg(&harness)
        .arg(&provider.posix_library)
        .arg(&provider.core_library)
        .arg("-o")
        .arg(&executable);
    let compile = run(&mut command, "compile opaque PCRE2 shim contract harness");
    assert!(
        compile.status.success(),
        "failed to compile opaque PCRE2 shim contract harness with provider {provider:?}:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&compile.stdout),
        String::from_utf8_lossy(&compile.stderr)
    );

    let execute = run(
        &mut Command::new(&executable),
        "execute opaque PCRE2 shim contract harness",
    );
    assert!(
        execute.status.success(),
        "opaque PCRE2 shim contract harness failed with provider {provider:?}:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&execute.stdout),
        String::from_utf8_lossy(&execute.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&execute.stdout).trim(),
        "opaque PCRE2 shim contract: ok"
    );
}

/// Discovers one complete host provider and fails explicitly when CI lacks it.
fn discover_provider() -> Pcre2Provider {
    let candidates = provider_candidates();
    for (include_dir, library_dir) in &candidates {
        if let Some(provider) = provider_at(include_dir, library_dir) {
            return provider;
        }
    }
    let inspected = candidates
        .iter()
        .map(|(include, library)| format!("{} + {}", include.display(), library.display()))
        .collect::<Vec<_>>()
        .join("\n  - ");
    panic!(
        "required host PCRE2 development provider is missing; expected aligned pcre2.h, \
         pcre2posix.h, libpcre2-posix and libpcre2-8. Inspected:\n  - {inspected}"
    );
}

/// Returns aligned Homebrew candidates on macOS and system-prefix candidates on Linux.
fn provider_candidates() -> Vec<(PathBuf, PathBuf)> {
    if cfg!(target_os = "macos") {
        let mut prefixes = Vec::new();
        if let Some(prefix) = std::env::var_os("HOMEBREW_PREFIX") {
            prefixes.push(PathBuf::from(prefix).join("opt/pcre2"));
        }
        prefixes.push(PathBuf::from("/opt/homebrew/opt/pcre2"));
        prefixes.push(PathBuf::from("/usr/local/opt/pcre2"));
        prefixes.sort();
        prefixes.dedup();
        return prefixes
            .into_iter()
            .map(|prefix| (prefix.join("include"), prefix.join("lib")))
            .collect();
    }

    if cfg!(target_os = "linux") {
        let multiarch = match std::env::consts::ARCH {
            "x86_64" => Some("x86_64-linux-gnu"),
            "aarch64" => Some("aarch64-linux-gnu"),
            _ => None,
        };
        let mut candidates = vec![
            (PathBuf::from("/usr/local/include"), PathBuf::from("/usr/local/lib")),
            (PathBuf::from("/usr/local/include"), PathBuf::from("/usr/local/lib64")),
        ];
        if let Some(tuple) = multiarch {
            candidates.push((PathBuf::from("/usr/include"), PathBuf::from("/usr/lib").join(tuple)));
            candidates.push((PathBuf::from("/usr/include"), PathBuf::from("/lib").join(tuple)));
        }
        candidates.push((PathBuf::from("/usr/include"), PathBuf::from("/usr/lib64")));
        candidates.push((PathBuf::from("/usr/include"), PathBuf::from("/usr/lib")));
        return candidates;
    }

    panic!("opaque PCRE2 shim ABI test supports only the Elephc macOS and Linux hosts");
}

/// Builds a provider only when both headers and both libraries share an expected prefix pair.
fn provider_at(include_dir: &Path, library_dir: &Path) -> Option<Pcre2Provider> {
    if !include_dir.join("pcre2.h").is_file() || !include_dir.join("pcre2posix.h").is_file() {
        return None;
    }
    let posix_library = find_library(library_dir, "pcre2-posix")?;
    let core_library = find_library(library_dir, "pcre2-8")?;
    Some(Pcre2Provider {
        include_dir: include_dir.to_path_buf(),
        posix_library,
        core_library,
    })
}

/// Prefers static archives and otherwise accepts the host platform's unversioned dev library.
fn find_library(directory: &Path, stem: &str) -> Option<PathBuf> {
    let extensions: &[&str] = if cfg!(target_os = "macos") {
        &["a", "dylib"]
    } else {
        &["a", "so"]
    };
    extensions
        .iter()
        .map(|extension| directory.join(format!("lib{stem}.{extension}")))
        .find(|path| path.is_file())
}

/// Creates an isolated host temporary directory for C compilation artifacts.
fn create_temp_dir() -> TempDir {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock precedes Unix epoch")
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "elephc_native_pcre2_shim_{}_{}",
        std::process::id(),
        nonce
    ));
    fs::create_dir(&path).expect("failed to create isolated PCRE2 shim test directory");
    TempDir(path)
}

/// Executes a child process and reports spawn failures with the operation name.
fn run(command: &mut Command, operation: &str) -> Output {
    command
        .output()
        .unwrap_or_else(|error| panic!("failed to {operation}: {error}"))
}
