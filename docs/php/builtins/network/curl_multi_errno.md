---
title: "curl_multi_errno()"
description: "Returns the last multi curl error number."
sidebar:
  order: 370
---

## curl_multi_errno()

```php
function curl_multi_errno(CurlMultiHandle $multi_handle): int
```

Returns the last multi curl error number.

**Parameters**:
- `$multi_handle` (`CurlMultiHandle`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected curl prelude.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/curl/curl_multi_errno.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/curl/curl_multi_errno.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `curl_multi_errno` is implemented in the compiler, see [the internals page](../../../internals/builtins/network/curl_multi_errno.md).
