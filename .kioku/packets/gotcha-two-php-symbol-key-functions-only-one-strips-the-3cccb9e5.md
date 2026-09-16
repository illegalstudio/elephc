---
id: gotcha-two-php-symbol-key-functions-only-one-strips-the-3cccb9e5
type: gotcha
title: "Two php_symbol_key functions, only one strips the leading backslash"
description: "names::php_symbol_key only lowercases; the magician builtin_metadata one also trims a leading backslash"
tags: [names, symbols, builtins]
created: 2026-09-15
verified_by: "read both definitions"
sources:
  - path: src/names.rs
    blob: 6793c0108b6a1371527770e1044e5ce79c9e4d9c
    lines: 210-212
    snip: 92d54975d045
    anchor: "pub fn php_symbol_key(name: &str) -> String {"
  - path: crates/elephc-magician/src/interpreter/builtin_metadata.rs
    blob: 97ea550f5975da24ac3978ef46e38cc26e480b24
    lines: 234-236
    snip: 28e4360e8204
    anchor: "fn php_symbol_key(name: &str) -> String {"
---

# Two php_symbol_key functions, only one strips the leading backslash

## Fact

`crate::names::php_symbol_key` is `name.to_ascii_lowercase()`. The magician copy in `interpreter/builtin_metadata.rs` is `name.trim_start_matches('\\').to_ascii_lowercase()`. Same name, same purpose, different normalization.

## Why

A fully-qualified name written with a leading backslash misses every lookup that goes through names::php_symbol_key.

## Trigger

looking up a PHP symbol by key, or a lookup silently finds nothing

## Apply

Normalize the leading backslash yourself before calling crate::names::php_symbol_key; only the magician copy does it for you.
