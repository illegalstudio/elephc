---
title: "htmlspecialchars()"
description: "Lowers a one-argument string builtin that directly delegates to a runtime helper."
sidebar:
  order: 358
---

## htmlspecialchars()

```php
function htmlspecialchars(string $string, int $flags, string $encoding, bool $double_encode): string
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

For how `htmlspecialchars` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/htmlspecialchars.md).

