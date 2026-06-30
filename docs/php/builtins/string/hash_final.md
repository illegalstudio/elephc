---
title: "hash_final()"
description: "Lowers `hash_final(context, binary?)` through the incremental hash finalizer."
sidebar:
  order: 351
---

## hash_final()

```php
function hash_final(resource $context, bool $binary): string
```

Lowers `hash_final(context, binary?)` through the incremental hash finalizer.

**Parameters**:
- `$context` (`resource`)
- `$binary` (`bool`), optional

**Returns**: `string`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `hash_final` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/hash_final.md).

