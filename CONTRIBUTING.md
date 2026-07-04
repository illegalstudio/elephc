# Contributing to Elephc

First of all, thank you for considering contributing to Elephc! ❤️

Every contribution matters, whether it's fixing a typo, improving documentation, reporting a bug, or implementing a new feature.

## Before You Start

If you're planning to work on a significant feature or architectural change, please open an issue first so we can discuss the design before implementation.

This helps avoid duplicated work and ensures that the proposed solution aligns with the project's long-term direction.

## Reporting Bugs

When reporting a bug, please include as much information as possible:

* Operating system
* CPU architecture
* Elephc version
* Steps to reproduce
* Expected behavior
* Actual behavior
* Relevant code snippets or logs

A minimal reproducible example is always appreciated.

## Pull Requests

Before opening a Pull Request, please ensure that:

* Your code builds successfully.
* Existing tests continue to pass.
* New functionality includes tests whenever possible.
* Documentation is updated when appropriate.
* Commits are reasonably organized and have meaningful commit messages.

Please keep Pull Requests focused. Smaller PRs are much easier to review than very large ones.

## Coding Style

Try to follow the style already used throughout the codebase.

Consistency is generally more important than personal preference.

## Adding a built-in function

elephc's PHP built-in functions are declared **once** in a single-source registry.
Each builtin has one *home file* at `src/builtins/<area>/<name>.rs` that declares it
with the `builtin!` macro; all declarations are collected at link time through the
`inventory` crate. From that single declaration the compiler derives the catalog
name-set (case-insensitive lookup, `function_exists`, namespace fallback,
redeclaration checks), the call signature (named arguments, defaults, by-ref params,
variadic, arity), the type-check entry, the EIR lowering dispatch, and the generated
documentation.

Do **not** re-add builtin names to the old hand-maintained tables (`catalog.rs`,
`signatures.rs`, per-area `check_builtin` arms). They are superseded by the registry;
a builtin is fully wired the moment its home file compiles.

### 1. Create the home file

Add `src/builtins/<area>/<name>.rs` and register it in `src/builtins/<area>/mod.rs`
with `pub mod <name>;` (keep the list alphabetical). Areas are `string`, `array`,
`math`, `io`, `system`, `types`, `callables`, `spl`, `pointers` (plus `internal` for
compiler-internal builtins). One builtin per home file; the file owns its declaration
plus its `check`/`lower` hooks. Start with the mandatory `//!` module preamble.

### 2. Declare it with `builtin!`

```rust
builtin! {
    name: "strlen",
    area: String,
    params: [string: Str],
    returns: Int,
    check: check,
    lazy_check: true,
    lower: lower,
    summary: "Returns the length of a string.",
    php_manual: "function.strlen",
}
```

Fields must appear in this canonical order; optional fields (marked `?`) may be
omitted:

`name`, `area`, `params`, `variadic?`, `min_args?`, `max_args?`, `arity_error?`,
`returns`, `by_ref_return?`, `check?`, `lazy_check?`, `lower`, `summary`, `examples?`,
`php_manual?`, `deprecation?`, `internal?`.

- **`params`** — `[name: TypeSpec, name: TypeSpec = DefaultSpec::Variant, ...]`. A
  parameter with `= DefaultSpec::…` is optional; without it, required. Prefix a
  parameter with `ref` to pass it by reference (mutating builtins):
  `params: [ref array: Mixed, offset: Int]`. Parameter names become PHP's
  named-argument keys and must match PHP exactly (Rust keywords work as names via raw
  identifiers, e.g. `r#type`).
- **`returns` and param `TypeSpec`** — written as a bare scalar type ident: `Int`,
  `Float`, `Str`, `Bool`, `Mixed`, `Null`, `Void`. Non-scalar shapes (arrays, unions,
  resources) are declared as `Mixed`; supply the precise type from a `check` hook when
  it matters (see the note in step 3).
- **`DefaultSpec`** — full path form: `DefaultSpec::Null`, `DefaultSpec::Int(0)`,
  `DefaultSpec::Bool(false)`, `DefaultSpec::Float(1.5)`, `DefaultSpec::Str("…")`,
  `DefaultSpec::IntMax`, `DefaultSpec::IntMin`, `DefaultSpec::EmptyArray`.
