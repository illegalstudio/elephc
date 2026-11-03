---
title: "ptr_sizeof()"
description: "Returns the byte size of the named pointer target type."
---

## ptr_sizeof()

```php
function ptr_sizeof(string $type): int
```

Returns the byte size of the named pointer target type.

**Parameters**:
- `$type` (`string`)

**Returns**: `int`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

- Hidden in strict PHP mode: this builtin is an elephc extension with no PHP equivalent, so programs compiled with [`--strict-php`](../../../compiling/cli-reference.md#strict-php-mode) treat the name as nonexistent, in compiled code and inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `ptr_sizeof` is implemented in the compiler, see [the internals page](../../../internals/builtins/pointer/ptr_sizeof.md).
