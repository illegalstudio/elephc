# PR #650 managed-PCRE2 standalone test-fixture specification

## Status and scope

This is a test-infrastructure addendum for PR #650. It does not amend
`.plans/pr598-greptile-follow-up.md`; that specification and its accepted SHA-256
digest remain unchanged.

All work stays on the existing `fix/pr598-greptile-feedback` branch and in the
existing `pr598-greptile-feedback` worktree. No additional branch or worktree is
created.

Implementation is blocked until GLM 5.2, Kimi K2.7 Code, and MiniMax M3 all
accept the exact same digest of this document with no blocker.

## Evidence

The current PR #650 CI run reports 17 failures in
`Non-Codegen Tests (linux-aarch64)`. Every failure reaches the production
fail-closed resolver and reports:

```text
native project error: regex support requires managed native package pcre2
project: not found
```

The final PR #598 CI run already had the same 17 failures on all three supported
targets. They are inherited from the managed-native migration, not introduced by
the Greptile fixes or the changelog follow-up in PR #650.

The failing tests are:

- `eval_object_handle_tests`:
  - `eval_can_name_enum_cases_matches_php_case_numbering`
  - `eval_writes_propagate_without_renumbering`
  - `object_created_inside_eval_takes_the_next_handle`
  - `repeated_and_nested_eval_do_not_drift_handles`
- `eval_resource_id_tests`:
  - `a_closed_eval_stream_does_not_release_its_id`
  - `eval_resource_ids_are_stable_across_runs_of_one_binary`
  - `eval_resources_are_numbered_consecutively_from_five`
  - `eval_streams_never_alias_a_host_descriptor`
  - `host_and_eval_resources_draw_from_one_shared_counter`
- `ir_backend_smoke_test`:
  - `ir_backend_fatals_on_invalid_dynamic_instanceof_target`
  - `ir_backend_handles_dynamic_instanceof_on_classes_with_methods`
  - `ir_backend_handles_dynamic_instanceof_targets`
  - `ir_backend_handles_preg_match_captures`
  - `ir_backend_handles_preg_replace_callback_static_function_fcc`
  - `ir_backend_handles_preg_replace_callback_static_string`
  - `ir_backend_handles_simple_regex_builtins`
- `php_version_surface_tests`:
  - `eval_sees_the_same_version_surface_on_the_default_profile`

Commit `ac0cbd5077` added a hermetic managed-PCRE2 CLI fixture under
`tests/codegen/support/`, but that module is compiled only into
`codegen_tests`. The four standalone integration-test binaries above have their
own CLI helpers. They create isolated source directories, set an isolated
`XDG_CACHE_HOME`, and invoke Elephc without an `elephc.toml`, `elephc.lock`, or
`ELEPHC_NATIVE_CACHE`.

The production requirement is correct:

- direct `preg_*` calls require PCRE2;
- dynamic `instanceof` makes the emitted class set include the built-in regex
  iterator classes and therefore their regex runtime methods;
- the affected dynamic-eval shapes expose runtime functionality that also
  requires the managed PCRE2 link inputs.

## Required outcome

All 17 tests compile and run through the production managed-native resolver on
macOS AArch64, Linux AArch64, and Linux x86_64 using the same test-only,
system-header-aligned PCRE2 provider already used by `codegen_tests`.

The production compiler remains fail-closed. A source program requiring regex
without a native project must still receive the existing actionable
`elephc native add pcre2` diagnostic.

## Design

### 1. Extract one reusable integration-test support module

Create `tests/support/managed_pcre2.rs` as the single home for the cohesive
test-only provider currently split between:

- `tests/codegen/support/native_projects.rs`; and
- the PCRE2 provider/shim functions in `tests/codegen/support/runner.rs`.

The support subdirectory prevents Cargo's automatic integration-test discovery
from treating the shared module as an independent `tests/*.rs` test target. The
module must:

- start with the mandatory `//!` module-level Rustdoc preamble described in
  `AGENTS.md`;
- give every explicit function a specific `///` docblock;
- be private to each integration-test crate that imports it;
- use `#![allow(dead_code)]` because different integration-test binaries consume
  different portions of the shared support API;
- accept an explicit `elephc::codegen::platform::Target` for target-sensitive
  provider operations;
- preserve the current one-target-per-test-binary invariant and cache provider
  discovery/shim construction with `OnceLock`; each cached value records the
  target and rejects a later call for a different target rather than silently
  returning the wrong artifact;
- preserve the production cache key, receipt schema, output hashes, manifest,
  lockfile, package version `10.47`, recipe `1`, and source SHA-256 used by the
  existing fixture;
- continue admitting the system PCRE2 headers/static archives only inside this
  test provider;
- copy only non-empty regular provider files into the synthetic managed
  artifact;
- compile the embedded Elephc PCRE2 shim with target-aware compiler/architecture
  arguments;
- keep tool selection and fingerprint normalization aligned with the production
  resolver, with a maintenance comment that changes to production fingerprint
  inputs must update the test fixture in the same change;
- return the isolated managed-native cache path after writing the project and
  verified receipt; and
- provide a small host-target command helper that sets
  `ELEPHC_NATIVE_CACHE` after preparing the project.

No download or source build is introduced in ordinary test execution. The
provider continues to reuse the static PCRE2 installation supplied by the local
developer environment or CI image. The current CI contract is already exercised
by the three passing `Managed Native Packages` jobs and by the existing
cross-target codegen regex tests; this change does not alter or separately pin
the CI image.

