---
title: "htmlentities()"
description: "Lowers a one-argument string builtin that directly delegates to a runtime helper."
sidebar:
  order: 357
---

## htmlentities()

```php
function htmlentities(string $string, int $flags, string $encoding, bool $double_encode): string
```

Lowers a one-argument string builtin that directly delegates to a runtime helper.

**Parameters**:
- `$string` (`string`)
- `$flags` (`int`)
- `$encoding` (`string`)
- `$double_encode` (`bool`)

**Returns**: `string`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `htmlentities` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/htmlentities.md).

