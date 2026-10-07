# Contributing
- Short-lived feature branches; PR required for main.
- `cargo fmt --check`, `cargo clippy -- -D warnings`, tests, schema checks must pass.
- Changes to public schemas/CLI semantics require changelog + ADR when architectural.
- Benchmark ground truth is reviewed separately from engine changes.
- Never weaken coverage reporting to turn UNKNOWN into PASS.
