# SynEvid (Code Autopsy) Python / uvx Package

Zero-install Python package and `uvx` runner for the **SynEvid (Code Autopsy)** offline, deterministic software verification and change-impact engine.

## Installation

```bash
# Via pip
pip install synevid

# Via pipx
pipx install synevid

# Or run instantly with zero installation via uvx:
uvx synevid --help
uvx synevid verify
uvx synevid ui --port 3333
```

## Features

- **Zero-LLM Correctness**: 100% deterministic code analysis.
- **Two-Tier Blast Radius**: Proven bottleneck vs inferred noise reachability.
- **Embedded Web Dashboard**: `synevid-ui` or `autopsy ui` with interactive graph explorer.
- **Terminal TUI**: `autopsy tui` with mouse scroll and keyboard navigation.