The catalog source identity remains the exact checked-in fixture constant, while
the receipt output hashes continue to be computed from the actual copied
test-provider files. Add a maintenance comment pointing to
`examples/date-json-regex/elephc.toml` and `elephc.lock` as the canonical
manifest/lock fixtures.

### 2. Keep `codegen_tests` behavior stable

Declare `tests/support/managed_pcre2.rs` from `tests/codegen_tests.rs` with
`#[path = "support/managed_pcre2.rs"]`.

Replace `tests/codegen/support/native_projects.rs` with thin adapters that pass
the codegen harness's cached `target()` into the shared module. Re-export those
adapters through `tests/codegen/support/mod.rs` so existing call sites keep
their current API.

Move the provider discovery and shim implementation out of
`tests/codegen/support/runner.rs`. The runner must keep using thin target-aware
adapters for:

- the embedded shim archive;
- `pcre2.h` and `pcre2posix.h`; and
- `libpcre2-posix.a` and `libpcre2-8.a`.

This refactor must not change direct codegen-test linking, cross-target selection,
QEMU behavior, bridge handling, archive behavior, or production code.

### 3. Wire only the standalone CLI harnesses that need managed PCRE2

Each affected standalone integration-test crate declares the shared module with
an explicit `#[path = "support/managed_pcre2.rs"]`.

- `eval_object_handle_tests.rs`: the common `compile_php` helper prepares the
  host-target managed-PCRE2 project and sets `ELEPHC_NATIVE_CACHE`. The file is
  eval-focused, so all of its CLI compilations may consistently use the managed
  fixture. The file contains no assertion for the missing-native-project
  diagnostic; that diagnostic remains covered by `tests/codegen/cli.rs`.
- `eval_resource_id_tests.rs`: the common `compile` helper does the same. The
  file's host-only control remains semantically unchanged; merely making PCRE2
  available does not force a regex requirement. This file also contains no
  missing-native-project diagnostic assertion.
- `ir_backend_smoke_test.rs`: do not prepare PCRE2 for the entire 250+ test
  binary. Add documented managed-PCRE2 variants of the existing compile/run
  helpers and route exactly the seven failing regex/dynamic-`instanceof` tests
  through them. Existing helpers and unrelated tests retain their current
  no-project behavior.
- `php_version_surface_tests.rs`: add documented managed-PCRE2 variants along
  the `compile_raw`/`compile_with_flags`/`run_for_profile` path and use them only
  for `eval_sees_the_same_version_surface_on_the_default_profile`.

The managed variants must prepare the project before spawning Elephc, set
`ELEPHC_NATIVE_CACHE` on that child command only, and preserve all existing
target/triple selection, flags, current directories, stdout/stderr assertions,
runtime arguments, and cleanup.

### 4. Changelog

Add one terse entry at the top of `CHANGELOG.md` under `## [Unreleased]`:

```markdown
- Fixed the supported-target test matrix for managed-PCRE2 programs so eval, dynamic `instanceof`, and regex regressions are validated hermetically without relying on the removed system-library fallback.
```

This satisfies the repository's same-PR changelog policy without claiming a new
production fallback or PHP-visible feature.

## Invariants and non-goals

- No new branch or worktree.
- No production changes under `src/` or `crates/`.
- No reintroduction of system-PCRE2 fallback in production.
- No weakening or bypassing of manifest, lockfile, receipt, hash, size,
  toolchain, ABI, target, or cache verification.
- No network download or source build in the ordinary test harness.
- No ignored tests, CI exclusions, allow-failure rules, target removal, or
  workflow-only masking.
- No global process-environment mutation; `ELEPHC_NATIVE_CACHE` is scoped to
  each spawned Elephc command.
- No behavior changes to the original Greptile fixes.
- Do not change `.plans/pr598-greptile-follow-up.md` or its consensus record.
- Do not run `cargo fmt` or `cargo fmt --all`.
- Do not run the full local test suite.

## Validation

Before commit:

1. Confirm the original Greptile spec SHA-256 is still
   `8b23be5880d0e261189f7c71ddca018986263f5bf97f93ecce894745ec99f686`.
2. Run the complete focused binaries:
   - `cargo test --test eval_object_handle_tests`
   - `cargo test --test eval_resource_id_tests`
3. Run the seven exact affected `ir_backend_smoke_test` tests, using exact test
   filters or one equivalent nextest expression:
   - `ir_backend_fatals_on_invalid_dynamic_instanceof_target`
   - `ir_backend_handles_dynamic_instanceof_on_classes_with_methods`
   - `ir_backend_handles_dynamic_instanceof_targets`
   - `ir_backend_handles_preg_match_captures`
   - `ir_backend_handles_preg_replace_callback_static_function_fcc`
   - `ir_backend_handles_preg_replace_callback_static_string`
   - `ir_backend_handles_simple_regex_builtins`
4. Run
   `cargo test --test php_version_surface_tests eval_sees_the_same_version_surface_on_the_default_profile`.
5. Run the existing managed CLI-provider sentinel
   `test_strict_php_eval_introspection_matches_aot_surface` and the direct
   regex-linking codegen sentinel `test_preg_match_simple`, proving the
   extraction did not regress the original harness.
6. Run `cargo check --tests --locked`.
7. Run focused
   `rustfmt --edition 2021 --check` on every changed Rust file.
8. Run `git diff --check`.

Use the existing shared Cargo target directory if the dedicated worktree does
not have enough disk space. Set `CARGO_INCREMENTAL=0` for the focused local
commands.

The complete supported-target matrix remains authoritative in GitHub CI. After
push, verify that all three `Non-Codegen Tests` jobs pass rather than merely
checking the managed-native smoke jobs.
