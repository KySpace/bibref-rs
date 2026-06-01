# GPUI BibTeX Lookup Tool

## Goal

Build a small Rust desktop app that searches literature by DOI, title, or author and renders a Google Scholar-like BibTeX entry with an added DOI field.

## Implementation Plan

- Use GPUI for a two-panel desktop interface:
  - left panel: search box, loading/error state, compact result list.
  - each result: title, authors, year, source, and an external-link button.
  - right panel: formatted BibTeX preview and copy-to-clipboard button.
- Metadata sources:
  - Crossref REST API for DOI lookup and published-title/author search.
  - arXiv Atom API for arXiv identifiers and weak Crossref searches.
  - no Google Scholar scraping; only mimic the sample output style.
- Normalize all remote metadata into one internal `WorkRecord`.
- Generate BibTeX locally:
  - Google Scholar-like field order and two-space indentation.
  - stable citation key from first author, year, and first title word.
  - LaTeX accent escaping for common accented Latin characters.
  - include `doi = {...}` whenever known.

## Important Crates

- UI: `gpui`
  - Alternative: `gpui-component` if richer prebuilt widgets become useful.
- HTTP: `reqwest`
  - Alternative: `ureq` for blocking requests, but async `reqwest` keeps the UI responsive.
- Async runtime: `tokio`
  - Alternative: GPUI tasks only, if the app later removes async HTTP runtime needs.
- Serialization: `serde`, `serde_json`.
- arXiv Atom XML: `quick-xml`
  - Alternative: `feed-rs`, broader but heavier.
- URL/query handling: `urlencoding`.
- Clipboard: `arboard`
  - Alternative: GPUI clipboard APIs if they are stable in the selected GPUI version.
- External browser: `webbrowser`
  - Alternative: `open`, less URL-specific.
- Errors/logging: `anyhow`, `thiserror`, `tracing`, `tracing-subscriber`.
- Accent handling: explicit Unicode-to-LaTeX map.
  - Alternative: `unicode-normalization` for decomposition; avoid lossy transliteration crates for final BibTeX.

## Tests

- Input detection for DOI, arXiv ID, and normal bibliographic query.
- Crossref JSON and arXiv Atom mapping to `WorkRecord`.
- BibTeX formatting, escaping, citation key generation, DOI inclusion, and accented names.
- UI smoke path: search state, result selection, copy action, and external URL construction.
