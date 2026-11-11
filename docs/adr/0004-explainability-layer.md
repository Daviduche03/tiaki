# ADR-0004: Explainability layer

- **Status:** accepted
- **Date:** 2026-11-07
- **Related:** TIAKI.md §8.7

## Context

Non-technical users must be able to understand and undo what the agent did. Raw diffs are not usable for them. But a summary written by the same model that made the change can lie or be wrong.

## Decision

Every transaction and generation carries a plain-language `summary`, an `impact` block, a `summary_source`, and a `summary_verified` flag.

- The structured diff and hash chain remain authoritative.
- Summaries are derived from the diff, not from the agent's memory.
- Mismatches set `summary_verified: false` and surface a warning.
- Summaries are stored at commit time and are immutable.
- The human UI is a timeline with one-line entries and a single Undo.

## Consequences

### Positive

- Non-technical users can audit and revert changes.
- Summary/diff mismatch detection catches misleading descriptions.
- Progressive disclosure serves both novices and experts.

### Negative

- Summary generation costs compute and latency.
- Mismatch detection needs a real definition of "consistent."
- Localization and reading-level support add surface area.

## Alternatives considered

- **Raw diffs only:** unusable for the target audience.
- **Trust the agent's own description:** vulnerable to misleading summaries.
