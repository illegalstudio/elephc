---
title: "curl_share_errno()"
description: "Returns the last share curl error number."
sidebar:
  order: 359
---

## curl_share_errno()

```php
function curl_share_errno(CurlShareHandle $share_handle): int
```

Returns the last share curl error number.

**Parameters**:
- `$share_handle` (`CurlShareHandle`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected curl prelude.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/curl/curl_share_errno.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/curl/curl_share_errno.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `curl_share_errno` is implemented in the compiler, see [the internals page](../../../internals/builtins/network/curl_share_errno.md).
