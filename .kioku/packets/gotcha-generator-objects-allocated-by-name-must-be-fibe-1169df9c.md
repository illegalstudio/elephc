---
id: gotcha-generator-objects-allocated-by-name-must-be-fibe-1169df9c
type: gotcha
title: "Generator objects allocated by name must be fiber-sized"
description: "__rt_object_free_deep frees every Generator as a 232-byte fiber; a by-name allocation sized from declared properties overran its block"
created: 2026-09-23
sources:
  - path: src/codegen_support/runtime/data/user.rs
    blob: 9c2086b14a47fd230cb07dd684dc7b4ebb6fd3c2
  - path: src/codegen_support/runtime/arrays/object_free_deep.rs
    blob: 241747593bc5d8c3b27b06cc3113ceb65c7b3c2d
---

# Generator objects allocated by name must be fiber-sized

## Fact

The interpreter creates Generator objects through __elephc_eval_value_new_object -> __rt_new_by_name, which allocated the class's declared-property size. __rt_object_free_deep's Generator branch releases the coroutine stack, transfer value, start args and last key/value at fixed fiber offsets up to FIBER_OBJECT_SIZE, and zeroes them. class_by_name_allocation_size now allocates max(declared, FIBER_OBJECT_SIZE) for Fiber and Generator. Only the by-name allocation table changes: _class_object_payload_sizes drives the property walk and must stay the declared size, although the generator branch skips that walk anyway.

## Why

Every interpreter-created generator freed zeros past its allocation, corrupting whatever lived next; Symfony's route loading lost array cells depending on heap layout
