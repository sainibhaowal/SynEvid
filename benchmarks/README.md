# Benchmarks

Ground-truth evaluation of impact recall/precision, agent tool calls, cost/context and time.
Keep task truth immutable under ordinary engine PRs. The canonical protocol — pre-registered
gate, baselines, falsification rules, and honest limits — is
[`docs/benchmark/PROTOCOL.md`](../docs/benchmark/PROTOCOL.md).

---

## 1. What lives here

| Path | Contents |
|---|---|
| `corpus/` | Five TypeScript repositories used as evaluation targets (`repo-api`, `repo-cross-package`, `repo-dead-call`, `repo-migration`, `repo-relayer`). |
| `tasks/` | Fifty immutable benchmark tasks (`task_001.json` … `task_050.json`) with pre-registered ground truth. |
| `engine/` | The offline harness: `runner.py` (executes a task against a baseline), `metrics.py` (recall/precision/cost/time), `task_loader.py`, `gate_evaluator.py`, and the three baselines — `baseline_grep.py` (A: agent + grep), `baseline_lsp.py` (B: agent + LSP), `baseline_autopsy.py` (C: agent + Autopsy). |
| `tests/` | Pytest smoke tests for the harness (`python3 -m pytest benchmarks/tests/ -q`). |
| `results/` | Committed reports — see §4. |
| `run_benchmarks.py` | The harness CLI: runs all tasks across Baselines A, B, and C and evaluates the pre-registered gate. |
| `generate_tasks.py` | Task-generation helper (only used when the corpus changes; ordinary PRs never touch task truth). |

## 2. Running the suite

The harness is offline and deterministic — no network calls, no timestamps in
metric-bearing output.

```bash
# 1. Build the CLI under test
cargo build -p autopsy-cli

# 2. Run all 50 tasks across the three baselines (defaults to target/debug/autopsy;
#    AUTOPSY_BIN or --autopsy-bin overrides the binary)
python3 benchmarks/run_benchmarks.py

# 3. Harness smoke tests
python3 -m pytest benchmarks/tests/ -q

# 4. Lint / format (the same commands CI runs)
ruff check benchmarks/
ruff format --check benchmarks/
```

Python dependencies are declared in [`pyproject.toml`](pyproject.toml) (`pydantic`, `pandas`,
`pytest`; Python `>=3.12`).

## 3. The pre-registered gate

The gate and its thresholds were fixed **before** the measurements were taken; the harness
evaluates them mechanically in `engine/gate_evaluator.py`. The protocol — including the
falsification rules and the definition of each baseline — is
[`docs/benchmark/PROTOCOL.md`](../docs/benchmark/PROTOCOL.md). Gate outcomes are not
editorial: if a threshold fails, the report says so.

## 4. Committed results (the only benchmark truth)

| Report | What it records |
|---|---|
| [`results/reproduction_report.json`](results/reproduction_report.json) / [`.md`](results/reproduction_report.md) | The reproducible three-baseline measurement behind the headline claim: recall `71.67% / 71.67% / 100%` for A / B / C (**+28.33 pp** for Autopsy), precision `90% / 90% / 6.11%`, tool calls `2.50 / 1.50 / 3.00`, tokens `800 / 675 / 2338`. |
| [`results/4.2_pillar_report.json`](results/4.2_pillar_report.json) / [`.md`](results/4.2_pillar_report.md) | Pillar-level results. RQ6–RQ10 are recorded as **not measured** because their promotion-gate corpora are unavailable — never fabricated or extrapolated. |

Any number quoted in README/docs must trace to one of these files. The older `+31.73 pp`
figure survives only in frozen evidence receipts (06, 13); it is historical, not current.

## 5. Ground-truth integrity rule

`tasks/` ground truth and `corpus/` fixtures are evaluation truth, not implementation
detail. Ordinary engine PRs must never alter them to improve a score. A legitimate change
requires a dedicated PR that states why the truth was wrong, with evidence — and the
protocol's rules apply.
