---
title: "curl_multi_getcontent()"
description: "Returns the content of a cURL handle if CURLOPT_RETURNTRANSFER is set."
sidebar:
  order: 373
---

## curl_multi_getcontent()

```php
function curl_multi_getcontent(mixed $handle): ?string
```

Returns the content of a cURL handle if CURLOPT_RETURNTRANSFER is set.

**Parameters**:
- `$handle` (`mixed`)

**Returns**: `?string`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected curl prelude.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/curl/curl_multi_getcontent.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/curl/curl_multi_getcontent.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `curl_multi_getcontent` is implemented in the compiler, see [the internals page](../../../internals/builtins/network/curl_multi_getcontent.md).
