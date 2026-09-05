---
name: update-builtin-docs
description: Regenerate and audit Elephc's generated builtin documentation from the shared builtin contract plus builtin! and eval_builtin! backend bindings. Use when a change touches crates/elephc-builtin-contract, src/builtins, crates/elephc-magician/src/interpreter/builtins, builtin signatures, builtin lowering hooks, docs/php/builtins, docs/internals/builtins, scripts/docs/builtin_registry.json, or before opening a PR that changes PHP builtins.
---

# Update Builtin Docs

Run the same generated-docs workflow enforced by the `builtins-docs-sync` CI job.
Use the repo root as the working directory.

## Workflow

1. Build the exporter that reads the shared contract joined to the compiler's
   `builtin!` and eval interpreter's `eval_builtin!` registries (an example target, so it can
   link the elephc-magician dev-dependency):

```bash
cargo build --example gen_builtins --features curl
```

2. Regenerate the JSON registries (functions, plus classes and constants in
   `symbol_registry.json`), the Markdown pages, the generated "Functions" blocks in
   the module pages, and the PHP comparison page:

```bash
python3 scripts/docs/extract_builtins.py --render --force
python3 scripts/docs/gen_module_sections.py
python3 scripts/docs/gen_php_comparison.py
```

3. Run the docs audits used by CI:

```bash
python3 scripts/docs/audit_builtins.py
python3 scripts/docs/elephc_builtins/validate_site_compat.py
python3 scripts/audit_builtin_eir_boundary.py --enforce-target-architecture
```

4. Inspect generated changes before reporting or committing:

```bash
git status --short -- docs/php docs/internals/builtins scripts/docs/builtin_registry.json scripts/docs/symbol_registry.json
git diff --check
```

## Rules

- Treat `crates/elephc-builtin-contract` as the PHP-surface source of truth. Treat `src/builtins/` (`builtin!`) and `crates/elephc-magician/src/interpreter/builtins/` (`eval_builtin!`) as implementation bindings for the AOT and eval support dimensions respectively.
- Do not hand-edit generated builtin pages to fix drift; fix the registry, lowering metadata, or `scripts/docs/elephc_builtins/` generator inputs, then rerun the workflow.
- If the user asked only for a sync check, also run:

```bash
git diff --exit-code -- docs/php/builtins.md docs/php/builtins docs/internals/builtins scripts/docs/builtin_registry.json
```

- If generated files changed, include those files in the same PR as the builtin change unless the user explicitly wants a separate docs-only follow-up.
