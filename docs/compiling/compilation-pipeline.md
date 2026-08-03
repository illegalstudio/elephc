---
title: "The compilation pipeline"
description: "Every phase a PHP or LFC source file passes through on the way to a native binary, in order, with the timing label each phase reports."
sidebar:
  order: 2
---

A compile is a fixed sequence of phases. Each one transforms the program and
hands it to the next. The phase names below match the labels printed by
[`--timings`](output-and-diagnostics.md#--timings), so you can map a slow compile
directly back to a stage here.

## Phase order

```text
Physical source (.php or .lfc)
  -> read              read the source file from disk
  -> classify          select tagged PHP or tagless LFC mode from its path
  -> tokenize          Lexer: text -> tokens
  -> parse             Parser: tokens -> AST (Pratt expression parsing)
  -> magic-constants   lower __FILE__, __DIR__, __LINE__, __FUNCTION__, ...
  -> (strict-audit)    reject elephc-only constructs (with --strict-php)
  -> (conditional)     apply compiler ifdef branches from --define
  -> autoload-build    discover autoload rules
  -> resolve           resolve include/require and declarations
  -> pdo-prelude        inject the PDO prelude when used
  -> tz-prelude         inject the timezone-introspection prelude when used
  -> list-id-prelude    inject the DateTimeZone identifier-list prelude when used
  -> var-export-prelude inject the var_export prelude when used
  -> image-prelude      inject the image (GD/Exif/Imagick) prelude when used
  -> web-prelude        inject the web runtime prelude with --web
  -> name-resolve       apply namespace/use rules, canonicalize names
  -> autoload-run       run autoload insertion
  -> opt-fold           AST constant folding
  -> typecheck          Type checker / warnings
  -> exports-scan       collect #[Export] functions (cdylib)
  -> opt-prop           AST constant propagation
  -> opt-post           prune constant control flow (native targets)
  -> opt-norm           control-flow normalization (native targets)
  -> dce                AST dead-code elimination (native targets)
  -> ir-lower           AST -> EIR lowering + EIR validation
  -> ir-opt             EIR optimization passes (native targets)
  -> ir-print           print EIR and stop (with --emit-ir)
  -> runtime-cache      build/reuse the prebuilt runtime object
  -> codegen            EIR -> target assembly
  -> write-asm          write the generated assembly
  -> source-map         write the .map sidecar (with --source-map)
  -> assemble           assembler: assembly -> object file
  -> link               linker: object files -> binary
```

## Front end: source to checked AST

- **read / classify / tokenize / parse** — every physical file is classified
  independently. `.lfc` starts in code mode with no tags; every other suffix
  retains tagged-PHP behavior. The [Lexer](../internals/the-lexer.md) turns the
  source into tokens and the [Parser](../internals/the-parser.md) builds the AST.
- **magic-constants** — magic constants such as `__DIR__` and `__LINE__` are
  substituted before any later pass sees them.
- **strict-PHP audit** — with [`--strict-php`](cli-reference.md#strict-php-mode),
  each freshly parsed PHP-mode AST is audited and every elephc-only construct is
  reported before any later pass runs. Included and autoloaded files are
  classified where they are parsed (inside resolve / autoload-run); LFC and
  compiler-injected source are exempt.
- **conditional compilation** — `ifdef` branches are resolved using the symbols
  passed with [`--define`](linking-and-conditional-compilation.md#conditional-compilation).
- **resolve / prelude injection / name-resolve** — `include`/`require` are
  resolved, declarations are discovered, demand-loaded PHP preludes for PDO,
  timezone introspection, `DateTimeZone::listIdentifiers()`, `var_export()`,
  and image processing are injected only when referenced, the web runtime
  prelude is injected with `--web`, and namespace/`use` rules rewrite
  references to fully-qualified names. Autoloading is wired in around these
  steps.
- **typecheck** — the [Type Checker](../internals/the-type-checker.md) infers and
  validates types and emits warnings.

## Middle: AST optimization

The AST optimizer runs PHP-preserving rewrites that are naturally expressed over
syntax: **opt-fold** (constant folding), **opt-prop** (constant propagation),
**opt-post** (constant control-flow pruning), **opt-norm** (control-flow
normalization), and **dce** (dead-code elimination). See
[The Optimizer](../internals/the-optimizer.md). Constant folding and propagation
run for every target. The current `wasm32-wasi` path deliberately skips
**opt-post**, **opt-norm**, and **dce** so its capability audit observes every
PHP diagnostic and coercion boundary before deciding whether the target can
lower it; native targets always run those three passes.

## Back end: EIR and code generation

- **ir-lower** — the checked AST is lowered into EIR, elephc's PHP-shaped
  intermediate representation, then validated once for structural, type,
  dominance, ownership, and effect invariants. See
  [The EIR Design](../internals/the-ir.md).
- **ir-opt** — on native targets, the
  [EIR optimization passes](optimization.md#eir-optimization-passes)
  run a fixed-point driver over each function: identity arithmetic folding,
  local peephole rewrites, constant folding, common-subexpression elimination,
  loop-invariant code motion, CFG-aware dead-instruction elimination, dead-store
  elimination, and branch simplification. In
  debug/test builds the function is re-validated after every pass. This phase
  can be turned off with [`--no-ir-opt`](optimization.md#eir-optimization-passes).
  `wasm32-wasi` currently keeps unoptimized EIR for the same capability-audit
  reason, regardless of `--ir-opt`.
- **ir-print** — only present with [`--emit-ir`](output-and-diagnostics.md#--emit-ir);
  formats the optimized or unoptimized EIR textual form, prints it to stdout,
  and stops before runtime preparation or code generation.
- **runtime-cache** — for native targets, the hand-written runtime is assembled once and cached in
  `~/.cache/elephc/`, then reused across compiles. See
  [The Runtime](../internals/the-runtime.md).
- **codegen** — native EIR is lowered to target assembly. `wasm32-wasi` instead
  produces, assembles, and validates WAT/WASM in memory, then publishes the
  requested WebAssembly artifacts. See
  [The Code Generator](../internals/the-codegen.md).

## Tail: assemble and link

For native targets, generated assembly is written out and assembled into an
object file. Only on a final-link path, logical [managed native
requirements](native-dependencies.md) are resolved read-only from the project
lock and verified cache receipts. Those exact archives, the cached runtime
object, bridge inputs, and any [extra
libraries](linking-and-conditional-compilation.md) become one typed ordered link
plan for the final binary. This resolution does not install or repair packages
and is folded into the untimed setup immediately before the `assemble`/`link`
timing labels.

The `wasm32-wasi` target ([Targets](targets.md#webassembly-partial-parity))
bypasses this native `as`/`ld` tail entirely. Instead of emitting native
assembly, the `src/codegen_wasm` backend emits a WebAssembly module
(`.wat`/`.wasm`) from the same EIR, so no system assembler or linker runs.

## Inspecting intermediate stages

You do not have to run the whole pipeline. Several flags stop early or dump an
intermediate artifact:

- [`--check`](output-and-diagnostics.md#--check) runs the front end only.
- [`--emit-ir`](output-and-diagnostics.md#--emit-ir) prints EIR (after `ir-opt`) and stops.
- [`--emit-asm`](output-and-diagnostics.md#--emit-asm) writes assembly without linking.
- [`--timings`](output-and-diagnostics.md#--timings) prints how long each phase took.
