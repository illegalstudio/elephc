---
title: "str_starts_with()"
description: "Lowers a two-argument string builtin that directly delegates to a runtime helper."
sidebar:
  order: 384
---

## str_starts_with()

```php
function str_starts_with(string $haystack, string $needle): bool
```

Lowers a two-argument string builtin that directly delegates to a runtime helper.

**Parameters**:
- `$haystack` (`string`)
- `$needle` (`string`)

**Returns**: `bool`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `str_starts_with` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/str_starts_with.md).

