---
title: "session_cache_expire()"
description: "Reads or sets the cache expiry, in minutes, of the session cache limiter."
sidebar:
  order: 977
---

## session_cache_expire()

```php
function session_cache_expire(?int $value = null): mixed
```

Reads or sets the cache expiry, in minutes, of the session cache limiter.

**Parameters**:
- `$value` (`?int`), default `null`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `session_cache_expire` is implemented in the compiler, see [the internals page](../../../internals/builtins/web/session_cache_expire.md).
