---
title: "Class builtins"
description: "Builtins in the Class category."
sidebar:
  order: 115
---

## Class builtins

| Function | Signature | Returns |
|---|---|---|
| [`class_alias()`](./class/class_alias.md) | `(string $class, string $alias, bool $autoload): bool` | `bool` |
| [`class_attribute_args()`](./class/class_attribute_args.md) | `(string $class_name, string $attribute_name): array` | `array` |
| [`class_attribute_names()`](./class/class_attribute_names.md) | `(string $class_name): array` | `array` |
| [`class_exists()`](./class/class_exists.md) | `(string $class, bool $autoload): bool` | `bool` |
| [`class_get_attributes()`](./class/class_get_attributes.md) | `(string $class_name): mixed` | `mixed` |
| [`class_implements()`](./class/class_implements.md) | `(mixed $object_or_class, bool $autoload): mixed` | `mixed` |
| [`class_parents()`](./class/class_parents.md) | `(mixed $object_or_class, bool $autoload): mixed` | `mixed` |
| [`class_uses()`](./class/class_uses.md) | `(mixed $object_or_class, bool $autoload): mixed` | `mixed` |
| [`enum_exists()`](./class/enum_exists.md) | `(string $enum, bool $autoload): bool` | `bool` |
| [`function_exists()`](./class/function_exists.md) | `(string $function): bool` | `bool` |
| [`get_class()`](./class/get_class.md) | `(object $object): string` | `string` |
| [`get_declared_classes()`](./class/get_declared_classes.md) | `(): array` | `array` |
| [`get_declared_interfaces()`](./class/get_declared_interfaces.md) | `(): array` | `array` |
| [`get_declared_traits()`](./class/get_declared_traits.md) | `(): array` | `array` |
| [`get_parent_class()`](./class/get_parent_class.md) | `(mixed $object_or_class): string` | `string` |
| [`interface_exists()`](./class/interface_exists.md) | `(string $interface, bool $autoload): bool` | `bool` |
| [`is_a()`](./class/is_a.md) | `(object $object_or_class, string $class, bool $allow_string): bool` | `bool` |
| [`is_subclass_of()`](./class/is_subclass_of.md) | `(mixed $object_or_class, string $class, bool $allow_string): bool` | `bool` |
| [`trait_exists()`](./class/trait_exists.md) | `(string $trait, bool $autoload): bool` | `bool` |
