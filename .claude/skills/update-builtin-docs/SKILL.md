---
name: update-builtin-docs
description: Regenerate and audit Elephc's generated builtin documentation from the builtin! and eval_builtin! registries. Use when a change touches src/builtins, crates/elephc-magician/src/interpreter/builtins, builtin signatures, builtin lowering hooks, docs/php/builtins, docs/internals/builtins, scripts/docs/builtin_registry.json, or before opening a PR that changes PHP builtins.
---

# Update Builtin Docs

Run the same generated-docs workflow enforced by the `builtins-docs-sync` CI job.
Use the repo root as the working directory.

## Workflow

1. Build the exporter that reads the single-source `builtin!` registry and the
   eval interpreter's `eval_builtin!` registry (an example target, so it can
   link the elephc-magician dev-dependency):

```bash
cargo build --example gen_builtins
```

2. Regenerate the JSON registry and Markdown pages:

```bash
python3 scripts/docs/extract_builtins.py --render --force
```

3. Run the docs audits used by CI:

```bash
python3 scripts/docs/audit_builtins.py
python3 scripts/docs/elephc_builtins/validate_site_compat.py
```

4. Inspect generated changes before reporting or committing:

```bash
git status --short -- docs/php/builtins.md docs/php/builtins docs/internals/builtins scripts/docs/builtin_registry.json
git diff --check
```

## Rules

- Treat `src/builtins/` (`builtin!`) and `crates/elephc-magician/src/interpreter/builtins/` (`eval_builtin!`) as the source of truth for the AOT and eval support dimensions respectively.
- Do not hand-edit generated builtin pages to fix drift; fix the registry, lowering metadata, or `scripts/docs/elephc_builtins/` generator inputs, then rerun the workflow.
- If the user asked only for a sync check, also run:

```bash
git diff --exit-code -- docs/php/builtins.md docs/php/builtins docs/internals/builtins scripts/docs/builtin_registry.json
```

- If generated files changed, include those files in the same PR as the builtin change unless the user explicitly wants a separate docs-only follow-up.
