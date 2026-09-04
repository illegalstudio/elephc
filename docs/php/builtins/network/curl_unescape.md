---
title: "curl_unescape()"
description: "Decodes the given URL-encoded string."
sidebar:
  order: 391
---

## curl_unescape()

```php
function curl_unescape(CurlHandle $handle, string $string): string
```

Decodes the given URL-encoded string.

**Parameters**:
- `$handle` (`CurlHandle`)
- `$string` (`string`)

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected curl prelude.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/curl/curl_unescape.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/curl/curl_unescape.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `curl_unescape` is implemented in the compiler, see [the internals page](../../../internals/builtins/network/curl_unescape.md).
