---
title: "json_validate()"
description: "Lowers `json_validate(json, depth?, flags?)` into the shared validator runtime."
sidebar:
  order: 217
---

## json_validate()

```php
function json_validate(string $json, int $depth, int $flags): bool
```

Lowers `json_validate(json, depth?, flags?)` into the shared validator runtime.

**Parameters**:
- `$json` (`string`)
- `$depth` (`int`), optional
- `$flags` (`int`), optional

**Returns**: `bool`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `json_validate` is implemented in the compiler, see [the internals page](../../../internals/builtins/json/json_validate.md).

