## Rules

- Test names follow Given/When/Then (`Given an expired card, when paying, then it returns 402`). In Go, where test functions must start with `Test`, the `t.Run` subtest name follows it.
- New and changed code is covered at 80% of lines and 70% of branches.
- Prefer integration and E2E tests over unit tests.