- **`variadic`** — the PHP name of the trailing variadic parameter, e.g.
  `variadic: "values"`.
- **`min_args` / `max_args` / `arity_error`** — override only the arity check (not the
  derived signature or the parity gate). Use when a builtin's PHP arity is
  tighter/looser than its declared parameter list, or needs a verbatim error message.
- **`summary` / `examples` / `php_manual` / `deprecation`** — documentation metadata
  surfaced by the `gen_builtins` exporter.
- **`internal: true`** — a compiler-internal builtin that is not PHP-visible and is
  excluded from catalogs and docs.

A builtin whose return type does not depend on its arguments and needs no extra
validation can omit `check` entirely — `returns:` is then authoritative for the
checker.

### 3. The `check` hook (type checking)

Add a `check` hook when the return type depends on argument types/values, or when the
call needs validation beyond arity and the parameter list:

```rust
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    if !matches!(ty, PhpType::Str | PhpType::Mixed | PhpType::Union(_)) {
        return Err(CompileError::new(cx.span, "strlen() argument must be string"));
    }
    Ok(PhpType::Int)
}
```

The hook receives `BuiltinCheckCtx { checker, name, args, span, env }` and returns the
call's `PhpType` (or a diagnostic). Its returned type overrides `returns:` for the
checker.

For a normal builtin the registry already checks arity and infers every argument once
(for side effects such as variable narrowing and undefined-variable diagnostics)
before calling the hook. Set **`lazy_check: true`** when the hook must control
inference order — most importantly when it injects element/parameter type hints into
an unannotated closure argument *before* that closure is inferred (e.g. `usort`,
`array_map` with a callback). With `lazy_check: true` the hook is responsible for
inferring each argument itself.

> **Return typing is a checker-only contract.** The `returns:` field and the `check`
> hook drive the **type checker** only. The EIR backend derives call return types
> independently in `call_return_type` (`src/ir_lower/expr/mod.rs`). If you declare
> `returns: Mixed` + a precise `check` hook (the standard pattern for non-scalar
> returns), you must also add a matching arm to the EIR return-type derivation, or the
> checker and EIR will disagree on the value's type. This caveat is documented on the
> `returns`/`check` fields in `src/builtins/spec.rs`.

### 4. The `lower` hook (EIR codegen)

`lower` is mandatory — it is the builtin's EIR lowering entry point. Keep it a thin
wrapper that dispatches to the actual emitter:

```rust
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::lower_strlen(ctx, inst)
}
```

Write the emitter itself under `src/codegen_ir/lower_inst/builtins/<area>/`, following
the target-aware codegen conventions in `CLAUDE.md` (support every target through
`emitter.target`, one emitter per leaf file, an inline `//` comment on every
`emitter.instruction(...)`). If the builtin needs a runtime routine, add it under
`src/codegen/runtime/<category>/`. The registry dispatches `spec.lower` first, so no
match arm needs editing.

### 5. What derives automatically

Once the home file compiles, all of the following see the builtin with no further
edits: `function_exists()` and case-insensitive/namespaced lookup, the named-argument
`FunctionSig`, first-class-callable syntax (`strlen(...)`), the arity check and its
error message, and the `gen_builtins` JSON docs export.

### 6. Surfaces you still wire by hand

The registry single-sources the declaration, signature, checker entry, lowering
*dispatch*, and docs. These related surfaces are **not** derived and must be updated
when relevant:

- **The EIR emitter** the `lower` hook calls (and any runtime routine it needs).
- **EIR return typing** — see the note in step 3.
- **Optimizer effects** in `src/optimize/effects/builtins.rs` when purity, reads/writes,
  or thrown/fatal behavior matter for DCE and constant propagation. Never mark a call
  pure if it can read/write globals, files, the environment, heap state, or emit output.
- **Runtime-callable wrapper exclusion** — if the builtin cannot be dispatched through
  the dynamic string-callable wrapper, add it to `runtime_builtin_wrapper_excluded()`
  in `src/codegen/callable_dispatch.rs`.

### 7. Tests, examples, and docs

- Add codegen tests for normal use (plus at least one case-insensitive or namespaced
  call for a PHP-visible builtin), and error tests for wrong argument count/types.
- Add or update an example under `examples/` when the builtin is a notable user-facing
  feature.
