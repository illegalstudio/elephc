---
title: "Filter"
description: "PHP's filter extension: validation, sanitization, array filtering, request input, callbacks and the complete FILTER_* constant family."
sidebar:
  order: 29
---

Elephc implements PHP's `ext/filter` surface through a synthetic prelude backed by
the `elephc-filter` bridge. The functions, constants and `Filter\*` exceptions are
available on every supported target, in compiled code and inside `eval()`.

## Functions

```php
filter_var(mixed $value, int $filter = FILTER_DEFAULT, array|int $options = 0): mixed
filter_var_array(array $array, array|int $options = FILTER_DEFAULT, bool $add_empty = true): mixed
filter_input(int $type, string $var_name, int $filter = FILTER_DEFAULT, array|int $options = 0): mixed
filter_input_array(int $type, array|int $options = FILTER_DEFAULT, bool $add_empty = true): mixed
filter_has_var(int $input_type, string $var_name): bool
filter_list(): array
filter_id(string $name): int|false
```

```php
<?php
$age = filter_var("42", FILTER_VALIDATE_INT, [
    "options" => ["min_range" => 18, "max_range" => 120],
]);
var_dump($age); // int(42)

$email = filter_var("ada@example.com", FILTER_VALIDATE_EMAIL);
var_dump($email); // string(15) "ada@example.com"
```

## Strict types

A direct `filter_*()` call made from a file with `declare(strict_types=1)` validates
the argument against PHP's documented signature before the filter runs. Passing
`"1"` where an `int $filter` is expected throws the same `TypeError`; `array|int`
accepts only arrays or integers. Files without the directive keep PHP's coercive
mode, including null/float deprecations and range errors.

## Validation filters

- `FILTER_VALIDATE_INT` with `FILTER_FLAG_ALLOW_OCTAL` / `FILTER_FLAG_ALLOW_HEX`,
  plus `min_range` and `max_range`.
- `FILTER_VALIDATE_BOOL` (`FILTER_VALIDATE_BOOLEAN` is the same id).
- `FILTER_VALIDATE_FLOAT` with `decimal`, `thousand`, `min_range`, `max_range`,
  `FILTER_FLAG_ALLOW_FRACTION`, `FILTER_FLAG_ALLOW_THOUSAND` and
  `FILTER_FLAG_ALLOW_SCIENTIFIC`.
- `FILTER_VALIDATE_REGEXP` with the `regexp` option.
- `FILTER_VALIDATE_DOMAIN` with `FILTER_FLAG_HOSTNAME`.
- `FILTER_VALIDATE_URL` with `FILTER_FLAG_PATH_REQUIRED`,
  `FILTER_FLAG_QUERY_REQUIRED` and `uri_parser_class`. `Uri\Rfc3986\Uri` uses
  the RFC 3986-compatible path; `Uri\WhatWg\Url` uses a WHATWG-compatible URL
  parser, including special-scheme backslashes, port ranges and IPv4
  normalization.
- `FILTER_VALIDATE_EMAIL` with `FILTER_FLAG_EMAIL_UNICODE`.
- `FILTER_VALIDATE_IP` with `FILTER_FLAG_IPV4`, `FILTER_FLAG_IPV6`,
  `FILTER_FLAG_NO_PRIV_RANGE`, `FILTER_FLAG_NO_RES_RANGE` and
  `FILTER_FLAG_GLOBAL_RANGE`.
- `FILTER_VALIDATE_MAC` with the `separator` option.

On success the filter returns the original value for the URL, email, IP, MAC and
domain validators, and the converted value for integer, float and boolean
validators. On failure it returns `false`, `null` with
`FILTER_NULL_ON_FAILURE`, or throws `Filter\FilterFailedException` with
`FILTER_THROW_ON_FAILURE`.

## Sanitization filters

`FILTER_SANITIZE_STRING` (and its alias `FILTER_SANITIZE_STRIPPED`),
`FILTER_SANITIZE_ENCODED`, `FILTER_SANITIZE_SPECIAL_CHARS`,
`FILTER_SANITIZE_FULL_SPECIAL_CHARS`, `FILTER_SANITIZE_EMAIL`,
`FILTER_SANITIZE_URL`, `FILTER_SANITIZE_NUMBER_INT`,
`FILTER_SANITIZE_NUMBER_FLOAT`, `FILTER_SANITIZE_ADD_SLASHES` and
`FILTER_UNSAFE_RAW` are implemented with php-src's byte maps and tag stripping.
`FILTER_SANITIZE_FULL_SPECIAL_CHARS` escapes the HTML metacharacters, preserves
existing HTML 4.01 and numeric entities, and maps non-ASCII code points to named
entities for UTF-8, ISO-8859-1, ISO-8859-15 and Windows-1252.

## Arrays, options and callbacks

