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
  `FILTER_FLAG_QUERY_REQUIRED` and `uri_parser_class`.
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
`FILTER_UNSAFE_RAW` are implemented with php-src's byte maps, tag stripping and
entity encoding.

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

Email and `FILTER_VALIDATE_REGEXP` validation run through a pure-Rust
PCRE-compatible engine in the `elephc-filter` bridge, so the filter surface needs
no managed native package. Delimiters and the `i`, `m`, `s`, `x` and `D` modifiers
are honoured; `D` rejects a trailing newline like PHP's PCRE end anchor.

## Request input and `filter.default`

`--ini filter.default=<name>` applies a filter to the request superglobals at request startup,
exactly like the PHP INI directive, and `--ini filter.default_flags=<int>` supplies its flags.
`filter_input()` and `filter_input_array()` keep reading the raw request values, so filtering
`$_GET` does not change what `filter_input(INPUT_GET, …)` sees. The directive is deprecated in
PHP 8.1+ and emits the same deprecation once per process.

```php
// elephc --web --ini filter.default=special_chars app.php
echo $_GET['name'];                                  // filtered
echo filter_input(INPUT_GET, 'name');                // raw request value
echo filter_input(INPUT_GET, 'name', FILTER_SANITIZE_SPECIAL_CHARS);
```

The request arrays are snapshotted before user code runs. In a CLI binary the HTTP request arrays
are empty, matching PHP's CLI SAPI, while `$_ENV` and `$_SERVER` are seeded from the process
environment; web builds parse the request into `$_GET`, `$_POST` and `$_COOKIE` first.

`parse_str()` applies the same configured `filter.default` to every parsed leaf. Rejected scalar
values become PHP's empty-string input value, while sanitizers replace the leaf with their
sanitized string; nested arrays are walked recursively.
