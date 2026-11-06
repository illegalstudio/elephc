---
title: "PHP 8.6"
description: "PHP 8.6 language and library support in elephc: what ships, what changed in the runtime, and what remains."
sidebar:
  order: 22
---

<!-- Keep this page in sync with ROADMAP.md and the php-src 8.6 corpus under tests/php_src. -->

PHP 8.6 is a minor release. elephc keeps the existing `--php-version` profiles and
adds the 8.6 behaviour behind `--php-version=8.6` where the change is profile
sensitive.

## Delivered

| Feature | Notes |
|---|---|
| `Time\Duration` | `final readonly` value class with `seconds`, `nanoseconds`, `negative`; every `from*()` factory; `negate()`, `absolute()`, `add()`, `sub()`, `multiplyBy()`, `divideBy()`, `compare()`; comparison operators through the internal object handler; overflow raises `Time\TimeException`; serialization and `__unserialize()` validation. The php-src `ext/date/tests/time/duration` corpus is migrated into `tests/codegen/time_duration_php_src.rs` and the serialization fixture into `tests/codegen/time_duration_serialization.rs`. |
| `Time\TimeException` | Extends `\Exception`; every overflow and ISO-8601 parse failure uses php-src's message text. |
| `clamp()` | Full `mixed` signature. Int, float, and string operands keep the typed runtime path; boolean, null, array, and object operands go through php-src's own polyfill, so PHP's cross-type comparison rules apply. `DateTime`/`DateTimeImmutable` gained the internal comparison adapter this needs. The upstream `ext/standard/tests/math/clamp.phpt` fixture is transcribed in `tests/codegen/math/clamp_php86.rs`. |
| `grapheme_strrev()` | Now classified as `ext/intl` (PHP 8.6) instead of an elephc extension. Both upstream `ext/intl/tests/grapheme_strrev*.phpt` fixtures pass, including the UBRK_DONE regression and the Arabic grapheme-cluster case. |
| Default trim mask | `trim()`, `ltrim()`, `rtrim()` and `chop()` include form feed (`\x0C`) under `--php-version=8.6`, and leave it untouched on 8.5 and earlier. |
| Readonly property defaults | A readonly instance property may declare a default value, including inside a `readonly class`. The default initializes the slot before the constructor runs, so later writes (including from the constructor) raise `Cannot modify readonly property`. Inheritance, get-only interface properties, and serialization follow php-src's `Zend/tests/readonly_props/readonly_with_default*.phpt`. |
| Stream error API | `StreamErrorMode`, `StreamErrorStore`, `StreamErrorCode`, final readonly `StreamError`, `StreamException::getErrors()`, `stream_last_errors()`, `stream_clear_errors()`, and the `stream.error_mode` / `error_store` / `error_handler` context options. `fopen()` runs through an error-aware wrapper: Error mode warns, Exception mode throws, Silent suppresses, and the store/handler policies apply. User-wrapper `stream_read()` excess data is truncated to the requested chunk. Under 8.6 the runtime records the event in program globals, and injected `fread()` / `stream_get_contents()` wrappers build `UserspaceInvalidReturn` errors, apply the stream context's mode/store/handler policy, accumulate every read in one operation, and preserve an outer operation's pending errors across nested reads. A `stream_select()` wrapper reports the two terminating `stream_cast()` errors (`NotImplemented` and `CastNotSupported`) with the same policy. `fwrite()` reports missing or over-reported `stream_write()` calls, `file_get_contents()` dispatches missing reads for user wrapper schemes, and `stream_context_set_option()` keeps the per-context option map current. Earlier profiles keep emitting the legacy warning directly. The runtime now recognizes `php://memory` / `php://temp` for paths known only at run time, and `$GLOBALS['name']` reads resolve to the program-global slot from any scope. The 18-fixture upstream corpus is vendored; `tests/codegen/io/stream_errors_corpus_php86.rs` locks in all 14 runnable fixtures and records the 4 `--SKIPIF--`-only fixtures; the runner ignores blank-line interleaving and names the concrete exception class where the checker cannot dispatch through `Throwable`. |
| `pack()` / `unpack()` endianness | `<`/`>` modifiers on `s`, `S`, `l`, `L`, `q`, `Q`, `f`, and `d`, with the upstream rejection messages for inherent-endian and unsupported codes. The base `pack`/`unpack` family is implemented as an AOT prelude, including named unpack elements, offsets, repeats, padding, integer widths, and IEEE-754 floats through the `elephc_pack` bridge. The upstream `pack_endian_modifiers*`, `unpack_endian_modifier_names*`, `pack_float`, `pack_A`, `pack_Z`, `pack_arrays`, `unpack_offset`, and `unpack_error` fixtures are represented in `tests/codegen/strings/pack_unpack_php86.rs`; 10 focused tests pass. |
| `Io\Poll` | `Io\Poll\Backend`, `Event`, `Handle`, `Watcher`, `Context`, global `StreamPollHandle`, and the `PollException` hierarchy. The portable `Poll` backend reports level Read/Write readiness, buffered data, one-shot rearming, `maxEvents`, `Time\Duration` timeouts, duplicate/closed/invalid handles, and watcher/context lifetime rules. Thirty-two upstream `ext/standard/tests/poll` fixtures are vendored and represented by 24 focused tests in `tests/codegen/io/io_poll_php86.rs`; edge triggering, hang-up precision, and descriptors above 63 remain backend limits. |
| URI builders and types | `Uri\Rfc3986\UriType`, `Uri\Rfc3986\UriHostType`, and `Uri\WhatWg\UrlHostType` enums; `getUriType()`, `getHostType()`, and `isSpecialScheme()`; `Uri\Rfc3986\UriBuilder` / `Uri\WhatWg\UrlBuilder` with reset/setters/base builds; `Uri\WhatWg\UrlPercentEncodingMode` and `url_percent_encode()`. `tests/codegen/uri86_php_src.rs` runs the vendored PHP 8.6 builder/getter corpus: 198 pass, 6 are skipped for opaque-path spaces, the mixed-object `array_map()` comparison, and the typed-property by-reference shape. |
| Parameter doc comments | `/** ... */` before a parameter or after it before the comma is retained as `ReflectionParameter::getDocComment()` for functions, methods, constructors, promoted parameters, closures, arrow functions, and property-hook `set` parameters. The indented upstream fixture passes; the basic fixture is transcribed with its closure/arrow reflection cases removed until dynamic `ReflectionFunction` construction is supported. |
| Partial function application | `foo(1, ?, ...)` placeholders, named placeholders, trailing `...`, constant-expression partials, inherited parameter names/types/defaults/reference modes, creation-time evaluation, extra-argument forwarding, and closure scope. Ten php-src `Zend/tests/partial_application` fixtures are transcribed into `tests/codegen/oop/partial_application_php86.rs`; the invalid shapes are rejected in `tests/error_tests/partial_application.rs`. |
| `ReflectionProperty::isReadable()` / `isWritable()` | Report whether a property can be read or written from a class scope, with the optional object adding initialization and readonly state. Visibility, asymmetric write visibility, virtual/backed hooks, `__get`/`__set`/`__isset` fallbacks, static properties, and the readonly `__clone` reinitialization window are covered by the six upstream `ext/reflection/tests/ReflectionProperty_is{Readable,Writable}_*.phpt` fixtures in `tests/codegen/oop/reflection_property_access_php86.rs`. This also implements PHP 8.5's asymmetric visibility for static properties. |
| `mysqli_quote_string()` | Escapes a string through `mysqli::real_escape_string()` and wraps it in single quotation marks, as the method `mysqli::quote_string()` and the procedural alias. Covered by the unconnected-object test and an opt-in live MySQL test. |
| Secure session defaults | Under `--php-version=8.6`, `--web` starts with `session.use_strict_mode=1`, `session.cookie_httponly=1`, and `session.cookie_samesite=Lax`. A later `ini_set()` can relax them for the request. The session profile matrix in `tests/web_session_tests.rs` covers 8.2 through 8.6. |
| Constructor/destructor return deprecation | A `__construct()`/`__destruct()` that returns a value or contains `yield` emits PHP 8.6's `Deprecated:` diagnostic at startup. The upstream `Zend/tests/magic_methods/constructor_destructor_return.phpt` fixture is transcribed into `tests/codegen/oop/constructor_deprecation_php86.rs`. |
| Enum `__debugInfo()` | An enum case renders `enum(E::C) (N) { … }` from the array returned by `__debugInfo()`. The three php-src `Zend/tests/enum/debugInfo/{backed_enum_value,magic_method,missing_magic}.phpt` fixtures are migrated into `tests/codegen/oop/enum_debug_info_php86.rs`; the visibility-warning fixture remains out; the parameter and return-type failures are covered by `tests/error_tests/enum_debug_info.rs`. |
| `#[\Override]` on class constants | Class, interface, enum, enum-case, and trait-composed constants must have a matching parent/interface constant. The php-src `Zend/tests/attributes/override/constants` fixtures for classes, interfaces, enums, cases, and traits are migrated into `tests/codegen/oop/override_constants_php86.rs` and `tests/error_tests/override_constants.rs`. |
| `SortDirection` | `Ascending` / `Descending`, catalogued as a PHP 8.6 core enum. |
| `array_filter()` mode validation | An invalid `$mode` raises `ValueError` on the 8.6 profile instead of silently using value mode. |
| `json_decode()` error positions | Token coordinates in decoder messages follow PHP 8.6. |

