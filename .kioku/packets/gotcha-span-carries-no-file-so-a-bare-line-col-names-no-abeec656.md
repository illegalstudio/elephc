---
id: gotcha-span-carries-no-file-so-a-bare-line-col-names-no-abeec656
type: gotcha
title: "Span carries no file, so a bare line:col names nothing after splicing"
description: "Span is four u32 fields with no file; CompileError carries the file separately"
tags: [checker, diagnostics, autoload]
created: 2026-09-15
verified_by: "read src/span.rs and src/errors/mod.rs"
sources:
  - path: src/span.rs
    blob: 049c51745829523abdd12f838e7211dfc354e9e0
    lines: 29-34
    snip: 4b0102ee5c07
    anchor: "pub struct Span {"
  - path: src/errors/mod.rs
    blob: c58d396c0f9b8205b7ea5a08041162514c733cf7
    lines: 17-22
    snip: 83b8108f8719
    anchor: "pub struct CompileError {"
---

# Span carries no file, so a bare line:col names nothing after splicing

## Fact

Span is { line, col, end_line, end_col } — four u32, 16 bytes, no file, deliberately. CompileError carries file: Option<String> alongside its span.

## Why

Span sits in every token and AST node, so adding a file field there costs memory everywhere; after the autoload and include passes splice files together, line numbers collide and a bare line:col is unactionable.

## Trigger

a multi-file compile error points at the wrong file, or you are tempted to add a file to Span

## Apply

Attribute through CompileError.file, which is Option<String> and set by the pass that raises the error. Do not widen Span.
