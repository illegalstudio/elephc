# Tagged nullable scalar PR refresh (#1996 / PR2000)

- [x] Fetch exact current PR head and inspect the failed CI job.
- [x] Rebase while preserving main's nullable-pointer truthiness/empty arms and null/sort regressions.
- [x] Pass40 related tests with IR on and6 direct truthiness/null/sort/pointer controls with IR off.
- [ ] Verify emitter/comment hygiene and fresh main integration, then update the existing fork branch.
- [ ] Observe a fresh complete CI run with its build-archive producers.

Reviewed head6342737e53709a328271ca0d29b47e25f26d2942. The failing macOS CLI job did not run tests: nextest-archive-macos-aarch64 was absent. Its run's134 retained artifacts contain no nextest archives. A new push must generate a complete workflow; a failed-job-only rerun cannot restore the producer.

Dispatch nullable scalar unions using their TaggedScalar storage representation. Keep main's raw-address arm for nullable pointers and packed values; they must not be interpreted as Mixed cells. Both upstream null/sort tests and the PR tagged-scalar test remain registered.

All own worktrees, code, temporary files and artifacts remain inside Elephc. No public review replies or merges are implied.

Ten supported-target assemblies pass, and emitter comment alignment is valid. The ci:macos label was added before the new push so the previously failing tier and its archive producers run together. No codegen test failed in the old missing-archive job.
