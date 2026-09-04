---
title: "curl_multi_info_read()"
description: "Gets information about the current transfers."
sidebar:
  order: 374
---

## curl_multi_info_read()

```php
function curl_multi_info_read(CurlMultiHandle $multi_handle, int $queued_messages = null): mixed
```

Gets information about the current transfers.

**Parameters**:
- `$multi_handle` (`CurlMultiHandle`)
- `$queued_messages` (`int`), passed by reference, default `null`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected curl prelude.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/curl/curl_multi_info_read.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/curl/curl_multi_info_read.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `curl_multi_info_read` is implemented in the compiler, see [the internals page](../../../internals/builtins/network/curl_multi_info_read.md).
