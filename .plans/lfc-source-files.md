# `.lfc` source files

## Task checklist

- [x] Define the user-visible `.lfc` source contract and central `SourceMode`.
- [x] Add tagless lexing while preserving the existing PHP lexer/parser API.
- [x] Centralize path-aware source loading for the entry point, includes, and autoload.
- [x] Preserve each user statement's source mode through every AST transformation.
- [x] Make `--strict-php` and extension-builtin visibility source-aware.
- [x] Make dynamic callable introspection and `eval()` honor the call-site source mode.
- [x] Extend include and Composer/SPL autoload discovery to `.lfc` files.
- [x] Add lexer, parser, CLI, error, include, autoload, strict-mode, and corpus coverage.
- [x] Add a runnable `.lfc` example and update user/internal documentation.
- [x] Run focused validation and leave the complete supported-target matrix to CI.

## Goal

Allow elephc to compile source files whose physical path ends in `.lfc`.
An `.lfc` file is code from its first byte to its last byte:

- it has no `<?php` opening tag;
- it has no `?>` closing tag;
- it has no inline-HTML region and therefore no implicit output;
- all ordinary output still comes from explicit language constructs such as
  `echo` or `print`;
- elephc language extensions are always available in that file, even when the
  compiler invocation also contains `--strict-php`.

The extension of every physical source file, not only the entry point, selects
the source mode. A project may therefore mix `.php` and `.lfc` files through
`include`, `require`, Composer autoloading, or supported SPL autoload rules.

## User-visible contract

### Source classification

Introduce a central source classifier with these rules:

- a case-insensitive `.lfc` suffix selects `SourceMode::Lfc`;
- every other path retains the current tagged-PHP behavior and selects
  `SourceMode::Php`.

Treating only `.lfc` specially preserves compatibility with existing projects
that use PHP source in paths such as `.inc` or extensionless include files. It
also avoids adding a new entry-point extension rejection that does not exist
today.

The source classifier must be the only place that interprets a path extension.
The entry pipeline, resolver, autoloader, examples corpus, and tests must not
grow independent `.extension() == "lfc"` decisions.

### Tags and output

For `SourceMode::Php`, preserve the current contract: after an optional UTF-8
BOM and leading whitespace/comments, the source must begin with `<?php`.

For `SourceMode::Lfc`:

- start lexing ordinary code at the first source character after an optional
  UTF-8 BOM;
- synthesize the structural `Token::OpenTag` expected by the existing parser
  without prepending text to the source and without shifting spans;
- emit a focused diagnostic if an opening or closing PHP tag appears in code;
- do not reject tag-shaped text inside strings, heredocs, nowdocs, or comments;
- do not add inline-HTML tokens or implicit `echo` statements.

Plain text that is not valid elephc syntax must therefore fail parsing rather
than being copied to stdout.

### `--strict-php`

`--strict-php` remains an invocation-level request but is effective only while
processing `SourceMode::Php` user code:

- PHP source receives the existing syntax audit and extension-builtin hiding;
- LFC source always receives the normal elephc language surface;
- compiler-generated preludes and synthetic internal code remain extension
  capable and are never audited as user PHP.

This policy applies per source file in mixed projects. Computing one effective
boolean from the entry-point extension is insufficient: a strict PHP entry may
include LFC code, and an LFC entry may include PHP code that still needs the
requested strict audit.

The source of an expression also controls runtime-selected behavior:

- `function_exists`, `is_callable`, first-class callables, string/dynamic
  callables, and runtime callable dispatch use the caller's source mode;
- `eval()` invoked from strict PHP exposes the strict PHP surface;
- `eval()` invoked from LFC exposes the elephc surface, including when both
  call sites are compiled into the same binary.

If a strict PHP file declares a user function whose name matches an extension
builtin, calls originating in strict PHP resolve to that user function as they
do today. LFC call sites retain access to the extension builtin. Resolution
must therefore preserve a typed user-function-versus-builtin target instead of
letting one global string catalog decide both call sites.

### `--define`

