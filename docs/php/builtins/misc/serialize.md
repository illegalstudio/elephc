---
title: "serialize()"
description: "Generates a storable representation of a value."
sidebar:
  order: 301
---

## serialize()

```php
function serialize(mixed $value): string
```

Generates a storable representation of a value.

**Parameters**:
- `$value` (`mixed`)

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `serialize` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/serialize.md).
