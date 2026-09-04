---
title: "curl_exec()"
description: "Performs a cURL session."
sidebar:
  order: 365
---

## curl_exec()

```php
function curl_exec(CurlHandle $handle): string|bool
```

Performs a cURL session.

**Parameters**:
- `$handle` (`CurlHandle`)

**Returns**: `string|bool`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected curl prelude.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/curl/curl_exec.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/curl/curl_exec.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `curl_exec` is implemented in the compiler, see [the internals page](../../../internals/builtins/network/curl_exec.md).