## Deliberately deferred

These PHP 8.6 items are not implemented yet. Each has a tracked follow-up; the
list is here so a PHP 8.6 program that depends on one fails visibly during review
instead of during production.

| Item | Why it is deferred |
|---|---|
| Direction-aware property resolution, const object property writes, function-argument error display, closure optimizations, TLS session resumption, SNMP updates | Smaller upstream changes with no standalone user-visible surface in elephc yet. |

## Known deviations

- The PHP 8.6 URI builder corpus passes 189 of 204 fixtures. The 15 skipped fixtures
  pass an uninitialized variable or typed property to a by-reference `$softErrors`
  parameter; the checker requires mixed/union/nullable storage for that call shape,
  so those fixtures are excluded until the by-reference storage rules widen.
- `pack()`/`unpack()` are declared by an AOT prelude. Literal `eval` source that names them
  gets the prelude through usage detection; opaque dynamic eval source needs `--with-pack`. The prelude's float conversions link the small
  `elephc_pack` bridge, so a program that never calls `pack`/`unpack` links nothing.
- `Io\Poll` bundles only the portable `Poll` backend. Edge triggering, precise
  `HangUp`/`ReadHangUp` reporting, and descriptors above 63 are not available; the
  platform-specific `Epoll`, `Kqueue`, `EventPorts`, and `WSAPoll` backends raise
  `BackendUnavailableException`. `get_debug_type()` is not implemented, so the
  upstream closed-handle fixture is adapted to use `isValid()` instead.
  `Watcher::getHandle()` returns the concrete `StreamPollHandle` type rather than the
  `Io\Poll\Handle` interface so the checker accepts the upstream
  `getHandle()->getStream()` pattern; the returned object still implements the interface.
