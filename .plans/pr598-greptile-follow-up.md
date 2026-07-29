# PR #598 Greptile Follow-up Specification

Status: revision 6 awaiting unanimous external review

Reference revision: `2829ce0abac43bcb6e868781b3fa6cd6a80ecf13`

## Checklist

- [x] Audit every Greptile review thread against the merged source.
- [x] Decide which findings are valid and define the correction boundaries.
- [ ] Obtain unconditional acceptance of this exact specification from GLM 5.2,
      Kimi K2.7, and MiniMax M3.
- [ ] Implement only the unanimously accepted specification.
- [ ] Add and run the focused regression tests and hygiene checks.

## Context and decisions

PR #598 was merged into `main` before the Greptile review completed. This
follow-up therefore starts from the merge commit and must not modify the merged
PR branch.

Greptile left three findings:

1. **Accept — leaked download temporary on fallback success.** When publishing a
   verified temporary archive fails but another process has already published a
   valid cache entry, `ensure_source` returns success without attempting to
   remove its own temporary archive.
2. **Accept — redundant lock expansion in `--locked` mode.** `install` expands
   the manifest before the mode branch, then `validate_current` expands it again
   while validating a locked install.
3. **Accept with a corrected premise — duplicated recipe file invariant.** The
   PCRE2 and zlib helpers are not textually identical because their diagnostics
   are package-specific, but they implement the same validation and copy
   invariant. Share the invariant while preserving the existing package names
   and diagnostic wording.

## Required implementation

### 1. Download publication cleanup

- `ensure_source` becomes a thin public wrapper. Its entire function definition
  is exactly the following; it contains no quarantine, cache, download,
  verification, cleanup, or restoration logic:

  ```rust
  pub fn ensure_source(
      cached: &Path,
      source: &SourceArchive,
      offline: bool,
      downloader: &dyn Downloader,
  ) -> Result<PathBuf, NativeError> {
      ensure_source_with_publish(
          cached,
          source,
          offline,
          downloader,
          |temporary, cached| fs::rename(temporary, cached),
      )
  }
  ```

- All logic formerly in `ensure_source` moves exactly once into the separate
  private `ensure_source_with_publish`; no logic is duplicated between the two
  functions. The private helper has this documented signature:

  ```rust
  fn ensure_source_with_publish<F>(
      cached: &Path,
      source: &SourceArchive,
      offline: bool,
      downloader: &dyn Downloader,
      publish: F,
  ) -> Result<PathBuf, NativeError>
  where
      F: FnOnce(&Path, &Path) -> std::io::Result<()>
  ```

  `ensure_source` passes
  `|temporary, cached| fs::rename(temporary, cached)`. The injected operation is
  used only for the verified temporary-to-cache publication, not quarantine
  moves or restoration. It exists solely to make the publish-failure fallback
  deterministic in a unit test.
- Only `ensure_source_with_publish` owns the whole current implementation,
  including
  `let mut quarantine = None`, initial cache verification/quarantine, offline
  handling, parent creation, temporary allocation, result computation, cleanup,
  and quarantine restoration/removal. `ensure_source` contains only the
  production delegation and does not capture or pass quarantine state.
- The publish closure's first argument is always the verified `temporary` path
  and its second argument is always the final `cached` path.
- `std::io::Result<()>` is `Result<(), std::io::Error>`; the `Err(error)` in the
  required match is therefore a `std::io::Error`. `NativeError::io` accepts an
  `impl fmt::Display`, converts that I/O error to the existing diagnostic, and
  attaches the supplied `cached` path. There is no type conversion ambiguity.
- Preserve the current sequence:
  download to a unique sibling, verify the temporary, attempt atomic publish,
  and, if publish fails, accept only an independently verified cache entry.
