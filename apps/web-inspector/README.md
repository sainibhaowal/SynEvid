# Web Inspector

A lightweight, zero-backend, read-only visual forensic Evidence Board and Blast Radius explorer for SynEvid (Code Autopsy) artifacts.

## What it renders

| Artifact | Produced by | View |
|---|---|---|
| Verification report | `autopsy verify --format json` | Forensic Evidence Board, Blast Radius & Multigraph |
| Revision journal | `autopsy temporal --format json` | Temporal Evolution Timeline |
| Organisational blast radius | `autopsy blast-radius <component-ref> --format json` | Distributed Blast Radius |

Anything else is **refused** with a message naming what the page does render, rather than
shown as an empty case. The page derives nothing the engine already states: coverage, notes
and unresolved references are displayed as the engine reported them, an unresolved reference
is never drawn as an edge, and a component the catalog does not contain is shown as a refusal
to answer instead of an empty graph. The inspector is offline by construction — it has no
network API, no storage write and no clock.

## Usage

Simply open `index.html` directly in any web browser, or serve locally:

```bash
# Direct browser opening:
xdg-open index.html   # On Linux
open index.html       # On macOS

# Or serve via local static server:
npx serve -l 3000 .
```

Then produce an artifact and load it in the *Autopsy Report JSON* tab:

```bash
cargo run -p autopsy-cli -- --repo-root /path/to/repo --format json temporal > journal.json
cargo run -p autopsy-cli -- --repo-root /path/to/repo --format json \
  blast-radius component:default/checkout > blast-radius.json
```

## Tests

```bash
npm test
```

The page has no build step, so the tests extract the script the browser would run and run
that in a `vm` context with a minimal DOM stub — the same bytes, plus checks that the page
stays read-only and time-free.
