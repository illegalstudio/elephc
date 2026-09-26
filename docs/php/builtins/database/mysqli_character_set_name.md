---
title: "mysqli_character_set_name()"
description: "Returns the connection's current character set."
sidebar:
  order: 104
---

## mysqli_character_set_name()

```php
function mysqli_character_set_name(mixed $mysql): string
```

Returns the connection's current character set.

**Parameters**:
- `$mysql` (`mixed`)

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_character_set_name` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_character_set_name.md).
