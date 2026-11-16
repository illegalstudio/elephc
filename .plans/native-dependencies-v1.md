# Native dependencies: post-v1 backlog

- [x] Ship exact manifests/locks, verified source downloads, isolated recipes, cache publication, recovery diagnostics and explicit cache pruning.
- [x] Add transitive catalog dependencies, more native packages and all five supported targets.
- [x] Document managed-package scope, toolchain identity, runtime capability selection and the read-only compile path.
- [ ] Design signed prebuilt artifacts as an explicit provider using the same lock/receipt and verification model.
- [ ] Evaluate an explicit system provider only if a concrete packaging/debug requirement justifies a separate ABI and reproducibility contract.
- [ ] Decide whether a macOS cache-default migration is useful; preserve environment overrides and existing-cache compatibility.
- [ ] Review recipe-only revision upgrade diagnostics and recovery commands.
- [x] Reconcile obsolete process/formatting guidance with the current repository instructions.

## Shipped baseline

Reviewed against release candidate `df57a9800f` on 2026-10-19. The catalog has
PCRE2, zlib, OpenSSL, nghttp2, libssh2, curl, Oniguruma and libxml2. Package
selection, dependencies and ordered outputs are typed catalog data, not
project-supplied recipes. Each recipe declares its supported target set within
the five-target policy; iOS toolchains require the matching Xcode SDK.

| Boundary | Source of truth |
|---|---|
| Catalog versions, dependencies and provided libraries | `src/native_deps/catalog.rs`, `src/native_deps/recipes/` |
| Manifest/lock reconciliation and commands | `src/native_deps/orchestration/` |
| Verified source publication and extraction | `src/native_deps/download.rs`, `src/native_deps/archive.rs` |
| ABI/toolchain-qualified cache and receipts | `src/native_deps/cache.rs` |
| Compiler requirements and installed-artifact resolution | `src/native_deps/requirements.rs`, `src/native_deps/resolver.rs`, `src/pipeline/backend.rs` |

The permanent [native dependency guide](../docs/compiling/native-dependencies.md)
and [CLI reference](../docs/compiling/cli-reference.md) supersede the original
v1 design narrative. Cache precedence remains `ELEPHC_NATIVE_CACHE`, then
`XDG_CACHE_HOME/elephc/native`, then `HOME/.cache/elephc/native`.

Regex is a selected runtime capability; opaque eval alone does not unconditionally
require PCRE2. `--with-regex` and relevant detected usage request it. Mbregex
uses managed Oniguruma. The former blanket eval-to-PCRE2 TODO is superseded by
this documented capability boundary.

## Remaining design decisions

Signed prebuilts must verify provenance and artifact identity before atomic
publication. They must not weaken exact lock checks, source/recipe/target/ABI
identity, toolchain compatibility, interrupted-build cleanup, or offline rules.

A system provider is not an implemented feature or an implied fallback. Any
future proposal needs a named consumer, explicit opt-in, negative ABI checks,
and documentation of its reproducibility limits before it becomes product work.

For recipe-only upgrades, check `native install --locked`, `native update`,
stale-receipt detection and printed recovery together. Preserve deterministic
locks and explain why a lock refresh is required; do not silently repair it
inside ordinary compilation.

## Invariants that stay closed

- Ordinary compilation never downloads, builds or repairs managed packages.
- Missing or invalid managed artifacts fail closed; no production `-lpcre2`,
  `-lcurl` or `-lxml2` fallback satisfies the requirement.
- Composer source packages, Rust bridges, toolchains/SDKs and user FFI libraries
  retain their separate workflows.
- Project-supplied recipes, arbitrary download URLs, semver solving and silent
  system substitution remain rejected non-goals, not unchecked tasks.
- Preserve bounded extraction, checksum/size validation, locks, transactional
  staging and cleanup of the caller's own temporary files.

Use focused command/receipt and target checks for a future implementation. This
consolidation ran no package downloads, native builds, tests or behavioral probes.
