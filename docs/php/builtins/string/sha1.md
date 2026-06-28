---
title: "sha1()"
description: "Lowers `sha1(data, binary?)` through the shared crypto-backed runtime helper."
sidebar:
  order: 357
---

## sha1()

```php
function sha1(string $string, bool $binary): string
```

Lowers `sha1(data, binary?)` through the shared crypto-backed runtime helper.

**Parameters**:
- `$string` (`string`)
- `$binary` (`bool`), optional

**Returns**: `string`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `sha1` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/sha1.md).

