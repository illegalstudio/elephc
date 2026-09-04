---
title: "curl_setopt_array()"
description: "Sets multiple options for a cURL transfer."
sidebar:
  order: 383
---

## curl_setopt_array()

```php
function curl_setopt_array(CurlHandle $handle, array $options): bool
```

Sets multiple options for a cURL transfer.

**Parameters**:
- `$handle` (`CurlHandle`)
- `$options` (`array`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected curl prelude.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/curl/curl_setopt_array.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/curl/curl_setopt_array.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `curl_setopt_array` is implemented in the compiler, see [the internals page](../../../internals/builtins/network/curl_setopt_array.md).
