# PhpStorm plugin proposal

- [ ] Choose a supported PhpStorm/PHP plugin SDK baseline and verify its extension APIs.
- [ ] Define project opt-in, `.php` / `.lfc` recognition and strict-PHP behavior.
- [ ] Generate IDE signatures from the shared builtin contract.
- [ ] Prototype accurate syntax recognition, extern navigation and pointer types.
- [ ] Add compiler settings and compile/run configurations after language support works.
- [ ] Qualify the plugin against the declared IDE versions and representative examples.

## Status and scope

No plugin implementation exists in this checkout at release candidate
`df57a9800f` (reviewed 2026-10-19). This is a product proposal, not shipped IDE
support. The earlier XML/Kotlin snippets and hand-maintained integer-pointer
stubs were illustrative and have been removed because they are not a verified
SDK implementation or an authoritative compiler contract.

Elephc supports macOS ARM64, iOS device ARM64, iOS Simulator ARM64, Linux ARM64
and Linux x86_64. Project files may use `.php` or `.lfc`; `.lfc` stays
extension-enabled even with `--strict-php`, which restricts only PHP-mode files.
IDE diagnostics and run targets must reflect those choices.

## Proposed integration

Extend the existing PHP language support so ordinary PHP completion, navigation
and inspections remain available. Validate the chosen SDK's APIs before
committing to highlight filtering, completion contributors, indexes or type
providers. Keep syntax diagnostics outside recognized elephc constructs intact;
a line-wide suppression is not sufficient parsing or type support.

| Component | Required behavior |
|---|---|
| Project and file profile | Opt in explicitly; recognize `.php` and `.lfc`, source-profile overrides and strict-PHP restrictions. |
| Builtin library | Derive names, signatures, visibility and documentation from the shared contract and generated registry, without duplicate manual signature tables. |
| Syntax | Recognize `extern` declarations and `ptr` / `ptr<T>` / `ptr_cast<T>()`; account for the shipped generics and other elephc syntax before advertising complete extension support. |
| Navigation and types | Index extern functions/classes/globals/library blocks; retain pointer pointee types rather than representing every pointer as a PHP integer. |
| Compiler integration | Configure the compiler path and working directory, preserve argument boundaries, expose target/output selection and link source diagnostics. |
| Compatibility | Declare the IDE/PHP plugin versions actually tested; explain incomplete syntax or type support. |

An extern text index may be a prototype if PHP PSI cannot represent the syntax,
but it must handle multiline declarations, comments, strings and duplicate
names predictably. Test whether error filtering leaves usable PSI before
choosing it as the long-term approach. Do not promise correct completion from
suppression alone.

## Source contracts and acceptance

- [Source files](../docs/beyond-php/lfc-source-files.md),
  [extern](../docs/beyond-php/extern.md),
  [pointers](../docs/beyond-php/pointers.md), and
  [generics](../docs/beyond-php/generics.md) define the user-visible language.
- `src/parser/stmt/ffi.rs`, `src/parser/ast/ffi.rs`, and
  `src/parser/expr/prefix_complex.rs` own relevant parser constructs.
- `crates/elephc-builtin-contract/`, `src/builtins/pointers/`, and
  `scripts/docs/builtin_registry.json` replace the old checker-catalog references.
- `examples/ffi/main.php` is an initial navigation/completion fixture; add `.lfc`,
  strict-PHP, malformed syntax and target-specific run cases before release.

Verify ordinary PHP behavior alongside extension syntax and navigation. Exercise
compiler settings, diagnostics, paths containing spaces and supported output
forms, including iOS library outputs. The historical proposal remains available
at `54c5c3332e`; no IDE or compiler execution was performed during this cleanup.
