# Tests

- `integration/` — cross-subsystem flows (transaction → snapshot → revert)
- `chaos/` — power loss, disk full, agent loops, plugin crashes
- `replay/` — replay an action log and assert the resulting state

The replay harness is the answer to "how do you test a self-modifying OS" (TIAKI.md §23, open question 10).
