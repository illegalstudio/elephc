# Contributing to Elephc

First of all, thank you for considering contributing to Elephc! ❤️

Every contribution matters, whether it's fixing a typo, improving documentation, reporting a bug, or implementing a new feature.

## Before You Start

If you're planning to work on a significant feature or architectural change, please open an issue first so we can discuss the design before implementation.

This helps avoid duplicated work and ensures that the proposed solution aligns with the project's long-term direction.

## AI-Assisted Contributions

Contributions created with the help of AI tools are welcome. What matters to us is the quality and correctness of the result, not how it was produced — so the usual expectations still apply: the code must build, be covered by tests, follow the surrounding style, and come with a clear description. Please review anything an AI helps you write as carefully as you would your own work, since you remain responsible for whatever you submit.

## Planning Larger Work

If you're working toward something bigger than a single, self-contained change, we recommend writing a plan before you dive into the code. Plans live in the `.plans` directory of the repository, and every plan in `.plans` must be written in English.

Start each plan with a checklist of the tasks it involves, then follow it with the detailed implementation notes for each of them. Keeping the task list up front makes the plan's progress easy to verify at a glance — whether it's complete is simply a matter of checking which tasks are marked done.

Leave your plan in the repository until the work it describes is entirely finished. You don't have to land everything in one Pull Request: a plan may be split across several PRs, as long as the test suite stays green at every step. Once a plan is complete, the maintainers will clear it out during periodic cleanups, so there's no need to remove it yourself.

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

## Working locally

Do your work on a dedicated branch — one branch per feature or fix — and prefer a
separate **git worktree** per branch so your changes stay isolated from `main` and
from any other work in progress. Worktrees let you keep several branches checked
out at once without stashing or switching back and forth in a single checkout.

Name the branch with a short, descriptive slug behind a type prefix that mirrors
the commit-message prefixes used in this project:

- `feat/` — a new feature
- `fix/` — a bug fix
- `docs/` — documentation only
- `refactor/` — internal restructuring with no behavior change
- `chore/` — tooling, CI, or housekeeping
- `test/` — tests only

When the work tracks a GitHub issue, include the issue number, e.g.
`fix/369-tier2-range-analysis`.

