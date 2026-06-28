---
title: "ctype_space()"
description: "Lowers `ctype_space(string)` by checking every byte against PHP's ASCII whitespace set."
sidebar:
  order: 392
---

## ctype_space()

```php
function ctype_space(string $text): bool
```

Lowers `ctype_space(string)` by checking every byte against PHP's ASCII whitespace set.

**Parameters**:
- `$text` (`string`)

**Returns**: `bool`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `ctype_space` is implemented in the compiler, see [the internals page](../../../internals/builtins/type/ctype_space.md).

