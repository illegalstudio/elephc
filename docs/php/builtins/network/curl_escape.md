---
title: "curl_escape()"
description: "URL-encodes a string with the given cURL handle."
sidebar:
  order: 364
---

## curl_escape()

```php
function curl_escape(CurlHandle $handle, string $string): string
```

URL-encodes a string with the given cURL handle.

**Parameters**:
- `$handle` (`CurlHandle`)
- `$string` (`string`)

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected curl prelude.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/curl/curl_escape.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/curl/curl_escape.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `curl_escape` is implemented in the compiler, see [the internals page](../../../internals/builtins/network/curl_escape.md).
