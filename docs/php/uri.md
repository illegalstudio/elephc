---
title: "URI"
description: "PHP 8.5's ext/uri: RFC 3986 and WHATWG URL parsing, normalization, mutation, validation errors, and serialization."
sidebar:
  order: 26
---

elephc implements PHP 8.5's `uri` extension: `Uri\Rfc3986\Uri`, `Uri\WhatWg\Url`,
the `Uri\UriComparisonMode` and `Uri\WhatWg\UrlValidationErrorType` enums, the
`Uri\UriException` / `Uri\UriError` / `Uri\InvalidUriException` hierarchy, and
`Uri\WhatWg\InvalidUrlException` with its `UrlValidationError` list. Parsing and
normalization follow php-src's bundled uriparser and lexbor behavior, and the
public surface is exercised by the embedded php-src `ext/uri` case table.

The extension is pay-for-use: a program that never names `Uri\` neither declares
the classes nor links the `elephc_uri` bridge. `--with-uri` force-enables it for
indirect calls and opaque `eval()` source; it requires a PHP 8.5 or later profile,
because ext/uri does not exist on older profiles and the combination is rejected.
Whenever the surface is present, `extension_loaded('uri')` reports `true`.

## RFC 3986

```php
<?php
$uri = new Uri\Rfc3986\Uri(
    "HTTPS://user:pass@EXAMPLE.com:0443/a/../b?x=%7e#f"
);

echo $uri->toString(), "\n";
// https://user:pass@example.com:443/b?x=~#f
echo $uri->toRawString(), "\n";
// HTTPS://user:pass@EXAMPLE.com:0443/a/../b?x=%7e#f

echo $uri->withPath("/new")->toString(), "\n";
// https://user:pass@example.com:443/new?x=~#f
```

`get*()` methods return the normalized component; `getRaw*()` returns the
component as written. Normalization lowercases the scheme and registered host,
repairs percent escapes (decoding unreserved octets and uppercasing the rest),
removes dot segments, and canonicalizes IPv6 text in serialization. The
constructor and `parse()` accept an optional base URI; `resolve()` resolves a
reference against the receiver. A relative or scheme-less base is rejected.

`Uri\Rfc3986\Uri::parse()` returns `null` when the input is invalid, while
`new Uri\Rfc3986\Uri()` throws `Uri\InvalidUriException`. `with*()` methods return
a new object and never mutate the receiver.

## WHATWG URLs

```php
<?php
$url = new Uri\WhatWg\Url("https://user:pass@example.com:443/a/../b?q=1#f");

echo $url->toAsciiString(), "\n";
// https://user:pass@example.com/b?q=1#f
echo $url->toUnicodeString(), "\n";
echo $url->getAsciiHost(), "|", $url->getUnicodeHost(), "\n";

