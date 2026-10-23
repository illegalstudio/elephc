---
title: "mysqli_options()"
description: "Sets a connection option before connecting."
sidebar:
  order: 145
---

## mysqli_options()

```php
function mysqli_options(mixed $mysql, int $option, mixed $value): bool
```

Sets a connection option before connecting.

**Parameters**:
- `$mysql` (`mixed`)
- `$option` (`int`)
- `$value` (`mixed`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected mysqli prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_options` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_options.md).
