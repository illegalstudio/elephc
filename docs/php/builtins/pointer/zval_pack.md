---
title: "zval_pack()"
description: "Packs an elephc runtime value into a heap-allocated PHP zval pointer."
sidebar:
  order: 301
---

## zval_pack()

```php
function zval_pack(mixed $value): pointer
```

Packs an elephc runtime value into a heap-allocated PHP zval pointer.

**Parameters**:
- `$value` (`mixed`)

**Returns**: `pointer`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `zval_pack` is implemented in the compiler, see [the internals page](../../../internals/builtins/pointer/zval_pack.md).

