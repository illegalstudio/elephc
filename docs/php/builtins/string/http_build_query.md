---
title: "http_build_query()"
description: "Generates a URL-encoded query string from an array or object."
sidebar:
  order: 811
---

## http_build_query()

```php
function http_build_query(array|object $data, string $numeric_prefix = '', ?string $arg_separator = null, int $encoding_type = PHP_QUERY_RFC1738): string
```

Generates a URL-encoded query string from an array or object.

**Parameters**:
- `$data` (`array|object`)
- `$numeric_prefix` (`string`), default `''`, optional
- `$arg_separator` (`?string`), default `null`, optional
- `$encoding_type` (`int`), default `PHP_QUERY_RFC1738`, optional

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected http_build_query prelude.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/http_build_query.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/http_build_query.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `http_build_query` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/http_build_query.md).
