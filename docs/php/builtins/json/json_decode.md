---
title: "json_decode()"
description: "Decodes a JSON string."
sidebar:
  order: 230
---

## json_decode()

```php
function json_decode(string $json, bool $associative = null, int $depth = 512, int $flags = 0): mixed
```

Decodes a JSON string.

**Parameters**:
- `$json` (`string`)
- `$associative` (`bool`), default `null`, optional
- `$depth` (`int`), default `512`, optional
- `$flags` (`int`), default `0`, optional

**Returns**: `mixed`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `json_decode` is implemented in the compiler, see [the internals page](../../../internals/builtins/json/json_decode.md).

