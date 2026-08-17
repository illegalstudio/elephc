---
title: "curl_multi_close()"
description: "Closes a set of cURL handles."
sidebar:
  order: 343
---

## curl_multi_close()

```php
function curl_multi_close(CurlMultiHandle $multi_handle): void
```

Closes a set of cURL handles.

**Parameters**:
- `$multi_handle` (`CurlMultiHandle`)

**Returns**: `void`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected curl prelude.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/curl/curl_multi_close.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/curl/curl_multi_close.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `curl_multi_close` is implemented in the compiler, see [the internals page](../../../internals/builtins/network/curl_multi_close.md).
