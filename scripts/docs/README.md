# Elephc builtins documentation generator

This directory holds a small Python pipeline that turns the Elephc source
tree into one Markdown page per supported PHP builtin, in two flavours:

- **User reference** (`docs/php/builtins/<name>.md`) — what each function
  does, its signature, return type, parameters, and pointers to the
  matching PHP manual entry.
- **Compiler internals** (`docs/internals/builtins/<name>.md`) — which
  source file lowers the call, which runtime helper it dispatches to,
  what the type checker enforces.

The script is data-driven. Its source of truth is the single-source
`builtin!` registry (`src/builtins/`), read via the `gen_builtins` binary
(`cargo run --bin gen_builtins --include-internal`). It enriches that data
with each builtin's lowering location (parsed from the home file's `lower`
hook) and documentation area, then writes a single JSON registry
(`scripts/docs/builtin_registry.json`) which the Markdown renderer consumes.
Everything else is generated.

## Usage

From the repo root. The generator invokes the `gen_builtins` binary, so build
it first (the extractor prefers the prebuilt binary at `target/debug/gen_builtins`
and otherwise falls back to `cargo run`):

```bash
# 0. Build the registry exporter the generator reads from
cargo build --bin gen_builtins

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
| Registry | `gen_builtins` binary (reads `src/builtins/`) | The authoritative set of builtins (incl. `internal` helpers) with exact signatures: parameter names, types, defaults, by-ref flags, variadics, and return types. |
| Lowering | Home files `src/builtins/<area>/<name>.rs` + `src/codegen_ir/lower_inst/builtins/` | Each home's `lower` hook names the emitter it dispatches to; we resolve that emitter's file, line, `__rt_*` runtime helpers, and leading `///` doc comment. |
| Precision | `elephc_builtins/registry.py` | Presentation refinements the registry represents coarsely as `Mixed`: `PARAM_TYPES` (param display types) and `RETURN_TYPE_OVERRIDES`. Return types are also recovered from a home's `check` hook when possible. |

The registry represents non-scalar params/returns as `Mixed`; the generator
recovers array/typed returns from the home file's `check` hook and applies
`PARAM_TYPES` for param display types. Builtins whose emitter cannot be
resolved are still emitted, but the internals page notes that no dedicated
lowering was found.

The 8 PHP language constructs that stay checker-resident
(`isset`/`unset`/`empty`/`exit`/`die`/`buffer_len`/`buffer_free`/`buffer_new`)
are not in the registry; they are added from a hand-curated table
(`LANGUAGE_CONSTRUCTS`) in `extract.py`.

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
├── php/builtins/                      # user-facing reference
│   ├── README.md                      # master index (all builtins)
│   ├── array.md  string.md  math.md   # one top-level area index per area
│   ├── array/                          # per-area folder of builtin pages
│   │   ├── array_chunk.md
│   │   ├── array_map.md
│   │   └── …
│   ├── string/
│   ├── math/
│   └── …
└── internals/builtins/                # compiler internals (same shape)
    ├── array/
    │   └── array_map.md
    └── …
```

Every builtin lives in a subfolder that matches its area. The internal
`__elephc_*` helpers live under `_internal/`.

## Known limitations

- **Parameter names, defaults, by-ref flags, variadic shape, and arity are
  exact** — they come straight from the `builtin!` registry, so they match
  Elephc's actual supported surface (which is sometimes smaller than PHP's,
  e.g. fewer optional parameters). **Non-scalar types are coarse**: the
  registry declares arrays/callables/unions as `Mixed`. The generator
  recovers array/typed *return* types from each home's `check` hook, and
  refines *param* display types via `PARAM_TYPES` in `registry.py`; where
  neither applies, a non-scalar shows as `mixed`.
- **A few builtins have no captured lowering.** When a home's `lower` hook
  cannot be resolved to an emitter definition, the internals page notes that
  no dedicated lowering was found.
- **Areas are inferred from the dispatch module** in `builtins.rs` and the
  file path of the lowering function. Hand-curated overrides live in
  `elephc_builtins/registry.py` (`AREA_BY_NAME`, `AREA_BY_LOWERING_FN`,
  `AREA_BY_MODULE`) for the cases the heuristic gets wrong.
- **One-line descriptions** come from the lowering function's `///` doc
  comment or from `DESCRIPTION_OVERRIDES`. Many builtins still use the
  generic stub sentence.
