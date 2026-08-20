# iconv extension support (AOT + Magician)

Bring PHP's complete `iconv` extension surface — its 10 functions plus its 4 constants —
to elephc, on both the AOT compilation path and the Magician `eval()` runtime, for every
supported target (`macos-aarch64`, `linux-aarch64`, `linux-x86_64`).

Ground truth for every semantic decision is PHP 8.3 CLI plus `php-src`'s
`ext/iconv/iconv.c`, which the engine follows function by function.

## Tasks

- [x] 1. `crates/elephc-iconv` bridge staticlib: one Rust engine over the platform `iconv`
- [x] 2. Bridge registration in `src/linker/bridges.rs` and `src/link_plan.rs` (`--with-iconv`)
- [x] 3. Shared builtin contracts for the 10 PHP functions plus neutral requirements
- [x] 4. AOT: typed `RuntimeFnId` targets, builtin home files, bounded dispatch group, lowering
- [x] 5. AOT: `__rt_iconv_*` runtime helpers on both architectures
- [x] 6. `ICONV_*` constants on the AOT path
- [x] 7. Magician bindings for the 10 PHP functions plus the `ICONV_*` constants
- [x] 8. Tests: crate unit tests, codegen, error, eval-parity, heap-debug, extension-loaded, CLI
- [x] 9. Example under `examples/iconv/`
- [x] 10. Docs: `docs/php/iconv.md`, CLI reference, linking page, generated builtin docs,
      compatibility table, architecture tree, README, CHANGELOG

## 1. Bridge crate

`crates/elephc-iconv/` is a `staticlib` + `rlib` workspace member with no third-party
dependencies; it binds libc `iconv_open`/`iconv`/`iconv_close` directly and links
`-liconv` on macOS through the `BuiltinRequirement::MacOsLibrary("iconv")` mechanism
`mb_strlen` already uses.

Single-scope modules:

| File | Owns |
|---|---|
| `lib.rs` | crate surface re-exports and module wiring |
| `error.rs` | `IconvError`, its severity, and php-src's diagnostic wording |
| `ffi.rs` | the libc declarations, the `Converter` RAII wrapper, and the UTF-8 `LC_CTYPE` setup |
| `convert.rs` | `iconv()`, including php-src's own `//IGNORE` skip loop |
| `text.rs` | the fixed-width UCS-4LE view the character-oriented functions index through |
| `search.rs` | `iconv_strlen`, `iconv_substr`, `iconv_strpos`, `iconv_strrpos` |
| `encoding_state.rs` | the process-wide input/output/internal encoding trio |
| `mime/base64.rs` | RFC 2045 base64 for `B` encoded-words |
| `mime/quoted_printable.rs` | RFC 2047 `Q` encoding and php-src's cost table |
| `mime/encode.rs` | `iconv_mime_encode` and its line-budget folding |
| `mime/decode.rs` | `iconv_mime_decode` / `iconv_mime_decode_headers` scanner |
| `abi/mod.rs` | the panic-free `elephc_iconv_call` / `elephc_iconv_release` C ABI |
| `abi/args.rs` | the uniform staged argument block |
| `abi/result.rs` | the result block and the packed array encoding |
| `abi/dispatch.rs` | opcode → Rust API dispatch and diagnostic packing |

Magician links the same crate as an `rlib`, so both backends share one implementation
and one copy of the encoding trio.

### Behaviors that needed php-src rather than the manual

- An omitted or `null` `$encoding` resolves to `iconv.internal_encoding`; an explicitly
  empty one resolves to `default_charset`. The two are different charsets.
- `iconv_strpos()`/`iconv_strrpos()` scan one character at a time and stop at the first
  match, so a match found before a malformed tail is still reported — but a conversion
  failure recorded while producing the matching character suppresses it.
- An empty `$needle` is answered before anything is converted, so it never reports a
  diagnostic and never raises the out-of-range `$offset` `ValueError`.
- `iconv_mime_encode()` sizes each encoded-word from the remaining line budget: `B` grows
  its reserved shift-sequence tail, `Q` shrinks its conversion by the escaped overflow
  divided by three, rounded up.
- The MIME decoders report their source charset as the literal `???` placeholder.
- glibc drives `//TRANSLIT` from `LC_CTYPE`, and the PHP CLI installs a UTF-8 one at
  startup, so the crate installs one the first time a converter is opened.

## 2. Linker

One `BridgeStaticlib` entry (`lib_name: "elephc_iconv"`, `flag_name: "iconv"`,
`php_extension: Some("iconv")`) plus one entry in Magician's embedded-bridge relationship,
so a program that links both Magician and the bridge keeps a single copy of the
encoding trio.

## 3. Shared contracts

Ten `BuiltinContract` entries in `crates/elephc-builtin-contract/src/catalog_data.rs`
(area `String`) matching PHP's parameter names, defaults, and declared returns.
`requirements.rs` maps all ten onto `Bridge("elephc_iconv")` + `MacOsLibrary("iconv")`.

