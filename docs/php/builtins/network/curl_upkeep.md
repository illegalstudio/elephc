---
title: "curl_upkeep()"
description: "Performs any connection upkeep checks."
sidebar:
  order: 366
---

## curl_upkeep()

```php
function curl_upkeep(CurlHandle $handle): bool
```

Performs any connection upkeep checks.

**Parameters**:
- `$handle` (`CurlHandle`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected curl prelude.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/curl/curl_upkeep.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/curl/curl_upkeep.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `curl_upkeep` is implemented in the compiler, see [the internals page](../../../internals/builtins/network/curl_upkeep.md).
