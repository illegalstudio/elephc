---
title: "json_encode()"
description: "Returns the JSON representation of a value."
sidebar:
  order: 231
---

## json_encode()

```php
function json_encode(mixed $value, int $flags = 0, int $depth = 512): string
```

Returns the JSON representation of a value.

**Parameters**:
- `$value` (`mixed`)
- `$flags` (`int`), default `0`, optional
- `$depth` (`int`), default `512`, optional

**Returns**: `string`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `json_encode` is implemented in the compiler, see [the internals page](../../../internals/builtins/json/json_encode.md).

