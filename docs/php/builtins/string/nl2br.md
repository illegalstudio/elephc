---
title: "nl2br()"
description: "Lowers a one-argument string builtin that directly delegates to a runtime helper."
sidebar:
  order: 352
---

## nl2br()

```php
function nl2br(string $string, bool $use_xhtml): string
```

Lowers a one-argument string builtin that directly delegates to a runtime helper.

**Parameters**:
- `$string` (`string`)
- `$use_xhtml` (`bool`)

**Returns**: `string`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `nl2br` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/nl2br.md).