- Document the PHP surface (signature, parameters, return type, a short example) on the
  relevant `docs/php/` page.
- The signature/arity parity gates (`derived_signatures_match_legacy`,
  `arity_messages_match_legacy` in `src/builtins/parity_tests.rs`) must stay green.

### 8. Not every "builtin" is a function

A small set of PHP language constructs — `isset`, `unset`, `empty`, `exit`, `die`, plus
the `buffer_*` intrinsics — are l-value/lazy constructs with dedicated EIR paths and are
intentionally kept in the checker (`numeric`/`arrays` `check_builtin`), not in the
registry. Do not migrate those into `builtin!`.

## Adding functionality via a Rust crate (bridge crates)

elephc compiles a static subset of PHP straight to native code, so most features
are implemented directly in the compiler (lexer → parser → type checker → EIR →
codegen). But some functionality is heavy, well-served by an existing Rust
library, or simply not worth re-implementing by hand — TLS, PDO database drivers,
image codecs, hashing, timezone tables, Phar archives, the `--web` server.

**If the functionality you want to add can be realized through Rust libraries,
implement it as a bridge crate and register a `--with-<crate>` flag** instead of
hand-writing it in the runtime. A *bridge crate* is a `staticlib` under
`crates/elephc-<name>/` that elephc links into compiled PHP programs on demand.
The whole linking model is table-driven from the `BRIDGES` table in
`src/linker.rs`, so wiring a new bridge is a single table entry plus the PHP-facing
surface.

Follow these technical specifications in full.

### 1. Decide it belongs in a crate

Use a bridge crate when the feature (a) maps cleanly onto a maintained Rust crate,
(b) is optional (programs that do not use it must not pay for it), and (c) exposes
a small, stable C ABI surface. Do **not** use a crate for core language semantics,
ownership/GC, or anything that must be understood line-by-line in the generated
assembly — that belongs in the compiler proper (`AGENTS.md`/`CLAUDE.md`).

### 2. Create the crate

Create `crates/elephc-<name>/` as a workspace member:

```toml
# crates/elephc-<name>/Cargo.toml
[package]
name = "elephc-<name>"
version = "0.1.0"
edition = "2021"
license = "MIT"
publish = false

# staticlib: linked into compiled PHP programs. rlib: lets the bridge be
# unit-tested via `cargo test -p elephc-<name>`.
[lib]
crate-type = ["staticlib", "rlib"]

[dependencies]
# Prefer pure-Rust, musl-friendly crates so the Linux Docker test images link.
```

Add the crate to **both** `members` and `default-members` in the root
`Cargo.toml`, and an entry under `[workspace.dependencies]`. Being a default
member is what makes a plain `cargo build` materialize `target/<profile>/libelephc_<name>.a`.

Expose a **stable C ABI**: every entry point is `#[no_mangle] pub extern "C"` and
must be panic-free across the boundary (catch/encode errors, return error codes or
null; never unwind into generated code). Keep the surface small and explicit —
pass pointers + lengths for strings/buffers, return primitive status values. Name
exports `elephc_<name>_*` so they are easy to find and namespace-clean.

Every supported target must build and link the crate: `macos-aarch64`,
`linux-aarch64`, `linux-x86_64`. A bridge that only works on one target is not
acceptable (see the supported-target policy in `CLAUDE.md`).

### 3. Register the bridge in `BRIDGES` (`src/linker.rs`)

Add one `BridgeStaticlib` entry. This is the only linker change required —
discovery, on-demand build, search paths, whole-archiving, and macOS frameworks
are all driven from the table:

```rust
BridgeStaticlib {
    lib_name: "elephc_<name>",          // `-l` name → links lib<name>.a
    env_var: "ELEPHC_<NAME>_LIB_DIR",   // dir override for prebuilt staticlibs
    crate_name: "elephc-<name>",        // cargo package (auto-build + workspace)
    flag_name: "<name>",                // user-facing `--with-<name>` flag
    whole_archive: false,               // true if link-time side effects / owns entry
    macos_frameworks: &[],              // transitive native deps' frameworks
    needs_libdl: true,                  // Rust runtime/unwinder needs -ldl on Linux
},
```

Set `whole_archive: true` only when the staticlib has link-time side effects that
must survive (e.g. a provider registration) or owns the program entry point (like
`elephc_web`). Otherwise leave it `false`; `--with-<name>` force-loads it anyway.

