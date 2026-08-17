---
title: "curl_version()"
description: "Gets cURL version information."
sidebar:
  order: 367
---

## curl_version()

```php
function curl_version(): mixed
```

Gets cURL version information.

**Parameters**: none.

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected curl prelude.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/curl/curl_version.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/curl/curl_version.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `curl_version` is implemented in the compiler, see [the internals page](../../../internals/builtins/network/curl_version.md).
