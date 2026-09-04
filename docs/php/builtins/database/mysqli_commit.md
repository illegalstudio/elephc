---
title: "mysqli_commit()"
description: "Implemented by the compiler-injected mysqli prelude."
sidebar:
  order: 103
---

## mysqli_commit()

```php
function mysqli_commit(mixed $mysql, int $flags = 0, string $name = null): bool
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

For how `mysqli_commit` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_commit.md).
