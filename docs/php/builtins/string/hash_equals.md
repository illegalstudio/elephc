---
title: "hash_equals()"
description: "Compares two strings using a constant-time algorithm."
sidebar:
  order: 359
---

## hash_equals()

```php
function hash_equals(string $known_string, string $user_string): bool
```

Compares two strings using a constant-time algorithm.

**Parameters**:
- `$known_string` (`string`)
- `$user_string` (`string`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/hash_equals.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/hash_equals.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `hash_equals` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/hash_equals.md).