- Implement the result and cleanup block with this exact behavior:

  ```rust
  let result = (|| {
      downloader.download_to(source, &temporary)?;
      verify_source(&temporary, source)?;
      match publish(&temporary, cached) {
          Ok(()) => Ok(cached.to_path_buf()),
          Err(error) => {
              if source_node_exists(cached)? {
                  verify_source(cached, source)?;
                  Ok(cached.to_path_buf())
              } else {
                  Err(NativeError::io(
                      "publish verified native source",
                      cached,
                      error,
                  ))
              }
          }
      }
  })();
  let _ = fs::remove_file(&temporary);
  if result.is_err() {
      if let Some(quarantine) = &quarantine {
          if source_node_exists(cached).is_ok_and(|exists| !exists) {
              let _ = fs::rename(quarantine, cached);
          }
      }
  } else if let Some(quarantine) = &quarantine {
      remove_quarantine(quarantine);
  }
  result
  ```

- The publish closure is called exactly once, only after download and temporary
  verification succeed, and without a prior `cached` existence check. It is not
  called if download or temporary verification fails.
- On publish failure:
  - if `cached` is absent, return the publish I/O error mapped with the existing
    `publish verified native source` diagnostic;
  - if `cached` exists but its verification fails, return that verification
    error, not the earlier publish error;
  - only a successfully verified `cached` entry converts the publish failure to
    success.
- The best-effort `fs::remove_file(&temporary)` is intentionally attempted
  exactly once after every computed result: downloader failure, temporary
  verification failure, publish success, publish failure with fallback success,
  publish failure with absent cache, and publish failure with invalid cache.
  `NotFound` after successful rename or before a downloader created the file is
  intentionally ignored. Running this cleanup on the success path is an
  intentional behavior change from the merged code and is the bug fix.
- Quarantine handling runs after that cleanup attempt, with the existing error
  branch and success branch unchanged.
- `unique_sibling(cached, "download")` currently produces
  `.{cached_file_name}.download.{process_id}.{nanoseconds}` in `cached`'s parent.
- Add a regression test whose downloader writes the expected bytes both to its
  provided unique temporary destination and to the captured `cached` path. Call
  `ensure_source_with_publish` with an injected
  `FnOnce(&Path, &Path) -> std::io::Result<()>` that returns
  `std::io::Error::new(std::io::ErrorKind::PermissionDenied, "injected publish failure")`.
  Assert that the operation returns the captured `cached` path and that `cached`
  contains the verified bytes. Define
  `cached_file_name` as
  `cached.file_name().unwrap().to_string_lossy()` and assert that `cached`'s
  parent has no entry whose filename starts with
  `format!(".{cached_file_name}.download.")`.
- The fallback-success regression test must follow this exact observable setup:
  - create an otherwise empty unique test directory below
    `std::env::temp_dir()` using
    `unique_sibling(&std::env::temp_dir().join("elephc-source-race"), "test")`,
    call `fs::create_dir_all` on the returned unique path, then use
    `source.tar.gz` inside it as `cached`;
  - assert `cached` does not exist before invoking
    `ensure_source_with_publish`, ensuring the initial cache check cannot return
    early;
  - give the downloader a `Cell<usize>` call counter and assert it is called
    exactly once; the downloader alone writes both the trait-provided temporary
    destination and `cached` to simulate the concurrent winner;
  - give the publish closure its own `Cell<usize>` counter, capture its first
    path argument in `RefCell<Option<PathBuf>>`, assert inside the closure that
    the second argument equals `cached`, perform no filesystem write in that
    closure, then return the injected `PermissionDenied` error;
  - assert the publish closure was called exactly once, the returned `PathBuf`
    equals `cached`, and, after the operation returns, the captured temporary
    path no longer exists; failure of the best-effort cleanup makes this
    regression fail, which is intentional;
  - call `verify_source(&cached, &source)` successfully, then inspect metadata
    and assert `cached` is a non-symlink regular file whose length equals the
    fixture length; `verify_source` supplies the exact SHA-256 assertion;
  - assert the directory contains only `cached`, has no filename beginning with
    `.{cached_file_name}.download.`, and has no filename beginning with
    `.{cached_file_name}.quarantine.`;
  - remove the unique directory at the end.
