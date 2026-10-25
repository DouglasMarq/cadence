## 0. Scope

These rules are language-neutral. Examples in specific languages illustrate the intent — apply the equivalent construct in the project's language (Rust, Python, etc.). When the language or its tooling requires something a rule forbids, the language wins.

## 1. Think Before Coding

**Don't assume. Don't hide confusion. Surface tradeoffs.**

Before and during implementing:

- State your assumptions explicitly. If something is uncertain or unclear at any step, stop, name what's confusing and ask — never proceed on a guess.
- Check the code first; ask about what it can't answer (requirements, behavior, tradeoffs).
- If multiple interpretations exist, present them - don't pick silently.
- If a simpler approach exists, say so. Push back when warranted.

## 2. Simplicity First

**Minimum code that solves the problem. Nothing speculative.**

- No features beyond what was asked.
- No abstractions for single-use code.
- No "flexibility" or "configurability" that wasn't requested.
- No error handling for impossible scenarios.
- If you write 200 lines and it could be 50, rewrite it.

Ask yourself: "Would a senior engineer say this is overcomplicated?" If yes, simplify.

## 3. Surgical Changes

**Touch only what you must. Clean up only your own mess.**

When editing existing code:

- Don't "improve" adjacent code, comments, or formatting.
- Don't refactor things that aren't broken.
- Match existing style, even if you'd do it differently.
- If you notice unrelated dead code, mention it - don't delete it.
- The rules in `.claude/rules/` apply to new code and the lines you change, and override "Match existing style" there — existing code is refactored to them only when asked.

When your changes create orphans:

- Remove imports/variables/functions that YOUR changes made unused.
- Don't remove pre-existing dead code unless asked.

The test: Every changed line should trace directly to the user's request.

## 4. Goal-Driven Execution

**Define success criteria. Loop until verified.**

Transform tasks into verifiable goals:

- "Add validation" → "Write tests for invalid inputs, then make them pass"
- "Fix the bug" → "Write a test that reproduces it, then make it pass"
- "Refactor X" → "Ensure tests pass before and after"

For multi-step tasks, state a brief plan:

```
1. [Step] → verify: [check]
2. [Step] → verify: [check]
3. [Step] → verify: [check]
```

Strong success criteria let you loop independently. Weak criteria ("make it work") require constant clarification.

## 5. Conventions

- Commits follow Conventional Commits, written in English.
- Refactoring and behavior changes go in separate commits.
- Never put credentials, tokens or real customer data in code, tests or examples — use fictitious values.
