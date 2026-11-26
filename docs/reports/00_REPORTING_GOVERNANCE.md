# Reporting Governance and Evidence Contract

## Purpose

SynEvid reports are intended for maintainers, reviewers, AI-agent integrators,
and users deciding whether to rely on a result. They must answer:

1. What was tested or measured?
2. Against which repository, snapshot, fixture, and configuration?
3. On which host, operating system, architecture, and toolchain?
4. Which exact command, test, workflow, or receipt produced the result?
5. What does the result prove, and what does it not prove?
6. What changed since the previous result?

## Required evaluation record

Every evaluation report must include:

- report ID and version;
- source requirements and gate;
- tested commit and repository identity;
- fixture or real-repository identity;
- platform, architecture, toolchain, dependency versions, and runner label;
- exact command or workflow job;
- sample count and timing method;
- verdict and coverage state;
- raw-artifact or log location;
- deterministic digest when a result artifact exists;
- limitations, exclusions, and follow-up work.

The report must distinguish an executable behavioral test from a source-text
check. Source-text checks are regression guards, not proof of full user
behavior.

## Required ablation record

An ablation report is required when a change claims that a particular mechanism
improved correctness, latency, throughput, memory, coverage, or trust. It must
contain:

- hypothesis;
- baseline implementation and variant implementation;
- one fixed input corpus and one fixed configuration;
- the changed component and all held-constant components;
- correctness equivalence checks before timing comparison;
- repeated measurements, distribution statistics, and outlier policy;
- resource measurements where relevant (CPU, RSS, I/O, queue depth);
- effect size and uncertainty;
- regression and failure analysis;
- raw measurements and reproducible command;
- decision: adopt, reject, or collect more evidence.

If a paired baseline was not run, the result must be `DIRECTIONAL`, never a
causal claim. If only a correctness control exists, label the report
`NOT APPLICABLE` for performance rather than inventing a number.

## Evidence classes

| Class | Meaning | Publication use |
|---|---|---|
| A | Real process, real repository, executable behavioral test, or native CI job | Direct claim within scope |
| B | Deterministic unit, schema, golden, negative, or source-contract test | Component claim; not alone a user-workflow claim |
| C | Design, ADR, or manual review | Context and intended boundary only |
| D | Planned experiment or unavailable platform | Roadmap only; never a pass claim |

## Truth and safety rules

- `PASS` means “no violation found within analyzed scope.”
- Unsupported runtime behavior remains `UNKNOWN` or `PARTIAL`.
- Corrupt input, parser failure, missing objects, and unavailable services are
  non-success states.
- Timing values are not inserted into semantic digests.
- No report uses a hardcoded metric as observed data.
- Benchmarks must not be tuned by changing ground truth.
- Reports must not expose secrets, source contents, or unrestricted user paths.
- The 2D/3D canvas is outside the scope of backend performance claims unless a
  dedicated visual evaluation explicitly measures it.

## Publication gate

A claim can be published only if the report contains a direct evidence link and
an explicit boundary. “Production grade” means the stated gate is repeatable
and its failures are visible; it does not mean arbitrary repositories, dynamic
runtime behavior, or every hardware profile is proven.

