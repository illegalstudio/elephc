---
title: "zval_pack()"
description: "Lowers `zval_pack(value)` by boxing the operand as a Mixed cell and invoking"
sidebar:
  order: 301
---

## zval_pack()

```php
function zval_pack(mixed $value): mixed
```

Lowers `zval_pack(value)` by boxing the operand as a Mixed cell and invoking

**Parameters**:
- `$value` (`mixed`)

**Returns**: `mixed`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `zval_pack` is implemented in the compiler, see [the internals page](../../../internals/builtins/pointer/zval_pack.md).

