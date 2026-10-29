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
public surface is exercised by the vendored `ext/uri` PHPT suite.

The extension is pay-for-use: a program that never names `Uri\` neither declares
the classes nor links the `elephc_uri` bridge. `--with-uri` force-enables it for
indirect calls and opaque `eval()` source. Whenever the surface is present,
`extension_loaded('uri')` reports `true`.

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

- The php-src `ext/uri` PHPT suite is vendored under
  `tests/fixtures/php_src_uri` and run by
  `cargo test --test codegen_tests test_uri_php_src_phpt_bundle`.
- The `zend_test`-driven fixtures (`100.phpt`–`102.phpt`) exercise php-src's
  test-only `zend_test_uri_parser()` helper. The harness supplies an equivalent
  helper built on the public `Uri\*` classes, so those fixtures validate the same
  parsing, raw/normalized component, large-port, and validation-error behavior
  without the `zend_test` extension.
- `--INI-- memory_limit` is honored as the runtime heap ceiling and the fatal message
  matches php-src's `Allowed memory size of N bytes exhausted …` form. All 330 vendored
  fixtures pass, including the twelve `oom_*` cases.
- The URI implementations retain one path-segment node per segment (an estimated 32-byte
  node) plus copied text for non-empty segments. Because that storage lives inside the Rust
  bridge rather than in the elephc heap, the prelude mirrors the footprint in hidden PHP
  arrays charged against `_heap_limit`: `4 * segments + 2 * non-empty segments` array cells
  (32 bytes per segment plus 16 bytes of text). The charge arrays live in private static
  maps keyed by `spl_object_id`, are dropped by `__destruct`, and are rebuilt by `__clone`
  and `__unserialize`. `--INI-- memory_limit` and runtime `ini_set('memory_limit', …)` both
  observe the charge.
- `057.phpt` passes a typed object property by reference. The caller passes a
  boxed temporary and writes it back through the declared property slot, so an
  incompatible array is refused with PHP's
  `Cannot assign array to reference held by property …` `TypeError` and the
  property keeps its previous value.
- To let user code call `parse()` and `equals()` on values whose static type is
  `?Uri` (a common php-src test shape), the prelude accepts `mixed` for those
  parameters and performs the runtime `instanceof` check itself. This means
  `ReflectionMethod` reports `mixed` there, and a wrong-type argument's message
  lacks php-src's `, <type> given` suffix; the methods, return values, and
  exceptions otherwise match PHP.
- `with*()` builds its result through `__elephc_rebind`: the original handle is cleared just
  for the PHP object copy and restored immediately, so the copy starts without a bridge handle
  and receives the mutated one directly. This performs no extra native clone or free compared
  with php-src's `zend_object` copy; user-visible clone semantics are unchanged.
- `withScheme(null)` matches PHP 8.5.10: it removes the scheme without a deprecation notice.
- The general AOT by-reference limits documented in [PHP types](types.md) apply to URI calls:
  an incompatible concrete property type is rejected at compile time, and a property passed by
  reference is copied in and written back after the call.
- The classes are prelude-declared, but Reflection reports php-src's engine shape:
  `isInternal()`, `isReadOnly()`, and `isFinal()` are true and `getModifiers()` is
  `65568`. `newInstanceWithoutConstructor()` throws php-src's internal-final-class
  `ReflectionException`, and the parsing, getter, mutation, cast, serialization,
  and debug behavior matches. Assigning an undeclared property is refused at compile
  time; php-src reports `Error: Cannot create dynamic property …` at run time.
