# PR #1746 shadowed-property follow-up

- [x] Reproduce the duplicate private property on the reviewed head and compare full PHP output.
- [x] Resolve physical property metadata through ancestry before filtering/rendering rows.
- [x] Select the visible slot for explicit debug-info projections.
- [x] Add a full-output regression covering all three renderers and document the public-name divergence.
- [x] Run focused renderer regressions and supported-target checks.
- [x] Prepare the verified review response.

Reviewed head: `f307660259487e5364e2bd31f6f297268d3b9b30`.

A user private `__name` shadow leaves the inherited backing slot in the physical property
list, while the name maps now describe the child's slot. The reviewed program prints the
child twice in var_dump, print_r and var_export. PHP prints it once (plus its public name
property, which Elephc still does not model; the existing divergence is explicitly retained).

For each physical index, the renderer now walks ancestry until the name resolves to that
index, then uses that metadata for backing-slot filtering, visibility, offset and value tag.
A debug-info projection resolves its visible slot first so a shadow does not select an
inherited tuple by accident. Evidence: `/private/tmp/elephc-1746-review`.

Final evidence:
- The full-output shadow regression passes with optimization on/off. Added PHP-exact controls
  for private shadow slots with different types and a debug-info projection of the user slot.
- Focused Reflection, private-property and debug-info renderer groups pass.
- Linux x86_64 and ARM64 execute all three new fixtures with optimization on/off: twelve
  expected full outputs. The ordinary/private and projection fixtures are PHP-exact; the
  Reflection full renderer fixture deliberately retains the documented missing-public-name
  divergence. These rendering probes make no heap-cleanliness claim.
- macOS ARM64, iOS device and iOS Simulator assemble the runtime and both shadow program modes:
  nine objects, without claiming iOS execution.
- `cargo build --lib` is warning-free; the compiler executable was built by the focused test
  build before the exact-source probes. `git diff --check` passes.
