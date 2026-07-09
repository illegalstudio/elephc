# Bidirectional PHP Extension / Library Bridge

**Goal:** Achieve full bidirectional interoperability between elephc-compiled code and real PHP C extensions / the Zend Engine.

- **Consumer direction**: elephc programs (and libraries) can call functions provided by existing PHP extensions (mbstring, curl, hash, json, etc.).
- **Producer direction**: elephc can emit loadable PHP extensions (Zend modules) so that ordinary PHP scripts can call elephc-compiled functions directly, without going through a separate C FFI layer.

This enables both "use PHP libraries from elephc" and "ship elephc code as PHP libraries".

## Current State (at time of plan creation)

The following items are already implemented and landed:

## Task Checklist

- [x] zval_pack / zval_unpack / zval_type / zval_free compiler builtins (scalars, strings, packed arrays, hash/associative arrays, nested structures)
- [x] Full cross-target runtime implementation (macOS aarch64, Linux aarch64, Linux x86_64) with deep copies into zend_string / zend_array / HashTable / Bucket and recursive free
- [x] Type checker, signatures (including first-class callable), catalog registration, and EIR lowering for the zval bridge builtins
- [x] Comprehensive codegen tests (round-trips, type bytes, content verification for arrays, error cases for wrong arg count/types)
- [x] Heap-debug leak regression tests for zval_free and the bridge itself
- [x] Example (`examples/zval-pack/`) and documentation (`docs/beyond-php/zval-bridge.md`)
- [x] `--emit cdylib` (and aliases) producing position-independent shared libraries (`.so`/`.dylib`)
- [x] `#[Export]` / `#[Elephc\Export]` attribute to select functions for export in cdylib mode
- [x] Basic C-ABI marshaling for cdylibs (int, float, bool, string parameters as `(ptr, len)` pair)
- [x] Lifecycle symbols (`elephc_init`, `elephc_shutdown`, `elephc_last_error`, `elephc_free`) and symbol visibility rules for cdylibs
- [ ] Consumer: support for linking against PHP extension shared libraries at compile time
- [ ] Consumer: practical way to declare and invoke functions from PHP extensions (using zval* via `ptr` or higher-level helpers)
- [ ] Consumer: correct ownership/refcount hand-off when passing zval values into PHP extension code
- [ ] Consumer PoC: successfully call at least one real function from a common PHP extension (e.g. a hash or mbstring function) from elephc-compiled code
- [ ] Producer: new emit kind (e.g. `--emit phpext` / `--emit php-extension`) that produces a Zend-compliant loadable extension
- [ ] Producer: generation of `zend_module_entry` and the `get_module()` / `get_module_ptr` symbol required by the Zend engine
- [ ] Producer: per-exported-function wrapper code that receives zval** arguments from Zend, converts them to elephc values (via the bridge), invokes the compiled body, and converts the result back
- [ ] Producer: correct refcounting, temporary management, and error propagation for functions exported to PHP
- [ ] Producer: minimal module lifecycle (MINIT/RINIT or equivalent init hooks) and clean shutdown
- [ ] Example of a complete, loadable elephc-generated PHP extension (with at least scalar + string + array arguments/returns)
- [ ] Documentation and user guide for both directions (how to call PHP extensions, and how to build + load an elephc PHP extension)
- [ ] Bidirectional PoC / test: an elephc-produced PHP extension that itself uses the consumer bridge to call another PHP extension
- [ ] Update ROADMAP (v0.28 and beyond), CHANGELOG entries, and any related CLI / pipeline docs
- [ ] Test harness / CI coverage for produced extensions (loading + execution from PHP where the environment has PHP; otherwise focused compile+dlopen tests)
- [ ] All changes respect project rules (EIR-only paths, all supported targets in the same change or explicitly isolated, `//!` preambles, `///` docblocks, assembly `//` comments at column 81, one runtime emitter per leaf file, focused tests before landing, etc.)

