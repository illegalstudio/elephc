---
title: The passing-mode alias cell must be reset on every execution of a call site
author: Guillaume
date: 2026-11-24
branch: bead-byrefquery3
touches:
  - src/ir_lower/expr/descriptor_native_selections.rs
  - src/ir_lower/expr/descriptor_invoke.rs
  - tests/fixtures/descriptor_element_arguments_in_loops.php
---

Root cause of elephc-yg2f ($cb($rows[$i][0]) wrong result, leak, free-list corruption with SimpleXMLElement declared): descriptor_native_selections::prepare mints a synthetic alias local per call site, stores null, promotes it to a ref cell (owner slot + a runtime state word, 1 = promoted) and the invoke retires only the OWNER (release_local_ref_cell). The alias slot kept the pointer to the retired cell and its state word stayed 1, so the second execution of the site (a loop body, or a catch in a loop after a throwing callee, whose unwind retires the owner too) released and stored through the stale cell: corruption with an object element, one leaked cell with a scalar one. Fix: emit UnsetLocal + ZeroLocalSlot on the alias slot BEFORE its first store (covers the throw path) and after the retirement in emit_callable_descriptor_invoke_as (the same detach `unset($ref)` performs; unset_local also clears the state word at run time). Before the fix only the first iteration of a loop worked.

Method that made this fast: a project dir with elephc.toml declaring libxml2 (copy /private/tmp/claude-501/borrowed-write-native/, run `elephc native install --target <host>` offline) lets the CLI compile SimpleXML programs in seconds (`ELEPHC_CLOSED_WORLD=strict elephc --heap-debug main.php`), versus minutes through the codegen harness; `--emit-ir` / `--emit-asm` show the slot offsets (a `ldr x10,[state]; cbnz` pair before release_local_slot is the ref-bound check).

Verifying the x86_64 routine: no Rosetta on this host (`arch -x86_64` says Bad CPU type) and no cross cc for the managed libxml2 package, so a SimpleXML program cannot be built for linux-x86_64. The routine alone was run instead: dump it with a throwaway #[test] calling emit_callable_param_is_by_ref for Target::new(Platform::Linux, Arch::X86_64), prefix `.intel_syntax noprefix`, link with a C driver in `docker run --platform linux/amd64 elephc-test-linux-x86_64 gcc driver.c rt.s` and compare against a C model of CallArgumentModes::parameter_index over 1456 signature/name/position cases (0 mismatches; the aarch64 body passed the same cases natively).

Open, filed: by-reference callee write-back to array elements/properties through a descriptor is not implemented (only SimpleXML selections observe the query); interpreter-linked builds leak a row with `$cb($box->items[0][1])` (identical on base).
