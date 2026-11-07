# Public Benchmarking and Reproducibility Standard

## Why this exists

A single fast CI line is an acceptance check, not a complete public benchmark.
SynEvid must publish enough context for another engineer to reproduce, compare,
or challenge the result.

## Required benchmark dimensions

### Workload identity

- fixture repository name and immutable content digest;
- object format: loose, packed, delta, or mixed;
- commit count, object count, file count, symbol count, edge count, and bytes;
- dirty/staged state and branch/tag/merge topology;
- cold-cache versus warm-cache definition;
- operation boundary, for example object loading only versus full semantic analysis.

### Environment identity

- operating system and exact image label;
- CPU architecture and available CPU/memory;
- Rust, Node, Python, and package-manager versions;
- release/debug profile and relevant environment variables;
- runner job ID and workflow URL;
- whether the runner is virtualized or shared.

GitHub documents that standard hosted runners are newly provisioned virtual
machines and that `-latest` labels can move to newer images. Therefore SynEvid
records the exact label and date and does not treat different runner images as
the same hardware. See the [GitHub-hosted runner reference](https://docs.github.com/en/actions/reference/runners/github-hosted-runners).

### Statistics

For performance publication, collect at least:

- sample count;
- warm-up policy;
- median, p50, p95, p99, minimum, maximum, and MAD or standard deviation;
- cold and warm distributions separately;
- throughput where a meaningful unit exists;
- CPU time, wall time, peak RSS, I/O bytes, and queue/back-pressure signals for
  resource-sensitive operations;
- failure count, timeout count, and incomplete-coverage count.

Criterion.rs is the reference model for the statistical shape of Rust
microbenchmarks: warm-up, repeated samples, confidence intervals, comparison
against a baseline, and noise analysis. Its documentation also warns that
virtualized CI and background load can create noise. See the [Criterion command
line output guide](https://bheisler.github.io/criterion.rs/book/user_guide/command_line_output.html),
[analysis process](https://bheisler.github.io/criterion.rs/book/analysis.html),
and [FAQ](https://bheisler.github.io/criterion.rs/book/faq.html).

### Correctness before speed

Every performance variant must first prove:

- identical verdict and coverage state;
- identical complete evidence set;
- identical sorted serialization and digest, or a documented intentional change;
- identical source/Git immutability for read-only operations;
- no dropped event or replay gap;
- no parser, object, or adapter error hidden by a success fallback.

### Release thresholds

The current Phase 3 acceptance threshold is a strict operation-specific
`<150 ms` for the 5,000-loose-commit Galaxy history read. It is not a universal
promise for every repository. Any future threshold must specify the operation,
workload, runner, percentile, and whether cold or warm caches are intended.

## Public report artifacts

Each benchmark publication should include Markdown narrative, machine-readable
JSON or newline-delimited JSON samples, raw command output, source commit and
fixture digest, a BLAKE3 digest of the canonical result, optional HTML charts,
a comparison to the previous approved baseline, and an explicit limitation.

OpenTelemetry semantic conventions are the recommended naming reference for
future process, runtime, queue, and system metrics. See [OpenTelemetry semantic conventions](https://opentelemetry.io/docs/concepts/semantic-conventions/)
and [metrics conventions](https://opentelemetry.io/docs/specs/semconv/general/metrics/).

## Reproducibility and artifact review

For external publication, preserve the workflow URL, commit, raw artifacts, and
reproduction command. This follows the spirit of ACM artifact review: an
artifact should be available, functional, reusable, and its reported results
should be reproducible where claimed. See [ACM artifact evaluation guidance](https://www.acm.org/publications/policies/artifact-review-and-badging-current).

## Current compliance

The current Phase 0–3 report set has direct native CI evidence for Ubuntu,
macOS, and Windows gate execution. It has exact single-operation timings for
the Phase 3 strict gate, but not yet a Criterion-style multi-sample p95/p99
study for every operation. Those missing distributions remain explicitly
`NOT MEASURED` rather than being inferred from one CI sample.