Remove the blanket CLI rejection of `--strict-php` combined with `--define`.
The combination is required for mixed projects and for LFC entry points:

- `ifdef` in LFC source consumes the supplied define normally;
- `ifdef` in PHP source is rejected by that file's strict audit;
- a strict PHP project that supplies an otherwise unused define is valid.

The audit must still run before conditional compilation consumes an `IfDef`
node, so strict PHP cannot hide an extension construct in an inactive branch.

### Autoload compatibility

Composer PSR-4, PSR-0, classmap, and `autoload.files` loading must be able to
parse `.lfc` files. A supported SPL autoload closure may also resolve to an
`.lfc` path.

Do not change the observable PHP default returned by
`spl_autoload_extensions()`: it remains `.inc,.php`. LFC discovery comes from
explicit paths and the compile-time Composer index, not by changing a PHP
builtin's default value.

## Design

### 1. Central source model

Add a small cohesive module, for example `src/source.rs`, containing:

- `SourceMode::{Php, Lfc}`;
- the path classifier;
- helpers such as `requires_open_tag()` and
  `strict_php_is_effective(requested)`;
- the shared path-aware tokenize/parse entry point;
- a compilation/source-loading context carrying the requested strict flag and
  conditional symbols.

The module must start with the required `//!` preamble, and every function must
have a specific `///` docblock.

Keep `lexer::tokenize(source)` as the tagged-PHP compatibility API used by
existing inline fixtures and generated snippets. Add an explicit mode-aware
entry point for physical files rather than changing hundreds of callers that
intentionally parse PHP-tagged strings.

### 2. Lexer and parser boundary

Update `src/lexer/scan.rs` so the initial-tag step is controlled by
`SourceMode`. Both modes should feed the same ordinary token-scanning loop.

The parser should continue to receive an `OpenTag` followed by code tokens and
`Eof`; no parser grammar fork is needed. The synthetic LFC open-tag token should
use a point span at line 1, column 1 and must never appear as user-written
source in diagnostics.

Add tag detection at the normal scanning boundary instead of searching the raw
source string, otherwise `"<?php"` and comment contents would be rejected
incorrectly.

### 3. Shared physical-file loader

Centralize this ordered operation for every user source file:

1. read the physical file;
2. classify its `SourceMode`;
3. tokenize in that mode;
4. parse;
5. stamp the parsed statements with their source mode;
6. substitute file/scope magic constants for that physical path;
7. run the strict-PHP audit when effective for that file;
8. apply conditional compilation using the invocation's define set.

Adopt the loader in:

- `src/pipeline.rs` for the entry point;
- `src/resolver/files.rs` for include/require discovery and expansion;
- `src/autoload/mod.rs` for Composer files and autoloaded class files;
- any test helper that claims to execute the real file pipeline.

Discovery and final include expansion may parse a file more than once today;
both paths must use the same loader so they cannot disagree on source mode,
strict audit, magic constants, or conditional branches.

Thread the source-loading context explicitly through resolver and autoload
entry points. Keep compatibility wrappers for unit-test helpers only when they
have an unambiguous default (`SourceMode::Php`, no strict mode, no defines).

### 4. Source mode in the AST

The merged AST needs durable source provenance after include declarations are
hoisted and autoloaded statements are spliced into the entry program.

Add compact source-mode metadata to `Stmt`, defaulting to an internal/generated
mode for compiler-created statements. Recursively stamp all statements from a
parsed physical file, including nested bodies. Expressions inherit the active
statement mode, so the much more numerous `Expr` nodes do not need another
field.

Do not add source metadata to `Span`: the repository deliberately pins it to
16 bytes because it is embedded throughout the recursive parser.

Add a statement rebuilding helper that preserves span, attributes, and source
mode. Use it in transformations rather than reconstructing user statements
with an internal default. Audit all direct `Stmt { ... }` construction and all
passes that move or rebuild statements, especially:

- magic-constant and conditional walkers;
- resolver discovery, declaration hoisting, function variants, and include
  guards;
