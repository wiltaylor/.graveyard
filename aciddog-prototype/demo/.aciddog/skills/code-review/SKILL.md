---
description: Systematic code review covering correctness, clarity, and test coverage
---

# Code review

Use this skill when a teammate sends you a diff, a file, or a specific
change to review. Work through the checks below in order and stop at the
first blocker — don't pile on nits when the change has a real defect.

## 1. Understand the intent

- Read the change against the sending agent's message: what problem is
  this trying to solve?
- If the intent is unclear, `send` a single clarifying question back
  before reviewing. Don't guess.

## 2. Correctness

- Read every touched function top-to-bottom. Note any path that can
  panic, silently drop errors, or skip a required step.
- Check boundary conditions the author likely glossed over: empty
  inputs, zero-length collections, timezone or locale assumptions,
  concurrent access, failure of the external call that was just added.
- Flag any mutation of shared state that isn't obviously synchronised.

## 3. Clarity

- One concern per block: a function or module that's doing three things
  is a review comment, not a style preference.
- Name-vs-behaviour mismatches. A `validate_*` that also mutates, a
  `get_*` that hits the network — call them out.
- Comments that restate the code add noise. Comments that capture a
  non-obvious *why* are valuable. Push for the latter.

## 4. Tests

- Does the change come with a test that would have failed before and
  passes after? If not, name the specific test that should be added.
- Are the existing tests still relevant? A change that forces a test
  rewrite usually means the test was coupled to the implementation.

## 5. Report

Reply with a short structured message:

- **Blocker(s)** — bullets, each one specific enough to act on (file,
  line, what's wrong, what to do).
- **Suggestions** — optional improvements, clearly labelled so the
  author can ignore them.
- **Looks good** — if nothing rises to a blocker or suggestion, say so
  plainly and close out.

Keep the whole reply under ~20 lines unless the change is large.
