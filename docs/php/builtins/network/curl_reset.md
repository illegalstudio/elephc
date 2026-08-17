---
title: "curl_reset()"
description: "Resets all options of a libcurl session handle."
sidebar:
  order: 355
---

## curl_reset()

```php
function curl_reset(CurlHandle $handle): void
```

Resets all options of a libcurl session handle.

**Parameters**:
- `$handle` (`CurlHandle`)

**Returns**: `void`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected curl prelude.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/curl/curl_reset.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/curl/curl_reset.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `curl_reset` is implemented in the compiler, see [the internals page](../../../internals/builtins/network/curl_reset.md).