## 4./5. AOT

Ten `RuntimeFnId` variants lower through one uniform staged argument block: the call site
reserves 272 bytes, clears every slot's presence flag, stages its arguments through the
shared target-neutral stack-field helpers, and calls `__rt_iconv_call` (or
`__rt_iconv_call_bool` for `iconv_set_encoding()`). The runtime helper calls the bridge
through `_elephc_iconv_call_fn`, prints any diagnostic through `__rt_diag_warning`,
materializes the result, and releases the bridge's payloads.

`iconv_mime_encode()`'s `$options` array is read at the call site with one
`__rt_iconv_mime_option` call per recognized key, because only the backend can see the
receiver's runtime storage. The receiver is resolved once — through
`__rt_iconv_option_table` when its static type is `mixed`, so a boxed cell is unboxed
first — and parked in the block's scratch slot, because each lookup clobbers the argument
registers.

Boxed results are stamped by hand rather than through `__rt_mixed_from_value`: that helper
persists strings and retains containers a second time, and both payloads here are already
owned, so routing through it leaked one allocation per call.

## 6./7. Constants

`ICONV_MIME_DECODE_STRICT` (1) and `ICONV_MIME_DECODE_CONTINUE_ON_ERROR` (2) join the
predefined integer-constant tables on both paths. `ICONV_IMPL` follows the compilation
target (`glibc` / `libiconv`) and `ICONV_VERSION` reports the `unknown` spelling php-src
itself uses when it cannot identify its provider, because the runtime libc version is not
knowable while compiling ahead of time.

## 8. Verification

A 23,448-case differential corpus (every function, 12 charsets including `//TRANSLIT` and
`//IGNORE` variants, malformed and fuzzed inputs, all four decode modes, and the full
`iconv_mime_encode()` option matrix) was generated from PHP 8.3 and replayed through four
independent execution paths. All four agree with PHP on every case:

| Path | How it was run |
|---|---|
| Rust engine | `elephc-iconv`'s API directly |
| AOT, linux-x86_64 | compiled binary, host |
| AOT, linux-aarch64 | cross-compiled, run under real arm64 glibc in Docker |
| Magician `eval()` | the whole driver loop inside one `eval()` |

Cases the corpus cannot express were checked separately against PHP: named and
out-of-order named arguments, static string-keyed unpacking, namespaced and
case-insensitive calls, `function_exists()`, `ReflectionFunction`, `--strict-php`
visibility, first-class callables for all ten functions, every `$options` receiver shape,
`PHP_INT_MIN`/`PHP_INT_MAX` offsets and lengths, NUL-carrying binary payloads, 200-byte
charset names, 5,000-character payloads, and the whole encoding trio including its sharing
between compiled code and `eval()` in both directions.

### Bugs that verification found and fixed

- Union-returning builtins must declare `TypeSpec::Mixed` in the shared contract, as
  `strpos`/`gzuncompress` do. Declaring the narrow type worked for direct calls, where the
  check hook narrows it, but the first-class-callable wrapper reads the contract and
  handed the caller a raw pointer.
- A first-class callable reaches a builtin without any direct call recording that
  builtin's link requirements, so the program failed to link. The checker now reads the
  requirements off the shared contract when it binds a callable, exactly as a direct call
  does; this fixes every bridge-backed builtin, not just iconv's ten.
- `iconv_mime_encode()` silently ignored its `$options` when the receiver's static type was
  `mixed`, because only an associative-array pointer was accepted. The receiver is now
  resolved once through `__rt_iconv_option_table`, which unboxes a Mixed cell.
- Boxed results were routed through `__rt_mixed_from_value`, which persists strings and
  retains containers a second time, leaking one allocation per call.

## Known pre-existing limitations encountered

Neither is caused by this work; both are reproducible on `main` without any iconv call.

- A ternary whose branches call a user function produces a wrongly typed result slot, so
  `$a = $cond ? f(1) : f(2);` with `f(): string` yields `0`. Reproduces with no builtin
  involved at all.
- A runtime-`null` optional argument is passed as its zero value rather than as an absent
  argument, so `substr("hello", 1, $len)` with a runtime-null `$len` returns `""`. Core
  `substr()` behaves identically; `iconv_substr()` inherits the same convention.
- A positional array spread into a builtin (`iconv(...$args)`) is rejected at compile time.
  `str_replace`, `strpos` and `substr_replace` reject it identically; the documented static
  string-keyed form (`iconv(...["from_encoding" => …])`) works.

## Deliberate divergences from PHP

- `ICONV_VERSION` reports `unknown` rather than the runtime libc version, which cannot be
  known while compiling ahead of time. Both backends report the same value so a program
  cannot observe a difference between them.
- A negative or `PHP_INT_MAX` `line-length` makes php-src abort with an integer-overflow
  fatal; elephc reports php-src's `Buffer length exceeded` warning and `false` instead.
