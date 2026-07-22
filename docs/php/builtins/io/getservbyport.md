---
title: "getservbyport()"
description: "Gets the Internet service that corresponds to a port and protocol."
sidebar:
  order: 189
---

## getservbyport()

```php
function getservbyport(int $port, string $protocol): mixed
```

Gets the Internet service that corresponds to a port and protocol.

**Parameters**:
- `$port` (`int`)
- `$protocol` (`string`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/network_env/getservbyport.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/network_env/getservbyport.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `getservbyport` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/getservbyport.md).
