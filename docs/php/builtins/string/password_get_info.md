---
title: "password_get_info()"
description: "Returns the algorithm and options a password hash was made with."
sidebar:
  order: 1029
---

## password_get_info()

```php
function password_get_info(string $hash): array
```

Returns the algorithm and options a password hash was made with.

**Parameters**:
- `$hash` (`string`)

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected password prelude.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/password_get_info.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/password_get_info.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `password_get_info` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/password_get_info.md).