`FILTER_REQUIRE_SCALAR`, `FILTER_REQUIRE_ARRAY` and `FILTER_FORCE_ARRAY` control
the shape of a result. `filter_var_array()` accepts a definition array keyed by
input name; `add_empty` controls whether missing keys become `null`. Definition
entries may be a filter id or an array with `filter`, `flags` and `options`.

`FILTER_CALLBACK` calls the callable supplied through the `options` entry. The
callable receives the stringified value, exactly like `php_filter_callback`.

## Request input

`filter_input()`, `filter_input_array()` and `filter_has_var()` read the request
superglobals (`$_GET`, `$_POST`, `$_COOKIE`, `$_ENV`, `$_SERVER`). Missing values
follow PHP's inverted `FILTER_NULL_ON_FAILURE` rule and honour the `default`
option before the sentinel is returned.

## Exceptions and constants

```php
<?php
try {
    filter_var("nope", FILTER_VALIDATE_INT, FILTER_THROW_ON_FAILURE);
} catch (\Filter\FilterFailedException $e) {
    echo $e->getMessage();
}
```

The complete PHP 8.5 `FILTER_*` and `INPUT_*` constant family is predefined,
including the deprecated `FILTER_SANITIZE_STRING` / `FILTER_SANITIZE_STRIPPED`
reads, which emit PHP's deprecation diagnostic.

## Regular expressions

Email validation uses the `elephc-filter` bridge's pure-Rust PCRE-compatible
engine. `FILTER_VALIDATE_REGEXP` compiles and matches through the same managed
PCRE2 path as `preg_match()`: delimiters and PHP's `i`, `m`, `s`, `x`, `A`, `D`,
`u`, `U`, `J`, `n` and `r` modifiers are honoured, and PCRE2-only syntax (`\h`,
`\R`, possessive quantifiers, recursion) behaves like PHP. Patterns that are not
valid UTF-8 still pass through the length-aware PCRE2 compile check, so a
malformed byte pattern reports PHP's `Compilation failed: …` warning instead of
failing silently.

Programs that use `FILTER_VALIDATE_REGEXP` link the managed `pcre2` package, so
invalid patterns report PCRE2's exact `Compilation failed: … at offset N`
diagnostic. Install it with `elephc native add pcre2` when a project does not
already declare it.

## Request input and `filter.default`

`--ini filter.default=<name>` applies a filter to the request superglobals at request startup
in web builds, exactly like the PHP INI directive, and `--ini filter.default_flags=<int>`
supplies its flags. Filter results keep their native types (`int`, `float`, `bool`), including
`false` for a validation failure; only `parse_str()` stringifies its leaves. `filter_input()`
and `filter_input_array()` keep reading the raw request values, so filtering `$_GET` does not
change what `filter_input(INPUT_GET, …)` sees. The directive is deprecated in PHP 8.1+ and emits
the same deprecation once per process.

```php
// elephc --web --ini filter.default=special_chars app.php
echo $_GET['name'];                                  // filtered
echo filter_input(INPUT_GET, 'name');                // raw request value
echo filter_input(INPUT_GET, 'name', FILTER_SANITIZE_SPECIAL_CHARS);
```

The request arrays are snapshotted before user code runs. In a CLI binary the HTTP request arrays
are empty, matching PHP's CLI SAPI, while `$_ENV` and `$_SERVER` are seeded from the process
environment; web builds parse the request into `$_GET`, `$_POST` and `$_COOKIE` first. A compiled
CLI binary leaves its superglobals unfiltered: the CLI SAPI only applies `filter.default` to a
small server-key subset, while elephc's CLI superglobals are process-environment seeds.

`parse_str()` applies the same configured `filter.default` to every parsed leaf. Rejected scalar
values become PHP's empty-string input value, while sanitizers replace the leaf with their
sanitized string; nested arrays are walked recursively.

## Known gaps

- Strict scalar checks cover direct filter calls. A first-class callable or other dynamic
  invocation still uses the generic descriptor path and PHP's weak-mode coercion.
- Filter-id selection is conservative. A runtime filter id, a dynamic `filter_var_array()`
  specification, or a dynamic `call_user_func*()` callback may select
  `FILTER_VALIDATE_REGEXP`, so those programs link the managed `pcre2` package even when the
  executed filter turns out to be a non-regexp one. A literal non-regexp filter keeps the
  bridge stubs and does not require `pcre2`.
- Opaque `eval()` cannot be inspected at compile time. When evaluated code must use
  `FILTER_VALIDATE_REGEXP`, pass `--with-regex` or name the constant in compiled code so the
  managed `pcre2` path is linked. Without it the call reports
  `regexp validation requires the managed pcre2 package` and validation fails instead of
  silently matching through a different engine.
