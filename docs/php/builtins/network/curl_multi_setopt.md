---
title: "curl_multi_setopt()"
description: "Sets an option on a cURL multi handle."
sidebar:
  order: 378
---

## curl_multi_setopt()

```php
function curl_multi_setopt(CurlMultiHandle $multi_handle, int $option, mixed $value): bool
```

Sets an option on a cURL multi handle.

**Parameters**:
- `$multi_handle` (`CurlMultiHandle`)
- `$option` (`int`)
- `$value` (`mixed`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected curl prelude.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/curl/curl_multi_setopt.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/curl/curl_multi_setopt.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `curl_multi_setopt` is implemented in the compiler, see [the internals page](../../../internals/builtins/network/curl_multi_setopt.md).
