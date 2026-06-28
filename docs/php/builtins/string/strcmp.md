---
title: "strcmp()"
description: "Lowers a two-argument string builtin that directly delegates to a runtime helper."
sidebar:
  order: 369
---

## strcmp()

```php
function strcmp(string $string1, string $string2): int
```

Lowers a two-argument string builtin that directly delegates to a runtime helper.

**Parameters**:
- `$string1` (`string`)
- `$string2` (`string`)

**Returns**: `int`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `strcmp` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/strcmp.md).

