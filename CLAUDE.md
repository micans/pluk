# pluk

pluk is a Rust reimplementation of pick (a command-line utility, sources in pick/, tests in
pick/test/, documentation in pick/doc/). It is co-written step by step; the plan is the
documentation.

## At the start of a session

1. Read plan/README and plan/guidelines.txt (project rules, coding guidelines, intended
   differences from pick).
2. The current step equals the current git branch name (branch step3 develops plan/step3.txt).
   Read plan/step<N>.txt, plan/step<N>-notes.txt and plan/step<N>-context.txt where present,
   and the notes of the previous step.
3. Read the relevant design files in plan/ (*.design.txt) and their seeds (*.notes.txt).
4. Also read deferred.txt (items moved to later steps) and plan/sketch (draft of future steps).
5. Check `git status` and `git log --oneline` to see which step items are committed.

## Working rules (details in plan/guidelines.txt)

- Confirm before starting a step; suggest improvements to the step file first.
- Git commits, merges to main and pushes are done by the user unless they ask otherwise.
- Each item ending in "Commit." requires tests added and the full test suite passing.
- Options starting with --test are temporary; their tests are grouped separately.
- Rationales for implementation choices go in plan/step<N>-notes.txt.

## Building and testing

- Build: `cd pluk && cargo build --release`
- Tests: `cd pluk && make test` (builds pluk, runs pluk/test/Makefile, compares o.*.test with
  o.*.ref) plus `cargo test` for unit tests.
- Documentation: README.md (the introduction; its examples name doc/...) and doc/ at the
  top level (manual, SAM-NA, data files); their console examples are run by `make test`
  (pluk/test/doccheck.py).
- Reference behaviour: compare against pick (pick/pick) on the same input.
- shed/ is a scratch Cargo project for small Rust exercises.