- autoload alias synthesis;
- namespace/name resolution;
- AST folding, propagation, control-flow pruning, normalization, and DCE;
- type-checker declaration/body walks;
- EIR local discovery and statement lowering.

Compiler preludes parsed from embedded strings must be stamped as internal
source so they remain extension-capable and cannot inherit a surrounding PHP
file's strict profile.

### 5. Source-aware builtin resolution

Replace compilation-wide builtin visibility decisions with an explicit
visibility/profile argument derived from the active statement:

- builtin catalog lookup and canonicalization;
- name-resolver builtin fallback;
- builtin redeclaration validation;
- checker direct-call dispatch and undefined-function hints;
- literal `function_exists` and `is_callable` folding;
- first-class callable signatures and aliases;
- dynamic callable candidate construction.

Keep raw registry enumeration independent of source mode. Consumers should
request either the full elephc-visible set or the strict-PHP-visible set rather
than consulting a thread-local global boolean while memoizing results.

Preserve the current diagnostic quality for strict PHP calls to hidden
extension builtins, including the “exists as an elephc extension; it is
disabled by --strict-php” hint.

### 6. EIR and runtime-selected behavior

When lowering a source statement, record its effective builtin visibility in
the EIR operations that can select a callable or inspect the function catalog
at runtime. Prefer a small enum/flag in the relevant typed EIR operation over
duplicating PHP names in backend code.

Emit the complete builtin metadata needed by an LFC call site when a binary
contains LFC code. Strict call sites must filter extension entries using their
EIR visibility operand rather than forcing one global table for the whole
binary.

Change eval setup so every runtime eval dispatch writes the desired strict
state (`true` for effective strict PHP, `false` for LFC/non-strict PHP) before
entering Magician. Setting only the strict case would leak state when a strict
eval call executes before an LFC eval call in the same process.

Literal/AOT eval eligibility checks must use the eval call site's source mode
instead of the invocation-wide strict flag.

No new target-specific assembly behavior should be necessary. If an EIR
operand reaches assembly lowering, handle macOS ARM64, Linux ARM64, and Linux
x86_64 through the existing target-aware runtime-call path in the same change.

### 7. Includes and autoload indexing

Use the central source classifier in `src/autoload/index.rs`:

- PSR-4 and PSR-0 walkers accept both PHP-mode paths and `.lfc`;
- suffix removal handles `.php` and `.lfc` without hard-coded duplicate
  branches;
- classmap file scanning tokenizes according to the physical path;
- `autoload.files` continues to accept explicit existing file paths and loads
  each through the shared loader.

Include/require path folding is unchanged: a string path resolving to `.lfc`
selects LFC mode at load time. `__FILE__` and `__DIR__` substitution must retain
the real `.lfc` path.

The OPcache compile-time manifest should continue listing every physical source
compiled into the binary, including `.lfc` files. Keep stable source-map field
names such as `php_line`; document that they also identify LFC source
coordinates rather than breaking the schema.

### 8. CLI and output paths

Update CLI help and usage text from `<source.php>` to a neutral form such as
`<source-file>`, explicitly listing `.php` and `.lfc`.

The existing output-path implementation already derives artifacts from
`file_stem()`, so `main.lfc` should naturally produce `main`, `main.s`,
`main.o`, and `main.map`. Add coverage rather than a separate LFC output-path
branch.

## Test plan

### Lexer and parser

Add focused coverage for:

- an empty LFC source (`OpenTag`, `Eof`);
- tagless `echo`, declarations, and elephc-only syntax;
- UTF-8 BOM with correct line/column spans;
- `<?php` and `?>` rejected in LFC code;
- tag-shaped strings and comments accepted as data;
- free text rejected rather than emitted;
- existing PHP missing-open-tag and tagged-source tests unchanged.

### CLI and end-to-end codegen

Add `tests/codegen/lfc.rs` (or an equivalently focused module) covering:

