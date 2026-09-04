---
title: "curl_multi_strerror()"
description: "Returns string describing error code."
sidebar:
  order: 379
---

## curl_multi_strerror()

```php
function curl_multi_strerror(int $error_code): string
```

Returns string describing error code.

**Parameters**:
- `$error_code` (`int`)

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected curl prelude.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/curl/curl_multi_strerror.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/curl/curl_multi_strerror.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `curl_multi_strerror` is implemented in the compiler, see [the internals page](../../../internals/builtins/network/curl_multi_strerror.md).