$errors = [];
$resolved = $url->resolve(" /next", $errors);
var_dump($resolved->getPath(), $errors[0]->type->name);
```

`Uri\WhatWg\Url` follows the WHATWG URL standard: IDNA hosts, default-port
removal, dot-segment and backslash handling, opaque paths, file URLs, IPv4/IPv6
serialization, and the standard percent-encode sets. `parse()` returns `null` on
failure and fills the optional `$errors` by-reference parameter; the constructor
throws `Uri\WhatWg\InvalidUrlException`. On success the same parameter receives
the soft validation entries (warnings), which is how `resolve()` reports inputs
such as a leading space that are accepted with a warning.

`getUnicodeHost()` returns the IDNA Unicode form while `getAsciiHost()` returns
the serialized ASCII/punycode form. `toUnicodeString()` changes only the host;
path, query, and fragment stay percent-encoded, as in php-src. Successful parses
also report lexbor's soft warnings: `InvalidCredentials`, a special scheme missing
its second solidus, leading-zero IPv4 parts (`Ipv4NonDecimalPart`), and removed
component whitespace. A forbidden domain code point inside a non-ASCII label is
rejected with `DomainInvalidCodePoint`.

## Validation errors

`Uri\WhatWg\UrlValidationError` exposes `context`, `type`, and `failure`.
`UrlValidationErrorType` includes all 29 php-src cases (`DomainToAscii`,
`Ipv4TooManyParts`, `PortOutOfRange`, `InvalidUrlUnit`, …). `InvalidUrlException`
carries them in its readonly `errors` array:

```php
<?php
try {
    new Uri\WhatWg\Url("http://localhost:99999");
} catch (Uri\WhatWg\InvalidUrlException $e) {
    echo $e->getMessage(), "\n";
    // The specified URI is malformed (PortOutOfRange)
    echo $e->errors[0]->type->name, "\n";
    // PortOutOfRange
}
```

The `InvalidUrlException` constructor validates that `$errors` is a list of
`UrlValidationError` objects and throws `ValueError` otherwise.

## Serialization and introspection

Both classes are opaque: `(array)$uri`, `json_encode($uri)`, and `var_export($uri)`
see no properties, matching php-src's internal classes. `var_dump($uri)` uses the
computed `__debugInfo()` and prints the eight components. `serialize()` uses
`__serialize()` and stores the raw URI string, so a round-trip through
`unserialize()` reconstructs the same value.

```php
<?php
$uri = new Uri\Rfc3986\Uri("https://example.com/a");
var_dump((array) $uri);   // array(0) {}
var_export($uri);
// \Uri\Rfc3986\Uri::__set_state(array(
// ))
```

## Compatibility notes

- The php-src `ext/uri` compliance cases are embedded as a Rust table in
  `tests/codegen/uri_php_src/cases.rs` and run by
  `cargo test --test codegen_tests_ext test_uri_php_src_bundle`.
- The `zend_test`-driven cases (`100`–`102`) exercise php-src's test-only
  `zend_test_uri_parser()` helper. The Rust harness supplies an equivalent helper
  built on the public `Uri\*` classes, so those cases validate the same parsing,
  raw/normalized component, large-port, and validation-error behavior without the
  `zend_test` extension.
- The OOM cases run with their recorded `memory_limit` heap ceiling and the fatal
  message matches php-src's `Allowed memory size of N bytes exhausted …` form. All 332
  embedded cases pass, including the eleven OOM cases.
- The URI implementations retain one path-segment node per segment (an estimated 32-byte
  node) plus copied text for non-empty segments, and the complete path string itself.
  Because that storage lives inside the Rust bridge rather than in the elephc heap, the
  prelude mirrors the footprint in hidden PHP arrays charged against `_heap_limit`:
  `4 * segments + 2 * non-empty segments` array cells (32 bytes per segment plus 16 bytes
  of text), plus one string header and its real byte payload for the path. Charging the
  path bytes keeps a single very long segment from hiding behind the per-segment node
  count. The charge arrays live in private static maps keyed by `spl_object_id`, are
  dropped by `__destruct`, and are rebuilt by `__clone` and `__unserialize`. The OOM heap
  ceiling and runtime `ini_set('memory_limit', …)` both observe the charge. The charge also
  mirrors the retained scheme, userinfo, host, port, query, and fragment strings as one
  header plus payload cells per string.
- A resolved WHATWG reference keeps the base URL's saved ASCII/Unicode host pair and its
  extra-solidus file path. `withScheme()`, `withHost()`, and other unrelated writes preserve
  that saved path; only a write that actually changes the path clears it.
- WHATWG validation warnings share the parsed input with byte ranges instead of copying the
  remaining URL suffix for every invalid byte, so a warning-heavy untrusted URL stays linear
  in the input size until the PHP error list is charged against `memory_limit`.
- Relaxed `xn--` host recovery also applies to absolute references parsed with a base,
  network-relative references, and `withHost()` writes, not only to a standalone parse.
- Relative references are scanned for lexbor's component warnings; the base scheme decides
  whether a path reverse solidus is `InvalidReverseSoldius` and query/fragment bytes remain
  `InvalidUrlUnit`.
- A direct `__clone()` call on a URI object raises PHP's `Call to undefined method …::__clone()`
  error instead of replacing and leaking the original native handle; the `clone` operator
  still produces an independent value.
- Case `057` passes a typed object property by reference. A compatible cell is shared for
  the call's duration, so a write through the alias is visible immediately; an incompatible
  concrete property type raises PHP's call-site `TypeError` and leaves the property unchanged.
- To let user code call `parse()` and `equals()` on values whose static type is
  `?Uri` (a common php-src test shape), the prelude keeps a wider internal `mixed`
  contract and performs the runtime `instanceof` check itself. Reflection reports
  php-src's declared `?Uri\…` parameter types, and a runtime wrong-type argument raises
  php-src's exact `TypeError` wording, including the `, <type> given` suffix.
- `with*()` clones the receiver before calling the native copy operation. The clone owns an
  independent duplicate of the original native URL; once the new handle arrives, the copy
  switches to it and frees the duplicate before charging the new URL. Cloning first keeps the
  receiver's handle intact if a PHP allocation fails, and the new native URL cannot leak
  across that allocation. This costs one extra native clone and free compared with php-src's
  `zend_object` copy; user-visible clone semantics are unchanged.
- `withScheme(null)` matches PHP 8.5.10: it removes the scheme without a deprecation notice.
- The general AOT by-reference limits documented in [PHP types](types.md) apply to URI calls.
- The classes are prelude-declared, but Reflection reports php-src's engine shape:
  `isInternal()`, `isReadOnly()`, and `isFinal()` are true and `getModifiers()` is
  `65568`. `newInstanceWithoutConstructor()` throws php-src's internal-final-class
  `ReflectionException`, and the parsing, getter, mutation, cast, serialization,
  and debug behavior matches. Assigning an undeclared property compiles and throws
  php-src's `Error: Cannot create dynamic property …` at run time; reading it afterwards
  reports `Warning: Undefined property: …` and answers `null`.
