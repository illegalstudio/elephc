---
title: "curl_getinfo()"
description: "Gets information about the last transfer."
sidebar:
  order: 340
---

## curl_getinfo()

```php
function curl_getinfo(CurlHandle $handle, int $option = null): mixed
```

Gets information about the last transfer.

**Parameters**:
- `$handle` (`CurlHandle`)
- `$option` (`int`), default `null`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected curl prelude.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/curl/curl_getinfo.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/curl/curl_getinfo.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `curl_getinfo` is implemented in the compiler, see [the internals page](../../../internals/builtins/network/curl_getinfo.md).
