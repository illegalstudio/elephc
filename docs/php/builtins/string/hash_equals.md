---
title: "hash_equals()"
description: "Lowers `hash_equals(known, user)` through the timing-safe runtime compare helper."
sidebar:
  order: 333
---

## hash_equals()

```php
function hash_equals(string $known_string, string $user_string): bool
```

Lowers `hash_equals(known, user)` through the timing-safe runtime compare helper.

**Parameters**:
- `$known_string` (`string`)
- `$user_string` (`string`)

**Returns**: `bool`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `hash_equals` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/hash_equals.md).

