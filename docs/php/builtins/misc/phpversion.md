---
title: "phpversion()"
description: "Returns the current PHP version information."
sidebar:
  order: 283
---

## phpversion()

```php
function phpversion(): string
```

Returns the current PHP version information.

**Parameters**: none.

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/network_env/phpversion.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/network_env/phpversion.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `phpversion` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/phpversion.md).

