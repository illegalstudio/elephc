# Foreach map fusion measured no gain (reverted)

- Tried lowering `foreach (array_map(fn($v) => <pure arithmetic>, $ints) as $x)` to iterate the
  source directly and prepend `$x = <body($x)>`, with guards for source mentions, by-reference
  iteration, and reference aliases.
- Release measurement on `array_map_foreach` (5k maps x 100 elements, 5 runs, 2 warmups):
  baseline without the pass 20.579 ms, with the pass 21.732 ms — inside noise and not better.
- Root cause: the checker still types the foreach value from the original `array<mixed>` map
  result, so `$x` is a Mixed local and every body add/compare pays Mixed boxing. Removing the
  intermediate array does not offset that, and it adds alias-snapshot complexity.
- Reverted the foreach transform. Kept the alias analysis as a correctness guard for the
  assignment fusion: a reference alias created before `$mapped = array_map(...)` must keep the
  generic map, because PHP snapshots the source while the fused read is deferred.
- Kept regression:
  `tests/codegen/optimizer/array_map_read_fusion.rs::test_map_read_alias_before_assignment_keeps_generic_map`.
