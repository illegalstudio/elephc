---
title: "mysqli_refresh()"
description: "Flushes tables or caches, or resets replication server information."
sidebar:
  order: 165
---

## mysqli_refresh()

```php
function mysqli_refresh(mixed $mysql, int $flags): bool
```

Flushes tables or caches, or resets replication server information.

**Parameters**:
- `$mysql` (`mixed`)
- `$flags` (`int`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_refresh` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_refresh.md).
