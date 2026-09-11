# mbstring compatibility data

The extension work is tracked in [the implementation plan](../../.plans/mbstring-extension.md).
The complete public scope comes from PHP 8.5.10, matching `scripts/docs/php_baseline.json`.
The native engine does not invoke PHP or download data at build/run time.

`php_surface.json` records function signatures, constants, encoding order, aliases,
and MIME names. The encoding generators capture deterministic conversion results
from the same PHP executable. They do not copy libmbfl implementation code.
One-byte reverse tables enumerate every Unicode scalar, preserving PHP's chosen
reverse mapping even when it differs from a naive decoder-table inversion.
Legacy multibyte tables capture every byte pair, EUC-JP three-byte overrides,
invalid consumption, composite mappings, mobile emoji, and SoftBank escape sequences.
Sparse mobile UTF-8 tables preserve carrier PUA conversions, emoji compositions,
and standalone regional-indicator rejection. GB18030 and EUC-TW tables retain
independent forward and reverse mappings for the exact PHP-supported revisions.

Name resolution checks canonical names first, then MIME charset names, then aliases,
preserving PHP's catalog order within each pass. The 458 observations from
`capture_encoding_lookup.php` cover casing, shared MIME labels, MIME-only names
such as `BIG5`, and invalid inputs. Regex and ordinary text operations use this
same resolver, while retaining their separate accepted-name and byte-validation rules.

Regenerate the PHP data in this order, with PHP 8.5.10 and mbstring installed:

```bash
php scripts/mbstring/capture_surface.php > scripts/mbstring/php_surface.json
php -d error_reporting=0 scripts/mbstring/capture_encoding_lookup.php > crates/elephc-mbstring/tests/fixtures/encoding_lookup.json
php -d memory_limit=256M scripts/mbstring/capture_singlebyte.php
php -d memory_limit=256M scripts/mbstring/capture_doublebyte.php
php scripts/mbstring/capture_mobile_utf8.php
php scripts/mbstring/capture_gb18030.php
php scripts/mbstring/capture_euctw.php
php scripts/mbstring/capture_jis.php
php scripts/mbstring/capture_boundaries.php
python3 scripts/mbstring/generate_encodings.py
python3 scripts/mbstring/capture_html.py
php scripts/mbstring/capture_languages.php
python3 scripts/mbstring/generate_languages.py
php scripts/mbstring/capture_unicode.php > crates/elephc-mbstring/tests/fixtures/unicode.json
php scripts/mbstring/capture_codecs.php > crates/elephc-mbstring/tests/fixtures/codecs.json
php scripts/mbstring/capture_text.php > crates/elephc-mbstring/tests/fixtures/text.json
php scripts/mbstring/capture_operations.php
php scripts/mbstring/capture_kana.php
php scripts/mbstring/capture_batches.php
php scripts/mbstring/capture_transfer.php
php scripts/mbstring/capture_entities.php
php scripts/mbstring/capture_mime_decode.php
php scripts/mbstring/capture_mime_encode.php
php scripts/mbstring/capture_info.php
php scripts/mbstring/capture_http_input.php
php scripts/mbstring/capture_detect_many.php
php scripts/mbstring/capture_parse_str.php
php scripts/mbstring/capture_parse_str_reentry.php
php -d max_input_nesting_level=1 -d log_errors=0 scripts/mbstring/capture_parse_str_display.php > crates/elephc-mbstring/tests/fixtures/parse_str_display.json
php scripts/mbstring/capture_ini.php
python3 scripts/mbstring/capture_ini_startup.py
python3 scripts/mbstring/capture_ini_reentry.py
python3 scripts/mbstring/capture_regex.py
python3 scripts/mbstring/capture_regex_split.py
python3 scripts/mbstring/capture_regex_replace.py
python3 scripts/mbstring/capture_regex_replace_public.py
python3 scripts/mbstring/capture_regex_capture.py
python3 scripts/mbstring/capture_regex_output.py
php scripts/mbstring/capture_entity_maps.php
php scripts/mbstring/capture_state.php
php scripts/mbstring/capture_detect.php
php scripts/mbstring/capture_detected_conversion.php
php scripts/mbstring/capture_conversion_errors.php
php scripts/mbstring/capture_arrays.php
php scripts/mbstring/capture_split_batches.php
php scripts/mbstring/capture_coercions.php
php scripts/mbstring/capture_coercion_order.php
php scripts/mbstring/capture_utf7.php
php scripts/mbstring/capture_hz.php
php scripts/mbstring/capture_iso2022kr.php
```

