# Elephc builtins documentation generator

This directory holds a small Python pipeline that turns the Elephc source
tree into one Markdown page per supported PHP builtin, in two flavours:

- **User reference** (`docs/php/builtins/<area>/<name>.md`) — what each function
  does, its signature, return type, parameters, and pointers to the
  matching PHP manual entry.
- **Compiler internals** (`docs/internals/builtins/<area>/<name>.md`) — which
  source file lowers the call, which runtime helper it dispatches to,
  what the type checker enforces.

The script is data-driven. Its source of truth is the dependency-neutral shared
contract (`crates/elephc-builtin-contract`), joined to the compiler's `builtin!`
bindings and Magician's `eval_builtin!` bindings by the `gen_builtins` example
(`cargo run --example gen_builtins -- --include-internal`). It enriches that data
with each builtin's lowering location (parsed from the home file's `lower`
hook) and documentation area, then writes a single JSON registry
(`scripts/docs/builtin_registry.json`) which the Markdown renderer consumes.
Everything else is generated.

## Usage

From the repo root. The generator invokes the `gen_builtins` example, so build
it first (the extractor prefers the prebuilt binary at
`target/debug/examples/gen_builtins` and otherwise falls back to `cargo run`):

```bash
# 0. Build the registry exporter the generator reads from
cargo build --example gen_builtins

# 1. Parse the source and write the JSON registry
python3 scripts/docs/extract_builtins.py

# 2. (optional) Render the Markdown pages on top of an existing tree
python3 scripts/docs/extract_builtins.py --render

# 3. Force overwrite of any hand-written pages
python3 scripts/docs/extract_builtins.py --render --force
```

By default, hand-written pages are preserved — only stubs (i.e. pages that
the script itself wrote) are overwritten. Use `--force` to overwrite
everything.

## What the script reads

| Layer | Source | What we extract |
|---|---|---|
| Contract | `gen_builtins` binary (reads `elephc-builtin-contract`) | The authoritative catalog, canonical signatures, visibility, support routes, and explicit per-backend signature profiles. |
| Bindings | `src/builtins/` and `crates/elephc-magician/src/interpreter/builtins/` | AOT semantic descriptors, Magician dispatch metadata, and concrete backend availability. |
| Lowering | Home files `src/builtins/<area>/<name>.rs` + `src/codegen/lower_inst/builtins/` | Each home's `lower` hook names the emitter it dispatches to; we resolve that emitter's file, line, `__rt_*` runtime helpers, and leading `///` doc comment. |
| Precision | `elephc_builtins/registry.py` | Presentation refinements the registry represents coarsely as `Mixed`: `PARAM_TYPES` (param display types) and `RETURN_TYPE_OVERRIDES`. Return types are also recovered from a home's `check` hook when possible. |

The shared contract represents some non-scalar params/returns as `Mixed`; the generator
recovers array/typed returns from the home file's `check` hook and applies
`PARAM_TYPES` for param display types. Builtins whose emitter cannot be
resolved are still emitted, but the internals page notes that no dedicated
lowering was found.

The five PHP language constructs plus dedicated `buffer_new`, the four injected
hash-prelude functions, and the four intentionally eval-only reflection functions
are emitted directly from the same contract. Python does not reconstruct their
signatures or backend availability.

## Layout

```
scripts/docs/
├── README.md                  # this file
├── extract_builtins.py        # CLI entry point
├── builtin_registry.json      # generated — do not edit by hand
└── elephc_builtins/           # Python package
    ├── extract.py             # reads gen_builtins + resolves lowering
    ├── render.py              # emits Markdown
    └── registry.py            # data model + area maps + precision tables
```

## Output tree

```
docs/
├── php/
│   ├── builtins.md                    # master index (all builtins)
│   └── builtins/                      # user-facing reference
│       ├── array.md string.md math.md  # one top-level area index per area
│       ├── array/                     # per-area folder of builtin pages
│       │   ├── array_chunk.md
│       │   ├── array_map.md
│       │   └── …
│       ├── string/
│       ├── math/
│       └── …
└── internals/builtins/                # compiler internals (same shape)
    ├── array/
    │   └── array_map.md
    └── …
```

