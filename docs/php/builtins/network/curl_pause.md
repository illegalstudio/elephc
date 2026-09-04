---
title: "curl_pause()"
description: "Pauses and unpauses a connection."
sidebar:
  order: 380
---

## curl_pause()

```php
function curl_pause(CurlHandle $handle, int $flags): int
```

Pauses and unpauses a connection.

**Parameters**:
- `$handle` (`CurlHandle`)
- `$flags` (`int`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected curl prelude.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/curl/curl_pause.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/curl/curl_pause.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `curl_pause` is implemented in the compiler, see [the internals page](../../../internals/builtins/network/curl_pause.md).
