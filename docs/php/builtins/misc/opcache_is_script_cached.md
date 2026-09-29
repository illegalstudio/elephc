---
title: "opcache_is_script_cached()"
description: "Reports whether a script is in the opcode cache."
sidebar:
  order: 642
---

## opcache_is_script_cached()

```php
function opcache_is_script_cached(mixed $filename): bool
```

Reports whether a script is in the opcode cache.

**Parameters**:
- `$filename` (`mixed`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `opcache_is_script_cached` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/opcache_is_script_cached.md).
