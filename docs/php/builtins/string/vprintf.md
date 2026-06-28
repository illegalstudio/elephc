---
title: "vprintf()"
description: "Lowers `vprintf(format, values)` as `vsprintf()` followed by stdout emission."
sidebar:
  order: 385
---

## vprintf()

```php
function vprintf(string $format, array $values): int
```

Lowers `vprintf(format, values)` as `vsprintf()` followed by stdout emission.

**Parameters**:
- `$format` (`string`)
- `$values` (`array`)

**Returns**: `int`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `vprintf` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/vprintf.md).

