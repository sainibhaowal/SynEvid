# Recipe: Repair Loop with Session Receipts

The canonical agent loop: impact → edit → verify → repair → re-verify, with every
step recorded as an honest receipt.

## Steps

1. **Start a session**
   ```bash
   autopsy session start --repo-root .
   # → session_id: <blake3 id>
   ```

2. **Record the baseline impact** (exit code 0 expected)
   ```bash
   autopsy impact -s <symbol> --format json
   autopsy session record --session-id <id> --exit-code 0 --snapshot-id <snap> -- impact -s <symbol>
   ```

3. **Make the edit** (this is the only step that mutates source — done by the agent, not autopsy)

4. **Verify against the baseline**
   ```bash
   autopsy verify --baseline-id <snap> --strict --format json
   autopsy session record --session-id <id> --exit-code <0|2|4> -- verify --baseline-id <snap> --strict
   ```

5. **On failure (exit 2/4):** read `what_failed`/`why`/`next_action` from stderr,
   repair, then re-verify. Record each attempt.

6. **Close the session**
   ```bash
   autopsy session close --session-id <id>
   # → verdict: pass | fail | unknown (derived only from recorded steps)
   # → exit code mirrors the verdict (0 pass, 2 fail, 3 unknown)
   ```

## Key invariants

- The receipt verdict is derived **only** from recorded evidence steps — never
  from wall-clock time or agent self-report.
- Close exit code mirrors the evidence exit code (0 pass, 2 fail, 3 unknown).
- Double-close returns exit 3 (`session_already_closed`).
- `--snapshot-id` on each step pins the evidence to one immutable state.

## Tips

- Use `--suggestion "the fix"` on any step to leave a correction receipt.
- Use `--disable-suggestion` in CI to suppress hints.
- Run `autopsy session list` to see all sessions and their status.