---

## Background

### Two distinct but related problems

1. **C ABI libraries** (`--emit cdylib`)
   - Already shipped.
   - Produces plain C-callable shared libraries.
   - Useful from Rust, Go, Python ctypes, C, etc.
   - Not directly usable as a PHP extension (PHP's Zend Engine expects a specific module registration and zval-based calling convention).

2. **PHP / Zend extension bridge**
   - The zval bridge (landed in the feat/zval-bridge PR) gives elephc the ability to speak the memory layout that PHP extensions use internally (`zval`, `zend_string`, `zend_array`).
   - This is the *value representation* layer needed for both directions.

### Consumer (elephc calls PHP extensions)

PHP extensions are typically loaded by the Zend Engine. They register functions in the function table rather than exporting plain C symbols under their PHP names. To call them from a standalone elephc binary we need:

- The ability to link extra `.so`/`.dylib` files.
- A way to obtain or reconstruct the C entry points (or go through `zend_call_function` style APIs if embedding).
- Correct construction of `zval*` arguments using the primitives from this plan's foundation.
- Ownership transfer rules (who frees what).

Many extensions expose lower-level C functions; for a practical first PoC we can target those.

### Producer (PHP calls elephc code)

To appear as a normal PHP extension the artifact must:

- Export `get_module()` returning a `zend_module_entry*`.
- Register function entries that receive the classic Zend calling convention (`INTERNAL_FUNCTION_PARAMETERS` style or equivalent: `zval* return_value, zval* this_ptr, int return_value_used`, plus `zval** args` or `zval* args` + `int argc` in older styles).
- Convert incoming zval arguments into elephc-native values.
- Invoke the compiled function body (the same EIR/codegen path used for executables and cdylibs).
- Convert the result back into a zval that Zend owns.
- Participate in module/request lifecycle.

This is a new emit mode, not a small extension of the existing C cdylib path.

## High-Level Architecture

- Keep `--emit cdylib` as the **C ABI** track (for non-PHP hosts). Do not change its calling convention.
- Introduce a new emit kind (suggested: `phpext`, `zend`, or `php-extension`) for **Zend module** output.
- The zval bridge (pack/unpack) becomes the central conversion layer used by both the consumer call sites and the producer argument/return wrappers.
- New runtime code lives under `src/codegen/runtime/zend/` (or `phpext/`) following the existing one-emitter-per-leaf-file discipline.
- All new lowering goes through EIR (`src/ir_lower/`, `src/codegen_ir/`).
- Both consumer linking and producer module emission must work on all three supported targets.
- Ownership model must be explicit and tested with heap-debug mode (elephc side retains its own heap; zvals handed to Zend are independent deep copies or properly refcounted transfers).

### Emit mode matrix (target state)

| Mode          | ABI / calling convention     | Primary host          | Uses zval bridge? |
|---------------|------------------------------|-----------------------|-------------------|
| executable    | native (main)                | OS / direct exec      | No                |
| cdylib        | C ABI (`#[Export]`)          | C, Rust, Go, Python…  | Optionally (if the C code wants to use PHP exts) |
| phpext        | Zend module + zval handlers  | PHP (via `extension=`) | Yes (both directions) |

## Detailed Tasks

Each task should be landable with green tests. Follow TDD where practical (write a failing test first). Every change that affects generated assembly must have assembly comments at column 81 and pass `./scripts/check_asm_comments.py`.

### Task 1 — Consumer linking support (`--ext` / extra shared libs)

Add the ability to tell the compiler "link these additional shared objects at final link time". This is the mechanical prerequisite for pulling in PHP extension `.so`s.

- CLI: `--ext path/to/ext.so` (repeatable) or `--exts` file list. Store in `Emit` / build context.
- Pass the libraries through to the linker invocation (respect existing linker flags handling).
- Update docs and `--help`.
- Add a basic smoke test that compiles a program while requesting an extra (dummy) `.so` and verifies the binary links.

Files: `src/cli.rs`, `src/pipeline.rs` (or linker driver), `src/linker.rs`, tests under `tests/codegen/cli.rs` or a new integration test.

### Task 2 — Declaring and calling PHP extension symbols (consumer)

Provide a way for user code to call into a linked PHP extension.

Options (choose one primary path, document trade-offs):

- Low-level: allow `extern "C" { fn some_ext_func(zval_ptr: ptr, ...) -> ptr; }` combined with the existing zval bridge. User does manual `zval_pack` before the call and `zval_unpack` + `zval_free` after.
- Medium-level: a new attribute or builtin that describes a PHP function signature in terms of PHP types and generates the pack/unpack boilerplate.

Start with the low-level path (most flexible and lowest risk). Add sugar later if needed.

- Ensure `ptr` (already existing) is sufficient to hold `zval*`.
- Add effects modeling so the optimizer does not wrongly DCE or reorder calls into extensions.
- Tests: at minimum, compile-only checks + a runtime test that links a tiny hand-written C "extension-like" object exposing a zval-based function and round-trips values.

### Task 3 — Consumer PoC + ownership rules

Land a concrete, end-to-end PoC.

- Choose a small, widely available target (e.g. functions from the `hash` extension or a simple string function that is easy to reach via C symbol).
- Demonstrate:
  - Packing an elephc value
  - Calling the external function
  - Unpacking the result (or inspecting via `zval_type`)
  - Explicit `zval_free` on the caller's side
- Document and test the ownership contract: "the zval returned by `zval_pack` is owned by the caller; after passing it to an extension the extension may retain a refcount or take ownership — the bridge user must still free the top-level zval unless the extension docs say otherwise."

Add heap-debug tests that prove no leaks on the elephc side for the temporaries created during packing.

Update `docs/beyond-php/zval-bridge.md` with a "Calling PHP extensions" section.

### Task 4 — New emit kind for PHP extensions (producer)

Introduce `--emit phpext` (aliases: `php-extension`, `zendext`, `extension`).

- Wire the new variant in `cli.rs`, `Emit` enum, pipeline dispatch.
- In cdylib/phpext mode, top-level statements are still not executed at load (same as cdylib).
- Reject combinations that don't make sense (`--web` + phpext, etc.).
- Add a minimal "hello" emitter that produces a valid shared library containing the required symbols (even if the functions do nothing yet). The binary must be `dlopen`able without immediate crash.

Ensure PIC codegen path is reused / extended from cdylib work.

### Task 5 — Zend module entry emission

Emit the data structures required by the Zend engine:

- A `zend_module_entry` (or compatible layout) in a dedicated section or as global data.
- The `get_module()` (or `get_module_ptr`) function that returns its address.
- Module name, version, function table pointer, etc.
- Proper visibility (the module entry and get_module must be exported; internal elephc symbols remain hidden).

This is mostly data emission + a tiny assembly stub. Keep it in the EIR/codegen_ir world where possible (or use existing data emission helpers).

Add target-aware emission (different name mangling / leading underscore on macOS vs Linux).

### Task 6 — Inbound zval argument conversion for exported functions (producer)

When a function is exported under phpext mode:

- Generate (or lower) a Zend-compatible handler.
- The handler receives arguments in zval form from the engine.
- Use (or extend) the zval bridge to convert each argument to an elephc-native value (the reverse of what `zval_pack` does).
- Materialize the arguments in the way the existing compiled function body expects (same as how cdylib wrappers currently do for C ABI).
- After the body returns, convert the elephc return value back to a zval that the engine owns (using `zval_pack` logic or a dedicated "return zval" helper).

Handle arity, optional parameters, and basic type checking at the wrapper level (or rely on the existing signature info).

Special care for string and array returns (who allocates, who frees).

### Task 7 — Return values, refcounting, and error paths (producer)

- Implement return value boxing into the `return_value` zval slot that Zend provides.
- Define and implement the ownership rules for the producer side (elephc side should not leak; Zend side receives properly refcounted or newly allocated storage).
- Map elephc fatals / uncaught errors to something reasonable for the Zend environment (bailout or throwing a PHP exception). Start conservative (process-terminating fatal is acceptable for v1).
- Add tests that exercise argument conversion in both directions using a small test harness that mimics what the Zend engine does (or actually load under PHP when available).

### Task 8 — Module lifecycle and examples

- Provide init / shutdown hooks (MINIT, RSHUTDOWN, etc.) or at minimum wire the existing `elephc_init` / shutdown style so users can run setup code.
- Create `examples/php-extension/` (or `examples/zend-extension/`) containing a small `.php` that becomes a loadable extension, plus a `README.md` showing the compile + `php -d extension=...` steps.
- The example must demonstrate at least one scalar, one string, and one array round-trip from PHP → elephc → PHP.

Add a `.gitignore` for the example that ignores the produced `.so`/`.dylib` and any generated artifacts.

### Task 9 — Bidirectional demonstration

Build (and test where possible) a scenario that exercises both directions in one artifact:

- Compile a PHP extension with elephc.
- Inside one of its exported functions, use the consumer bridge to call a function from another PHP extension.
- Verify end-to-end from a PHP script that loads the produced extension.

This proves the zval bridge is sufficient for both roles and that the two tracks compose.

### Task 10 — Documentation, ROADMAP, and release artifacts

- Expand `docs/beyond-php/` with a new page (or substantial section) "Producing PHP Extensions".
- Update the zval-bridge page to cover the consumer use case.
- Update `docs/compiling/cli-reference.md` and output docs for the new emit kind.
- Mark the corresponding items in `ROADMAP.md` (move or add under v0.28 / v0.29 as appropriate).
- Add user-facing bullets to `CHANGELOG.md` when pieces land.
- Add a "how to" or internals note if the generated Zend structures are interesting for future work.

### Task 11 — Polish, constraints, and CI

- Ensure every new runtime emitter follows the single-file-per-emitter rule and has a proper `//!` preamble.
- All new assembly has column-81 comments and passes the checker script.
- Run focused tests on all three targets (use Docker scripts for Linux).
- Add ignored tests or environment-gated tests for actual PHP loading (`ELEPHC_TEST_WITH_PHP=1` or similar).
- Verify no regression in existing cdylib or executable paths.
- Zero warnings on `cargo build` / `cargo check`.
- Update any relevant Agents.md / internal notes if new patterns are established.

## Exit Criteria

- A user can compile a PHP program that calls a function from a real PHP extension and get correct results + clean heap.
- A user can compile a PHP program with `--emit phpext`, load the resulting `.so`/`.dylib` into PHP via `extension=`, and successfully call the exported functions from PHP code (scalars, strings, arrays at minimum).
- The same elephc artifact (or a different one) can do both in one process.
- Full test coverage, documentation, and ROADMAP updated.
- All changes land following the project's strict engineering rules (EIR, targets, tests, style).

## Non-Goals (for this plan)

- Full object / resource / reference bridging in the first cut.
- Automatic generation of PHP stub files (`.php` declarations) from the compiled extension (nice to have later).
- Thread-safety or re-entrancy guarantees beyond what the current runtime provides.
- Supporting every obscure Zend extension hook on day one.

## Open Questions (to resolve during implementation)

- Exact name of the new emit kind and attribute (reuse `#[Export]` with context, or new `#[PhpExport]`?).
- Whether we need a small embed-SAPI shim for the consumer direction or pure direct symbol linking is sufficient for the initial PoCs.
- How much of the Zend internal structures we want to hardcode vs. discover at build time (headers vs. our own definitions).

---

Leave this plan in the repository until the entire checklist is complete. Individual tasks may land in separate PRs as long as the suite stays green.