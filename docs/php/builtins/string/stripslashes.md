---
title: "stripslashes()"
description: "Lowers a one-argument string builtin that directly delegates to a runtime helper."
sidebar:
  order: 370
---

## stripslashes()

```php
function stripslashes(string $string): string
```

Lowers a one-argument string builtin that directly delegates to a runtime helper.

**Parameters**:
- `$string` (`string`)

**Returns**: `string`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `stripslashes` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/stripslashes.md).

