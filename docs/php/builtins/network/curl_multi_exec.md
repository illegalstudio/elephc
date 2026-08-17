---
title: "curl_multi_exec()"
description: "Runs the sub-connections of the current cURL handle."
sidebar:
  order: 345
---

## curl_multi_exec()

```php
function curl_multi_exec(CurlMultiHandle $multi_handle, int $still_running): int
```

Runs the sub-connections of the current cURL handle.

**Parameters**:
- `$multi_handle` (`CurlMultiHandle`)
- `$still_running` (`int`), passed by reference

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected curl prelude.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/curl/curl_multi_exec.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/curl/curl_multi_exec.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `curl_multi_exec` is implemented in the compiler, see [the internals page](../../../internals/builtins/network/curl_multi_exec.md).
