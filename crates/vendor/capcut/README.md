# Vendored CapCut engine

This directory contains the Rust draft-building, FFmpeg, and validation crates
used by Artcraft's production workshop. The source was imported from the local
`capcut-cli` project on 2026-09-15 so Artcraft can build and run independently
after that standalone checkout is removed.

The Artcraft crate depends only on these local workspace packages. Draft
templates required at compile time live under `capcut/templates/_init`.
