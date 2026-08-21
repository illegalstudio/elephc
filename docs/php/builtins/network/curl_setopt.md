---
title: "curl_setopt()"
description: "Sets an option for a cURL transfer."
sidebar:
  order: 356
---

## curl_setopt()

```php
function curl_setopt(CurlHandle $handle, int $option, mixed $value): bool
```

Sets an option for a cURL transfer.

**Parameters**:
- `$handle` (`CurlHandle`)
- `$option` (`int`)
- `$value` (`mixed`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected curl prelude.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/curl/curl_setopt.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/curl/curl_setopt.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `curl_setopt` is implemented in the compiler, see [the internals page](../../../internals/builtins/network/curl_setopt.md).
