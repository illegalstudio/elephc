---
title: "curl_init()"
description: "Initializes a cURL session."
sidebar:
  order: 367
---

## curl_init()

```php
function curl_init(string $url = null): CurlHandle
```

Initializes a cURL session.

**Parameters**:
- `$url` (`string`), default `null`, optional

**Returns**: `CurlHandle`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected curl prelude.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/curl/curl_init.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/curl/curl_init.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `curl_init` is implemented in the compiler, see [the internals page](../../../internals/builtins/network/curl_init.md).