Every builtin lives in a subfolder that matches its area. The internal
`__elephc_*` helpers live under `_internal/`.

## Known limitations

- **Parameter names, defaults, by-ref flags, variadic shape, and arity are
  exact** — they come from the shared contract and its explicit AOT/eval
  signature profiles, so backend differences such as the narrower compiled
  `hash_init()` prelude signature are visible. **Non-scalar types are coarse**:
  some contracts declare arrays/callables/unions as `Mixed`. The generator
  recovers array/typed *return* types from each home's `check` hook, and
  refines *param* display types via `PARAM_TYPES` in `registry.py`; where
  neither applies, a non-scalar shows as `mixed`.
- **A few builtins have no captured lowering.** When a home's `lower` hook
  cannot be resolved to an emitter definition, the internals page notes that
  no dedicated lowering was found.
- **Areas start from the shared contract** and use validated presentation-only
  refinements in `elephc_builtins/registry.py` where the user-facing docs need a
  narrower family such as Regex, Hash, or Process.
- **One-line descriptions** come from the lowering function's `///` doc
  comment or from `DESCRIPTION_OVERRIDES`. Many builtins still use the
  generic stub sentence.

## PHP comparison page

`docs/php/compatibility.md` is generated by `gen_php_comparison.py` from four
sources:

| Source | Kind | Regenerated by |
|---|---|---|
| `builtin_registry.json` | elephc functions, each with the PHP module and first PHP version its shared contract declares | `extract_builtins.py` (see above) |
| `symbol_registry.json` | elephc classes and global constants from the shared class and constant catalogs (`gen_builtins --symbols`) | `extract_builtins.py` |
| `php_baseline.json` | vendored snapshot of real PHP: functions, classes, and constants per module, with constant values | `extract_php_baseline.py` (run only to bump the pinned baseline) |
| `comparison_catalog.toml` | hand-curated language constructs, extensions, limitations | edited by hand |

Every elephc symbol carries its module in the shared contract
(`crates/elephc-builtin-contract`), so the page groups by that module and the
baseline serves only as denominator and cross-check: a symbol whose module or
existence disagrees with PHP, or a constant whose value differs without an entry
in `KNOWN_VALUE_DIVERGENCES`, fails generation. The page does not distinguish how
elephc implements a symbol (registry builtin, injected prelude, name-resolver
rewrite): one count per module and kind, with the compiled-vs-`eval()` split
listed underneath where it differs.

The baseline snapshot is taken from a PHP build that loads every extension
php-src bundles; `php_baseline/Dockerfile` builds that image:

```bash
docker build -t elephc-php-baseline scripts/docs/php_baseline
printf '#!/bin/sh\nexec docker run --rm -i elephc-php-baseline php "$@"\n' > /tmp/php && chmod +x /tmp/php
python3 scripts/docs/extract_php_baseline.py --php /tmp/php
```

The extractor keeps only extensions bundled with php-src (its `BUNDLED_EXTENSIONS` allowlist), so PECL/third-party modules on the local machine can never enter the snapshot, whatever they are named. Bundled extensions the local PHP does not load are recorded in the snapshot's `missing_bundled` field and disclosed on the generated page.

```bash
python3 scripts/docs/gen_php_comparison.py       # regenerate the page
python3 -m unittest discover -s scripts/docs -p "test_*.py"   # generator unit tests
```

Catalog entries with `status = "supported"` or `"partial"` must carry
`evidence` — a repo path or a test function name found as `fn <name>` under
`tests/` — and generation fails if the evidence is missing or dangling. A
symbol whose contract says PHP has it but the baseline does not also fails
generation: fix the contract's module or `since`, mark it `extension`/`internal`,
or refresh the baseline.

CI's `builtins-docs-sync` job regenerates the page and fails when the
committed copy differs. Maintenance flows:

- **New builtin, class, or constant:** add its shared contract (with module and
  `since`), regenerate builtins docs, then the comparison page.
- **New language feature:** add a catalog entry with evidence, regenerate.
- **PHP version bump:** rebuild the baseline image for the new PHP, run
  `extract_php_baseline.py`, regenerate, commit both.
