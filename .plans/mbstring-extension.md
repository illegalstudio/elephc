# Complete mbstring extension

- [x] Audit the existing implementation and identify related pull requests.
- [x] Capture the complete PHP 8.5 baseline surface and foundational compatibility fixtures.
- [ ] Implement one shared `elephc-mbstring` engine for encodings, Unicode, and state.
- [ ] Implement all string, conversion, detection, MIME, and entity operations.
- [ ] Implement the complete mbregex family with PHP-compatible regex semantics.
- [ ] Implement variable/reference, callback, HTTP, output, and mail integration.
- [ ] Declare all PHP functions and constants in the shared builtin contract.
- [ ] Route AOT and Magician through the same engine; remove superseded implementations.
- [x] Route the implemented shared eval operations through production coercion callbacks and catchable arity errors.
- [x] Migrate direct AOT argument preparation and consume physical-source strictness.
- [ ] Finish source strictness propagation through dynamic callable wrappers and eval fragments.
- [ ] Wire optional linking, `--with-mbstring`, monitoring, packaging, and all targets.
- [ ] Add focused bridge, codegen, error, eval, optimizer, ownership, and target tests.
- [ ] Update examples, module/CLI/internals docs, roadmap, and generated builtin docs.
- [ ] Run the builtin documentation skill gates and relevant focused verification.
- [ ] Audit all baseline functions, constants, and encoding behavior before declaring completion.

## Scope and architecture

The requested scope is the complete mbstring module in the repository's vendored
PHP 8.5.10 baseline, including mbregex, stateful settings, references, callbacks,
and HTTP/mail-facing functions. A subset of the string functions is not completion.
The starting baseline provided only `mb_strlen` and `mb_ereg_match`, with separate
AOT/eval implementations. The existing regex path uses PCRE2 and only interprets
the `i` option, which needs replacement for full mbregex parity.

Use one optional bridge crate as the engine, with a stable panic-contained C ABI
for AOT and the same operations for Magician. Backend adapters own boxed values,
references, callback invocation, and integration with output/mail/request services;
they must not duplicate encoding or Unicode algorithms. All five supported targets
remain first-class. State must be shared between AOT and eval and isolated/reset
at the appropriate request boundary. Returned buffers are caller-owned.

Compatibility evidence comes from `scripts/docs/php_baseline.json`, PHP 8.5.10
with mbstring installed locally, the official PHP manual, php-src, and Unicode
data matching PHP. Preserve malformed-sequence behavior, byte offsets versus
character offsets, all aliases, substitution modes, full/simple case modes,
contextual sigma, and non-UTF encodings. Test invalid arguments as well as values.

Sources:
- https://www.php.net/manual/en/book.mbstring.php
- https://www.php.net/manual/en/ref.mbstring.php
- https://github.com/php/php-src/tree/php-8.5.10/ext/mbstring

## Related pull request ledger

Read-only audit on 2026-09-07, rechecked at 2026-09-10 10:22:43 UTC: all five PRs remain open
at the recorded heads. Keep these open while implementation is incomplete.
The eventual PR comment can state which PRs this implementation supersedes and
close them when the user proceeds with that final step.

| PR | Status | Surface | Audited head |
| --- | --- | --- | --- |
| [#895](https://github.com/illegalstudio/elephc/pull/895) | Open | `mb_strwidth` | `e96b43f219c085551a3c42c4cbbbd752846c0f88` |
| [#898](https://github.com/illegalstudio/elephc/pull/898) | Open | `mb_strtoupper` | `b15d629eb1d159ffb32fe272eb0f5c17472fb97b` |
| [#899](https://github.com/illegalstudio/elephc/pull/899) | Open | `mb_strtolower` | `089ffc6bee2d2b3f17746da6fdf435f5be423440` |
| [#900](https://github.com/illegalstudio/elephc/pull/900) | Open | `mb_strimwidth` | `f402f762e093b4ad1c98bee1c83cb59bb4278c8c` |
| [#902](https://github.com/illegalstudio/elephc/pull/902) | Open | `mb_convert_case`, `MB_CASE_*` | `405f77283dd4e5c805eff74dc70f3fe013b56885` |
| [#460](https://github.com/illegalstudio/elephc/pull/460) | Merged | Existing `mb_ereg_match` | Already in the base |
| [#466](https://github.com/illegalstudio/elephc/pull/466) | Merged | Existing `mb_strlen` | Already in the base |

The open PRs are references for behavior and regression cases, not an implementation
to combine mechanically. In particular, their encoding restrictions and malformed
UTF-8 behavior must be cross-checked against PHP before reuse.

## Current implementation and evidence

`crates/elephc-mbstring` is registered as a workspace/default member and compiler dev
dependency. Sixty-three of the 65 baseline functions now have public AOT and eval
bindings to the shared engine, including the Oniguruma-based mbregex family,
`mb_parse_str`, `mb_output_handler`, and `mb_ereg_replace_callback`.
The remaining two functions are `mb_convert_variables` and `mb_send_mail`.
Existing bindings still have reference, strictness, ownership, frontend, and host
integration gaps recorded in the checkpoints below; binding coverage does not
establish complete PHP semantics.

- `scripts/mbstring/php_surface.json` captures all 65 functions, nine constants,
  79 canonical encodings, aliases, MIME names, defaults, types, and passing modes.
  The catalog test compares function scope with the repository's independent baseline.
- Unicode 17 case mappings and width/property tables are reproducibly generated from
  hash-pinned UCD files. PHP oracle hashes check all 1,112,064 Unicode scalars under
  eight case modes and display width. Context tests also cover PHP's bounded sigma
  lookbehind, batch boundaries, full/simple mappings, and ISO-8859-9 Turkish casing.
- Fifteen core codec implementations cover UTF-8/16/32, UCS-2/4, ASCII/7bit, and 8bit.
  Exact decoder and encoder tables also cover 22 legacy one-byte encodings and 14
  Shift-JIS/Chinese/Korean variants, including composite mappings and SoftBank emoji
  escape state. Additional codecs now cover all four mobile UTF-8 variants, EUC-JP,
  eucJP-win, EUC-JP-2004, CP51932, UTF-7, UTF7-IMAP, both GB18030 revisions, EUC-TW,
  HZ, ISO-2022-KR, and all eight Japanese stateful variants (JIS, ISO-2022-JP,
  ISO-2022-JP-MS, CP50220/21/22, ISO-2022-JP-2004, and mobile KDDI).
  The four transfer codecs (BASE64, Quoted-Printable, UUENCODE, and HTML-ENTITIES)
  bring the total to all 79 canonical encodings (78 distinct oracle names plus
  7bit, which shares ASCII).
- Codec oracle hashes cover every one- and two-byte input plus deterministic longer
  samples and boundary cases (66,831 inputs per tested encoding), checking validity,
  length, decoding, scrubbing, and uppercase output. Additional encoded text fixtures
  cover all eight case modes and original byte strings across decoder batches.
- Seven additional text functions and eight case constants now have shared PHP
  contracts and AOT/eval bindings. The separate AOT and Magician `mb_strlen`
  implementations have been replaced by the shared engine. The `Codec::Pending` and `CodecUnavailable` development markers
  are now removed. Canonical codec operations are infallible; PHP argument failures
  remain structured operation errors.

Next work: complete variable conversion and mail, close the retained string-owner
leak in the native regex callback path, and address the recorded callback frontend,
reference, strictness, ownership, and host integration gaps. The complete checklist
above remains authoritative.

Focused verification is recorded below and updated as each engine surface lands.
Earlier codec work passed the catalog, codec, JIS, kana, batch, operation, text,
and UTF-7 test binaries on 2026-09-07, together with
`cargo build -p elephc-mbstring`, PHP and Python syntax checks, mapping SHA-256
checks, encoding catalog regeneration, and new-file documentation/text hygiene.
Generated filenames also pass a case-insensitive collision check for macOS.
The first bridge integration and target verification are recorded below.

## String operations and additional codec progress

The shared text engine now implements substring, byte cutting, splitting, ord/chr,
first-character title/lowercase conversion, all trim variants, padding, display-width
truncation, forward/reverse sensitive/insensitive position and substring searches,
and nonoverlapping substring counting. Structured engine errors preserve PHP's
exception classes and messages. Backend argument adaptation and deprecation output
are still required.

- A compressed PHP oracle currently compares 75,197 complete operation requests,
  including malformed input, raw-byte preservation, signed bounds, empty values,
  multibyte markers, pad validation order, and exact errors.
- Seven SJIS-mac substring calls are explicitly excluded because the PHP oracle
  process terminates on an upstream allocation underflow. Their complete requests
  and reason are retained in `operations-excluded.json`; audit them before completion.
- Raw slicing strategies are generated from PHP. UTF-8, UTF-16, and GB18030 have
  dedicated cut rules. UTF-7 has a separate legacy streaming filter for mb_strcut,
  including fixed default substitution and provisional-flush byte budgets.
- UTF-7 conversion separately records validation failures which need not emit an
  invalid codepoint. Explicit decoder batches preserve contextual case conversion.
- Mobile UTF-8 tests hash every scalar in both directions. EUC-JP tests add every
  suffix to the 0x8F prefix. GB18030 tests every four-byte address and encoder value.
  EUC-TW tests all suffixes of the three older CNS planes supported by PHP.
- UTF-7 tests cover every BMP code unit with and without a terminator, every
  Unicode-range encoder input, and malformed streaming cuts across varied budgets.
- HZ reuses the EUC-CN tables with its U+2225 decoder override and U+2016 encoder
  exclusion. Exhaustive shifted byte-pair fixtures, a complete Unicode-range encoder
  hash, escape/line-continuation cases, and streaming byte-budget cuts pass.
- ISO-2022-KR shares UHC mappings with PHP's restricted decoder rows and historical
  raw-code encoder fallback. Exhaustive shifted pairs, all Unicode-range encoder
  inputs, malformed escapes, SI/SO transitions, and streaming cuts pass. Its final
  legacy flush may emit a designation even when the cut budget is smaller. mb_chr
  checks actual representability instead of treating those control bytes as success.
- All eight Japanese stateful variants now have complete scalar encoder hashes,
  exhaustive shifted byte-pair hashes, malformed escape fixtures, and legacy cuts.
  CP50220 shares kana KV transformations and preserves legacy deferred output and
  final error suppression. CP50222 uses SI/SO kana shifts. ISO-2022-JP-2004 shares
  EUC-JP-2004 mappings and retains PHP's legacy shared-plane and pending-tail quirks.
- KDDI preserves the distinction between modern flag/keycap composition and legacy
  cuts, including non-reversible keycap/flag mappings. Fast conversion retains the
  source decoder's 128-word partitions because KDDI lookahead stops at a boundary.
- `mb_convert_kana` now has a shared text operation and validated mode descriptor.
  Independent hashes cover all Unicode-range inputs for its 17 flags; 9,375 complete
  requests cover every flag combination through length three, invalid binary flags,
  inverse-rule diagnostic order, kana composition, and the 78 oracle encodings.
  Binary exception messages are represented losslessly for backend adaptation.
- 15,696 dedicated batch requests cover KDDI conversion, UTF-16 contextual casing,
  mobile Shift-JIS replacement state, and mobile kana lookahead.
  A portable scan reproduces the pinned PHP x86_64 AVX2 decoder's 16-unit blocks,
  scalar reserved slot, and atomic malformed-surrogate output. The baseline's
  platform-dependent partitions remain an explicit compatibility-audit decision.

## Transfer encodings, numeric entities, and shared settings

- All four transfer codecs implement both ordinary character transforms and PHP's
  byte-oriented fast-conversion exceptions. BASE64 and Quoted-Printable can replace
  the requested conversion source with raw bytes; BASE64, Quoted-Printable, and
  UUENCODE can replace the destination. Sensitive searches use the same exception,
  while insensitive searches retain character conversion and simple folding.
- The transfer fixture contains 127,480 requests covering malformed syntax, line
  lengths, UUENCODE headers, named/numeric HTML entities, legacy byte cuts, cross-codec
  conversion, and all substitution modes with multiple remembered characters.
  HTML entity names and encoder preferences are captured independently from PHP.
- Substitution now shares a single marker policy. Long markers have no artificial
  leading zeroes; an unrepresentable remembered character in long/entity mode is
  discarded. Mobile Shift-JIS encoders retain recursive replacement-call lookahead,
  including deferred final digits, across the caller's decoder buffers.
- Encoding-specific byte cuts cover every converted slicing descriptor. The former
  provisional generic cut fallback has been removed.
- `mb_encode_numericentity` and `mb_decode_numericentity` have shared engine operations.
  206,394 oracle requests cover all encodings, overlapping maps, wrapping offsets,
  masks, malformed references, optional semicolons, decimal overflow, substitution,
  and mobile encoder boundaries. Backend map-value coercion remains an adapter task.
- Shared request settings now own language, internal/output encoding, detection
  order, replacement mode/character, and the explicit encoding lookup cache.
  1,329 ordered requests compare exact values, failures, resulting settings, and
  deprecation emission. The cache preserves PHP's last-name behavior, including
  alias changes and NUL-terminated ordinary names.
- Twelve language descriptors centralize canonical names, aliases, auto detection
  lists, and mail charset/header/body defaults. Changing language preserves the active
  detection list; a failed change resets the language while retaining earlier auto
  defaults. Comma-separated and array encoding lists share parsing, while preserving
  their distinct whitespace, auto abbreviation, duplicate, and NUL rules.
- Case conversion now shares explicit decoder partition metadata with conversion and
  entity operations. Transformed encoder calls retain their boundaries. Kana's pending
  final character is also retained for the mobile codecs that can observe that boundary.

These are engine implementations, with focused PHP comparisons. Request lifecycle,
INI integration, diagnostics in compiled/eval programs, and user-visible bindings
remain mandatory. No full compiler suite was run.

Latest focused verification passed on 2026-09-07: 17 tests across the catalog,
codec, Unicode, text, operation, kana, batch, transfer, entity, and state binaries.
`cargo build -p elephc-mbstring`, `cargo check -p elephc-mbstring --tests`, and
`git diff --check` passed. PHP/Python syntax, Rust preambles/function docs, mapping
hashes, case-insensitive filenames, and deterministic catalog/language/state
regeneration passed as well.

## Detection and automatic source selection

- Shared single-string detection now implements PHP's common/rare codepoint scores,
  punctuation/supplementary penalties, strict rejection, specialized prechecks,
  BOM handling, stable ties, and single-precision candidate-order weighting.
  The request state supplies default lists and strictness through the common parser.
- The public encoding snapshot records detection eligibility. All 73 text candidates
  participate; transfer encodings and 7bit/8bit are filtered according to the caller's
  contract. A complete `mb_list_encodings()` array has identity-dependent order
  semantics, which the backend adapter must preserve through an explicit flag.
- 468,304 PHP requests compare detection, including every single byte in every
  canonical encoding, all byte pairs for three competing candidate groups, natural
  encoded text, longer binary samples, complete lists, order, and exact errors.
- `ConversionSources` reuses detection for `mb_convert_encoding`. An explicit single
  source bypasses detection and accepts transfer codecs; multiple sources apply
  text filtering before per-string detection. 52,224 further requests cover output,
  default/internal encoding, strictness, all replacement modes, filtering, and the
  ordering of deprecations, source errors, and failed-detection warnings.
- The common-codepoint bitmap is generated from PHP 8.5.10's hash-pinned data.
  Its bits were independently checked against PHP's generated rare-codepoint table.
  The bitmap carries PHP-3.01 provenance, `LICENSE-PHP`, and `NOTICE.md` alongside
  the existing Unicode license. Include both data notices in packaged distributions.
- Multi-string detection for `mb_convert_variables` and request parsing still needs
  PHP's reverse string traversal, repeated weighting, and decoder state carried
  between strings. Container key/value conversion, recursion, illegal-character
  accounting, and adapter diagnostics remain integration tasks.

Focused detection verification passed: six tests across the detection, automatic
conversion, encoding catalog, and state binaries. This extends the earlier engine
checks without running the compiler's full suite.

Outstanding compatibility audit details:

- Audit remaining transformed consumers (substring, splitting, width trimming, and
  future MIME/detection operations) for mobile encoder invocation boundaries and
  recursive replacement state. Keep transfer deprecations in the shared state adapter.
- Complete request lifecycle and INI integration, illegal-character accounting,
  the strict-detection INI adapter, HTTP input/output state, and mail configuration.
- Preserve the explicit UTF-16 baseline partition policy during all-target integration;
  cross-check KDDI lookahead when adding remaining conversion consumers and codecs.
- Runtime bindings, MIME, regex, callbacks, references, HTTP/output/mail,
  and all-target integration remain unimplemented. Numeric entity value coercion and
  container adaptation are not yet connected to either backend.

## First shared AOT/eval bridge

- Versioned neutral argument/result structs pin all offsets for the five supported
  64-bit targets. The panic-contained engine owns thread-local request state and
  explicit result-buffer release; Magician does not embed a second production copy.
- `mb_strlen` uses the same codecs and lookup cache in native calls, opaque eval,
  and callable dispatch. Binary ValueError messages remain catchable across eval
  without unwinding through Rust or retaining the pending exception's raw owner.
- Optional library discovery, `--with-mbstring`, monitoring, request entry reset,
  runtime feature bits, test bridge discovery, CI/nightly/release build lists,
  packaged static archives, Homebrew installation, and third-party notices are wired.
  Host INI overrides and complete HTTP request integration remain outstanding.
- The 2026-09-07 focused checks passed: nine mb_strlen codegen cases with PHP
  comparison, the additional nullable-function/empty-encoding regression, three
  error tests, three engine ABI tests, eight Magician registry tests, the Magician
  builtin execution case, neutral ABI/registry gates, runtime feature gates, and
  all 22 bridge catalog tests. The compiler builds without warnings.
- Clang assembled the complete mbstring/eval runtime for macOS ARM64, iOS device,
  iOS Simulator, Linux ARM64, and Linux x86_64. Local executable checks ran on
  Linux x86_64; CI supplies the remaining executable target matrix.
- The builtin docs skill sequence, YAML/TOML and shell syntax checks, example PHP
  syntax check, assembly comment alignment, Rust preambles, and diff hygiene pass.
- The allocation regression repeats calls with reused values. A broader opaque
  eval fixture also revealed existing temporary/call-argument ownership behavior
  unrelated to the engine. Audit that boundary before declaring full mbstring
  ownership coverage; do not infer it from the narrower passing regression.

Next integration batch: share generic argument/result adapters and expose width,
case conversion, first-character casing, and display-width trimming, including the
related PR scopes and all eight case constants.

## Text bridge expansion

- Added shared contracts and AOT/eval homes for `mb_strwidth`, `mb_strtoupper`,
  `mb_strtolower`, `mb_convert_case`, `mb_ucfirst`, `mb_lcfirst`, and `mb_strimwidth`.
  Eight `MB_CASE_*` constants come from the neutral constant catalog. The ninth
  baseline constant belongs to the remaining mbregex implementation.
- Generic runtime adapters stage typed scalar arguments, copy bridge-owned string
  results, and construct the correct PHP error class after Rust returns. AOT and
  eval consume the same versioned operation IDs, codecs, diagnostics, and state.
- Nullable builtin parameter storage is explicit in semantic metadata. Shared
  argument planning and callable signatures preserve nullable encoding values,
  avoiding an early string cast that would turn null into an invalid empty name.
- Successful native and opaque eval case/width operations, encoding-error ordering,
  all eight case modes, named/spread arguments, and repeated string-result cleanup
  have passed focused checks with PHP comparison. Runtime-selected callable names
  and nullable first-class/named arguments also pass. All five supported targets
  compile the text operations, including exported iOS static-library functions.
  Clang assembled both the complete runtime and user program for each target.
- The whole-runtime x86_64 call-alignment audit passes. The obsolete mb_strlen
  exclusion has been removed; the new adapters need no audit exception.
- Remaining argument work includes PHP eval weak/strict coercion, invalid container
  and resource types, Stringable objects, null-to-scalar diagnostics, and catchable
  argument-count/type errors. The initial scalar adapters must be broadened before
  the full extension is considered complete.

The text batch verification additionally includes five exported engine ABI tests,
23 neutral contract tests, a FakeOps integration test using the real engine, all
five x86_64 call-alignment gates, and the nullable-signature and logical-runtime-ABI
gates. The nullable regression also passes with sentinel null representation, and
the runtime-selected callable regression passes with EIR optimization disabled.
The native allocation case now includes a nullable function-result temporary.

Generated documentation templates now use regular hyphens instead of em dashes,
in accordance with the user preference. This causes mechanical punctuation changes
across the regenerated reference pages in addition to new mbstring content and
sidebar ordering. The two focused generator test groups pass (9 and 23 tests).

Final text-batch checks: all 17 focused mbstring codegen tests pass together with
`ELEPHC_PHP_CHECK=1`, including both string-result ownership regressions and the
five-target compile test. A direct CLI probe also force-links `--with-mbstring`
and executes successfully. `cargo build -p elephc` completed without warnings.
The original mb_strlen home now shares the same contract-driven checker as the
seven newly bound functions. The EIR boundary audit passes with explicit typed
operation arms, and documentation/site audits report zero errors.


## Scalar text and settings expansion

The second generic batch adds 22 shared contracts and AOT/eval homes:
`mb_substr`, `mb_strcut`, `mb_scrub`, `mb_trim`, `mb_ltrim`, `mb_rtrim`,
`mb_str_pad`, `mb_convert_kana`, `mb_substr_count`, `mb_ord`, `mb_chr`,
`mb_strpos`, `mb_stripos`, `mb_strrpos`, `mb_strripos`, `mb_strstr`,
`mb_stristr`, `mb_strrchr`, `mb_strrichr`, `mb_language`,
`mb_internal_encoding`, and `mb_http_output`.

- A validated argument view consumes neutral parameter defaults. The bridge now
  accepts boolean slots and zero-argument calls with a null argument pointer.
- Boolean results have a distinct wire kind. Zero positions, empty strings, and
  failed searches retain their PHP identities. AOT mixed results and every eval
  scalar result use one owned-cell adapter, which releases intermediate strings.
- Nullable integer arguments preserve both the compiler's inline tagged form and
  its boxed sentinel-mode form. A regression caught null lengths being coerced to
  zero; both representations now have focused integration coverage.
- Substring bound checks, empty ordinal/count inputs, and kana mode checks run
  before encoding lookup as PHP requires. Tests verify that these failures neither
  emit an encoding deprecation nor populate its cache.
- Language/internal/output getters and setters use the same request state in both
  directions across AOT/eval. This does not finish INI, HTTP conversion, request
  configuration, exported-host lifecycle, or output-handler integration.
- `examples/mbstring/main.php` demonstrates imported product-label normalization,
  character/byte limits, search variants, and Unicode ordinals.

At this stage, 23 mbstring codegen tests pass together with PHP comparison enabled,
including all newly exposed text operations and repeated native/eval scalar-result
cleanup. Eight engine ABI tests, 23 neutral contract tests, two real-engine FakeOps
integration tests, shared nullable-signature/runtime-ID checks, the scalar diagnostic
matrix, and all five whole-runtime SysV alignment gates also pass.

Remaining integration issues discovered or confirmed in this batch:

- The neutral union-descriptor gap identified here is resolved in the subsequent
  union and array batch below. Container parameters and the remaining scalar
  coercion rules still need their runtime adapters.
- Magician's lexer currently converts high hexadecimal/octal string escapes through
  Unicode characters, does not accept hexadecimal integer literals, and leaves
  Unicode string escapes uninterpreted. These are frontend limitations, independent
  of the bridge. Current invalid-byte bridge fixtures construct bytes with `chr()`;
  repair and test the source-literal boundary before final full-extension verification.
- Weak/strict scalar coercion, Stringable/resource/container handling, null-scalar
  diagnostics, and catchable runtime arity/type errors remain pending from the
  previous batch. HTTP/mail, arrays/references/callbacks, MIME, mbregex, and the
  remaining state/INI APIs remain part of the original required scope.

Additional scalar-batch verification completed:

- The optimizer regression keeps discarded settings writes and repeated default
  encoding reads observable; it matches PHP output.
- Nullable-length operations pass under `ELEPHC_NULL_REPR=sentinel`; mixed-result
  callable tests pass with `ELEPHC_IR_OPT=off`.
- Clang assembled both exported scalar-operation programs and the complete
  mbstring/eval runtime for macOS ARM64, iOS device, iOS Simulator, Linux ARM64,
  and Linux x86_64. Executable behavior was tested locally on Linux x86_64;
  CI remains responsible for executable coverage on the other host platforms.
- The new product-label example compiles and matches PHP byte for byte.
- `cargo build -p elephc` and the curl-enabled builtin exporter build without warnings.
- Generated references contain 1038 total builtins, with 22 additional user pages
  and 22 additional internals pages. Builtin audits, 1998-page site validation,
  and the enforced EIR/target boundary audit report zero errors.
- Assembly-comment alignment, module Rustdoc preambles, generated punctuation,
  and `git diff --check` are clean. No formatter or full compiler suite was run.

Next priority: make shared type contracts expressive enough for the PHP union
signatures, then extend array-bearing conversion/detection/settings operations and
repair the confirmed eval literal/coercion boundaries. Keep the original 65-function
scope, mbregex replacement, host integration, and PR ledger active.

The final scalar recheck passes all six matching tests after adding namespace
fallback coverage for every new text operation and case-insensitive setting names.


## Shared unions and initial array returns

- Neutral `TypeSpec` now represents explicit `false`, `null`, and union alternatives.
  Registry validation rejects malformed unions; the compiler, callable signatures,
  backend result types, and documentation exporter consume the same declarations.
  Existing mbstring false/scalar and boolean/string results now declare their exact
  PHP unions instead of Mixed metadata. Other builtins retain their existing
  nullable-parameter compatibility behavior.
- Added shared AOT/eval contracts for `mb_str_split`, `mb_encoding_aliases`, and
  `mb_preferred_mime_name`. The shared engine now exposes 33 operations; the old
  `mb_ereg_match` makes 34 of the required 65 functions available. Eight of nine
  constants are connected. The original complete-extension scope is unchanged.
- String arrays use one owned, length-framed wire buffer. Native materialization
  validates every prefix and complete payload, preserves arbitrary binary strings,
  and transfers cells into a fresh indexed array with ordinary COW metadata.
  Boxed eval results consume their intermediate native array owner after retaining
  its payload. The fake adapter consumes exactly the same real-engine results.
- All 79 alias lists and preferred MIME names match the independent PHP baseline.
  MIME lookup bypasses the normal explicit-encoding cache; alias lookup participates
  in its deprecation behavior. Split length validation precedes encoding lookup.
- A new reproducible oracle contains 35,076 complete split-array comparisons across
  all 79 encodings, long inputs around decoder boundaries, malformed/incomplete
  units, and substitution modes. Every case passes. This covers the split consumer's
  mobile/batch audit; the substring and display-trimming consumer audits remain.
- Direct by-value array-variable assignment inside eval now detaches the boxed
  value while retaining its native payload. Existing shared runtime COW helpers
  perform the eventual split. Tests cover copies, self-assignment, source removal,
  references, and later appends. This does not claim a completed audit of all eval
  expression-result ownership, function returns, ternaries, or nested write paths.

Remaining frontend and lifecycle observations:

- Opaque eval does not parse the `@` suppression operator. Keep this with the
  previously recorded binary literal and scalar-coercion work. Metadata warnings
  and false returns are tested without suppression in eval; native suppression
  already follows the emitted runtime diagnostic policy.
- A repeated eval array write using a fresh literal index retains one temporary
  index cell per write. The bridge/COW ownership test reuses the index and value
  cells to isolate its own owners. The literal-index leak remains part of the
  broader eval temporary-ownership audit, alongside the earlier call-argument cases.
- `mb_list_encodings` remains unbound because PHP's cached list identity affects
  automatic detection ordering. Its eventual native request cache must preserve
  identity across ordinary copies and let mutations detach through COW.
- Array inputs, recursive conversion/checking, detection/settings arrays,
  `mb_convert_variables`, numeric-entity maps, MIME headers, mbregex, callbacks,
  HTTP/output/mail, and complete INI/request/host integration remain required.

Union/array batch verification completed:

- All 31 focused mbstring codegen tests pass together with `ELEPHC_PHP_CHECK=1`.
  This includes the new array/COW lifetime tests, existing scalar/string ownership
  regressions, request-setting optimizer coverage, and five-target compile fixture.
- Eleven real C-ABI tests, 26 neutral contract tests, three real-engine FakeOps
  tests, four mbstring diagnostic matrices, four compiler mbstring contract/runtime
  gates, and all five whole-runtime SysV alignment gates pass. The existing native
  array-alias/eval-write regression also passes after the generic assignment change.
- Clang assembled the complete runtime and the exported program containing all
  33 shared operations for macOS ARM64, iOS device, iOS Simulator, Linux ARM64,
  and Linux x86_64. Executable checks ran locally on Linux x86_64; the other
  executable target checks remain CI responsibilities.
- The extended product-label example compiles and matches PHP byte for byte.
  The compiler and both bridge crates build without warnings.
- Both Rust documentation exporters use the neutral type formatter. The Python
  extraction stage now preserves unions instead of collapsing them back to Mixed;
  three focused extraction regressions pass. The full builtin documentation skill
  workflow passes with 1041 registry entries, 2004 validated reference pages,
  and zero builtin/EIR/target-boundary errors.
- Assembly comments, Rust module preambles, new-content punctuation, PHP fixture
  syntax, and `git diff --check` pass. No formatter, full root test suite, commit,
  PR comment, or PR closure was performed.

Next integration work: recursive array inputs and array-bearing state/detection
APIs, then the recorded eval literal/coercion/suppression and ownership boundaries.
Continue through MIME, mbregex, references/callbacks, and HTTP/mail/host lifecycle;
34 exposed functions is an intermediate checkpoint, not extension completion.

## Recursive array engine and conversion accounting

- Conversion measurement now counts rejected original units and rejected replacement
  fallbacks across every codec. The instrumentation is thread-local and separate
  from PHP request state; nested measurements and unwinding leave later calls valid.
  Only counted request operations accumulate these measurements. The existing
  `mb_scrub` C-ABI path now updates the request's illegal-character counter.
- A reproducible PHP oracle covers 174,748 conversion output/count pairs across all
  79 source and destination names, four input classes, and seven substitution modes.
  All pairs pass. Request tests cover selected-codec counting, failed detection,
  cumulative string/array conversions, reset, and thread isolation.
- Recursive validation and conversion use explicit work stacks. Validation preserves
  the distinction between an invalid key ending its array traversal and an invalid
  value allowing later warnings. Conversion independently detects every string key
  and value, copies scalars, dereferences graph aliases, warns on unsupported values,
  and retains the first converted key while still processing colliding values.
- The array oracle captures 3,670 complete PHP results, warning sequences, and error
  deltas, including all encodings, shared references, cycles, finite reentry after
  PHP clears recursion protection, binary strings, float bits, and numeric-looking
  converted string keys. Every case passes through request state and a wire roundtrip.
- PHP 8.5.10 does not return for a graph with two traversed self-references; bounded
  probes terminate at the process memory limit, and an unrestricted probe crashed.
  This shape is excluded from successful oracle captures unless strict detection
  skips the first malformed key. The engine detects a repeated active protection
  state and returns `NonTerminatingRecursion`, retaining prior error counts. It does
  not fabricate a finite PHP array. The future host adapter must expose this failure.
- The neutral contract owns the validated array graph and its pointer-free wire
  representation. Nodes preserve identity and insertion order; scalar cells preserve
  binary strings and raw float bits. Decoder tests pin independent wire bytes and
  reject truncation, trailing data, invalid tags/reserved payloads, dangling nodes,
  duplicate keys, and impossible allocation counts. A 512-level traversal verifies
  that conversion and checking do not recurse through the Rust call stack.

Integration status remains explicit: `ARG_ARRAY` and `RESULT_ARRAY` reserve the
shared graph wire shapes, but native/eval argument snapshots and result materializers
are not connected yet. No additional PHP function is exposed in this batch. The
surface remains 34 of 65 functions, with eight of nine constants, and all previously
recorded scalar/eval, MIME, mbregex, references/callbacks, and host lifecycle work
remains required. Encoding-list adapters must retain lazy PHP element coercion and
must not invoke PHP callbacks while the Rust request-state borrow is active.
Graph result materialization must preserve converted numeric-looking string keys
without running ordinary PHP key normalization, and must copy float payload bits
without canonicalizing NaNs or signed zero.

The focused eval integration run exposed a source-checkout archive problem:
`libelephc_magician.a` still embedded the previous neutral contract while mbstring
embedded the current one, producing duplicate Rust symbols at final link. The
existing freshness check only scanned each bridge's own crate. Bridge discovery
now also follows transitive local normal/build dependencies, workspace-inherited
paths, and target-specific paths, and watches workspace Cargo configuration and
the lockfile. Dev-only and unrelated crates and nested build output are excluded.
Cargo still decides the actual conditional build work. Synthetic timestamp tests
cover the shared-contract regression and dependency cycles without real builds or
sleep-based ordering. The codegen test builder had a second package-only freshness check; it now
uses the same `bridge_sources` module as CLI discovery. Its prebuilt-CI override
remains authoritative. The fresh-archive eval integration recheck now passes: all six matching native,
eval, callable, diagnostic-order, shared-setting, and ownership tests succeed with
`ELEPHC_PHP_CHECK=1`. The test runner itself refreshed the obsolete Magician archive;
no manual rebuild was used to obtain that passing result.


Recursive-array/accounting batch verification completed:

- Six engine array tests pass, including all 3,670 wire-roundtripped PHP fixtures,
  deep traversal, explicit nontermination with partial counts, and nested values
  discarded by converted-key collisions. The latter independently matches PHP's
  retained array, warning, and four-error count.
- All 174,748 counted conversion comparisons, three request-accounting integration
  tests, two private C-ABI/meter unit tests, and eleven existing C-ABI tests pass.
- Three wire framing tests and both shared source-discovery regressions pass.
  The existing build-output exclusion regression also passes.
- The compiler and engine build without warnings. Six focused codegen tests pass
  with PHP comparison, and a separate actual CLI/native/opaque-eval smoke program
  matches PHP byte for byte after the dependency-aware archive refresh.
- PHP generator syntax, Rust preambles/function docblocks, new-content punctuation,
  and `git diff --check` pass. The compatibility-data README and bridge linking docs
  describe the new engine fixtures and archive freshness behavior.
- No generated PHP signature or emitter changed in this batch. The previously
  verified five-target lowering/assembly paths remain unchanged. No formatter,
  full root test suite, commit, PR comment, or PR closure was performed.

Next priority: connect neutral graph arguments/results to AOT and eval, then bind
array-bearing checking/conversion/detection/settings APIs. Preserve exact key
identity, ownership, lazy element coercion, and request-state callback boundaries.
The original full 65-function extension goal remains active.
## Host array snapshots and public encoding checks

- The neutral host-reader ABI describes borrowed concrete values using fixed
  tag/low/high triples. The Rust snapshot entry walks each distinct array once,
  copies reader strings before the next callback, preserves cycles, shared nodes,
  insertion order, binary keys, integer identities, and raw float bits, and rejects
  malformed metadata or stalled/repeated cursors without publishing a partial graph.
- A target-aware native reader owns indexed strides, tagged nullable scalar arrays,
  hash iteration, Mixed unboxing, null-container normalization, and unsupported
  object/resource values. It neither allocates nor mutates the source arrays.
  Independent C layouts exercise its emitted machine code, then pass the same
  callback to the actual Rust snapshot export and compare the resulting graph.
- AOT and eval copy snapshots into one owned runtime wire string per array argument.
  The status adapter consumes these owners after the Rust operation returns and
  clears their slots before diagnostics or exception construction. Eval's remaining
  scalar-cast cleanup therefore cannot release an array snapshot twice. The raw C
  engine API continues to borrow all input buffers.
- `mb_check_encoding` is connected through one neutral array/string/null contract,
  stable runtime identity 55, AOT semantics, and eval binding. It checks recursive
  keys and values, preserves PHP recursion warnings, resolves encoding errors before
  deprecated null-input warnings, and uses the shared request conversion-error count
  for null/omitted values. Checking invalid strings itself does not increment that
  count. Its native result is a non-owned boolean.
- The fake interpreter's array keys previously converted invalid UTF-8 bytes into
  replacement text. Keys now retain binary bytes through construction, updates,
  sorting, iteration, and the real mbstring C-ABI adapter. Dedicated binary-key and
  existing sorting tests cover this correction.
- Verification: all 30 neutral-contract tests, four snapshot ABI tests, three new
  encoding-check ABI tests, eleven existing ABI tests, four fake mbstring tests,
  one binary-key regression, four fake sorting tests, five compiler mbstring tests,
  five SysV alignment gates, and four diagnostic matrices pass. All 37 focused
  mbstring codegen tests pass with PHP comparison enabled, including ownership,
  optimizer effects, callables, nullable unions, shared request state, and the
  supported-target compile fixture. Clang assembles the complete runtime for all
  five supported targets. The example matches PHP's 749 output bytes.
- The builtin-docs workflow passes completely, including all 2,006 generated-page
  validations and the EIR target-boundary audit. Runtime docs describe snapshot and
  argument ownership. Assembly comments, PHP syntax, all 237 changed/new Rust
  preambles, new-content punctuation, and `git diff --check` pass.

The exposed surface is now 35 of 65 PHP functions: 34 use the shared engine and
`mb_ereg_match` still uses its old implementation. Eight of nine constants are
available. This is an intermediate integration checkpoint. General recursive array
result materialization, array-valued encoding-list coercions, detection/settings
adapters, MIME, mbregex, references/callbacks, numeric-entity adapters, HTTP/mail,
INI/request hosting, and the recorded eval semantic/ownership audits remain part
of the original goal. No formatter, full root test suite, commit, PR comment, or
PR closure was performed.

## Substitution setting and scalar union arguments

- `mb_substitute_character` now joins the neutral contract, stable runtime identity
  56, typed AOT semantics, eval registry, and shared request-state dispatcher.
  Omitted/null input reads the active setting; integers select a valid Unicode
  scalar and named modes retain the remembered replacement character. Failed mode
  or codepoint validation leaves the prior setting intact.
- Native boxed argument staging now reads concrete integer, boolean, string, null,
  and array tags through one dispatcher. Static integer/string unions preserve
  integer codepoints instead of turning them into numeric string modes. Eval and
  its fake adapter likewise distinguish numeric inputs from actual string modes.
- Two C-ABI tests cover scalar boundaries, case-insensitive modes, NUL/invalid mode
  strings, binary zero replacement, remembered characters, thread isolation, and
  request reset. Five new codegen tests cover native/eval union calls, callables,
  named/spread arguments, shared request state, optimizer effects, and ownership.
  The fake interpreter also exercises the real substitution dispatcher.
- Verification passes: all 30 neutral-contract tests, five fake mbstring tests,
  five compiler mbstring tests, five SysV alignment gates, four diagnostic
  matrices, and all 42 focused mbstring codegen tests with PHP comparison enabled.
  The supported-target fixture now includes scalar-union substitution calls.
  Clang assembles all five complete runtimes with operation 56 present. The example
  selects U+FFFD for malformed imported bytes and matches PHP's 751 output bytes.
- The complete builtin-docs workflow passes again: 1,043 registry entries, 2,008
  validated generated pages, and no docs or EIR target-boundary errors. Runtime
  docs describe the shared substitution behavior. All 240 changed/new Rust files
  have module preambles; assembly comments, PHP syntax, added-content punctuation,
  and `git diff --check` pass.

Current public coverage is 36 of 65 functions, with 35 using the shared engine and
one legacy mbregex function, plus eight of nine constants. All previously listed
remaining extension work stays in scope. The central PHP argument-coercion audit
must cover this new union as well: PHP converts booleans and representable floats
to integers, warns on lossy float conversion, falls back to string conversion for
INF/out-of-range floats, and emits its NaN-to-string diagnostic before rejecting
that mode. The existing generic eval casts do not yet provide all those boundary
semantics or strict/object/array TypeErrors. This batch verifies the declared
integer/string/null surface and does not claim that the pending coercion audit is
complete. No formatter, full root test suite, commit, PR comment, or PR closure
was performed.

Next shared-boundary design work should keep type coercion and array snapshots in
separate phases. PHP's outer argument parser may invoke Stringable callbacks while
coercing a later scalar parameter; those callbacks can change referenced values
reachable from an earlier array argument. Capture a PHP ordering regression before
moving snapshots, and ensure array contents are observed after outer parameter
coercions. Encoding-list element coercion is a later, lazy traversal: stop at the
first invalid encoding, retain earlier diagnostics, and read language-dependent
`auto` expansion at the PHP-observable point. No PHP callback may run while the
Rust request-state RefCell is borrowed. This is a design requirement to verify,
not a claim that the current declared-type tests exercise callback coercion.

## Shared coercion planning and lazy list preparation, 2026-09-08

- Added a request-independent parameter planner derived from the neutral contract.
  It preserves actual string/int/null/array union members, implements weak scalar
  conversion and strict rejection, validates complete numeric grammar and signed
  64-bit bounds, and retains PHP's lossy-conversion, null, and NaN diagnostics.
  Binary TypeErrors preserve the actual class name; PHP distinguishes true/false
  in parameter errors and names closed resources simply resource.
- Added versioned C preparation and arity entries, with a 40-byte neutral input
  descriptor and the existing releasable result layout. Deferred Stringable,
  float-formatting, borrowed-string, array, and null results have explicit tags.
  Ordered diagnostics use length-framed level/message records, preserving NUL,
  invalid UTF-8, and embedded newlines. Invalid kinds, flags, counts, ranges, or
  strictness fail closed. Preparation does not borrow request state or invoke PHP.
- Captured 37,100 independent PHP coercion cases, including 2,048 deterministic
  random float bit patterns, 512 generated numeric strings, scalar boundaries,
  nullable and array unions, resources, Stringable callbacks, and throwing casts.
  Every case now matches through both the pure planner and actual preparation C
  API, followed by real operation dispatch. Host float formatting and callbacks
  are explicit fixture-driven actions; production host parity is still pending.
  All 63 captured arity failures for the 35 shared functions also match PHP.
- Captured 108 ordering cases. PHP sees a reference mutation inside a previously
  passed array, but ordinary caller-array COW changes do not alter that argument.
  Scalar arguments passed by value are copied before Stringable callbacks, even
  when the unpacked argument array contains references. Diagnostics can throw
  before later callbacks or mutate settings before later coercions. A prior float
  string is formatted before a later parameter's lossy-conversion warning.
- Refactored encoding-list parsing through an incremental builder. Each array
  element is converted and resolved before advancing, and `auto` observes the
  language active at that element. First failure remains terminal. Shared state
  supplies current defaults separately on each step, so a host can run callbacks
  between steps without holding a request borrow. Thirty-nine PHP list-order
  cases pass through the real parser, state, detector, and converter; the other
  outer/native/eval cases remain explicit integration fixtures.
- Focused preparation, arity, binary-framing, malformed-input, repeated-release,
  request-borrow, neutral-layout, incremental-list, state-transition, detection,
  and automatic-conversion tests pass. No public PHP functions were added in this
  batch, so coverage remains 36/65 functions and 8/9 constants.

Next: migrate native and eval argument adapters to the shared preparation protocol,
including retained caller context, per-call strictness, protected Stringable
execution, diagnostic delivery, cleanup on early errors, and delayed array
snapshotting after outer coercions. The 108 ordering fixtures are the independent
reference for this migration. Then continue the outstanding public conversion,
detection, list/settings, entities, MIME, Oniguruma, references/callbacks,
HTTP/mail/INI/request hosting, and final target/doc/packaging work. This is not a
completed extension. No formatter, full root suite, commit, PR comment, or PR
closure was performed.

## Caller profiles and protected Stringable boundary, 2026-09-08

- Direct mbstring runtime instructions now carry the physical call site's
  `strict_types` state in EIR, separately from strict-PHP extension visibility.
  Statement lowering reinstalls the full source profile, so generated arrow
  function bodies inherit it and included functions keep their own file's mode.
  Callable-wrapper instructions use an explicit deferred caller profile instead
  of capturing their creation site's parameter strictness. Textual EIR exposes
  these values; runtime consumption remains part of the adapter migration.
- Added `__rt_mbstring_stringable`, a C ABI boundary accepting eval context,
  borrowed boxed object, and a neutral 24-byte native string result. It reuses
  existing native/eval string-context dispatch behind a complete PHP handler
  record. Success transfers bytes, length, and the native owner; failure returns
  PendingThrowable and zero output. Handler, activation-frame, and diagnostic
  suppression state are restored before returning to Rust.
- An independent C fixture executes the emitted host machine code with real
  setjmp/longjmp. It checks binary output and ownership, ordinary throws, nested
  throwing callbacks, null inputs/outputs, and restoration of all three saved
  runtime states. The production mbstring argument adapters do not yet invoke
  this boundary; full method/callback/cleanup integration is still required.
- The versioned builtin dispatcher now retains the incoming eval context in a
  callee-saved register on both architectures, with balanced SysV alignment. This
  prepares native/eval argument callbacks to receive the correct active context.
- Focused verification passes: two source-profile regressions (including includes
  and arrow functions), the executable protected-boundary fixture, all eight
  compiler mbstring library checks, ten SysV checks, and two neutral preparation
  layout/framing tests. All 42 mbstring codegen tests pass with PHP comparison.
  The compiler builds warning-free, and Clang assembles the complete runtime,
  including the new boundary, for all five supported targets. The architectural
  builtin/EIR audit reports no structural errors.
- A direct PHP probe confirms that eval code without its own strict_types directive
  uses weak internal-parameter conversion even when invoked from a strict file.
  An explicit directive inside the eval fragment changes that behavior. Preserve
  this distinction when the remaining Magician source-profile work is connected.

Next production work remains the actual argument-adapter migration: concrete host
value descriptors (including native/eval class names and Stringable capability),
shared preparation calls, caller strictness, ordered diagnostic delivery, deferred
host actions, and cleanup before an early error. Array snapshots must follow all
outer coercions, while scalar argument values must already be copied before any
callback can mutate referenced caller storage. The original complete-extension
scope and public coverage (36/65 functions, 8/9 constants) remain unchanged.


## Concrete native/eval input descriptions, 2026-09-08

- Added a C ABI classifier preserving concrete scalar kinds and float bits,
  binary strings, opaque arrays, nullable containers, resources, and Closure
  identity. It neither casts values nor traverses arrays or executes Stringable.
- Native metadata tables provide bounded class-name and string-conversion lookup.
  Dynamic eval objects use the caller context and existing metadata-only APIs;
  acquired class-name and error cells are released on malformed or failed lookup.
  Successful dynamic descriptions transfer one metadata owner for the argument
  adapter to release after the planner copies class names and diagnostics.
- An independent C fixture executes the real emitter and Mixed unboxer through
  the real Rust preparation ABI. It covers binary native/eval class names,
  Stringable flags, nested Mixed cells, nullable containers, integer sentinel
  collisions, NaN bits, opaque arrays, malformed lookup results, missing context,
  invalid output pointers, and balanced cleanup of acquired metadata.
- Nine compiler mbstring library tests and ten SysV checks pass; the compiler
  builds warning-free. Clang assembles the complete mbstring/eval runtime for all
  five supported targets. Production argument-adapter migration remains pending.


## Shared invocation coordinator and callback-order integration, 2026-09-08

- Added elephc_mbstring_invoke_v1 and a versioned 72-byte host callback table.
  The coordinator validates arity before reading host pointers, copies all PHP
  argument values before coercion, uses the existing pure parameter planner,
  copies borrowed metadata before releasing it, delivers ordered diagnostics,
  executes deferred Stringable/float actions, then snapshots arrays and dispatches
  through the existing request owner. No callback retains a request-state borrow.
- Host owners live in an explicit arena outside the panic-catching closure.
  Callback outputs are recorded before status/metadata validation; all cleanup
  continues after release errors. Pending PHP exceptions survive later fatal
  cleanup statuses. Rust destructors never execute PHP callbacks.
- All 66 independently captured PHP outer-order cases pass through the real
  invocation C ABI with a separate host model. Results, callback/diagnostic traces,
  reference observers, array COW, precision order, and final request settings match.
  Every host callback reenters the real request API. Stringable and float actions
  remain fixture implementations, so this does not claim production host parity.
- Fifty-four callback failure/malformed-success cases pass with no retained owners,
  including failures that publish an owner, reader errors, and failing releases.
  Invalid tables, null outputs/inputs, unsupported IDs, strictness, null values,
  and wrong arity before poisoned pointer reads are covered. A held request borrow
  provokes a contained Rust panic after argument copies; cleanup succeeds and the
  next call works. The neutral callback-table layout test also passes.
- The compiler builds warning-free with the new callback contract and coordinator.
- Existing preparation, arity, and lazy-list regressions remain green, including
  37,100 scalar coercion fixtures and 63 PHP arity messages. The earlier concrete
  input batch also passes all 42 mbstring codegen tests with PHP comparison and
  the builtin/EIR architectural audit, with aligned assembly comments.

Next: install real host callbacks and migrate production AOT/eval adapters to the
coordinator, including source strictness and ArgumentCountError materialization.
Native release needs careful destructor handling: merely catching longjmp around
an entire deep-free walk may skip its remaining payload releases. Audit and test
that boundary before claiming cleanup parity. Existing operation diagnostics are
still delivered from result buffers after dispatch and need their own ordering
integration. The complete extension remains unfinished at 36/65 public functions
and 8/9 constants, with conversion/detection/list/settings, entities, MIME,
Oniguruma, references/callbacks, HTTP/mail/INI/request hosting, and final target/docs/
packaging work outstanding. No formatter, full root suite, commit, PR comment, or
PR closure was performed.


## Production eval invocation and constructor ownership, 2026-09-08

- All 35 shared eval operations now use `__rt_mbstring_invoke` and the real
  72-byte host callback table. The generated adapter supplies value cloning,
  concrete metadata, protected Stringable execution, native float formatting,
  diagnostic delivery, protected release, and nonmutating array iteration.
  Results transfer into the existing native materializer without leaking or
  double-releasing Rust buffers. Generic casts no longer normalize these calls.
- Dynamic objects resolve their owning eval context before native class lookup.
  This preserves eval class names and Stringable metadata even when their native
  storage uses the ordinary stdClass payload. Native and eval Stringable methods
  execute against the active context and may change mbstring request settings
  before the outer operation dispatches. Their exceptions remain catchable.
- Invalid arity reaches the coordinator after source argument expressions run.
  The materializer creates ArgumentCountError, and direct calls plus the evaluated
  call_user_func route match PHP messages and side-effect ordering.
- Repeated throwing Stringable calls exposed independent eval ownership defects.
  Existing string views now borrow actual string bytes. String-context exception
  transfer releases its temporary boxed Throwable. Argument packing releases its
  boxed keys, and native constructors borrow normalized argument cells from the
  private array instead of allocating keys and retaining each read.
- Constructor initialization releases replaced Throwable message/previous fields.
  Native method/constructor binding tracks materialized defaults and releases
  them after writeback or failed binding. New-object evaluation releases directly
  allocated literal arguments on success, argument failure, and constructor failure.
  Other expression temporary ownership, compound-default internals, and unrelated
  eval call paths still need their own ownership audit.
- Dynamic property assignment retains the borrowed value for its property hash.
  Constructor Mixed reference slots own their staging value independently of the
  caller. Unchanged slots release that staging owner; changed slots transfer the
  replacement into the caller cell. This preserves mixed/nullable by-reference
  writeback while avoiding the retained argument-read owners.
- The compact Throwable constructor emitter now has its own focused source file.
  Assembly comment coverage/alignment passes for every emitter touched in this
  integration, and all 292 touched/new Rust files have module preambles.
- Verification passes: 46 mbstring codegen tests, 37 dynamic-constructor codegen
  regressions, five eval Stringable regressions, nine native mbstring fixtures,
  ten SysV checks, two constructor ownership unit tests, and two additional
  native constructor GC tests. PHP cross-checking was enabled for the mbstring,
  constructor-regression, and Stringable batches. The compiler builds without
  warnings; builtin/EIR boundary and diff hygiene checks pass.
- Clang assembles both the complete mbstring/eval runtime and generated user
  assembly containing native constructors, Mixed reference writeback, property
  setters, and native/eval Stringable calls for all five supported targets.
  The iOS probe uses a native-only export: the existing library-boundary checker
  correctly rejects an exported function that reaches eval.

Host gaps at this checkpoint included the old production AOT wire adapter (replaced
in the integration below). Eval still supplies weak mode rather than fragment strictness,
float formatting has fixed runtime precision, diagnostics do not dispatch PHP
error handlers, eval destructor exceptions are swallowed by the existing callback,
and a protected longjmp does not yet finish an interrupted deep-free walk. The
FakeOps mbstring adapter also retains its earlier generic conversion path.
Source argument value capture before later argument side effects and general
eval argument temporary cleanup also need explicit production coverage.
The complete extension remains unfinished at 36/65 functions and 8/9 constants.
The remaining conversion/detection/list/settings, numeric entities, MIME,
Oniguruma, references/callbacks, HTTP/mail/INI/request hosting, and final target/docs/
packaging work remains in scope. No formatter, full root suite, commit, PR comment,
or PR closure was performed.

## Direct AOT invocation, value capture, and exception cleanup, 2026-09-08

- All 35 shared AOT operations now call the same invocation coordinator as production
  eval. Extended stack-local invoker descriptors carry concrete tags, raw values,
  caller strictness, optional eval context, and exceptional capture ownership.
  The old wire API remains independently callable and retains its documented ownership.
- `PreserveValues` replaces the narrower nullable-only argument strategy. Shared
  argument planning keeps names, defaults, arity, reference modes, and source order,
  while a storage-only signature prevents premature Mixed-to-string or scalar casts.
  The neutral PHP signature remains authoritative for checking and documentation.
- Weak AOT checking admits PHP scalar conversion and runtime Stringable checks.
  Strict source mode rejects known invalid types and preserves dynamic original
  values for catchable runtime TypeError. Existing static-error regressions now
  explicitly request strict mode where weak PHP conversion is valid.
- Source argument capture copies Mixed cells and pins concrete heap payloads before
  later argument expressions can replace the original variables or mutate arrays.
  EIR now classifies `MixedClone` as an owning temporary for normal call cleanup.
- Captured owners previously leaked when the shared parser threw. Extended AOT
  descriptors record those owners; the native wrapper clears and releases each
  through protected callbacks before propagating a failed invocation. Normal calls
  retain ordinary EIR argument cleanup. The wrapper has ARM64 and SysV implementations.
- Mbstring effects retain `WRITES_GLOBAL` because Stringable conversion can execute
  arbitrary PHP methods. A focused optimizer regression observes global and request
  encoding changes from an otherwise discarded mbstring call.
- Five focused AOT regressions pass with PHP cross-checking: weak conversions and
  errors, strict dynamic inputs, named/spread evaluation order, source value capture,
  and repeated successful/throwing capture ownership. The source-capture fixture
  covers strings, Mixed replacement, indexed-array COW, and named arguments.
  Broader post-integration verification is recorded after completion below.

Remaining integration work includes caller-source strictness for dynamic wrappers
and eval fragments, pre-adapter argument-evaluation exception cleanup, general
user-call argument boxes on exceptional exits, eval source-value capture and general
expression-temporary ownership, float precision/INI, PHP error-handler delivery,
and destructor/deep-free exception completion. Dynamic spread omission and arity
paths need explicit production auditing beyond the shared parser itself.
The extension still exposes 36/65 functions and 8/9 constants.

Pre-adapter ownership probe: `/tmp/elephc-mbstring-capture-before-entry.php`
passes a retained string before a second argument that throws RuntimeException.
One versus 24 iterations produce GC residuals 1 and 24, confirming that an
exception before `__rt_mbstring_native` still bypasses capture cleanup. The
post-entry regression is balanced; this earlier exit requires shared argument
unwinding support rather than another engine-specific conversion path.

Post-integration verification passes: 52 mbstring codegen regressions, 54 shared
named-argument regressions, ten mbstring static-error tests, and three mbstring
EIR lowering/profile tests. PHP cross-checking was enabled for the executable
regressions. The final additional weak numeric-string rejection case passes too.
The compiler builds without warnings. Clang assembles both current runtime and
user assembly for all five supported targets, including captured Mixed and native
Stringable paths. The updated ProductLabel example matches PHP output exactly.
The builtin exporter, generated pages/registries/module sections/PHP comparison,
docs audit, site compatibility validation, and builtin/EIR architecture audit all
pass. Assembly comment alignment and diff hygiene pass.

Next concrete ownership task: protect captured source arguments when evaluating a
later argument throws before the mbstring adapter can consume the failure path.
Keep the existing balanced post-entry ownership regression while adding the
pre-entry regression. Then finish dynamic-wrapper/eval caller strictness and return
to the remaining public mbstring operations.


## Shared argument exception guards, 2026-09-08

- The pre-adapter leak recorded above is fixed for captured strings, Mixed cells,
  arrays, native objects, and callable descriptors. The original one-versus-24
  string probe now reports allocs/frees of 4/4 and 73/73. Fresh closures exposed a
  separate typed-release requirement: generic heap release ignores callable
  descriptors. Guard callbacks now select the dedicated descriptor release path.
- Typed EIR `ExceptionGuardOwned` and `ExceptionUnguardOwned` use fixed 32-byte
  frame records with the shared exception-activation prefix and one captured owner.
  Guards register immediately after capture and unlink before normal argument
  cleanup. The mbstring adapter's previous descriptor-owned cleanup loop was
  removed; failures before and after entry now use the same exception walker.
- Insertion anchors preserve PHP parameter-order destruction independently of
  named argument source order. Nested call groups remain independent. Try handlers
  save the surviving activation head so an inner catch preserves outer captures.
  The walker publishes the previous activation before callbacks, preventing a
  reentrant destructor from processing an already-consumed record.
- The protected callback emitter now belongs to common exception support and is
  reused by mbstring host callbacks. It restores handler, activation, diagnostic,
  and GC suppression state after a PHP exception and continues to later guards.
  It does not claim to finish an interrupted object's deep-free walk.
- Six focused guard regressions pass with PHP comparison: pre-entry ownership,
  nested catches, destructor timing, reordered successful arguments, parameter
  cleanup order with a throwing destructor, and fresh closure capture ownership.
  The earlier five pass with EIR optimization disabled too. The shared integration
  batch passes 57 mbstring codegen tests, the guard-shape validator, three mbstring
  lowering/profile tests, and focused caught-exception/finally regressions.

Still pending: dynamic spread omission/arity and temporary ownership, source
strictness for dynamic wrappers and eval fragments, eval source-value capture,
general call-temporary cleanup, destructor/deep-free exception completion, float
precision/INI, and PHP error-handler delivery. The public extension still exposes
36/65 functions and 8/9 constants. Conversion/detection/list APIs, numeric entities,
MIME, Oniguruma, references/callbacks, HTTP/mail/INI/request hosting, and final
packaging/target/documentation verification remain required. PRs #895, #898,
#899, #900, and #902 remain tracked for the later superseding comment. No formatter,
full root suite, commit, PR comment, or PR closure was performed.


## Encoding catalog public binding, 2026-09-08

- `mb_list_encodings()` now has one neutral zero-argument contract, AOT/eval home
  files, and typed runtime ID 57. It uses the existing invocation coordinator,
  canonical engine catalog, and owned string-array result materializer.
- Native and opaque eval regressions compare every name and its exact order with
  the independent PHP reflection snapshot. Namespaced/case-insensitive lookup,
  first-class callable results, call_user_func, array copies and mutations,
  unchanged request settings, and catchable eval excess-argument errors pass.
- Repeated-result GC checks pass for AOT and eval with reused mutation operands.
  An initial probe with fresh eval index/value literals confirmed the previously
  recorded general eval temporary leak, three allocations per mutation. This is
  separate from catalog/result ownership and remains unfinished.
- The public surface now exposes 37/65 functions, with 36 using the shared engine.
  This binding does not yet implement PHP's cached native list identity. That
  identity and mutation-driven COW detachment must be connected before declaring
  automatic-detection/list integration complete. The previous list-identity
  observations and detection fixtures remain authoritative.
- The common exception-guard checkpoint passed the final warning-free compiler
  build, callable-guard test with optimization disabled, recoverable cdylib string
  boundary test, all five target runtime/user assembler checks, builtin/EIR audit,
  generated-site compatibility validation, assembly alignment, and diff hygiene.

Next required integration retains dynamic-spread arity/omission/ownership,
callable/eval caller strictness, and cached catalog identity for detection, plus
all remaining functions, constants, reference/callback and request/host semantics.
No goal completion, commit, PR comment, or closure was requested or performed.


Catalog binding final checks all pass: three PHP-cross-checked codegen regressions,
array-contract static errors, the exported catalog ABI test, stable runtime-ID
round trips, the warning-free compiler build, and the complete builtin-docs skill
workflow. Both generated pages and registry metadata declare the same zero-argument
array contract and shared runtime ID 57. All 311 touched/new Rust source files have
module preambles; assembly comments and diff hygiene pass. Clang assembles current
runtime and user catalog calls for all five targets, and the updated product-label
example matches PHP exactly.

Concrete dynamic positional-spread regressions confirmed after these checks:

- `/tmp/elephc-mbstring-dynamic-spread-excess.php` passes a runtime-selected array
  containing `"a", "UTF-8", "extra"` to `mb_strlen(...$args)`. PHP catches
  `ArgumentCountError` with `mb_strlen() expects at most 2 arguments, 3 given`.
  Current AOT silently drops the excess element and returns `int(1)`.
- `/tmp/elephc-mbstring-dynamic-spread-missing.php` selects an empty runtime array.
  PHP catches `ArgumentCountError` with `mb_strlen() expects at least 1 argument,
  0 given`. Current AOT exits one with `Fatal error: too few arguments for spread
  call`. The shared invocation coordinator already produces the correct errors;
  `src/ir_lower/expr/positional_spreads.rs` currently pads/truncates static operand
  slots and emits its own underflow fatal before that coordinator can see the
  actual supplied count. The next fix must preserve that count through shared
  argument planning and typed EIR, including capture ownership and named spreads.


## Dynamic positional invocation, 2026-09-08

- The two recorded `mb_strlen(...$args)` probes now reach shared arity validation
  with their actual expanded counts. Excess values produce the exact catchable
  maximum-arity error; empty arrays produce the exact catchable minimum-arity
  error instead of the old native fatal.
- Profiled EIR runtime calls now carry `RuntimeArgumentLayout`, defaulting to
  separate values. The indexed-array layout passes one owned array of copied
  Mixed cells to the same native coordinator with physical caller strictness.
  The validator rejects unboxed arrays and runtime targets without this strategy.
  Textual EIR exposes `arguments=indexed-array`; target identity, requirements,
  monitoring, result ownership, and PHP signatures remain descriptor-driven.
- Shared call planning supplies source expressions. Positional values and each
  spread element are copied before later expressions run; multiple indexed
  spreads retain every argument. Known array-returning function calls use the
  shared indexed-spread type discovery. Direct and statically resolved builtin
  callable calls select the same packed route.
- A generic array-guard refresh follows reallocating appends, preserving cleanup
  after container growth. Four PHP-cross-checked regressions pass: exact arity
  including zero-parameter calls and callable aliases, source capture and mixed
  result representations, strict dynamic scalar rejection, and repeated normal/
  arity/pre-entry exits with twelve-element containers that force growth.
- Static builtin arity checks no longer count each unresolved spread as one
  argument for `PreserveValues`. The mbstring checker still infers every source,
  including zero-parameter calls, and leaves uncertain parameter positions to the
  runtime planner. Extra ordinary arguments retain their existing static errors.

Known remaining spread boundaries: dynamic named/associative normalization,
parameter holes and overwrite errors, opaque callable dispatch, and runtime
argument sources whose types require a separate dynamic container strategy.
Public coverage remains 37/65 functions and 8/9 constants; the full original
mbstring scope remains required, including cached catalog identity for detection.

A destructor probe also confirms the previously recorded interrupted deep-free
problem affects packed argument containers. The pre-entry PHP fixture at
`/tmp/elephc-mbstring-packed-destructor-before-entry.php` expects first, second,
destructor, after: a throwing first object's destructor must not skip the second
object. Native deep-free completion still requires its own fix. The post-entry
variant additionally depends on PHP's retained exception argument traces; with
`zend.exception_ignore_args=1` it isolates the same cleanup-order requirement.
These observations are not a claim of complete destructor or exception parity.


## Callable-only runtime requirements, 2026-09-08

The `named_args` regression group exposed undefined `__rt_mbstring_native` and
`__rt_mbstring_box_result` references in generated PDO callback wrappers. Generic
string dispatch emitted every mbstring wrapper despite the runtime feature being
disabled. Wrapper eligibility now consumes the module runtime feature set and
omits the optional mbstring family until direct use, eval, or explicit capability
enables it. The focused PDO named-createFunction regression passes again.

First-class callable checker requirements are retained in EIR metadata. Feature
discovery also reuses the existing finite callable-name analysis at descriptor,
normalization, indirect invocation, and typed callback operand sites. This enables
mbstring for programs whose only reference is a first-class descriptor or a
runtime-selected finite name, without adding the bridge for unrelated callbacks.
The CLI `--with-mbstring` flag now enables runtime emission as well as force-linking
the archive, supporting a callable name read from the environment.

Focused regressions pass for both callable-only forms, explicit capability with
an opaque name, and EIR feature presence/absence. Full scoped regression and docs
verification results follow after the current sequential check run completes.
The wrapper argument/coercion and caller-strictness gaps recorded above remain
separate requirements; this correction addresses feature discovery and linking.


## Packed array ABI and literal storage follow-up, 2026-09-08

The product-label example now supplies its optional substring parameters through
an indexed spread. It exposed two independent errors omitted by the first four
spread fixtures:

- Native heap arrays need not be pointer-aligned. The recorded failing array was
  at `_heap_buf + 1612`; borrowing its pointer payload as a Rust slice triggered
  a non-unwinding unsafe-precondition panic. GDB confirmed the actual address,
  operation 30, and count four. The adapter now copies borrowed argument-cell
  pointers into aligned stack storage bounded by the neutral contract's maximum
  arity, while preserving the actual count for shared diagnostics. Array ownership
  and individual argument values stay with the original guarded container.
- Array literal element inference ignored builtin result metadata and used a
  syntactic integer fallback. `[0, 4, mb_internal_encoding()]` consequently stored
  zero instead of the returned encoding string. Indexed and associative literal
  inference now consults the shared builtin result resolver, preserving boxed
  union values before spread packing. Dedicated literal regressions and the
  complete example now pass against PHP with the original four spread tests.

The broader `array_literal` group then caught another optional-runtime boundary:
compiled eval fragments using only native scopes emitted mbstring dispatcher arms
without mbstring helpers. Runtime features are now threaded through the eval value
wrapper facade and dispatcher; absent capabilities omit both the optional branch
labels and their bodies. A focused emitter test checks the omission on all five
targets. This preserves the native-scope route instead of force-linking Magician
or mbstring solely to satisfy unused symbols.


Verified results after the follow-up fixes:

- 67 mbstring codegen tests and 46 named-argument tests passed after the callable
  requirement fixes, including the original PDO regression. The subsequent ABI
  and literal fixes pass all six spread/literal tests against PHP, both with
  ordinary EIR optimization and with `ELEPHC_IR_OPT=off`.
- All 63 `array_literal` codegen tests pass after the native-eval-scope dispatcher
  correction. The four caller-profile/requirement EIR tests, three dispatcher
  unit tests, and five-target scope-only emitter regression pass.
- `cargo build -p elephc` and the curl-enabled builtin exporter build pass.
  Full generated builtin extraction/module/comparison regeneration and all three
  docs/boundary audits pass, including 2,010 generated builtin pages.
- Runtime and user assembly compile with clang for macOS ARM64, iOS device ARM64,
  iOS Simulator ARM64, Linux ARM64, and Linux x86_64. Updated user fixtures include
  growing/excess arrays, function-returned spreads, and composed getter results.
  This is assembler evidence for the other targets, with local execution on
  Linux x86_64. Runtime source comment alignment and `git diff --check` pass;
  all 325 touched or new Rust files start with their module preambles.
- The real example matches PHP byte-for-byte (799 output bytes). The compact
  loop at `/tmp/elephc-mbstring-packed-alignment.php`, which interleaves sixteen
  string allocation sizes with composed spread parameters, also matches PHP
  (167 output bytes).

The pre-entry destructor probe has now been run natively after all fixes. Both
programs exit zero. Native output is `first\ndestructor\nafter`; PHP output is
`first\nsecond\ndestructor\nafter`. The next ownership correction must finish
container cleanup after the first destructor throws, including the container and
individual object storage and nested cleanup paths. No current passing test or
ABI fix closes that requirement. Full public coverage remains 37/65 functions
and 8/9 constants; all remaining function families and host integrations remain
in scope. No PR comments, commits, or PR closures were performed.


## Resumable native deep release, 2026-09-08

This update supersedes the earlier interrupted native deep-free observations.
Native indexed arrays, hashes, Mixed cells, objects, and callable descriptors now
own a scoped pending-exception flag and their incoming GC suppression state.
Potentially throwing child releases run behind the existing complete protected
PHP handler. Cleanup suspends the prior pending Throwable during each child,
allowing independent nested destructor catches. It completes remaining children
and releases the enclosing storage before propagating the newest exception.

Escaping exceptions consume the suspended prior owner into their previous chain.
A shared ancestor scan avoids self-links and intersecting-chain cycles. The first
PDO probe exposed a storage distinction: compact heap-kind-6 Throwables store a
raw previous pointer, whereas ordinary subclasses, including PDOException, store
a boxed nullable property at the inherited offset. Shared read and insertion
helpers now respect both representations; native getPrevious lowering uses the
same reader. Nullable insertion allocates its own cell and transfers exactly one
child owner without changing aliases of the replaced null cell.

Eight new focused regressions pass against PHP, with EIR optimization both enabled
and disabled. They cover nested object properties, hashes, closure captures,
inner destructor catches, complete previous chains, intersecting previous chains,
PDO and custom subclass storage, successful-call source destruction, and stable
outstanding allocations across one versus twenty-four repeated failures for both
compact and boxed previous storage. The original pre-entry probe now executes the
second destructor before reaching the catch.

Additional checks completed:

- Six existing mbstring argument-guard tests pass.
- Twenty-one focused COW/cycle ownership tests and the existing exception
  constructor previous-parameter regression pass.
- The all-runtime SysV call-alignment gate, seven affected free-emitter tests,
  and the recoverable owned-string cdylib boundary regression pass.
- Twenty-two of the twenty-three existing `destruct` tests pass. The remaining
  `test_eval_dynamic_object_runs_destructor_after_cycle_collection` outputs
  `after` instead of its expected `drop:A:after`. Repeating that exact test with
  all five deep-release emitters restored to HEAD reproduces the same failure.
  All five working files were subsequently restored byte-for-byte. This result
  establishes that the failure predates this scoped-cleanup correction; it does
  not establish complete eval/cycle parity or remove that open requirement.

The custom-subclass probe also encountered the existing unsupported inherited
constructor path when a subclass adds properties without declaring its own
constructor: codegen reports `constructor call to Exception::__construct without
an emitted EIR method body`. The storage regression declares its constructor
explicitly, as PDO does. The inherited-constructor path remains an independent
open compiler issue.

Remaining cleanup boundaries include eval destructor exception propagation,
cycle-collector destructor phases, resurrection, general function-local cleanup
callbacks, and PHP's retained exception argument traces. None is closed by the
ordinary container traversal fix. Full public coverage remains 37/65 functions
and 8/9 constants, with all previously recorded mbstring families, request-host
integration, and target/packaging requirements still in scope. No commits, PR
comments, or PR closures were performed.


Final verification for this cleanup increment:

- `cargo build -p elephc` passes without warnings after restoring the working
  implementation from the controlled baseline comparison.
- Clang assembles updated runtime and user fixtures for all five supported
  targets. A second runtime-only fixture also enables Fiber and Generator so
  their conditional release paths are assembled on every target.
- The original standalone PDO reproducer now exits successfully and matches
  PHP exactly: `PDO\nsource\n`.
- Runtime assembly-comment alignment and `git diff --check` pass. All 335 touched
  or new Rust files retain module preambles.
- PHP 8.5.10 itself prints `afterdrop:A:` for the open eval-cycle fixture when
  deprecation output is suppressed. The existing test expectation `drop:A:after`
  also needs reconciliation with PHP's shutdown/cycle timing; native currently
  omits the destructor entirely. The controlled baseline comparison establishes
  only that this behavior predates the scoped-cleanup changes, not that it is
  acceptable or PHP-equivalent.

Other-target validation here is assembly validation; executable testing in this
workspace remains Linux x86_64. CI must supply the remaining supported execution
and iOS compile/emitter matrix. Full mbstring implementation remains active.

## Detection-order binding and incremental encoding-list host ABI (2026-09-08)

Added the shared `mb_detect_order` contract, AOT and eval homes, stable runtime
operation 58, and request-state dispatch. The getter returns canonical names;
setters accept a comma-separated string or an array, ignore array keys, preserve
ordered duplicates, and commit only after complete validation. Outer scalar
coercion, named arguments, callable lookup, arity, and strict direct-call checks
continue through the shared contract and invocation coordinator.

The encoding-list coordinator now copies, casts, resolves, and releases one entry
before advancing to the next. `auto` observes the language active after that
entry's Stringable callback. The first invalid name or thrown conversion prevents
later callbacks. Rust request-state borrows do not span host callbacks or cleanup.
Scalar entries use ordinary string-cast rules independently of caller strictness;
floats use the host formatter and arrays/objects/resources use its protected
string context. State changes made by a callback remain observable even when the
outer list is rejected.

`MbInvokeHostV2` preserves the complete 72-byte V1 prefix and adds an owned-entry
callback at offset 72, producing an 80-byte table. Original V1 callers continue
working for existing operations and detection-order getter/string-setter calls;
encoding-list arrays require V2 and fail closed without that capability. Entries
published during callback failure are still released. Cursor repetition and
malformed result metadata fail closed. The generated native/eval host supplies
the V2 callback on both architectures. Raw array iteration preserves original
Mixed cell identity, so a resource cast retains its existing resource owner;
ordinary graph snapshots retain their previous normalized-tag behavior.

The native codegen regression covers a Stringable closure that mutates a later
local reference. A separate eval regression creates the equivalent reference
using eval's supported reference-array literal syntax. It currently fails: the
eval context indexes array-element reference metadata by the original boxed-array
handle, whereas invocation clones that box. The native reader therefore sees the
stale stored value rather than the updated eval reference. The dedicated test
`test_mbstring_detect_order_eval_later_reference` is temporarily ignored with this
exact reason. This is an open bridge requirement, not claimed eval reference
parity. A future fix must retain source reference metadata across the argument
copy and resolve each reference at entry-read time, without holding a Rust context
borrow across Stringable execution.

A second independent compiler issue was isolated while constructing the native
fixture. Binding a global name to an array element releases its previous global
cell and writes the reference pointer only into a local slot. A later method's
`global` store still accesses that released global cell. The mbstring argument
copy can reuse its address, exposing corruption during the callback. The exact
reproducer is retained in `.plans/mbstring-probes/detect-order-global-reference.php`.
A local reference captured by a closure behaves correctly and supplies the native
incremental-read regression. Global reference storage still needs its own fix.

The example now configures and displays preferred import encodings. Runtime docs
explain V1/V2 compatibility, lazy entry coercion, and both reference limitations.
Public exposure is now 38/65 functions, with 37 routed through the shared engine;
constants remain 8/9. Exposure counts do not imply completed PHP parity. MIME,
Oniguruma, conversion/detection bindings, entities, HTTP/mail/INI, request hosting,
remaining reference/callback and ownership behavior, and packaging stay in scope.
The five tracked open mbstring PRs retain their previously recorded heads; no
commits, PR comments, or PR closures were performed in this increment.

Verification completed for the detection-order increment:

- Six shared-invocation tests pass, including V1 compatibility, V2 rejection,
  callback-by-callback fatal/pending fault injection, and copied-entry ownership.
- Nine shared mbstring ABI tests, twelve native runtime tests, the complete-runtime
  SysV call-alignment gate, and the new signature diagnostic test pass.
- The focused `test_mbstring` codegen group passes: 75 tests, zero failures, and
  the one explicitly pending eval-reference regression ignored.
- All seven active detection-order regressions also pass with
  `ELEPHC_IR_OPT=off ELEPHC_PHP_CHECK=1`. The ignored eval-reference case was run
  before exclusion and demonstrably fails with the stale `bad` encoding value.
- `cargo build -p elephc` and the curl-enabled builtin exporter build pass without
  warnings. The full generated-docs workflow passes, including the builtin audit,
  site compatibility validator, and enforced target-architecture boundary audit.
  The renderer reports 1,991 generated builtin pages and no audit errors.
- Clang assembles runtime and user code including detection-order Stringable
  arrays for all five supported targets. Runtime execution here is Linux x86_64;
  this assembly evidence does not replace CI execution on the other targets.
- The updated mbstring example produces exactly the same 845 stdout bytes as
  PHP 8.5.10, with no stderr from either executable.
- Assembly-comment alignment, all 341 touched/new Rust module preambles, and
  `git diff --check` pass.

The original complete-extension goal remains active. The two reference failures,
previously recorded cleanup and strictness gaps, and every unimplemented public
family remain open; no release-completeness claim is made by this increment.


## Eval array references across mbstring callbacks (2026-09-08)

The previously ignored detection-order eval reference regression is now active and
passes. The V2 array callback receives both the retained by-value array copy and
an opaque original-identity token. Magician resolves current reference targets at
entry-read time without dereferencing that identity token. Variable, nested-array,
instance/static property, property-alias, and invoker-slot reads produce owned
values; intermediate owners remain in an explicit arena until protected cleanup.
The context borrow ends before native ownership callbacks run. An eval Throwable
is published into native pending storage before cleanup, and native unwinding
starts only after the Rust FFI returns. Pending exceptions take precedence over a
later fatal cleanup status.

New regression coverage includes late updates through local references, nested
arrays, eval and native properties, static aliases, resources, captured-reference
COW, and repeated-read allocation balance. A native executable C/assembly fixture
checks Throwable publication, retention, ownership consumption, malformed inputs,
and success/fatal/pending release statuses. The boxed original identity is tested
as an opaque token with no corresponding fake runtime cell.

Verification passes: 81 mbstring codegen tests with none ignored; all 13
detection-order regressions with EIR optimization disabled and PHP cross-checking;
six bridge invocation tests; nine ABI tests; thirteen native runtime tests;
SysV call alignment; the Magician reference test, six promoted-reference tests,
and twenty property-hook tests. Compiler and curl-enabled exporter builds are
warning-free. Generated documentation, builtin/site audits, and the enforced EIR
target boundary all pass. Clang assembles runtime and user fixtures, now including
opaque eval reference arrays, for every supported target. Execution evidence in
this workspace remains Linux x86_64.

The native global-reference storage failure recorded above remains open. Another
independent eval scope failure is preserved in
`.plans/mbstring-probes/detect-order-eval-global-array.php`: a method declared in
opaque eval does not update the array global freshly created in that same eval
fragment. Native/PHP print `bad` after the callback; eval retains `UTF-8`. The COW
regression uses a captured local reference and passes for both backends. Original
identity lifetime/address reuse, escaping reference targets, and property/container
rebinding still need audit; this increment does not establish complete reference
parity. Public coverage remains 38/65 functions and 8/9 constants. The complete
extension goal stays active, with no commits or PR actions performed.


## Numeric-entity public bindings and map coercion (2026-09-08)

Added neutral contracts and both backend homes for `mb_encode_numericentity` and
`mb_decode_numericentity`, using stable runtime IDs 59 and 60. Both call the
existing shared entity engine after common outer parameter planning. Map elements
use the V2 protected reader, ignoring keys and retaining each value before its
integer conversion. Encoding validation precedes map length, which must be a
multiple of four, followed by ordered element conversion even for empty strings.
The chosen encoding is fixed across callbacks; substitution is read after the map
has finished preparing. Direct wire calls use the same engine and map-conversion
rules with owned result diagnostics.

The shared map integer planner follows PHP's arithmetic conversion rules rather
than scalar parameter coercion: null/bool values convert without null warnings,
numeric prefixes can warn, finite float overflow wraps, numeric strings saturate,
nonfinite numbers preserve their diagnostic sequence, and unsupported values fail
without invoking Stringable. The common numeric scanner now supplies either a
complete numeric parameter or a leading map-number prefix; the existing parameter
coercion fixture remains green. The reproducible `entity_maps.json` capture covers
86 PHP calls, each checked with strict and weak invocation profiles. Protected-host
tests cover later-reference changes, fixed encoding with updated substitution,
diagnostic exceptions, and owner cleanup after injected callback failures. Native
PHP error-handler routing remains an existing open host requirement; mock-host
callback coverage is not a claim that production supports it yet.

The public callback fixture exposed an independent native representation bug:
a closure's writable reference was widened to Mixed while its source array still
stored raw integers. Writing through the capture placed a box pointer into that
integer slot. The reproducer failed even with every mbstring call removed.
Element-reference lowering now normalizes local indexed-array storage to Mixed
and separates existing COW copies before binding. The standalone regression covers
integer/string/null writes and an unaffected earlier array copy. All five focused
array-element reference tests pass with PHP cross-checking.

Opaque eval's existing lexer cannot express non-ASCII escaped bytes faithfully:
`\xe9` becomes UTF-8 `c3a9` rather than byte `e9`. The isolated repro is
`.plans/mbstring-probes/eval-binary-escapes.php`. Binary entity tests construct bytes
with `chr()` so they test the actual binary runtime/codec contract in both
backends. Hexadecimal numeric literals also remain outside the current eval
parser's accepted syntax; the shared entity fixture uses equivalent decimal map
bounds. These are existing eval-language limitations, not entity-engine results.

Public exposure is now 40/65 functions, with 39 shared-engine operations, and 8/9
constants. Conversion/detection binding and catalog identity, MIME, Oniguruma,
HTTP/mail/INI/request hosting, remaining references and callbacks, ownership,
strictness, and packaging all remain in the full goal. A fresh read-only GitHub
query still finds exactly the five tracked open mbstring PRs at the recorded
heads. No commits, comments, or PR closures were performed.


Final verification for numeric entities and typed-array reference normalization:

- All 87 mbstring codegen tests pass, with none ignored. All six entity regressions
  also pass with `ELEPHC_IR_OPT=off ELEPHC_PHP_CHECK=1`.
- All five focused array-element reference regressions pass with PHP comparison,
  including capture retyping and COW separation before reference binding.
- Six mbstring signature/error tests, thirteen compiler runtime tests, and the
  complete pinned entity-engine fixture pass. Both existing scalar-coercion tests
  remain green after sharing the numeric scanner.
- All nine invocation tests pass, including the 86 map oracle calls checked in
  both strictness profiles. Stable runtime-ID round trips pass after correcting
  the previously stale unknown-ID sentinel. The final bridge library mbstring
  unit group passes, including actual wire-ABI entity results, diagnostics,
  malformed-map ordering, and repeated result release.
- Compiler and curl-enabled exporter builds pass without warnings. The complete
  generated-docs workflow passes, rendering 1,995 pages with no audit errors,
  site incompatibilities, or enforced EIR target-boundary violations.
- Clang assembles current runtime and user fixtures for all five supported
  targets, including native typed-array reference captures and opaque eval entity
  calls. These remain assembler checks outside the Linux x86_64 execution host.
- The updated product-label example matches PHP 8.5.10 exactly: 922 stdout bytes,
  successful exits, and empty stderr for both programs.
- Assembly-comment checks, all 362 touched/new Rust module preambles, prohibited
  punctuation checks on added text, and `git diff --check` pass.

The full mbstring goal remains active. No formatter, full root test suite, commit,
PR comment, or PR closure was run in this increment.


## Encoding detection bindings and request catalog identity (2026-09-08)

Added `mb_detect_encoding` to the neutral catalog, AOT home, Magician home, and
shared typed runtime inventory with stable runtime ID 61. The binding uses the
existing PHP-compatible detector after common outer coercion and incremental
candidate preparation. Omitted strictness observes request state, while explicit
false overrides that default. The protected candidate reader also serves the
second parameter of detection, preserving later references changed by Stringable
callbacks. Empty lists, invalid names, transfer filtering, defaults, named calls,
case-insensitive namespace fallback, and callable surfaces have public coverage.

`mb_list_encodings()` now preserves PHP's request-local cached array identity for
V2 hosts. The cache owns one native reference, and each return acquires its own
reference. Ordinary native COW detaches mutated copies, including copies restored
to identical contents. The input descriptor carries an explicit catalog identity
bit into the wire array metadata. Detection uses this identity to disable candidate
order weighting, without inferring identity from equal contents. V1 invocation and
direct wire catalog results keep the existing packed string-array result kind;
V2 gets the dedicated catalog kind, normalized by native materialization before
existing AOT/eval result consumers inspect it.

Native request entry releases any prior catalog before resetting engine settings.
Main cleanup releases its root after user locals/statics/globals, and web cleanup
releases it before the arena reset. A one-worker web regression sends twelve
requests, verifying catalog recreation, detached-copy guesses, default detection
order, and internal-encoding reset on every response. Catalog copies survive a
native GC safe point, and repeated copy/mutate/detect operations have stable heap
residuals in native and opaque eval. These checks cover the current executable and
web boundaries; remaining library/request-host integration is still in scope.

Verification completed for this increment:

- All 102 tests selected by the compiler codegen `mbstring` filter are green across
  the group run and focused correction rerun. The sole group failure was a stale
  assertion for the old direct reset symbol; the updated target test checks the
  combined reset and catalog cleanup calls and includes detection on all five
  supported targets. No tests were ignored.
- All six detection codegen regressions pass with PHP comparison and EIR
  optimization disabled, including cached/rebuilt/restored-mutated identity,
  AOT/eval crossings, callbacks, and later element-reference mutation.
- All three existing list-encoding codegen tests pass with PHP comparison, and the
  independent detection-engine fixture passes its 468,304 PHP oracle requests.
- Seven mbstring error tests, thirteen native runtime tests, eleven protected
  invocation tests, five bridge unit tests, four coercion-ABI tests, and the
  stable runtime-ID round-trip test pass. Metadata tests reject unknown bits,
  accept catalog identity only for arrays, and retain normal PHP type errors.
- Compiler build, example PHP syntax, assembly-comment checks, and all 370
  touched/new Rust module preambles pass.
- The curl-enabled exporter builds without warnings. The complete generated-docs
  workflow passes, rendering 1,997 pages with zero audit errors, site compatibility
  failures, or enforced EIR target-boundary violations.
- Clang assembles both current runtime and public user fixtures for macOS ARM64,
  iOS ARM64 device and Simulator, Linux ARM64, and Linux x86_64. Only Linux x86_64
  executes locally; the other four results are assembler checks.
- The updated import-label example matches PHP 8.5.10 exactly: 954 stdout bytes,
  successful exits, and empty stderr. Added-text punctuation checks and
  `git diff --check` pass.

Public exposure is now 41/65 functions, including 40 shared-engine operations,
and 8/9 constants. Conversion bindings, MIME, Oniguruma, HTTP/mail/INI, remaining
request hosts, references/callbacks, strictness, ownership, and packaging remain
in the full goal. The five tracked open PRs were checked again read-only on
2026-09-08 and retain the recorded heads. No formatter, full root suite, commit,
PR comment, or PR closure was performed.


## Public conversion and completed-array restoration (2026-09-08)

Added the shared `mb_convert_encoding` contract, AOT/eval home bindings, typed
runtime operation, and stable boxed-call ID 62. The public surface is now 42/65
functions (41 shared-engine operations) and 8/9 constants. Conversion accepts
strings or recursive arrays, uses current internal encoding when the source is
omitted, and resolves destination encoding before source-list callbacks. Source
encoding validation completes before input traversal, and callbacks run outside
the shared request-state borrow.

The pointer-free array graph now has a host restoration ABI. The Rust restorer
validates and compacts the graph, rejects cycles and unsupported graph values
before native allocation, and builds completed children before their parents.
A callback receives exact key/value descriptors and an explicit indexed-layout
flag. It copies strings and retains completed child arrays. Cleanup consumes all
intermediate owners, including after partial host allocation failures or a Rust
panic; only the completed root owner is returned. Shared child graph identity,
binary keys, distinct integer/numeric-string keys, null/bool/int/float bit patterns,
and insertion order survive restoration. Native adapters implement this protocol
for both CPU architectures and classify the restored root before boxing it.

Public regressions exposed related value/iteration problems:

- By-value assignments of `mixed` or union results shared their mutable boxed
  cell. An array mutation then retargeted every local sharing that cell, despite
  payload COW. Both statement and expression assignments now use the shared
  `copy_assignment_value` helper, which clones the box and consumes an owned RHS.
  Explicit reference aliases, object identity, and resource identity retain their
  existing semantics. A standalone assignment-expression regression failed with
  the original implementation and passes after the fix.
- Direct shared mbstring eval calls retained fresh literal argument cells.
  Their argument wrapper now tracks and releases literal owners on success,
  arity errors, and later argument-evaluation errors. Repeating a nested array
  conversion 24 times has the same residual heap count as one conversion in
  both backends. General nonliteral-expression and other eval-call ownership
  remain separate open audit items.
- Eval JSON and by-value foreach read values by their normalized key, conflating
  integer key `1` with the distinct string key `"1"` that conversion can produce.
  A typed native iterator now returns each entry's actual value by position,
  and these two eval consumers use it. Other eval array consumers still need
  the corresponding exact-key audit.
- Eval float negation now flips the IEEE sign bit for concrete floats, preserving
  signed zero, and JSON preserves that sign. An existing JSON partial-output
  unit expectation was independently reproduced as failing with the unchanged
  HEAD implementation and checked against PHP. The fixture now expects an empty
  invalid-UTF-8 key under partial-output mode, matching PHP's `{"":null}`.

Evidence completed before the final assignment-expression change:

- All 110 focused mbstring codegen tests passed. Eight conversion tests passed
  with PHP comparison and EIR optimization disabled. The expanded ninth test
  covers assignment expressions and was verified separately against PHP.
- All 38 focused COW codegen tests passed with PHP comparison.
- Three host-restoration tests cover graph identity and representation, injected
  builder failures, malformed/truncated buffers, cycles, and unsupported values.
  Two direct conversion ABI tests preserve exact keys, substitution state, and
  scalar bit patterns. All five pass.
- Existing focused array (six), conversion-error (three), and detected-conversion
  (one) engine tests pass. All eleven protected invocation tests, thirteen native
  runtime tests, eight mbstring error tests, ten Magician JSON tests, and the
  stable runtime-ID test pass. Compiler build succeeds.

The public example now decodes an ISO-8859-1 supplier label before its existing
Unicode workflow. Generated documentation and all-target assembly verification
were refreshed after the final shared assignment helper change.

An additional reference probe remains a confirmed compatibility gap:
`.plans/mbstring-probes/conversion-input-reference.php` prints `["after"]` in
both native code and PHP after a source-list callback changes an input element
through a reference. The opaque eval equivalent,
`conversion-eval-input-reference.php`, prints `["before"]`, while PHP prints
`["after"]`. The input graph snapshot still uses the raw physical array reader,
which bypasses eval's reference side metadata. Fix this through protected owned
graph reads with stable reference identity and cleanup, including nested arrays;
do not resolve PHP references under a Rust request-state borrow. The existing
original-token lifetime audit also remains open.

MIME, Oniguruma, `mb_convert_variables`, HTTP/mail/INI, remaining request hosts,
strictness, ownership, packaging, and the remaining public functions/constants
are still required. The five tracked open PRs were checked read-only again and
retain the recorded heads. No PR comment, closure, commit, or formatter was run.


Final verification for public conversion and the shared assignment helper:

- All nine conversion regressions pass with PHP comparison and EIR optimization
  disabled. All 42 focused assignment-expression regressions pass with PHP
  comparison. The standalone new assignment-expression test also passes with
  default optimization. The earlier 110-test mbstring run and 38-test COW run
  passed before the last expression-assignment fix; neither was unnecessarily
  repeated in full afterward.
- The compiler and curl-enabled builtin exporter build successfully. The complete
  generation and audit workflow renders 1,999 builtin pages and reports no
  registry, site-compatibility, or enforced EIR target-boundary failures.
- Current runtime and public fixtures assemble with Clang for macOS ARM64,
  iOS ARM64 device and Simulator, Linux ARM64, and Linux x86_64. Only Linux x86_64
  is executed locally; other-target execution remains CI's responsibility.
- The updated example matches PHP 8.5.10 exactly: 984 stdout bytes, successful
  exits, and empty stderr. The native and eval input-reference probes were
  rerun with the final compiler and reconfirm the documented eval gap.
- All 388 touched/new Rust files have their module preamble. Assembly-comment
  coverage/alignment, added-content punctuation, and `git diff --check` pass.
- All verification processes for this conversion work have finished. No full
  root suite, formatter, commit, PR comment, or PR closure was performed.


## Protected recursive input reads and copied eval references (2026-09-08)

Added a backward-compatible V3 invocation table (96 bytes) with protected
exact-key graph reads and original-identity pinning. The V1/V2 prefixes and their
older host contracts remain supported. V3 graph entries publish a copied value,
borrowed exact key, and separately owned original identity; all owner slots are
initialized before callbacks and cleaned even when callbacks fail or publish a
pending exception. Native emitters share their list/graph implementation and use
all six C argument registers for eval graph reference resolution when needed.

The coordinator now snapshots recursive input through this protocol after outer
coercion and source-list callbacks. It walks depth first, guards cursor progress,
visits each physical input array identity once, and preserves cycles in the wire
graph for the operation engine to handle. PHP calls remain outside the request
state borrow. Original argument boxes are pinned before callbacks can replace
caller storage; nested original boxes remain owned through traversal. This closes
the in-call original-token address-reuse window for V3 callers. Complete eval
metadata retirement outside an invocation is a separate remaining audit.

The original conversion probes now match PHP in both native and opaque eval
execution. New regressions cover direct and nested references, replacing the
caller's original array during a source-list callback, replacing a referenced
child array, and reading a reference created inside a returning closure. The last
case also required following captured reference targets before storing array
metadata, rather than retaining a pointer to the temporary closure activation.
A standalone eval regression verifies that lifetime fix without calling mbstring.

A further public probe showed that ordinary eval array assignments detached the
value box without propagating reference metadata. Array-reference context methods
now have their own cohesive module, and by-value assignments copy the source's
reference bindings into the fresh box, replacing any previous metadata for that
destination identity. A standalone metadata unit checks binary keys, stale
destination replacement, source preservation, late reads, and identity-preserving
copies. A public regression combines copied references with an ordinary COW write
and conversion of both the original and copied arrays.

Verification completed for this change:

- Fourteen independent protected-invocation tests pass. V3 tests check shared
  input graph traversal, current reference values, scalar bits, missing callbacks,
  failures at pin/read/describe/release stages, malformed cursor/end records, and
  cleanup of every published owner. Conversion correctly produces independent
  output occurrences from repeated shared input nodes.
- Both contained-panic unit tests pass, including a panic after graph snapshotting
  with live argument pins, copied graph values, and nested identity owners.
- The V3 ABI layout test, thirteen compiler/runtime mbstring unit tests, eight
  mbstring error tests, and both Magician reference-metadata units pass.
- All 114 focused mbstring codegen tests passed after the recursive reader and
  captured-reference fix. All twelve conversion tests also passed with PHP
  comparison and EIR optimization disabled.
- After the final copied-array metadata fix, all thirteen conversion codegen
  tests and all 38 focused COW codegen tests pass with PHP comparison. The
  compiler builds successfully. The full mbstring group was not needlessly
  repeated after this localized eval-copy fix.
- The curl-enabled exporter and complete docs workflow pass, rendering 1,999
  pages with no registry, site, or enforced EIR boundary failures. No PHP-visible
  contract changed after that generation; the later change only propagates eval
  metadata during array assignment.
- Current runtime and user fixtures assemble for macOS ARM64, iOS ARM64 device
  and Simulator, Linux ARM64, and Linux x86_64. Only Linux x86_64 executes locally.
  The later eval-only metadata copy adds no target assembly changes.
- The example matches PHP 8.5.10 exactly (984 stdout bytes, successful exit, empty
  stderr), and the two original input-reference probes now both print `["after"]`.

A discarded diagnostic probe called `gc_collect_cycles()` from opaque eval. That
PHP function has no public compiler/eval binding here, so the probe did not
exercise cycle collection. The same failure occurred with a controlled V2-table
runtime, and a direct cast variant reported an unsupported eval construct. Do not
report this as a collector regression or use it as collection evidence. The
existing runtime allocation/free counters remain the valid ownership signal.

Public exposure remains 42/65 functions, including 41 shared-engine operations,
and 8/9 constants. Remaining reference/host semantics, MIME, Oniguruma,
`mb_convert_variables`, HTTP/mail/INI, strictness, packaging, and the rest of the
public surface remain in the full goal. No formatter, full root test suite,
commit, PR comment, or PR closure was performed.


Final reference audit after the copied-array fix:

- `conversion-eval-copied-reference.php` now matches PHP: `["after"]`.
- `conversion-eval-reference-write.php` remains incorrect. Writing through a
  copied referenced element leaves the source variable unchanged, and the reader
  continues to prefer its old reference metadata over the newly stored physical
  array value. Actual output is `before:["before"]:["before"]`; PHP prints
  `changed:["changed"]:["changed"]`. Array writes must resolve and update the same
  reference storage used by reads, including after COW and value copies.
- `conversion-eval-returned-reference.php` terminates with SIGSEGV (exit -11),
  whereas PHP prints `["alive"]`. The new protected graph reader exercises eval's
  existing reference metadata, which can retain raw pointers to ordinary function
  activation scopes after they have ended. Following captured parent targets
  fixes returning closures with live outer variables, but it does not establish
  persistent storage for references to ordinary locals returned in an array.
  This requires actual stable reference ownership, not a fallback to stale array
  contents or an assertion that all graph reads are now safe.
- `conversion-eval-reference-address-reuse.php` prints `["after"]` instead of
  PHP's `["second"]`: alias metadata from a freed array box is consulted after its
  address is reused for a fresh ordinary literal. V3 pins prevent reuse during a
  call, and copy propagation clears stale destination metadata, but neither
  handles the full allocation/release lifecycle of ordinary eval array boxes.

These three retained probes are required follow-up work before reference support
or public conversion parity can be considered complete. Their captured outcomes
are in `/tmp/elephc-mbstring-v3-reference-audit.json`. The audit disabled core dumps
for its generated binaries and bounded each execution with a timeout. All probe
and verification processes have completed.

All 396 touched/new Rust module preambles, assembly-comment alignment, added-text
punctuation checks, and `git diff --check` pass. The full goal remains active;
none of the outstanding reference failures has been marked resolved.

## Persistent local references, 2026-09-08

Local variables referenced by eval arrays and closure captures now use a native
GC-traced Mixed wrapper instead of retaining their activation's Rust scope
address. The wrapper uses tag 7, an owned concrete Mixed child, and high-word
marker 1. `__rt_mixed_deref`, `__rt_reference_new`, `__rt_reference_replace`, and
`__rt_reference_array_copy` share this representation across both architectures.
Replacement publishes a copied value before returning the previous child owner
for cleanup. The existing Mixed collector traces the child without a new heap kind.

The common promotion and captured-target resolution lives in Magician's
`interpreter/persistent_references.rs`. A real local reference is stored in the
array itself; its literal does not also retain legacy side-table metadata.
Ordinary literals clear old metadata for a reused array-box address. Copies of
arrays retain shared reference wrappers, while COW duplication detaches a
reference whose only remaining owner is the original array slot. Associative
writes now split their hash before examining stored reference identities.

Scope writes, array writes, closure captures, ordinary variable copies, parameter
binding, and activation return handling understand these wrappers. The scope
marks reference aliases dirty without transferring their owner between aliases.
Read adapters for output, array lookup/count/key/value iteration, class names,
and stdClass properties follow the concrete value. Ordinary element reads release
their temporary reference owner after copying the value. Resource copies retain
the concrete resource cell rather than a mutable wrapper around it.

The three previously retained probes now have the expected behavior:

- Writing through a copied referenced element updates both arrays and its local
  variable, including associative keys.
- `return [&$value]` no longer reads a dead scope. After its creating activation
  ends, copying the returned array follows PHP's orphan-reference COW behavior.
- Reusing an array-box address for an ordinary literal no longer revives the
  previous local-variable alias.

Additional regressions cover scalar copies, by-value arguments and captures,
returned closure/array pairs, nested array values, reading before `unset()` and
COW, object replacement, and source-variable ownership during repeated assignment.

Focused verification after the fixes:

- All 21 conversion codegen tests passed.
- The new repeated-reference-assignment GC test passed for 1 and 24 assignments.
  It also keeps the independently stored replacement variable readable.
- Eleven `test_cow` codegen tests, both indexed-clone emitter units, 67 Magician
  reference units, and five Magician closure units passed.
- The numeric-entity validation-order and scalar-callable tests passed separately.
  The resource detection-order probe exactly matched PHP (`resource:alive`).
- The independent PHP comparison covers ten variable/capture/conversion cases;
  results are retained in `/tmp/elephc-reference-comparison.json`.
- Runtime and user assembly assembled successfully for macOS ARM64, iOS device,
  iOS Simulator, Linux ARM64, and Linux x86_64. Mach-O verification caught and
  corrected conditional branches to external runtime symbols in the new COW
  helper. Only Linux x86_64 binaries execute locally; CI remains necessary for
  executable target coverage.
- `cargo build -p elephc`, the builtin EIR boundary/target architecture audit,
  assembly-comment checks, all 424 touched/new Rust module preambles, and
  `git diff --check` passed.

The extended `mbstring` run reached 93 passing tests before `/tmp` exhausted its
space and the assembler failed. It did not produce a complete successful suite
result. Its reported callback-array discrepancy was reproduced and fixed in the
array read adapters. Other reported cases were checked separately after moving
local test artifacts to `/home/nahime/.cache/elephc-mbstring-test-tmp`. Only completed
test directories belonging to this session were removed from `/tmp`. The target
probe executable also moved to that disk directory after its linker encountered
a bus error while producing the old `/tmp` artifact.

Required follow-up remains concrete:

1. `.plans/mbstring-probes/persistent-reference-nested-slot-return.php` still exits
   with SIGSEGV instead of PHP's `["alive"]`. A literal containing `&$array[0]`
   still uses `NestedArrayElement` metadata with a dead `Variable` scope target.
   This needs actual persistent reference storage in the source array slot, with
   COW applied before binding. Merely retaining a container in the side table
   would hide the crash without preserving references across subsequent copies.
2. By-value parameter copies of reference-backed variables currently leak their
   new concrete owners at activation exit. The independent GC probe records:
   ordinary arguments retain 5 allocations for both 1 and 24 calls; reference
   arguments retain 7 and 53, respectively. That is 46 extra allocations across
   23 additional calls. `bind_method_scope_args()` creates owned copies, while
   `finish_scope_references()` currently releases only reference wrappers. Fix
   ownership and returned-value transfer without making by-value parameters alias
   the caller or delaying PHP-visible destructor behavior.
3. Continue the lifetime/COW audit for property and nested-element references,
   closures, throws, and ordinary-value temporaries. A returned property-reference
   probe currently prints `["alive"]`, but that alone does not establish balanced
   ownership or complete lifetime safety for the legacy metadata paths.

Evidence lives in `/tmp/elephc-reference-validation*.log`,
`/tmp/elephc-reference-targets-final.log`, `/tmp/elephc-reference-extra-audit.json`,
and `/tmp/elephc-reference-parameter-audit.json`. The latter two retain the failing
nested-slot and parameter-ownership cases for the next implementation step.

The public surface remains 42/65 functions and 8/9 constants. No formatter,
commit, PR comment, or PR closure was performed. The complete mbstring goal is
still active, including the remaining MIME, Oniguruma, variable conversion,
HTTP/mail/INI, strictness, packaging, and reference/host work.

Final probe refresh: all ten independent conversion/reference comparisons now
match PHP exactly, and the corrected object-copy probe also matches. The extra
probe audit has four matches and the retained nested-slot crash. Heap-debug
execution of the copied-reference write exits normally with matching stdout;
its shutdown report still lists 24 live blocks, so this is not zero-leak evidence.
That report is saved in `/tmp/elephc-reference-heap-debug.json`. All compilation,
verification, and probe processes launched for this checkpoint have completed.

## Activation ownership and parameter snapshots, 2026-09-08

The copied-parameter leak recorded above is now fixed by releasing the complete
owned activation scope, after its escaped values acquire independent owners.
The implementation does not keep by-value arguments attached to caller reference
cells and does not defer their release to a separate lifetime guard.

`eval_owned_expr` shares the existing expression evaluator through an explicit
result-ownership mode. Variable results are copied, references are detached,
and the chosen null-coalescing, ternary, shorthand ternary, or match branch
preserves that result contract. Property and legacy array-alias reads use the
existing ownership-aware reference readers. Return, throw, variable storage,
and discarded expression statements request owned results. Return coercion
releases a replaced or rejected result. Consequently a `return $arg` snapshot
survives both local cleanup and a subsequent `finally` assignment.

`finish_activation_scope` replaces the reference-only cleanup and releases every
owned scope entry through the dynamic-destructor-aware release path. By-reference
variable writeback retains a distinct value into the caller; unchanged values
leave the caller owner intact. Static locals retain replacements before the
activation releases its local entry. Static property storage now retains its
borrowed input, and the three direct static-property assignment shapes release
their owned expression result after the setter. This also fixes the reproduced
empty read after storing a parameter snapshot into a static property.

Ordinary array literal elements now use owned expression results and release
the temporary after runtime storage retains it. Indexed literal keys are also
released after alias metadata has consumed the key. This removes the measured
per-call growth when a function builds and returns `[$arg]` from a detached
reference-backed parameter.

Four new native regression tests in `tests/codegen/runtime_gc/mbstring.rs` pass:

- `test_mbstring_eval_reference_parameter_ownership`: equal residual allocations
  after 1 versus 24 calls with a copied scalar parameter.
- `test_mbstring_eval_reference_parameter_array_ownership`: equal residuals after
  1 versus 24 ignored returns of a local array containing that parameter.
- `test_mbstring_eval_reference_parameter_escape`: ordinary and conditional
  returns, `finally`, caller writeback, and static-property storage keep the
  original string after the caller reference changes.
- `test_mbstring_eval_reference_parameter_destructor`: clearing the caller through
  a by-reference argument destroys the by-value object snapshot at function exit,
  before the following caller output.

The first focused reference unit run passed 67 tests, and the conversion run
passed 21 tests before the final literal-owner changes. The four new native
regressions passed together after the final changes. A complete mbstring-filtered
native run and focused existing activation tests are being verified next.

An additional opaque-eval probe using `global $value` still prints
`body:afterdrop:` where PHP prints `body:drop:after`. GDB shows that this case
never calls the native reference replacement helper and the parameter's object
payload still has two owners at cleanup. The equivalent caller mutation through
an explicit by-reference argument passes. This is retained as a separate global
scope resolution investigation, not evidence that parameter cleanup is complete
for every global/host path. The original probe is
`.plans/mbstring-probes/scope-destructor-parameter.php`; its trace is
`/tmp/elephc-scope-destructor-trace.log`. Static-local, ordinary object-property,
and local-array escape probes matched PHP before the final literal changes.

Required follow-up remains the actual persistent array-slot reference operation,
with COW before binding and no activation-scope pointer in escaped references.
The retained nested-slot-return crash has not been fixed in this checkpoint.
The existing fetch-for-write runtime already handles COW and stored slot access,
but autovivifies a missing final slot to an array; a reference fetch must produce
null there and replace the unique parent slot with a reference wrapper rather
than mutating an ordinary cell shared with another array. Both architectures
and the C adapter must keep that distinction explicit. Broader ownership audits
still include legacy aliases, exceptional exits before return processing,
associative literal key temporaries, and object/resource copies. The complete
65-function, 9-constant mbstring goal remains active.

Final verification for this checkpoint:

- The mbstring-filtered native run completed: **128 passed**, zero failures,
  in 322.62 seconds (`/tmp/elephc-scope-mbstring-tests.log`).
- Magician filters passed: reference 67, closures 5, static 62, return 34.
  These counts overlap and are not a unique-test total.
- Existing native static-local persistence and declared method return-value
  tests each passed. Native destructor tests passed 2 of 3.
- The expanded `interpreter::tests` run completed with **728 passed and 1 failed**
  (`/tmp/elephc-scope-interpreter-tests-final.log`). Two older exception tests
  inspected the first release rather than the thrown object. They now assert
  exactly one released object, allowing earlier scalar temporary cleanup.
- PHP independently confirms the new escape and destructor expectations
  (`/tmp/elephc-scope-php-checks.json`).
- The builtin/EIR target architecture audit reported zero errors. The shell
  command's eventual status 1 was from a following `rg` with no matching failure
  text, not from the audit. No runtime assembly was changed in this checkpoint.

Two additional current-branch failures must be addressed before reference and
activation lifetime parity can be claimed:

1. `interpreter::tests::dynamic_calls::runtime_callables::execute_program_call_user_func_array_runtime_method_writes_back_by_ref_type_coercion`
   leaves the referenced caller value as string `"3"` instead of integer `3`.
   `append_unpacked_call_arg_values` in `interpreter/dynamic_functions.rs` only
   recovers targets from legacy array alias metadata. Its `array_get` read now
   detaches a persistent reference, losing the raw reference target. Native
   `__rt_mixed_array_get` already returns an owned retained stored Mixed cell;
   the Rust runtime hook detaches the reference for ordinary reads. A distinct
   reference-preserving argument read can use that existing ABI, but must
   release temporary reference owners after the call, otherwise orphan-reference
   COW decisions remain poisoned by leaked owners. `EvalReferenceTarget::Cell`
   writeback currently does nothing and must write through a native reference
   when coercion produces a distinct value. Do not weaken the expected type.
2. `test_eval_dynamic_object_runs_destructor_after_cycle_collection` prints
   `after` instead of its existing expected `drop:A:after`. The other two native
   destructor tests passed. This case stores `$box->self = $box`, unsets `$box`,
   then echoes. Whether this originates in the current checkpoint or earlier
   reference/deep-release work is not yet established. The existing native
   `__elephc_eval_value_release` tail-calls `__rt_decref_mixed`, whose contract
   explicitly leaves cycle collection to safe points. Investigate the actual
   collection/finalizer path rather than changing the expected output.

The nested-slot-return crash and opaque-eval `global` probe also remain open.
The ABI/catalog coverage remains 42 of 65 public functions and 8 of 9 constants;
this checkpoint adds no new public builtin surface and does not complete the
full mbstring goal. No formatter, commit, PR comment, or PR state change was made.

## Argument-array owners and real cycle diagnosis, 2026-09-08

The previous call-array reference writeback failure is resolved. A shared
`call_argument_owners` guard snapshots the argument array, retains raw reference
reads, and releases all acquired owners after invocation and reference writeback.
It covers `call_user_func_array`, the context function-call-array ABI, reflection
`invokeArgs`/`newInstanceArgs`, and `iterator_apply`. Native reference targets now
accept typed coercion writeback. Ordinary source argument spreads still require
migration to an equivalent lifetime guard. A leaked integer cast used for array
keys was also fixed in the common eval scalar helper.

Evidence before the subsequent cycle-collector changes:

- Magician interpreter tests: 729 passed, no failures
  (`/tmp/elephc-reference-array-surfaces-unit.log`).
- New native call-array lifetime/coercion tests: 2 passed
  (`/tmp/elephc-reference-call-array-native-final.log`).
- New native reflection-array reference and iterator argument reference tests:
  both passed in their respective focused logs.
- PHP independently matched the coercion, reflection, and iterator results
  (`/tmp/elephc-call-array-php-checks.json`).

The existing eval self-cycle destructor test was reproduced on exact baseline
HEAD `217ff6caad7e0965688c54b41d2410a8f5e92dec` in an isolated detached worktree.
That baseline passes, but GDB establishes that it never calls the cycle
collector. Its stdClass property setter passed a borrowed Mixed value to a
consuming native operation without retaining it. The baseline box therefore had
refcount one despite both the scope and the self-property owning it. Unset
incorrectly freed the undercounted cycle immediately. The current branch's
correct retain gives refcount two and exposes the absent eval collection safe
point. Do not restore the undercount to satisfy the old assertion.

Collector integration is being implemented and is not fully validated yet:
protected eval release/collection boundaries, dynamic-property graph edges,
resumable cleanup, a destructor phase before freeing cyclic property storage,
and exclusion of allocations created during destructor execution. The original
self-cycle test now passes with correct ownership
(`/tmp/elephc-eval-cycle-safe-point-2.log`). New cyclic peer-property and repeated
throwing-destructor tests pass; a nested receiver read retains an extra cycle
owner, and the new last-argument-owner fixture still needs diagnosis. The current
four-test boundary run has two passes and two failures, so it is not a completed
GC validation (`/tmp/elephc-eval-cycle-boundaries.log`).

The current changes do not add public mbstring functions. Coverage remains
42/65 public functions, 41 shared operations, and 8/9 constants. The nested
array-slot reference crash, opaque-eval global alias probe, remaining source
spread lifetimes, and all previously tracked missing mbstring families remain
open. No commit, formatter, PR comment, or PR state change was made.

## Eval cycle safe points and destructor cleanup, 2026-09-08

The cycle integration described above now has passing native regression coverage.
The stdClass property retain remains in place. The collector now accounts for
object-owned dynamic-property hashes in both incoming-edge counts and reachable
traversal. AArch64 child-edge accounting also includes heap kind 5, so Mixed and
persistent reference cells do not become spurious external roots.

Collection first runs destructors for the captured unreachable objects while
all graph properties remain intact, then frees the remaining graph storage.
Destruction guards survive the free phase to prevent duplicate method calls.
Candidate bit 17 excludes allocations created or reused by destructors. Shared
resumable cleanup retains exceptions while finishing later destructors and
releases, then restores collector and release-suppression state before throwing.
A focused independent C heap fixture verifies rooted cycles, dynamic hashes,
Mixed/reference edges, intact property reads, reused allocation survival,
suppressed collection, and resumption after a pending cleanup exception.

Magician now emits one explicit `EvalStmt::GcCollect` after all operands of a
source-level unset, matching AOT's safe-point placement. Variable, property, and
array unsets retain their source-order behavior; collection does not interleave
between arguments of the same unset. The expression-form unset adapter also
collects once after its operands.

Eval's native release and collection entries contain PHP exceptions behind
complete native handler records. The dynamic destructor callback returns an
owned Throwable through an output slot when it reports status two, and native
code propagates it after the Rust callback returns. Dynamic object identity
metadata is removed at actual storage release, rather than during the earlier
destructor phase. Direct eval destructor failure no longer skips releasing its
owned value.

Two additional lifetime problems exposed by these tests are fixed:

- Property reads own and release their receiver, including nested, dynamic, and
  nullsafe reads. A nested self-property read no longer retains an invisible
  owner that prevents a later unset from collecting the cycle.
- Native array-like predicates dereference persistent wrappers before checking
  the concrete tag. A by-reference closure capture can therefore serve as a
  call-array source. The private call-array snapshot's legacy alias metadata is
  retired at the end of its guard.

The native pending-Throwable adapter previously wrapped a transferred raw
object with a new retaining box but never released the transferred raw owner.
A 1-versus-24 cyclic throwing-destructor regression observed 6 versus 52 residual
allocations before the fix. Boxing now consumes that raw owner, and the repeated
regression has constant residual ownership.

Validation:

- Native `runtime_gc::eval_cycles`: 7 passed on Linux x86_64 after the final
  Throwable transfer fix (`/tmp/elephc-eval-cycle-boundaries-complete.log`).
  Coverage includes rooted mbstring reads, cyclic peer reads, repeated throws,
  AOT/eval multi-operand unset order, exception chains, repeated Throwable
  ownership, and four native/eval throwing/nonthrowing last-argument-owner cases.
- The original existing eval self-cycle regression passed with the corrected
  reference counts (`/tmp/elephc-eval-cycle-safe-point-2.log`).
- The existing three native cycle reclamation regressions passed
  (`/tmp/elephc-gc-existing-cycle-tests.log`).
- All 133 mbstring codegen tests passed in 306.40 seconds after collector,
  receiver, and predicate changes, before the final grouped-unset and raw
  Throwable-transfer refinements (`/tmp/elephc-gc-mbstring-tests.log`).
- All 1,194 Magician library unit tests passed after grouped-unset lowering
  (`/tmp/elephc-gc-magician-final.log`). The final native-only Throwable adapter
  adjustment is covered by native tests rather than fake-runtime unit tests.
- The independent collector machine-code fixture passed on Linux x86_64
  (`/tmp/elephc-gc-native-graph-test.log`) and Linux ARM64 under Docker/QEMU
  (`/tmp/elephc-gc-arm-native.log`). This is focused collector execution, not a
  full ARM64 PHP or Magician test run.
- Runtime and user assembly passed assembly checks for macOS ARM64, iOS device
  ARM64, iOS Simulator ARM64, Linux ARM64, and Linux x86_64
  (`/tmp/elephc-gc-all-targets.log`).
- The builtin/EIR architecture audit reported no structural errors
  (`/tmp/elephc-gc-builtin-boundary.log`). Assembly comments are aligned and
  `git diff --check` passed. No formatter was run.

The exception-chain test inspects the resulting chain from AOT after eval
propagates it. Direct opaque-eval `getPrevious()` is not implemented by the
existing eval Throwable method adapter; the initial all-eval inspection failed
for that separate reason. This checkpoint does not claim support for that
method, object resurrection, activation-exit collection, or complete source
argument/array-slot reference lifetimes.

The full mbstring goal remains active at 42/65 public functions and 8/9
constants. Source argument spreads, escaping references to nested array slots,
and the previously recorded global-alias probe remain tracked for subsequent
work, along with all missing mbstring function families. PR tracking remains
read-only, and no commit, PR comment, or PR state change has been made.

Final focused follow-up: the existing `test_mb_strlen_eval_exception_ownership`
passes after the raw Throwable transfer fix
(`/tmp/elephc-gc-existing-mb-exception-test.log`). The clean, task-created detached
baseline worktree was removed after validating its exact commit and clean Git
status; its diagnostic logs and comparison results remain available. Other
worktrees and their build artifacts were left untouched.
The final existing eval-destructor filter also passes all three tests, including
the original self-cycle case, after all safe-point and Throwable refinements
(`/tmp/elephc-gc-existing-eval-destructors-final.log`).

## MIME decoding and callable argument diagnostics, 2026-09-08

Implemented `mb_decode_mimeheader` in the shared engine, neutral contract, typed
AOT binding, and eval runtime inventory. Public inventory is now 43/65 functions,
42 shared operations, and still 8/9 constants. The extension goal remains active.

The initial 60,385-header oracle passed; the expanded corpus contains 81,173
headers, all now matching PHP 8.5.10. It covers every internal encoding, B/Q
malformed syntax, missing terminators, NULs, whitespace folding, cross-charset
state, SoftBank escapes, UTF-7 pending surrogates, deterministic arbitrary bytes,
and encoder invocation boundaries. MIME uses a fixed `?` replacement and does not
increment request illegal-character counters. Shared codecs now preserve the
actual PHP decoder-state word between MIME words. UUENCODE MIME output preserves
partial-group padding and repairs line lengths across calls. Ordinary entity and
conversion encoding paths retain their prior final-call semantics.

The public native tests exposed two pre-existing callable adapter issues:
invalid indexed arity could cause EIR validation failure or an uncatchable generic
missing-argument fatal; original array argument types could be converted before
the shared mbstring parameter checker. Known invalid mbstring callable arity now
uses descriptor dispatch. Mbstring descriptor invokers retain operation identity
in their cache and reject bad indexed counts through the shared native coordinator
before parameter adaptation. Their wrapper ABI now keeps all parameter values
boxed, and dynamic candidate filtering allows the shared checker to report
TypeError for unsupported original types. Named-container missing/extra argument
normalization remains part of the broader tracked callable work.

New pending evidence: runtime-unknown native callable targets still lose caller
`strict_types` in the generic wrapper. Reproducer:
`.plans/mbstring-probes/dynamic-callable-strict-mime.php` prints `123` natively but
PHP raises the mb_decode_mimeheader string TypeError. The existing wrapper lowering
explicitly sets `strict_types: None`; fixing this correctly requires propagating
the invocation profile, including call_user_func's weak callback rules, rather
than baking the descriptor creation site's mode into a cached wrapper. This gap
is NOT fixed by the MIME decoder or the boxed parameter change.

PHP oracle also confirmed that opaque eval starts weak even inside a strict
native caller. A declaration inside an eval fragment is currently rejected by
Magician's parser; this is a separate existing eval syntax limitation. The native
MIME test checks a known strict callable and the independently weak eval behavior.

Final focused integration and generated-doc checks are pending at this checkpoint.
No commits, PR actions, agents, or cargo fmt were performed.

Next MIME encoder note: local PHP confirmed that explicitly supplied null for
`mb_encode_mimeheader` charset/transfer_encoding is deprecated and coerced to an
empty string despite its reflected nullable signature. A null charset then raises
ValueError; a null transfer encoding keeps the default transfer selection. Omitted
arguments retain language defaults. The shared parameter planner must preserve
this runtime exception to reflected nullability, including strict rejection,
without conflating explicit null with omission. Reference source is cached at
`/home/nahime/.cache/elephc-mbstring-test-tmp/php-mime-baseline/mbstring.c`.

## MIME callable ownership follow-up, 2026-09-08

The expanded decoder corpus now contains 82,373 cases and passes. Twenty focused
codec/coercion tests also pass (`/tmp/elephc-mime-codec-regressions.log`). The earlier
137-test native mbstring run passed before the ownership follow-up below.

A new allocation-slope regression exposed leaks in runtime-unknown native and eval
calls. Isolated one/four invocation baselines showed native leaks of two blocks per
scalar success, three per Stringable success, five per scalar arity/type failure,
and six/seven with nested object arguments. Eval leaked one scalar argument owner,
three blocks for a Stringable argument, and all argument owners on failure.
Standalone object creation/discard was balanced on both backends.

Native indexed mbstring descriptor invokers now borrow the normalized original
Mixed cells directly into the shared coordinator. This avoids unused retains in
the typed wrapper, removes an extra owned-string persistence copy, and preserves
actual arity and omitted defaults. All seven isolated native cases are balanced
(`/tmp/elephc-mime-ownership-native-fix.log`). EIR argument containers and the
codegen-created normalized box/optional descriptor are protected across throws.
The named-container adapter still uses the generic typed wrapper and remains an
explicit follow-up, together with invocation-site strictness propagation.

Eval now captures independent by-value mbstring arguments through the existing
source-order argument evaluator. Named and unpacked calls share that path, and
named-gap defaults join explicit cleanup. The contract controls eligibility and
keeps by-reference surfaces on their existing binder. Dynamic named mbstring targets use the same value-capture boundary. Five MIME
native/eval tests, including the new
allocation-slope test, passed (`/tmp/elephc-mime-ownership-eval-fix.log`).

PHP comparison then confirmed destructor order is parameter order, including
reordered named arguments. Cleanup now retains partial evaluation metadata,
orders argument releases by bound parameters, and drops temporary spread roots
once their copied elements are ready. A dedicated order test is running at this
checkpoint (`/tmp/elephc-mime-order.log`). No claim of final verification yet.

Other pending evidence from code inspection: the prebuilt Mixed argument-container
codegen path releases its operand, while descriptor EIR also emits a release for
an owning temporary. Verify and fix that ownership boundary with a runtime-unknown
named callable regression rather than assuming the current raw-array tests cover
it. Guarding an EIR container currently begins at invocation, so partial argument
construction failures also need earlier guard registration and relocation updates.

The dedicated destructor-order test passed on native and eval. Named descriptor
EIR now normalizes a private copy instead of allowing backend cleanup to consume
the EIR-owned input; its heap-debug success/type-error/success regression passed
(`/tmp/elephc-mime-named-heap.log`). Other legacy prebuilt-container consumers
remain unchanged and need their own ownership audit.

The guarded descriptor builder registers the initial indexed/hash container before
source argument evaluation. Indexed growth updates its guard, and boxing a hash
moves exceptional ownership to the resulting cell. Named hash insertions now drop
the inserted temporary's original reference after the hash acquires it. The six
allocation-slope cases (scalar, Stringable, later argument failure, growing spread
then failure, arity error, type error) pass on both native and eval with one versus
sixteen invocations (`/tmp/elephc-mime-evaluation-owners.log`). The generic dynamic
constructor/Closure::call builders retain their existing unguarded contract;
ordinary descriptor expressions use the new guarded builder.

Broad focused mbstring validation, callable by-reference regression, contract IDs,
registry/eval string units, and the docs exporter build are running sequentially
at this checkpoint (`/tmp/elephc-mime-final-*.log`). Note that the builtin boundary
Python audit can start Cargo internally, so do not overlap it with Rust builds.

While Rust validation ran, captured a preliminary 61,598-case PHP 8.5.10 MIME
encoder oracle with `scripts/mbstring/capture_mime_encode.php`, saved as
`crates/elephc-mbstring/tests/fixtures/mime_encode.jsonl.gz` (618 KiB). This is
preparation data, not an implemented encoder or a passing Rust test. It captures
all codec pairs, language defaults, explicit null versus omission, transfer-prefix
selection, line folds/separators/indent, stateful source restarts, and fixed MIME
replacement semantics. PHP syntax and the complete capture succeeded.

The broad native run found a fixture-only iOS boundary rejection: newly added
runtime-unknown MIME callable probes were inside `#[Export] mbstring_fixture`.
Opaque descriptor calls are intentionally rejected at exported library boundaries.
Move those probes outside the exported function and assert their invoker assembly
is still emitted on all targets. Do not weaken the export safety analysis.

MIME decoder validation checkpoint: the broad focused native/eval run completed
with 139 passing tests and only the expected iOS fixture rejection. Moving opaque
callback probes into a non-exported function fixed the remaining target test;
`mbstring_pointers_ready` is now asserted on all five targets. The MIME contract
error test, callback named-reference writeback regression, runtime-ID round trip,
30 compiler registry tests, and 26 eval string tests passed. The exporter build,
complete generated-doc workflow, docs/site audits, builtin EIR target boundary
audit, and runtime plus application assembly for all five targets passed. Evidence
is in `/tmp/elephc-mime-final-*.log`. Generated docs now include the decoder.

A separate partial-argument regression confirmed leaks when a later argument
throws before `call_user_func` or a reference-aware literal `call_user_func_array`
can invoke the target. One/four-call residuals were native [3, 12] and [4, 16],
eval [9, 18] and [13, 34]. Native argument builders now guard their initial array
and update guards after insertion. Eval indexed literals clean their partial
array and temporary keys on failure; positional mbstring `call_user_func` routes
through the same owned-value argument boundary as direct mbstring calls. Focused
verification is running in `/tmp/elephc-mime-cuf-partial-after.log`.

Additional pending call semantics: named `call_user_func($callback, string: ...,
extra: ...)` is rejected by the compiler's builtin parameter planner even though
PHP forwards those names to the callback. PHP evaluates a throwing later named
argument before invocation. A positional argument after unpack is invalid PHP;
use a later named argument when testing that evaluation sequence. The generic
unknown-callback native path currently accepts some such invalid positional forms.

Correction to the previous hash-insertion checkpoint: the call to
`release_value_after_retaining_insert(..., Mixed, ...)` is currently a no-op by
that helper's storage contract. It does not by itself prove named hash argument
temporaries are balanced. Keep the named-container allocation audit pending.

The partial `call_user_func`/literal call-array ownership regression now passes on
native and eval (`/tmp/elephc-mime-cuf-partial-after.log`). The native descriptor
reference test, eval callback reference/value tests, mbstring call-array allocation
regression, 41 eval dynamic-call units, 13 array-literal units, and the target
fixture with both throwing callback forms all passed (`/tmp/elephc-mime-cuf-*.log`).

MIME encoder preparation now has a bounded source-decoder API,
`Encoding::decode_next`, which advances the input and preserves the exact shift
state at the first output-buffer boundary. Stateful decoders share their existing
atomic parser with this stop mode; stateless codecs use bounded lookahead. MIME
decoding now consumes those real invocation boundaries. Two focused units check
all 79 codecs across six capacities and a UTF-7 mid-shift restart. Both pass, and
the complete 82,373-case MIME decoder oracle still passes
(`/tmp/elephc-mime-stream-bounded.log`, `/tmp/elephc-mime-stream-oracle.log`).
Focused codec regressions are running in `/tmp/elephc-mime-stream-codecs.log`.
The public encoder and its destination-buffer rollback are not implemented yet.

The bounded-reader follow-up passed all seven affected codec test binaries (JIS,
HZ, ISO-2022-KR, UTF-7, transfer codecs, split batches, and transform batches),
plus native/eval MIME public calls and request-state changes. The full exporter
build and generated-doc checks are running again after these changes in
`/tmp/elephc-mime-stream-final-*.log`.

Expanded the pending MIME encoder oracle to 67,550 cases. The additional 5,952
cases place JIS-2004 compositions, CP50220 kana pairs, mobile flag/keycap pairs,
and a trailing pending digit around trial-chunk and line boundaries, with two
transfer modes and two indents. The capture succeeded with PHP 8.5.10.

Next encoder implementation notes: `Encoding::decode_next(&mut input, capacity,
&mut state)` now provides the needed exact source cursor. Its empty-input call is
intentional: PHP's MIME encoder may call a decoder with no bytes remaining, and
stateful transfer/UTF-7 decoders can still have EOF behavior. Preserve the
90-word initial ASCII scan state when restarting the source. Destination encoding
still needs provisional non-final output plus a separate empty final flush,
restoring both bytes and encoder state after a rejected trial. Ordinary complete
encoding is insufficient for pending CP50220/mobile digits or UUENCODE call
boundaries. Replaying only the bounded current line's accepted chunks can be
linear in total header size, but do not re-decode the entire unread source suffix.

The final bounded-reader exporter build and full generated-doc/target-boundary
audits passed without warnings or structural errors. New Rust modules all have
preambles. Corrected one pre-existing newly added GC comment to respect the
no-em-dash preference; touched assembly-comment alignment and `git diff --check`
pass. No Cargo or capture process remains active at this checkpoint.


## MIME encoder integration checkpoint

Added `mb_encode_mimeheader` as operation 64 through the neutral catalog, AOT and
Magician bindings, and shared ABI. Current public inventory is 44/65 functions,
43 shared operations, and 8/9 constants. Full extension work remains active.
The encoder and ABI pass all 71,154 captured PHP requests (70,026 successful
outputs), and MIME decoding still passes its 82,373 headers. Trial encoding
preserves final flush bytes, JIS-2004 composition, and SJIS-mac's record-zero
resume quirk. PHP's early return for highly compact Apple compositions is captured
and reproduced instead of triggering a Rust assertion.

Explicit MIME charset/transfer nulls parse as nonnullable strings, while null
warnings retain the reflected `?string` spelling. Direct PHP probing disproved the
prior assumption that named holes must retain a special omitted marker: PHP fills
holes with defaults before parsing them, so skipped MIME charset/transfer slots
also deprecate and coerce null. Trailing omissions must remain absent.

The first native/eval public-call, state, and runtime-error tests pass. A new
runtime-selected named-call test exposed full-signature padding in the native
associative invoker. The mbstring-specific path now stages borrowed raw cells,
tracks the final supplied slot, fills only internal holes, and invokes shared
coercion directly. This removes typed-wrapper return adaptation for associative
mbstring calls. That regression passes on AOT and eval. Named allocation-slope,
existing heap checks, and all-target validation are the next gates.

Outstanding named-invoker work remains: unknown-name and missing-required-name
errors, duplicate/positional ordering, actual excess positional arity in mixed
hashes, caller strictness, and ownership of the argument-container construction.
These pre-existing gaps must be resolved before full mbstring completion.
Evidence: `/tmp/elephc-mime-encoder-{mac-fixed,codegen-second,named-fix}.log`.

Named allocation-slope follow-up: native direct, associative callback, and error
paths are balanced. Eval associative callback calls leaked four allocations per
call because associative literal key boxes/strings were never released. The
shared eval literal builder now owns and releases keys, cleans numeric next-key
scratch values, updates the array handle before cleanup, and releases partial
arrays on failure. The three success/validation-error regressions pass after the
fix. Splitting them keeps each CI test below the global per-test time budget.
The new partial-array regression exposed a separate native callback hash leak
(3 versus 12 residual allocations for 1 versus 4 calls). A guarded associative
literal mode now protects only callback argument construction, including hash
relocation, and leaves ordinary literal lowering unchanged. Its focused regression
is running in `/tmp/elephc-mime-encoder-partial-hash-fixed.log`.

Correction to the earlier Mixed insertion concern: `HashSet` consumes an owning
Mixed producer through `retain_hash_refcounted_value_if_borrowed`; the no-op in
`release_value_after_retaining_insert` is therefore not evidence of a named
argument leak. The new native allocation slopes confirm balanced ownership for
these named object calls after the shared associative invoker adaptation.


MIME encoder verification complete for this checkpoint. The partial associative
callback regression now passes on AOT and eval; a distinct typed
`ExceptionUpdateHashGuard` preserves EIR's array/hash distinction while sharing
its backend pointer refresh. Named success, mixed numeric/string-key arrays,
validation failures, and partial source-evaluation failures have stable allocation
slopes. Magician's 13 array-literal tests and 41 dynamic-call tests pass, as do the
existing named heap-debug regression, static errors, 30 builtin registry checks,
runtime ID checks, and the focused JIS/HZ/ISO-2022-KR/UTF-7/transfer/batch/coercion
regressions. The build is warning-free. All five supported targets pass both
runtime and user assembly through Clang, including the new typed hash guard.

The builtin exporter, generated module/comparison pages, documentation audit,
site compatibility, and EIR boundary audit pass. Reviewing the rendered MIME
signature exposed Python `repr` incorrectly displaying the CRLF default in PHP
single quotes. `_render_default` now emits valid PHP string literals, with
explicit control-byte escapes and safe dollar/backslash quoting. Its four unit
tests and six PHP byte roundtrips pass; docs were regenerated and audited again.
The complete mbstring example produces the same 1,074 stdout bytes as PHP.
`git diff --check`, touched assembly-comment alignment, and new Rust module
preambles pass. No Cargo, capture, or verification process remains active here.
Logs: `/tmp/elephc-mime-encoder-verify-*.log`,
`/tmp/elephc-mime-encoder-docs-final-*.log`, and
`/tmp/elephc-mime-encoder-partial-hash-typed.log`.

Next bounded function candidate: `mb_get_info`, audited read-only against the
cached PHP 8.5.10 source at mbstring.c lines 4775-4929. Its reflected return is
`array|string|int|false|null`, with optional string `type = "all"`. It compares
selectors case-insensitively with full length (not C-string truncation). `all`
omits unset `http_input`, but the individual `http_input` selector returns null.
Unknown selectors warn `argument #1 ($type) must be a valid type` and return false.
Current State already owns language/mail defaults, internal/output encoding,
detection order, substitution, strict detection, and illegal-character count.
HTTP input identification, encoding-translation configuration, and the output
MIME-type expression need authoritative request fields/adapters. The current
result wire has no successful null kind yet (1 through 11 are occupied), so do
not disguise the `http_input` result as false or an empty string. No code for this
next function has been added yet. The complete goal remains active at 44/65
public functions, 43 shared operations, and 8/9 constants.

## Information function completion, 2026-09-08

Added `mb_get_info` through the neutral contract, AOT and Magician homes, and
shared operation 65. The public scope is now 45/65 functions, with 44 shared
operations and 8/9 constants. The complete objective remains active.

`State::info` owns selector behavior and insertion-ordered snapshots. It reads
actual language/mail defaults, internal/output encoding, detection order,
substitution mode, strict detection, and the conversion-error count. Selectors
compare full bytes case-insensitively, including embedded NULs. Missing HTTP
identification is omitted from `all`, but a standalone selector returns null.
Invalid selectors warn and return false. State now also owns the input
identification, original output MIME expression, and translation-enable setting.
Their host setter APIs are present; public INI and request-parser adapters are
still required and must not be claimed complete.

Added `RESULT_NULL = 12` to the existing result wire and both architecture
materializers/boxers, using runtime null tag 8. Every array result uses the
existing owned graph or indexed-string restoration path. `mb_get_info` runs after
all Stringable selector callbacks, so changes in those callbacks are observable.

The reproducible PHP oracle has 5,040 rows, including all thirteen selectors and
`all`, twelve languages, six substitution settings, repeated detection entries,
MIME expressions, case variations, NULs, invalid/removed selectors, arity, and
weak coercion. All three engine/ABI tests pass. The three native/eval codegen
tests pass, including named calls, snapshot independence, state mutation,
null/false identity, diagnostics, and repeated allocation/free balance. Eval's
pre-existing parser rejection of `@` was encountered and remains a tracked core
language gap: the native branch checks suppression; the eval branch explicitly
checks the unsuppressed warning instead, matching the existing detection tests.

Focused static diagnostics, the twelve-request web-worker reset regression,
30 registry checks, runtime ID checks, and five-target EIR emission pass. Clang
assembles both full runtime and user code for all five supported targets. The
warning-free exporter build, generated docs/module/comparison pages, builtin
audit, site compatibility, and typed EIR boundary audit pass. The updated example
has 1,100 stdout bytes, exactly matching PHP with empty native stderr.
Logs: `/tmp/elephc-mbstring-info-engine.log`,
`/tmp/elephc-mbstring-info-codegen.log` (initial eval-parser failure),
`/tmp/elephc-mbstring-info-errors-fixed.log`, and
`/tmp/elephc-mbstring-info-verify-*.log`. Verification runner
`/tmp/elephc-mbstring-info-verify.py` completed successfully; no own Cargo or
verification process remains active.

Read-only preparation for HTTP/INI follow-up:

- Cached PHP source remains at
  `/home/nahime/.cache/elephc-mbstring-test-tmp/php-mime-baseline/mbstring.c`.
  `mb_http_input` is at 1259-1335, `mb_parse_str` at 1512-1548, INI handlers at
  681-929, directive declarations at 932-957, and request initialization at 1107.
- `mb_http_input(?string $type = null): array|string|false` accepts omitted/null
  for aggregate identification, exactly one byte G/P/C/S for per-source identity,
  I for the configured input-encoding list, L for comma-joined configured names.
  Letter case is ignored; any other string throws ValueError. Unset identities
  return false, while I returns an array even if empty; L returns false if empty.
  Default CLI result is `[false, ["UTF-8"], "UTF-8"]` for null/I/L. Identification
  needs pass as well as ordinary Encoding identities. Parsing updates aggregate
  and source-specific fields; it cannot be inferred from current detect_order.
- INI has eleven mbstring entries. All have access 7 except
  `mbstring.encoding_translation` (6). Null raw defaults: detect_order,
  http_input, http_output, internal_encoding, substitute_character. Other raw
  defaults: language neutral, strict_detection 0, encoding_translation 0,
  regex_stack_limit 100000, regex_retry_limit 1000000, and the existing MIME
  expression. Preserve raw local/global nulls in ini_get_all, separately from
  resolved settings and empty-string ini_get/ini_set results.
- `ini_set(mbstring.detect_order, ...)` updates configured defaults but DOES NOT
  change the current request's mb_detect_order or mb_get_info detection list.
  Language, internal/output encodings, and substitute settings update live state.
  Deprecated internal/http encodings emit diagnostics on nonnull raw INI writes.
  Invalid language resets the language to neutral even though the INI write fails.
  Invalid internal encoding warns, falls back to UTF-8, and still accepts raw text.
  INI substitution parses C strtol(base 0) cast to int, without public-function
  codepoint validation. Subsequent probes confirmed getters expose the cast value
  as unsigned 32-bit, including 4294967295 for -1.
- MIME expression acceptance uses PCRE2_CASELESS, trimmed then zero-terminated,
  while mb_get_info retains the original raw bytes. This is PHP's PCRE2 usage,
  distinct from mbregex's required Oniguruma semantics.
- Public CLI INI wrappers are generated in src/opcache_prelude/build.rs at 737,
  750, 778, injected by opcache_prelude/injection.rs at 198+. CLI ini_get consults
  only OPcache and ini_set always returns false. Web wrappers are generated in
  src/web_prelude/build.rs (ini_get 4355, ini_set 4451, ini_get_all 5028).
  `src/version_prelude.rs::ini_restore_decl` is currently a no-op with stale
  all-settings-immutable documentation. Integrating mbstring must keep existing
  OPcache/session dispatch and honor user-defined function guards.
- CLI --ini already retains non-OPcache assignments but ignores them downstream;
  pipeline passes overrides to OPcache and web prelude injection. Startup config
  must also reach repeated native/web requests and opaque eval using the one
  bridge-owned state, with no second interpreter-local settings copy.

## HTTP input information completion, 2026-09-08

Added `mb_http_input` as shared operation 66 with a single neutral signature and
ordinary AOT/eval bindings. The exposed scope is now 46/65 functions, 45 shared
operations, and 8/9 constants. Remaining public names are mb_convert_variables,
mb_ereg, mb_ereg_replace, mb_ereg_replace_callback, mb_ereg_search,
mb_ereg_search_getpos, mb_ereg_search_getregs, mb_ereg_search_init,
mb_ereg_search_pos, mb_ereg_search_regs, mb_ereg_search_setpos, mb_eregi,
mb_eregi_replace, mb_output_handler, mb_parse_str, mb_regex_encoding,
mb_regex_set_options, mb_send_mail, and mb_split. Legacy mb_ereg_match still needs
the shared Oniguruma replacement. Full goal completion is not claimed.

`State::http_input` validates complete one-byte selectors and returns the
configured names or a previous identification. State now owns a configured
`Vec<OutputEncoding>` (default UTF-8) and four independent optional source
identifications, alongside the aggregate field used by mb_get_info. Pass is a
real identification alternative. Getters do not detect input and do not derive
candidates from current internal encoding, language, or detect_order. Empty I
returns an array; empty L and unset source/aggregate values return false.

Source setters intentionally do not alter the aggregate automatically. PHP
`mb_parse_str("a=1", $v)` was cross-checked: aggregate becomes UTF-8 while S stays
false. The ordinary mbstring-aware SAPI treat-data path updates both; direct
mb_parse_str only changes aggregate. Source is now cached at
`/home/nahime/.cache/elephc-mbstring-test-tmp/php-mime-baseline/mb_gpc.c`.
Its treat-data path clears the selected source before parsing, resets illegal
characters, and writes aggregate plus the selected successful identification.
Public mb_parse_str calls `_php_mb_encoding_handler_ex` directly. Do not collapse
these state transitions into one unconditional source-plus-aggregate setter.

The new reproducible HTTP input fixture has 2,176 PHP 8.5.10 rows, including all
256 selector bytes, exact-length failures, weak and nullable parsing, eight
configured lists, aliases, duplicate entries, pass, transfer encodings, and all
79 encodings in one list. Three engine/ABI tests pass, including source/aggregate
independence and empty configured lists. The 5,040 mb_get_info fixture cases and
its engine/ABI tests also remain green. Two native/eval tests cover every result
shape, named and first-class/dynamic calls, nullable getters, Stringable callbacks,
and exact catchable errors. The updated repeated-result GC regression passes
for native and eval. Six static contract cases, the repeated-worker web test,
30 registry tests, runtime IDs, and five-target emission pass.

All full runtime and user assemblies pass Clang on the five supported targets.
The exporter build is warning-free; generated builtin docs, module/comparison
pages, documentation audit, site compatibility, and EIR boundary audit pass.
The complete example exactly matches PHP at 1,128 stdout bytes and no native
stderr. `git diff --check`, assembly-comment alignment, and the twelve new Rust
module preamble/punctuation checks pass. The final test-only empty-buffer safety
change was verified by rerunning all three HTTP input engine tests successfully.
Logs: `/tmp/elephc-mbstring-http-input-engine.log`,
`/tmp/elephc-mbstring-http-input-engine-final.log`, and
`/tmp/elephc-mbstring-http-input-verify-*.log`. Serial runner
`/tmp/elephc-mbstring-http-input-verify.py` completed. It used `--features curl`
for focused root tests to reuse the same feature set as the docs exporter. No
own Cargo, capture, or verification process remains active.

HTTP parser adapters and public INI integration are still outstanding. The
getter fields and their Rust host setters must not be mistaken for completed
PHP ini_set/ini_get/ini_restore support. Further startup integration should audit
`src/codegen/block_emit.rs::emit_main_function` (around 938), which currently
calls `__rt_mbstring_request_reset` once per CLI program or web request; the
runtime helper is in strings/mbstring/catalog.rs and invokes the bridge reset.
BackendInputs/IR Module currently do not carry ini_overrides. Preserve cached
runtime objects, exported-library initialization, optional bridge linking,
opaque eval configuration, and per-request reset when adding startup settings.

## Shared INI engine and MIME provider foundation, 2026-09-08

The public count remains 46/65 functions, 45 shared operations, and 8/9 constants.
This step adds tested shared configuration machinery; public INI routing is still
pending and must not be reported as complete.

Implemented `crates/elephc-mbstring/src/state/ini/`:

- One eleven-entry catalog owns exact keys, nullable raw defaults, access bits,
  sorted getter order, and PHP directive-registration order.
- `ini_get`, `ini_get_all`, `ini_set`, and `ini_restore` preserve raw configuration
  independently of resolved encodings, current detection order, and substitution.
  Failed writes still mark entries modified. Repeated unmodified restores do not
  rerun handlers. For a null original value, PHP's `ini_get_all` global field
  falls back to the current local value, even though restore still uses null.
- `with_ini_configuration` uses the final override for each key, handles rejected
  startup values, and runs the inherited-encoding hook after registration.
  Before handlers run, PHP initializes output to pass and input candidates empty;
  invalid inherited encodings retain those values. Explicit valid defaults produce
  the normal UTF-8 request state. Startup diagnostics preserve their distinct
  PHP Startup prefix and the repeated hook warnings for invalid inherited input.
- Immutable startup core defaults are separate from request-local core defaults.
  `reset_ini_request` reconstructs startup settings and clears counters, input
  identifications, lookup caches, public setters, and runtime INI mutations.
- Numeric handlers preserve PHP 8.5.10 quantity suffix/overflow diagnostics,
  C atoi boolean handling, and C strtol replacement parsing, including binary
  prefixes supported by the captured baseline. Substitution getters expose all
  unsigned 32 bits, so -1 becomes 4294967295. Invalid text can retain the remembered
  replacement character while switching away from entity/long/none mode.
- Public `mb_language` now also changes the raw language INI entry and modified
  bit. Public internal/output setters set their explicit-inheritance flags but
  leave raw INI text untouched. Failed explicit input/output INI writes can also
  protect the prior effective encoding from subsequent core-default changes.
- INI encoding lists reuse the existing encoding-list parser with the correct
  INI warning context, including startup versus runtime function spelling.
- MIME handlers retain original text, pass only PHP-trimmed C-string text to a
  supplied native validator, and preserve the previous expression on failure.

Independent fixtures and verification:

- `scripts/mbstring/capture_ini.php` captured 9,446 traces, each containing a
  write and two restores, on PHP 8.5.10. They cover binary keys and values, all
  encoding identities, settings changed by public functions, every byte in
  numeric positions, raw metadata, exact diagnostics, and effective state.
- `capture_ini_startup.py` captured 141 fresh-process startup profiles, including
  duplicated assignments, language-dependent auto expansion, rejected defaults,
  explicit core encodings, and invalid inherited core encodings.
- Five INI integration tests pass, including every oracle case, independent
  MIME-validator boundary checks, explicit-override inheritance, and request reset.
  The existing state, encoding-list ordering, information, and HTTP input engine
  tests also pass. Final engine log: `/tmp/elephc-mbstring-ini-engine-final.log`.
- The capture exposed a PHP 8.5.10 ownership defect: mb_get_info's MIME field
  transfers a borrowed zend_ini_str result without retaining it. Repeated dynamic
  INI MIME writes plus discarded information arrays can corrupt PHP's heap.
  The fixture reads individual selectors and uses ini_get for the MIME field to
  avoid invoking that faulty ownership path. Elephc keeps independently owned
  snapshot strings, as already covered by native/eval result-ownership tests.

Extended the managed PCRE2 shim with four native 8-bit ABI functions:
`elephc_pcre2_v1_mime_compile`, `elephc_pcre2_v1_mime_match`,
`elephc_pcre2_v1_mime_free`, and `elephc_pcre2_v1_error_message`.
The shim uses PCRE2's caseless compile API and preserves diagnostic byte offsets,
explicit lengths, allocation ownership, nonmatch versus errors, and invalid-range
failure. Existing POSIX regex handles retain their separate lifetime and layout.
The host C contract harness passes for both old and new APIs. The managed package
is now recipe revision 3; revisions 1 and 2 fail the dispatcher gate. Test-provider
metadata was updated, and all three PCRE2 example lockfiles were regenerated with
`elephc native update pcre2 --offline`, using the cached official source archive.
This also built and installed the actual revision-3 host package successfully.

Required next integration work:

1. Add the native provider table and panic-contained INI bridge protocol. The
   current pure State methods collect diagnostics and are not sufficient for PHP
   error-handler reentry. Do not wire them to emit warnings only after completion.
2. Preserve PHP's warning-time state transitions and old-value capture. A probe
   in `/tmp/elephc-mbstring-ini-reentry.php` shows an outer internal-encoding write
   to SJIS whose deprecation handler writes ISO-8859-1: the outer result remains
   the original empty raw string, final raw INI is ISO-8859-1, and effective
   encoding is SJIS. PHP skips publishing the outer raw value when the handler
   replaced it. A throwing deprecation handler still lets the C handler complete
   and leaves both raw/effective SJIS before the exception is caught. No PHP
   callback may run under the engine's RefCell borrow.
3. Route CLI/web ini_get/set/restore/get_all through the one engine while keeping
   existing OPcache/session behavior and user declaration guards. Enforce PHP's
   ini_set scalar-only value coercion, including rejection of Stringable objects.
4. Carry startup overrides through compiler/library initialization, request reset,
   and opaque eval. Resolve core default_charset/internal/input/output settings
   and changes consistently. Preserve optional bridge linking and runtime caches.
5. Register/require the managed PCRE2 MIME provider where needed, then implement
   output-handler and HTTP parser consumers. Oniguruma remains the separate,
   required mbregex engine; the MIME PCRE2 API is not an mbregex substitute.

Final focused verification for this step completed successfully:

- Warning-free compiler build with `--features curl`.
- Shared engine: 15 tests across INI, state, encoding-list ordering, information,
  and HTTP input, including 9,446 runtime INI traces and 141 startup profiles.
- Managed native dependency units: 75 passed, including the current recipe
  dispatcher, previous revision rejection, catalog, cache, and installation gates.
- PCRE2 native C shim ABI: 1 passed for the existing POSIX and new MIME surfaces.
- Executable regex regressions: 23 preg_match tests and 1 invalid-pattern family
  test passed with the revision-3 test provider.
- PHP/Python capture syntax and `git diff --check` passed.

Logs are `/tmp/elephc-mbstring-ini-{build,tests,startup-tests,engine-final,pcre2-shim,root-build,managed-pcre2}.log`
and `/tmp/elephc-mbstring-ini-verify-*.log`. The sequential runner
`/tmp/elephc-mbstring-ini-verify.py` exited successfully. No own Cargo or verification
process remains active. No commit, PR comment, closure, agent, or cargo fmt was used.


## Reentrant INI state, native ABI, and string ownership, 2026-09-08

The verified public scope is still 46/65 functions, 45 shared operations, and 8/9
constants. This step extends the shared configuration foundation and its C ABI.
Public ini_get/set/restore/get_all routing is not yet implemented.

Completed the first two integration items in the preceding section:

- Pure state calls and protected ABI calls now use the same handler implementation
  through short `Access` borrows. Deprecations, invalid-encoding warnings, numeric
  warnings, and inherited core hooks release every request borrow before calling
  the PHP diagnostic adapter. A pending throwable suppresses further PHP callbacks
  but does not skip the remaining native handler state changes.
- Mutations save the old return value and raw identity before callbacks. The outer
  write publishes its raw text only if the live slot still has that identity.
  Restore consumes the live original slot and saved access bits after callbacks.
  Nested restore can clear both, so the result can be raw null with access zero;
  using the immutable catalog access bits was incorrect and is now covered.
- `capture_ini_reentry.py` captures 1,256 independent PHP 8.5.10 processes, covering
  936 warning/state/exception combinations plus 320 string-identity combinations.
  All captured warning-time snapshots, inner results, outer results, final state,
  and pending exception outcomes match the engine.
- A retained `IniString` separates byte equality from immutable string identity.
  Startup values and literals share interned identities. Ordinary runtime values
  retain distinct allocations. Scalar getters and ini_set old-value returns
  normalize noninterned one-byte strings; ini_get_all retains raw identities.
  Explicit fresh empty strings remain distinct from canonical empty strings.
- Two PHP ownership defects are deliberately not reproduced. The prior MIME
  information getter issue remains documented above. In the new reentry fixture,
  an outer write plus a nested equal-byte copy can over-release Zend's previous
  raw string and corrupt the original JSON-decoded input array. The worker takes
  all observations first and reconstructs only the fixture input fields from the
  immutable original JSON before serialization. Engine ownership stays balanced.

New dependency-neutral contracts in `mbstring_abi::ini` define the complete MIME
provider table, protected INI host table, operation IDs, and checked INI graph
identity framing. The provider uses only native PCRE2 callbacks. Engine C exports:

- `elephc_mbstring_mime_provider_v1`: process-lifetime complete provider registration,
  idempotent only for the same callback and allocator function set.
- `elephc_mbstring_ini_v1`: GET, SET, RESTORE, GET_ALL, fresh-string import, and
  interned-string import. Normal directives and text calls use the same REQUEST.
- `elephc_mbstring_configure_v1`: first three effective core encoding defaults,
  then raw mbstring key/value overrides. Installs an immutable process prototype;
  exact repeated input is a no-op, differing configuration fails closed. Startup
  diagnostics must not invoke PHP user code. New threads and request resets clone
  the prototype, not a live request. Reset reuses validated MIME configuration.
- `elephc_mbstring_core_encoding_v1`: request-local inherited encoding updates with
  protected warning reentry, preserving explicit overrides and startup defaults.
- `elephc_mbstring_ini_string_retain_v1` and `_release_v1`: explicit host identity
  leases. Result kinds 13 and 14 own string/graph identity leases in addition to
  their byte buffers. Kind 13 stores one identity in value; kind 14 stores the
  graph prefix length in value and appends array/entry/identity records. Standard
  result release reclaims every lease. Expired IDs fail, and IDs never reuse an
  earlier allocation. No historical raw strings remain rooted after their last
  host lease and state owner disappear.

Provider validation frees every published handle, including malformed responses.
Only actual PCRE2 syntax errors become PHP warnings; missing providers, null-success
handles, allocated error handles, malformed error messages, and invalid ABI ranges
remain fatal. The native integration loads the actual embedded C shim and checks
caseless PCRE2 semantics, lookbehind, byte offsets, PHP trimming, NUL boundaries,
rejected writes, and restores. Linux CI has libpcre2-dev and pkg-config in its image;
macOS uses the existing Homebrew pcre2 dependency directly.

Required next integration work, replacing the preceding section's pending list:

1. Add native/eval INI dispatch using these contracts. Preserve PHP string identity
   through native aliases, literals, getter results, and array cells. A native
   string metadata owner must retain/release its identity lease across final
   string cleanup and request arena reset. Merely copying equal bytes into
   ARG_STRING is insufficient for existing strings. Share this path with eval.
2. Register the managed PCRE2 provider and declare the INI runtime requirement on
   all five supported targets. The default MIME expression and output-handler
   consumers must not rely on system fallback. Oniguruma remains a separate
   required mbregex dependency, not replaceable by the MIME provider.
3. Route CLI/web ini_get/set/restore/get_all while preserving OPcache/session
   behavior and user declaration guards. Use one scalar-only ini_set value
   coercion path; reject arrays/resources/objects, including Stringable objects.
4. Carry startup overrides and effective core defaults through program/library
   initialization, worker reset, and opaque eval. The core-update ABI currently
   models ini_set warnings; provide the proper caller context for core ini_restore.
5. Implement output-handler and HTTP input-parser consumers, then the remaining
   public mbstring functions and the tracked full-scope integration gaps. Do not
   raise the public completion count for this internal configuration machinery.

Verification logs for this step use `/tmp/elephc-mbstring-ini-identity-*.log`.
The sequential runner `/tmp/elephc-mbstring-ini-identity-verify.py` records the final
contract, engine, compiler, native/eval getter, and reused-web-worker checks.


Final verification completed successfully for this step:

- Neutral contract: two layout and malformed-identity-graph tests.
- Shared engine: 20 focused tests across INI, ABI, identity ownership, actual native
  PCRE2, state, encoding lists, information, and HTTP input. The INI tests include
  9,446 runtime traces, 141 startup profiles, and all 1,256 reentry traces.
- Warning-free root compiler build with `--features curl`.
- Five native/eval information and HTTP input codegen tests, plus the reused-web-
  worker request reset test, passed.
- Final ownership review removed deferred weak-index cleanup: the last interned
  string owner now retires its byte key immediately. A dedicated unit regression
  verifies retirement and nonreuse of identity. After that fix, all ten focused
  INI/ABI/native-provider tests, the native/eval result-ownership codegen regression,
  and the reused-web-worker reset test passed again.
- PHP/Python capture syntax, INI Rust module preambles and punctuation, and
  `git diff --check` passed.

Both sequential verification runners exited successfully. Logs include
`/tmp/elephc-mbstring-ini-identity-verify-*.log`,
`/tmp/elephc-mbstring-ini-intern-retirement.log`, and
`/tmp/elephc-mbstring-ini-retirement-verify.log`. No own Cargo or test process remains
active. No commit, PR comment, closure, agent, or cargo fmt was used.

Native integration investigation for the next step:

- Magician already carries opaque native runtime cells (`value.rs`), so INI
  identity can be implemented once on the native payload and shared with eval.
- `__rt_heap_free` is the allocator's common final-release point, and the existing
  object-handle side table demonstrates metadata retirement before payload reuse.
  Do not put an INI identity in spare heap-header bits: AArch64 cycle collection
  masks them and x86_64 uses upper bits for its marker. Keep optional bridge linking
  and the register-preservation contract intact if adding a release hook.
- `__rt_mbstring_request_reset` currently releases the native catalog then resets
  the Rust request. Any native INI identity map must also retire its leases before
  a web arena reset; it must not keep historical native strings alive indefinitely.
- Public wrappers are in `opcache_prelude` (CLI), `web_prelude/build.rs` (web), and
  `version_prelude.rs` (currently no-op ini_restore). Their selection and user
  declaration guards remain authoritative while adding shared mbstring routing.


## Native INI identity ownership hooks, 2026-09-08

The preceding goal turn was verified implementation progress. This step adds
native ownership integration without changing the public mbstring count:
46/65 functions, 45 shared operations, and 8/9 constants. Public INI adapters
remain unwired, and complete mbstring support is not yet achieved.

Implemented native allocation metadata in the shared bridge:
`crates/elephc-mbstring/src/abi/ini/native.rs` exports versioned bind, lookup, copy,
forget, and reset entry points. Metadata is thread-local and owns one explicit
Rust identity lease per tracked native allocation, never a native heap reference.
Repeated identical binding is idempotent. Replacement acquires its lease before
releasing the old one. Copy preserves only a complete, already tracked source
range; differing lengths and untracked sources do not inherit identity. Retiring
or resetting metadata removes leases outside the map borrow. Invalid identities,
zero destination allocations, invalid lengths, and expired owners are covered.

Connected the native lifecycle on both architectures:

- `__rt_str_persist` copies a tracked source identity to newly owned storage,
  covering value boxing and ownership stabilization without keeping source heap
  allocations alive. Its existing concat-temporary takeover path keeps the same
  allocation and therefore the same metadata owner.
- `__rt_heap_free` calls the metadata forget hook before payload poisoning,
  free-list insertion, coalescing, bump rollback, or native address reuse.
- Catalog/request cleanup also resets native identity metadata before a web arena
  reset. Rust-only request reset remains separate from native allocation lifetime.
- `__rt_mbstring_ini_bind/copy/forget/reset` adapt native registers to C and preserve
  every general/vector register across Rust. SysV wrappers realign either existing
  runtime leaf stack phase locally. Internal contract failure exits unsuccessfully
  after Rust returns. Copy/free/reset stay dormant until native binding activates
  the request flag. Programs without mbstring/eval emit none of these hooks or
  their bridge calls; their complete absence is verified across all five targets.

Independent verification:

- The engine metadata test checks repeated binding, exact-length copying, failed
  metadata, final-lease expiry, reused addresses, and thread-local reset isolation.
- The executable C/assembly harness loads actual emitted persistence, heap-kind,
  and heap-free helpers with a separate bounded C heap and forwards identity
  calls to the real bridge. It verifies dormant hooks, copied identity, final
  release before address reuse, external leases surviving native reset, and
  balanced heap counters. A deliberate C clobber of integer and SIMD registers
  is surrounded by full register snapshots, proving the wrapper restores them.
- Root emitter checks cover ordinary/PIC symbol and frame variants for all five
  targets. The full current runtime and existing user probes were assembled with
  Clang for macos-aarch64, ios-arm64, ios-sim-arm64, linux-aarch64, and linux-x86_64.
- A warning-free root build, both native/eval string-result GC regressions, and
  the reused-web-worker request reset test passed. The native identity and
  ownership ABI tests also passed. Assembly comments and git diff checks passed.

Logs: `/tmp/elephc-mbstring-ini-native-{metadata,runtime,registers}.log` and
`/tmp/elephc-mbstring-ini-native-verify-*.log`. The sequential runner
`/tmp/elephc-mbstring-ini-native-verify.py` exited with ALL PASSED. No own Cargo,
compiler, or test process remains active. No commit, PR comment/closure, agent,
or cargo fmt was used.

Required next work before routing public INI calls:

1. Register logical string origins before the first relevant by-value copy. The
   new copy hook preserves an already bound identity; it does not yet establish
   identity for all source literals, fresh runtime values, or aliases made before
   any INI binding. For example, `$b = $a` before `ini_set($key, $a)` must preserve
   the PHP relationship when a warning callback later passes `$b`. Merely binding
   the wrapper's already-copied parameter is too late.
2. Distinguish literal/interned values, fresh temporary results, and stabilization
   of an existing value. Audit source-data registration, constant folding, boxed
   native/eval copies, short/empty string construction, and logical string mutation.
   Host mutation must retire identity before changing bytes in place. Transforming
   strings must not inherit old identity merely because a helper first copied them.
   Existing str_inc_dec transforms scratch copies; strtr/count_chars use separate
   result reservations, but the complete origin/transform audit is still required.
3. Materialize RESULT_INI_STRING and RESULT_INI_ARRAY through the native binding
   hooks, preserving cell identities and balanced leases on exceptions. Wire
   protected typed INI dispatch and scalar-only value coercion through AOT/eval.
4. Complete CLI/web wrapper routing and declaration guards, startup/core defaults,
   managed PCRE2 registration/requirements, output handling, and HTTP input parsing
   as specified in the preceding section. The remaining 19 functions, legacy regex
   replacement, ninth constant, and earlier full-scope gaps remain pending.

Live read-only PR refresh again found exactly the five tracked open mbstring PRs,
with unchanged heads: #895 e96b43f219c085551a3c42c4cbbbd752846c0f88,
#898 b15d629eb1d159ffb32fe272eb0f5c17472fb97b,
#899 089ffc6bee2d2b3f17746da6fdf435f5be423440,
#900 f402f762e093b4ad1c98bee1c83cb59bb4278c8c, and
#902 405f77283dd4e5c805eff74dc70f3fe013b56885. No remote writes were performed.


### Lazy native INI string origins, 2026-09-08

Verified implementation progress. Public coverage remains 46/65 functions,
45 shared operations, and 8/9 constants. Public INI routing is still pending.
This section advances the origin-registration prerequisites recorded above.

- Native allocation metadata now shares an Rc origin containing exact length,
  literal/fresh provenance, and an initially empty identity slot. Copying native
  aliases acquires no Rust byte copy or global string lease until INI resolves
  the origin. Resolution copies the current live owner's bytes once and publishes
  one lease shared by every native alias. Removing the original native allocation
  before the first resolution does not invalidate surviving aliases.
- The C ABI adds native_string_fresh/literal/persist/resolve_v1 entries. Fresh
  origins distinguish independent equal strings, including empty strings. Explicit
  literals resolve through the shared intern table. Persistence shares a complete
  known source origin or creates a fresh destination, never registering an unknown
  scratch or foreign source address. Different-length views get separate origins.
  The unsafe resolver requires live immutable native bytes and returns a borrowed
  identity bounded by native metadata ownership. Bind and explicit result leases
  remain supported; retirement/reset release resolved ownership at the final alias.
- Native str_persist now records lazy origins during ordinary copies and fresh
  concat-temporary takeover. The complete-register wrappers add fresh/literal/
  persist operations for both native architectures. Creation activates tracking
  before the first INI access; copy/free/reset retain the empty-map fast gate.
- EIR ConstStr emission registers its literal origin before later copies. Magician
  constant evaluation uses a dedicated string_literal runtime operation. Its C
  wrapper constructs the ordinary owned Mixed string first, then registers that
  native payload as a literal. Temporary Rust input addresses never enter the map.
  The eval-scope-only path omits this wrapper unless mbstring/eval needs it, keeping
  all mbstring symbols absent across the five supported targets.

Verification:

- Three engine native-metadata tests and two existing identity ABI tests passed.
  They exercise prebinding aliases, first resolution after the original is freed,
  exact-length checks, address replacement, independent equal scratch copies,
  short views, empty/one-byte fresh versus interned values, repeated binding,
  thread isolation, reset, and final global lease expiry.
- Two root library tests passed. The independent C/assembly harness executes real
  persistence, heap-kind and free helpers, concat takeover, and the new eval
  literal wrapper while calling the actual Rust origin registry. It checks
  register preservation, native address reuse, resolution after source release,
  literal identity across separately boxed values, and balanced native ownership.
  Structural checks cover both PIC modes and all five supported targets.
- The root build is warning-free. Full runtime plus user assembly assembled for
  macos-aarch64, ios-arm64, ios-sim-arm64, linux-aarch64, and linux-x86_64. The
  scope-only omission gate passed for that complete matrix. Native/eval string
  result GC tests and reused-worker web request reset passed.
- The sequential runner /tmp/elephc-mbstring-ini-origins-verify.py finished with
  ALL PASSED; logs are /tmp/elephc-mbstring-ini-origins-verify-*.log. Final expanded
  engine assertions passed again in /tmp/elephc-mbstring-ini-origins-final-engine.log.
  Assembly-comment checks and git diff --check passed. No own process remains.

Remaining prerequisites for public INI integration:

1. Complete the origin audit beyond ordinary persistence and ConstStr/eval
   literals. Direct heap constructors, scalar-to-string conversion, class-name
   constants, empty/short result normalization, and standard builtin identity
   returns need PHP-grounded classification. A heap address alone does not prove
   that a view is a complete logical string or that a transform returns its input.
2. Audit constant provenance through AST/EIR optimization. For example, the AST
   pipe folder can replace strtolower/trim with StringLiteral nodes. Do not claim
   INI pointer-sensitive equivalence until computed values and true literals are
   classified correctly for every optimization mode. Continue auditing mutation
   and result transformations before they can inherit an already tracked origin.
3. Use the native resolver for original PHP string inputs and implement the
   RESULT_INI_STRING/RESULT_INI_ARRAY materializers with balanced ownership on all
   protected callback/exception paths. Complete typed scalar-only INI coercion,
   CLI/web wrapper selection, startup/core defaults, and managed MIME provider
   routing. These adapters are not implemented by the origin registry itself.
4. The remaining 19 public functions, Oniguruma replacement, ninth constant,
   output/input handlers, and earlier full-scope gaps remain in the active goal.

A live read-only PR refresh still found #895, #898, #899, #900, and #902 open at
exactly the heads recorded in the preceding section. No commit, PR comment,
closure, external message, agent, or cargo fmt was used.

### ASCII case origins and shared literal bytes, 2026-09-08

Verified implementation progress. Public coverage remains 46/65 functions,
45 shared operations, and 8/9 constants. Public INI routing remains pending.

- Native strtolower/strtoupper share one target-aware emitter. It scans for an
  actual ASCII change, acquires independent native storage through str_persist,
  preserves the source origin for unchanged results, and assigns a fresh origin
  before modifying bytes. The helper does not rely on concat scratch capacity.
  The C/assembly fixture checks unchanged aliases, changed results, equal-byte
  round trips with distinct origins, fresh empty strings, source immutability,
  final lease expiry, and balanced native ownership. The obsolete SysV
  strtolower misalignment allowance was removed and its gate passes.
- Eval ASCII case conversion preserves unchanged string origins, returns raw
  byte strings after changes, and selects canonical empty results for ucfirst/
  lcfirst. Scalar and Stringable origin classification still needs completion.
- A new binary regression exposed eval lexing of escaped 0xFF as UTF-8 C3 BF.
  The existing AOT literal codec now lives in elephc-builtin-contract and is
  consumed by both lexers. Eval parsing decodes markers before forming constants:
  valid UTF-8 uses EvalConst::String, while other bytes use EvalConst::Bytes.
  Programmatically constructed Unicode names keep their existing representation.
  Both forms register interned identity only after native boxing. Byte-valued
  attribute arguments survive parsing and native metadata decoding; retained
  defaults use PHP Reflection's printable byte escaping.
- Pipe folding keeps case-changing results, nonempty changed trims, and all
  strrev results at runtime. Unchanged case transforms and unchanged/canonical
  empty trims can still fold. This does not complete constant provenance:
  concatenations after propagation, folded casts, and folded conditionals can
  still erase the distinction between computed strings and source literals.
  PHP evidence distinguishes literal concatenation from concatenation through a
  variable, conditional, or cast. That distinction needs an explicit solution
  before pointer-sensitive public INI behavior is enabled.
- Default trim masks now preserve form feed in AOT and eval. A regression checks
  trim/ltrim/rtrim using hexadecimal output and separately checks the
  AOT constant-folded pipe. Eval does not currently parse the pipe operator.
- The independent PHP INI reentry fixture grew from 1,256 to 1,376 traces with
  lower/upper/reverse inputs. The shared engine matches all 1,376 traces.

Verification:

- Three shared codec tests, six binary-literal regressions, 28 eval lexer tests,
  15 parser call tests, eight attribute parser tests, and 27 eval string tests
  passed. Two native origin tests, two pipe-fold tests, and the SysV alignment
  gate passed. The original 0xFF corruption assertion now passes end to end.
- Repeated lower/upper conversion of 60,000-byte strings has stable residual
  allocation counts in AOT and eval. The initial test also repeated echo and
  exposed an existing eval temporary leak: isolated echo of mb_strlen retains
  one allocation per call, and echo of a fresh literal retains two. Isolated
  lower, upper, assignment, and combined transforms all keep residuals constant
  at five allocations for 1 versus 12 repetitions. The GC regression prints only
  once to measure transform ownership; the general echo cleanup gap remains.
  Reproduction inputs and results are in /tmp/elephc-mbstring-ascii-gc-isolate.py
  and /tmp/elephc-mbstring-ascii-gc-isolate.log.
- The focused trim regression, 82 tests matching test_strto, both existing
  mbstring string-result GC tests, and reused-worker web reset passed. The root
  build is warning-free. Full runtime and user assembly assemble for all five
  supported targets, and the scope-only omission gate passes for that matrix.
- The builtin exporter, registry/page generation, module/comparison generation,
  docs audit, site compatibility, and enforced EIR boundary audit all passed.
  Generated string pages were inspected. Assembly-comment alignment and
  git diff --check passed. The final sequential runner ended with ALL PASSED in
  /tmp/elephc-mbstring-ini-case-verify.log, with per-step logs beside it. The
  expanded engine fixture passed in /tmp/elephc-mbstring-ini-case-reentry.log.

The origin/conversion/materialization prerequisites in the preceding section,
general eval temporary cleanup, and all 19 remaining public functions are still
open. A live read-only refresh found PRs #895, #898, #899, #900, and #902 open at
the same recorded heads. No commit, PR comment, closure, external message, agent,
or cargo fmt was used. No own build or test process remains running.

### Shared Oniguruma provider and managed package, 2026-09-08

Verified implementation progress. Public coverage remains 46/65 functions,
45 shared operations, and 8/9 constants. This checkpoint adds the shared native
regex foundation; public mbregex activation and the legacy mb_ereg_match cutover
remain pending.

- The curated native catalog now pins Oniguruma 6.9.10 revision 1 to the official
  release archive, SHA-256
  2a5cfc5ae259e4e97f86b68dfffc152cdaffe94e2060b770cb827238d769fc05,
  exact size 979159 bytes. Its provider archive precedes libonig.a in the
  declared link order. The recipe retains the public headers and supports all
  five catalog targets through the existing target toolchain. Production
  package resolution has no system-library fallback.
- A versioned, dependency-neutral provider contract keeps Oniguruma structures
  private to the C shim. It exposes compilation, independent match regions,
  copied capture bounds, duplicate-name selection, named backreference lookup,
  numbered-backreference policy, and bounded diagnostics. Registration checks
  completeness, initializes once, and rejects any changed callback table.
  Compiled owners remain thread-confined and pair each native allocation with
  its original provider's free callback.
- Rust owns PHP option normalization, all supported regex encoding aliases,
  alias-sensitive byte validation, explicit per-call limits, and copied capture
  metadata. Empty and unmatched captures stay distinct for the future PHP
  adapters. Zero limits remain distinct from untouched native defaults, and
  anchored matching preserves PHP's different unsigned-boundary rules.
- The PHP 8.5.10 / Oniguruma 6.9.10 oracle contains 8279 records: 3625 option
  cases, 426 alias cases, and 4228 anchored/search cases. The real provider
  matches 4098 native observations, including exact compile warnings, all
  syntax selectors, Unicode/binary input, and named/empty/unmatched captures.
  The other 130 observations are mb_ereg's empty-pattern ValueError cases;
  they remain recorded for the pending public argument adapter and are not
  claimed as implemented by the native transport.
- A PHP-grounded cache regression distinguishes SJIS from SJIS-WIN validation
  while reusing the same native encoding and compiled pattern. The compiled
  owner must never retain the previous PHP validation alias. Future cache hits
  must validate pattern bytes with the current alias. Subject validation belongs
  to each PHP operation; progressive search retains its initialized subject and
  does not repeat that validation after an alias change.
- The managed-native CI smoke matrix now installs Oniguruma, verifies locked
  offline reuse, and replays the oracle against the exact installed archives on
  macOS ARM64, Linux ARM64, and Linux x86_64. Its macOS job also builds the full
  device and Simulator packages. These workflow changes have been checked
  locally for YAML, shell, and embedded Python syntax; CI has not run on this
  uncommitted state.

Verification:

- The focused regex binary passes all three tests, including the explicitly
  selected native-provider test, both with pinned host development files and
  with the exact managed static archives. The native test also checks rejected
  provider replacements and 1000 repeated searches. Logs:
  /tmp/elephc-mbstring-regex-tests.log and
  /tmp/elephc-mbstring-regex-managed-tests.log.
- Nine native catalog tests and five recipe tests passed. Root and shared-engine
  builds are warning-free. The C provider compiles with -Wall -Wextra -Werror.
  ASan/UBSan probes pass 500 compile/search/error/free cycles against both the
  pinned host library and the managed archives, with leak detection enabled.
- The actual production recipe installed Oniguruma on Linux x86_64 and the
  locked offline install passed. Native doctor confirms that artifact installed
  when using the same TMPDIR/toolchain context. An unrelated pre-existing
  OpenSSL staging entry keeps the global doctor report unhealthy; it was left
  untouched. Install log: /tmp/elephc-mbstring-oniguruma-install.log.
- The C shim cross-compiles to objects for all five supported target triples.
  This is local C-object evidence, not execution or a full Oniguruma cross-build
  on every target. The new CI steps provide the remaining host execution and
  iOS SDK package-build checks once pushed.
- The builtin exporter, generated registry/pages and module/comparison docs,
  docs audit, site compatibility, and enforced EIR boundary audit all passed.
  The sequential docs runner ends with ALL PASSED in
  /tmp/elephc-mbstring-regex-docs.log. PHP/Python fixture syntax checks, new Rust
  module/function documentation checks, and git diff --check passed.

Next integration work:

1. Add shared request cache and progressive search state with PHP-equivalent
   replacement/invalidation behavior, options/encoding updates, request reset,
   and the live-alias validation rules above.
2. Implement replacement expansion, named/numbered backreferences, callback
   sequencing, and by-reference register arrays through protected, balanced
   native/eval materializers.
3. Add typed public contracts and shared operation IDs, the ninth constant,
   managed requirements/provider activation, static detection, and the explicit
   mbstring capability policy for opaque eval. Remove the legacy PCRE2
   mb_ereg_match path only after both public backends use the shared engine.
4. Complete focused public codegen/error/ownership and request-lifecycle tests.
   Public INI origin/coercion/materialization, general eval temporary cleanup,
   host input/output/mail integration, and all other full-scope gaps recorded
   above remain part of the active goal.

A live read-only refresh found #895, #898, #899, #900, and #902 still open at the
same recorded exact heads. No commit, PR comment, closure, external message,
agent, or cargo fmt was used. No own build or test process remains running.

### Shared mbregex request state and padded subjects, 2026-09-08

Verified implementation progress. Public coverage remains 46/65 functions,
45 shared operations, and 8/9 constants. The shared engine now owns the request
cache and progressive search state, but public AOT/eval adapters remain pending.

- A byte-keyed cache compares the native encoding identity, effective compiled
  option mask, and syntax. It validates every lookup through the current PHP
  alias, invalidates progressive pattern/captures when replacing their cache
  owner, and keeps operation results alive independently through reference-counted
  pattern and subject owners.
- Progressive state now retains subject, byte position, compiled pattern, and
  last captures. It implements initialization, all three search result shapes,
  get/set position, retained registers, empty versus unmatched groups, and PHP's
  forward progress rule for empty matches. Options/encoding changes and request
  reset preserve PHP's observed mutation order.
- Option errors in progressive search retain the last parsed syntax while
  discarding accumulated flags. Diagnostics and exceptions are emitted outside
  every state borrow, so warning handlers can reenter regex operations, replace
  state, or throw. Hosts suppress later warnings when an exception is pending.
- A PHP worker retains regex option defaults across requests but frees patterns,
  subjects, captures, and position. Request shutdown restores the configured
  regex default encoding and canonical validator. Ordinary mb_internal_encoding
  calls do not affect regex state; the INI handler updates the regex default.
- Native syntax defaults can add option bits. The provider ABI now exposes the
  actual compiled mask so POSIX/Java/Perl patterns are not reused under an
  incompatible requested profile. This changed the managed Oniguruma package
  recipe to revision 3 while retaining the same official 6.9.10 archive.
- ASan found an out-of-bounds decoder read when a valid multibyte subject was
  searched from an offset inside a character. Shared subjects now own 32 zeroed
  lookahead bytes for their entire lifetime, and repeated progressive searches
  reuse that allocation. Provider callers without this contract receive a
  bounded copy. A protected-page native test covers UTF-8, UTF-16BE/LE, and
  UTF-32BE/LE so a future read beyond the declared padding fails deterministically.

Verification:

- The expanded PHP 8.5.10 oracle contains 578 independent request traces and
  3707 top-level operations, including 18 warning-reentry scenarios. It covers
  cache replacement, settings, limits, malformed strings, all byte offsets in
  representative multibyte subjects, nested callbacks, exceptions, and retained
  captures. All traces pass against Oniguruma 6.9.10.
- Four separately configured two-request HTTP worker observations reproduce
  exact defaults and reset behavior, including the SJIS-WIN to canonical SJIS
  validation change. The fixture regenerates byte-for-byte.
- Instrumented Oniguruma and the provider pass a native stress probe over more
  than 100,000 arbitrary-byte searches. After the subject-padding fix, the full
  engine and request oracle also pass with ASan/UBSan and leak detection enabled.
- Managed recipe revision 3 installs and reuses offline. Catalog/recipe tests,
  warning-free root/shared builds, the exact managed-archive replay, and the
  complete builtin documentation/audit workflow passed in
  /tmp/elephc-mbstring-regex-request-verify.log. The provider compiles as a C
  object for all five supported target triples. Focused tests pass with the
  protected-page regression on the exact managed archives.
- CI now selects both regex test binaries on each managed native execution host.
  CI has not run on this uncommitted state. PHP/Python/workflow syntax checks,
  Rust preamble/function documentation checks, and git diff --check passed.

Next work is the typed public AOT/eval request boundary, result/register array
materialization, provider activation and native requirements, then replacement
expansion/callbacks. Public tests must cover all progressive functions before the
legacy PCRE2 mb_ereg_match path is removed. The remaining non-regex mbstring and
INI integration gaps recorded above remain active. PRs #895, #898, #899, #900,
and #902 remain open at their previously recorded exact heads. No commit, PR
comment, closure, external message, agent, or cargo fmt was used. No own build or
test process remains running.

### Public mbregex settings and live INI ordering, 2026-09-08

The preceding status-only goal turn made no implementation progress. This turn
revalidated the active goal and made verified changes to the public boundary.
`mb_regex_encoding` and `mb_regex_set_options` now have neutral contracts,
stable shared runtime IDs 67/68, AOT/eval bindings, typed EIR dispatch, and
generated documentation. The shared bridge implements 47 operations; together
with the legacy matching function the public surface covers 48/65 functions.
Full mbstring support remains incomplete.

- One thread-local regex Session is owned by the shared static library. AOT,
  opaque eval, dynamic names, first-class callables, named arguments, and
  call_user_func_array use the same setting state and generic coercion boundary.
- Null/omitted arguments remain getters. Encoding setters return true and retain
  the original alias validator; option setters return the previous canonical
  string. Failed changes retain the last valid setting, including binary errors
  and NUL-containing inputs. Stringable callbacks execute before the outer setter.
- Startup text-state prototypes retain the exact resolved INI encoding bytes,
  independently of public mb_internal_encoding mutations. IniRequest attaches
  the live Session to the existing shared INI handlers without introducing
  thread-confined native owners into the process-wide Clone/Sync prototype.
- The handler updates regex encoding after its ordinary warning callbacks and
  before later inherited input/output handlers. Reentry and pending exceptions
  preserve PHP's commit order. Reset restores startup INI defaults and clears
  regex request owners while retaining the worker's default option string.
- The protected-page native regression now runs after provider initialization.
  No native ABI revision or native package rebuild was required for these settings.

Verification on the final state:

- Warning-free root/shared build; stable runtime ID round-trip test.
- All 4,051 pinned PHP encoding/option observations pass through the exported
  settings ABI, with exact diagnostics and result release checks.
- Nine shared/native/INI tests pass against the installed managed Oniguruma
  revision-3 archives, including all previous request and protected-page tests.
- Thirty registry tests pass, including typed runtime joins, nullable callable
  signatures, and the new encoding union result.
- Three public codegen tests and the static error-contract test pass, with
  ELEPHC_PHP_CHECK=1. Separate direct PHP 8.5.10 executions confirm expected
  Stringable, binary-message, and INI callback observations.
- The target-lowering test passes for macos-aarch64, ios-arm64, ios-sim-arm64,
  linux-aarch64, and linux-x86_64. This is compile/emitter coverage; native
  execution on other architectures remains CI work.
- The complete builtin docs skill workflow, target-boundary audit, assembly
  comment check, and git diff --check pass. Generated public/internal pages
  were inspected. Example syntax passes. Logs are recorded in
  /tmp/elephc-mbstring-regex-settings-verify.log and its per-step companion logs.

A fresh GitHub read finds the same five related open PRs (#895, #898, #899,
#900, #902), all at their previously recorded exact heads. No external writes,
commits, PR comments, closures, subagents, or cargo fmt were used.

Next work remains the public matching/progressive-search boundary, diagnostic
callbacks and register arrays, managed provider activation, and replacement
expansion/callbacks. The existing PCRE2 mb_ereg_match has not yet been replaced.
The other remaining functions, ninth constant, public INI host integration,
strictness/ownership gaps, and HTTP/output/mail surfaces remain part of the
original full objective. All own build/test handles from this checkpoint are
terminal.

### Shared mb_ereg_match cutover and managed activation, 2026-09-09

The user asked for status while this goal remained active. The pending build
failed because an AOT requirement used the neutral contract's variant name;
that wiring error was corrected. The generated registry now reports 48 public
mbstring functions and 48 shared runtime bindings. The complete 65-function
objective remains unfinished, with 17 functions and other previously recorded
integration gaps still outstanding.

- mb_ereg_match uses stable RuntimeBuiltinId 69 and the generic shared AOT/eval
  argument adapter. Its PHP parameter name is string, correcting the old
  subject spelling. The old AOT helper and Magician PCRE2 adapter were removed.
- The shared library exports provider registration and availability. Registration
  checks the readable version/size header before reading the complete table.
  The managed Oniguruma recipe and its revision-3 native ABI are unchanged.
- RuntimeFeatures.mbregex has its own appended cache bit. Direct matches and
  reachable matching callables select the shared bridge and managed oniguruma
  package independently of PCRE2. Ordinary text operations and regex settings
  do not require the native package. --with-mbstring enables the provider for
  opaque eval and runtime-unknown callables.
- __rt_mbregex_init preserves all five invocation arguments on both target
  architectures. Eval setup registers the same provider before availability
  checks. Magician queries the bridge and owns no second matching engine.
- Protected matching dispatch reads INI limits before entering the Session and
  holds no text-state borrow during diagnostics. Pending callback exceptions
  outrank ordinary false results and later cleanup failures. Bare C ABI calls
  buffer exact warnings for hosts without protected callbacks.

Verified behavior and checks:

- Three new public codegen tests pass for native/opaque eval matching, named
  arguments, callable forms, Unicode, UTF-16LE, binary NUL, SJIS-WIN validation,
  default/null/empty options, Stringable side effects, dynamic argument errors,
  exact warnings, and explicit CLI capability activation. Matching activation
  leaves preg_match unavailable unless PCRE2 is independently enabled.
- All 2,114 pinned matching observations pass through the actual exported ABI,
  alongside the existing 4,051 setting observations and native/request corpus.
  The focused shared run passes ten tests using the managed Oniguruma prefix.
- Protected invocation tests cover nested matching and setting changes during a
  warning callback, pending exceptions, injected diagnostic/release failures,
  and balanced host ownership. set_error_handler/restore_error_handler are not
  available in the PHP compiler surface, so these tests exercise the actual C
  callback boundary. The old FakeOps mb_ereg_match test was migrated into real
  AOT/eval integration coverage.
- Typed feature selection, cache identity, complete SysV runtime call alignment,
  thirty AOT registry checks, eval provider availability, two existing matching
  regressions, three static error tests, and stable ID round-trip all pass.
- Both the CLI lowering fixture and runtime emitter checks cover all five
  supported targets. Local native execution remains Linux x86_64; these checks
  are not evidence of native execution on the other targets.
- The builtin exporter, generated registries/pages, module/comparison pages,
  docs audit, site compatibility, EIR target-boundary audit, assembly-comment
  checks, example syntax, and git diff --check pass. Generated matching pages
  were inspected and the obsolete documentation override for the removed
  __rt_mb_ereg_match helper was deleted. Fifteen focused Rust files passed
  module/function documentation checks. No full local suite was run.

The sequential verification runner is
/tmp/elephc-mbstring-regex-match-verify.py. Its master log and per-step logs use
the same prefix. The master log retains initial failures and resumed successful
runs; its final run ends with ALL PASSED. The three new public tests also passed
in the direct cargo invocation before that runner. All own build/test handles
are terminal at this checkpoint.

The next concrete prerequisite is archived CI fixture integration. The new
tests/codegen/support/oniguruma.rs uses production native add and the verified
resolver for local test archives and isolated CLI projects. Archived offline
nextest shards still need their Oniguruma source/artifact cache prepared and
packaged; the local helper alone does not complete that requirement. Avoid
fabricated receipts or a system-library fallback. Native fingerprints include
PATH and temporary-directory environment, so transferring an archive between
jobs needs deliberate handling of that identity. Existing managed-native jobs
already test the real Oniguruma recipe and supported target builds.

Then continue the public progressive-search/register-array and replacement
boundaries. Progressive errors can continue state changes and chain exceptions,
so the anchored operation's early-return adapter must not be copied blindly.
The remaining functions, ninth constant, public INI host integration,
strictness/ownership gaps, and HTTP/output/mail work remain in scope. Related
PRs retain their previously recorded ledger; no GitHub writes, commits, PR
comments, closures, subagents, or cargo fmt were used in this turn.

### Offline Oniguruma test archives and release probes, 2026-09-09

The preceding goal turn was progress: it replaced the legacy matching adapter
and verified the shared public boundary. This continuation revalidated HEAD
217ff6caad7e0965688c54b41d2410a8f5e92dec and completed the pending archive and
packaging integration. The public function count remains 48/65, with all 48
using shared runtime bindings. The full objective remains active.

- scripts/ci/prepare_oniguruma_tests.py builds a real managed project plus its
  source/artifact cache under target/debug/elephc-oniguruma. It invokes the
  production native add command and verifies locked offline reuse. An offline
  flag supports verification using an already verified local source archive.
- Each of the macOS ARM64, Linux x86_64, and Linux ARM64 archive jobs prepares
  that bundle before nextest archiving. The archive include is recursive and
  fails on a missing bundle. A focused timeout allowance covers a cold native
  C rebuild followed by native/eval program links.
- Archived oniguruma.rs fixtures use locked offline installation and the exact
  production resolver. Different PATH/TMPDIR fingerprints rebuild from cached
  verified source, preserving the normal identity/integrity policy. The new
  resolve_for_compilation_in_cache API selects an explicit cache without
  mutating process environment, so concurrent fixtures can remain isolated.
- The managed-native CI comparison now includes protected invocation and live
  INI tests alongside the engine/request corpus. Device and Simulator native
  package build checks remain in that existing matrix.
- --with-mbstring now requires Oniguruma, so the release-artifact verifier adds
  oniguruma before its mbstring link probe. Curl retains its own add-first
  behavior. Release/nightly native-cache keys include the Oniguruma recipe and
  C/header provider sources. Packaging tests require the mbstring native marker
  before accepting its link, and still reject a broken unrelated archive.

Verification:

- The codegen test binary built warning-free with the changed fixture/resolver.
- The bundle was built completely offline from the existing verified official
  source. Its production locked offline install passed.
- A relocated copy of the compiler, codegen test binary, required Rust archives
  (mbstring, Magician, BCMath, crypto, and tz), and bundle passed all three public
  matching tests. The child ran outside the checkout with prebuilt-bridge mode,
  CARGO_NET_OFFLINE=true, and a changed PATH. It produced a new compatible
  native artifact fingerprint while preserving the project lock exactly.
  The cold run took 59.12 seconds, supporting the focused timeout adjustment.
- In that owned relocated copy, deleting native artifacts and corrupting the
  source caused matching setup to fail with the offline integrity diagnostic.
  It did not use the host's system Oniguruma as a fallback. The original source
  and workspace cache were not corrupted.
- All three verify_release_artifact tests pass. Python syntax, shell syntax,
  YAML/TOML parsing, all three archive-job target mappings, release/nightly
  Oniguruma cache-key inputs, and git diff --check pass.
- cargo-nextest is not installed locally. Archive include fields were checked
  against https://nexte.st/docs/ci-features/archiving/ and configuration was
  validated directly; the relocation test executes the actual test binary and
  compiler, but is not a claimed local nextest archive/extract run. Remote CI
  has not run these unpushed changes.

Logs and the reproducible local runners are:
/tmp/elephc-mbstring-oniguruma-bundle-build.log,
/tmp/elephc-mbstring-verify-relocated-bundle.py,
/tmp/elephc-mbstring-relocated-bundle-verify.log,
/tmp/elephc-mbstring-relocated-bundle-public.log,
/tmp/elephc-mbstring-relocated-bundle-corruption.log, and
/tmp/elephc-mbstring-release-probe.log. The first relocation attempt omitted the
tz bridge required by eval; the successful second attempt included it. Owned
relocation copies were removed after verification, while logs and the valid
workspace bundle remain. All own process handles are terminal.

A fresh read at 2026-09-08 22:23 UTC still finds #895, #898, #899, #900, and #902
open at the exact heads already recorded in this ledger. No PR comments,
closures, commits, external writes, subagents, cargo fmt, or full local suites
were used. No public builtin metadata changed in this continuation.

The next implementation is the seven public progressive-search functions, not
a new engine. The existing 578 request traces already cover their pure Session
semantics. An audit found 36 captured calls with a nonempty previous-exception
chain, for example search(null, "iQ") without a pattern yields Error("No pattern
was provided") with the earlier ValueError("Option Q is not supported") retained
as previous (the actual fixture includes the exact quoted/binary bytes).
Current Outcome and protected anchored dispatch retain only one MbError, and
both native status emitters initialize the previous-throwable field to null.
The public progressive boundary therefore needs faithful exception-chain
transport/materialization, including callback/pending cleanup ordering, before
reusing the verified Session::initialize/search/registers/position methods.
Do not drop earlier errors or stop the Session at the first option error.
Register arrays also need exact numeric/name keys and false versus empty-string
preservation through the existing graph result path. The remaining functions,
ninth constant, public INI integration, ownership/strictness, HTTP/output/mail,
and final supported-target/PR closure audit remain in the original objective.

### Shared exception-chain result transport, 2026-09-09

The preceding status-only turn made no implementation progress. This continuation
revalidated HEAD 217ff6caad7e0965688c54b41d2410a8f5e92dec and implemented the
missing shared exception transport and native chain materialization. Public
coverage remains 48/65 functions, all 48 shared bindings, and 8/9 constants.
The seven progressive search functions are still not registered publicly.

- crates/elephc-builtin-contract/src/mbstring_abi/exception.rs defines oldest-first
  class/length/binary-message records, full-buffer validation, and a borrowed
  24-byte MbExceptionV1 view. RESULT_EXCEPTION_CHAIN is additive kind 15 with
  the record count in value. Existing single-error kinds remain unchanged.
- crates/elephc-mbstring/src/abi/exception.rs owns Outcome::error_chain and the
  exported elephc_mbstring_exception_at_v1 reader. It validates the entire
  chain before exposing the first record, returns 1/0/-1 for record/end/invalid,
  clears output metadata on failure, and never transfers the result's ownership.
  Current bare/protected matching adapters collect all engine errors instead
  of overwriting a single slot. Anchored semantics still yield the same results.
- src/codegen_support/runtime/strings/mbstring/exception.rs now centralizes
  native Throwable construction for ordinary errors and chains. ARM64 and
  x86_64 both copy exact binary messages, consume earlier errors into previous,
  and publish only the completed chain. The existing status/materialize
  adapters retain bridge-buffer release responsibility.
- The native fixture in tests/codegen/strings/mbstring_exception.rs checks
  exact Error/ValueError class identities, embedded NUL and invalid UTF-8
  messages, previous order, and an exact two-node chain. It releases the root
  under heap debugging and requires _gc_live to return to its prior value.
  Malformed trailing records must leave both pending ownership and live bytes
  unchanged. The test uses the existing main-exit injection/link harness.
- scripts/mbstring/README.md documents the shared wire ownership and validation
  boundary without claiming public progressive or destructor-ordering parity.

Verification completed warning-free:

- 2 neutral contract framing/layout tests and 2 bridge ownership/validation
  unit tests pass.
- The native exception-chain fixture passes on local Linux x86_64, including
  the strengthened exact live-byte cleanup checks.
- All 10 focused shared regex/provider/request/INI/protected-invocation tests
  pass, including the existing 2,114 match records and 578 progressive traces.
- The five-target mbstring emitter check, SysV call-site alignment gate, and
  five-target builtin lowering check pass. These are emitter/compile checks
  for nonlocal targets, not a claim of local ARM or iOS execution.
- All 6 public regex settings/matching tests, 9 shared coercion codegen tests,
  and the dynamic eval error/callable test pass.
- Assembly comment coverage/alignment and git diff --check pass.

The verification runner is /tmp/elephc-mbstring-exception-verify.py, with per-step
logs /tmp/elephc-mbstring-exception-{shared,emitter,alignment,targets,public-regex,
coercion,eval-errors,comments,diff}.log. Its final status was ALL PASSED. The last
native fixture rerun after adding live-byte assertions also passed. All own
Cargo/CLI handles are terminal. Builds used the required
TMPDIR=/home/nahime/.cache/elephc-mbstring-test-tmp. No full suites, cargo fmt,
commits, subagents, PR writes, or generated PHP documentation updates occurred.
No PHP-visible signature or builtin catalog metadata changed in this turn.

A direct PHP comparison changed the cleanup design decision. Do not add a V4
callback that merely publishes engine errors before the coordinator's final
cleanup. PHP can destroy a Stringable during conversion, before the operation's
own error. Such a callback alone preserves the wrong ordering. The tentative V4
changes were removed; the host remains V1/V2/V3, with its original coordinator
cleanup ordering. The shared chain transport does not resolve that existing gap.

Exact reproducer: /tmp/elephc-mbstring-exception-cleanup.php. ErrorOption's
__toString sets global $holder to null and returns "Q"; its __destruct throws
RuntimeException("cleanup"). The caller stores one object in $holder and calls
mb_regex_set_options($holder), then prints each Throwable and getPrevious.
Installed PHP 8.5.10 prints:

    ValueError:Option "Q" is not supported
    RuntimeException:cleanup
    after

The current native compiler instead prints:

    RuntimeException:cleanup
    ValueError:Option "Q" is not supported
    after

Both the original PHP source and compiled native reproducer remain in /tmp.
This confirms a conversion/temporary-owner lifetime gap, not a defect in the
new oldest-first transport. The native compiler also warns that the method's
global $holder assignment is unused. Audit argument-copy/identity-pin and
caller temporary release together before changing publication order; final
native cleanup may occur after the existing materializer has already published
the engine error. Capture state side effects as well as chains for that repair.

Next, connect the seven progressive operations to their already verified shared
Session methods and this chain transport, preserving exact register-array keys
and false versus empty strings through ArrayGraph. Retain the separate verified
Stringable/destructor ordering gap in the full completion audit. Remaining
functions, ninth constant, public INI/strictness/reference/callback work,
HTTP/output/mail semantics, supported-target execution/packaging verification,
and final PR superseding/closure work remain in the original objective. Related
PR heads were not refreshed in this turn; the prior verified ledger is retained.

### Public progressive regex cutover, 2026-09-09

HEAD remains 217ff6caad7e0965688c54b41d2410a8f5e92dec. The original complete
mbstring objective remains active. The generated registry now contains 55 of
PHP 8.5.10's 65 public mbstring functions, all with AOT bindings and shared-runtime
eval execution. This is catalog coverage, not a claim of complete PHP semantics.
Constants remain 8 of 9. No commits, PR writes, closures, or subagents occurred.

The seven public mb_ereg_search operations now use stable RuntimeBuiltinId values
70 through 76: init, search, pos, regs, getpos, getregs, and setpos respectively.
Their neutral contracts, AOT homes, eval homes, typed runtime targets, callable
routes, and managed Oniguruma requirements are connected. Every operation uses
the existing shared Session. Position and capture results cross ArrayGraph as
independent arrays, retaining ordered numeric/named keys and false versus empty
strings. Callback-free and protected invocation paths use the same dispatcher.
Regex warnings remain ordered and suppressed after a pending PHP exception.

The public protected-ABI replay test in tests/invoke/regex_request.rs replays all
578 PHP traces, including warning callback reentry and all multi-error chains.
The five native/eval integration tests cover PHP fixture output, invalid calls,
partial state changes, alternating backends over one session, capture ownership
and copy-on-write, and previous exceptions retained after releasing the root.
The static error test covers invalid signatures across all seven functions.
examples/mbstring/main.php demonstrates progressive product-code extraction.

Integration testing exposed missing getPrevious support in the native Throwable
method bridge used by eval. The new eval_method_helpers/throwable_previous.rs
emitter implements that method for ARM64 and x86_64 using __rt_throwable_previous
and owned Mixed boxing. Absent previous values return boxed null; object results
retain the borrowed previous Throwable. Case-insensitive dispatch and zero-arity
validation follow the existing Throwable methods. Heap-debug regression tests
pass in native code and opaque eval. The existing compact class list is unchanged.
The opaque-eval warning in pipeline/output.rs no longer incorrectly identifies
mb_ereg_match as requiring PCRE2. Forced capability tests now check availability
and actual invocation of all seven progressive functions with --with-mbstring.

Verification completed:

- 5 new codegen tests and 1 static error test pass. The final rerun log is
  /tmp/elephc-mbstring-progressive-public-check.log. The prior process handle was
  unavailable after context recovery, so this rerun establishes the result.
- All 40 neutral contract tests pass. Their stale counts were reconciled against
  HEAD's two mbstring contracts and the current 55: 1028 total contracts without
  curl, 572 eval registry entries, 672 AOT registry entries, 74 shared-runtime
  eval routes, and 496 interpreter adapters. Curl adds its existing 34 contracts.
- All 11 focused shared regex/provider/request/INI/protected-invocation tests,
  30 compiler registry checks, managed-provider lowering, five-target runtime
  emitter checks, and the SysV alignment gate pass.
- The updated five-target compilation/emission test passes, including opaque
  eval and getPrevious symbols. Nonlocal ARM and iOS checks are emission checks;
  local executable validation was Linux x86_64, with full host execution left to CI.
- All 3 anchored matching tests, 3 regex settings tests, and the expanded eval
  provider availability test pass.
- The complete update-builtin-docs workflow passes: exporter with curl, registry
  and page generation, module/comparison generation, docs audit, site validation,
  and the enforced EIR/target boundary audit. The exporter emits 1062 contracts
  with curl. All seven generated PHP signatures were compared to php_surface.json;
  their AOT support and shared eval routes agree. The compatibility page shows
  55/65 functions and 8/9 constants. Generated pages were inspected.
- Assembly comment coverage/alignment and git diff --check pass. No compiler
  warnings appeared in the focused verification logs. No full local suites or
  cargo fmt were run. Required TMPDIR was used throughout.

The sequential verification runner is /tmp/elephc-mbstring-progressive-verify.py;
its per-step logs use /tmp/elephc-mbstring-progressive-<step>.log and its final
status was ALL PASSED. All own Cargo/test handles are terminal. The README under
scripts/mbstring documents the public cutover and retains the separate pending
Stringable/destructor ordering gap from the previous checkpoint.

The ten missing public functions are mb_convert_variables, mb_ereg,
mb_ereg_replace, mb_ereg_replace_callback, mb_eregi, mb_eregi_replace,
mb_output_handler, mb_parse_str, mb_send_mail, and mb_split. MB_ONIGURUMA_VERSION
is the missing constant. Public INI/HTTP routing, provenance, coercion/strictness,
references and callback lifetimes, suppression, destructor ordering and temporary
cleanup, remaining host semantics, final supported-target execution/packaging
verification, and the PR superseding/closure work remain in the full objective.
The existing PR ledger was preserved and was not refreshed during this cutover.

### Shared mb_split, complete constants, and guarded results, 2026-09-09

This continuation made implementation and verification progress. HEAD remains
217ff6caad7e0965688c54b41d2410a8f5e92dec. The generated catalog now contains 56/65
public functions, all with shared AOT/eval implementation bindings, and 9/9
constants. These counts do not establish complete PHP semantics; the original
objective and its cross-cutting gaps remain active. No commits or PR writes occurred.

mb_split uses RuntimeBuiltinId 77 and a typed RuntimeFnId, with ordinary neutral
contract/AOT/eval registration and the managed Oniguruma requirement. Its shared
Session::split implementation lives in regex/request/split.rs. It validates the
subject before compilation even for limits 0/1, preserves PHP's unsigned negative
limit behavior, advances empty matches by bytes without discarding unmatched
text, retains delimiter-independent fields, and emits exact split-specific
warnings. Pattern cache replacement has the same progressive-state side effects
as other regex operations. ArrayGraph publishes independent result fields.

scripts/mbstring/capture_regex_split.py captures 1,871 independent PHP 8.5.10
traces using the existing request oracle. Both the shared Session test and the
public protected invocation replay pass every trace, including nested/throwing
warning handlers, limits, empty matches, multiple encodings, invalid data, and
cache invalidation. The existing 578 progressive traces remain unchanged.
Native/eval fixtures cover named, case-insensitive, namespaced and callable forms,
defaults, dynamic signature errors, Stringable order, deprecations, and COW.
BIG5 fields are compared as exact hex in the text-output fixture; an initial
include_str attempt failed because raw BIG5 output was not UTF-8, and was corrected.

The split corpus exposed a preexisting central encoding-name defect: BIG5, a
MIME charset spelling, was rejected. Encoding::lookup now follows PHP's three
ordered passes: canonical names, MIME names, aliases. The new PHP oracle
capture_encoding_lookup.php records 458 exact name resolutions, all passing in
the catalog test. This shared fix applies to text operations and regex validation.

MB_ONIGURUMA_VERSION is registered as a predefined PHP string. Its source is
mbstring_abi::regex::ONIGURUMA_VERSION, also consumed by the native catalog's
Oniguruma version and default selection. The immutable source URL, digest,
archives, and recipe revision 3 remain unchanged. All nine constant values match
the independent PHP reflection fixture. Static aliases, fully qualified names,
defined/constant lookup, and opaque eval pass without requiring a regex call.

Compiling the complete example exposed a checker regression in a natural search
loop: after while (is_array($code_match)), assigning the next array|false result
was rejected as an array-to-union retype. Checker::guarded_union_types now retains
the original union contract while guards narrow reads. Local assignment merge
can fall back to that original contract. If/while handling saves/restores these
facts, terminal guards keep the applicable fallthrough contract, and function
entry/exit and binding kill/retype isolate or retire the metadata. Properties
retain their existing separate narrowing rules. No EIR or assembly workaround was
needed. The original example loop remains intact.

Verification completed:

- 4 catalog tests, including all 458 name resolutions and all 9 constants, pass.
- All 13 focused regex/provider/request/INI/public-invocation tests pass, including
  the 1,871 split traces through both shared and public paths.
- 3 split codegen tests, 1 split static-error test, the version-constant test,
  and the guarded-union native/eval heap-debug regression pass.
- 34 existing narrowing codegen tests, 15 narrowing error tests (including the
  new declared-boundary/function-scope regression), and the existing while-guard
  lifetime test pass.
- 40 neutral-contract checks, 30 compiler registry checks, 9 native catalog
  checks, provider lowering, five-target runtime symbols, forced opaque-eval
  availability/invocation, and eval availability checks pass.
- Five-target compilation/emission passes after adding the guarded search loop
  and version constant. Local executable coverage is Linux x86_64; ARM/macOS/iOS
  execution remains CI work, not a claim made by these emission checks.
- The full examples/mbstring/main.php compiles with --strict-locals using the
  managed Oniguruma fixture and exactly matches PHP's 1,333 stdout bytes.
- The complete builtin-docs workflow passes: exporter, registries/pages,
  module/comparison generation, docs audit, site validation, enforced target/EIR
  boundary audit, assembly comments, and git diff --check. The exporter emits
  1063 functions and 1078 constants with curl enabled. The mb_split signature was
  compared to the PHP snapshot; its generated public/internal pages were inspected.

The runner is /tmp/elephc-mbstring-split-verify.py and logs use
/tmp/elephc-mbstring-split-<step>.log. The initial run stopped on the real example
checker error; after the fix and additional narrowing/target validation, resuming
at example completed with ALL PASSED. The focused guarded-union log and catalog,
public, constant, and invocation logs retain their independent results. All own
Cargo/test handles are terminal. Required TMPDIR was used throughout. No full
local suite, cargo fmt, subagent, or external message was used.

PR tracking was refreshed at 2026-09-08 23:50 UTC. All 18 open PR titles were
checked; the mbstring titles remain #895, #898, #899, #900, and #902. Individual
live reads confirm all five are OPEN with unchanged heads:
895 e96b43f219c085551a3c42c4cbbbd752846c0f88;
898 b15d629eb1d159ffb32fe272eb0f5c17472fb97b;
899 089ffc6bee2d2b3f17746da6fdf435f5be423440;
900 f402f762e093b4ad1c98bee1c83cb59bb4278c8c;
902 405f77283dd4e5c805eff74dc70f3fe013b56885.
The exact read-only snapshot is /tmp/elephc-mbstring-split-pr-ledger.json.

Remaining public functions: mb_convert_variables, mb_ereg, mb_ereg_replace,
mb_ereg_replace_callback, mb_eregi, mb_eregi_replace, mb_output_handler,
mb_parse_str, and mb_send_mail. No public constants are missing. Continue the
remaining regex operations and their reference/callback host contracts, then the
remaining variable/HTTP/output/mail surfaces and the existing INI, strictness,
provenance, suppression, destructor/temporary-cleanup, target/packaging, and
final PR superseding/closure audits. The earlier verified Stringable destructor
exception-ordering gap is unchanged and must not be mistaken for solved by these
guarded-union or split changes.

### Shared regex replacements and implicit arity exceptions, 2026-09-09

The current tree implements 58 of the 65 PHP mbstring functions and all nine
constants. The generated registry was compared directly with php_surface.json;
the new mb_ereg_replace and mb_eregi_replace contracts match PHP 8.5.10's four
parameter names/types/defaults and string|false|null return type. RuntimeBuiltinId
78 and 79 append to the stable ABI; ALL now contains 79 entries and MBSTRING 58.
Both backend home files bind the neutral contract to the same protected shared
operation, with typed EIR lowering, fresh ownership, generic timing, and the
managed Oniguruma requirement on all five supported targets.

regex/request/replace.rs owns the replacement loop and capture expansion. It
preserves default and explicit options, forced case-insensitivity, numbered and
named groups, duplicate-name selection, unmatched captures, leading-zero and
64-bit numeric-reference rules, raw replacement-character widths, and one-byte
empty-match advancement. Invalid subjects return null before option validation;
compile/search failures return false with the public function's exact warning.
The existing request cache retains its progressive-search invalidation semantics.

Protected invocation captures the replacement encoding before any argument
cloning/conversion callback. Subject validation and replacement scanning use that
snapshot, while compilation uses live settings after conversions. This capture
is limited to the two replacement operations, so ordinary text calls do not
initialize regex state. The host ABI remains V1/V2/V3; no callback table layout
or native provider ABI changed.

Independent PHP evidence and verification:

- capture_regex_replace.py records 1,431 independent PHP 8.5.10 / Oniguruma
  6.9.10 traces in regex_replace.jsonl.gz. Both the shared Session replay and
  protected public invocation replay pass every trace, including encoding and
  option variants, null/false results, cache changes, search limits, and warning
  callbacks that reenter or throw.
- capture_regex_replace_public.py captures native/eval fixtures with exact
  string/error bytes and portable diagnostic location suffixes. Binary results
  from empty matches are hex-encoded. Three end-to-end tests pass for public
  calls, dynamic errors, and heap-debug ownership. They include namespaced and
  named calls, first-class callables, call_user_func(_array), source evaluation
  order, Stringable encoding mutations, retained strings, and null returns.
- The dynamic-error fixture exposed missing implicit ArgumentCountError
  metadata: catch (Throwable) failed unless the concrete class was explicitly
  named. The checker gate, runtime class seeder, and feature-reachability class
  list now retain it, and declaration pruning no longer treats it as optional.
  The original broad catch remains in the passing regression. Fifteen gate
  tests, the existing synthetic-TypeError pruning regression, and the explicit
  error-hierarchy ancestor test also pass.
- Forty neutral-contract checks, thirty compiler registry checks, fifteen
  shared regex/provider/request/INI/invocation tests, the replacement static
  error test, managed-provider lowering, runtime symbols, forced opaque-eval
  availability/invocation, and the Magician availability test pass.
- Five-target compilation/emission passes with both replacement functions and
  an unknown-length argument spread caught by Throwable. Local executable
  evidence is Linux x86_64; foreign execution remains CI work. The iOS library
  boundary's existing restrictions on opaque invocations remain authoritative.
- examples/mbstring/main.php now formats supplier codes with named captures
  and normalizes a case-insensitive label. Its --strict-locals build exactly
  matches PHP's 1,396 stdout bytes.
- The complete builtin-docs workflow passes, including exporter, rendering,
  module/comparison generation, audits, site compatibility, and enforced
  target/EIR boundaries. Generated public/internal replacement pages were read
  and checked. The exporter emits 1,065 functions, 192 classes, and 1,078
  constants with curl enabled. Assembly-comment alignment and git diff --check
  pass. Public coverage is 58/65 with the intended seven omissions below.

The verification runner is /tmp/elephc-mbstring-replace-verify.py. Its final
resumed run completed with ALL PASSED; per-step logs use
/tmp/elephc-mbstring-replace-<step>.log. After limiting the entry snapshot to
replacement operations, all four public regex invocation tests and the
Stringable/native/eval public replacement test passed again (entry-abi and
entry-public logs). All own Cargo/test handles are terminal, including 91808
and 62764. Required TMPDIR was used throughout; no full local suite, cargo fmt,
subagents, commit, or external message was used.

PR tracking was refreshed at 2026-09-09 00:37:55 UTC. All 18 open titles were
checked. The related PRs remain #895, #898, #899, #900, and #902, all OPEN with
unchanged heads. The read-only snapshot is
/tmp/elephc-mbstring-replace-pr-ledger.json. No superseding comment or closure
has been published while the overall extension remains unfinished.

Remaining functions: mb_convert_variables, mb_ereg, mb_ereg_replace_callback,
mb_eregi, mb_output_handler, mb_parse_str, and mb_send_mail. Continue reference
and callback host integration, variable/HTTP/output/mail functionality, and the
existing INI/public routing, strictness, provenance, suppression, temporary and
destructor cleanup, foreign-target execution/packaging, and final PR audits.
set_error_handler/restore_error_handler are still absent from the compiler PHP
surface; the warning-reentry corpus verifies the real protected C host boundary,
not a claim that those PHP functions were added. The previously verified
Stringable destructor exception-ordering gap remains unresolved.

### Shared capture engine and output-reference evidence, 2026-09-09

Public coverage remains 58/65, with all nine constants implemented. Neither
mb_ereg nor mb_eregi has been registered as a public builtin: the shared engine
and PHP evidence are complete for this increment, but the native/eval output
reference adapters remain outstanding. HEAD is still
217ff6caad7e0965688c54b41d2410a8f5e92dec. No commit or external write was made.

regex/request/capture.rs adds Session::capture. It rejects an empty pattern
before output initialization, then reads live encoding, options, and search
limits after the initialization callback. That callback can stop matching on a
typed-reference failure or permit it to continue with a destructor exception
pending. Matching uses the existing request cache and padded subject storage.
Native search failures and mismatches return false without a search warning;
successful captures use registers(false), including false for empty groups.

Independent PHP 8.5.10 / Oniguruma 6.9.10 evidence:

- capture_regex_capture.py generates 718 traces in regex_capture.jsonl.gz for
  optional output arguments, output seeds, groups, binary and multibyte inputs,
  encoding and option changes, limits, progressive cache interactions, and
  warning callbacks observing or changing the output.
- capture_regex_output.py and capture_regex_output.php generate 1,200 traces
  in regex_output.jsonl.gz for local, mixed-property, union-property, object-only,
  and integer-only reference storage. The old output object has a single owner,
  so its destructor really runs at initialization. Traces include mutation,
  by-value copies, regex/INI reentry, pending exceptions, and unchanged values
  after rejected typed assignments.
- regex_request.rs replays the ordinary corpus. Its child module
  regex_capture_output.rs explicitly models the host's reference initialization
  and construction-array identity around the real shared engine. Passing this
  model is not evidence that the native/eval reference adapters exist.

PHP source and the recorded traces establish different initialization order:
zend_try_array_init_size uses zval_ptr_safe_dtor for an untyped reference, so
the old object's destructor observes null and the fresh array is published
afterwards. A typed reference first validates array assignability, then publishes
the fresh array before destroying the old value. A destructor of an assignable
typed property's old value can add entries and copy the exposed array by value;
both the output and that copy see later capture insertions. A pending destructor
exception does not by itself prevent the body from compiling, searching, filling
captures, or changing the shared regex cache. Incompatible typed assignments stop
before the old value is released.

Adapter work must therefore preserve caller lvalue identity without copying the
old output value, distinguish successful initialization from pending-exception
status, and retain the active construction array across capture insertion. The
current invocation host still supports only V1/V2/V3 and clones every argument
by value. Existing generic reference-argument writeback after a call is
insufficient for destructor-visible output changes. Native hash storage embeds
entries inline, and __rt_hash_grow both separates shared storage and returns a
new allocation. A construction writer cannot simply use ordinary COW insertion
or replace the output with a new ArrayGraph without losing PHP's observed aliases.
No speculative callback ABI or native array-layout change has been introduced.

At 2026-09-09 01:08 UTC, the focused regex command passed all 17 tests across
regex, regex_request, regex_ini_abi, and invoke, including both new corpora and
existing public split/replacement/progressive invocation checks. The log is
/tmp/elephc-mbstring-capture-shared.log; process 39676 is terminal with exit 0.
Python syntax checks for both new generators and PHP syntax checks for the two
capture helpers pass. scripts/mbstring/README.md now documents the two corpora,
their modeled-host limitation, and corrected pending-function names. Required
TMPDIR and the managed Oniguruma fixture were used. No full local suite, native
adapter test, foreign-target execution, cargo fmt, subagent, or PR update was
performed in this increment. The prior Stringable destructor exception-ordering
gap and all seven remaining public functions remain open.

### Protected capture invocation and reference-output ABI, 2026-09-09

This increment implements the shared host boundary for mb_ereg and mb_eregi.
Public AOT/eval availability is still 58/65, with all nine constants. The neutral
catalog now includes both capture signatures, exactly matching the PHP reflection
snapshot: required string pattern/string, optional untyped-by-reference matches
with a null default, and bool return. Both contracts explicitly carry
ReferenceAdaptersPending for both backends. Neither has a RuntimeBuiltinId,
builtin! home, or eval_builtin! home yet. ALL remains 79 and MBSTRING remains 58;
neutral catalog counts become 1,033 without curl and 1,067 with curl. This is
incomplete integration, not completion of either public function.

The new MbInvokeHostV4 preserves the 96-byte V3 prefix and appends three callbacks
at offsets 96, 104, and 112, for a 120-byte table. MbCaptureOutputV1 is 16 bytes:
ready followed by an opaque owned writer. Initialization records the writer before
inspecting callback status. Ready=1 permits matching despite PendingThrowable;
ready=0 stops after a rejected typed assignment. The fill callback receives an
ordered ArrayGraph and must preserve the active construction array, its unrelated
entries, and aliases exposed during initialization. Writer release is protected
and happens before argument pins retire. Unknown statuses or malformed readiness
fail closed while preserving an actual pending throwable's precedence.

elephc_mbstring_capture_v1 accepts ignore_case=0/1, the original argument pointer
array, actual arity, source strictness, the ordinary versioned host pointer, and
MbResultV1 output storage. Supplied output references require V4; omitted outputs
also work with V1/V2/V3. This entry is the integration boundary, not a public PHP
binding. It uses the same prepare_argument, prepare_contract, arity formatter,
panic boundary, and ownership arena as ordinary mbstring invocation. The first
two arguments are copied before coercion; the third is only pinned and is never
described, coerced, snapshotted, or copied by value. Output initialization follows
coercions and empty-pattern validation. Live encoding/options/limits follow
initialization. Pending destructor exceptions survive subsequent matching and
capture filling. Native/eval emitters still supply V3 until their reference
adapters are implemented.

Independent tests and verification:

- The protected capture entry replays all 718 ordinary capture traces and all
  1,200 output-reference/destructor traces through actual C host callbacks. The
  host models PHP lvalues and checks that only the two value parameters are
  cloned. These tests complement the existing Session replays; they do not
  replace native/eval adapter tests that still need to be written.
- Fault injection exercises initialization, filling, writer release, pending
  exceptions, invalid status codes, malformed readiness, rejection of supplied
  outputs with an older host, and poisoned pointers with invalid arity. Separate
  checks cover coercion errors before output mutation, Stringable parameter order,
  and V1 calls without output. A library test verifies writer cleanup after a
  contained Rust panic.
- All 41 neutral-contract checks, 3 focused invocation panic tests, 22 protected
  invocation tests, 4 coercion ABI tests, 30 AOT registry tests, and 12 eval
  registry tests pass. The existing replacement public-call, runtime-error, and
  heap-ownership tests pass natively and through opaque eval on Linux x86_64.
  No foreign executable coverage is claimed by this increment.
- The exporter, generated pages/registries, module sections, comparison page,
  docs audit, site validation, and enforced EIR/target boundary audit pass. The
  docs generator now distinguishes absence in both backends from an eval-only
  builtin, finds the real neutral source file for unbound contracts, and avoids
  nonexistent lowering links. Both new public/internal capture pages were read.
  The generated registry proves 60 mbstring contracts, 58 supported in each
  backend, and eval_only=false for the two incomplete capture contracts.
- All 14 documentation-generator tests pass. After extending the pending-contract
  regression to exercise the docs audit and deliberately inconsistent flags,
  its 10-test contract-pipeline file passed again. git diff --check passes.

The resumable runner is /tmp/elephc-mbstring-capture-abi-verify.py; logs use
/tmp/elephc-mbstring-capture-abi-<step>.log. The final audit/site/boundary/diff
resume completed with ALL PASSED. All own processes are terminal: 62127, 33389,
90561, 62153, 56372, 12870, and 30930. Required TMPDIR and the managed Oniguruma
fixture were used. No full local suite, cargo fmt, subagent, commit, PR comment,
or PR closure was performed.

The PR ledger was refreshed read-only at 2026-09-09 05:41:45 UTC. All 18 open
titles were checked. Related PRs remain #895, #898, #899, #900, and #902, all OPEN
with unchanged heads. The exact snapshot is
/tmp/elephc-mbstring-capture-abi-pr-ledger.json.

Continue native/eval reference acquisition and construction-array writers, then
add typed RuntimeBuiltinId/RuntimeFnId bindings, public homes, caller/lvalue and
coercion tests, all-target validation, examples, and regenerated docs before
removing ReferenceAdaptersPending. The native inline-hash growth and construction
alias constraint recorded above remains unresolved; a detached temporary or
ordinary COW insertion is not a correct adapter. The earlier Stringable cleanup
exception-ordering gap also remains open. All seven previously listed public
functions and the broader request/INI/strictness/HTTP/mail/packaging/final PR work
remain part of the original goal.

### Stable native hash identity for construction writers, 2026-09-09

This increment removes the native inline-hash relocation obstacle. Public mbstring
support remains 58/65 with all nine constants. The capture contracts still carry
ReferenceAdaptersPending; no public capture binding or native/eval output adapter
has been added in this increment.

All native associative arrays now use a stable 48-byte header. The first five
words keep count, capacity, value type, head, and tail. Offset 40 owns a pointer
to a separate raw heap allocation of 64-byte entries. Both allocations use the
ordinary allocator and its accounting. The entry allocation has raw kind 0, so
GC finds PHP child edges through the hash header rather than treating the entry
buffer as another container. Deep release destroys keys/values, then entry
storage, then the header. Empty hashes also own a raw entry allocation; the heap
allocator applies its normal minimum allocation size. Negative capacities are
normalized to zero in metadata as well as sizing.

The new arrays/hash_layout.rs owns entry-address generation on both architectures.
Eighteen readers were updated, including lookup, insertion, iteration, unset,
append scans, sorting, list/pointer/key helpers, deep release, cycle scanning,
reachability, and EIR hash append lowering. Runtime's narrow module boundary
reexports hash_layout to codegen. No representation flag or optional old-layout
path was introduced, so an existing ordinary array assigned during a destructor
already has the stable representation needed by a future construction writer.

__rt_hash_grow first performs ordinary copy-on-write separation and delegates to
__rt_hash_grow_owned. The owned entry skips separation, rehashes into temporary
replacement storage, transfers all six header words back into the original
header, frees old entries and the temporary replacement header, and returns the
original identity. Original refcount/kind metadata is preserved. This internal
entry must only be used where alias-visible construction is intentional. The
ordinary PHP mutation routes still use their existing COW gates. Entry addresses
remain borrowed and must be reacquired after a mutation.

The independent native_growth.c fixture executes real emitted constructors,
owned/ordinary growth, insertion, lookup, and iteration with independent C heap
accounting. It covers zero capacity, repeated growth through 257 insertions,
shared header identity, refcounts/kind preservation, lookup, insertion order,
and balanced raw allocations. Its C uniqueness stub checks entry routing only;
the PHP COW tests validate real separation. The independent collector graph
fixture now supplies a separate entry pointer too.

Two additional issues were resolved while validating the change:

- Mixed boxing's load-local/string fallback could consume a reference despite an
  explicit EIR Release for that same SSA value. Stable entry storage exposed this
  after krsort on a promoted local. value_can_own_mixed_box_source now rejects
  consumption whenever EIR still releases the value, sharing that check with
  value_can_transfer_ownership_to_consumer. A new krsort heap-debug regression
  verifies the returned local, an aliased copy, mutation, iteration, and clean
  teardown. All 23 focused krsort tests pass.
- The SysV audit previously could not analyze and rsp, -16 in the existing INI
  identity hooks. It now models the exact relative alignment adjustment from an
  entry rsp congruent to 8 modulo 16. Three obsolete exclusions were removed.
  A new regression proves that alignment is accepted and a later deliberately
  misaligned call is still detected. All six alignment audit tests pass.

Verification completed:

- Native owned-growth and native collector graph tests each pass.
- All 742 matching array tests pass; one unrelated ignored test stays ignored.
- The GC group had 329 passes and one obsolete allocation-count assertion. Hash
  COW now creates a header and an entry buffer, so its documented expected delta
  changed from one to two. That focused test then passed. The unchanged 329 tests
  were not rerun after the assertion-only edit.
- All eight runtime emitter tests and three regex-replacement native/eval tests
  pass. The final curl-enabled compiler build is warning-free.
- The complete RuntimeFeatures::all runtime was emitted from the built library
  and assembled with clang for linux-x86_64, linux-aarch64, macos-aarch64,
  ios-arm64, and ios-sim-arm64. All five objects assembled successfully. Execution
  in this increment is Linux x86_64 only; no foreign execution is claimed.
- Assembly-comment checks, touched-file module preambles, and git diff --check
  pass. Memory-model, architecture, and runtime helper docs describe the new
  layout. No full local suite, cargo fmt, subagent, commit, or PR write was used.

The resumable runner is /tmp/mbstring-stable-hash-verify.py with per-step logs
/tmp/mbstring-stable-hash-<step>.log. Its final gc-count/mbstring/build resume
finished with ALL PASSED. Target verification is
/tmp/mbstring-stable-hash-targets.py, with five objects and logs under
/tmp/mbstring-stable-hash-targets/. Pre-increment snapshots for the initial hash
edits remain in /tmp/mbstring-stable-hash-before/. All own process handles are
terminal, including 66829, 15558, 39257, 11085, 94528, 18666, 83380, 55541, and
53946. Required TMPDIR and the managed Oniguruma fixture were used.

The read-only PR ledger was refreshed at 2026-09-09 08:05:49 UTC. All 18 open
titles were checked. Related PRs #895, #898, #899, #900, and #902 remain OPEN with
unchanged heads; /tmp/mbstring-stable-hash-pr-ledger.json holds the exact snapshot.

Continue implementing native/eval reference acquisition and the V4 capture
initialization/fill/release callbacks. Stable headers now permit alias-preserving
growth, but hash_insert_owned alone is not a capture writer: replacement must
still handle old value destruction, protected pending exceptions, fresh argument
references, typed assignment timing, and reentrant mutation. Ordinary COW must
not be used for internal capture population. The earlier Stringable destructor
exception-chain ordering gap remains unverified and open. All seven public
functions and the original full-extension goal remain incomplete.

### Live capture references and hash lifetime pins, 2026-09-09

Public support remains 58/65 with all nine constants. The capture contracts still
carry ReferenceAdaptersPending. This increment implements and tests shared ordered
capture application plus the native lifetime primitive needed by construction
writers; it does not register public capture functions or complete their native
and eval adapters.

The shared elephc_mbstring_capture_apply_v1 entry validates the whole flat capture
graph before any callback, borrows exact integer/string keys and string/false
values through MbCaptureStoreV1, and calls the host once per entry. Status two
preserves a pending throwable and continues later writes; other failures stop
mutation, preserving an earlier pending throwable. Rust panics are contained.
Writer and native values remain caller-owned, and request/regex state is not
borrowed across callbacks. The V4 host layout remains 120 bytes.

The independent invocation host now retains a reference token in its writer,
rereads the current output for every entry, and routes filling through the shared
capture-apply entry. It no longer freezes the initialization array in its writer.
The PHP 8.5.10 oracle scripts/mbstring/capture_regex_retarget.php produces four
checked-in traces in tests/fixtures/regex_retarget.json: mb_ereg and mb_eregi,
each with ordinary and throwing old-entry destruction. PHP completes the current
write in the array selected before the destructor, then follows the reassigned
reference for later numeric and named captures. A saved copy sees only the first
capture. These are coordinator/model-host comparisons, not native PHP coverage.

Stable native hash headers now occupy 56 bytes. The six existing layout words
remain unchanged; offset 48 counts internal lifetime pins. __rt_hash_pin adds
one physical reference and one pin. __rt_hash_unpin removes the pin before
tail-calling normal hash release, which may run destructors. COW compares physical
references minus pins, so retaining storage through a callback does not introduce
an extra PHP value owner. The collector continues to see each pin as an external
root. Constructors and COW clones start with zero pins; owned growth copies only
the six layout words and preserves the original pins, heap kind, and refcount.
Pins require a nonnull managed hash and paired release under the caller's cleanup
boundary. The native capture writer does not consume these helpers yet.

The owned-growth x86_64 entry also now reads its declared rdi input rather than
depending on an incidental rax value. Its native fixture explicitly clears rax
before invoking that entry through the C ABI. The fixture now executes real COW
and shallow-clone emitters as well as pin/unpin, growth, insertion, lookup, and
iteration. Its independent allocator/release callbacks verify physical owner
counts, pins through repeated growth, clones with zero pins, alias independence,
last-pin lifetime, and zero leaked allocations. It uses integer payloads and
aborts on unexpected string/child callbacks. The separate real collector fixture
proves a pin retains an entire cyclic graph until its physical root is removed.

Verification completed with the required TMPDIR and managed Oniguruma prefix:

- Both native growth/COW/pin and collector graph tests pass.
- Five focused capture invocation tests pass, including 718 ordinary traces,
  1,200 output-reference traces, four new retargeting traces, protocol failures,
  coercion, and legacy hosts. All five capture-apply tests pass.
- All 21 COW/cycle, nine allocation-guard, and 18 key-sort codegen tests pass.
- All six SysV alignment and eight runtime emitter tests pass.
- The curl-enabled compiler build is warning-free. Complete runtime assembly
  emitted from that build assembles for linux-x86_64, linux-aarch64,
  macos-aarch64, ios-arm64, and ios-sim-arm64. Native execution is Linux x86_64;
  no foreign execution is claimed.
- The complete update-builtin-docs workflow passes. All 2,087 previously generated
  documentation file contents remain identical. Architecture, memory-model, and
  runtime helper documentation describe the new pin field and ownership rules.
- Assembly-comment alignment, all 14 touched Rust module preambles, and
  git diff --check pass. No full suite, cargo fmt, subagent, commit, or PR write.

Logs use /tmp/mbstring-pins-<step>.log and
/tmp/mbstring-pins-docs-<step>.log. Five-target assembly artifacts are under
/tmp/mbstring-pins-targets/. All own process handles are terminal: 63565, 42406,
27109, 48222, and 14384. The PR ledger was refreshed read-only at
2026-09-09 08:47:53 UTC: all 18 open titles were checked; #895, #898, #899,
#900, and #902 remain OPEN with unchanged heads. The exact snapshot is
/tmp/mbstring-pins-pr-ledger.json.

Next work remains the actual V4 native/eval reference initialization, ordered
store, and release adapters. Preserve the reference identity during initialization
without retaining the fresh array as an ordinary PHP owner. For each capture,
select the current destination, pin its storage without changing COW, contain
old-value destructor exceptions, complete that selected write, and release the
pin before moving to the next reference lookup. Reassignment to existing indexed
arrays still needs representation/promotion handling; stable hash headers alone
do not resolve it. Typed property constraints, untyped versus typed initialization
timing, reentrant mutation, and cleanup remain necessary before public bindings.
The Stringable destructor exception-chain ordering gap remains open. All seven
public functions and the original full-extension goal remain incomplete.

### Native selected-hash capture stores, 2026-09-09

This increment adds the native component that consumes the previously introduced
hash lifetime pins. Public support remains 58/65 with all nine constants. Capture
contracts still carry ReferenceAdaptersPending; there is still no general V4
native/eval output-reference adapter or public mb_ereg/mb_eregi registration.

strings/mbstring/capture_hash.rs emits __rt_mbstring_capture_hash_store for both
architectures. Its four C inputs are unused context, an already selected managed
hash, and borrowed validated key/value descriptors. The outer reference adapter
must select the live destination anew for each call. The helper retains an
internal hash pin, persists captured string bytes, looks up the selected key,
protects overwritten heap/callable release through __rt_cleanup_call, and looks
the key up again after callbacks may have grown the entry allocation. It writes
existing entries without COW, or grows with hash_grow_owned, persists a new string
key, and transfers an owned insertion. Protected unpinning happens after the
completed write, even if losing the last PHP owner makes this unpin destroy the
selected array. Pending releases return status two after the store completes.
There is no request-state or Rust borrow inside the native helper.

The helper has an explicit unfinished precondition: its selected key must not be
replaced or removed while the old value is being destroyed. Other-key growth and
replacement of the caller's output reference are supported. Do not wire this
primitive as a general public capture store until same-key reentrant ownership is
handled. Leaving the old value observable during destruction and recognizing
ownership of a replacement written by a callback require further tracking;
clearing the slot to null first or blindly releasing the old owner twice would
be incorrect. Indexed destinations, reference initialization, and typed assignment
timing remain additional adapter work. This limitation is documented in the
native helper and runtime wiki, not treated as completion of public semantics.

Two complementary regressions now run:

- capture_hash/native_store.c executes actual emitted capture stores, pins, hash
  growth, lookup, insertion, COW, and string equality with an independent C
  allocator/release host. It checks 257 shared-array insertions, binary keys and
  values, duplicate-key replacement, empty string versus false, ordered entries,
  and zero live allocations after cleanup. Destructor actions replace the current
  output, grow the old hash through 200 nested stores, return pending status,
  and optionally drop its last PHP alias. Subsequent stores follow the new
  destination, while the selected old write completes before final unpinning.
  The C cleanup host models pending status; it does not exercise PHP longjmp.
- tests/codegen/runtime_gc/mbstring_capture_hash.rs supplies that missing native
  exception coverage. It compiles real PHP object destructors and closure captures,
  replaces only a marked typed test function body with a small ABI shim, calls the
  actual store, and propagates returned status two through __rt_throw_current.
  Ordinary and throwing object/callable-capture destruction run at repetition
  counts one and eight. The PHP catch observes the expected exception after the
  captured string has been stored, and both executions report a clean debug heap.
  This is internal-helper integration coverage, not public mb_ereg/eval coverage.

The independent native fixture and real PHP boundary regression pass, including
their final empty-string and callable-capture additions. All six SysV alignment,
eight runtime emitter, and eight related deep-cleanup tests pass. The compiler
build is warning-free. Complete runtime objects assemble for linux-x86_64,
linux-aarch64, macos-aarch64, ios-arm64, and ios-sim-arm64. Execution remains Linux
x86_64 only. The enforced EIR/target-architecture audit has zero structural errors.
Assembly-comment alignment, five touched Rust module preambles, and git diff
--check pass. Runtime documentation also now reflects the actual 96-byte V3 host
and 58 shared operations, alongside the pending V4/native capture integration.

Logs use /tmp/mbstring-capture-store-<step>.log; complete runtime assembly and
objects are under /tmp/mbstring-capture-store-targets/. All own handles are
terminal: 86402, 55960, 70677, 14178, 24055, 83731, 88005, and 3852. Required
TMPDIR was used. No full local suite, cargo fmt, subagent, commit, or PR write.
The read-only PR ledger refreshed at 2026-09-09 09:22:13 UTC still finds 18 open
PRs and the unchanged related #895, #898, #899, #900, and #902. Its exact snapshot
is /tmp/mbstring-capture-store-pr-ledger.json.

Continue same-key reentrant ownership and indexed destinations, then actual
reference acquisition/initialization/fill/release for both AOT and eval. The
Stringable exception-chain ordering gap and all seven unfinished public functions,
request/INI/HTTP/mail integration, complete target checks, examples, generated
documentation, and eventual superseding PR comment remain part of the original
full-extension goal.

### Guarded reentrant capture writes, 2026-09-09

HEAD remains 217ff6caad7e0965688c54b41d2410a8f5e92dec. This is progress on the
complete mbstring goal, not completion. Public support remains 58/65 functions
and all nine constants. No public capture binding or runtime operation ID was
added in this step.

The pending ownership changes are now verified for ordinary native hash
set/unset and nested capture construction. arrays/hash_write_guard.rs emits
push/pop/claim helpers. Its caller-owned five-word record contains next,
selected stable hash, key low/high, and changed; _hash_write_guard_top links
active release scopes. Hash pins and capture descriptor bytes retain identity.
Claim compares normalized binary key bytes and hash identity, marks every
matching record, and returns zero if any matching record still owns the old
release. Subsequent claims own the replacement value. Pop unlinks an exact
record, even below a newer scope, and reports whether the entry changed.

capture_hash.rs uses a 128-byte ARM64 frame or 112-byte x86_64 local frame.
The record occupies offsets 56..95, old payload 96, release callback 104, and
pending status 48. Protected old-value release links the record, executes
__rt_cleanup_call, and unlinks it. A changed entry repeats lookup and releases
the callback-installed owner before the final capture write. An unchanged
entry is looked up again without consuming the old owner twice. The old entry
remains observable during destruction. hash_set/hash_unset claim ownership
before release and skip it when the outer capture already owns it. The hash
header remains 56 bytes.

The native C fixture now emits real set/unset and guard helpers, including
target heap magic on its independent string/object owners. Direct guard tests
cover binary equality, key kind/length and hash identity isolation, nested
scopes, first/repeated claims, and non-LIFO/already-unlinked pop. Twenty-eight
reentry cases cover string/false replacement, removal, remove-then-set, two
string writes, nested capture, and four-object destructor chains, with numeric
and binary string keys and ordinary/pending cleanup. Final captures, insertion
order, exactly-once destruction, cleared guard state, and zero allocations are
checked. Existing alias, growth, and retarget cases pass. C cleanup still models
pending status rather than PHP exception unwinding.

The codegen regression adds 48 reentrant operations from compiled PHP
destructors with real exceptions and a clean debug heap. Test-only assembly
shims borrow the selected hash and invoke actual hash_set/hash_unset or nested
capture construction without extra PHP reference conversion/COW owners. The
mutation helper takes its scalar selector by reference to prevent inlining;
the test verifies that its call remains before replacing the marked body.
An initial by-value identity helper was inlined and that ineffective fixture
was corrected. These shims do not claim public AOT/eval mb_ereg coverage.

Validation passed: the native fixture, both PHP boundary regressions, all six
SysV alignment tests, eight runtime-emitter tests, eight mbstring deep-cleanup
regressions, and a warning-free cargo build -p elephc. Complete
RuntimeFeatures::all assembly succeeds for linux-x86_64, linux-aarch64,
macos-aarch64, ios-arm64, and ios-sim-arm64; execution remains Linux x86_64 only.
The enforced EIR/target-architecture audit has zero structural errors.
Assembly comments, nine touched Rust module preambles, and git diff --check pass.

Logs use /tmp/mbstring-hash-guard-<step>.log; complete runtime assembly and objects
are in /tmp/mbstring-hash-guard-targets/. Handles 57887, 86797, 82916, 55223,
84741, and 52890 are terminal. Required TMPDIR was used. No full local suite,
cargo fmt, subagent, commit, or PR mutation. The read-only ledger
/tmp/mbstring-hash-guard-pr-ledger.json refreshed at 2026-09-09 10:12:36 UTC:
18 open PRs, with #895, #898, #899, #900, and #902 unchanged and still open.

Remaining: hash_to_mixed conversion can transfer an active borrowed payload
into a new box and needs ownership-aware handling. Copying a Mixed box whose
deep release is active, direct slot writers, and library/fiber teardown that
bypasses protected cleanup also need review before public capture registration.
Ordinary destructor exceptions follow the tested pop path; arbitrary nonlocal
teardown is not covered. Indexed destinations and live reference
initialization/fill/release for AOT and eval remain pending. Prototype direct
global/static-property reference tests encountered existing lowering
restrictions before execution, so accepted tests use documented internal shims.
All seven unfinished public functions, the Stringable exception-chain ordering
gap, request/INI/HTTP/mail integration, complete target execution/packaging,
examples, generated docs, and eventual superseding PR comment remain in scope.

### Retained destructor receivers and refreshed cycle roots, 2026-09-09

HEAD remains 217ff6caad7e0965688c54b41d2410a8f5e92dec. The previous guard turn
was progress; this turn adds a verified runtime prerequisite for capture
representation changes. The full goal is still active, with 58/65 public
functions and nine constants. hash_to_mixed and public capture adapters have
not been changed in this turn.

Inspection found that the existing destructor dispatcher set refcount bit 31
to prevent recursive destruction, but object_free_deep always reclaimed the
receiver even when a callback retained it. A conversion of a borrowed capture
entry cannot acquire a safe new object owner under that behavior.

objects/call_destructor.rs now emits a small public dispatcher that records
object kind-word bit 14 (0x4000) before invoking its existing native/eval body,
now named __rt_call_object_destructor_body. The public dispatcher skips an
already-called destructor. It introduces no additional exception frame:
both callers, object_free_deep and the cycle-destructor phase, already invoke
it through their protected cleanup scope. Refcount bit 31 remains temporary;
persistent destructor-called state is independent of the actual owner count.
Object allocation/clone paths stamp fresh kind 4, so clones do not inherit the
called bit.

After protected destruction, object_free_deep masks off bit 31 when testing
remaining owners. A nonzero count is restored and returns through CLEANUP.finish
without releasing properties or eval identity metadata. A zero-owner receiver
keeps an in-progress bit while its properties and allocation are dismantled.
That bit protects recursive release and distinguishes pending allocated storage
from free-list blocks. Pending exceptions propagate after either path.

The collector must also preserve objects retained by cyclic destructors. Its
destructor phase now skips called receivers and records whether it invoked any
original callback. If callbacks ran, a rescan clears reachability/incoming-edge
metadata while preserving the original candidate bit, then recomputes graph
edges and external roots before reclaiming nodes. New/reused callback allocations
remain excluded from the original candidate set. Graphs with no callbacks keep
one root scan. State values are zero initially, two after invoking a callback,
and one after starting the rescan. ARM64 uses sp+96 in its existing 112-byte frame;
x86_64 uses rbp-72 in an 80-byte local frame (formerly 64). Object refcounts with
only bit 31 set are not roots, but remain visible until the free phase. Rescan
restores ordinary counts for retained objects. The low sixteen kind bits preserve
the new persistent called flag.

A direct deep-free ownership caller was found in fopen's failed user-wrapper
path. It now consumes its owner through __rt_decref_object on both architectures,
and x86_64 supplies the actual private RAX input. This prevents a normal count-one
failed wrapper from being mistaken for a receiver retained by its destructor.

The independent native collector fixture now covers callback retention,
survival through another collection, later reclamation without a second
destructor, and a destructor breaking its own cycle down to zero real owners.
Existing graph-readability, pending exceptions, new/reused allocation exclusion,
and hash pin cases still pass.

tests/codegen/runtime_gc/destructor_resurrection.rs checks ordinary/throwing
retention and cyclic retention through AOT and opaque eval, four repetitions of
ordinary and throwing variants. Saved receiver properties remain readable;
destructors run once; final AOT live allocation counts and bytes match unused
static-declaration baselines. AOT keeps one declared nullable-static null box,
so claiming a completely empty heap would be wrong. The cycle fixture uses an
uninitialized nonnullable self property and a typed release_saved helper reached
after instanceof narrowing. The initial nullable-self-property prototype retained
its graph and did not invoke the destructor; its storage behavior is still an
open, separate issue. Direct nullable/mixed property unset shapes also have
existing lowering restrictions and are not claimed by this fixture.

Eval traces pass, but generic eval scalar temporary growth is still open.
This is not an eval heap-clean claim. Debugger inspection of the ordinary
scenario found only string/Mixed allocations remaining, no object allocations.
One, two, and four repetitions leave 36/67/129 blocks and 2087/3327/5807 bytes.
The unused-declaration baseline leaves five blocks/847 bytes. Unchanged
interpreter code such as eval_inc_dec_value allocates a boxed one operand without
releasing it; broader temporary ownership needs its own follow-up. Keep the
scope of the passing eval trace evidence explicit, not a full ownership proof.
The retained diagnostics are /tmp/mbstring-resurrection-eval-{1,2,4}.php and
/tmp/mbstring-resurrection-eval-heap.log. AOT heap snapshots are
/tmp/mbstring-resurrection-{ordinary,cycle}-heap.log; older prototype cycle
snapshots predate the final nonnullable self-property fixture.

Validation after the final collector/fopen edits:
- Native collector fixture: passed.
- Entire focused runtime-GC group: 334 passed, no failures/ignored, 207.33 seconds.
  This broader focused group was justified by changing shared object release/GC.
- User-wrapper fopen group: 22 passed.
- SysV alignment: six passed. Aggregate runtime emitters: eight passed.
- Warning-free cargo build -p elephc.
- Complete RuntimeFeatures::all assembly for linux-x86_64, linux-aarch64,
  macos-aarch64, ios-arm64, and ios-sim-arm64. Executed tests remain Linux x86_64.
- Enforced EIR/target-architecture audit: zero structural errors.
- Assembly comments, eight touched Rust module preambles, git diff --check.
- PHP reference runs agree on eight destructor invocations and four exceptions
  per scenario. Those reference runs explicitly collect after unset to model
  elephc's documented eager unset safe point.

Logs use /tmp/mbstring-resurrection-<step>.log. Runtime assembly and objects are
under /tmp/mbstring-resurrection-targets/. All own handles are terminal: 4169,
77237, 21163, 19750, 14035, 19062, 96156, 84990, 40127, 95007, 15248, and 91857.
Cargo/native compiler runs used the required TMPDIR. No full project suite,
cargo fmt, subagents, commit, or PR mutation. The read-only PR ledger
/tmp/mbstring-resurrection-pr-ledger.json refreshed at 2026-09-09 11:03:27 UTC:
18 open PRs; #895, #898, #899, #900, and #902 remain unchanged and open.

Next concrete capture work: hash_to_mixed must claim guard ownership before
transferring a raw entry into a Mixed box. For a borrowed active release,
mixed_from_value can acquire a new child owner, now supported for objects by the
retained-receiver change; ordinary owned entries should keep the existing
transfer-only boxing path. Guard mutation must cause capture construction to
relookup/release the new box before its final write. Exercise conversion alone,
then set/unset/nested capture, with binary keys and pending destructor exceptions.
Do not treat active preexisting Mixed boxes or nested containers already partway
through deep release as solved: cloning those values, indexed destinations,
direct slot writers, and nonlocal library/fiber teardown remain open. Complete
live-reference init/fill/release in AOT/eval, all seven public functions,
Stringable exception chaining, request/INI/HTTP/mail integration, target
execution/packaging, examples, generated docs, and eventual PR superseding comment
remain part of the unchanged goal.

### Ownership-aware hash conversion during protected captures, 2026-09-09

Implemented and verified the next capture-construction ownership step. HEAD is
still 217ff6caad7e0965688c54b41d2410a8f5e92dec. Public support remains 58/65
functions and all nine constants; this is internal progress, not public capture
registration or goal completion.

hash_to_mixed now claims raw-entry ownership after COW. Ordinary payloads still
transfer into their new box without an extra retain. A payload borrowed by an
active protected release instead acquires a new owner through mixed_from_value.
The retained-object destructor semantics from the preceding checkpoint make that
new object owner valid when its destructor returns.

Existing Mixed entries require a nonmutating inspection first. The new private
C-ABI __rt_hash_write_guard_owns shares the exact binary-key/hash scan with claim,
but does not mark any matching record changed. Already owned boxes remain
unchanged, including on repeated conversion. Borrowed boxes claim the rewrite
and use mixed_clone to acquire a detached PHP value, unwrapping nested reference
cells without retaining the old zero-owner Mixed allocation. The suspended
release still owns those old boxes. Capture construction later releases the
replacement box before its final write.

The five-word caller guard record and public store ABI are unchanged. The
shared guard scan uses its previously unused local slot at sp+40 / rsp+40 for
inspect-versus-claim mode. hash_to_mixed preserves Mixed-entry keys at ARM
sp+48/sp+56 and x86 rbp-56/rbp-64. Existing aligned frame sizes remain unchanged:
ARM conversion 96 bytes, x86 conversion 64 bytes; ARM guard scan 64 bytes and
x86 guard scan 48 bytes after saving rbp.

The independent C fixture now emits real mixed_from_value, mixed_clone,
mixed_unbox, mixed_reference, and hash_to_mixed helpers, with independent heap
and release observations. Its object destructor host models temporary refcount
bit31, persistent called bit14, and surviving new owners. Mixed boxes release
their owned children. The obsolete reference-copy abort adapter was removed;
the unrelated resource-id adapter intentionally still aborts on unexpected use.

The fixture checks 180 reentrant combinations: 15 action modes, numeric/binary
keys, raw/boxed/nested-reference object payloads, and ordinary/pending throws.
Conversion runs twice to verify stable box identity and owner counts. Actions
include set/false/unset/unset-set/two-sets/nested capture/replacement chains.
Every case ends with no live allocations, pending status, or linked guards.
An ordinary-conversion control checks transfer-only string ownership and stable
boxes. Read-only guard tests prove inspections preserve all nested changed flags.

The real PHP destructor test now runs seven modes with raw and preboxed input,
eight iterations of ordinary and throwing callbacks: 224 reentrant operations.
It covers set/unset/nested capture, conversion alone, and conversion followed by
each mutation. Exact stdout and clean native heaps pass. Existing direct-object
and closure-capture destructor cases also pass. Input boxing and mutations still
use test-only native shims; these are not public mb_ereg calls or eval capture
adapter tests.

Regression evidence was obtained before each production correction:
- Raw conversion failed to acquire the expected object owner.
- Existing Mixed conversion reused the old zero-owner box instead of creating
  an owned replacement.
Both now pass. Logs are /tmp/mbstring-conversion-before.log and
/tmp/mbstring-conversion-boxed-before.log.

Final validation after all Rust edits, handle 93297 terminal:
- Native capture fixture: one test passed.
- Real PHP capture/destructor group: two tests passed.
- Associative arrays: 44 passed.
- Foreach-reference GC regressions: three passed.
- By-reference array parameters: five passed.
- SysV alignment: six passed. Aggregate runtime emitters: eight passed.
- Warning-free cargo build -p elephc.
- Enforced EIR/target-architecture audit: no structural errors.
- RuntimeFeatures::all assembles for linux-x86_64, linux-aarch64,
  macos-aarch64, ios-arm64, and ios-sim-arm64. Execution remains Linux x86_64.
- Assembly comments, four touched Rust preambles, and git diff --check pass.

Logs use /tmp/mbstring-conversion-final-<step>.log. Target assembly/objects are
under /tmp/mbstring-conversion-targets/. All own handles are terminal: 44453,
21450, 83636, 99314, 87464, and 93297. An initial invocation mistakenly used
--exact with only a short test name and ran zero tests; the filter was corrected
before collecting either regression or passing evidence. Cargo and native
compiler runs used the required TMPDIR. No full project suite, cargo fmt,
subagents, commit, or PR mutation.

The read-only PR ledger /tmp/mbstring-conversion-pr-ledger.json was refreshed
at 2026-09-09 11:25:10 UTC: 18 open PRs; #895, #898, #899, #900, and #902 remain
unchanged and open.

Next concrete prerequisite: COW/shallow-copy of a hash during an active release
still routes an old Mixed entry through reference_array_copy, which can retain
its zero-owner box. Conversion's post-COW guard inspection cannot repair a bad
copy that already happened, and the new hash has a distinct guard identity.
Exercise a destructor acquiring a separate PHP hash owner, converting that
owner through COW, preserving the old selected capture hash, and then either
releasing or retaining the independent snapshot. Keep the old hash guard
unchanged while the copied value acquires its own valid owner.

Copying nested arrays/hashes already partway through child destruction remains
open, as do resource/callable teardown, direct slot writers, indexed capture
destinations, and nonlocal/fiber/library teardown. The earlier generic eval
temporary growth, nullable-self-property prototype, and Stringable exception
ordering issues are unchanged. Finish live-reference init/fill/release in both
backends, all seven remaining public functions, request/INI/HTTP/mail integration,
target execution/packaging, examples, generated docs, and the eventual
superseding PR comment before completing the unchanged goal.

### COW snapshots of capture values during destruction, 2026-09-09

Completed and verified the next shared copy prerequisite. HEAD remains
217ff6caad7e0965688c54b41d2410a8f5e92dec. Public support is still 58/65 functions
and all nine constants. The preceding goal turn was verified implementation
progress; the full objective remains active.

__rt_reference_array_copy now detects a zero-owner Mixed cell that is still
borrowed while protected child release runs. It tail-calls mixed_clone to copy
the PHP value instead of incrementing the dying cell's count. Ordinary owned
boxes and shared reference identity keep the existing behavior; orphan
references still detach. Both architectures implement the same rule. No new
runtime ABI or frame is required. hash_clone_shallow and array_clone_shallow
already consume the returned owner, so their code did not need modification.

The native capture fixture now exercises 288 reentry combinations: 24 modes,
two key shapes, three boxing depths, and ordinary/pending exceptions. Added
modes acquire a separate PHP hash owner and force COW through hash_to_mixed,
then either release the snapshot inside the destructor or retain it across
capture completion. Copies are also combined with conversion/set/unset/nested
capture on the original hash. The snapshot has its own hash identity and zero
pins, the original guard remains unchanged by the copy, and each retained
receiver has one owner after the original capture finishes. Reading its payload
and final release do not repeat its destructor. Every case ends with zero
live allocations and no pending status or linked guard.

Before the fix, the new test failed in fixture_retain while the source Mixed
box was still observable with zero owners. The corrected fixture passes.
The failing evidence is /tmp/mbstring-snapshot-before.log; the passing native
test is /tmp/mbstring-snapshot-native-final.log.

The real PHP test adds immediate-release and retained-snapshot modes to the
existing raw/preboxed matrix, now 288 reentrant operations. A typed PHP helper
receives the retained object after capture completion and reads its mode
property, then unsets it. Exact stdout and clean native heaps pass for ordinary
and throwing destructors. Native snapshot creation and extraction are explicit
test shims in tests/codegen/runtime_gc/mbstring_capture_hash/snapshot.rs;
the parent uses target-aware symbol storage for both selected and snapshot
hashes. These tests do not claim public mb_ereg/eval capture adapter coverage.

Validation:
- Native capture fixture: one passed.
- PHP capture/destructor group: two passed.
- COW/cycle GC group: 21 passed.
- Associative arrays: 44 passed.
- Existing mbstring ownership/eval reference group: 17 passed (121.83 seconds).
- SysV alignment: six passed. Aggregate runtime emitters: eight passed.
- Warning-free cargo build -p elephc.
- EIR/target architecture audit: no structural errors.
- Assembly comments, three touched Rust module preambles, and git diff --check.

The first cross-assembly pass caught a Mach-O constraint: ARM cbz cannot target
the global __rt_mixed_clone symbol. The corrected conditional targets the local
__rt_reference_array_copy_clone label, which tail-branches to the shared helper.
After this ARM-only adjustment, native capture and emitter tests were repeated,
the compiler rebuilt warning-free, and all five complete runtimes assembled:
linux-x86_64, linux-aarch64, macos-aarch64, ios-arm64, and ios-sim-arm64.
Execution evidence remains Linux x86_64; the other targets have assembly
verification here. Artifacts are /tmp/mbstring-snapshot-targets/.

All own processes are terminal: 30115 (expected negative test), 58414 (native
and PHP tests), 97857 (regressions/build and the first assembler failure), 96567
(final tests/build and successful five-target assembly). Logs use
/tmp/mbstring-snapshot-<step>.log. Cargo/native commands used the required
TMPDIR. No full project suite, cargo fmt, subagents, commit, or PR mutation.

Read-only PR state refreshed at 2026-09-09 11:41:52 UTC in
/tmp/mbstring-snapshot-pr-ledger.json: 18 open PRs; #895, #898, #899, #900,
and #902 are unchanged and open.

Next implementation focus is the native/eval capture writer's initialize,
fill, and release callbacks around the existing V4 capture coordinator.
Initialization must preserve typed versus untyped reference ordering:
php-8.5.10 Zend/zend_API.h:1481 (zend_try_array_init_size) delegates typed
references to typed assignment, while an untyped slot uses safe destruction
before publishing the fresh array. The existing 1,200-trace
regex_capture_output.rs fixture models typed publication before the old
destructor and null visibility during untyped destruction. This is not an
ordinary value writeback. Native invoke.rs still publishes a V3/96-byte host
table with ten callbacks; V4 needs the three capture callbacks and the separate
capture coordinator entry. AOT descriptors currently stage copied raw SSA
storage, so a caller-output operand needs a real captured lvalue identity,
not that detached argument buffer. Eval ref_targets.rs is ordinary writeback
and must not be assumed to provide capture initialization semantics.

Copying nested arrays/hashes already partway through child destruction,
resource/callable teardown, direct slot writers, indexed destinations, and
nonlocal/fiber/library teardown remain unfinished. The generic eval temporary
growth, nullable-self-property prototype, and Stringable exception ordering
issues are unchanged. Complete both reference adapters, all seven outstanding
public functions, request/INI/HTTP/mail integration, target execution/packaging,
examples, generated docs, and the eventual superseding PR comment before
claiming the unchanged full goal is achieved.

### Live reference filling and protected destructor locals, 2026-09-09

Verified another implementation step. HEAD remains
217ff6caad7e0965688c54b41d2410a8f5e92dec. Public support remains 58/65 functions
and all nine constants. The complete goal remains active, with no external block.

Added the native associative capture-reference adapter in
src/codegen_support/runtime/strings/mbstring/capture_reference.rs.
__rt_mbstring_capture_reference_store validates a persistent tag-7/high-1
reference, resolves its current PHP value for each capture, and delegates the
selected hash write to the existing protected hash store. The caller retains
the reference; each selected hash is pinned only for that write. Malformed
references and non-hash destinations return the fatal host status.
__rt_mbstring_capture_reference_fill supplies this callback to the actual Rust
elephc_mbstring_capture_apply_v1 graph validator and ordered insertion engine.
Both native architectures implement the same C ABI and status contract.

The independent C fixture now tests reference retargeting with retained/dropped
aliases, old-hash growth, pending throws, and invalid reference markers. It also
retains the earlier 288 reentrant COW/conversion combinations. Its standalone
link emits only the per-entry adapter, while the real PHP fixture links the
Rust graph engine. Three PHP fill tests cover binary keys and values, complete
validation before any mutation, pending exceptions, and a destructor replacing
the live reference with another PHP-created hash. The current write completes
on the original hash; later entries reach the replacement. Exact output and
clean heaps pass. Reference creation/retargeting in these tests uses explicit
native shims, not a public mb_ereg call or caller-lvalue initialization.

Those tests exposed two destructor-frame cleanup gaps. Native destructors did
not publish exception cleanup activations outside library mode, so throwing
bodies leaked their locals. Registering the activation alone still abandoned
later locals when a child destructor threw. Native destructor frames now use
a dedicated walker in src/codegen/frame/destructor_cleanup.rs on normal and
exceptional exits. Each owner is cleared and released through __rt_cleanup_call;
later owners run after pending throws. Raw fallback reference cells free their
storage after protected payload release. Normal completion propagates pending
throws only after removing the destructor activation. Other user functions
retain the existing activation policy. The module is 153 lines and reuses
the existing target-aware slot, ownership, and exception helpers.

New regressions verify case-insensitive destructor names, owned array locals,
several throwing child destructors, complete previous-exception chains, and
explicitly aliased local reference cells. Before activation, the simple local
fixture leaked 16 blocks/5248 bytes over four throwing calls. Before protected
per-owner cleanup, /tmp/mbstring-destructor-nested-cleanup.php skipped the final
child when the parent body had already thrown. Both paths now match PHP and
the final regression fixtures have clean heaps.

Reading the exception chains exposed a separate extra retain in native
Throwable::getPrevious: the nullable result retained its object before Mixed
boxing retained it again. Boxing now acquires that owner once; unboxed results
still acquire their required owner. The nested and aliased-reference tests
verify complete cleanup even after observing the previous chain.

An existing eval/native round-trip regression then exposed inconsistent
previous-slot publication. Eval's builtin Throwable constructor wrote a raw
object pointer even for the ordinary object layout whose previous slot is
boxed. It now transfers the retained previous owner through the shared
__rt_throwable_append_previous writer on both architectures. The existing
test_eval_exception_previous_round_trips_through_native_bridge passes all
three executions in its test helper after this fix.

Validation before the final eval-constructor adjustment:
- Runtime-GC group: 340 passed (165.51 seconds).
- Ordinary destructor group: 10 passed. Exception group: 61 passed.
After the final adjustment:
- Eval/native previous round-trip: one passed.
- PHP capture group: five passed. Destructor ownership group: five passed.
- Eval constructor ownership: two passed. Deep cleanup group: eight passed.
- Independent native store: one passed. Shared Rust capture-apply ABI: five passed.
- SysV alignment: six passed. Runtime emitter tests: eight passed.
- Warning-free cargo build -p elephc; EIR/target audit reports zero structural errors.
- Assembly comments, all eleven touched Rust module preambles, and git diff --check pass.

All five complete runtimes assemble. The actual multiple-throw destructor,
aliased-reference, and opaque eval/previous PHP fixtures also compile and
assemble on every target, as do the exact reference-fill/retarget/result shims
extracted from the PHP test harness. iOS device and Simulator use --emit staticlib;
the first harness attempt requested a standalone iOS executable and was
corrected after the CLI rejected that unsupported output mode. Execution
evidence remains Linux x86_64; other targets have assembly verification here.
Artifacts: /tmp/mbstring-reference-targets/. Reproduction script:
/tmp/mbstring-reference-targets.py. Logs: /tmp/mbstring-reference-final-*.log,
/tmp/mbstring-destructor-*.log, and /tmp/mbstring-reference-eval-previous-fixed.log.

All own processes are terminal, including 90921, 53579, 24358, 79866, 34459,
77711, 36731, 26385, 4633, 24890, 28489, and 43967. Earlier failures were
resolved or narrowed explicitly as described here. Cargo and native compiler
runs used the required TMPDIR. No full project suite, cargo fmt, subagents,
commit, PR mutation, or generated builtin metadata change.

Read-only PR state was refreshed at 2026-09-09 12:36:54 UTC in
/tmp/mbstring-reference-pr-ledger.json: eighteen open PRs; #895, #898, #899,
#900, and #902 retain the exact heads recorded in the preceding checkpoint.

One separate prototype remains unresolved: a local captured by reference in a
closure can be promoted without a tracked fallback-cell owner. Its raw local
cleanup is skipped, so the captured object is not destroyed. Reproduction:
/tmp/mbstring-destructor-captured_reference.php, with native/PHP traces beside
it. The repository regression source instead exercises explicit local
aliasing, which creates a tracked fallback owner and passes. This is not
closure-capture ownership coverage. Activations/cleanup for callees reached
from a destructor or another PHP callback also remain separate work.

The public native invoke table still uses V3. V4 initialization and writer
release routing, typed/untyped publication order, actual caller-lvalue lowering,
indexed destinations, and the eval capture writer remain unfinished. The
earlier nested-container partial destruction, resource/callable/nonlocal/fiber
teardown, generic eval temporary growth, nullable-self-property prototype,
and Stringable exception ordering issues remain open. Complete both reference
adapters, all seven outstanding public functions, request/INI/HTTP/mail work,
target execution/packaging, examples, generated docs, and the eventual
superseding PR comment before marking the complete mbstring goal achieved.

## Native capture reference initialization, 2026-09-09

HEAD remains 217ff6caad7e0965688c54b41d2410a8f5e92dec. Public support remains
58/65 functions and all nine constants. This checkpoint completes an internal
initialization primitive and its tests; it does not register mb_ereg/mb_eregi,
install a V4 host, resolve PHP caller lvalues, or implement request shutdown.

The neutral mbstring ABI now defines typed/untyped initialization modes and
MbCaptureReferenceInitV1. Its layout is 24 bytes: the existing sixteen-byte
ready/writer output followed by a separately owned discarded-value pointer.
It is explicitly different from the V4 callback's sixteen-byte output buffer.

The new capture_reference_begin runtime emitter accepts C4(context, borrowed
persistent reference, mode, output). It validates the reference's tag 7/high 1
shape and the mode, clears output fields before validation, and returns the
shared success/invalid/pending status. It retains only reference identity,
publishes that writer owner, allocates and boxes a fresh heterogeneous hash,
and releases the previous boxed value through __rt_cleanup_call.

Typed initialization publishes the fresh hash before the old destructor and
preserves subsequent destructor reassignment. Untyped initialization exposes
null to the destructor, then publishes the fresh hash. A destructor-installed
value overwritten by this final publication transfers through discarded;
writer release must not prematurely destroy it. The host still needs to adopt
that separate owner for PHP request lifetime and appropriate shutdown ordering.

The PHP oracle /tmp/mbstring-capture-init-oracle.php demonstrates why this is
necessary: the overwritten replacement object's destructor runs after the
function, explicit unset, gc_collect_cycles, and the final script output.
The relevant local PHP sources remain Zend/zend_API.h's array_init and
Zend/zend_variables.c's safe destruction in /tmp/php-8510-src.

Independent C coverage lives beside the new emitter. Real hash allocation and
Mixed boxing run against checked C ownership accounting. Thirty-two scripted
publication cases cover typed/untyped state, replacement, modeled pending
exceptions, recursive initialization, and retained construction-array snapshots.
Four alias cases separately retain the old box or its object payload. Invalid
reference shapes, modes, missing output, and missing reference are also checked.
Every case finishes with no live fixture allocations. Modeled C pending flags
are not evidence for the PHP exception unwinder.

The new PHP integration fixture reference_begin.rs supplies that separate
evidence: a real PHP factory transfers a fresh object into one persistent
reference, real destructors inspect and retarget it, and native initialization
contains and later propagates their exceptions. Sixty-four executions verify
typed/untyped publication, replacement lifetime, writer retirement, and an
explicit later deferred-owner release, with a clean heap. The fixture uses local
by-reference integer arguments for its test controls. Earlier harness failures
were corrected by preserving observable helper calls and avoiding the different
global-reference argument representation. They did not require production
runtime changes. The shared retarget shim is now accessible to both fixtures,
and missing-function diagnostics include the expected helper name.

Final focused validation: contract layout 1 passed; independent native fixtures
2 passed; PHP capture group 6 passed; shared capture-apply ABI 5 passed; runtime
emitter tests 8 passed. The initial shared-test command selected the wrong test
binary and ran zero tests; the corrected integration binary ran all five and
replaced that log. The reproduction script contains the corrected command.

The update-builtin-docs workflow, including the curl-enabled exporter build,
all generators, docs/site audits, and enforced EIR target-boundary audit passes.
All 2087 tracked documentation artifacts are byte-identical to the before
manifest. The normal cargo build -p elephc is warning-free. Assembly comment
checks, all seven touched Rust module preambles, new function docblocks, and
git diff --check pass. No cargo fmt or full project suite was run.

All five complete runtimes assemble. The actual PHP fixture and exact native
initialization/observation/retarget/release shims also compile and assemble for
linux-x86_64, linux-aarch64, macos-aarch64, ios-arm64, and ios-sim-arm64. iOS uses
staticlib output. Execution evidence here is Linux x86_64; the other targets
have assembly verification. Reproduction scripts:
/tmp/mbstring-reference-begin-validate.py and
/tmp/mbstring-reference-begin-targets.py. Artifacts and logs:
/tmp/mbstring-reference-begin-targets/ and
/tmp/mbstring-reference-begin-final-*.log. All own test/build processes are terminal.

Read-only PR refresh at 2026-09-09 13:45:12 UTC found seventeen open PRs.
The five tracked mbstring PRs #895, #898, #899, #900, and #902 remain open at
their previously recorded exact heads. Full state is retained in
/tmp/mbstring-reference-begin-pr-ledger.json. No commit, PR mutation, external
message, or subagent was used.

Next integration work must define and enforce the actual caller-reference
carrier, typed constraints, deferred request ownership/shutdown, and V4
initialization/filling/release routing for both AOT and eval. The native invoke
table is still V3. Existing __rt_mbstring_release may supply protected writer
release once its argument convention is verified; avoid creating a duplicate
cleanup mechanism without inspecting it. All earlier ownership/lifecycle gaps
and the seven unfinished public functions remain open. Keep the complete
mbstring goal active until public integration and the remaining scope are done.

## Native V4 capture coordinator integration, 2026-09-09

The previous goal turn was verified implementation progress. This turn adds the
native V4 invocation path and checks it through the actual shared regex engine.
HEAD remains 217ff6caad7e0965688c54b41d2410a8f5e92dec. Public support still remains
58/65 functions and all nine constants. No PHP capture builtin has been registered.

New runtime entry __rt_mbstring_capture_invoke accepts six C inputs: ignore-case,
argument pointers, actual count, strictness, original eval context, and optional
MbNativeCaptureV1 state. It returns the same native value/status/length/kind tuple
as the ordinary mbstring invoker. It is emitted only with the mbregex capability.
The neutral state is sixteen bytes: reviewed initialization mode at zero and an
owned discarded value at eight. Invocation clears that output before callbacks.
The caller supplies fresh ownership storage and must adopt the returned value's
request lifetime, including after a pending-throw return. Type validation and
caller-lvalue resolution are not supplied by this internal state.

The stack-local V4 host reuses the authoritative ten-entry base callback inventory
from invoke.rs, now visible to its sibling module. Small tail-call wrappers restore
the original eval context before delegating to each existing callback. Capture
filling reuses the live-reference graph writer. Capture release reuses the same
protected __rt_mbstring_release callback as ordinary owner cleanup. No second
release mechanism was introduced.

Native invocation uses host bytes 0-119, wrapped context at 128/136, bridge result
at 144, and status at 192. AArch64 reserves 224 bytes with linkage at 208; x86_64
reserves 208 bytes after push rbp. Capture state is saved before __rt_mbregex_init,
which preserves only the original five invocation inputs. The separate V4
initialization adapter uses a distinct twenty-four-byte internal result, transfers
ready/writer to the coordinator's sixteen-byte result, and transfers discarded
ownership to the caller's state before returning success, fatal, or pending status.

The new native V4 integration test drives real PHP destructors, real shared
argument cloning and cleanup, managed Oniguruma, named/numeric capture graphs,
and actual native exception handling. Across sixty-four executions, the old
destructor changes regex options from case-sensitive to case-insensitive before
compilation, optionally retargets the reference, and optionally throws. Matching
uses the live changed options. All captures are inserted before the pending
throw returns. Typed replacement arrays keep unrelated entries; untyped replaced
owners survive until the explicit deferred release. Every execution has a clean
heap. Boolean results are also checked through the fixture's status/result encoding.

A second test executes eight native protocol cases: ordinary capture success;
two-argument true and false results without capture state; empty-pattern and
argument-count exceptions before mutation; missing capture state; invalid native
mode; and a regex compile warning after initialization. Exact traces, warning
text, old-output lifetime, and clean heaps pass. The existing observer shim now
reports non-array tags too, allowing unchanged old objects to be checked safely.
These fixtures still create their references through native test shims. They do
not prove public AOT lvalue lowering or opaque eval capture dispatch.

Final focused runs: contract layout 1 passed; independent native fixtures 2 passed;
PHP capture group 8 passed; existing public regex-match group 3 passed; shared
capture-apply ABI 5 passed; runtime emitter tests 8 passed. Total: 27 passed.
The curl-enabled exporter, all builtin-doc generators and audits, enforced EIR
target-boundary audit, and warning-free cargo build -p elephc pass. All 2087
documentation artifacts remain byte-identical to their before manifest. Assembly
comments, module preambles and function docblocks for all seven touched Rust files,
and git diff --check pass. No full project suite or cargo fmt was run.

All five complete runtimes assemble, as do the main V4 PHP fixture and the exact
coordinator/initialization/observation/retarget/release test shims. The shim artifacts
include calls with and without capture state. iOS device and Simulator use staticlib
output. Execution evidence remains Linux x86_64; other targets have assembly
verification. Scripts: /tmp/mbstring-capture-v4-validate.py and
/tmp/mbstring-capture-v4-targets.py. Artifacts: /tmp/mbstring-capture-v4-targets/.
Logs: /tmp/mbstring-capture-v4-final-*.log and /tmp/mbstring-capture-v4-php.log.
The initial missing child-module path and assembly-comment alignment issues were
fixed before final validation. All own processes are terminal, including 47313,
33281, 2863, and the final validation runner 49068.

The five superseded-candidate PRs retain their last verified ledger from
2026-09-09 13:45:12 UTC in /tmp/mbstring-reference-begin-pr-ledger.json. This turn
made no PR writes, commits, external messages, or subagent calls.

Next public integration must supply actual stable caller reference identities
through AOT lowering, callable adapters, and eval. Existing AOT local/property/
element reference cells are raw storage, not automatically the managed tag-7/high-1
reference required by this invocation entry. reference_arguments.rs can also stage
temporary Mixed writeback cells, which do not provide destructor-observable direct
publication. Relevant existing paths are LoweringContext::promote_local_mixed_ref_cell,
local_stores.rs, property_access.rs, and Magician persistent_references.rs. Do not
silently treat a value snapshot or a raw ref-cell address as a persistent reference.

Typed-reference rejection must remain at the PHP-observable initialization point
after string coercion and empty-pattern validation. Supplying a reviewed typed
mode is not a substitute for that runtime validation. Deferred request ownership
and shutdown ordering remain unimplemented. All earlier ownership/lifecycle gaps,
the seven unfinished public functions, examples, packaging/target execution, and
the eventual superseding PR comment remain part of the active full mbstring goal.

## Compiler-owned Mixed references and capture publication, 2026-09-09

This turn is verified implementation progress, not public builtin completion.
HEAD remains 217ff6caad7e0965688c54b41d2410a8f5e92dec. Public support remains
58/65 functions and all nine constants. No capture builtin was registered.

Owned local Mixed reference promotion now uses the existing managed reference
layout: heap kind five, tag seven, boxed child at eight, marker one at sixteen.
The native local and its hidden LocalKind::RefCell owner store the child address
(wrapper plus eight), preserving existing native loads, stores, aliases, and
by-reference argument conventions. Promotion uses __rt_mixed_from_value to retain
the current boxed child and then releases the replaced local owner. It does not
introduce a new runtime allocator, reference tag, or release mechanism.

This layout applies only when promotion has a tracked owner and Mixed storage.
Capture-only promotion with no hidden owner, concrete native cells, borrowed
parameters, property cells, and element addresses retain their existing layouts.
Production cleanup relies on the tracked owner's declared representation; it
does not infer arbitrary pointer provenance by probing adjacent memory.

The ABI owner-release helper recovers the wrapper and decrefs it, allowing a
separate identity owner to survive frame exit without retaining the old child.
Protected destructor cleanup uses that same helper after clearing the owner slot.
It neither releases the child separately nor frees the interior child address.
Raw reference cleanup keeps its existing protected payload/free path.

The first real AOT capture fixture exposed an additional ownership bug:
store_value_to_ref_cell_as boxed an owned source as borrowed, leaving an extra
object owner and delaying its destructor. It now uses the same
value_can_own_mixed_box_source predicate and owned boxing helper as ordinary local
stores. The independent PHP regression assigns a new object through a Mixed alias
and checks immediate destruction when that alias is cleared, with a clean heap.

New AOT capture coverage creates locals, aliases, callbacks, and managed cells
through actual PHP lowering. Only the capture call body is a native adapter;
it verifies the expected carrier and invokes the existing V4 coordinator.
Across sixteen executions, an old destructor observes null through the actual
alias, assigns a replacement array, changes regex options before compilation,
and optionally throws. Both original and alias variables see all three final
captures. Exact output agrees with PHP 8.5.10 and the native heap is clean.
Discarded replacements contain only scalars and are explicitly released after
the call, so this does not implement request-deferred destructor ownership.

A second fixture retains the managed identity across an ordinary function exit,
a normal destructor exit, and a throwing destructor exit. Across twenty-four
executions, the child remains live until explicit pin release, then destructs
exactly once; the heap is clean. Existing aliased-destructor cleanup coverage now
runs both raw object cells and managed Mixed cells, including a throwing child,
later local cleanup, and preservation of the previous parent exception.

Final focused validation: capture group 10 passed; owned Mixed assignment 1
passed; destructor local cleanup 3 passed; reference group 25 passed. Total:
39 passed. cargo build -p elephc is warning-free. The enforced EIR target-boundary
audit, assembly-comment check for all four touched production files, seven Rust
module/function documentation checks, and git diff --check pass. No full project
suite or cargo fmt ran. Builtin metadata and generated docs were not changed.

Both AOT PHP fixtures and their exact native call/pin/coordinator adapters assemble
for linux-x86_64, linux-aarch64, macos-aarch64, ios-arm64, and ios-sim-arm64. The
iOS fixtures use staticlib output. The initial Apple assembler rejection of an
external conditional branch in the new test shim was corrected with a local
condition and unconditional external branch. Execution evidence is Linux x86_64;
the other targets have assembly verification. Scripts and artifacts:
/tmp/mbstring-aot-local-validate.py, /tmp/mbstring-aot-local-final-*.log,
/tmp/mbstring-aot-local-targets.py, /tmp/mbstring-aot-local-targets/,
/tmp/mbstring-aot-local-php-oracle.php, and its .log.

An additional callable temporary leak remains open: directly invoking
($this->observe)() in the destructor acquires the property descriptor without
releasing that temporary. The original fixture produced the correct trace but
leaked sixteen blocks / 1536 bytes over sixteen calls. Its retained assembly is
/home/nahime/.cache/elephc-mbstring-test-tmp/mbstring_capture_aot_local_1832573_ThreadId(2)_0/caller.s.
The final reference-focused fixture reads the property into a local and invokes
that local, with clean ownership. This does not close the broader callable
temporary cleanup issue. Capture-only raw cell lifetime and closure ownership
remain open as previously recorded.

The PR ledger was refreshed at 2026-09-09 14:40:51 UTC. Seventeen PRs are open;
the same five mbstring candidates (#895, #898, #899, #900, #902) retain their prior
head SHAs. Evidence: /tmp/mbstring-aot-local-pr-ledger.json. No PR writes, commits,
external messages, or subagent calls were made. Own sessions 28646, 22138, 43401,
34624, 27374, 23327, 61303, 22428, 58519, and 43982 are terminal.

Next public integration must expose compiler-guaranteed stable references to
the V4 adapter, handle raw/borrowed and nonlocal lvalues, validate typed constraints
at the observable initialization point, route direct/callable/eval calls, and
adopt request-deferred owners with correct shutdown ordering. The seven unfinished
functions and all earlier ownership, HTTP/mail, packaging, examples, and final PR
work remain part of the active full mbstring goal.

## Callable temporary cleanup and unresolved return ownership, 2026-09-09

At 15:06 UTC this turn has made implementation progress, but a new regression is
still failing. Do not describe the current worktree as fully validated. The full
mbstring goal remains active with 58/65 public functions and all nine constants.
No public builtin was added or removed and HEAD remains
217ff6caad7e0965688c54b41d2410a8f5e92dec.

The original AOT capture fixture now calls ($this->observe)() directly again.
It passes with a clean heap, so the local-variable workaround from the previous
checkpoint was removed. EIR descriptor invocation now guards owned callback
temporaries before argument evaluation, and releases them after argument cleanup.
Owned results are also guarded while argument/callback cleanup can throw. This
uses the existing exception-owned guard protocol and ownership predicates.

Changed production files are src/ir_lower/expr/descriptor_calls.rs,
src/ir_lower/expr/closure_calls.rs, and src/ir_lower/expr/descriptor_invoke.rs.
guard_owned_descriptor_callback registers only owning temporaries not already
guarded. Calls that reach emit_callable_descriptor_invoke consume their owning
callback and argument container and then transfer the guarded result onward.
Early guards were added to literal callable-array invocation, expression calls,
generic closure-variable invocation, first-class method expressions,
call_user_func descriptor preparation, and call_user_func_array preparation.
Legacy ExprCall-only branches and the immediate-static-closure shortcut have not
been normalized by this change; do not infer complete callable lifetime coverage.

New tests are under
tests/codegen/runtime_gc/argument_guards/callable_temporaries.rs, included by
argument_guards.rs. test_mbstring_descriptor_temporary_callback_ownership passes:
direct, named, call_user_func, and call_user_func_array calls cover successful
returns, failures while evaluating arguments, and failures in the callback body.
Across ninety-six factory-created captures, exact destructor order and clean heaps
pass. Their callback bodies call the public mb_strlen implementation.

test_mbstring_descriptor_borrowed_object_return_ownership also passes with a clean
heap. It checks objects returned from a native parameter and a by-value capture,
including continued caller access after releasing the separate returned value.
This is a required counterexample to indiscriminately consuming every native
return pointer in the descriptor boxer.

test_mbstring_descriptor_result_cleanup_after_callback_throw is currently RED.
It covers all four combinations of a throwing/nonthrowing captured-object
destructor and returned-object destructor, eight times each. PHP prints each
returned object's destruction, including cleanup when the callback's capture
throws before the result can be assigned. Native execution never destroys any
of the thirty-two returned objects and leaks thirty-two blocks / 1280 bytes.
The returned-object exception therefore never replaces/chains the capture
exception either. This failing regression remains enabled and must be fixed.

The confirmed boundary is src/codegen/runtime_callable_invoker.rs,
emit_boxed_invoker_return (around line 256). It consumes only Str, whose prior
restore_concat_offset_after_nested_call unconditionally persists the string.
All other concrete heap results are boxed with the borrowed helper, which retains
freshly returned objects without consuming their original owner. The new result
guard releases its result box correctly, but that extra original owner survives.
No return-boxing or return-ABI implementation was edited in this turn.

Return ownership is not uniform today. src/ir_lower/stmt/control_exit.rs acquires
container/property/static-slot reads and returned $this, but immutable parameter
and capture loads can return borrowed pointers. Owned locals transfer ownership
via frame::return_cleanup_skip_slot. Array/hash parameters use callee-owned COW
shadows. LoweringContext::value_is_borrowed_user_call_result and ReturnArgAlias
summaries explicitly encode borrowed parameter returns for direct calls.
release_owned_call_arg_temporaries_with_signature in nullable_method_calls.rs
uses alias-aware cleanup, including ReleaseUnlessAliases. These consumers must
stay coherent with any return-ownership normalization; a blanket change to owned
boxing is unsafe. Ternary lowering already uses owned hidden temporaries.

RuntimeCallableInvoker currently carries only signature, captures, label, and
optional mbstring operation. Its shared cache key also lacks an ownership policy.
Construction occurs in runtime_wrappers.rs, eval_callable_helpers.rs, and
builtins/eval/function_registration.rs. If adding an ownership contract, its
cache identity and all AOT/eval entry paths must be addressed. A complete uniform
owned-return strategy would also need to reconcile native caller argument and
temporary Mixed-box cleanup, source return lowering, and builtin/extern wrapper
contracts. No architectural fix was selected or implemented yet.

Verified this turn: original AOT capture test 1 passed; new invocation-form test
1 passed; new borrowed-result test 1 passed; codegen::callables:: group 447 passed.
One new result-cleanup test failed as detailed above. Six PHP 8.5.10 oracle
programs agree with all checked-in expected traces, including the failing native
case. Evidence: /tmp/mbstring-descriptor-*-oracle.php and their logs;
/tmp/mbstring-descriptor-callables.log contains the 447-test result. Six touched
Rust files have checked module preambles/function docblocks; git diff --check
passes. No full suite, cargo fmt, builtin-doc regeneration, or new cross-target
assembly verification ran in this unfinished checkpoint.

Own sessions 43161, 37350, 1913, and 49391 are terminal. No commits, PR writes,
external messages, or subagent calls were made. The PR ledger remains the prior
14:40:51 UTC read in /tmp/mbstring-aot-local-pr-ledger.json. Continue by resolving
the contradictory native return ownership without weakening either fresh-result
or borrowed-result coverage, then complete the pending capture adapters and the
rest of the original mbstring scope.

## Typed object return ownership and remaining eval allocations, 2026-09-09

Checkpoint at 15:45 UTC. The previous goal turn made progress by fixing temporary
callback cleanup and exposing an enabled typed-result regression. This turn fixes
that regression without weakening its expected trace or clean-heap assertion.
The goal remains incomplete: RuntimeBuiltinId::MBSTRING is still 58 entries, with
all nine constants supported. No public builtin bindings or contracts changed.

By-value native returns whose final representation is PhpType::Object now carry
an independent caller owner. In src/ir_lower/stmt/control_exit.rs,
acquire_borrowed_return_value acquires borrowed object results, including
immutable parameters and captures. Provisional concrete local loads require
special handling: acquire the result, emit a release for the possible owned
Mixed unbox, and let existing Builder finalization prune that source release if
the local stays concrete. The acquired result no longer transfers the local's
frame owner, so ordinary local cleanup balances it. Fresh results and owned
expression temporaries keep their existing owner. acquire_returned_this avoids
acquiring an already owning result twice. By-reference returns retain their
previous path.

LoweringContext::value_is_borrowed_user_call_result excludes these typed object
returns. Native call argument cleanup likewise releases temporary arguments
independently of the returned object, even when their payloads alias. It guards
the result while argument destructors run, then hands the owner onward. This
preserves replacement/chaining of exceptions when argument and result
destructors both throw.

src/ir_lower/expr/function_calls.rs now owns
release_user_call_argument_temporaries, used by both ordinary user calls and
the user-function/static-closure arms of callable_resolution.rs. Those static
callable arms previously omitted argument cleanup entirely. Closure cleanup
consumes only visible argument temporaries, not hidden capture operands.

src/codegen/runtime_callable_invoker.rs consumes owned typed object returns
when boxing its result. Its indexed by-value object argument loader now borrows
the argument container's owner, matching native parameter ownership instead of
retaining an additional owner that nobody released. Hash argument paths already
borrow typed object payloads. Other return representations retain their existing
contracts: this is not a blanket owned-return change for arrays, Iterable,
Callable, or Mixed. No per-entry policy/cache flag was introduced because the
object representation now has one by-value return contract across native source
functions and their descriptor invokers. Extern signatures cannot return Object;
ordinary builtin callable signatures use their existing representations.

The callable_temporaries.rs regression module now has six tests. All pass with
exact PHP destructor/exception ordering and clean heaps. Besides the previous
three tests, coverage includes direct/first-class/dynamic-descriptor/
call_user_func/call_user_func_array/method/static-method argument returns,
local and widened-slot returns, forwarded and conditional returns, receiver
returns, by-reference parameter reads, and result cleanup after argument
destructors throw. The latter covers both positional and named descriptor
containers. The original temporary callback/property-call coverage remains
enabled and passed in the runtime-GC group.

Verified current focused suites: codegen::callables:: 447 passed;
codegen::runtime_gc:: 352 passed; codegen::references:: 25 passed;
codegen::eval_callables:: 49 passed; codegen::eval_closures:: 12 passed.
Total 885 unique passing tests, including the six descriptor ownership tests.
Logs: /tmp/mbstring-object-return-{callables,runtime-gc,references,
eval_callables,eval_closures}.log. cargo build -p elephc passed without Rust
warnings. Seven touched Rust files have checked module preambles and function
docblocks; a missing nested helper docblock in function_calls.rs was added.
The assembly-comment checker, enforced builtin EIR boundary audit, and
git diff --check passed. No full suite, cargo fmt, or builtin-doc regeneration
ran, and no public builtin metadata changed.

/tmp/mbstring-object-return-targets.py verified fifteen exact PHP fixtures with
EIR optimization both on and off for all five supported targets: 150 generated
assembly files assembled successfully. Thirty Linux x86_64 executions matched
PHP 8.5.10 stdout exactly and reported clean heaps. Other targets were assembled,
not executed locally. Sources, emitted assembly, binaries, and per-step logs are
under /tmp/mbstring-object-return-targets/. The original PHP oracle sources and
traces are /tmp/mbstring-object-return-*.php and their .php.log files.

An additional opaque-eval probe is semantically correct but NOT heap-clean.
/tmp/mbstring-object-return-eval.php repeatedly calls native typed object
factories and identity functions through ordinary eval calls and eval first-class
callables. All objects are destroyed at the PHP-equivalent point, and stdout
matches PHP, but eight entries retain 176 blocks / 10450 bytes. This standalone
heap assertion failed; do not report full eval ownership as fixed or count it
as a clean-heap test. It is a retained diagnostic fixture, not an ignored or
weakened checked-in test.

Isolation probes under /tmp/mbstring-object-eval-probe-* show:
- Empty eval, one entry: 2 blocks / 64 bytes retained.
- Empty eval, eight entries: 16 blocks / 624 bytes retained.
- Object probe, one entry: 22 blocks / 1308 bytes retained.
- Object probe, eight entries: 176 blocks / 10450 bytes retained.
- Eight object sequences in one eval entry: 162 blocks / 10058 bytes retained.
Thus there is both an entry-related residue and twenty additional blocks per
object sequence; destructor traces alone do not prove all intermediate cells
are released. No fix for this residual allocation growth was attempted yet.
Candidate consumers to inspect next include native_execution.rs under
crates/elephc-magician/src/interpreter/dynamic_functions/: its argument-array
builder allocates index cells without an explicit release, and error paths
after a native result is returned need ownership review when argument cleanup
or reference writeback fails. These observations are not a proven explanation
of all retained blocks. Continue isolating those allocations, then complete
the capture reference adapters and the original remaining mbstring surfaces.

The open PR ledger was refreshed this turn and saved at
/tmp/mbstring-object-return-pr-ledger.json. Seventeen PRs remain open; the five
mbstring heads are unchanged: #895 e96b43f219c085551a3c42c4cbbbd752846c0f88,
#898 b15d629eb1d159ffb32fe272eb0f5c17472fb97b,
#899 089ffc6bee2d2b3f17746da6fdf435f5be423440,
#900 f402f762e093b4ad1c98bee1c83cb59bb4278c8c,
#902 405f77283dd4e5c805eff74dc70f3fe013b56885.
No commits, PR writes, external messages, or subagent calls were made.
All own test/build/probe sessions are terminal. There is no external blocker;
the full mbstring goal remains active.

## Eval output and native value ownership, 2026-09-09

Checkpoint at 16:46 UTC. HEAD remains
`217ff6caad7e0965688c54b41d2410a8f5e92dec`. This continuation made progress;
the goal is not complete. Public support remains 58/65 functions and all nine
constants. No public builtin contracts or bindings changed in this checkpoint.

The two previously failing checked-in tests in
`tests/codegen/runtime_gc/eval_native_calls.rs` now pass without weakening their
assertions. The eight-entry native object-return fixture, originally retaining
176 blocks / 10450 bytes, now reports a clean heap. Repeating the output fixture
24 times also reports a clean heap, including temporary Stringable destruction.
The argument-array-key regression passes as well.

Changes across this checkpoint:
- Magician releases temporary native free-function argument-array indices.
- Shared declared-return cleanup releases rejected or replaced return cells,
  including an undelivered coerced result when original-value destruction throws.
- `echo` and `print` share owned-expression output cleanup, releasing both the
  original operand and a distinct Stringable conversion result.
- AOT eval result ownership recognizes `ProfiledData` as well as `Data` metadata.
  Eval source cleanup follows scope widening and uses an independent-result
  contract. String reads from known Mixed locals are recognized as owned casts.
- `src/codegen/eval_value_helpers.rs` shares borrowed native string argument
  staging and derives consumable method-return owners from EIR. By-reference
  strings keep separate mutable copies. Compact Throwable message storage keeps
  its owning conversion. Method boxing consumes typed by-value object results
  and string results proven owned through metadata or Acquire/StrPersist.
- `docs/internals/memory-model.md` records these eval value boundaries and the
  conservative fallback for unproven string returns.

Validation: 434 unique codegen tests passed across runtime_gc (355),
eval_constructors (10), eval_callables (49), eval_closures (12), native function
entry forms (7), and native Stringable contexts (1). Logs are
`/tmp/mbstring-eval-owner-validation-*.log`. The initial GC run had 352 passes
and three environment failures: its isolated cache lacked Oniguruma and network
name resolution was unavailable. The cached source and revision-3 artifacts were
copied into `target/mbstring-oniguruma-cache`; all four capture-invocation tests
then passed, including the three failures. This is dependency recovery, not a
code or assertion change. Rerun log:
`/tmp/mbstring-eval-owner-capture-rerun.log`. Earlier validation in this checkpoint
also passed 21 Magician native-scope and nine magic-method unit tests.

`/tmp/mbstring-eval-owner-targets.py` verified the two exact regression fixtures
with EIR optimization on and off for all five supported targets. All 20 assembly
files assembled. The four Linux x86_64 runs matched PHP 8.5.10 stdout and reported
clean heaps. Other targets were assembled, not executed. Artifacts and the clean
`cargo build -p elephc` log are under `target/mbstring-eval-owner-targets/`.
The enforced builtin EIR boundary audit passed, all four edited assembly emitters
passed comment alignment, 15 touched Rust files passed preamble/function-docblock
checks, and `git diff --check` passed. No full suite or cargo fmt ran.

Future focused runs can stay inside the writable workspace without escalation:
TMPDIR is `target/mbstring-eval-output-probes/tmp`, XDG_CACHE_HOME is
`target/mbstring-eval-output-probes/cache`, and ELEPHC_NATIVE_CACHE is
`target/mbstring-oniguruma-cache` (use absolute paths in command environments).
An initial escalation request timed out in automatic review; using these local
cache directories completed the work without additional permissions.

The five tracked PRs were reread through exact GitHub PR metadata. All remain
open at their existing recorded heads. The refreshed compact ledger is
`/tmp/mbstring-eval-ownership-pr-ledger.json`. No PR comments, closures, commits,
external messages, or subagent calls were made.

Return to the original remaining surfaces: mb_convert_variables, mb_ereg,
mb_ereg_replace_callback, mb_eregi, mb_output_handler, mb_parse_str, and
mb_send_mail. Capture lvalue adapters and request/shutdown owner ordering remain
unfinished. This checkpoint does not claim complete eval ownership: native method
argument-array read owners, transformed/default argument owners, and methods with
mixed borrowed/owned string return paths still need bounded follow-up where they
affect those surfaces. Avoid replacing the original mbstring goal with a general
compiler ownership rewrite. All own processes are terminal; no external blocker
remains and the full goal stays active.

## Unique indexed capture destinations, 2026-09-09

Checkpoint at 17:24 UTC, HEAD still
217ff6caad7e0965688c54b41d2410a8f5e92dec. This is verified internal capture
adapter progress. Public mbstring remains 58/65 functions and all nine constants;
the mb_ereg/mb_eregi contract support guards still report ReferenceAdaptersPending.

The new capture_destination emitter follows the current persistent reference to
its terminal Mixed cell. Existing hashes preserve their exact payload identity.
A uniquely owned indexed payload is converted into a heterogeneous hash through
array_to_hash; the existing terminal cell is updated before the old indexed
owner is released. Aliases to that same cell observe the new representation.
Each later capture resolves the live destination again, so the existing guarded
hash writer continues to contain destructor throws and follow retargeting.

Shared indexed payloads still return the unsupported host status before any
mutation. Copying only one payload owner would hide PHP's capture writes from
other array aliases. Their representation adapter remains required for public
capture integration, alongside caller lvalue resolution, typed-reference
validation, callable/eval routing, and request-deferred ownership/shutdown.

The shared array_to_hash helper now retains x86_64 child pointers through rax,
the actual native incref convention. Its previous rdi staging freed live Mixed
elements and could run a destructor during container conversion. Both targets
also retain callable payloads with runtime tag 10. Inline nullable scalar arrays
(storage tag 11) now contribute their per-slot PHP tag, including float bits and
null, with heterogeneous destination metadata.

Four new executable capture fixtures cover unique indexed Mixed/object arrays,
destructor throws with subsequent writes, unchanged shared indexed aliases,
binary string strides, closures inside Mixed arrays, and nullable scalar slots.
They use real compiler-owned Mixed reference cells and replace only the internal
capture invocation. The independent C fixture additionally verifies a separately
retained terminal cell, rejected shared indexed owners, raw callable-tag owners,
and inline float/null payloads, with complete allocation accounting.

Final validation: 51 distinct codegen tests passed (14 mbstring_capture, 28
assoc_set_ops, 6 strtr, and 3 regression_408). The 14 capture tests also passed
with ELEPHC_IR_OPT=off. The native C ownership test passed. The explicit ignored
clang test assembled the complete changed destination, reference, and conversion
helpers for linux-x86_64, linux-aarch64, macos-aarch64, ios-arm64, and
ios-sim-arm64. Executable behavior was checked on the local Linux x86_64 host;
the other targets received assembly validation here. cargo build -p elephc was
warning-free. The enforced builtin EIR boundary audit, assembly comments, nine
touched Rust-file preambles/function docblocks, and git diff --check passed.
Final logs are /tmp/mbstring-capture-indexed-final-*.log; the default optimized
capture result is /tmp/mbstring-capture-indexed-all.log. All own sessions,
including final validation driver 38602, are terminal.

Two bounded follow-ups were isolated during validation. Compiler-created arrays
containing only Callable elements currently lack tag 10 in
emit_array_value_type_stamp. Enabling that stamp exposed missing callable retains
in the generic array/hash union conversion used by ordinary PHP assignments.
The final change leaves that compiler metadata table unchanged; the current
closure fixture uses correctly stamped Mixed storage, while the independent C
test covers valid tag-10 native input. The ordinary PHP reproducer remains at
/tmp/mbstring-indexed-callables-oracle.php, with the earlier native failure in
/tmp/mbstring-capture-indexed-ordinary.log. This is still required follow-up for
complete mbstring value adaptation. A closure returning an object's string
property also left one allocation per invocation in a discarded probe; the
final ownership fixture returns an integer to isolate capture/container owners.
Do not treat either result as complete general callable/return ownership.

No public contracts or builtin bindings changed, so generated builtin pages were
not regenerated in this checkpoint. No commits, PR writes, external messages,
or subagents were used. The five candidate PRs remain in the previously verified
ledger; no fresh PR-state claim is made here. Continue the seven missing public
functions and the full original goal. Do not mark it complete or blocked.

## Typed capture invocation, 2026-09-09

Checkpoint at 17:50 UTC; HEAD is still
217ff6caad7e0965688c54b41d2410a8f5e92dec. This is progress on the active goal,
not completion. Public support remains 58/65 PHP mbstring functions.

RuntimeBuiltinId now includes MbEreg (80) and MbEregi (81). The shared
elephc_mbstring_invoke_v1 entry selects their existing protected reference
coordinator before ordinary by-value argument copying. The capture-specific C
entry remains a compatibility wrapper over these typed IDs. The native
__rt_mbstring_capture_invoke C6 helper now accepts RuntimeBuiltinId as its
first argument and calls the shared entry directly; its other inputs and
result convention remain unchanged. Update future callers accordingly.

Engine operation identity is separate from frontend availability. The AOT
runtime registry now requires a binding exactly when the authoritative support
contract declares an implemented registry route. Unsupported contracts must
have no binding. runtime_builtin_ids() exposes only this validated set, and the
generated boxed dispatcher derives its mbstring arms from that iterator.
Both capture PHP contracts remain ReferenceAdaptersPending in AOT and eval.
No capture homes or public bindings were added. The corresponding eval parity
gate still requires every implemented contract to use its declared runtime
binding and checks that unsupported contracts remain absent.

The shared mbstring checker now accepts concrete values for TypeSpec::Mixed,
which is the capture-output parameter contract. Ordinary scalar and array
parameter restrictions remain intact. Public capture lowering still needs to
provide the actual reference identity and initialization policy.

Validation completed without Rust warnings:
- Two runtime ID contract tests and 30 AOT registry tests.
- One eval runtime-binding parity test and four boxed dispatcher tests.
- Seven capture invocation tests, including 718 ordinary PHP corpus records,
  1200 output/destructor records, reference retargeting, malformed protocol
  inputs, and explicit typed-versus-legacy case-sensitive/caseless parity.
- Three coordinator panic-cleanup tests and one Mixed-parameter checker test.
- Fourteen native codegen capture tests. The three full invocation tests also
  passed with ELEPHC_IR_OPT=off. Native protocol cases now exercise MbEregi
  through the typed bridge, including its own exception name.
- The ignored clang test assembled capture invocation, reference dispatch,
  and indexed conversion on all five supported targets. Non-host targets
  were assembled, not executed.

This is 63 distinct focused tests plus three repeated without EIR optimization.
The compiler and curl-enabled builtin exporter built successfully. The full
generated-docs workflow, both documentation audits, and enforced EIR boundary
audit passed. Hash comparison of 2087 generated documentation files found no
changes. Three edited assembly emitters passed comment alignment, 13 touched
Rust files passed module/function documentation checks, and git diff --check
passed. Logs use /tmp/mbstring-typed-capture-*.log. All own process sessions
(5674, 83049, 42844, 27587) are terminal. No full suite or cargo fmt ran.

The five tracked PRs were refreshed through exact GitHub metadata and remain
open at the recorded heads: #895 e96b43f219c085551a3c42c4cbbbd752846c0f88,
#898 b15d629eb1d159ffb32fe272eb0f5c17472fb97b,
#899 089ffc6bee2d2b3f17746da6fdf435f5be423440,
#900 f402f762e093b4ad1c98bee1c83cb59bb4278c8c, and
#902 405f77283dd4e5c805eff74dc70f3fe013b56885.
The refreshed ledger is /tmp/mbstring-typed-capture-pr-ledger.json and is also
stored as mbstring_typed_capture_pr_ledger in functions storage. No PR writes,
commits, external messages, approval requests, or subagents were used.

A fresh comparison against local PHP 8.5.10 confirms the seven remaining public
functions: mb_convert_variables, mb_ereg, mb_ereg_replace_callback, mb_eregi,
mb_output_handler, mb_parse_str, and mb_send_mail. The coverage report is
/tmp/mbstring-typed-capture-coverage.json. All nine mbstring constants remain
accounted for by the existing implementation.

Next work should complete actual caller-reference adaptation and deferred-owner
request lifetime, not add more dispatch layers. Existing capture engine tests
already cover PHP semantics. Deferred overwritten owners still require adoption
beyond invocation cleanup. Main teardown in src/codegen/frame.rs and request
reset in src/codegen/web.rs currently call __rt_mbstring_release_catalog after
ordinary cleanup and before heap reclamation; these are relevant lifecycle
boundaries to inspect for that integration. No lifecycle code changed here.
Typed property constraints, shared indexed destination identity, and complete
eval/AOT lvalue forms remain outstanding. Preserve the callable metadata and
return-ownership follow-ups recorded in the previous checkpoint.

## Deferred capture request owners, 2026-09-09

Checkpoint at 18:13 UTC. The previous goal turn was progress, and this turn
adds native request-lifetime adoption rather than another dispatch layer.
HEAD was revalidated as 217ff6caad7e0965688c54b41d2410a8f5e92dec. The full goal
remains active; public support is still 58/65 functions with nine constants.

New src/codegen_support/runtime/strings/mbstring/deferred_capture.rs emits
__rt_mbstring_defer_capture and __rt_mbstring_release_deferred_captures.
Adoption consumes an existing boxed owner without copying its PHP value. Raw
sixteen-byte heap nodes retain next/owner pointers; those references stay
external roots under the collector's refcount-minus-graph-edge accounting.
The queue head, tail, and reentrant-drain flag live in runtime fixed data.
Draining detaches and frees a node before protected PHP cleanup, continues
through pending exceptions and newly adopted owners, and restores incoming GC
suppression. Reentrant drains leave traversal to the active outer drain.

__rt_mbstring_capture_invoke now automatically adopts its initialization
state's discarded owner after coordinator/materializer cleanup, preserving
all four native result words on success, pending throw, and fatal returns.
MbNativeCaptureV1.discarded is cleared before normal return. Its Rustdoc now
describes this generated-native behavior and custom-host responsibilities.
The lower-level MbCaptureReferenceInitV1 still transfers a separate owner to
its immediate host; its direct tests retain their explicit ownership checks.

__rt_mbstring_release_catalog drains request owners before clearing catalog
identity and string metadata. It propagates a pending destructor exception
only after those cleanup steps. The existing main epilogue, web reset, and
mbstring request-reset callers therefore reach the new lifetime cleanup.
No main/web lifecycle source was otherwise edited. Apple assemblers exposed
an external conditional-branch restriction in the new propagation path;
the final emitter branches conditionally to a local label and then uses an
unconditional branch to __rt_throw_current.

Native capture fixtures now check that reentrant discarded values remain alive
after invocation and cycle collection and are released after the program's
final output. New invoke/deferred.rs tests real queue adoption and cleanup
across four cycles, including destructor-created owners, recursive drains,
GC calls, pending exceptions, an empty second drain, and clean heap reports.
The compiler does not expose gc_collect_cycles as a PHP builtin: collector
probes use a fixture function whose body calls __rt_gc_collect_cycles. Its
result is printed so optimization cannot erase the probe. The drain fixture
declares a potentially throwing PHP body so the surrounding try handler is
emitted before that body is replaced with the actual native cleanup call.
Earlier fixture failures from those two test-adapter mistakes were corrected;
the final native run has fifteen passes.

Verified final results:
- Fifteen focused capture codegen tests, including the new deferred-root test.
- Five invocation/deferred tests repeated with ELEPHC_IR_OPT=off.
- Five shared native/eval catalog tests and one neutral capture ABI-layout test.
- One explicit clang test assembled full capture invocation, adoption/drain,
  catalog cleanup, reference dispatch, and indexed conversion on all five
  supported targets. Non-host targets were assembled, not executed.
- Compiler and curl-enabled builtin exporter builds completed without warnings.
- The complete generated-docs workflow, both docs audits, enforced EIR boundary
  audit, assembly-comment alignment, and git diff --check passed. Ten touched
  Rust files passed preamble/function-docblock checks. Generated file hashes
  show no documentation changes.

This is 22 distinct focused tests plus five repeats without EIR optimization.
Authoritative logs are /tmp/mbstring-deferred-capture-native-rerun.log and
/tmp/mbstring-deferred-capture-verified-{targets,no-opt,catalog,abi,build,exporter}.log.
Docs and boundary logs use /tmp/mbstring-deferred-capture-*.log. All own process
handles (89367, 51181, 47320, 69025, 28164, 35066) are terminal. The initial
failed native and target logs are retained separately. No full suite, cargo
fmt, commits, PR writes, external messages, approvals, or subagents were used.

Important remaining semantic work: queue order is currently adoption order,
not PHP object-store shutdown order. /tmp/mbstring-deferred-order-oracle.php
creates outer owner 1 before a nested capture creates owner 2, while nested
capture completion would adopt owner 2 first. PHP 8.5.10 prints:
created:1, created:2, collected:0, end, late:1, late:2 (one per line).
The cached primary source at /tmp/php-8510-src/Zend/zend_objects_API.c shows
zend_objects_store_call_destructors traversing valid object handles in
ascending order, with handle reuse disabled during that shutdown traversal.
Do not claim the FIFO queue is PHP-complete or replace this requirement with
an allocation-address heuristic. Nested object graphs and shutdown exception
behavior also require verification before final completion.

Continue toward actual capture frontend adapters, preserving reference identity
and typed constraints in AOT and eval. Request adoption is now available in
the generated native coordinator; custom eval-host paths must respect the same
ownership contract. Shared indexed output identity, callable metadata/return
ownership follow-ups, and the seven missing public functions remain as recorded
above. The PR ledger from 17:50 UTC remains the last verified metadata; this
turn made no new PR-state claim. Do not mark the goal complete or blocked.

## Direct AOT capture bindings, 2026-09-09

Checkpoint verified at 18:53 UTC against unchanged HEAD
217ff6caad7e0965688c54b41d2410a8f5e92dec. This is implementation progress,
not completion of the mbstring goal. No external blocker, commit, PR write,
approval request, or delegated agent work occurred.

### Implemented

The shared catalog's existing mb_ereg and mb_eregi contracts now join actual
AOT homes under src/builtins/string/. Both use typed RuntimeFnId operations,
the existing value-preserving argument planner, and the V4 native capture
coordinator. Output validation skips reading an undefined local; the checker
records its post-call Mixed representation. Ordinary arity, reference-lvalue,
strictness, and input validation continue through shared contracts.

Reference promotion happens at the output's source-order evaluation point,
including NamedArg wrappers. A prior conditional promotion is repeated
idempotently so every incoming runtime path has reference storage. Codegen
proves tracked Mixed local ownership, follows managed aliases, and rejects
raw by-reference parameters and BindRefCellPtr storage. It recovers the
wrapper from its child address only after that proof. Dynamic spread capture
references retain an explicit unsupported diagnostic.

The new __rt_mbstring_capture_native entry shares the existing native
mbstring unwind wrapper, then calls __rt_mbstring_capture_invoke with an
initialized untyped capture state. Runtime matching, initialization,
publication, deferred ownership, and pending throwable handling stay in the
existing shared engine and native host. No second regex implementation was
introduced.

The AOT registry now has 60 mbstring functions; eval and common availability
remain 58. Eval reference adapters retain ReferenceAdaptersPending, and
eval_runtime_builtin_ids filters those operations out of the boxed dispatcher.
The new AOT operations deliberately have StaticOnly callable policy until
runtime-selected reference wrappers exist. Generated support pages, registry,
indexes, comparison, audit counts, and pipeline assertions were regenerated
or updated. examples/mbstring/main.php now extracts supplier and label captures.
Runtime and compatibility-data documentation state the remaining limitations.

### Verified

- Eight actual PHP capture codegen tests pass: omitted output, undefined local,
  validation preserving an old scalar, named/source-order inputs, alias-observing
  destructor and pending exception, conditional promotion, later argument
  assignment, and explicit unsupported reference diagnostics.
- The same eight tests pass with ELEPHC_IR_OPT=off and ELEPHC_REGALLOC=stack.
  The destructor test uses heap debug and reports a clean heap. Successful
  output cases were compared with local PHP 8.5.10.
- One error test checks invalid arity, strict input types, and literal output
  references. Two registry tests, four boxed-dispatch tests, and three neutral
  support tests pass. Existing AOT/eval public regex matching and the complete
  mbstring example regression also pass.
- Five focused capture library/ABI tests pass, including real clang assembly
  for actual AOT caller code and the native capture/unwind helpers on all five
  supported targets. Non-host targets were assembled, not executed locally.
- Compiler and curl-enabled docs exporter build without warnings. All six
  generated-docs workflow steps pass, as do ten contract-pipeline Python tests.
  The audit reports 390 non-registry routes and zero errors.
- Twenty-two touched Rust preambles and 202 explicit function docblocks were
  checked. Assembly comments align in the three changed emitters; git diff
  --check passes. No full local suite was run.

Authoritative logs: /tmp/mbstring-aot-capture-tests.log,
/tmp/mbstring-aot-capture-errors.log,
/tmp/mbstring-aot-capture-unit-targets.log,
/tmp/mbstring-aot-capture-verified-{registry,dispatcher,support,no-opt-stack,match,build,exporter,example}.log,
and /tmp/mbstring-aot-capture-docs-{render,modules,comparison,audit,site,boundary,pipeline}.log.
All own process sessions are terminal. Generated-document changes from this
turn are listed in /tmp/mbstring-aot-capture-docs-changed.json.

### Coverage and continuation

/tmp/mbstring-aot-capture-coverage.json compares the generated registry with
the 65-function PHP reflection snapshot. Five functions still lack AOT homes:
mb_convert_variables, mb_ereg_replace_callback, mb_output_handler,
mb_parse_str, and mb_send_mail. Eval additionally lacks mb_ereg and mb_eregi.
The 60 AOT count means registered bindings, not full capture-reference coverage.

Continue with the capture reference adapters and their ownership contracts.
Raw parameters, properties, array destinations, runtime-selected callables,
and opaque eval need integration. Keep the existing shared indexed/COW and
resource/callable ownership debts visible. Deferred capture release order is
still adoption FIFO, not PHP object-store order; the preceding checkpoint's
counterexample and primary-source analysis remain valid.

Expression-level reference rebinding inside a later argument, such as
($matches =& $other), is currently rejected by the parser. It was not counted
as an implemented capture case. Any future adapter that admits rebinding must
preserve the reference identity selected at argument evaluation, including
its lifetime through later arguments. The accepted value-assignment case is
covered by test_mbstring_regex_capture_named_value_assignment.

The five tracked PRs were refreshed during this turn, all still open with
unchanged heads: #895 e96b43f219c085551a3c42c4cbbbd752846c0f88,
#898 b15d629eb1d159ffb32fe272eb0f5c17472fb97b,
#899 089ffc6bee2d2b3f17746da6fdf435f5be423440,
#900 f402f762e093b4ad1c98bee1c83cb59bb4278c8c,
#902 405f77283dd4e5c805eff74dc70f3fe013b56885.
Metadata is saved in /tmp/mbstring-aot-capture-pr-ledger.json. Superseding and
closing comments remain deferred until the complete feature is finished.
Do not mark the active goal complete or blocked.

## Eval capture references and callback values, 2026-09-09

Checkpoint verified at 19:42 UTC against unchanged HEAD
217ff6caad7e0965688c54b41d2410a8f5e92dec. This is further implementation
progress. No commit, PR write, approval request, external blocker, or delegated
agent work occurred. The goal remains active.

### Implemented

The shared mb_ereg and mb_eregi contracts now have actual Magician homes and
SharedRuntime bindings. The owned source-order argument evaluator reads passing
modes from the neutral contract. It promotes and pins persistent local reference
wrappers, including aliases and eval-declared reference parameters, instead of
copying their old values. Named dynamic calls use that same boundary. Explicitly
referenced output elements survive positional and named unpacking. Ordinary
inputs remain independent values; argument/default/reference owners are released
in parameter order on both success and failure.

The boxed native dispatcher routes the two capture IDs through the existing
V4 capture coordinator, passing initialized state through the sixth C argument.
The capture helper is referenced only with the mbregex capability. Both emitters
cover every supported target; an absent provider returns unsupported without
introducing an unresolved capture symbol. AOT still uses its existing typed path.

Two supporting eval defects were necessary for actual destructor integration.
Closure parameters now use the same dynamic class relationship as instanceof,
and callable/object parameters accept callable cells. Replacing the transient
null child exposed during untyped capture initialization now materializes an
owned PHP null, allowing the destructor to continue after a valid assignment
and propagate subsequent settings changes or an exception.

Direct call_user_func syntax now uses the common owned boundary in by-value
mode. It evaluates all supplied inputs, warns for the reference output using
the shared parameter metadata, and transfers that input's owner into a temporary
wrapper. It does not mutate the caller's variable. Releasing the superseded
input owner before invocation allows a temporary old object's destructor to run
during capture initialization. Tests also confirm explicitly referenced output
elements through call_user_func_array. Its by-value outputs and other values-only
callback entry points remain pending.

The generated registry now contains 60 AOT and 60 eval mbstring functions, with
60 in common. This is a registration count, not complete lvalue coverage. The
former eval ReferenceAdaptersPending count is zero. Generated builtin pages,
indexes, registry, runtime docs, compatibility-data docs, and contract-pipeline
assertions reflect the new bindings and the remaining limits.

### Verified

- Sixteen capture integration tests pass: eight prior AOT cases and eight eval
  cases covering public output, aliases/names, explicit unpacked references,
  destructors, pending exceptions, callback values, and repeated owner cleanup.
  The eight eval cases also pass with ELEPHC_IR_OPT=off and ELEPHC_REGALLOC=stack.
  Repeated ordinary and callback calls show no growth in residual allocations.
- One closure-parameter test, twelve eval registry tests, three neutral support
  tests, and five dispatcher tests pass. The latter include actual clang assembly
  for all five targets with mbregex both enabled and disabled. Non-host targets
  were assembled, not executed locally.
- The forced eval capability test, five existing value-spread tests, the shared
  eval arity regression, the MIME callback argument-failure ownership regression,
  and fifteen existing call_user_func interpreter tests pass.
- Compiler and curl-enabled exporter builds are warning-free. The full generated
  docs workflow passes, including ten Python pipeline tests, 390 non-registry
  route audits with zero errors, and 2,056 generated pages passing site checks.
- Seventeen touched Rust preambles and 166 explicit function docblocks were
  checked. The two changed dispatcher emitters pass assembly-comment alignment;
  git diff --check passes. No full local Rust suite was run.

Logs are /tmp/mbstring-eval-capture-verified-*.log,
/tmp/mbstring-eval-capture-callback-verified-*.log, and
/tmp/mbstring-eval-capture-docs-*.log. Changed generated paths, coverage, Rust
hygiene, and refreshed PR metadata are recorded in the corresponding
/tmp/mbstring-eval-capture-{docs-changed,coverage,rust-hygiene,pr-ledger}.json
files. All own process sessions are terminal.

### Oracle limits and continuation

The ordinary callback result/warning cases match local PHP 8.5.10. The destructor
test's validation-error case specifically uses zend.exception_ignore_args=1 as
its oracle. With argument-retaining PHP traces, the old temporary object remains
alive until the throwable releases its trace arguments. Native eval currently
releases that owner at call cleanup. Both PHP modes are recorded in
/tmp/mbstring-eval-capture-callback-trace-oracles.json, with a reproducible probe
at .plans/mbstring-probes/test_mbstring_eval_callback_trace_ownership.php.
Do not count argument-retaining throwable lifetime as implemented, or change
the test expectation to pretend the two PHP configurations have identical order.

Five functions still lack public AOT and eval homes: mb_convert_variables,
mb_ereg_replace_callback, mb_output_handler, mb_parse_str, and mb_send_mail.
Continue with their shared operations and the remaining reference adapters.
Raw AOT reference parameters, properties, direct array lvalues, non-reference
unpacked outputs, AOT runtime-selected capture wrappers, and general values-only
callback adaptation remain incomplete. Eval capture reference expressions
currently support LoadVar and persistent reference cells carried by explicit
unpacked elements; do not wrap an ordinary unpacked value in a detached cell
as a substitute for mutating the original array element.

The earlier shared-indexed/COW, callable/resource ownership, eval strictness,
and public INI/diagnostic integration debts remain. Deferred capture-owner
shutdown is still adoption FIFO rather than PHP object-store order. The preceding
counterexample and nested-graph/shutdown-exception requirements remain valid.

PRs #895, #898, #899, #900, and #902 were refreshed during this continuation.
All remain open with unchanged heads from the preceding checkpoint. Their
compact metadata is /tmp/mbstring-eval-capture-pr-ledger.json. Superseding and
closing comments remain deferred until the complete feature is finished.
Do not mark the active goal complete or blocked.

## Eval call-array owners and computed callbacks, 2026-09-09

Verified at 20:41 UTC on unchanged HEAD
217ff6caad7e0965688c54b41d2410a8f5e92dec. This checkpoint supersedes the
preceding pending claim for ordinary call-array outputs and values-only callback
entry points that reach the shared builtin argument boundary.

The registry now shares owned argument preparation across direct and evaluated
call_user_func/call_user_func_array, nested wrappers, array_map, and reflected
builtin invocation. Explicit references preserve caller identity; ordinary
callback output values warn and use temporary wrappers. Direct call-array syntax
releases its temporary source array before capture initialization. Dynamic
wrappers retain their ordinary outer argument frames until return. PHP 8.5.10
oracles verify the resulting destructor and regex-option timing difference.
ReflectionFunction construction preserves native construction and records the
shared builtin name for invoke/invokeArgs dispatch. This does not establish
complete reflection metadata, getClosure, or method argument lifetime parity.

Computed callback names and dynamic callees now retain their evaluated owners
through invocation. Owned concatenation releases operands and Stringable
conversion results. Full ternaries release an owned condition before evaluating
their selected branch. The native concat wrapper captures each converted byte
range independently and releases both buffers after boxing the concatenation,
fixing leaked string casts and numeric scratch-buffer overwrite on both target
architectures.

Twenty-eight capture integration tests pass, including twelve new callback
cases. Those twelve also pass with ELEPHC_IR_OPT=off and ELEPHC_REGALLOC=stack.
Focused interpreter checks pass: 41 dynamic calls, 12 reflection functions,
5 ternaries, 26 callback cases, and 6 closure cases. Existing native reflection
reference and MIME callback argument-failure ownership regressions pass. A
focused concat emitter test assembles with real clang for all five supported
targets; non-host targets were assembled, not executed. Compiler and exporter
builds pass without warnings.

The generated-docs workflow passes with no generated builtin/registry changes,
390 non-registry route checks and zero errors, 2,056 pages passing site checks,
and ten Python pipeline tests. All 21 touched Rust files have module preambles;
162 explicit function docblocks and affected assembly comments pass hygiene.
git diff --check passes. No full local Rust suite was run.

Evidence is /tmp/mbstring-callback-arrays-verified-*.log,
/tmp/mbstring-callback-arrays-docs-*.log, and the corresponding
oracles.json, rust-hygiene.json, and pr-ledger.json files with that prefix.
The five tracked PRs remain open with unchanged heads. All verification process
sessions are terminal. No commit, PR update, superseding comment, or closure was
made.

All five missing public functions and the preceding AOT/reference, request
shutdown, exception-trace, strictness, shared-indexed/COW, callable/resource,
and INI/diagnostic integration debts remain. General reflection argument
temporary ownership needs its own verification. The shared callback adapter's
fixed-parameter reference selection also needs extension before variadic
mb_convert_variables can use it. Begin the next public operation with the
shared HTTP parsing engine and mb_parse_str host boundary.

## Shared query preparation and multi-string detection, 2026-09-09

Verified at 20:56 UTC on unchanged HEAD
217ff6caad7e0965688c54b41d2410a8f5e92dec. The preceding callback verification
and docs workflow are complete. This continuation adds the shared engine stages
for mb_parse_str; it does not register that PHP function in either backend.
Public coverage therefore remains 60/65 functions in AOT and eval, plus all nine
constants. The full goal remains active.

### Implementation and evidence

The shared detector's guess entry now delegates to guess_many. The common
scorer validates every original input, visits strings in reverse collection
order, retains each candidate's decoder state across strings, skips the exact
PHP BOMs independently, and applies the single-precision candidate multiplier
after every string. It uses the existing bounded 128-point decoder interface.
An independent PHP mb_convert_variables oracle supplies 9,260 multiple-string
verdicts, including the original mb_list_encodings identity that disables order
weighting. All pass, as do all 468,304 existing single-string oracle records.

New src/input modules separate query decoding/detection, name planning, and an
owned graph host. Query::decode applies the raw C-string boundary, splits on
any configured separator byte, percent-decodes names and values independently,
and rejects the whole query when its variable limit is exceeded. Identification
uses all decoded names and values without order weighting. Single candidates
bypass strict validation like PHP; empty candidate configuration selects pass.
An actually empty raw query has no identification, unlike a nonempty query made
only of separators. Pair conversion records errors without clearing prior
request counts and reads live substitution settings for each pair.

registration emits ordered Enter, Store, and RemoveRoot steps with already
normalized keys. Native/eval adapters can apply these to live caller storage
without reimplementing name syntax. The owned Variables host verifies PHP's
negative next-index counters, maximum-integer append failure, canonical numeric
keys, malformed brackets, C-locale bracket whitespace (including vertical tab),
and forbidden mangled prefixes. Earlier child creations remain observable when
a later key is rejected. Nesting overflow removes the prior root, but an earlier
failed append stops before a later planned removal can execute.

The query fixture has 17,904 independent PHP records from six startup
configurations, every possible byte in names/brackets, all 79 destination
encodings, four substitution modes, candidate lists and strictness, separator
behavior, whole-query limits, and display-error-dependent nesting warnings.
Four engine tests pass, including independent regressions for partial output,
failed append versus nesting overflow, and empty/pass configuration. PHP syntax
checks pass for both new capture scripts. README instructions document fixture
regeneration and the exact pending public scope.

Focused shared automatic-conversion and invocation-catalog regressions pass.
All six existing mb_detect_encoding integration tests pass across AOT and eval,
including catalog identity, callback inputs, references, and owner cleanup.
Bridge and compiler builds are warning-free. All seven changed/new Rust files
have module preambles and all 34 explicit functions have docblocks. Source
whitespace checks and git diff --check pass. No new target-specific code was
introduced; non-host execution remains the CI matrix's responsibility.

Logs: /tmp/mbstring-input-detect-{single,many}.log,
/tmp/mbstring-input-query-engine.log, and
/tmp/mbstring-input-verified-{conversion,invoke,public,bridge-build,compiler-build}.log.
Rust hygiene is /tmp/mbstring-input-rust-hygiene.json. All process sessions are
terminal, including session 68017. No commit or external PR write was made.

### Next integration boundary

mb_parse_str needs a shared invocation coordinator with ordinary argument
coercion and a required pinned output at index one. PHP initializes the output
before snapshotting target/input encodings. Reuse the reviewed initialization
semantics, but do not pass nested query graphs through the V4 capture filler:
elephc_mbstring_capture_apply_v1 explicitly accepts only one flat array of
string-or-false entries. Replacing an entire snapshot after each field would
also discard exposed aliases and callback mutations. The new registration
steps instead need protected native/eval storage operations with live append
counters, nested COW, and ordered destructor/exception handling.

Read core parser configuration at the proper host boundary: arg_separator.input,
max_input_vars, max_input_nesting_level, and display_errors. No existing shared
core-INI routing for those names was found. Keep mbstring State authoritative
for input candidates, strict detection, target encoding, substitution, illegal
counts, and aggregate identification. Public mb_parse_str updates the aggregate
only, leaving the independent S source untouched. SAPI input filters, diagnostic
reentry, pending exceptions, caller/reference ownership, and startup settings
still require integration and fresh PHP traces. Do not mistake the pure engine
fixture for those host guarantees.

After the host boundary is validated, add neutral catalog/runtime identifiers,
typed AOT and eval homes, direct/named/dynamic/reference/error tests, all-target
emission checks, an example, and generated builtin documentation. The other
four missing public functions and all earlier outstanding debts remain. The
five tracked PRs retain the preceding read-only ledger; superseding and closing
comments remain deferred until the complete feature is finished.

## Shared mb_parse_str invocation and V5 host contract, 2026-09-09

Verified at 21:38 UTC on unchanged HEAD
217ff6caad7e0965688c54b41d2410a8f5e92dec. The previous turn made concrete
engine progress. This turn adds the actual shared invocation coordinator and
its protected host contract. Public AOT/eval storage adapters remain unfinished;
do not mark the overall goal complete or blocked.

### Implemented

mb_parse_str now has one neutral signature and RuntimeBuiltinId 82. The contract
has a required string source, a required by-reference mixed output named result,
and a bool return. The captured PHP reflection surface confirms the output has
no incoming type declaration. Both public backend support records explicitly
remain ReferenceAdaptersPending. There are 61 shared mbstring operations and
contracts, but still only 60 AOT and 60 eval bindings. All nine constants remain
implemented. The other four missing public functions still have no homes.

MbInvokeHostV5 is a 144-byte extension preserving the complete 120-byte V4 prefix.
The new callbacks are query_configuration at offset 120, optional query_filter
at 128, and query_register at 136. Configuration phases are entry (0, separators
and max_variables), field (1, current max_nesting after filtering), and diagnostic
(2, current display_errors after writes/destructors). A host normalizes the
display policy to the ABI's zero/one flag. MbQueryConfigV1 is 48 bytes,
MbQueryFilteredV1 32, MbQueryStepV1 40, and MbQueryRegisteredV1 8.

The main elephc_mbstring_invoke_v1 dispatcher routes operation 82 to
abi/invoke/query.rs. It validates arity before host access, requires a complete
V5 host before owner acquisition, copies/coerces only the source, and pins the
output without copying its old value. Reviewed V4 initialization semantics run
before mbstring settings are captured. The engine then splits/detects, converts
each field with live substitution state, runs an optional SAPI filter, and sends
normalized Enter/Store/RemoveRoot instructions to live host storage. Root aliases
remain observable; nested storage/COW belongs to the host. The flat V4 capture
filler is never used for query writes. Copied-value elephc_mbstring_call_v1 calls
explicitly reject this operation because they cannot supply the writer.

Pending callback exceptions retain valid completed metadata and allow the PHP
body to continue conversion and writes. Later user diagnostics are suppressed.
Aggregate HTTP input identification is committed after the body, including
pending exceptions; the independent S source is unchanged. Fatal host responses
stop work and preserve an earlier pending exception. All published configuration
and filter byte owners enter the common arena before status validation. Writer,
temporary, argument, and pin cleanup remains balanced on all exits.

### Evidence and corrections

Nine query invocation tests pass. They replay all 17,904 parser fixture records
through the real C ABI and an independent live table host, plus 94 PHP traces
covering output initialization, aliases/copies, scalar replacement, nested parse
calls, live settings, diagnostics, and pending throws. A source Stringable test
checks settings effects before output initialization and an early thrown cast.
Failure injection covers configuration/filter owners, output readiness, invalid
flags, unknown statuses, older/incomplete host tables, rejected outputs, pending
filter results, writer cleanup, and earlier exceptions surviving later fatal
responses. Older-host tests use each actual V1/V2/V3/V4 structure size.

The PHP reference-copy action now binds GLOBALS['copy'] by reference. Rebinding
a closure's local captured variable had left the outer copy null and did not
exercise the intended persistent alias. The fresh fixture records the corrected
PHP program. Suppressed INI setters still reach the user error handler during a
destructor; the replay therefore uses the real shared INI protocol with interned
literal identities and suppresses recursive handler invocation while a handler
is already active.

Two PHP worker cases did not produce valid semantic oracles. They are preserved
in tests/fixtures/parse_str_reentry_excluded.json: the max_variables=1,
seed=settings cases with action=settings or action=nested. The first exit status
was not retained; the second returned 255 without diagnostic output. No crash
cause or PHP result is claimed. The generator preserves and skips recorded
failures rather than retrying them automatically. Their semantics and eventual
native/eval behavior remain unverified.

An independent two-case PHP oracle exposed an ordering defect in the initial
coordinator: a destructor during registration can change display_errors before
PHP decides whether to emit a nesting warning. The coordinator now reads the
separate diagnostic configuration phase after the host write. The regression
consumes parse_str_display.json, generated by capture_parse_str_display.php.
Its host simulates the completed write's core-setting change; this is not yet
an emitted native destructor test.

The complete focused invoke binary passes 24 tests with ten unrelated optional
regex tests ignored. All seven capture-coordinator regressions also pass with
the managed Oniguruma provider, including the relevant ignored tests. Forty-two
neutral contract tests, one AOT registry join test, and thirteen eval registry
tests pass. Bridge/compiler/exporter builds are warning-free. No full local
repository Rust suite was run and no new target-specific assembly was emitted.

The generated-docs workflow passes: 391 exceptional routes with zero audit
errors, 2,058 pages passing site checks, the EIR boundary audit, and eleven Python
pipeline tests. New docs explicitly mark mb_parse_str unavailable in both public
backends. Unsupported internals pages now say Signature constraints instead of
claiming active type-checker enforcement. The registry and 384 generated paths
changed. Sixteen touched Rust preambles, 149 explicit function docblocks, PHP
syntax, and git diff --check pass.

Logs are /tmp/mbstring-query-host-verified-*.log,
/tmp/mbstring-query-host-docs-*.log, and initial layout/ID/check logs with the
same prefix. Final source/fixture hashes, Rust hygiene, generated-doc hashes and
changed paths, and refreshed PR metadata are recorded in the corresponding
/tmp/mbstring-query-host-{verified-files,rust-hygiene,docs-before,docs-after,docs-changed,pr-ledger}.json
files. All process sessions are terminal, including 9055. No commit, PR write,
superseding comment, or closure was made. All five tracked PRs remain open with
the same heads as the previous ledger.

### Next native/eval integration

Implement query_configuration, query_filter, and query_register in the emitted
runtime using the V5 contract and shared registration plans. Preserve live root
writes without ordinary root COW, while nested Enter uses PHP's separation rules.
Do not replace entire graphs or rerun name normalization in machine-code adapters.
Configuration must read actual core settings, including live display policy after
destructors. Core INI/startup routing for parser limits and separators remains
missing. The existing mbstring State continues to own all encoding settings and
aggregate identification; do not add a second interpreter-local copy.

Reuse the capture reference initialization/cleanup infrastructure while removing
any inappropriate Oniguruma dependency for non-regex queries. Adapt output index
one and one input string throughout shared argument planning, checker semantics,
EIR lowering, native calls, and eval binding. Existing capture_check and
is_capture_output_argument are specifically shaped for two regex inputs and
output index two; do not reuse them unchanged. RuntimeFnId and PHP homes are not
added yet. New native callbacks need all five target paths, exact ABI argument
placement, ownership/exception coverage, and actual clang/emitter tests.

Then add public direct/named/dynamic/reference/error tests and an example, update
the support records only when those bindings work, and regenerate builtin docs.
All earlier AOT lvalue, shutdown ordering, exception-trace ownership, strictness,
shared-indexed/COW, callable/resource, and public INI debts remain outstanding.

## Persistent native array indices for query registration, 2026-09-10

Verified at 2026-09-09 22:24 UTC (Europe/Rome local date 2026-09-10),
HEAD `217ff6caad7e0965688c54b41d2410a8f5e92dec`. This is a prerequisite for
native query registration, not a completed public `mb_parse_str` binding.

### Storage and append behavior

The native hash header now has 64 bytes. `NEXT_INDEX_OFFSET = 56` stores the
signed next automatic integer key; `i64::MIN` is the initial no-integer sentinel.
Both ordinary and owned insertion record newly inserted integer keys, preserve
negative successors, and saturate at `PHP_INT_MAX`. Unset keeps the counter.
Stable growth keeps original metadata, and shallow/COW cloning copies the exact
counter even when the largest key has been deleted. Zend zval packing/unpacking
preserves the same value through `nNextFreeElement`.

`src/codegen_support/runtime/arrays/hash_next_index.rs` emits:

- `__rt_hash_try_next_index`: borrowed C-ABI hash input; returns index/available in
  x0/x1 or rax/rdx. Initial sentinel maps to zero. Only saturation needs a lookup.
  An occupied maximum returns unavailable without throwing, separating, or
  changing the table. Query registration can stop a failed append before later
  RemoveRoot instructions.
- `__rt_hash_next_index`: checked ordinary-append wrapper raising PHP Error on
  exhaustion. Typed EIR append calls it before acquiring a stored value owner.
- The owned Mixed append path uses the same probe, consumes an uninserted value
  on failure, and preserves the existing maximum entry.

The previous capacity scans in both EIR hash append and the native append helper
are removed. EIR HashAppend and MixedArrayAppend effects include index reads,
allocation, and MAY_THROW. Existing AST statement effects already included
catchable failure.

### Eval integration and cleanup

Magician no longer computes statement append keys by iterating live entries.
RuntimeValueOps now asks its host for the persistent index and has an explicit
history-copy operation for a freshly rebuilt associative array. Generated C
callbacks live in `runtime/eval_bridge/array_next_index.rs`; they do not unwind
through Rust or execute PHP. Exact metadata copies preserve the initial sentinel
as well as saturation. The fake host records independent insertion history and
preserves it on value copies.

Variable, instance-property, and static-property append share
`interpreter/statements/array_append.rs`. They evaluate an owned RHS before index
selection, retain PHP-visible side effects on exhaustion, and retire temporary
keys/values on success or failure. Eval unset reconstructs a hash with preserved
keys, including dense trailing holes, then copies the original index history.
Iteration keys, comparison results, retained values, and normalized unset keys
have balanced owners.

The new heap checks also exposed existing leaks in owned array-read index
expressions and Error constructor arguments. Owned reads now release their key
while preserving the original array identity used by reference lookup; Error
construction releases its message/code temporaries and an unpublished object
after failure. This does not claim complete ownership coverage for other
expression or callback paths.

### Terminal verification

- `cargo build -p elephc`: warning-free.
- Five `arrays::append_history` codegen tests pass, including dynamic eval,
  positive/negative deleted-key history, COW, dense-tail unset, RHS execution
  before exhaustion, zval round trips, and catchable maximum-key Error.
  Four heap-instrumented tests report a clean heap.
- The same five tests pass with `ELEPHC_IR_OPT=off ELEPHC_REGALLOC=stack`.
- Independent native hash fixture passes: signed boundary keys, both insertion
  APIs, deleted-key history, growth, COW, nonthrowing exhaustion, checked Error,
  and release of every uninserted ownership tag.
- Clang assembles all changed hash, zval, and eval metadata callbacks for
  linux-x86_64, linux-aarch64, macos-aarch64, ios-arm64, and ios-sim-arm64.
  Only host Linux x86_64 execution is claimed.
- Two native capture storage/initialization regressions and two generated
  native capture-invocation regressions pass.
- One EIR effect regression, 21 Magician native-scope tests, and two Magician
  array-reference tests pass.
- Four PHP-compatible fixture programs match local PHP 8.5.10. The zval extension
  fixture has no reference-PHP equivalent.
- 35 touched Rust files have module preambles and all 421 explicit functions
  checked have docblocks; assembly comments align and `git diff --check` passes.
- No full repository suite, commits, external comments, or PR changes.

Logs and manifests are `/tmp/mbstring-query-indices-*`.
`/tmp/mbstring-query-indices-verified-files.json` pins 37 implementation/test/doc
files. Its final comparison reports no changes after validation.
The previous 24-file `mbstring-query-host-verified-files.json` manifest is also
unchanged. No contract, support record, builtin registry, or generated builtin
page changed in this checkpoint.

### Confirmed additional ownership debt

An earlier variant built the same eval source with an unconditional `.=`.
With current code it produces the correct PHP output but leaves two heap blocks,
126 bytes. The final append regression uses one source literal and retains the
same array mutation checks, plus extra COW/dense/RHS coverage. The failing variant
is retained at
`.plans/mbstring-probes/test_mbstring_eval_append_compound_source_ownership.php`;
`/tmp/mbstring-query-indices-compound-probe.json` records the successful compile,
exit zero, exact output, and leak report. The cause has not been established.
Do not report all eval/source-construction ownership as fixed.

### Public coverage and next work

Coverage remains 61 neutral mbstring contracts/operations, 60 AOT homes, 60 eval
homes, and nine constants. `mb_parse_str` remains ReferenceAdaptersPending for
both public backends. Missing public homes remain `mb_parse_str`,
`mb_convert_variables`, `mb_ereg_replace_callback`, `mb_output_handler`, and
`mb_send_mail`.

The next implementation step is still the real V5 query register callback:
resolve the live root, consume shared Enter/Store/RemoveRoot instructions,
use the new native append probe, preserve root aliases without ordinary root
COW, separate nested arrays, and contain destructor exceptions while completing
PHP-visible mutations. Do not use snapshot graph replacement or repeat PHP name
normalization in native code. Core parser configuration, public argument/output
adapters, runtime IDs/homes, examples, support records, and builtin docs remain
to be completed as described in the previous checkpoint.

PRs #895, #898, #899, #900, and #902 were refreshed and remain open at the same
heads recorded in the previous checkpoint. The fresh ledger is
`/tmp/mbstring-query-indices-pr-ledger.json`. Superseding/closing comments remain
deferred until the complete feature is finished. All earlier ownership, lvalue,
strictness, shutdown, and core INI debts remain outstanding.

## Native query root removal, 2026-09-10

Verified at 2026-09-09 22:52 UTC, on unchanged HEAD
`217ff6caad7e0965688c54b41d2410a8f5e92dec`.

- [x] Complete protected native removal of a selected query root entry.
- [x] Verify independent allocator ownership and all supported assembly targets.
- [x] Verify real PHP destructor reentry, nested exceptions, and heap cleanup.
- [ ] Connect removal to the complete V5 native query registration callback.
- [ ] Complete nested Enter/Store behavior, configuration, and public adapters.

`src/codegen_support/runtime/strings/mbstring/query_remove.rs` emits
`__rt_mbstring_query_hash_remove`, a C3 context/hash/normalized-key helper.
The caller supplies a valid hash and an integer or string key descriptor whose
bytes remain borrowed through the callback. A lifetime pin preserves the chosen
root without separating construction aliases. Lookup and a write-guard claim
precede detachment of the key/value owners. The helper repairs both insertion
links, publishes a tombstone, and decrements live count before releasing either
owner. Automatic-index history is unchanged. Guard claims prevent double
release when another construction callback already owns the dying value.

String keys, refcounted values, callable descriptors, and the final root pin are
released through the real protected cleanup boundary. Status two reports a
pending throwable after completing cleanup. No entry address is accessed after
destructors can grow, replace, or retarget storage. A destructor insertion of the
removed key therefore survives the outer removal.

The independent C fixture reuses the capture allocator and native adapters.
Its hash declaration now includes the persistent next-index word; its release
model handles nested arrays and a test-specific destructor override. The
original capture fixture also passes with these shared fixture changes.

### Terminal verification

- Independent native removal fixture passes 576 combinations of binary/integer
  keys, head/middle/tail removal, root aliases, raw objects, Mixed boxes, nested
  hashes, reinsertions, growth, snapshots, retargeting, and modeled pending
  cleanup. Additional cases cover deletion inside an active capture release,
  absent keys, scalars, and string ownership. Every case retires all allocations.
- Clang assembles the removal helper for linux-x86_64, linux-aarch64,
  macos-aarch64, ios-arm64, and ios-sim-arm64. Only Linux x86_64 execution is
  claimed in this checkpoint.
- `test_mbstring_query_remove_native_destructor_boundary` passes with actual
  compiled PHP destructors and exception handling: objects, callable captures,
  nested two-object cleanup, raw/preboxed entries, destructor reinsertion,
  insertion order, pending exception chaining, and a clean native heap.
  All six compiled fixture variants pass both normally and with
  `ELEPHC_IR_OPT=off ELEPHC_REGALLOC=stack`.
- Three reference PHP 8.5.10 programs using equivalent `unset` and destructor
  actions exactly match the complete expected native traces, including the
  two-exception previous chain. These oracles validate removal semantics, not
  the still-unwired public `mb_parse_str` adapter.
- The independent capture-store fixture, two real capture-storage regressions,
  and two real native capture-invocation regressions pass.
- `cargo build -p elephc` is warning-free. Seven touched Rust files have module
  preambles and all 39 explicit functions checked have docblocks. Assembly
  comments align and `git diff --check` passes.
- No full repository suite, commits, PR mutations, or external comments.

Logs, the PHP oracle records, hygiene results, and the fresh PR ledger are under
`/tmp/mbstring-query-remove-*`. The verified manifest pins ten implementation,
test, and documentation files. The previous 24-file query-host manifest is
unchanged. Of the previous 37-file array-index manifest, only
`docs/internals/memory-model.md` changed, to document this removal primitive.

### Remaining integration

Public coverage is unchanged: 61 neutral contracts/operations, 60 AOT homes,
60 eval homes, and nine constants. `mb_parse_str` still declares
ReferenceAdaptersPending in both backends. The five missing public homes remain
`mb_parse_str`, `mb_convert_variables`, `mb_ereg_replace_callback`,
`mb_output_handler`, and `mb_send_mail`.

The next step is the native V5 registration executor and its nested Enter
operation. Reuse shared normalized instructions and the native automatic-index
probe. Preserve root aliases, apply PHP COW to nested arrays, and distinguish
ordinary internal Mixed boxes from persistent PHP references. PHP's named
Enter replaces a reference-valued entry instead of transparently following it
as an existing array. Do not mutate an ordinary shared Mixed box in place.
Acquire child lifetime protection before releasing a cursor or replacing a
parent value; preserve completed mutations even when cleanup reports pending.
Failed append must stop the field before a planned RemoveRoot, and the host
must report nesting overflow only if that removal was reached.

Core parser configuration, the public output/lvalue adapters, runtime IDs and
homes, examples, support records, and generated builtin docs remain pending.
All earlier ownership, shutdown, strictness, and source-construction debts
remain outstanding. PRs #895, #898, #899, #900, and #902 were refreshed and are
still open at their previously recorded heads. Superseding and closing comments
remain deferred until the complete feature is finished.

## Native nested query entry, 2026-09-10

Verified at 2026-09-09 23:23 UTC, on unchanged HEAD
`217ff6caad7e0965688c54b41d2410a8f5e92dec`. The previous goal turn completed
native root removal and was progress; its ten-file manifest was unchanged at
the start of this continuation.

- [x] Implement protected nested array selection and cursor lifetime transfer.
- [x] Distinguish ordinary Mixed boxes, PHP references, shared arrays, and active release borrows.
- [x] Share guarded owned-array replacement with the capture construction writer.
- [x] Verify native ownership, real PHP callbacks, COW, heap cleanup, and supported assembly targets.
- [ ] Connect these primitives to the full native V5 query registration executor.
- [ ] Complete parser configuration and public AOT/eval output adapters.

`src/codegen_support/runtime/strings/mbstring/query_enter.rs` emits
`__rt_mbstring_query_hash_enter`, a C4 context/parent/normalized-key/output-child
helper. Valid inputs borrow a managed parent and an integer/string key descriptor.
The output receives one child lifetime pin, including when the return status is
two for a pending exception. Callers must retire that pin.

Unique nested hashes retain their identity through unique ordinary Mixed boxes.
Existing lifetime pins are excluded from PHP ownership. Shared children or any
shared ordinary box in the traversal require a shallow child copy, preserving
the source's contents and persistent automatic-index history. Dense arrays are
promoted with retained contents. A persistent PHP reference wrapper is replaced
with an empty array rather than transparently followed as an existing child.

The helper consults `__rt_hash_write_guard_owns` before lookup. An active
construction borrow requires a copy even when the physical owner count still
looks unique. Four explicit borrowed-array cases cover direct/boxed storage and
an additional existing pin. The saved pre-fix assembly fails the new regression
at `child != old`; the fixed implementation passes. Evidence is under
`/tmp/mbstring-query-enter-active-guard-pre-fix/`.

Fresh, cloned, and promoted children transfer one ordinary owner through
`__rt_mbstring_query_hash_store_array`, an explicit owned-array entry into the
existing guarded construction writer. Its descriptor/key bytes remain borrowed;
its tag-five payload owner transfers into the parent. This entry shares the
capture writer's actual implementation rather than duplicating destruction,
relookup, or insertion rules. The child is pinned before replacement can run
destructors, so its returned cursor survives loss of every PHP parent owner.
Parent cleanup is protected and preserves completed mutation plus pending state.

### Terminal verification

- The independent C fixture passes 212 cases: 144 nested-array combinations,
  56 destructive replacements, two nested captures, six missing/scalar entries,
  and four active-release borrows. Coverage includes shared boxes, persistent
  references, direct child sharing, root aliases, preexisting pins, dense
  promotion, history preservation, growth, callback replacement, retargeting,
  pending cleanup, and release of all allocations.
- Clang assembles the new helper for linux-x86_64, linux-aarch64,
  macos-aarch64, ios-arm64, and ios-sim-arm64. Only host Linux x86_64 execution
  is claimed in this checkpoint.
- Two real runtime-GC codegen tests pass: object/callable destruction with
  raw/preboxed entries, reentrant replacement and pending exceptions; plus
  direct child COW, ordinary-box sharing after root COW, and dense promotion.
  All five compiled fixture variants have clean heaps and also pass with
  `ELEPHC_IR_OPT=off ELEPHC_REGALLOC=stack`.
- The independent capture fixture, two removal library tests, two capture
  codegen regressions, and the real removal/destructor codegen regression pass.
- `cargo build -p elephc` is warning-free. Six touched Rust files have module
  preambles and all 35 explicit functions checked have docblocks. Assembly
  comments align and `git diff --check` passes.
- No full repository suite, commits, external comments, or PR mutations.

Logs and metadata are `/tmp/mbstring-query-enter-*`; the verified manifest pins
eight implementation/test/doc files. All remain unchanged after validation.
The previous 24-file query-host manifest is unchanged. The array-index
manifest differs only in the updated memory-model documentation. The removal
manifest differs only in the shared emitter, owned-array alias, test-module
registration, and memory-model documentation.

### Reference evidence and limitations

Three completed PHP 8.5.10 workers provide matching bounded evidence: ordinary
object cleanup, ordinary callable cleanup, and arrays seeded through a fresh
mutable parent. The latter includes direct child sharing, a copied parent,
persistent references, and dense arrays. The AOT test covers the three currently
expressible array cases; literal `["key" => &$referenced]` syntax is rejected by
the existing frontend. Native reference storage itself is covered by the
independent C fixture. Do not claim public AOT reference-literal support.

Two broader reference workers that reentered a destination write from an old
destructor terminated with signals: the object worker returned -6 and reported
`zend_mm_heap corrupted`; the callable worker returned -11. Their partial
traces are not semantic oracles and must not be rerun automatically. These are
new observations, separate from the earlier excluded parser-limit workers.

A third reference worker completed but did not match the repeated expected
snapshot trace: with a literal-seeded parent, the first pass preserves the old
snapshot and later passes show `captured` in that snapshot. The cause is not
established. Building the parent using a temporary child variable produces a
matching complete trace. Retain both observations for the public V5 integration
audit; do not silently classify the original completed worker as a match or
claim universal PHP reentry parity. Exact statuses and traces are recorded in
`/tmp/mbstring-query-enter-php-oracles.json`.

### Next implementation step

Implement the actual V5 register callback using shared Enter/Store/RemoveRoot
instructions. Resolve the live root for each field; use the native append probe;
preserve the selected root and nested cursor with lifetime pins; retire every
cursor before returning. Failed append must stop before later planned removal,
and nesting metadata changes only when RemoveRoot is actually reached. Continue
completed mutations after pending status while preserving the real exception
chain. Do not replace a whole output snapshot or repeat PHP name normalization.

Public coverage remains 61 neutral contracts/operations, 60 AOT homes, 60 eval
homes, and nine constants. `mb_parse_str` remains ReferenceAdaptersPending in
both backends. The missing public functions are still `mb_parse_str`,
`mb_convert_variables`, `mb_ereg_replace_callback`, `mb_output_handler`, and
`mb_send_mail`. Configuration, caller lvalues, public bindings, examples,
generated docs, and all earlier ownership/strictness/shutdown debts remain open.

PRs #895, #898, #899, #900, and #902 were refreshed and remain open at the same
heads. A fresh open-PR title inventory returned ten entries and the same five
mbstring candidates. Both the head ledger and discovery metadata are saved
under the current checkpoint prefix. Superseding/closing comments remain
deferred until the complete feature is finished.

## Shared query execution and native V5 registration, 2026-09-10

Progress checkpoint, verified around 00:08 UTC at unchanged HEAD
`217ff6caad7e0965688c54b41d2410a8f5e92dec`. The full mbstring goal remains active;
this completes an internal registration boundary, not public mb_parse_str.

### Implementation

The neutral `MbQueryStorageV1` ABI defines six required callbacks: acquire the
writer's live root, probe the next append index, enter a nested array, store a
borrowed string, remove a root key, and release a cursor. Its reviewed 64-bit
layout is 56 bytes; the append-result record is 16 bytes. Cursor publication is
independent of callback status, and every published pin must be released.

`elephc_mbstring_query_apply_v1` executes the shared Enter/Store/RemoveRoot plan
without a request-state borrow or repeated PHP name parsing. It validates the
transport and snapshots callback pointers before invoking native storage. Each
new child pin transfers before the previous cursor is released. Append
exhaustion stops the field before any later planned removal. Terminal removal
first releases the nested cursor, then resolves the writer's current root again;
this preserves retargeting during cleanup and does not postpone root-entry
destruction through a stale child pin. If that late root has become non-array,
removal is a no-op with nesting metadata set. Initial non-array roots ignore
the field with zero metadata. This is safe handling of a late invalid root,
not a claim of PHP behavior for unsafe reference-worker states.

Pending callback statuses retain completed mutations and continue the field;
fatal statuses stop it. All current/spare pins are retired after success,
failure, or contained Rust panic. Earlier pending state remains authoritative
over a later fatal status, matching the shared host protocol.

`__rt_mbstring_query_register` is a target-aware C7-to-C8 adapter supplying the
native table to the Rust executor. On x86_64 it forwards the seventh incoming
argument from the caller stack and supplies the eighth table argument without
clobbering the six register arguments. Its root callback resolves the caller's
persistent reference, ignores non-arrays, promotes unique indexed values using
the existing destination helper, and acquires one hash pin. Shared indexed
roots still fail without mutation. Append uses the existing history probe;
cursor release uses protected native cleanup. Nested entry, store, and removal
reuse the already verified native helpers.

### Input ownership correction

An end-to-end fixture exposed a separate compiler bug before query storage ran:
an array literal containing an object/closure ternary received an integer value
stamp even though branch lowering produced a boxed Mixed value. Generated code
cast that box to an integer and lost its owner. The original query fixture left
80 blocks/4096 bytes; an ordinary PHP-only unset probe left 10 blocks/512 bytes.
Both indexed and associative literal inference now use the same materialized
ternary result type as branch lowering. No homogeneous-callable stamp or generic
array/hash union conversion was changed by this correction.

A permanent PHP fixture under the runtime-GC test directory checks both literal
representations without any native writer shim. Its destructor trace matches
PHP 8.5.10 and its native heap is clean. The original object/closure query test
also passes unchanged, including throwing destructors and completed field writes.

The broader ordinary-unset probe exposed an additional existing cleanup gap:
after a destructor throws, native unset leaves count one and two key allocations
(58 bytes over the two throwing cases). This is independent of the protected
query removal helper. Preserve the observation in
`/tmp/mbstring-query-register-unset-pending-observation.json` and the source at
`.plans/mbstring-probes/test_mbstring_query_ternary_value_ownership.php`.
The permanent input-ownership test isolates nonthrowing unset; the native writer
tests separately require throwing destruction to complete with clean heaps.
Do not count general throwing-unset behavior or older callable/union debts as fixed.

### Terminal verification

- Seven shared-executor unit tests and the new ABI layout test pass.
- Five runtime-GC tests pass, covering two ordinary input representations,
  nested COW and negative append history, append exhaustion before removal,
  object/closure pending destruction, real caller-reference retargeting with
  chained exceptions, non-array roots, unique indexed promotion, and explicit
  unchanged failure for shared indexed roots. All six compiled fixture variants
  have clean heaps and also pass with `ELEPHC_IR_OPT=off ELEPHC_REGALLOC=stack`.
- All 62 focused ternary codegen regressions pass.
- Clang assembles the native registration adapter and storage callbacks for
  linux-x86_64, linux-aarch64, macos-aarch64, ios-arm64, and ios-sim-arm64.
  Executable evidence in this checkpoint is Linux x86_64 only.
- `cargo build -p elephc` passes without warnings. The complete
  `update-builtin-docs` workflow passes, including the curl-enabled exporter,
  registry/page regeneration, module/comparison generation, docs audit, site
  validation, and enforced target-architecture builtin boundary audit. All 2089
  tracked generated-document hashes match the pre-generation snapshot.
- Fourteen touched Rust files and 108 explicit functions have the required
  preambles/docblocks. Assembly comments and `git diff --check` pass.

Evidence and the 17-file implementation/test/document manifest use the
`/tmp/mbstring-query-register-*` prefix. The preceding entry-helper manifest
differs only in the shared emitter, test-module registration, and memory-model
documentation. The actual nested-entry, capture-store, and root-removal helpers
were not changed in this checkpoint. No full suite, commits, PR writes, external
messages, or subagents were used.

### Remaining work and PR ledger

Continue with native V5 configuration and public output-reference adaptation,
including a complete identity-preserving solution for shared indexed roots.
Wire the actual native query host through the existing initialization/release
protocol, then add the AOT/eval public mb_parse_str bindings and observable
configuration/error behavior. Do not enable a public success path that hides
the unsupported root representation. Retain all earlier reference-worker,
ownership, strictness, shutdown-order, and public-lvalue debts.

Public coverage is still 61 neutral contracts/operations, 60 AOT homes, 60 eval
homes, and nine constants. The five missing public functions remain
mb_parse_str, mb_convert_variables, mb_ereg_replace_callback, mb_output_handler,
and mb_send_mail. This goal is neither complete nor blocked.

PRs #895, #898, #899, #900, and #902 were refreshed and remain open at unchanged
heads. A full open-PR title inventory again returned ten entries and the same
five mbstring candidates. The current head ledger and discovery results are
saved with this checkpoint. Superseding/closing comments remain deferred until
the entire requested feature is complete.

## Native query V5 invocation, 2026-09-10

The native adapter now invokes the shared mb_parse_str operation through host
V5. MbNativeQueryV1 adds an independent policy context, required live query
configuration, and an optional input filter. The query and regex capture
adapters share initialization, result materialization/release, and deferred
owner adoption, with separate aligned frames. Query configuration and filter
trampolines preserve their C argument conventions on all supported targets.
The query adapter is emitted without requiring regex; regex initialization is
conditional on the provider. A missing configuration callback rejects the call
before argument coercion or output mutation. Native exception propagation occurs
only after the Rust invocation returns.

Four native integration tests cover nine compiled fixtures: Stringable input,
throwing old-output destruction, nested output and embedded NUL, live per-field
configuration, an independent filter context, missing configuration, invalid
arity, array input, and strict integer input. Every fixture has a clean heap in
both default mode and IR-off/stack mode. The full initialization/exception trace
matches PHP 8.5.10, including aggregate HTTP input identification and the unchanged
string-specific selector. Source cells in these C-host fixtures are borrowed
from an outer PHP Mixed frame; public builtin temporary-argument guards remain
unimplemented and are not established by this evidence.

The V4 invocation regressions and persistent-reference pin test pass. Clang
assembles both runtime feature variants on all five supported targets, and the
native query host layout test passes. cargo build is warning-free. The complete
update-builtin-docs workflow passes, including target-architecture enforcement;
all 2089 generated documentation files match the prior snapshot. Nine touched
Rust files and 35 explicit functions satisfy documentation rules. Assembly
comments and git diff --check pass. All thirteen verification-driver checks are
terminal with exit zero. Evidence uses /tmp/mbstring-query-invoke-*; the verified
implementation/test/document manifest contains eleven files.

No public mb_parse_str binding or RuntimeFnId has been enabled. The next work
must provide actual startup/runtime core INI routing and query diagnostics,
public AOT/eval output lvalues and temporary guards, and identity-preserving
shared indexed-root promotion. The native host accepts real configuration and
filter callbacks; it does not substitute a default-only public configuration.
All earlier ownership, strictness, callable, shutdown-order, and public-lvalue
debts remain open. Public coverage is unchanged at 61 contracts/operations,
60 AOT homes, 60 eval homes, and nine constants. This goal remains active.

The five tracked PRs remain open at the same heads: #895 at e96b43f219c0,
#898 at b15d629eb1d1, #899 at 089ffc6bee2d, #900 at f402f762e093,
and #902 at 405f77283dd4. The refreshed complete open-PR inventory contains
the same ten entries. No commits, PR writes, external messages, or subagents
were used. Superseding and closing comments remain deferred until completion.

## Public startup INI configuration and shared directive catalog, 2026-09-10

Explicit --ini overrides now initialize the real shared mbstring engine for
CLI programs and reused web workers, including opaque eval. The compiler carries
three effective core encodings and raw mbstring pairs through Module metadata;
the descriptors and initializer belong to user assembly, outside cached runtime
objects. Core encoding inheritance uses the final nonempty individual encoding,
then default_charset, then UTF-8, with PHP C-string boundaries. The bridge still
owns registration order, validation, duplicate directive semantics, and request
state. PHP reference runs quote raw INI values so words such as none survive
PHP's separate INI scanner; Elephc --ini itself preserves raw values.

The eleven directive names, defaults, access masks, stable indices, and handler
order now live once in the neutral mbstring ABI catalog. The engine re-exports
that catalog, and compiler selection uses its exact case-sensitive lookup.
Unknown mbstring keys do not trigger initialization or a new native dependency.
Programs without mbstring/eval likewise ignore unused overrides.

The program initializer registers the real managed PCRE2 MIME callbacks, calls
elephc_mbstring_configure_v1, and releases its result before returning or failing.
It runs before the existing request reset. Identical configuration installation
is idempotent; each request inherits the validated prototype rather than the
preceding request's live mutations. Startup diagnostics use a non-PHP stderr
callback. PCRE2 is a separate native requirement and does not enable preg_* in
opaque eval. Production linking still has no system fallback. Library output
with relevant startup overrides currently fails with an explicit compiler
diagnostic, covered by a test and CLI documentation. It must receive recoverable
host initialization before this startup feature is complete for library/iOS use.

### Ownership regression and remaining observations

The capability test exposed a direct eval function_exists argument leak:
eval_expr could allocate a temporary that the probe never released. The direct
adapter now acquires an independent argument owner and releases it on success
or failure, retiring a produced result if cleanup fails. Materialized argument
hooks continue to borrow their caller's values. A repeated 24-call regression
checks literals, an existing caller-owned name, missing functions, and the
original name after every probe, with a clean native heap in both compiler modes.

The initial broader startup fixture also reproduced unrelated existing eval
ownership issues. Without startup overrides it retained the same 17 blocks /
637 bytes. Isolating scalar getters and mb_detect_encoding gives clean heaps;
the composed mb_detect_order/implode array path remains unresolved, including
a function-local variant retaining nine blocks / 312 bytes in the observed
fixture. A plain eight-iteration eval for-loop, with no function_exists call,
independently retains 26 blocks / 1040 bytes. Permanent passing startup tests
exercise the configured array in AOT and scalar detection in eval. Neither
unresolved ownership path is claimed fixed. Observations are saved under
/tmp/mbstring-startup-eval-*; follow-up probe sources are in .plans/mbstring-probes.

### Terminal verification and next work

Six public CLI integration tests pass in default mode and with IR optimization
off plus stack allocation. They compile nine executable fixtures, seven with
explicit clean-heap assertions, and check the expected library diagnostic.
The new example counts Cafe with an accented e as four UTF-8 characters or five
bytes under --ini default_charset=8bit. The native/eval settings trace matches
PHP 8.5.10 exactly. Clang assembles the initializer and its data for all five
supported targets. Two startup-selection unit tests, six shared INI engine
tests, and the existing eval function-probe unit test pass. Both web regressions
pass after twelve requests each against one reused worker. Their local TCP bind
required a sandbox escalation, which automatic review approved; no user action
remains pending.

The final build and curl-enabled docs exporter are warning-free. The complete
update-builtin-docs workflow passes; all 2089 generated-document hashes are
unchanged. Seventeen touched Rust files and 188 explicit functions meet the
preamble/docblock policy. Assembly comments, example PHP syntax, and diff
whitespace checks pass. Evidence and the 22-file implementation/test/document
manifest use /tmp/mbstring-startup-*. The preceding eleven-file V5 invocation
manifest is unchanged. No full suite, commits, external messages, PR writes,
or subagents were used.

Next, replace the configured-library diagnostic with recoverable initialization
through library lifecycle and lazy export entry, preserving host arguments,
request mutations between calls, thread-local defaults, and host error status.
Continue the actual query core-INI provider (separators, limits, display_errors),
public ini_get/set/restore/get_all routing, and AOT/eval mb_parse_str output
lvalues and argument guards. Shared indexed-root promotion, the other four
missing mbstring functions, and all earlier tracked ownership/integration debts
remain open. Public function counts are unchanged: 61 contracts/operations,
60 AOT homes, 60 eval homes, and nine constants. The full goal remains active.

The fresh open-PR inventory still contains ten entries and the same five
mbstring candidates. Heads remain #895 e96b43f219c0, #898 b15d629eb1d1,
#899 089ffc6bee2d, #900 f402f762e093, and #902 405f77283dd4, all open.
Superseding/closing comments remain deferred until the full feature is complete.

## Recoverable configured-library initialization, 2026-09-10

Configured static and shared libraries now initialize the shared mbstring engine
through elephc_init or lazy exported-function entry. The former compile-time
library rejection is removed. User assembly owns a non-exiting status initializer:
provider validation precedes configuration, every published bridge result is
released, and the status returns to the host boundary. Executables retain their
exit-on-startup-failure policy through a separate framed wrapper. Startup remains
outside cached runtime objects and does not enable opaque eval preg capability.

Scalar and owned-string exports save and validate their C inputs before startup.
They enter boundary bookkeeping, initialize, and only then install the PHP
exception handler. Initialization does not execute PHP callbacks. Failure uses
the existing runtime-failure branch to restore the boundary and concat state,
record ELEPHC_STATUS_RUNTIME_FAILURE, and return zero or null/zero string outputs.
Explicit initialization uses a proper native frame on both architectures, also
correcting x86_64 helper-call alignment. Lifecycle/status/free emission now lives
in its own cohesive module; the export orchestrator stays below 500 lines.

Repeated initialization preserves live PHP mbstring setting changes. Host tests
also check a serialized second thread receives configured defaults and does not
replace the first thread's mutated state. This is not concurrent runtime support:
native boundary, heap, and stack-guard state remain shared, and each participating
thread explicitly initializes its stack guard before its serialized calls.
Shutdown bookkeeping and the broader outstanding cleanup debts are unchanged.

Static archives intentionally contain only user and runtime objects. The first
C fixture link exposed its missing explicit dependencies, not a packaging defect.
The static host now links the matching mbstring bridge and PCRE2 shim/POSIX/core
archives separately, followed by Linux system dependencies. Shared library hosts
link only the generated shared artifact. The CLI/library docs and new library
example explain that distinction; the example defaults to shared output.

### Verification

All 36 tests in the library integration binary pass, including the explicitly
enabled clang cross-assembly test. The configured host covers static/shared
artifacts, three first-entry forms in fresh processes, repeated initialization,
binary string ownership, mixed scalar inputs, stacked arguments/output addresses,
float returns, and serialized thread-local settings. Failure fixtures use the
actual bridge ABI with either an empty configuration argument list or a null
provider, and repeatedly verify init, integer, float, and string recovery while
the host stays alive. No bridge implementation is mocked for those failures.

Complete configured user assemblies assemble for all five supported targets.
Five boundary emitter tests pass, including all-target frame preservation and
configuration-before-PHP-handler ordering. The dedicated startup emitter also
assembles on all five targets. Six CLI startup tests pass in default mode and
again with IR optimization off and stack allocation. The final compiler build is
warning-free. PHP syntax checks and compiler assembly emission of the shipped
library example pass. Ten touched Rust files and 131 explicit functions satisfy
the module/function documentation checks; assembly comments and diff whitespace
checks pass. The preceding V5 query invocation manifest remains unchanged.

Evidence uses /tmp/mbstring-library-*. No builtin contract or home files changed
in this slice, and generated builtin documentation was not regenerated. No full
repository suite, commits, external messages, PR writes, or subagents were used.

### Remaining integration

Public coverage remains 61 contracts/operations, 60 AOT homes, 60 eval homes, and
nine constants. The full goal remains active. Next work is the actual core query
INI provider and public ini_get/set/restore/get_all routing, then mb_parse_str
output lvalues and argument guards. Current INI surfaces are injected PHP
functions in opcache_prelude, web_prelude, and version_prelude, so their dispatch
must share the engine rather than acquire an independent mbstring settings map.
The V5 query callback already requires live configuration for entry, field, and
diagnostic phases. Shared indexed-root promotion, the other four missing public
functions, and all previously recorded ownership/strictness/callable debts remain
open.

The fresh complete open-PR inventory still has ten entries. The five mbstring
heads are unchanged and open: #895 e96b43f219c0, #898 b15d629eb1d1,
#899 089ffc6bee2d, #900 f402f762e093, and #902 405f77283dd4. Superseding and
closing comments remain deferred until the whole mbstring feature is complete.

## Shared Core query INI state and provider, 2026-09-10

The neutral INI contract now declares the four Core directives used by query
parsing: arg_separator.input, max_input_vars, max_input_nesting_level, and
display_errors. Names, defaults, modification masks, storage order, and startup
handler order have one catalog. CoreIni is part of the same State owned by the
bridge's existing REQUEST thread-local value, so there is no separate native/eval
configuration map. Process configuration, new threads, and request reset inherit
the validated prototype. State's standalone reset path preserves the configured
Core defaults as well.

Compiler startup selection forwards exact known Core query pairs with the
existing three effective encodings and mbstring pairs. Validation keeps raw
strings distinct from parsed values: quantities reuse the existing Zend parser;
negative limits and empty separators retain their defaults; duplicate settings
use the final override; separators keep their raw bytes but query parsing sees
the C-string prefix. display_errors preserves PHP's stdout/stderr distinction
and unsigned-byte integer conversion, including 256 becoming disabled.
Core get/set/restore/get-all retain the existing INI string ownership contract,
including scalar short-string interning and raw array-result identities.

The new elephc_mbstring_core_ini_v1 C ABI exposes these operations, and
elephc_mbstring_query_configuration_v1 supplies the actual V5 configuration
callback. It reads the current request on each phase, never calls PHP, returns
borrowed separator bytes with a null owner, and clears output on invalid phase
metadata. The existing V5 coordinator copies those bytes before another host
callback. display_errors changes made during filtering or destructor-visible
registration therefore remain observable at the diagnostic phase.

### Verification and source evidence

Four new Core state/ABI tests pass. They exercise raw startup fallback, quantity
warnings, access masks, string identities, get-all framing, retained setter
arguments, explicit restore, request reset, new-thread defaults, live provider
reads, and invalid output metadata. The actual configuration test uses the real
repository PCRE2 shim. Its provider builder is shared with the existing native
INI-PCRE2 test rather than duplicated. The five focused engine test binaries
pass 14 tests in total, including prior INI/reentry/identity coverage.

The native query integration host has a Core-policy mode that invokes the actual
provider and changes display_errors through the actual Core ABI during filtering.
An eight-iteration fixture crosses the default nesting limit, reads all three
configuration phases, preserves binary values and output ownership, suppresses
the diagnostic after the live setting change, and ends with a clean native heap.
All five native query tests pass in default mode and with IR optimization off
plus stack allocation. This tests the native parser/coordinator and PHP storage;
it is still a test embedding entry, not the public mb_parse_str binding.

Seven CLI startup tests pass in both compiler modes, including a real Core
quantity warning before PHP execution. Three startup-selection tests pass.
The existing startup assembly test remains green across all five supported
targets. Build and the curl-enabled docs exporter are warning-free. The complete
update-builtin-docs workflow passes, and all 2089 generated-document hashes are
unchanged. Sixteen touched Rust files and 149 explicit functions satisfy the
preamble/docblock checks. Diff whitespace checks pass. Evidence and the
implementation manifest use /tmp/mbstring-core-ini-*.

The observed display modes were cross-checked with eleven independent PHP 8.5.10
processes. Two further startup traces confirm the selected raw/global/local
values, access masks, fallback behavior, and parser limits. Source references
are the [PHP Core handlers](https://github.com/php/php-src/blob/php-8.5.10/main/main.c)
and [Zend INI handlers](https://github.com/php/php-src/blob/php-8.5.10/Zend/zend_ini.c).
The initial oracle accidentally requested the case-sensitive module name Core;
that observation was rejected and replaced with verified lowercase core results.

### Next integration boundary

Public ini_get/set/restore/get_all dispatch is still pending, as are native/eval
materialization of RESULT_INI_STRING and RESULT_INI_ARRAY and their preserved
native identities. Existing scalar mbstring materialization must not treat those
new result kinds as ordinary strings or ignore the INI graph identity trailer.
The public preludes still need routing to these shared Core/mbstring APIs and a
combined sorted get-all result. Core encoding raw-value routing also remains
separate from this query-settings work. The current provider can be installed
directly in MbNativeQueryV1.configuration once public argument/lvalue lowering
is ready. No public function count was increased by this infrastructure slice.

Shared indexed-root promotion, query temporary guards, variadic reference and
callable adapters, shutdown order, and earlier ownership debts remain open.
Public coverage remains 61 contracts/operations, 60 AOT homes, 60 eval homes, and
nine constants. The full goal remains active. No full repository suite, commits,
PR writes, external messages, or subagents were used.

The refreshed open-PR inventory still contains the same ten numbers. All five
tracked mbstring heads remain open and unchanged: #895 e96b43f219c0,
#898 b15d629eb1d1, #899 089ffc6bee2d, #900 f402f762e093, #902 405f77283dd4.
Superseding and closing comments remain deferred until the entire feature is
complete.

## Native INI result materialization, 2026-09-10

### Completed boundary

The native result adapter now consumes RESULT_INI_STRING and RESULT_INI_ARRAY.
Scalar results acquire an INI identity lease on the new native string before the
wire result is released. Native return kinds normalize to ordinary string or
indexed/associative array ownership, so the existing Mixed boxer preserves the
identity through subsequent string copies. The original wire kind remains intact
for its allocator's buffer and lease cleanup. A rejected scalar identity releases
the partial native allocation and returns RuntimeFatal without exiting PHP.

The new versioned MbIniArrayBuildV1 callback adds one borrowed identity vector to
the existing array construction contract. Each string value has a nonzero token;
other values have zero, and keys retain their existing byte/integer semantics.
elephc_mbstring_restore_ini_v1 shares the ordinary restorer's postorder traversal,
descriptor construction, layout selection, and ownership arena. It validates
the complete identity trailer and checks that each live lease contains the exact
string bytes before any host allocation. Original graph indices remain intact,
including a nonzero root and unreachable nodes, so compaction cannot silently
assign a token to the wrong string. The original ordinary restore ABI is unchanged.

Native hash and indexed-Mixed builders retain those identities on the copied
string allocations before returning. The same builders still construct ordinary
graphs with no identity vector. Completed child aliases remain shared, all
construction owns its inputs independently, and partial construction releases
the arena on failure. Both AArch64 and x86_64 use the same six-argument C contract.

### Verification

The bridge restore binary passes six tests, including three new INI tests for
original node indices, binary/empty strings, equal bytes with different identities,
shared child ownership, every builder-failure position, malformed trailers,
unleased tokens, byte mismatches, and final lease retirement.

The new native integration test runs eight result modes for eight iterations in
both default compilation and IR-off/stack-allocation modes. It materializes fresh
binary, empty, and one-byte strings; actual Core get-all results with and without
details; actual mbstring detail rows; and a separately framed indexed INI root.
It verifies identities after wire release, result boxing, PHP aliases, explicit
string casts, and unset. A rejected scalar token exercises materialization failure
cleanup. Both executions finish with a clean native heap, and the host confirms
that final PHP release removes all result identity leases.

Five focused compiler/contract unit tests pass, including the existing identity
hook execution test and a new clang check of the complete mbstring adapters on
all five supported targets in direct and PIC modes. Seven existing executable
tests pass for conversion arrays, COW, conversion ownership, mb_get_info, and
encoding-catalog ownership. Build and the curl-enabled docs exporter are warning
free. All twelve recorded validation commands pass, including the complete
update-builtin-docs workflow; all 2089 documentation artifact hashes are unchanged.
Assembly comments and diff whitespace checks pass. Twelve touched Rust files and
82 explicit functions satisfy the module-preamble and function-docblock checks.
The thirteen-file implementation manifest and command logs use
/tmp/mbstring-ini-materialize-*.

### Remaining integration

This completes native materialization of INI result ownership, not public INI
dispatch. The next boundary is a protected INI invocation adapter using the
existing diagnostic callback and preserving native input string identities.
The CLI/web preludes still need routing for ini_get, ini_set, ini_restore, and
ini_get_all, including combined sorted results, existing session/opcache behavior,
extension filtering, and declaration/callable guards. Core encoding raw-value
routing and eval input/return adapters also remain pending.

Public mbstring coverage is unchanged: 61 contracts/operations, 60 AOT homes,
60 eval homes, and nine constants. The five missing public functions, reference
adapters, query temporary guards, shared indexed-root promotion, and the previously
recorded ownership/shutdown debts remain open. The overall goal remains active.
No full repository suite, commits, external messages, PR writes, or subagents were
used for this boundary.

The five tracked PRs remain open at unchanged heads: #895 e96b43f219c0,
#898 b15d629eb1d1, #899 089ffc6bee2d, #900 f402f762e093, #902 405f77283dd4.
Their refreshed metadata is recorded in /tmp/mbstring-ini-materialize-pr-ledger.json.
Superseding and closing comments remain deferred until the complete feature ships.

## Public INI routing and remaining eval argument ownership, 2026-09-10

### Implemented native boundary

The neutral internal contract `__elephc_shared_ini` now owns four arguments:
operation, option, value, and details. RuntimeBuiltinId::SharedIni is 83, and
RuntimeFnId::SharedIni uses the existing target-aware mbstring invocation path.
Both compiler and Magician have one internal binding. The support catalog explicitly
allows this internal operation through the shared eval runtime ABI.

The protected coordinator prepares all arguments before routing GET, SET, RESTORE,
or GET_ALL through the existing Core/mbstring INI C APIs. A Completed wire-result
guard holds buffers and identity leases until host cleanup succeeds. Failed cleanup
discards the completed result and its leases without executing PHP. Ordinary
mbstring outcomes use the same guard.

Only the SharedIni native callback table uses the new INI-aware value classifier.
It delegates ordinary type classification, resolves the retained native string
origin, and adds INPUT_INI_IDENTITY metadata. The coordinator validates a live
identity against the exact bytes and carries an IniString into the setter.
Other mbstring operations keep their ordinary classification path. Both AArch64
and x86_64 use this same metadata and ownership contract.

CLI and web ini_get/ini_set now route neutral-catalog Core and mbstring directives
before the existing opcache/session fallback. The version prelude's ini_restore
uses the same routing and preserves its declaration guard. A shared AST builder
owns ini_get_all filtering, legacy-module diagnostics, and sorted combined rows.
The explicit lowercase `mbstring` filter selects its eleven directives. PHP's
lowercase `core` filter behaves like null and returns the unfiltered surface:
four currently modeled Core settings, eleven mbstring settings, and 54 opcache
settings for the default CLI profile. Web enumeration also includes session rows.

ini_set's neutral parameter contract now permits string, int, float, bool, and
null, with string|false returns. ini_get_all declares array|false. The synthetic
PHP printer now renders Nullable(Union(...)) as `...|null`, preserving valid PHP
syntax and the full parameter set in prelude signature audits.

### Ownership regressions exposed by enumeration

The loop-storage adapter previously loaded a local before determining that its
recorded contract could not be applied. A concrete hash read from Mixed frame
storage acquires an owner, so the unused fallback load kept a prior COW generation
alive. Source loads now occur only in branches that actually convert the value.
A dedicated foreach regression covers populated and empty sources with a clean
heap, and all 71 selected foreach tests pass.

The opcache nullable detail helper now returns its cast directly instead of storing
an unnecessary temporary local. Repeated inlined executions of the old helper
retained prior local string values. This removes that temporary from the INI path;
general ownership of repeatedly overwritten inlined callee locals remains a
separate compiler concern, not a claim that the inliner has been repaired here.

### Verification and artifacts

- Native mutation/restore and enumeration tests pass with a clean heap, both with
  default optimization and with ELEPHC_IR_OPT=off/ELEPHC_REGALLOC=stack.
- The invocation ABI tests pass: 26 ordinary tests and ten regex tests enabled with
  the existing managed Oniguruma 6.9.10 test prefix. New INI cases verify state and
  enumeration plus 48 completed-result cleanup failures with exact lease retirement.
- All 43 neutral-contract tests pass after adding the internal operation to count
  gates. The AOT runtime-ID join and prelude signature parity checks pass. The
  earlier nullable-union parity failure was fixed by the synthetic PHP printer.
- All 18 opcache INI tests pass across the main run and the corrected first-key
  expectation rerun. The first sorted directive is now arg_separator.input.
- Seven startup tests pass, including the updated public INI example and existing
  link-scope/configuration checks.
- The web startup/reset test passes across twelve requests in one worker and checks
  public INI mutation, combined session/mbstring enumeration, and restoration. Its
  initial sandbox run compiled successfully but could not bind a socket; the scoped
  loopback test then passed with escalation. No approval rejection remains.
- The complete native adapters cross-assemble on all five supported targets, in
  direct and PIC modes. This is assembler coverage, not execution on every target.
- The curl-enabled exporter, all builtin-doc generators and audits, four extraction
  unit tests, assembly-comment alignment, and git diff whitespace checks pass.

The docs extractor now locates ini_get_all in shared_ini_prelude.rs. Regeneration
changes 29 artifacts relative to the preceding checkpoint, including the new
internal helper page, corrected public signatures, and affected source anchors.
The final exporter rebuild reproduces those artifacts without further changes.
All 34 tracked Rust source files retain their module preambles.

Evidence lives under /tmp/mbstring-public-ini-*. The source manifest records 37
files; docs before/after manifests record generated changes. Checks JSON/logs retain
historical failures, including count/first-key failures corrected by later runs.
The prelude parity check's final successful run is tool session 34206.

### Open eval ownership boundary

`test_mbstring_public_ini_native_and_eval` is deliberately still failing its clean
heap assertion. Its output is correct:
`Japanese:111:Japanese:neutral:neutral:Japanese:Japanese`, but it leaves eleven
blocks / 394 bytes. The executable and exact result are recorded in
/tmp/mbstring-public-ini-eval-gap.json. Do not describe the full public INI/eval
integration or the overall mbstring goal as complete.

The retained executable for isolation is
target/mbstring-eval-output-probes/tmp/mbstring_startup_1529_ThreadId(4)_2/main.
It reads MB_STARTUP_CODE and registers all four public INI wrappers. Empty eval
and an ordinary mb_language getter finish clean. Isolated public calls retain:
ini_get with a literal name, three blocks / 106 bytes; ini_set with literal name
and value, five / 170; ini_restore with a literal name, three / 106; explicit
ini_get_all with string/bool arguments, three / 104. Assigning arguments to eval
variables and unsetting them removes the literal owners but leaves one / 33 for
get/restore and three / 97 for set. ini_get_all with omitted defaults leaves
two / 80. This points to separate literal/default and native argument-adaptation
ownership gaps, not retained INI result graphs.

Next inspect the general native-function argument boundary before changing its
ownership convention. Relevant code:
crates/elephc-magician/src/interpreter/dynamic_functions/native_execution.rs,
dynamic_functions.rs, dynamic_functions/function_binding.rs, and
src/codegen/runtime_callable_invoker.rs. Direct native calls currently use
eval_call_arg_values without temporary-owner tracking; default materialization and
descriptor argument coercions/retains need balanced cleanup on success, binding
failure, native throw, and by-reference writeback. Existing literal-constructor
and owned-call argument helpers provide reusable patterns. No production edits to
those general native-function ownership paths were made in this checkpoint.

General diagnostic integration (PHP error handlers and display_errors), raw Core
encoding INI routing, and the older reference/query/ownership/shutdown gaps remain.
Public mbstring coverage remains sixty implemented public AOT/eval homes, the
pending mb_parse_str contract, and nine constants; the additional SharedIni home
is internal. The five missing public functions remain mb_parse_str,
mb_convert_variables, mb_ereg_replace_callback, mb_output_handler, and mb_send_mail.

The overall goal stays active. No commits, PR writes, external messages, full
repository suite, or subagents were used. The five tracked PRs remain open at the
same heads, refreshed at 2026-09-10 03:28:13 UTC in
/tmp/mbstring-public-ini-pr-ledger.json. Superseding/closing comments remain deferred
until the entire requested feature is complete.

## Native eval argument ownership checkpoint, 2026-09-10

### Completed INI regression

The previously failing `test_mbstring_public_ini_native_and_eval` now produces
the expected `Japanese:111:Japanese:neutral:neutral:Japanese:Japanese` output
and a clean native heap. It passes both the default backend configuration and
`ELEPHC_IR_OPT=off` with `ELEPHC_REGALLOC=stack`.

The interpreter now shares literal-argument cleanup between constructors and
native function calls. Native binding records defaults, newly coerced scalar
cells, and by-reference marker cells in `BoundNativeFunctionArgs::owners`.
It releases them after invocation and writeback, or after binding/staging failure.
Reference staging lives in a separate cohesive module. Failed marker allocation
releases its retained raw payload; failed reference writeback retires the remaining
slots. Successful native results are released if argument-array cleanup or
writeback prevents returning them to the caller.

The descriptor invoker records owned conversions from indexed arguments and
materialized associative arguments in a lazily allocated array of Mixed cells.
The array transfers each captured owner without an extra retain and updates its
exception guard after growth. A second guard protects the boxed return value
while argument cleanup runs. Both guards use the existing activation chain,
including native exceptions caught by the eval bridge. Raw reference-marker
reads and associative reads share the same conversion cleanup path. Borrowed
typed object arguments retain their existing convention.

### Verification

- 25 native-scope interpreter tests pass, including new literal, default,
  coercion, and reference-marker ownership regressions.
- 43 dynamic-call and constructor-ownership interpreter tests pass.
- 33 selected codegen tests pass: native eval results and arguments, public INI,
  string/scalar/Mixed/heap reference writeback, constructors, named descriptor
  arguments, stack overflow arguments, and callee-saved register preservation.
- The INI regression and both new conversion regressions pass with IR optimization
  disabled and the stack allocator selected. The conversion tests cover more than
  four simultaneous string owners, returned argument aliases, and native throws.
- Full normal and eval-bounded invokers cross-assemble for all five supported
  targets with direct and PIC references. This is assembly coverage, not execution
  on every target.
- Assembly comment alignment, module preambles, and diff whitespace checks pass.
  Compiler and Magician builds used by these checks complete without warnings.

The 13-file source manifest is
`/tmp/mbstring-native-argument-ownership-source-manifest.json`. Test sessions
63853, 48360, 34340, and 38611 record the final native-scope, executable,
cross-assembler, and alternate-backend checks. The attempted nextest command was
unavailable locally; the selected tests ran through Cargo's ordinary harness.

### Remaining ownership and feature work

This fixes the public INI regression and the recorded argument conversion paths.
It does not establish complete ownership for every eval argument expression:
nonliteral temporary expressions still use their prior evaluator contract.
Invoker-created defaults, variadic containers, and temporary raw reference-cell
allocation retain their separate existing cleanup gaps.

Isolation also confirmed an independent native string-return leak. A native
function returning `$a . $b . $c . $d . $e . $f` already persists its returned
string in EIR; the descriptor invoker persists another copy before restoring
the concat offset. Returning an existing string parameter borrows that parameter
and requires different handling. Do not simply free all original string returns.
The retained fixture is `/tmp/mbstring-invoker-owners-probe.php`, its executable
and assembly have the same stem, and results are in
`/tmp/mbstring-invoker-owners-isolation.json`. Empty eval, a string literal,
Mixed identity, and a six-parameter string selector are clean; the concatenation
case retains one 25-byte block for the nine-byte result. The focused ledger
regression reads all six arguments and returns an existing parameter so that
this distinct return-ownership defect does not obscure argument cleanup.

Public INI dispatch through registered native preludes now works for the tested
eval path; public eval registry/capability documentation is still a separate task.
Diagnostic integration, raw Core encoding INI routing, reference/query adapters,
and prior shutdown/inliner ownership concerns remain open. Public coverage is
still 60 implemented AOT/eval homes, the pending mb_parse_str contract, and nine
constants. The five missing functions remain mb_parse_str, mb_convert_variables,
mb_ereg_replace_callback, mb_output_handler, and mb_send_mail.

The overall goal remains active. No commits, PR writes, external messages,
full repository suite, or subagents were used in this checkpoint. The PR ledger
and the deferral of superseding/closing comments are unchanged.

## Public query binding checkpoint, 2026-09-10

`mb_parse_str` now has public AOT and eval homes, typed `RuntimeFnId` dispatch,
and a shared V5 invocation adapter. The adapter stages the real
`elephc_mbstring_query_configuration_v1` provider, retaining live Core INI query
settings. The native entry and the non-unwinding eval entry both reach the
existing shared parser. Capture/output argument selection derives the reference
parameter position and name from the neutral contract rather than assuming the
third argument used by regex captures.

The checker forms a local output reference before evaluating later named
arguments that may mutate that variable. EIR promotion widens an unbound local's
final frame representation directly to Mixed, allowing earlier stores to use
the final representation. This removes redundant load/conversion/boxing during
promotion and fixes the observed 24-byte residual for initialized output locals.
Tracked aliases retain their existing publication and deferred-owner path.

Native heap isolation separated query ownership from two ordinary eval consumers:
parsing, replacing the output, named calls, aliases, and an eval reference
parameter were already clean, while a nested array read retained its temporary
receiver and scalar `var_dump` retained its argument/rendered output. Owned array
reads now retire intermediate receivers and keys. `var_dump` uses the existing
owned builtin argument boundary and releases its rendered output on success and
failure. Its focused interpreter regression also covers partial argument failure.
These fixes do not establish complete ownership for every legacy eval expression
or every nested debug formatter path.

### Verification

- Eleven focused public codegen regressions pass: all four new query tests and
  seven existing capture tests. Query fixtures check binary values, repeated and
  normalized keys, nested arrays, aliases, named argument mutation, opaque eval,
  and seven initial concrete output representations. Query fixtures require a
  clean native heap.
- The four query regressions also pass with `ELEPHC_IR_OPT=off` and
  `ELEPHC_REGALLOC=stack`.
- Four interpreter `var_dump` tests, three neutral support gates, the new query
  error test (five diagnostics), and four source-profile lowering tests pass.
- Actual lowered AOT query/capture calls assemble on all five supported targets.
  Eval dispatch also assembles on all five, both with and without mbregex.
  These checks establish assembly coverage, not execution on every target.
- The compiler builds without warnings. Example PHP syntax and compiler type
  checking pass. Assembly comment alignment, focused Rust preambles/text checks,
  Python syntax, and `git diff --check` pass.
- The complete builtin-docs workflow passes, including target-architecture
  enforcement, site compatibility, and eleven contract-pipeline tests. Generated
  registry/pages publish the new homes and static-only AOT callable policy.
  The string module documents the remaining adapter restrictions, and the
  mbstring example now parses product-search fields and tags.

Logs use `/tmp/mbstring-parse-check-*`, `/tmp/mbstring-parse-docs-final-*`, and
`/tmp/mbstring-parse-final-*`. Source hashes are recorded in
`/tmp/mbstring-public-query-source-manifest.json`. Sessions 73038, 16899, and
51686 reached terminal success for the final contract/build, docs, and native
verification drivers respectively.

### Remaining work

Both backends now have 61 public mbstring homes against the 65-function baseline.
The four missing functions are `mb_convert_variables`,
`mb_ereg_replace_callback`, `mb_output_handler`, and `mb_send_mail`.

The AOT query adapter currently accepts managed local references and aliases.
Raw by-reference function parameters, property-backed references, packed output
references, and generic callable wrappers still need adapters. Shared indexed
query-root promotion remains restricted to uniquely owned payloads. PHP
error-handler routing, eval/dynamic caller strictness, raw Core encoding INI
routing, earlier return/default/variadic ownership concerns, and shutdown/inliner
issues remain open. The complete goal and PR ledger remain active; the binding
count is not a completion claim.

## Output-handler engine checkpoint, 2026-09-10

The preceding query-binding turn made verified implementation progress. Its
34-file checkpoint was revalidated before this work. Public coverage remains
61 of 65 functions while the output-handler engine gains its host-independent
semantics; no public `mb_output_handler` binding is enabled yet.

`state/output.rs` now separates header planning from conversion. An output plan
captures the destination before header publication and owns the proposed header
bytes. The host provides MIME metadata and the result of matching the configured
MIME expression. Header callbacks can run outside the mutable request borrow;
conversion subsequently reads the live internal encoding and replacement policy.
START enables conversion when the response permits it, without disabling a
previously active conversion on a later unmatched MIME type. END resets state
only on the conversion path. Pass mode returns before all phase transitions.

The request preserves PHP's packed source decoder state between calls. Each call
creates a new destination encoder, using 128-word decoder batches and applying
the final flag to its final actual batch. Empty input makes no encoder call.
This differs from MIME's separate empty final call, so the common SJIS-mac,
mobile Shift-JIS, and UUENCODE helpers now expose that distinction. SJIS-mac
hint failures preserve the rejected codepoints for counted long/entity/custom
substitution instead of emitting fixed question marks.

### Evidence

The reference implementation is PHP 8.5.10 `ext/mbstring/mbstring.c`, specifically
`PHP_FUNCTION(mb_output_handler)`, and its existing libmbfl encoder routines.
The local source is `/tmp/php-8510-src`; the fixture generator verifies the exact
PHP version before running ordinary, isolated output-conversion requests.

`scripts/mbstring/generate_output_fixtures.py` records 264 independent requests
and 1,818 calls in `tests/fixtures/output_handler.jsonl.gz`. All 79 encoding names
appear as both source and destination, primarily paired with UTF-8. Coverage
includes phase transitions, partial source buffers, decoder batch boundaries,
nonfinal destination output, mobile compositions, Apple hints, replacement modes,
and between-call changes to internal/output encoding and replacement settings.
The fixture SHA-256 is
`f70d48890ad37ad08642e5138518c76da91a675bec13b8358824f699e041c24b`.

All four output tests pass, including the oracle and independent header-plan
checks. The existing MIME encoder oracle, transfer-operation oracle, and decoder
batch oracle also pass after the shared encoder changes. The bridge builds without
warnings. Python syntax, module preambles, function documentation, and diff
whitespace checks pass. Final verification session 82794 is terminal with exit
zero; logs use `/tmp/mbstring-output-engine-*.log` and source hashes are in
`/tmp/mbstring-output-engine-source-manifest.json`.

The five tracked PRs were refreshed through live GitHub metadata at 04:50:05 UTC.
All remain open at the exact heads in the ledger. Their code does not need a new
diff audit because those heads have not changed.

### Next integration boundary

Complete the native/eval output host adapter before enabling the public contract:
retrieve actual response MIME/default metadata, run the already installed native
PCRE2 MIME provider, publish headers through protected callbacks, and share the
request output state through ordinary and output-buffer callback invocations.
The current non-web `header()` runtime is a no-op, so it cannot yet supply the
metadata PHP CLI output conversion observes. Resolve that host-state gap rather
than assuming every response is the default text MIME type. Core default MIME
configuration and actual output-buffer phases also need end-to-end coverage.

The remaining variable conversion, regex callback, mail, public reference,
diagnostic, strictness, and ownership requirements retain their original scope.
The goal remains active and incomplete.

## Output response ABI checkpoint, 2026-09-10

The twelve-file output-engine checkpoint was revalidated before this change.
The response boundary is now implemented and tested independently of the native
and eval value-coercion tables. Public binding coverage remains 61 of 65.

The neutral `MbOutputHostV1` contract carries native-only metadata reads and a
protected header callback. Its 32-byte table and 48-byte metadata record preserve
absent versus present-empty MIME/default settings. Flags and pointer/length
framing are validated before use. `elephc_mbstring_output_v1` accepts already
coerced string bytes and an integer phase; PHP arity/coercion and public backend
binding remain the enclosing adapter's responsibility.

The coordinator copies input bytes and response strings before header callbacks.
Pass mode and non-START phases do not read the response table. START matches the
actual MIME through the installed native PCRE2 provider, preserving PHP trimming,
case-insensitive matching, and C-string boundaries. INI validation and output
matching now share the same MIME-pattern normalization. Compiled native handles
are released through their provider on every result path.

An owned plan captures the destination before header publication; conversion
uses the live internal encoding and replacement settings afterward. A protected
pending header throwable allows conversion side effects and END reset to finish,
then returns pending status without transferring a PHP result. Ordinary header
refusal does not disable conversion. The host owns actual response updates and
clearing its default-content flag when the header succeeds. The bridge does not
invent those response facts.

### Verification

- All seven `output_abi` tests pass, including replay of the existing 264 PHP
  requests and 1,818 output calls through the real request ABI and native PCRE2.
- Tests cover MIME selection, lookbehind and case folding, embedded NULs, empty
  MIME/default settings, phase and handler boundaries, header-time source changes,
  pending/fatal callback statuses, invalid metadata, and result ownership.
- The four existing output-engine tests, six INI tests, and real native PCRE2 INI
  test pass. The native/eval executable adapters are not covered by these tests
  because they are still unbound.
- PHP 8.5.10 CLI independently confirms that explicit `application/json` leaves
  UTF-8 input unchanged under the default selection expression, while
  `text/plain; charset=UTF-8` converts the same input to ISO-8859-1.
- The new neutral ABI layout test passes, and the bridge plus builtin exporter
  build without warnings. The complete builtin documentation workflow passes;
  generated Markdown and JSON hashes are unchanged because no PHP contract or
  public backend home was added.
- Eleven touched Rust files pass module preamble, explicit function documentation
  (114 functions), text, and whitespace checks. No assembly emitter changed.

Focused logs are `/tmp/mbstring-output-adapter-final-tests.log` and
`/tmp/mbstring-output-adapter-check-*.log`. Sessions 83984 and 24362 reached
terminal exit zero. Source hashes, including this checkpoint, are recorded in
`/tmp/mbstring-output-adapter-source-manifest.json`.

The five tracked PRs were refreshed at 05:11 UTC. All remain open, unmerged, and
at the exact previously audited heads. No external messages or PR edits were made.

### Next integration work

Connect actual CLI/web response metadata and default MIME configuration to this
response capability, including accepted-header state and output-buffer handler
flags. The current non-web `header()` emitter is still a no-op. Add the public
contract, shared coercion route, typed AOT/eval adapters, callable/output-buffer
coverage, and supported-target validation only with those actual host semantics
in place. Do not count this prepared-input ABI as a public function binding.

The other three missing functions and all previously recorded reference,
diagnostic, strictness, and ownership debts remain open. The goal remains active.

## Native response metadata checkpoint, 2026-09-10

The previous response-ABI turn made verified progress. Its twelve-file source
checkpoint was revalidated before this work. Native response events now feed the
shared state used by the prepared output API. Public coverage remains 61 of 65.

`state/response.rs` owns accepted MIME history, startup response defaults, and
header commitment. PHP retains the first accepted Content-Type for output MIME
selection even after a later header replaces the outgoing field. Explicit header
charset insertion uses PHP's case-sensitive text/ and charset= checks, whereas
default-header commitment uses its case-insensitive text/ check. Header validation
and trailing whitespace handling precede MIME mutation. Empty and absent MIME
values remain distinct.

The new response C ABI provides header validation/publication planning, terminal
commitment, and `MbOutputInfoV1` reads from the actual request state. Accepted
headers return owned normalized wire bytes; ordinary rejection returns false.
Protected diagnostics run outside request borrows and identify either header() or
mb_output_handler(). Metadata reads accept the actual `_ob_in_handler` flag as
their context and borrow MIME strings only through the next request mutation.

When mbstring is selected, `__rt_header` uses the shared response operation in CLI
and web builds. Web mode forwards accepted normalized bytes to the existing web
header sink before releasing the Rust result. The separate
`__rt_mbstring_output_header` callback returns protected status to Rust; the
ordinary native header wrapper unwinds only after Rust returns. Terminal stdout
commitment occurs after print_r return-mode capture, handler-output suppression,
and output buffering. Thus buffered bytes do not freeze headers. Both native
architectures implement these boundaries.

Startup selection now retains raw default_mimetype/default_charset values for
response initialization. Invalid charset values containing NUL or line breaks
also fall back to UTF-8 before compiler-side encoding inheritance. Request reset
clears accepted headers and commitment while retaining configured defaults.

### Verification

- Four shared response integration tests pass, including actual metadata reads
  and protected header writes through the prepared output API. The configured
  response reset unit test also passes.
- PHP 8.5.10 CLI confirms that application/json followed by text/plain retains
  JSON MIME selection and leaves UTF-8 bytes unchanged; reversing the headers
  converts the same bytes to ISO-8859-1.
- All seven existing output ABI tests and four output-engine tests pass,
  retaining the 264-request, 1,818-call codec oracle.
- Two new executable codegen tests pass with default optimization/allocation and
  again with IR optimization disabled and stack allocation. They cover buffered
  and return-mode output, late-header suppression, invalid header bytes, normal
  stdout, and clean native heap summaries.
- The existing opaque-eval Stringable/coercion regression passes with native and
  eval callbacks that print during mbstring argument conversion.
- The combined header/stdout emitter assembles in all twenty combinations of
  five supported targets, web on/off, and mbstring on/off. Disabled capabilities
  do not name their bridge symbols. The gate exposed an Apple conditional-branch
  relocation restriction; the final implementation uses a local conditional
  branch followed by an unconditional native unwind transfer.
- All four compiler startup-selection tests pass in the elephc binary test
  harness. An earlier library-only filter selected no runnable pipeline tests;
  `/tmp/mbstring-response-startup-bin.log` is the authoritative execution evidence.
- The compiler and bridge build without warnings. Example PHP syntax and
  compiler checks pass. Assembly comments, fifteen Rust module preambles and
  110 function docblocks, text hygiene, the enforced builtin EIR boundary audit,
  and `git diff --check` pass.

The mbstring example now declares its text response before writing output, and
the string documentation describes the current response integration and remaining
public/Core limitations. No PHP contract or generated builtin page changed.

Logs use `/tmp/mbstring-response-*.log`; final successful sessions include 42704
(target/native checks), 78326 (coercion and alternative codegen), 33981 (build and
example), and 88608 (actual compiler startup tests). Source hashes are recorded in
`/tmp/mbstring-response-source-manifest.json`. The final stdout source change only
clarified its Rustdoc after executable verification.

All five tracked PRs were refreshed at 05:40 UTC. They remain open, unmerged, and
at their previously audited heads. No PR messages or edits were made.

### Remaining integration

Expose the public mb_output_handler contract and typed AOT/eval/callable routes
through shared argument coercion, the prepared output API, and the now-emitted
response callbacks. Test actual output-buffer handler invocation and supported
target lowering for that public binding. Web transport execution, including
default-header publication, still needs end-to-end verification; assembly alone
does not prove that behavior.

Runtime Core default_mimetype/default_charset getters/setters and raw
internal/input/output encoding routing remain incomplete. Integrate their raw
ownership and inherited-setting callbacks with one authoritative Core value per
directive, rather than leaving independently mutable response defaults. The
current diagnostic adapter still lacks PHP error-handler routing and complete
output-start source-location details. These requirements, the other three missing
functions, and earlier reference/ownership/strictness debts remain in scope.
The complete goal remains active and unproven.

## Public output-handler checkpoint, 2026-09-10

Public coverage is now 62 of 65 functions. `mb_output_handler` has one shared
two-argument contract, stable runtime ID 84, typed AOT lowering, an eval registry
binding, and ordinary callable support. Its new value-host entry reuses shared
argument copying and ordered coercion before calling the existing prepared
response API. Arity is checked before either host is dereferenced. Strictness,
binary strings, Stringable conversion, callback failure, and owner retirement
follow the shared invocation contract.

The native invocation frame now contains independent value and response host
tables plus the result. The response table reads the live `_ob_in_handler` flag
and uses the protected response-header callback. Both native architectures
implement this path, including Apple external-symbol spelling.

### Initialization and ownership corrections

Default-only programs previously skipped MIME-provider initialization. Mbstring
and opaque eval now install their effective startup encodings even when no INI
override was supplied. Their runtime requirements include managed PCRE2 for MIME
selection. This does not enable the independent eval `preg_*` capability.
Library lowering supplies the same defaults as the CLI. Platform header/stdout
selection also includes opaque eval; a broader existing eval test caught the
initial missing output-header symbol when no static mbstring call was present.

Output-handler string results now persist a borrowed string payload once instead
of allocating an unused intermediate cast. Buffer pop releases native callable
descriptors through their typed release helper. Eval's shared owned-argument
policy now applies before direct builtin fast paths and covers `bin2hex`,
`header`, and `ob_start`. Eval output callback argument boxes are retired after
invocation. Context destruction detaches registered callback roots and releases
them while the context still supports protected value destruction.

The latter is only context-level cleanup. Individual eval buffer closure still
retains its registration until context destruction. This remains a required
lifecycle fix, together with exception/status propagation through output calls.
Do not describe arbitrary repeated buffer creation as memory bounded yet.

### Verification

- Five independent public output-ABI tests, the existing argument-copy ordering
  regression, and all seven output-ABI codec tests pass. The existing oracle
  covers 264 requests and 1,818 calls.
- Thirteen executable public binding/ownership tests pass in the default mode
  and with IR optimization disabled plus stack allocation. Coverage includes
  native/eval direct, named, unpacked, first-class, `call_user_func`, binary,
  nested-buffer, split-flush, and clean-heap paths. Four functional fixtures were
  independently executed with PHP 8.5.10. PHP replaces the split truncated UTF-8
  segments in these fixtures; do not infer carry behavior from the API name.
- The compile-error regression checks seven invalid signature cases. Shared
  output return coercion and native first-class output handlers pass. Existing
  eval buffer queries, native/eval coercion, eval query parsing, public INI,
  and public info regressions pass.
- Five dynamic-eval tests pass after updating their managed-package fixtures.
  Missing MIME dependencies produce a recovery diagnostic; installing PCRE2
  permits compilation while `preg_match` remains unavailable without capability.
  Explicit regex capability still enables the managed provider.
- The actual invocation emitter assembles in ten target/mbregex combinations.
  Header/stdout plus edited result-conversion and buffer-pop emitters assemble
  in twenty target/web/mbstring combinations. Feature-selection checks cover
  twenty target/static-mbstring/eval combinations. These are assembly and
  selection checks, not execution on non-host targets.
- Runtime ID/support gates pass, and the compiler builds without warnings.
  The complete mbstring example passes PHP syntax and compiler checks, compiles
  with real managed Oniguruma/PCRE2 installed offline, produces byte-identical
  PHP 8.5.10 output, and reports 639 allocations, 639 frees, and no live blocks.
- The builtin documentation skill workflow passed: generator build with curl,
  rendering, module/comparison generation, builtin audit, site compatibility,
  enforced EIR boundary audit, and eleven contract-pipeline tests. New public
  and internal output-handler pages describe both backends and all five targets.
  Manual dependency and eval docs now distinguish the MIME provider from regex
  capability and remove stale claims that mbregex still uses PCRE2.

Logs use `/tmp/mbstring-output-binding-*.log`. Final successful sessions include
32185 (eval capability, ID/support, invocation assembly, build) and 93749 (real
managed example). Session 64456 passed the response target/capability checks and
six shared regressions before the stale no-managed-project eval test failed;
the corrected eval tests subsequently passed in 32185. Earlier failed probes
remain diagnostic evidence, not successful validation. Source hashes and final
text/Rust hygiene results are recorded in
`/tmp/mbstring-output-binding-source-manifest.json` and its hygiene report.

The tracked PRs 895, 898, 899, 900, and 902 were refreshed again before
06:56 UTC. All remain open and unmerged at the exact heads in the ledger.
No PR messages, closures, or edits were made.

### Remaining work

`mb_convert_variables`, `mb_ereg_replace_callback`, and `mb_send_mail` remain
unbound. Per-buffer eval callback retirement, Core response/encoding INI
mutation, diagnostic routing, web transport verification, strictness,
reference adapters, and the earlier ownership debts remain required. Binding
coverage and focused passing checks do not establish complete extension support.
The original goal remains active.

## Output callback retirement checkpoint, 2026-09-10

Explicit output-buffer closure now detaches the eval callback registration and
releases its retained owner immediately. Active registrations use a map with
monotonic IDs, so closed buffers leave neither callback roots nor tombstones.
Failed starts return their retained callback owner, and context retirement
removes only registrations that are still active.

Native output calls from eval use the versioned `OutputRequestV1` boundary,
including the shared boxed `ob_clean`, `ob_flush`, and `ob_end_*` dispatcher.
A callback destructor can return a pending Throwable to native code without
unwinding through Rust. Buffer removal snapshots and clears slot ownership,
releases name and byte storage, and then releases the callback. Get-and-pop
operations protect their copied result through the existing owned-value
exception guard until callback retirement succeeds.

The new lifecycle tests exposed two separate native-method argument leaks.
Methods and constructors now share validated borrowed argument loading from
the private array retained by the Rust caller. Mutable Mixed reference slots
retain an independent staging owner and release it during ordinary or exceptional
writeback. Instance, static, dynamic, and nullsafe method expressions also reuse
the existing literal-argument cleanup helper. This fixes the observed literal
11/12/13 leak rather than excluding those arguments from the final regression.

Verification on the completed changes:

- Seven lifecycle tests pass with clean native heap reports, covering native
  closure captures, native and eval-declared callable objects, repeated
  `mb_output_handler` registrations, and throwing destructors during end/get.
  The same seven cases pass with IR optimization off and stack allocation.
- Three native-method ownership tests pass both normally and with those alternate
  settings. They cover returned argument aliases, literal arguments across six
  method call forms, Mixed reference replacement, and writeback before throws.
  The original real CLI diagnostic program now reports 84 allocations and
  84 releases, with no remaining blocks.
- Two interpreter ownership tests cover all six method forms when later argument
  evaluation fails, plus nullsafe argument skipping. Eight existing method-contract
  tests, the rejected-start ownership test, and the two registry tests pass.
- The 45 existing output-buffer tests and 13 public mb_output_handler tests pass.
  Focused constructor ownership, instance/static argument and reference tests,
  and the wide method/constructor bridge regression pass. A static Mixed reference
  regression found during validation was fixed and its full eight-test group rerun.
- The protected output entry plus shared dispatcher assembles on all five targets.
  Response, buffer retirement, and guarded get-and-pop emitters assemble in twenty
  target/web/mbstring combinations. Real compiler-generated instance/static method
  and Mixed-reference bridges also assemble for all five targets. Non-host results
  establish emission and assembly, not execution on those platforms.
- The compiler builds without warnings. Rust preambles and 520 function or foreign
  declaration docblocks across the 35 scoped source files pass the hygiene check;
  assembly comments and `git diff --check` pass. No full local suite was run.

Evidence is under `/tmp/mbstring-output-lifecycle-*.log`, including final retirement,
method literals, alternate configurations, static references, and build logs.
Compiler/assembler artifacts are in
`/tmp/mbstring-output-lifecycle-method-ref-targets/`. The scoped file list, hygiene
report, and source manifest use the same lifecycle prefix. Earlier failed logs
document corrected regressions and are not successful validation evidence.

Remaining output work includes status-aware eval handler invocation itself,
restoration after a handler throws, and shutdown buffer ownership. The new
retirement protocol does not complete those paths. General eval expression and
loop temporary ownership still needs its separate audit. The three missing
public functions remain `mb_convert_variables`, `mb_ereg_replace_callback`, and
`mb_send_mail`; Core INI, host response behavior, strictness, reference adapters,
and the other recorded compatibility/packaging checks remain in scope.
No PR comments, closures, commits, or pushes were made. The complete mbstring
goal remains active.

## Output handler invocation checkpoint, 2026-09-10

The private invocation hook now uses `OutputHandlerCallV1`, a shared 48-byte
C record with borrowed input bytes and separate owned result and Throwable
outputs. Its return status distinguishes success, fatal failure, and PHP throw.
Magician transfers pending exceptions instead of silently mapping every failure
to pass-through. Both native architectures consume a returned Throwable only
after the Rust hook has returned. The existing two-pointer installation protocol
continues to install invocation and retirement together.

The shared output process helper now completes the operation before propagating
a callback exception. It restores the incoming output-handler guard, preserves
original bytes for flush operations, empties the slot, and retires FINAL buffers.
The public end/get callers and shutdown drain no longer pop that slot again.
Protected unary adapters publish replacement outputs into the caller's frame;
the existing protected cleanup machinery keeps cleanup resumable when a parent
sink or retiring callback throws. Native descriptor invocation also guards its
argument container and any replacement bytes with the shared ownership records.
Empty writes continue to skip response commitment and parent chunk processing.

Handler state is now stored authoritatively in the slot flags. A false result or
throw sets DISABLED and later operations skip that callback. STARTED and PROCESSED
remain distinct, including for default buffers. The start helper clears caller
type/status bits according to the PHP 8.5 oracle rather than treating them as
preexisting handler state. AOT and eval status queries read the same stored flags.

The new heap checks also found ownership defects in status construction. AOT
boxing now consumes the original raw hash owner after the Mixed box retains it.
Eval releases temporary keys, values, full-status entries, and the optional
full-status argument. Successful simple, full, and empty status arrays now leave
no outstanding native allocations in the focused regression.

Verification completed for this checkpoint:

- Five new tests cover 23 ordinary program cases: six explicit operations through
  native callbacks, eval-to-native callbacks, and eval-declared callbacks; disabled
  false-returning handlers; and simple/full/empty status ownership plus input flags.
  All pass with clean native heap reports. The same five tests pass with IR
  optimization off and stack register allocation. Normal results are recorded in
  the fourth-test log for four tests and the final status log for the corrected
  status case. Earlier failed runs are diagnostic evidence, not passing results.
- The 45 existing output-buffer tests, seven callback-retirement tests, and all
  13 mbstring output-handler tests pass. Seven Magician output-buffer unit tests
  and both shared output ABI layout tests pass.
- The complete handler/apply/process/start/pop/get emitters assemble in all
  twenty target/web/mbstring combinations. The Apple assembler rejected a
  conditional branch to an external throw symbol; this was corrected to a local
  conditional branch followed by an ordinary external tail branch and rechecked.
- A real compiler-generated program containing native and eval handler throws,
  guarded get-and-pop, and status boxing compiles and assembles on all five
  supported targets. iOS device and Simulator use static-library emission.
  These non-host results establish code generation and assembly, not execution
  or linking on those platforms.
- `cargo build` completes without warnings. All 16 scoped Rust files have module
  preambles, and all 121 explicit functions have docblocks. Assembly comments and
  `git diff --check` pass. No full local Rust or Docker suite was run.

Evidence uses `/tmp/mbstring-output-invocation-*`: `status-final`, `alternate`,
`existing-output`, `lifecycle`, `eval-unit`, `abi`, `mb-handler-final`,
`targets-final`, and `build` logs; the scoped file list, hygiene report, and source
manifest; plus real compiler/assembler artifacts under `real-targets/`.
The standalone baseline is local PHP 8.5.10. The six-action exception oracle is
saved as `/tmp/mbstring-output-handler-exception-oracle.json`; additional ordinary
parent-buffer cases are in `parent-oracles.json` under this checkpoint prefix.
The primary manual reference is
<https://www.php.net/manual/en/outcontrol.user-level-output-buffers.php>.

The complete output compatibility work remains unfinished. In particular:

- When an inner handler and a parent chunk handler both throw during one flush,
  PHP preserves the inner exception without the parent exception as `previous`.
  The current protected cleanup helper selects the newest exception and chains
  the suspended one. Output-specific exception priority still needs implementation
  and focused tests, including interaction with callback retirement.
- A dynamically declared eval handler returning `string|false` exposed a return
  coercion discrepancy: the false branch did not remain false, so the handler was
  not disabled. This is still open. The final disabled-handler regression uses a
  native boolean-returning handler that visibly changes on a second call, plus
  an eval-declared boolean-returning handler; it does not claim union-return parity.
- Handler-produced output on false/throw, shutdown exception suppression, and
  host/request lifecycle behavior remain unverified. Shutdown now retires ordinary
  drained owners, but its exception and reentry behavior is not declared complete.
- General eval conditional, operand, subscript-key, and inline-callable temporary
  ownership remains separate work. The output operation regression keeps queried
  values in explicit variables and generates the expected retained/closed branch
  in Rust, so those unrelated temporary paths do not mask callback ownership.

The public function count remains 62 of 65. `mb_convert_variables`,
`mb_ereg_replace_callback`, and `mb_send_mail`, plus the previously recorded Core
INI, response, strictness, reference-adapter, and packaging audits, remain in scope.

The PR ledger was refreshed read-only at 2026-09-10 08:34:30 UTC. All five remain
open and unmerged, with unchanged heads:

| PR | Function | Head SHA |
| --- | --- | --- |
| #895 | mb_strwidth | e96b43f219c085551a3c42c4cbbbd752846c0f88 |
| #898 | mb_strtoupper | b15d629eb1d159ffb32fe272eb0f5c17472fb97b |
| #899 | mb_strtolower | 089ffc6bee2d2b3f17746da6fdf435f5be423440 |
| #900 | mb_strimwidth | f402f762e093b4ad1c98bee1c83cb59bb4278c8c |
| #902 | mb_convert_case | 405f77283dd4e5c805eff74dc70f3fe013b56885 |

The metadata snapshot is `/tmp/mbstring-output-invocation-pr-ledger.json`.
No PR comments, closures, commits, or pushes were made. The complete goal remains
active; this checkpoint records verified progress and explicit remaining work.

## Replacement callback engine checkpoint, 2026-09-10

The shared regex session now exposes `replace_callback` with owned PHP capture
registers, callback-produced bytes, and a distinct callback failure channel.
Ordinary and callback replacements use the same validation, compilation, search,
diagnostic, unmatched-suffix, and byte-advancement loop under
`regex/request/replace/walk.rs`. The ordinary renderer still initializes its
backreference metadata once after compilation, before searching. Its public Rust
input and result interfaces are unchanged.

The callback renderer preserves PHP's capture distinction: unmatched numeric
groups become empty strings, while named empty or unmatched groups remain false.
Replacement bytes are appended literally without backreference expansion or
encoding validation. The subject is validated with the encoding captured before
argument coercions; compilation uses the live request settings. An active pattern
retains its original options across callback changes to the defaults. Limits are
supplied afresh before each search, so future host integration can read the live
request INI values. Callback failure stops matching and discards partial output.

Evidence comes from local PHP 8.5.10 and the pinned primary source at
<https://github.com/php/php-src/blob/php-8.5.10/ext/mbstring/php_mbregex.c>, including
`_php_mb_regex_ereg_replace_exec` and `_php_mb_onig_search`. The new standalone
capture script records binary inputs, exact registers, results, warnings, callback
exceptions, and option changes. All 32 committed JSON-line fixtures reproduce
exactly when passed through the PHP capture script again.

Verification completed:

- Five focused callback tests pass, including all 32 independent PHP traces,
  first-error termination, fresh limit reads, retained compiled options, and
  validation with the entry encoding.
- All 1,431 existing ordinary-replacement PHP fixture cases pass through the
  extracted loop.
- The three existing AOT/eval replacement tests pass: public call forms,
  runtime errors, and result ownership. Each test exercises both backends.
- `cargo build` completes without warnings. PHP syntax validation and
  `git diff --check` pass. The six scoped Rust files have module preambles and
  all 49 explicit functions have docblocks. No target emitter changed in this
  checkpoint; execution evidence is from the Linux x86_64 host.

The public binding count remains 62/65. This checkpoint completes the shared
matching primitive, not the PHP-facing `mb_ereg_replace_callback` implementation.
Next steps are ordered callable resolution at parameter two, protected host
invocation and result string casting, ownership/exception transfer, neutral
catalog and typed runtime bindings, AOT/eval tests for all callback forms, and
the required documentation/example and target gates. `mb_convert_variables`,
`mb_send_mail`, and every earlier recorded integration gap remain in scope.

The PR ledger was refreshed at 2026-09-10 09:00:20 UTC. PRs #895, #898, #899,
#900, and #902 remain open and unmerged at the existing recorded heads. The
snapshot is `/tmp/mbstring-replacement-callback-pr-ledger.json`. Scoped source
files, their SHA-256 manifest, and the hygiene report use the same temporary
prefix. The complete mbstring goal remains active.

## Public replacement callback checkpoint, 2026-09-10

`mb_ereg_replace_callback` now has public AOT and eval bindings. Focused public
call, ownership, effects, and error coverage passes after aligning callback arity
errors across native and eval invocation. Public binding coverage is 63 of 65;
`mb_convert_variables` and `mb_send_mail` remain unimplemented.

This checkpoint does not establish complete callback support. The native
retained-owner regression still reports two unreleased string owners per regex
invocation in the exercised callback shape. Closure and method callback frontend
coverage, broader reference and strictness behavior, Core INI and response host
integration, and the earlier compatibility debts remain open.

The read-only PR ledger was refreshed at 2026-09-10 10:22:43 UTC. PRs #895,
#898, #899, #900, and #902 remain open and unmerged at the exact heads recorded
in the ledger above. No PR comments, closures, commits, or pushes were made.
