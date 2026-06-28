---
title: "json_decode()"
description: "Lowers `json_decode(json, associative?, depth?, flags?)` through the shared JSON decoder runtime."
sidebar:
  order: 213
---

## json_decode()

```php
function json_decode(string $json, bool $associative, int $depth, int $flags): mixed
```

Lowers `json_decode(json, associative?, depth?, flags?)` through the shared JSON decoder runtime.

**Parameters**:
- `$json` (`string`)
- `$associative` (`bool`), optional
- `$depth` (`int`), optional
- `$flags` (`int`), optional

**Returns**: `mixed`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `json_decode` is implemented in the compiler, see [the internals page](../../../internals/builtins/json/json_decode.md).