Unicode tables use the [Unicode 17.0.0 UCD](https://www.unicode.org/Public/17.0.0/ucd/),
the version used by this PHP baseline. Download `UnicodeData.txt`, `SpecialCasing.txt`,
`CaseFolding.txt`, `DerivedCoreProperties.txt`, and `EastAsianWidth.txt` to a directory,
then run:

```bash
python3 scripts/mbstring/generate_unicode.py /path/to/ucd
```

The generator rejects files whose SHA-256 differs from the pinned source and writes
an input/output digest manifest. Unicode's license is preserved alongside the tables
and must accompany distributions containing those data.

Validate changes with the focused crate suite:

```bash
cargo test -p elephc-mbstring
```

The fixtures are captured independently from PHP. The tests do not infer expected
behavior from the Rust algorithms or their generated tables. Passing these engine
tests alone does not prove AOT/eval integration or complete extension coverage.

The replacement callback fixture contains 32 PHP 8.5.10 traces with exact numeric
and named captures, binary replacement bytes, multiple encodings, empty-match
advancement, option errors, callback exceptions, and changes to live settings.
`capture_regex_replace_callback.php` reads its input records as JSON lines and
emits fresh expected traces. To regenerate into a separate review file:

```bash
jq -c '.input' crates/elephc-mbstring/tests/fixtures/regex_replace_callback.jsonl | php scripts/mbstring/capture_regex_replace_callback.php > /tmp/regex_replace_callback.jsonl
```

With the pinned Oniguruma 6.9.10 development files or a managed archive prefix,
run only the callback tests and the existing ordinary-replacement comparisons:

```bash
cargo test -p elephc-mbstring --test regex_request regex_callback_ -- --ignored
cargo test -p elephc-mbstring --test regex_request regex_replace_matches_php -- --ignored
```

These tests verify the shared matching engine. The public callback binding,
ordered callable validation, callback result casting, and native/eval exception
boundaries still require integration before the function counts as supported.

The compressed operation fixture stores original arguments, return values, and exact
exceptions. `operations-excluded.json` records calls which terminate the PHP oracle
process because PHP 8.5.10 underflows its SJIS-mac substring allocation when an
unbounded substring starts beyond the input. These exclusions remain explicit audit
items; they must not be counted as successful PHP comparisons.

UTF-7 fixtures cover every BMP code unit with and without a terminator, malformed
surrogates and shifts, and legacy streaming cut budgets. Encoder hashes cover every
Unicode-range value, including UCS-originated surrogates. Mobile UTF-8 checks every
Unicode scalar in both directions; GB18030 checks the full four-byte address space
and encoder range; EUC-TW checks all suffixes of the three supported historical planes.
HZ uses the shared EUC-CN tables with two PHP-specific mapping exceptions; its oracle
checks every shifted byte pair, every Unicode-range encoder input, and stateful cuts.
ISO-2022-KR likewise shares UHC mappings and tests designation escapes, SI/SO shifts,
every shifted pair, the raw-code encoder fallback, and legacy cut-flush behavior.

JIS, ISO-2022-JP, ISO-2022-JP-MS, CP50220/21/22, ISO-2022-JP-2004, and mobile KDDI
tests cover every Unicode-range encoder input and every byte pair under their
character-plane and escape prefixes. The 2004 transport reuses the EUC-JP-2004
maps. Other JIS variants share ordinary Japanese planes while retaining their
distinct validation, substitution, private-use, and legacy cut rules. Mobile
encoder composites are captured independently from decoder expansions because
PHP's keycap and flag mappings are not all reversible.

Kana fixtures hash every Unicode-range scalar for all 17 flags and compare every
one-, two-, and three-flag combination, binary invalid flags, and contextual
conversion across the supported codecs. CP50220 consumes the same Unicode KV
transform as the public text engine. Data filenames remain distinct on
case-insensitive filesystems.

Batch fixtures check KDDI emoji lookahead at the source decoder's 128-word boundaries
and UTF-16 contextual sigma around valid and malformed surrogates. UTF-16 partitions
follow the pinned PHP x86_64 AVX2 oracle, reproduced with a portable scalar scan;
no SIMD capability is required by the engine. The source PHP implementation can
choose different scratch-buffer partitions on other platforms, so these observable
baseline choices must remain explicit in the final compatibility audit.

The four transfer codecs complete the 79 canonical encoding identities. Their fixture
checks raw-byte conversion overrides, malformed Base64 and quoted-printable, UUENCODE
line parsing, HTML entities, and separate legacy byte-cut behavior. HTML names and
preferred encoder spellings are queried from PHP using Python's HTML name inventory
as candidates; the generator also captures the complete Unicode-range encoder hash.

Numeric-entity fixtures cover 32-bit map casts, offsets and masks, first-range wins,
optional semicolons, malformed lengths, overflow, all encodings, and substitution.
Mobile fixtures retain encoder calls across decoder batches, including recursive
replacement-marker lookahead and kana's deferred final character.

Language metadata captures all 12 PHP language identities, aliases, default auto
lists, and mail defaults. The state fixture records ordered setting requests and
compares getters, exact exceptions, and deprecation output after each request. It
covers comma-separated versus array lists, NUL handling, failed language updates,
and the last-name encoding cache. The INI fixtures additionally compare raw
configuration metadata, effective settings, unsuccessful writes, repeated restores,
binary keys and values, numeric overflow, and fresh-process startup configuration.
The startup capture quotes values so the PHP INI scanner preserves their text for
the mbstring handlers. The runtime capture avoids PHP 8.5.10's borrowed MIME-string
release bug in `mb_get_info()` by reading individual selectors and `ini_get()`.
The reentry capture adds 1,376 isolated PHP traces with nested setters, restores,
public setting changes, throwing handlers, retained getter values, equal byte
copies, ASCII case conversion, reversal, interned literals, and empty/one-byte
strings. PHP's raw slot commit guard compares string identity. Scalar INI getters
normalize one-byte strings, while
array getters retain raw storage; the shared `IniString` models both. Some nested
copy cases also expose Zend's previous-string over-release in the input array, so
the worker rebuilds fixture inputs from the original JSON after all observations.
The engine preserves owned data instead of reproducing this memory defect.

`elephc_mbstring_ini_v1` releases request borrows before protected PHP diagnostics.
A pending throwable suppresses subsequent callback delivery while the handler
finishes its state changes. The native MIME provider has a process-lifetime table
and paired allocation/free operations. The contract-owned default expression has
an exact internal prefix matcher, while a custom expression without the provider
reports the missing `--with-mbstring` capability. Malformed installed providers
remain fatal. The native test loads the actual repository
PCRE2 shim, using Homebrew on macOS and the aligned pkg-config provider on Linux.
Process startup configuration is immutable and shared with newly initialized
threads; resetting a request copies the validated prototype without recompiling
MIME expressions or inheriting another request's mutations.

Owned INI string results include a retained identity. Array results contain the
ordinary graph followed by checked identity records for every string-valued cell.
Hosts use `ARG_INI_STRING` for existing strings, retain a lease for native string
metadata, and release it at the final owner. The registry removes an identity when
its last host lease ends, and the intern index retires byte keys at final ownership
release. Request reset does not invalidate still-owned results.
`ARG_STRING` setter values instead create fresh temporary text. Public INI adapters
must preserve original native/eval string identities through aliases and array
results, including the lifecycle hooks that release their leases. Those adapters,
startup option routing, and host core-encoding updates remain integration work in
the implementation plan.

Native allocation metadata records lazy logical origins per thread. Persistence
shares a complete known origin or assigns a fresh destination, including empty
strings and concat temporaries taken over in place. Unknown scratch or foreign
source addresses are never retained. AOT string constants and eval string literals
explicitly mark interned origins; eval registers only its owned native payload.
Origins share an unresolved record until INI needs their bytes, then share one
leased identity until the final native alias is released. This also preserves
aliases created before the first INI access and after their original allocation
has been freed. Heap release retires metadata before address reuse; request cleanup
clears the map before arena reset. Creation activates tracking, while cleanup skips
an untouched map. Programs without mbstring/eval omit the hooks and bridge calls.
Independent C/assembly tests exercise actual persistence/free helpers, concat
takeover, and the eval literal wrapper. Deliberate provider register clobbers verify
complete wrapper preservation.

The native ASCII lower/upper helper returns independent owned storage, preserves
logical origin when no byte changes, and assigns a fresh origin before changing
bytes. Eval follows the same origin distinction and retains arbitrary PHP bytes.
AOT and eval share the literal-byte codec; eval retains non-UTF-8 constants and
attribute values explicitly and registers literal identity only after native
boxing. The pipe folder keeps identity-changing string transforms at runtime.
Auditing remaining direct string constructors, scalar conversions, concatenation
and cast provenance, short-result normalization, and other transform identities
remains necessary before routing public INI calls.

Detection uses PHP's version-pinned common-character selection. Download
[`common_codepoints.txt`](https://raw.githubusercontent.com/php/php-src/php-8.5.10/ext/mbstring/common_codepoints.txt)
and regenerate its portable bitmap with:

```bash
python3 scripts/mbstring/generate_detection.py /path/to/common_codepoints.txt
```

The generator checks the source SHA-256 and writes an output manifest. Retain
`crates/elephc-mbstring/NOTICE.md`, `src/detect/data/LICENSE-PHP`, and the Unicode
notice in distributions containing their data. Detection tests compare independent
PHP verdicts, including candidate-order weighting, strict/default settings, all
single bytes, complete byte pairs for three competing groups, and full encoding
lists. Automatic-conversion tests additionally check source filtering and diagnostic
order. Single-string detection and the new multi-string entry use the same scorer.
The multi-string fixture contains 9,260 independent PHP conversion verdicts,
including reverse source traversal, retained stateful-decoder state, a separate
validation penalty for each input, per-string order weighting, and unweighted
catalog identity. Source boundaries remain intact; concatenating the inputs would
change malformed-unit handling, BOM skipping, and decoder shifts.

Conversion-error fixtures contain 174,748 output/count comparisons across all 79
source and destination names, malformed inputs, and seven substitution settings.
The counter includes failures while encoding replacement characters. Request tests
separately verify accumulation, failed detection, scrub, reset, and thread isolation.

Array fixtures contain 3,670 complete result/warning/count comparisons. Graph
descriptors preserve shared identities and cycles; results retain binary keys,
numeric-string versus integer keys, float bits, insertion order, and first-wins
conversion collisions. The same graph representation belongs to the neutral wire
contract, whose decoder checks complete framing before the engine traverses it.
`mb_check_encoding` now consumes these graphs through both AOT and eval. Host-reader
and C-ABI tests additionally verify borrowed scratch-buffer lifetimes, cycles,
shared identities, malformed metadata, and request-state reentry. Native C fixtures
exercise the emitted machine-code reader against independent indexed/hash layouts.
Other array-bearing conversion and settings APIs still require their public adapters.

Two traversed self-references cause PHP 8.5.10 conversion to recur until process
failure. The generator excludes that conversion shape except when strict detection
skips the first malformed key. The engine detects repeated active protection states
and returns an explicit nontermination error, preserving counts already accumulated.
This failure is tested separately and is not counted as a successful PHP comparison.

Parameter-coercion fixtures contain 42,400 weak/strict PHP calls with complete
results, diagnostics, callback counts, and final settings. They cover all current
scalar/array parameter shapes, 2,048 deterministic random float bit patterns,
decimal grammar and integer limits, binary strings, null, resources, and Stringable
success/failure. The tests run both the pure planner and the versioned preparation
C ABI before actual operation dispatch. Float formatting and Stringable execution
are explicit host actions in these tests, so this does not validate the production
host formatter, callback boundary, or propagation of caller strictness.

The same generator captures 75 wrong-arity PHP messages for the 42 shared functions.
Separate C tests cover malformed metadata, binary class names and diagnostic
framing, repeated release, and preparation while request state is exclusively
borrowed. `coercion_order.json` captures 108 callback/diagnostic ordering cases,
including array references versus copy-on-write, scalar argument copies, throwing
error handlers, and language changes during lazy encoding-list traversal.
Incremental-list tests consume 39 of those cases through the real parser, state,
detector, and converter. The shared invocation C ABI additionally consumes all 66
outer-argument cases through an independent callback host and real request dispatch.
Every callback reenters the request API; results, diagnostic/callback traces, caller
references, array COW, precision ordering, and final settings match PHP. The host
fixture models Stringable execution and float formatting explicitly. Production
native/eval integration, native destructor cleanup, and the three strict outer
conversion cases for list-valued conversion remain pending.

Invocation tests also inject 54 failures or malformed successes across argument
copying, metadata lookup, Stringable/float conversion, diagnostic delivery, array
reading, and owner release. They require balanced ownership after every failure,
including owners published with an error status. A library regression deliberately
panics at request dispatch and verifies complete cleanup and a subsequent successful
call. Separate tests reject incomplete callback tables and validate wrong arity
before reading argument or host pointers.


The numeric-entity map fixture `entity_maps.json` records 86 calls against PHP
8.5.10, including map element warnings, numeric prefixes, nonfinite floats,
integer bounds, overflow, and rejected values. The invocation test checks every
result and diagnostic in strict and weak callers. Additional protected-host
cases cover diagnostic exceptions, changes to later references, fixed encoding
selection with updated substitution, and ownership on callback failure.
The coercion rules follow PHP's `make_conversion_map()` and `zval_try_get_long()`;
see [the PHP source](https://github.com/php/php-src/blob/PHP-8.5/ext/mbstring/mbstring.c)
and [integer conversion helpers](https://github.com/php/php-src/blob/PHP-8.5/Zend/zend_operators.h).


Public encoding detection now carries the host's cached `mb_list_encodings()`
identity separately from candidate contents. Native and opaque eval regressions
cover shared copies, reconstructed lists, mutations restored to identical names,
callback changes to later references, and stable ownership across repeated calls.
The protected-host fixture checks V1 packed catalog compatibility and V2 catalog
identity results. A single-worker web test additionally checks twelve consecutive
requests, resetting catalog ownership and request settings before reuse.


`mb_convert_encoding` now joins the shared string and recursive-array engines
through both public backends. Array results cross a validated graph boundary and
use the allocator-neutral `elephc_mbstring_restore_v1` adapter: children finish
before parents, list-shaped nodes receive indexed storage, and other nodes retain
exact associative keys. Construction copies binary strings and independently owns
child values. Restore tests cover aliases, scalar bits, exact keys, malformed
framing, cycles, and cleanup at every injected construction failure. Public tests
also distinguish numeric string keys from integers in JSON and foreach, preserve
COW copies, and compare repeated-call heap residuals.

The V3 invocation table adds protected graph-entry reads and original-identity
leases. Recursive input snapshots now resolve eval array references after outer
and source-list callbacks, using the same shared coordinator as native calls.
Traversal preserves exact keys, visits each input array identity once, and keeps
both copied values and original boxes owned until cleanup finishes. Existing V1
and V2 hosts retain their earlier callback contracts. Independent host tests cover
late reference updates, nested shared arrays, malformed cursors and end records,
missing callbacks, injected failures, pending exceptions, and panic cleanup.


Eval array variable copies propagate element-reference metadata into their new
boxes, and references created from closure captures follow the captured target
before its temporary activation ends. Public regressions combine these behaviors
with nested conversion and COW. Reference writes, references escaping ordinary
function-local scopes, and metadata retirement when array boxes are freed remain
under audit in the implementation ledger.

MIME decoding fixtures contain 82,373 complete headers. Every internal encoding is
covered, including permissive B/Q syntax, missing terminators, binary strings,
whitespace folding, cross-word charset state, and fixed question-mark replacement.
Additional deterministic inputs exercise all byte values, malformed stateful units,
SoftBank emoji escapes, UTF-7 surrogate state, and empty decoder calls at buffer
boundaries. UUENCODE output retains its PHP partial-group padding between calls.
The native tests cover the shared request state, named/callable forms, argument
coercion, and catchable dynamic arity/type errors in both AOT and opaque eval.

The MIME encoding oracle contains 71,154 PHP 8.5.10 cases consumed by the shared
encoder and ABI regression tests. It covers every source/output codec pair,
language defaults, explicit null and omitted options, B/Q selection, line folds,
separator truncation, stateful decoder restarts, and malformed input. This fixture
also covers composable pairs and deferred output around MIME trial-chunk and line boundaries.
SJIS-mac coverage includes every captured Apple hint composition, exact trial-call
state, and PHP's early return when compact compositions fill the decoded buffer
without filling an output line. ABI tests include validation before empty results,
nullable reflected parameters with nonnullable supplied-value parsing, and the
shared last-encoding deprecation cache.

The information oracle contains 5,040 PHP 8.5.10 cases covering every selector,
all twelve languages, substitution modes, detection settings, original MIME
expression bytes, omitted and coerced arguments, invalid selectors, and exact
ordered snapshots. Engine tests distinguish absent HTTP identification from false
and verify unsigned error-counter wrapping. Native/eval tests cover all result
shapes, named and dynamic callbacks, Stringable selectors, and balanced ownership.
The web regression verifies the conversion-error count resets between requests.
Host configuration setters exist in the shared state; wiring mbstring directives
into the public INI surface remains part of the implementation ledger.

The HTTP input oracle contains 2,176 PHP 8.5.10 cases: every possible one-byte
selector, invalid longer strings, nullable and weak arguments, eight configured
lists, repeated names, aliases, transfer encodings, and the complete catalog.
Shared state keeps the configured list, aggregate identification, and four source
identifications independent. Native/eval tests verify defaults, Stringable and
named calls, errors, and that text-setting changes preserve HTTP input candidates.
Request parsing and public INI updates must populate these fields through the
pending host integration, rather than copying state into individual adapters.

The shared query engine under `src/input` prepares the parsing stages needed by
`mb_parse_str`. Its 17,904 PHP cases cover raw NUL termination, binary percent
decoding, every byte in names and bracket keys, all 79 destination encodings,
substitution, detection fallback, startup-only limits, and display-error-dependent
nesting warnings. Raw empty input clears aggregate identification, while a
nonempty separator-only input can identify an encoding and return an empty array.
Exceeding the variable limit rejects the whole query before detection. Conversion
counts rejected units without resetting the request's existing total.

Name registration emits shared ordered steps for entering an array, storing a
value, or removing a root after nesting overflow. The owned graph host verifies
PHP's numeric keys, negative append counters, maximum-index failures, malformed
brackets, mangled-name rejection, and partial mutations. Native/eval hosts still
need to apply those steps to live caller storage, preserve exposed array aliases,
and handle destructor/diagnostic reentry and SAPI input filters. Public
`mb_parse_str` registration, argument/reference adaptation, core INI routing, and
its example therefore remain pending. Run the engine replays with
`cargo test -p elephc-mbstring --test detect_many --test parse_str`.

The neutral `mb_parse_str` contract and shared operation 82 now reach the V5
invocation coordinator. V5 extends the 120-byte V4 prefix to 144 bytes with
configuration, optional filtering, and live registration callbacks. The source
is copied/coerced by value; the required output is pinned independently. Output
initialization precedes settings capture. A pending callback exception suppresses
later user diagnostics while the PHP body continues conversion and writes;
aggregate input identification is committed after the body. Fatal host failures
retire all owners and retain any earlier pending exception. Copied-value calls
without the required V5 writer are rejected before touching caller storage.

An independent live table host replays all 17,904 parser cases and 94 PHP callback
traces through that actual ABI. It checks exposed root aliases, scalar replacement,
destructors, nested parsing, INI changes, partial output, and exception timing.
Error handlers still observe suppressed INI deprecations during destructors;
the replay preserves literal string identity through the shared INI protocol.
Two PHP workers failed to produce valid traces and remain explicitly excluded
in `parse_str_reentry_excluded.json`; they are not compatibility evidence and
the generator does not retry recorded failures automatically. Their cause and
native/eval behavior remain unverified.

The separate display-policy oracle confirms a destructor can change
`display_errors` while a nested query write removes a root. V5 reads that policy
after registration, before deciding whether to emit the nesting warning. Failure
injection covers owned configuration/filter bytes, invalid readiness/flags,
unsupported host versions, pending filters, writer cleanup, and an earlier
throwable surviving a later fatal host response. Run these shared host checks
with `cargo test -p elephc-mbstring --test invoke mbstring_query_`. The contract
still reports `ReferenceAdaptersPending` for both public backends until their
storage callbacks and PHP-visible bindings are integrated.

The mbregex corpus pins PHP 8.5.10 with Oniguruma 6.9.10. Its 8,279 records include
3,625 option cases, 426 encoding alias cases, and 4,228 anchored/search cases.
The native transport test compares 4,098 matching cases, exact compile warnings,
empty/unmatched groups, duplicate named groups, Unicode, binary inputs, and all
supported PHP syntax selectors. The remaining 130 records describe mb_ereg's
empty-pattern argument error, which belongs to its pending public adapter.

The Rust regex engine owns PHP option parsing, alias-sensitive validation, and
capture copies. The managed Oniguruma package owns the opaque native provider;
its compiled-pattern and region allocations have independent paired frees.
Per-call limit fields distinguish explicit zero from retaining native defaults.
The provider's process-lifetime registration refuses incomplete or different
callback tables. Compiled pattern owners stay thread-confined.

Run the settings and limit tests with `cargo test -p elephc-mbstring --test regex`.
The real-provider test is explicitly ignored by default because it requires
native files. Set `ELEPHC_ONIGURUMA_TEST_PREFIX` to a managed artifact prefix to
test its exact static archives, or provide Oniguruma 6.9.10 development files
through pkg-config. Run `cargo test -p elephc-mbstring --test regex --test regex_request -- --include-ignored`.
The managed-native CI smoke job selects the managed prefix on all three execution
hosts and builds the device and Simulator providers on its macOS runner.
The shared regex session owns the byte-keyed pattern cache, progressive subject,
position, and captures. Native syntax defaults participate in cache comparisons;
changing a cached pattern's profile invalidates dependent progressive state.
The live PHP encoding alias still validates every cache lookup. Invalid initial
subjects remain available for progressive native searches, including successful
end-anchor matches after failed initialization. Every shared subject retains
initialized native lookahead padding, including valid text searched from an
offset inside a character. Progressive searches reuse that allocation. Provider
callers without retained padding receive a bounded copy before native matching.

`python3 scripts/mbstring/capture_regex_request.py` records 578 independent PHP
request traces. They cover settings, repeated searches, cache replacement, empty
numeric versus named captures, every byte offset inside multibyte characters,
malformed UTF-8/UTF-16/UTF-32,
execution limits, and warning callbacks that reenter regex operations or throw.
Progressive option errors retain the parsed syntax, discard uncommitted flags,
and can precede further state changes. The coordinator emits exceptions and
diagnostics in order without holding a state borrow; host adapters suppress
warning delivery while a PHP exception is pending.

The shared result protocol preserves multiple engine exceptions as oldest-first
class/length/message records in `RESULT_EXCEPTION_CHAIN`. Single errors retain
their existing result kinds. `elephc_mbstring_exception_at_v1` validates the whole
buffer before exposing a borrowed record; the native materializer copies binary
messages and links each new Throwable to the earlier chain. Wire ownership stays
with the caller until explicit release. Contract, bridge, and native heap-debug
tests cover binary messages, class order, previous links, and malformed trailing
records. Native Throwable methods expose previous links to opaque eval as owned
nullable values, including when the caught root is released first. These checks
do not establish Stringable/destructor ordering parity, which remains pending.

Session reset frees the request's patterns, subject, and captures and restores
the configured regex default (normally UTF-8), with its canonical byte validator.
The INI hook updates that default independently of the live encoding; unsupported
names reset the default to UTF-8 while retaining the live encoding. Ordinary
mb_internal_encoding setters do not change regex settings.
PHP retains regex option defaults across requests in the same worker;
a new session starts with `pr`. Ownership tests retain active operation results
across cache replacement and verify that final owner release retires their
patterns and subjects. `python3 scripts/mbstring/capture_regex_worker.py` records
four configurations across consecutive HTTP requests in a single PHP worker,
including the SJIS-WIN to canonical SJIS validation change at request shutdown.
The public `mb_regex_encoding` and `mb_regex_set_options` bindings now use the
same thread-local session through AOT, opaque eval, and callable dispatch.
The settings ABI replays all 4,051 recorded option/alias observations, including
exact binary errors and preservation of the previous setting on failure.
`regex_ini_abi` checks configured startup aliases, protected warning reentry,
core encoding handler order, thread isolation, and option persistence through
the exported request reset. Live INI handlers update regex state after their
ordinary internal-encoding diagnostics and before later input/output handlers.

The public `mb_ereg_match` binding now uses that same session and the managed
Oniguruma 6.9.10 provider. Its anchored matches, encoding validation, default
options, and binary diagnostics pass all 2,114 recorded matching cases through
the exported ABI. Native and opaque eval tests cover named arguments (including
PHP's `$string` parameter), callable forms, Stringable side effects, invalid
options, and the provider availability gate. Protected host tests exercise
nested matching from warning callbacks and balanced cleanup after pending errors.
PHP's `set_error_handler` surface is not yet available in the compiler, so those
warning callback tests use the actual shared C invocation boundary.

The seven public `mb_ereg_search*` functions use the same session and invocation
path. The exported protected ABI replays all 578 progressive traces, including
warning reentry, pending errors, and exact previous chains. Position results and
capture arrays cross the shared `ArrayGraph` boundary as independent copies;
numeric and named keys, unmatched groups, and empty strings retain PHP's types.
Native and opaque eval tests cover named and callable arguments, partial state
changes after errors, shared positions and captures across both backends, and
retained arrays after later searches and copy-on-write mutation. Heap-debug tests
also retain previous exceptions after releasing their caught roots.

Guarded `array|false` variables retain their original assignment contract while
`is_array` narrows reads. A loop can assign its next search result, including
`false`, back to the same variable. Regressions cover loops, conditional branches,
early-return guards, function-scope isolation, and unchanged declared-array restrictions.

The public `mb_split` operation uses that session's pattern cache and returns
independent field arrays through `ArrayGraph`. Its 1,871 PHP traces cover signed
limits, empty-match byte advancement, delimiter captures, multibyte encodings,
invalid subjects and patterns, search limits, cache invalidation, and reentrant
or throwing warning handlers. Both the shared engine and the protected public ABI
replay every trace. Native/eval tests additionally cover default and named arguments,
callables, Stringable side effects, diagnostics, and copy-on-write ownership.

`MB_ONIGURUMA_VERSION` comes from the same reviewed version constant as the native
package catalog. Static PHP references and opaque eval expose it without requiring
a matching call. The neutral catalog's complete set of nine mbstring constants is
checked against the PHP reflection snapshot.

The shared `mb_ereg_replace` and `mb_eregi_replace` operations retain the regex
encoding at invocation entry, before Stringable conversions. Subject validation
and replacement scanning use that encoding, while pattern compilation uses the
live settings after conversions. Replacement failures distinguish null invalid
subjects from false regex errors. Numeric and named backreferences preserve
unmatched groups, duplicate names, numbered-capture restrictions, and raw encoding
widths; empty matches advance by one byte. The independent replacement corpus
contains 1,431 PHP traces replayed by both the shared engine and the protected
public invocation ABI, including warning-handler reentry and cache invalidation.

The shared `Session::capture` engine for `mb_ereg` and `mb_eregi` is covered by
718 independent PHP traces in `regex_capture.jsonl.gz`. Empty patterns fail before
initializing the optional output; later failures retain the initialized output.
Encoding, options, and search limits are read after initialization, which may
execute a destructor. Numeric and named empty or unmatched groups become false,
and native search failures return false without a warning.

Another 1,200 PHP traces in `regex_output.jsonl.gz` cover typed and untyped output
references, destructor reentry, pending exceptions, and copies made during
initialization. An untyped output exposes null to its old object's destructor;
an assignable typed property exposes the new array first. Typed constraints reject
an incompatible assignment before releasing the previous value. A destructor can
leave an exception pending while matching and capture insertion still proceed.
For typed properties, capture insertion preserves the exposed construction array,
its destructor-added entries, and by-value copies made during that destructor.
Both the Session replay and the protected `elephc_mbstring_capture_v1` entry replay
all 1,918 traces. The latter uses the common argument-coercion planner and ownership
arena with a V4 host. Output references are pinned without copying their old value.
Three additional callbacks initialize the output, fill its retained construction
array from an ordered capture graph, and release the writer. Readiness and pending
exception status remain independent. Failure injection checks writer cleanup,
malformed readiness, older-host rejection for supplied outputs, and retained V1
support when the output is omitted; a Rust-panic test verifies writer cleanup.

The independent host models PHP lvalues. Direct AOT and opaque eval bindings
accept omitted outputs and persistent locals or aliases, including named
arguments and destructor reentry. Eval also supports reference parameters,
named dynamic calls, and explicitly referenced unpacked output elements.
Direct eval `call_user_func` syntax warns for a supplied output and passes a
temporary reference around its independent value; its destructor can run
during capture initialization without mutating the caller's variable.
Explicit output references in `call_user_func_array` preserve caller identity.
Ordinary call-array outputs use temporary references with PHP warnings, including
named arrays and dynamic callback wrappers. Direct syntax releases a temporary
argument array before capture initialization; dynamic wrappers retain that array
until the outer call returns. A destructor that changes regex options verifies
the resulting difference in match results against PHP. Reflected invocation,
`array_map`, nested wrappers, computed callback names, and repeated successful or
failing calls also exercise the shared owned-argument boundary. Concatenated names
release both source operands and native conversion buffers; Stringable and
ternary cases cover destructor order and condition-owner cleanup.
The frontend regression group `test_mbstring_regex_capture_` exercises real
calls, captured owner cleanup, and explicit diagnostics for untracked AOT forms.
Raw AOT reference parameters, property/array destinations, AOT runtime-selected
callables, legacy native reference slots, and non-reference unpacked output
adaptation remain incomplete.
Deferred output owners also need PHP-compatible shutdown ordering for nested
captures. Run both shared replay layers with the managed provider and
`cargo test -p elephc-mbstring --test regex_request --test invoke regex_capture -- --include-ignored`.

Direct regex operations and reachable regex callables select the managed `oniguruma`
package independently of PCRE2. `--with-mbstring` also enables that provider for
opaque eval and runtime-unknown callbacks; run `elephc native add oniguruma` in
the project before linking. Text operations and regex settings alone need only
the shared Rust bridge. The old PCRE2-based matching adapters have been removed.

Public `mb_ereg_replace_callback` now has AOT and eval bindings, bringing public
binding coverage to 63 of 65 functions. Its focused public call, ownership,
effects, and error tests pass. This is not complete callback support: the native
retained-owner regression still reports two unreleased string owners per regex
invocation in the exercised callback shape, and closure and method callback
frontend coverage remains open. `mb_convert_variables`, `mb_send_mail`, public
INI host integration, and the earlier compatibility debts are still pending.

The three native CI archive jobs prepare `target/debug/elephc-oniguruma` with
`scripts/ci/prepare_oniguruma_tests.py`. Nextest packages the complete managed
project and source/artifact cache and fails if that directory is absent.
Archived fixtures use locked offline installation and the production resolver;
an incompatible toolchain fingerprint triggers a rebuild from verified cached
source. The runtime and native caches remain separate. Release and nightly
artifact probes also install Oniguruma before testing `--with-mbstring`.
