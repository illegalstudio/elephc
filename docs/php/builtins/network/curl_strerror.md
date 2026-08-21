---
title: "curl_strerror()"
description: "Returns string describing the given error code."
sidebar:
  order: 364
---

## curl_strerror()

```php
function curl_strerror(int $error_code): string
```

Returns string describing the given error code.

**Parameters**:
- `$error_code` (`int`)

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected curl prelude.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/curl/curl_strerror.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/curl/curl_strerror.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `curl_strerror` is implemented in the compiler, see [the internals page](../../../internals/builtins/network/curl_strerror.md).
