---
title: "zval_pack()"
description: "Packs an elephc runtime value into a heap-allocated PHP zval pointer."
sidebar:
  order: 305
---

## zval_pack()

```php
function zval_pack(mixed $value): pointer
```

Packs an elephc runtime value into a heap-allocated PHP zval pointer.

**Parameters**:
- `$value` (`mixed`)

**Returns**: `pointer`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: not available inside eval'd code.
- **Strict PHP mode**: hidden — this builtin is an elephc extension with no PHP equivalent, so programs compiled with [`--strict-php`](../../../compiling/cli-reference.md#strict-php-mode) treat the name as nonexistent, in compiled code and inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `zval_pack` is implemented in the compiler, see [the internals page](../../../internals/builtins/pointer/zval_pack.md).