- Add a second error-path test in the same module. Its downloader writes a
  partial file only to the trait-provided temporary destination and then returns
  `NativeError::new(NativeErrorKind::Network, "injected download failure")`.
  Assert that the publish closure counter remains zero, the returned error text
  contains `injected download failure`, and the captured directory has no
  `.{cached_file_name}.download.*` entry. Production cleanup removes the partial
  temporary; test teardown then removes the now-empty unique directory. This
  proves download failure skips publication and still cleans a partially
  written temporary.
- In `download.rs` tests, retain `use super::*`, change the cell import to
  `use std::cell::{Cell, RefCell}`, and use the module's existing `PathBuf`
  import for the captured temporary.

### 2. Locked-install lock expansion

- In `install`, replace the unconditional expansion and the following standalone
  `if locked` with this exact code, preserving the three deliberately distinct
  error mappings:

  ```rust
  let desired = if locked {
      let current = NativeLock::load(&project.lock).map_err(|_| {
          NativeError::new(
              NativeErrorKind::Lock,
              "--locked requires an existing current elephc.lock",
          )
          .with_path(&project.lock)
          .with_project(&project.root)
          .with_recovery(&reconcile_recovery)
      })?;
      current
          .validate_current(&manifest)
          .map_err(|error| {
              error
                  .with_path(&project.lock)
                  .with_project(&project.root)
                  .with_default_recovery(&reconcile_recovery)
          })?;
      None
  } else {
      Some(NativeLock::from_manifest(&manifest).map_err(|error| {
          error
              .with_project(&project.root)
              .with_default_recovery(&reconcile_recovery)
      })?)
  };
  ```

  Locked mode does not depend on a precomputed `desired`:
  `validate_current(&manifest)` performs its one required manifest expansion
  internally. Unlocked mode performs its one required expansion in the `else`
  branch.
- After the guarded lock write shown below, `desired` has no further use.
- Materialization remains after lock validation/expansion.
- Replace `if !locked` with:

  ```rust
  if let Some(desired) = desired {
      atomic_write(&project.lock, desired.render()?.as_bytes())?;
  }
  ```

- Preserve project locking, error context, recovery commands, materialization
  ordering, and output text.
- Existing locked-install and lockfile tests cover observable behavior; this
  change does not add an instrumentation-only production API merely to count a
  pure lock expansion.

### 3. Shared recipe file utility

- Add a module private outside `native_deps` at
  `src/native_deps/recipes/util.rs` with this preamble:

  ```rust
  //! Purpose:
  //! Shares filesystem validation and retained-output copying across curated native recipes.
  //!
  //! Called from:
  //! - `crate::native_deps::recipes::pcre2` and `crate::native_deps::recipes::zlib`.
  //!
  //! Key details:
  //! - Rejects empty, symlinked, or non-regular files while preserving package-specific diagnostics.
  ```

- Give every function, including test functions, a specific `///` docblock.
- Register it with exactly `pub(super) mod util;` before the two public recipe
  modules in `recipes/mod.rs`. This makes the module visible only to its
  `native_deps` parent, not as a crate-public API.
- The helpers remain `pub(super)` inside `recipes::util`; they are visible in
  the `recipes` parent and its `pcre2`/`zlib` descendants. This visibility
  combination unambiguously permits
  `super::util::{copy_regular, require_regular}` from both recipe modules.
- After the required preamble, `recipes/util.rs` must import exactly:

  ```rust
  use std::fs;
  use std::path::{Path, PathBuf};

  use super::super::error::{NativeError, NativeErrorKind};
  ```
- Define the shared helpers with these signatures:

  ```rust
  pub(super) fn require_regular(package: &str, path: &Path) -> Result<(), NativeError>

  pub(super) fn copy_regular(
      package: &str,
      source: &Path,
      destination: &Path,
  ) -> Result<PathBuf, NativeError>
  ```

