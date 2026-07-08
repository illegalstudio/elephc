---
title: "hash_final()"
description: "Finalizes an incremental hash and returns the digest string."
sidebar:
  order: 355
---

## hash_final()

```php
function hash_final(resource $context, bool $binary = false): string
```

Finalizes an incremental hash and returns the digest string.

**Parameters**:
- `$context` (`resource`)
- `$binary` (`bool`), default `false`, optional

**Returns**: `string`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `hash_final` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/hash_final.md).

