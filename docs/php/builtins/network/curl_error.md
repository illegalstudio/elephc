---
title: "curl_error()"
description: "Returns a string describing the last cURL error."
sidebar:
  order: 363
---

## curl_error()

```php
function curl_error(CurlHandle $handle): string
```

Returns a string describing the last cURL error.

**Parameters**:
- `$handle` (`CurlHandle`)

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected curl prelude.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/curl/curl_error.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/curl/curl_error.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `curl_error` is implemented in the compiler, see [the internals page](../../../internals/builtins/network/curl_error.md).