- compile and run a minimal `main.lfc`;
- `--check`, `--emit-ir`, or `--emit-asm` accepts LFC and uses the normal stem;
- `--strict-php main.lfc` still accepts a representative elephc-only syntax
  feature and extension builtin;
- a plain PHP file continues to compile exactly as before.

### Strict and mixed-source behavior

Add regression projects for:

- strict PHP directly using extension syntax: rejected;
- strict PHP directly calling an extension builtin: rejected with the current
  hint;
- strict PHP including LFC that uses the same extension: accepted;
- LFC including PHP that uses an extension while `--strict-php` is present:
  rejected and attributed to the PHP file;
- LFC `ifdef` with `--strict-php --define FEATURE`: selected normally;
- PHP `ifdef` with the same invocation: rejected before branch selection;
- strict PHP and LFC call sites in one binary observing different
  `function_exists`/callable visibility;
- strict PHP eval followed by LFC eval, proving the runtime strict state is
  reset in both directions;
- a strict PHP user function shadowing an extension name while an LFC call
  site still resolves the extension builtin.

### Includes, autoload, and paths

Cover:

- PHP requiring LFC and LFC requiring PHP;
- nested and once includes across both modes;
- `__FILE__` and `__DIR__` values for LFC;
- Composer `autoload.files` containing LFC;
- PSR-4 or classmap discovery of a class declared in LFC;
- an LFC entry and included LFC file appearing in the OPcache source manifest.

### Example corpus

Add `examples/lfc/main.lfc` and its local `.gitignore` containing:

```text
*.s
*.o
main
```

The example should be small and demonstrate both tagless syntax and one
recognizable elephc extension rather than acting as a duplicate lexer test.

Update `src/ir_lower/tests/corpus.rs` so it discovers both `main.php` and
`main.lfc`, lowers each using its physical source mode, and retains the
existing exclusions for examples that require pipeline-only preludes.

## Documentation

Update:

- `README.md` with one tagless `.lfc` invocation;
- `docs/compiling/overview.md` with the two source formats;
- `docs/compiling/cli-reference.md` with neutral input syntax and per-file
  strict behavior;
- `docs/compiling/compilation-pipeline.md` with source classification before
  lexing;
- the strict-PHP documentation, including mixed projects and `--define`;
- include/autoload documentation to mention `.lfc`;
- source-map and OPcache wording where “PHP source file” currently means every
  compiled source.

Add a focused user-facing page under `docs/beyond-php/` if the overview and CLI
reference would otherwise need to repeat the complete LFC contract.

Do not regenerate builtin documentation: this feature changes builtin
visibility context, not the builtin registry or any builtin signature.

## Focused verification

Run the smallest useful checks:

```bash
cargo build
cargo test --test lexer_tests lfc
cargo test --test parser_tests lfc
cargo test --test codegen_tests lfc
cargo test --test codegen_tests strict_php
cargo test --test codegen_tests autoload
cargo test --test error_tests lfc
cargo test --test error_tests strict_php
cargo test lowers_examples_corpus
git diff --check
```

Adjust filters to the final test names if necessary. Do not run the complete
local suite or unfiltered Linux Docker scripts by default. CI is responsible
for the full `macos-aarch64`, `linux-aarch64`, and `linux-x86_64` matrix.

## Acceptance criteria

- `elephc build main.lfc` compiles valid tagless source to the normal output paths.
- PHP tags in LFC code are rejected, and non-code text is never output
  implicitly.
- Existing tagged PHP input behavior remains unchanged.
- `--strict-php` is enforced for every PHP-mode user file and never disables
  elephc features in an LFC file.
- Mixed PHP/LFC include and autoload projects behave according to each
  physical file's mode.
- Direct, dynamic, introspection, first-class callable, and eval surfaces agree
  on extension visibility at each call site.
- Conditional defines work in LFC even when `--strict-php` was requested.
- Generated preludes remain extension-capable and invisible to the user strict
  audit.
- All new Rust functions carry docblocks, every new Rust module has the
  required preamble, focused tests pass, `cargo build` is warning-free, and
  `git diff --check` is clean.
