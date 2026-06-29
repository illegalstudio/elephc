---
title: "html_entity_decode()"
description: "Lowers a one-argument string builtin that directly delegates to a runtime helper."
sidebar:
  order: 341
---

## html_entity_decode()

```php
function html_entity_decode(string $string, int $flags, string $encoding): string
```

Lowers a one-argument string builtin that directly delegates to a runtime helper.

**Parameters**:
- `$string` (`string`)
- `$flags` (`int`)
- `$encoding` (`string`)

**Returns**: `string`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `html_entity_decode` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/html_entity_decode.md).

