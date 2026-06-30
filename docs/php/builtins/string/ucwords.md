---
title: "ucwords()"
description: "Lowers a one-argument string builtin that directly delegates to a runtime helper."
sidebar:
  order: 399
---

## ucwords()

```php
function ucwords(string $string, string $separators): string
```

Lowers a one-argument string builtin that directly delegates to a runtime helper.

**Parameters**:
- `$string` (`string`)
- `$separators` (`string`), optional

**Returns**: `string`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `ucwords` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/ucwords.md).

