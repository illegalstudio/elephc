---
title: "similar_text()"
description: "Calculates the similarity between two strings."
sidebar:
  order: 513
---

## similar_text()

```php
function similar_text(string $string1, string $string2, mixed $percent = null): int
```

Calculates the similarity between two strings.

**Parameters**:
- `$string1` (`string`)
- `$string2` (`string`)
- `$percent` (`mixed`), passed by reference, default `null`, optional

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `similar_text` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/similar_text.md).
