# Elephc builtins documentation generator

This directory holds a small Python pipeline that turns the Elephc source
tree into one Markdown page per supported PHP builtin, in two flavours:

- **User reference** (`docs/php/builtins/<name>.md`) — what each function
  does, its signature, return type, parameters, and pointers to the
  matching PHP manual entry.
- **Compiler internals** (`docs/internals/builtins/<name>.md`) — which
  source file lowers the call, which runtime helper it dispatches to,
  what the type checker enforces.

The script is data-driven: it parses three layers of the Rust source to
build a single JSON registry (`scripts/docs/builtin_registry.json`) which
the Markdown renderer consumes. The registry is the canonical source of
truth; everything else is generated.

## Usage

From the repo root:

```bash
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

| Layer | File | What we extract |
|---|---|---|
| Catalog | `src/types/checker/builtins/catalog.rs` | The authoritative list of PHP-visible builtins. |
| Signatures | `src/types/signatures.rs` | Per-builtin parameter names, defaults, variadics, by-ref flags, and first-class return types. |
| Lowering | `src/codegen_ir/lower_inst/builtins.rs` + per-area submodules | For each builtin: the lowering function, its location, runtime helpers it calls, and the leading `///` doc comment. |

The renderer currently reads the dispatch table in `builtins.rs` (root)
plus the `lower_*` function definitions in the submodule files. When a
builtin has no dedicated dispatch arm (e.g. it is handled by a multi-name
catch-all), the renderer falls back to a `lower_<name>` heuristic on the
root file. Builtins that cannot be mapped to a lowering are still emitted,
but the internals page will note that no dedicated lowering was found.

## Layout

```
scripts/docs/
├── README.md                  # this file
├── extract_builtins.py        # CLI entry point
├── builtin_registry.json      # generated — do not edit by hand
└── elephc_builtins/           # Python package
    ├── extract.py             # parses .rs files
    ├── render.py              # emits Markdown
    └── registry.py            # data model
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

Every builtin lives in a subfolder that matches its area. The 3 internal
`__elephc_*` helpers live under `_internal/`.

## Known limitations

- **Signature precision depends on `src/types/signatures.rs` and several
  hand-curated tables in `registry.py`.** Parameter names, types, return
  types, by-ref flags, optional defaults, and variadic shape are refined
  through `PARAM_TYPES`, `PARAM_NAME_OVERRIDES`, `PARAM_TYPE_OVERRIDES`,
  `REF_PARAM_OVERRIDES`, `OPTIONAL_PARAM_OVERRIDES`, `RETURN_TYPE_OVERRIDES`,
  and `VARIADIC_OVERRIDES`. The generator also reads
  `first_class_callable_builtin_sig()` and `check_builtin()` arms for
  additional precision. A few builtins still differ from PHP because Elephc
  intentionally supports a smaller surface (e.g. fewer optional parameters).
- **About 48 catalog builtins have no captured lowering.** These are
  usually handled by multi-name catch-all dispatchers (e.g. libm unary
  functions) or by special compiler paths that the heuristic does not
  yet recognize. Their internals page will show `(not lowered)`.
- **Areas are inferred from the dispatch module** in `builtins.rs` and the
  file path of the lowering function. Hand-curated overrides live in
  `elephc_builtins/registry.py` (`AREA_BY_NAME`, `AREA_BY_LOWERING_FN`,
  `AREA_BY_MODULE`) for the cases the heuristic gets wrong.
- **One-line descriptions** come from the lowering function's `///` doc
  comment or from `DESCRIPTION_OVERRIDES`. Many builtins still use the
  generic stub sentence.