### 4. Expose the PHP-visible surface

Pick one of two paths so PHP code can actually call into the crate. Both make the
type checker record `elephc_<name>` as a required library, which is what links the
bridge automatically when the feature is used.

- **Core builtins** — when the feature is a set of PHP built-in functions
  (`md5()`, `hash()`, …). Follow "Adding a built-in function" above (declare each
  builtin in `src/builtins/<area>/` with its `check`/`lower` hooks), and call
  `Checker::require_builtin_library("elephc_<name>")` from the `check` hook when a
  builtin that needs the crate is used. The PHP names are always available, so no
  prelude is needed.

- **A prelude** — when the feature is a set of classes/functions written in
  elephc-PHP that wrap the crate (PDO, timezone introspection, image). Add
  `src/<name>_prelude.rs`:
  - a static elephc-PHP source string declaring `extern "elephc_<name>" { ... }`
    plus the wrapper classes/functions;
  - `pub fn inject_if_used(program: Program, force: bool) -> Program` that returns
    `program` unchanged when `!force && !detect::program_uses_<name>(&program)`,
    and otherwise tokenizes/parses the prelude and prepends it (declarations are
    hoisted, so prepending does not change execution order);
  - a `detect` submodule that scans the AST for the feature's symbols.
  Wire the call into `src/pipeline.rs` after include resolution, mirroring the PDO
  block. The injected `extern "elephc_<name>"` block is what adds the bridge to
  `required_libraries`, so usage auto-links it.

### 5. Register the `--with-<name>` flag

Registering the bridge in `BRIDGES` with a `flag_name` is what enables the flag —
`src/cli.rs` parses `--with-<flag_name>` generically against the table (an unknown
crate is rejected, listing the valid ones), and `src/pipeline.rs` force-links the
bridge (whole-archived via `forced_bridge_libs`) for every crate named in
`with_crates`.

If your crate uses a **prelude**, also thread the force flag into its
`inject_if_used` call in `src/pipeline.rs`, mirroring pdo/tz/image:

```rust
let ast = <name>_prelude::inject_if_used(ast, with_crates.contains("<name>"));
```

so `--with-<name>` declares the PHP surface even when auto-detection would not.
Core-builtin crates need nothing extra here — force-linking the staticlib is
enough because their PHP names are always available.

`--with-<name>` semantics: it guarantees the crate is compiled in (whole-archived,
not dead-stripped) and, for prelude crates, that its API is declared — useful when
detection cannot see indirect usage. It is additive and never disables
auto-detection. Note that it increases binary size by force-including the whole
crate.

### 6. Tests

- Unit-test the `BRIDGES`/flag mapping in `src/linker.rs` and the CLI parsing in
  `src/cli.rs` (both are fast, no assembler/linker).
- Add codegen/end-to-end tests that exercise the feature, and error tests for
  argument-count/usage diagnostics, per the test-coverage rules in `CLAUDE.md`.
- The crate itself should have `cargo test -p elephc-<name>` unit tests (that is
  what the `rlib` crate-type is for).
- Run focused tests locally; CI runs the full supported-target matrix.

### 7. Documentation

- `docs/compiling/cli-reference.md` — add the `--with-<name>` flag.
- `docs/compiling/linking-and-conditional-compilation.md` — describe the bridge
  and its auto-link trigger.
- The relevant `docs/php/` or `docs/beyond-php/` page — document the PHP surface.
- Update `CLAUDE.md` only if you changed the bridge/flag mechanism itself.

## Contributor Certification

By submitting a contribution to this repository, you represent and warrant that:

* you have the legal right to submit the contribution;
* the contribution is your original work, or you have sufficient rights to submit it;
* to the best of your knowledge, the contribution does not knowingly infringe the intellectual property rights of any third party;
* you agree that your contribution will be distributed under the same license as the Elephc project.

You retain the copyright to your contributions.

## Code of Conduct

Please be respectful and constructive.

Healthy technical discussions are encouraged. Personal attacks, harassment, or disrespectful behavior will not be tolerated.

## Questions

If you're unsure about anything, feel free to open an issue or start a discussion.

Contributions of all sizes are welcome.

Happy hacking! 🐘