- `Uri\Rfc3986\Uri` and `Uri\WhatWg\Url` are accepted by `uri_parser_class` for URL
  validation. The URI objects' component methods are not exposed outside the filter surface.

<!-- elephc:generated:symbols:begin -->

## Functions {#functions}

Generated from the shared symbol catalog by `scripts/docs/gen_module_sections.py`; do not edit this section by hand. Each function links to its reference page.

| Function | Signature | Returns | AOT | eval() |
|---|---|---|:-:|:-:|
| [`filter_has_var()`](./builtins/filter/filter_has_var.md) | `(int $input_type, string $var_name): bool` | `bool` | ✓ | ✓ |
| [`filter_id()`](./builtins/filter/filter_id.md) | `(string $name): int|false` | `int|false` | ✓ | ✓ |
| [`filter_input()`](./builtins/filter/filter_input.md) | `(int $type, string $var_name, int $filter = FILTER_DEFAULT, array|int $options = 0): mixed` | `mixed` | ✓ | ✓ |
| [`filter_input_array()`](./builtins/filter/filter_input_array.md) | `(int $type, array|int $options = FILTER_DEFAULT, bool $add_empty = true): array|false|null` | `array|false|null` | ✓ | ✓ |
| [`filter_list()`](./builtins/filter/filter_list.md) | `(): array` | `array` | ✓ | ✓ |
| [`filter_var()`](./builtins/filter/filter_var.md) | `(mixed $value, int $filter = FILTER_DEFAULT, array|int $options = 0): mixed` | `mixed` | ✓ | ✓ |
| [`filter_var_array()`](./builtins/filter/filter_var_array.md) | `(array $array, array|int $options = FILTER_DEFAULT, bool $add_empty = true): array|false|null` | `array|false|null` | ✓ | ✓ |

Classes: `Filter\FilterException`, `Filter\FilterFailedException`.

Constants: `FILTER_CALLBACK`, `FILTER_DEFAULT`, `FILTER_FLAG_ALLOW_FRACTION`, `FILTER_FLAG_ALLOW_HEX`, `FILTER_FLAG_ALLOW_OCTAL`, `FILTER_FLAG_ALLOW_SCIENTIFIC`, `FILTER_FLAG_ALLOW_THOUSAND`, `FILTER_FLAG_EMAIL_UNICODE`, `FILTER_FLAG_EMPTY_STRING_NULL`, `FILTER_FLAG_ENCODE_AMP`, `FILTER_FLAG_ENCODE_HIGH`, `FILTER_FLAG_ENCODE_LOW`, `FILTER_FLAG_GLOBAL_RANGE`, `FILTER_FLAG_HOSTNAME`, `FILTER_FLAG_IPV4`, `FILTER_FLAG_IPV6`, `FILTER_FLAG_NONE`, `FILTER_FLAG_NO_ENCODE_QUOTES`, `FILTER_FLAG_NO_PRIV_RANGE`, `FILTER_FLAG_NO_RES_RANGE`, `FILTER_FLAG_PATH_REQUIRED`, `FILTER_FLAG_QUERY_REQUIRED`, `FILTER_FLAG_STRIP_BACKTICK`, `FILTER_FLAG_STRIP_HIGH`, `FILTER_FLAG_STRIP_LOW`, `FILTER_FORCE_ARRAY`, `FILTER_NULL_ON_FAILURE`, `FILTER_REQUIRE_ARRAY`, `FILTER_REQUIRE_SCALAR`, `FILTER_SANITIZE_ADD_SLASHES`, `FILTER_SANITIZE_EMAIL`, `FILTER_SANITIZE_ENCODED`, `FILTER_SANITIZE_FULL_SPECIAL_CHARS`, `FILTER_SANITIZE_NUMBER_FLOAT`, `FILTER_SANITIZE_NUMBER_INT`, `FILTER_SANITIZE_SPECIAL_CHARS`, `FILTER_SANITIZE_STRING`, `FILTER_SANITIZE_STRIPPED`, `FILTER_SANITIZE_URL`, `FILTER_THROW_ON_FAILURE`, `FILTER_UNSAFE_RAW`, `FILTER_VALIDATE_BOOL`, `FILTER_VALIDATE_BOOLEAN`, `FILTER_VALIDATE_DOMAIN`, `FILTER_VALIDATE_EMAIL`, `FILTER_VALIDATE_FLOAT`, `FILTER_VALIDATE_INT`, `FILTER_VALIDATE_IP`, `FILTER_VALIDATE_MAC`, `FILTER_VALIDATE_REGEXP`, `FILTER_VALIDATE_URL`, `INPUT_COOKIE`, `INPUT_ENV`, `INPUT_GET`, `INPUT_POST`, `INPUT_SERVER`.

<!-- elephc:generated:symbols:end -->