- The stream error surface wraps `fopen()` only. Other stream operations still emit PHP 8.5
  warnings, and user-wrapper operations that report several errors (`stream_select()` cast
  failures, over-reading `stream_read()`) are not accumulated. `stream_select()` accepts the
  new sixth `$context` argument but ignores its error options. Warnings are written to stderr;
  silent and exception modes suppress the `fopen()` warning.
- Parameter doc comments are attached, but `ReflectionFunction` cannot yet be constructed from a
  closure or arrow function, so those two parameter lists are parsed and stored without a
  user-visible reflection path. Doc comments on properties and hook methods themselves are not
  retained; only parameter comments are.
- Partial application of a magic `__call` / `__callStatic` dispatch, or of a receiver whose
  class cannot be resolved at compile time, is rejected with a diagnostic rather than
  synthesized. `new C(?)` is also rejected, matching PHP. Bound values use a generated typed
  identity closure at creation time; in a `declare(strict_types=1)` file a mismatch may be
  coerced where php-src raises `TypeError`, and the hidden variadic parameter makes the
  generated closure appear variadic to reflection.
- `ReflectionProperty::isReadable()` and `isWritable()` expose `mixed $object` rather than
  `?object $object`, so the static checker accepts dynamic values such as `new $scope`.
  The initialization probe casts the object to an array and compares the mangled backing
  slot; a readonly initialized property is reported writable inside the matching
  `__clone` frame, which is php-src's reinitializable window.
- A structurally malformed serialized payload for `Time\Duration` (for example
  `O:13:"Time\Duration"::{}`) reaches the generic `unserialize()` warning and
  returns `false`; php-src wraps it in `Exception: Invalid serialization data`.
  Well-formed payloads with invalid fields go through `Duration::__unserialize()`
  and raise that exception.
- `new Time\Duration()` is rejected while compiling (the constructor is private),
  whereas php-src reports the same invalid program at run time. The factories are the
  only supported construction path in both implementations.
- A readonly property's default is enforced by the static checker for named
  writes. Dynamic-name writes (`$object->$name = …`) and `unset()` do not yet
  consult readonly state, matching the pre-existing dynamic-property gap rather
  than the 8.6 defaults change.
- The constructor/destructor deprecation helper runs once at startup and scans only
  unconditional top-level/namespace/include-once declarations. A class declared inside a
  runtime branch emits the diagnostic early instead of when that branch executes.
- An enum's `__debugInfo()` is rendered by `var_dump()` and `print_r()`; the magic-method
  visibility warning is not emitted yet.
- A trait composition that redeclares an `#[\Override]` constant with a different value is
  rejected as an incompatible duplicate trait constant before the override rule runs, so
  the diagnostic differs from php-src's override message while the program is still
  refused.
- `grapheme_strrev()` implements the common `Grapheme_Extend` ranges (Latin,
  Hebrew, Arabic, Indic, Thai, Tibetan, variation selectors, emoji modifiers) but
  not the complete UAX #29 rule set (`SpacingMark`, `Prepend`, regional
  indicators). The upstream fixtures pass.

## References

- [PHP 8.6 RFC index](https://wiki.php.net/rfc#php_86)
- [`Time\Duration` RFC](https://wiki.php.net/rfc/duration_class)
- [`clamp()` RFC](https://wiki.php.net/rfc/clamp_v2)
- [Readonly property defaults RFC](https://wiki.php.net/rfc/readonly_property_defaults)
- [Partial function application v2 RFC](https://wiki.php.net/rfc/partial_function_application_v2)
- [PHP 8.6 essentials (Les-Tilleuls.coop)](https://les-tilleuls.coop/blog/ce-quil-faut-retenir-des-nouveautes-de-php-8-6)
