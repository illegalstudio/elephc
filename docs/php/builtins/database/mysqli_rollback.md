---
title: "mysqli_rollback()"
description: "Implemented by the compiler-injected mysqli prelude."
sidebar:
  order: 152
---

## mysqli_rollback()

```php
function mysqli_rollback(mixed $mysql, int $flags = 0, string $name = null): bool
```

Implemented by the compiler-injected mysqli prelude.

**Parameters**:
- `$mysql` (`mixed`)
- `$flags` (`int`), default `0`, optional
- `$name` (`string`), default `null`, optional

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_rollback` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_rollback.md).
