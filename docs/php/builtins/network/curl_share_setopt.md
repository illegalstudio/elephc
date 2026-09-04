---
title: "curl_share_setopt()"
description: "Sets an option for a cURL share handle."
sidebar:
  order: 388
---

## curl_share_setopt()

```php
function curl_share_setopt(CurlShareHandle $share_handle, int $option, mixed $value): bool
```

Sets an option for a cURL share handle.

**Parameters**:
- `$share_handle` (`CurlShareHandle`)
- `$option` (`int`)
- `$value` (`mixed`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected curl prelude.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/curl/curl_share_setopt.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/curl/curl_share_setopt.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `curl_share_setopt` is implemented in the compiler, see [the internals page](../../../internals/builtins/network/curl_share_setopt.md).