- Implement their bodies with the existing invariant and exact diagnostic
  construction:

  ```rust
  pub(super) fn require_regular(
      package: &str,
      path: &Path,
  ) -> Result<(), NativeError> {
      let action = format!("inspect {package} recipe file");
      let metadata = fs::symlink_metadata(path)
          .map_err(|error| NativeError::io(&action, path, error))?;
      if !metadata.file_type().is_file()
          || metadata.file_type().is_symlink()
          || metadata.len() == 0
      {
          return Err(NativeError::new(
              NativeErrorKind::Build,
              format!(
                  "{package} recipe file is missing, empty, symlinked, or not regular"
              ),
          )
          .with_path(path));
      }
      Ok(())
  }

  pub(super) fn copy_regular(
      package: &str,
      source: &Path,
      destination: &Path,
  ) -> Result<PathBuf, NativeError> {
      require_regular(package, source)?;
      let action = format!("copy retained {package} output");
      fs::copy(source, destination)
          .map_err(|error| NativeError::io(&action, destination, error))?;
      require_regular(package, destination)?;
      Ok(destination.to_path_buf())
  }
  ```

- Interpolate `package` verbatim into all three diagnostics. Call the helpers
  with `"PCRE2"` from `pcre2.rs` and `"zlib"` from `zlib.rs`.
- Make PCRE2 pass `"PCRE2"` and zlib pass `"zlib"`, preserving these exact
  existing diagnostics:
  - `inspect <package> recipe file`;
  - `<package> recipe file is missing, empty, symlinked, or not regular`;
  - `copy retained <package> output`.
- Remove the duplicated local helpers and now-unused imports from both recipe
  modules.
- In both recipe modules, keep `use std::fs;` and `use std::path::Path;`, remove
  `PathBuf` and `NativeErrorKind`, and import
  `super::util::{copy_regular, require_regular}`.
- Update each call to the exact parameter order
  `require_regular("<package>", path)` and
  `copy_regular("<package>", source, destination)?`. The returned `PathBuf`
  remains intentionally discarded after `?`, exactly as at the current call
  sites.
- Unit-test the shared helper with a successful non-empty file copy and rejected
  empty/non-regular inputs inside `util.rs` under `#[cfg(test)] mod tests`.
  In-module tests can call the `pub(super)` helpers directly. Give every test
  function a behavior-specific `///` docblock. Use a unique directory below
  `std::env::temp_dir()` created with
  `crate::native_deps::util::unique_sibling(
  &std::env::temp_dir().join("elephc-recipe-util"), "test")`; this includes
  process ID and nanoseconds and is safe for parallel tests. Call
  `fs::create_dir_all` on that returned path before creating fixtures, and
  remove it after each test. The test module imports `super::*` and
  `crate::native_deps::util::unique_sibling`; no download-test `RefCell` or
  captured `PathBuf` is used in this module. Symlink behavior remains enforced
  by the same predicate and may be tested only on platforms where deterministic
  symlink creation is available.

## Invariants and non-goals

- No catalog, manifest, lockfile schema, cache layout, receipt, recipe command,
  linker, target, or PHP-visible behavior changes.
- No weakening of checksum, size, regular-file, or symlink validation.
- No loss or genericization of package-specific diagnostics.
- No mutation of the merged `feat/native-dependencies` branch and no edits
  outside this dedicated follow-up branch.
- Do not run `cargo fmt` or `cargo fmt --all`; format touched files manually and
  use focused `rustfmt --check` invocations.
- Do not run the full local test suite.

## Acceptance checks

- `cargo test --lib native_deps::download`
- `cargo test --lib native_deps::recipes`
- `cargo test --lib native_deps::lockfile`
- `cargo test --lib locked_install_rejects_absent_and_stale_lock`
- `cargo check --bin elephc --tests --locked`
- `rustfmt --edition 2021 --check src/native_deps/download.rs
  src/native_deps/orchestration.rs src/native_deps/recipes/mod.rs
  src/native_deps/recipes/util.rs src/native_deps/recipes/pcre2.rs
  src/native_deps/recipes/zlib.rs`
