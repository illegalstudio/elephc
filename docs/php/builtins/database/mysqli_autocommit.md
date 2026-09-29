---
title: "mysqli_autocommit()"
description: "Turns automatic committing on or off."
sidebar:
  order: 102
---

## mysqli_autocommit()

```php
function mysqli_autocommit(mixed $mysql, bool $enable): bool
```

Turns automatic committing on or off.

**Parameters**:
- `$mysql` (`mixed`)
- `$enable` (`bool`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_autocommit` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_autocommit.md).
