---
title: "ptr()"
description: "Returns a raw pointer to the given variable."
---

## ptr()

```php
function ptr(mixed $value): pointer
```

Returns a raw pointer to the given variable.

**Parameters**:
- `$value` (`mixed`)

**Returns**: `pointer`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

- Hidden in strict PHP mode: this builtin is an elephc extension with no PHP equivalent, so programs compiled with [`--strict-php`](../../../compiling/cli-reference.md#strict-php-mode) treat the name as nonexistent, in compiled code and inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `ptr` is implemented in the compiler, see [the internals page](../../../internals/builtins/pointer/ptr.md).
