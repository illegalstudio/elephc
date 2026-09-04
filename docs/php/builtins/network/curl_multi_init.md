---
title: "curl_multi_init()"
description: "Returns a new cURL multi handle."
sidebar:
  order: 376
---

## curl_multi_init()

```php
function curl_multi_init(): CurlMultiHandle
```

Returns a new cURL multi handle.

**Parameters**: none.

**Returns**: `CurlMultiHandle`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected curl prelude.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/curl/curl_multi_init.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/curl/curl_multi_init.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `curl_multi_init` is implemented in the compiler, see [the internals page](../../../internals/builtins/network/curl_multi_init.md).
