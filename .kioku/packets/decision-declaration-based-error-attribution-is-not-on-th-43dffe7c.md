---
id: decision-declaration-based-error-attribution-is-not-on-th-43dffe7c
type: decision
title: "Declaration-based error attribution is not on this branch; CompileError.file is"
description: "the declaration field and resolve_declaration_files live on an unmerged branch, and two traps apply either way"
tags: [checker, diagnostics, autoload, branches]
created: 2026-09-15
verified_by: "checked against 6e6f0261 with kioku def and kioku find"
stale_when: "the declaration-based attribution branch merges into main"
sources:
  - path: src/errors/mod.rs
    blob: c58d396c0f9b8205b7ea5a08041162514c733cf7
    lines: 17-22
    snip: 83b8108f8719
    anchor: "pub struct CompileError {"
  - path: src/names.rs
    blob: 6793c0108b6a1371527770e1044e5ce79c9e4d9c
    lines: 210-212
    snip: 92d54975d045
    anchor: "pub fn php_symbol_key(name: &str) -> String {"
  - path: src/resolver/function_variants.rs
    blob: 0545792d1fc5f21e7915fddcfacb5b1218bca9c0
    lines: 39-112
    snip: b08506013b22
    anchor: "pub(super) fn rewrite_include_loaded_function_variants("
  - path: crates/elephc-magician/src/interpreter/builtin_metadata.rs
    blob: 97ea550f5975da24ac3978ef46e38cc26e480b24
    lines: 234-236
    snip: 28e4360e8204
    anchor: "fn php_symbol_key(name: &str) -> String {"
---

# Declaration-based error attribution is not on this branch; CompileError.file is

## Fact

Multi-file compile errors are hard to attribute because `Span` carries no file (see the
companion packet). On **this branch** the mechanism is `CompileError.file: Option<String>`,
set by whichever pass raises the error.

A different approach — attributing by DECLARATION, with `CompileError` carrying
`declaration: Option<String>` and the pipeline mapping that name back to a path through a
`resolve_declaration_files` helper — was developed on **another branch and is not merged
here**. Checked 2026-09-15 against `6e6f0261`:

- there is no `declaration` field on `CompileError`;
- there is no `resolve_declaration_files` symbol anywhere;
- the resolver exposes `resolve_collecting_includes` and
  `resolve_collecting_includes_with_defines`, but no `..._and_sources` variant.

Treat the declaration-based design as a proposal until that branch lands.

## The two traps, both real on this branch

**1. Two different `php_symbol_key`.** `crate::names::php_symbol_key` is only
`name.to_ascii_lowercase()`. The magician's private copy in
`interpreter/builtin_metadata.rs` is `name.trim_start_matches('\\').to_ascii_lowercase()`.
A fully-qualified name written with a leading backslash therefore misses every lookup that
goes through `names::php_symbol_key`. Normalize the backslash yourself.

**2. Include-loaded functions are renamed before the checker sees them.** A function declared
inside an include becomes `__elephc_include_variant_<hash>_<name>` in
`rewrite_include_loaded_function_variants`. Any path-attribution map has to record the
RENAMED symbol against the same file, or the lookup finds nothing for exactly the functions
that made attribution necessary in the first place.

## Correction to an earlier note

An earlier version of this memory said the map is "keyed by `crate::names::php_symbol_key`
(leading `\` stripped, lowercased)". The stripping half is wrong for that function — only the
magician copy strips. That is trap 1 above.

## Trigger

attributing a multi-file compile error, or looking for resolve_declaration_files
