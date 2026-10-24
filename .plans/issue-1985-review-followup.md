# Nested scalar-write review follow-up (#1985 / PR2027)

- [x] Reproduce the marked key/RHS order against PHP8.5.10.
- [x] Preserve main's owned-string cleanup and string-byte view pins while rebasing.
- [x] Keep synthetic append reads attached to storage through constant propagation.
- [x] Route rejected scalar-root appends through the ordinary write-context guard.
- [x] Add marked runtime, owned-RHS cleanup, live-root replacement and structural regressions.
- [x] Pass four focused codegen tests in both IR modes and three structural optimizer tests.
- [x] Finish supported-target assembly, compiler build and source hygiene.
- [ ] Publish the existing fork branch and inspect exact-head CI.

Reviewed head29ccfaae1fd69204a5239dde3baa5385c7a24513. Main at rebase65a06c6711. Worktree/code/temp files stay inside Elephc, and unrelated root changes stay untouched.

The review initially looked like missing coverage. A computed scalar append still failed backend emission: propagation had replaced the variable in its reserved synthetic read by IntLiteral42, breaking read/push/writeback recognition. Filter only that storage-root fact, then use the existing fused write path for integer/float/resource roots. Keep computed key and RHS evaluation before the scalar Error, and select any receiver replaced by the RHS.

Rebase preserved release_error_string before the shared guard's expression-position Error and main's key-view persistence/pins. The owned-string scalar append regression checks that cleanup. All materialization fixtures compare full output and require clean heaps. Linux/iOS local evidence is assembly-only; current PR CI supplies Linux execution and the merge queue supplies skipped target tiers.
