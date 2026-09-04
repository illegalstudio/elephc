---
title: "mysqli_autocommit()"
description: "Implemented by the compiler-injected mysqli prelude."
sidebar:
  order: 99
---

## mysqli_autocommit()

```php
function mysqli_autocommit(mixed $mysql, bool $enable): bool
```

Implemented by the compiler-injected mysqli prelude.

**Parameters**:
- `$mysql` (`mixed`)
- `$enable` (`bool`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_autocommit` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_autocommit.md).
