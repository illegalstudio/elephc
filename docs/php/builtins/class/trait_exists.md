---
title: "trait_exists()"
description: "Checks whether the trait exists."
sidebar:
  order: 88
---

## trait_exists()

```php
function trait_exists(string $trait, bool $autoload = true): bool
```

Checks whether the trait exists.

**Parameters**:
- `$trait` (`string`)
- `$autoload` (`bool`), default `true`, optional

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/symbols/trait_exists.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/symbols/trait_exists.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `trait_exists` is implemented in the compiler, see [the internals page](../../../internals/builtins/class/trait_exists.md).