- `git diff --check`

The root crate declares Rust edition 2021. The focused locked-install command
above resolves the existing fully qualified test
`native_deps::orchestration_tests::locked_install_rejects_absent_and_stale_lock`;
it is intentionally written as a Cargo substring filter.

The binary name in the acceptance command is exactly `elephc`
(`e-l-e-p-h-c`), matching `Cargo.toml`. It is not `elefpc`.

## Consensus protocol

GLM 5.2, Kimi K2.7, and MiniMax M3 receive the exact same specification and
relevant current source excerpts. Each reviewer must return either unconditional
`ACCEPT` or `REJECT` with concrete blocking changes. Conditional acceptance,
ambiguity, or a requested change counts as rejection. If any reviewer rejects,
the specification is amended and all three reviewers evaluate the same new
revision from scratch. Implementation starts only after all three
unconditionally accept the same specification digest.

## Review record

### Revision 1 — digest `0e47ada2bdf30f9b2ec11f5d159b039a6efadf6ae24281d33e204a440dc661bc`

- GLM 5.2: `REJECT`; requested exact temporary filename matching, locked-mode
  data flow, private-module registration, imports, and formatting scope.
- Kimi K2.7: `REJECT`; requested an exact publish closure contract, explicit
  unconditional cleanup placement, quarantine ordering, verbatim diagnostic
  interpolation, preamble, and formatting scope.
- MiniMax M3: semantically approved with `BLOCKERS: none`, but omitted the
  mandatory `VERDICT` line; treated as `REJECT` under the consensus protocol.

### Revision 2 — digest `17fb22d116269a7a9cf9f950b8e3b5ab827b15552168d5c3a994e4d87a933f58`

- GLM 5.2: `REJECT`; requested an explicit result/error priority and a
  path-by-path confirmation that temporary cleanup is intentional.
- Kimi K2.7: `REJECT`; requested exact publish-error mapping, error kind and
  filename expression in the regression test, distinct lock mappings, and the
  in-module test location.
- MiniMax M3: `ACCEPT`; no blockers.

### Revision 3 — digest `631efa68c7909b1b431c0ae769fd1a516ad3700e6e56687a068e76983efdd782`

- GLM 5.2: `REJECT`; accepted the production changes but requested explicit
  call counters, captured temporary identity, verified final metadata, and
  absence of download/quarantine siblings in the regression.
- Kimi K2.7: `REJECT`; requested explicit helper ownership of quarantine state,
  deterministic parallel-safe test directories, visibility semantics, caller
  argument order, path attachment, edition, and test-filter confirmation.
- MiniMax M3: returned no verdict body; treated as `REJECT` under the consensus
  protocol.

### Revision 4 — digest `8a836fe6dcede85844ae1fe94dd600f54aa2b7a8ea2752b1970545c3b1b11e63`

- GLM 5.2: `REJECT`; requested explicit wrapper/import/test types and repeated
  several claims contradicted by Rust types or the supplied source. Revision 5
  documents those types directly without changing the sound design.
- Kimi K2.7: `REJECT`; requested explicit initial-cache absence, directory
  creation/teardown, wrapper scope, and a less restrictive recipe utility
  module visibility.
- MiniMax M3: `ACCEPT`; no blockers.

### Revision 5 — digest `fdd5585aacf7a493fb9d85143aac6aee721a730e4c169a0abc1fc298453648c3`

- GLM 5.2: `ACCEPT`; no blockers.
- Kimi K2.7: `REJECT`; requested a single unambiguous statement separating the
  thin public wrapper from the private helper that owns all logic.
- MiniMax M3: `REJECT`; misread the existing `--bin elephc` acceptance command
  as `--bin elefpc`. Revision 6 spells the unchanged correct name explicitly.
