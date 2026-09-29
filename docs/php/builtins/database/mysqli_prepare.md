---
title: "mysqli_prepare()"
description: "Prepares a statement for execution."
sidebar:
  order: 148
---

## mysqli_prepare()

```php
function mysqli_prepare(mixed $mysql, string $query): mixed
```

Prepares a statement for execution.

**Parameters**:
- `$mysql` (`mixed`)
- `$query` (`string`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_prepare` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_prepare.md).
