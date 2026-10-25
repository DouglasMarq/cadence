## Rules

- Every error response (4xx/5xx) from a new endpoint is an [RFC 9457](https://www.rfc-editor.org/info/rfc9457) Problem Details document — `Content-Type: application/problem+json` with the members `type`, `title`, `status`, `detail` and `instance`.

## Anti-patterns

- NEVER change an existing endpoint's error format unless asked — it breaks its clients.
- NEVER let `status` differ from the actual HTTP response status.
- NEVER make clients depend on `title` or `detail` to identify the error — `type` is the identifier; `detail` only explains this occurrence.
- NEVER put stack traces, SQL, internal exception messages or credentials in any member.