Worktrees can be managed by hand (`git worktree add`), but a small helper makes it
painless. We recommend [`ggw`](https://github.com/illegalstudio/ggw), which
creates, navigates, and pushes worktree-backed branches for you:

```bash
ggw create feat/my-feature      # create the branch and its worktree
ggw cd feat/my-feature          # switch into the worktree
# ... implement your change ...
git commit -m "feat: add my feature"
ggw push                        # push the branch and set its upstream
```

`ggw push` is equivalent to `git push -u <remote> feat/my-feature` — use whichever
you prefer. Once the branch is pushed, open your Pull Request as described below.

## Pull Requests

Before opening a Pull Request, please ensure that:

* Your code builds successfully.
* Existing tests continue to pass.
* New functionality includes tests whenever possible.
* Documentation is updated when appropriate.
* Commits are reasonably organized and have meaningful commit messages.
* When the Pull Request addresses an existing issue, reference it in the description.

Please keep Pull Requests focused and self-contained. A Pull Request that solves a single, well-defined problem is far easier to review than a large one that bundles several unrelated changes together.

### Pull Request labels

Pull Requests are classified automatically by the `PR labels` workflow. Its
catalog and matching rules live in `.github/pr-labels.json`, and the classifier
is implemented in `.github/scripts/pr-labels.cjs`.

The workflow runs when a Pull Request is opened, reopened, updated, edited, or
marked ready for review. On each run it recalculates the managed labels and
removes managed labels that no longer match. Contributors therefore do not need
label-management permissions and should not add or remove labels in the
following namespaces by hand:

| Namespace | Meaning | Values |
|---|---|---|
| `type:*` | The intent of the change. Exactly one is assigned from the conventional PR title, falling back to the branch prefix. | `type:feature`, `type:fix`, `type:docs`, `type:refactor`, `type:chore`, `type:test`, `type:triage` |
| `area:*` | The primary compiler or repository components affected by the changed paths. Up to three are assigned. | `area:lexer`, `area:parser`, `area:resolver`, `area:types`, `area:optimizer`, `area:eir`, `area:codegen`, `area:runtime`, `area:builtins`, `area:web`, `area:magician`, `area:platform`, `area:tooling-ci`, `area:docs`, `area:triage` |
| `target:*` | A target explicitly affected by the title, branch, or changed paths. Target-neutral changes receive no target label. | `target:linux-x86_64`, `target:linux-aarch64`, `target:macos-aarch64`, `target:windows-x86_64`, `target:wasm32-wasi` |
| `size:*` | Review size derived from the number of changed files and total added plus deleted lines. Exactly one is assigned. | `size:xs`, `size:s`, `size:m`, `size:l`, `size:xl` |
| `scope:*` | Additional review attention for unusually broad changes. | `scope:multi-area` |

The recognized conventional prefixes map directly to type labels: `feat:` and
`feature:` become `type:feature`; `fix:` becomes `type:fix`; `docs:` becomes
`type:docs`; `refactor:` becomes `type:refactor`; `chore:` becomes `type:chore`;
and `test:` or `tests:` becomes `type:test`. A title or branch without a
recognized prefix receives `type:triage`. The title takes precedence over the
branch name, so correcting the title is normally enough to correct the type.

Size labels use the first matching threshold below. Either the file count or
the changed-line count can move a Pull Request into the larger category.

| Label | Threshold |
|---|---|
| `size:xl` | More than 100 files or 10,000 changed lines |
| `size:l` | More than 30 files or 2,000 changed lines |
| `size:m` | More than 10 files or 500 changed lines |
| `size:s` | More than 3 files or 100 changed lines |
| `size:xs` | Everything smaller |

A `target:*` label describes target-specific content; it does not add that
target to the supported-target matrix. The supported-target policy in
`AGENTS.md` remains authoritative.

Some labels require maintainer judgment and are deliberately preserved by the
automation:

- `topic:*` describes cross-cutting semantics. The available labels are
  `topic:php-compat`, `topic:ownership-gc`, `topic:performance`, `topic:abi`,
  `topic:arrays`, `topic:strings`, `topic:closures`, `topic:generators`,
  `topic:fibers`, `topic:regex`, `topic:magic-methods`, `topic:control-flow`,
  `topic:numeric-literals`, `topic:json`, and `topic:errors`.
- `priority:high` is assigned only by maintainers.

The repository-wide labels `bug`, `enhancement`, `duplicate`, `good first
issue`, `help wanted`, `invalid`, `question`, and `wontfix` are reserved for
issue triage. GitHub exposes the same label catalog to issues and Pull Requests,
but these labels should not be used in place of the PR `type:*` labels.

If an automatic label looks wrong, first check the conventional title and the
changed paths. Explain any remaining mismatch in the Pull Request instead of
creating a near-duplicate label; a maintainer can assign an appropriate
`topic:*` label or adjust the classifier rules. Changes to the label catalog or
classification behavior must update `.github/pr-labels.json` and the focused
tests, which can be run with:

```bash
node --test .github/scripts/pr-labels.test.cjs
```

### Draft until it's ready

Open your Pull Request as a **draft** while you are still iterating, and keep it
in draft until you are confident the implementation is complete and correct.

Once you switch it to **ready for review**, please leave the Pull Request
untouched — do not push further changes, and do not rebase or merge `main` into it
to keep it aligned. From that point on the maintainers take over the Pull Request
and will handle reviewing, updating, and integrating it.

## Coding Style

Try to follow the style already used throughout the codebase.

Consistency is generally more important than personal preference.

### Assembly comment alignment

The assembly elephc emits is meant to be read and understood by someone learning
how compilers work, so **every `emitter.instruction(...)` call must carry an inline
`//` comment** explaining what the instruction does — and those comments are aligned
to a fixed column. A few rules keep them consistent:

1. **Every instruction line gets a comment.** No exceptions: if you add an
   `emitter.instruction(...)`, it gets a `// comment`.
2. **The `//` starts at column 81.** Pad the line with spaces so the `//` sits at the
   81st character (1-indexed). If the code itself already reaches 80 characters or
   more, put exactly one space before the `//`.
3. **Group related instructions under a block comment.** Put a standalone
   `// -- description --` line before a block of related instructions (e.g.
   `// -- set up stack frame --`).
4. **Explain intent, not the mnemonic.** Write "store argc from OS", not "store x0 to
   memory" — the reader can already see the instruction; the comment should say *why*
   it's there.

For example:

```rust
    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #32");                                     // allocate 32 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // set new frame pointer
```

To verify alignment, run `./scripts/check_asm_comments.py` against any codegen file you
touch before opening a Pull Request. It flags every `emitter.instruction(...)` whose
`//` comment is misaligned and exits non-zero if it finds any, so it also works in a
pre-commit hook or CI:

```bash
./scripts/check_asm_comments.py src/codegen/lower_inst/your_file.rs
```

It accepts multiple files at once, e.g. `./scripts/check_asm_comments.py src/codegen/lower_inst/*.rs`.

## Adding a new operator

elephc parses expressions with a Pratt parser, so a new binary operator flows
through the whole pipeline — lexer, parser, type checker, optimizer, EIR
lowering, and target-aware codegen. Implement it end-to-end:

1. Add the token to `src/lexer/token.rs`.
2. Add scanning logic to `src/lexer/scan.rs`.
3. Add the `BinOp` variant to `src/parser/ast.rs`.
4. Add one line to `infix_bp()` in `src/parser/expr/pratt.rs` (the Pratt parser
   binding-power table) so precedence and associativity match PHP.
5. Add type checking/inference in the relevant `src/types/checker/` file, usually
   under `inference/ops.rs` or expression inference.
6. Add optimizer/effect handling when the operator can be folded, propagated,
   pruned, or has side effects (`src/optimize/`). Keep folds PHP-equivalent —
   cross-check edge cases with `php -r`.
7. Add EIR lowering in the relevant `src/ir_lower/expr/` path and target-aware EIR
   codegen under `src/codegen/lower_inst/` when the operator needs a new IR
   instruction or lowering path.
8. Add tests in all four test files (lexer, parser, codegen, error), including a
   Pratt binding-power test that asserts precedence relative to adjacent operators.

## Adding a new statement type

A new statement kind must be threaded through parsing, the frontend passes, the
type checker, the optimizer, and EIR lowering. Missing a pass usually produces
silent miscompilation rather than a compile error, so also audit every
AST-walking pass (see "Adding or changing an AST node" in `CLAUDE.md`):

1. Add the `StmtKind` variant to `src/parser/ast.rs`.
2. Add parser logic in `src/parser/stmt.rs`.
3. Add resolver/name-resolver handling if the statement can contain names,
   declarations, includes, function variants, or expressions.
4. Add type checking in the relevant `src/types/checker/` module.
5. Add optimizer/effects/warnings handling if the statement can be folded, pruned,
   read variables, write variables, or alter control flow (`src/optimize/`).
6. Add EIR lowering in `src/ir_lower/stmt/` and target-aware EIR codegen under
   `src/codegen/` when the statement needs new instruction or terminator
   support.
7. If it introduces variables or hidden temporaries, update EIR local/temp
   declaration in `src/ir_lower/context.rs` and any frame-layout allocation needed
   before frame sizing.
8. Add tests: at least one codegen test showing correct output, one for edge cases
   (empty body, nested), and one error test for malformed syntax.

## Adding a new EIR optimization pass

IR-level (EIR) transformations run after EIR lowering/validation through a
fixed-point driver, not in the AST optimizer (`src/optimize/`). Folds that need
value identity, basic blocks, or dominance belong here.

1. Implement the `IrPass` trait (`name()`, `run(&mut Function, &mut DataPool) ->
   bool`) in a new `src/ir_passes/<pass>.rs`; `run` mutates the function in place
   and returns whether it changed anything. The `DataPool` is the module's shared
   literal pool for passes that intern new constants (e.g. peephole string-literal
   concat folding); ignore it (`_data`) otherwise.
2. Register the pass in `default_passes()` in `src/ir_passes/driver.rs`. Order
   matters: the driver re-runs the whole set per function until none reports a
   change, capped by `MAX_PASS_ITERATIONS`.
3. Reuse `src/ir_passes/rewrite.rs` for value redirection (`replace_all_uses` for
   RAUW) and the shared fold helpers (`resolve_chains`, `neutralize_to_nop`,
   `defining_instruction`, `count_value_uses`) instead of re-walking
   operands/terminators. Keep rewrites dominance-safe and PHP-equivalent;
   cross-check edge cases (division by zero, signed-zero/`NaN` floats) with `php -r`.
4. The driver re-validates each function with `validate_function` after every pass
   in debug/test builds and panics (naming the pass) on malformed IR or
   non-convergence; both guards compile out of `--release`. Rely on this during
   development.
5. Add unit tests under `src/ir_passes/tests/` (hand-built EIR via
   `crate::ir::Builder`) and end-to-end tests under `tests/codegen/optimizer/`. In
   e2e fixtures, use runtime-unknown values (e.g. `$argc`) so the targeted IR
   construct survives AST-level folding and actually reaches EIR.
6. Passes are gated by `--ir-opt=on|off` / `--no-ir-opt` (env `ELEPHC_IR_OPT`),
   default on. Behavior must be identical with the flag on or off except for
   performance; verify with `--emit-ir` and `--emit-ir --no-ir-opt`.

## Adding a built-in function

elephc's PHP built-in functions are declared **once** in a single-source registry.
Each builtin has one *home file* at `src/builtins/<area>/<name>.rs` that declares it
with the `builtin!` macro; all declarations are collected at link time through the
`inventory` crate. From that single declaration the compiler derives the catalog
name-set (case-insensitive lookup, `function_exists`, namespace fallback,
redeclaration checks), the call signature (named arguments, defaults, by-ref params,
variadic, arity), the shared semantic contract, backend-neutral EIR lowering, and the
generated documentation.

Do **not** re-add builtin names to the old hand-maintained tables (`catalog.rs`,
`signatures.rs`, per-area `check_builtin` arms). They are superseded by the registry;
a builtin is fully wired the moment its home file compiles.

### 1. Create the home file

Add `src/builtins/<area>/<name>.rs` and register it in `src/builtins/<area>/mod.rs`
with `pub mod <name>;` (keep the list alphabetical). Areas are `string`, `array`,
`math`, `io`, `system`, `types`, `callables`, `spl`, `pointers` (plus `internal` for
compiler-internal builtins). One builtin per home file; the file owns its declaration,
optional checker hook, and complete backend-neutral semantic descriptor. Start with
the mandatory `//!` module preamble.

### 2. Declare it with `builtin!`

```rust
builtin! {
    name: "strlen",
    area: String,
    params: [string: Str],
    returns: Int,
    semantics: BuiltinSemantics {
        validation: BuiltinValidation::Shared(validate),
        result_type: BuiltinResultType::Declared,
        effects: BuiltinEffects::Shared(effects),
        result_ownership: BuiltinResultOwnership::NonHeap,
        requirements: BuiltinRequirements::Static(&[]),
        target_strategy: BuiltinTargetStrategy::EirGraph,
        target_support: BuiltinTargetSupport::All,
        runtime_functions: BuiltinRuntimeFunctions::None,
        argument_lowering: BuiltinArgumentLowering::Standard,
        callable: BuiltinCallablePolicy::Dynamic(callable_accepts_strlen_source),
        lowering: BuiltinLowering::Eir(lower),
    },
    summary: "Returns the length of a string.",
    php_manual: "function.strlen",
}
```

Fields must appear in this canonical order; optional fields (marked `?`) may be
omitted:

`name`, `area`, `params`, `variadic?`, `min_args?`, `max_args?`, `arity_error?`,
`returns`, `by_ref_return?`, `check?`, `lazy_check?`, `semantics`, `requirements?`,
`summary`, `examples?`, `php_manual?`, `deprecation?`, `extension?`, `internal?`.

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
- **`semantics`** — the complete shared contract for validation, result typing,
  effects, ownership/aliasing, runtime/link requirements, target strategy/support,
  typed runtime-function inventory, argument lowering, callable availability, and
  backend-neutral EIR lowering. For the common case use
  `runtime_fn_semantics(RuntimeFnId::...)`; for type predicates use
  `type_predicate_semantics(...)`; use a custom `BuiltinSemantics` only when the
  builtin composes EIR primitives or needs source/type-dependent contracts.
- **`requirements`** — an optional source-dependent resolver layered over the
  descriptor's fixed requirements. Do not rediscover bridges or runtime features from
  PHP names later in the pipeline.
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
- **`extension: true`** — an elephc extension with no PHP equivalent (`ptr_*`,
  `zval_*`, `buffer_*`, `class_attribute_*`, …). `--strict-php` hides it from user
  programs; update `EXPECTED_EXTENSION_BUILTINS` in `src/builtins/parity_tests.rs`.
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

The macro embeds a `check` hook into `semantics.validation` and changes a declared
result contract to `BuiltinResultType::Checked` when needed. EIR lowering consumes the
checked call-site type from that same descriptor. If the backend returns a different
storage representation from the checker-facing type (for example, a boxed dynamic
result), set `BuiltinResultType::Shared(resolve)` and make the resolver return the
actual EIR/backend representation. Do not add PHP-name arms to `call_return_type`.

### 4. Backend-neutral EIR lowering

The descriptor's `lowering` field is mandatory. Most builtins lower to one typed
runtime operation:

```rust
semantics: runtime_fn_semantics(crate::ir::RuntimeFnId::ArrayMap),
```

The backend receives `Op::RuntimeCall` with a typed `RuntimeCallTarget`; PHP names are
not present at that boundary. Add the target to `RuntimeFnId` (or another typed target
enum), implement it under `src/codegen/lower_inst/runtime_functions/` or
`runtime_calls.rs`, and keep every supported ABI in the same path. If it needs a
runtime routine, add it under `src/codegen_support/runtime/<category>/`.

For a builtin that is naturally expressed as reusable EIR operations, use
`BuiltinLowering::Eir(lower)`. The hook receives only `BuiltinLoweringContext` and a
`NormalizedBuiltinCall`; it may emit typed EIR values/runtime calls, but must not
import `crate::codegen`, mention physical registers, choose concrete helper symbols,
or emit assembly. `BuiltinLowering::TypePredicate` is the shared primitive for PHP
type predicates.

### 5. What derives automatically

Once the home file compiles, all of the following see the builtin with no further
edits: `function_exists()` and case-insensitive/namespaced lookup, the named-argument
`FunctionSig`, checker validation/result typing, optimizer effects, ownership cleanup,
runtime/link requirements, direct and runtime-selected callable policy,
backend-neutral EIR lowering, the arity check and its error message, and the
`gen_builtins` JSON docs export.

### 6. Surfaces you still wire by hand

The registry single-sources the semantic compiler contract. These implementation and
user-facing surfaces still need updates when relevant:

- **The typed backend target** and any runtime routine it needs. Every target must
  validate the operand/result contract and support macOS ARM64, Linux ARM64, and Linux
  x86_64.
- **The descriptor itself** when effects, ownership, requirements, argument lowering,
  or runtime-callable availability differ from the selected target's defaults. Never
  mark a call pure if it can read/write globals, files, the environment, heap state,
  argument storage, or emit output.
- **Examples and generated docs** for the PHP-visible surface.

### 7. Tests, examples, and docs

- Add codegen tests for normal use (plus at least one case-insensitive or namespaced
  call for a PHP-visible builtin), and error tests for wrong argument count/types.
- Add or update an example under `examples/` when the builtin is a notable user-facing
  feature.
- Document the PHP surface (signature, parameters, return type, a short example) on the
  relevant `docs/php/` page.
- The signature/arity parity gates in `src/builtins/parity_tests.rs` must stay green.

### 8. Not every "builtin" is a function

A small set of PHP language constructs — `isset`, `unset`, `empty`, `exit`, `die` — are
l-value/lazy constructs with dedicated EIR paths and are intentionally kept in the
checker (`numeric`/`arrays` `check_builtin`), not in the registry. Do not migrate those
into `builtin!`. `buffer_new` is similar (its call form is dedicated syntax lowered as
`ExprKind::BufferNew`; only its name lives in the catalog), while `buffer_len` and
`buffer_free` are ordinary registry builtins under `src/builtins/pointers/`.

Builtins that are elephc extensions with no PHP equivalent must declare
`extension: true` in `builtin!` so `--strict-php` hides them from user programs; the
pinned set lives in `src/builtins/parity_tests.rs` (`EXPECTED_EXTENSION_BUILTINS`).
Injected compiler preludes must never call a PHP-visible extension builtin — use an
`internal: true` `__elephc_*` alias instead (see `src/builtins/pointers/__elephc_ptr_read_string.rs`);
the `preludes_never_call_php_visible_extension_builtins` gate enforces this.

On the eval side, magician derives its extension set from `EvalArea::RawMemory`
plus the `SYMBOLS_EXTENSION_BUILTINS` list (`crates/elephc-magician/src/interpreter/builtins/spec.rs`)
instead of a per-declaration flag; the `extension_builtin_sets_agree_across_registries`
gate in `tests/builtin_parity_tests.rs` pins that derivation against the compiler
registry, so adding an extension builtin to either registry forces both sides to agree.

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
  builtin in `src/builtins/<area>/` with its semantic descriptor), and declare
  `BuiltinRequirement::Bridge("elephc_<name>")` in its fixed requirements or return
  it from the descriptor's source-dependent `requirements` resolver. The PHP names
  are always available, so no prelude is needed.

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
