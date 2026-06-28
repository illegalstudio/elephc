---
title: "The compilation pipeline"
description: "Every phase a PHP file passes through on the way to a native binary, in order, with the timing label each phase reports."
sidebar:
  order: 2
---

A compile is a fixed sequence of phases. Each one transforms the program and
hands it to the next. The phase names below match the labels printed by
[`--timings`](output-and-diagnostics.md#--timings), so you can map a slow compile
directly back to a stage here.

## Phase order

```text
PHP source
  -> read              read the source file from disk
  -> tokenize          Lexer: text -> tokens
  -> parse             Parser: tokens -> AST (Pratt expression parsing)
  -> magic-constants   lower __FILE__, __DIR__, __LINE__, __FUNCTION__, ...
  -> (conditional)     apply compiler ifdef branches from --define
  -> autoload-build    discover autoload rules
  -> resolve           resolve include/require and declarations
  -> pdo-prelude        inject the PDO prelude when used
  -> tz-prelude         inject the timezone-introspection prelude when used
  -> list-id-prelude    inject the DateTimeZone identifier-list prelude when used
  -> var-export-prelude inject the var_export prelude when used
  -> name-resolve       apply namespace/use rules, canonicalize names
  -> autoload-run       run autoload insertion
  -> opt-fold           AST constant folding
  -> typecheck          Type checker / warnings
  -> exports-scan       collect #[Export] functions (cdylib)
  -> opt-prop           AST constant propagation
  -> opt-post           prune constant control flow
  -> opt-norm           control-flow normalization
  -> dce                AST dead-code elimination
  -> ir-lower           AST -> EIR lowering + EIR validation
  -> ir-opt             EIR optimization passes (fixed-point driver)
  -> ir-print           print EIR and stop (with --emit-ir)
  -> runtime-cache      build/reuse the prebuilt runtime object
  -> codegen-ir         EIR -> target assembly
  -> write-asm          write the generated assembly
  -> source-map         write the .map sidecar (with --source-map)
  -> assemble           assembler: assembly -> object file
  -> link               linker: object files -> binary
```

## Front end: source to checked AST

- **read / tokenize / parse** — the [Lexer](../internals/the-lexer.md) turns
  source text into tokens and the [Parser](../internals/the-parser.md) builds the
  abstract syntax tree.
- **magic-constants** — magic constants such as `__DIR__` and `__LINE__` are
  substituted before any later pass sees them.
- **conditional compilation** — `ifdef` branches are resolved using the symbols
  passed with [`--define`](linking-and-conditional-compilation.md#conditional-compilation).
- **resolve / prelude injection / name-resolve** — `include`/`require` are
  resolved, declarations are discovered, demand-loaded PHP preludes for PDO,
  timezone introspection, `DateTimeZone::listIdentifiers()`, and `var_export()`
  are injected only when referenced, and namespace/`use` rules rewrite
  references to fully-qualified names. Autoloading is wired in around these
  steps.
- **typecheck** — the [Type Checker](../internals/the-type-checker.md) infers and
  validates types and emits warnings.

## Middle: AST optimization

The AST optimizer runs PHP-preserving rewrites that are naturally expressed over
syntax: **opt-fold** (constant folding), **opt-prop** (constant propagation),
**opt-post** (constant control-flow pruning), **opt-norm** (control-flow
normalization), and **dce** (dead-code elimination). See
[The Optimizer](../internals/the-optimizer.md). These always run; they are not
behind a flag.

## Back end: EIR and code generation

- **ir-lower** — the checked AST is lowered into EIR, elephc's PHP-shaped
  intermediate representation, then validated once for structural, type,
  dominance, ownership, and effect invariants. See
  [The EIR Design](../internals/the-ir.md).
- **ir-opt** — the [EIR optimization passes](optimization.md#eir-optimization-passes)
  run a fixed-point driver over each function: identity arithmetic folding,
  local peephole rewrites, constant folding, common-subexpression elimination,
  loop-invariant code motion, CFG-aware dead-instruction elimination, dead-store
  elimination, and branch simplification. In
  debug/test builds the function is re-validated after every pass. This phase
  can be turned off with [`--no-ir-opt`](optimization.md#eir-optimization-passes).
- **ir-print** — only present with [`--emit-ir`](output-and-diagnostics.md#--emit-ir);
  formats the optimized or unoptimized EIR textual form, prints it to stdout,
  and stops before runtime preparation or code generation.
- **runtime-cache** — the hand-written runtime is assembled once and cached in
  `~/.cache/elephc/`, then reused across compiles. See
  [The Runtime](../internals/the-runtime.md).
- **codegen-ir** — EIR is lowered to target assembly through the default backend.
  See [The Code Generator](../internals/the-codegen.md).

## Tail: assemble and link

The generated assembly is written out, assembled into an object file, and linked
together with the cached runtime object (and any
[extra libraries](linking-and-conditional-compilation.md)) into the final binary.

## Inspecting intermediate stages

You do not have to run the whole pipeline. Several flags stop early or dump an
intermediate artifact:

- [`--check`](output-and-diagnostics.md#--check) runs the front end only.
- [`--emit-ir`](output-and-diagnostics.md#--emit-ir) prints EIR (after `ir-opt`) and stops.
- [`--emit-asm`](output-and-diagnostics.md#--emit-asm) writes assembly without linking.
- [`--timings`](output-and-diagnostics.md#--timings) prints how long each phase took.
