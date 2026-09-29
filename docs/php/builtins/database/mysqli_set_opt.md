---
title: "mysqli_set_opt()"
description: "Alias of mysqli_options()."
sidebar:
  order: 159
---

## mysqli_set_opt()

```php
function mysqli_set_opt(mixed $mysql, int $option, mixed $value): bool
```

Alias of mysqli_options().

**Parameters**:
- `$mysql` (`mixed`)
- `$option` (`int`)
- `$value` (`mixed`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_set_opt` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_set_opt.md).
