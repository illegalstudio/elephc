---
title: "json_encode()"
description: "Lowers `json_encode(value, flags?, depth?)` through the shared JSON encoder runtime."
sidebar:
  order: 214
---

## json_encode()

```php
function json_encode(mixed $value, int $flags, int $depth): string
```

Lowers `json_encode(value, flags?, depth?)` through the shared JSON encoder runtime.

**Parameters**:
- `$value` (`mixed`)
- `$flags` (`int`), optional
- `$depth` (`int`), optional

**Returns**: `string`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `json_encode` is implemented in the compiler, see [the internals page](../../../internals/builtins/json/json_encode.md).

