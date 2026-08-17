---
title: "curl_share_init()"
description: "Initializes a cURL share handle."
sidebar:
  order: 360
---

## curl_share_init()

```php
function curl_share_init(): CurlShareHandle
```

Initializes a cURL share handle.

**Parameters**: none.

**Returns**: `CurlShareHandle`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected curl prelude.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/curl/curl_share_init.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/curl/curl_share_init.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `curl_share_init` is implemented in the compiler, see [the internals page](../../../internals/builtins/network/curl_share_init.md).
