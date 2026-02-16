# Agent Notes

This repository is a fork-first implementation of `sudachi.rs`.

## Core Fork Semantics

- Cross-lex IDs use `10^8` packing (`lex_id * 10^8 + relative_word_id`).
- Python `WordInfo` exposes packed, relative, and cross-lex ID variants.
- Dictionary-form lex/source IDs are exposed directly.
- Reading-constrained candidate tokenization is cost-sorted and supports `min_tokens`.
- Whitespace-bridged scoring treats whitespace and ellipsis separators (`…`, `⋯`, `.`, `．`, `・`) as bridge separators.

## Quick Validation Commands

- Rust tests: `cargo test -p sudachi`
- Python tests:
  - `cd python`
  - `./.venv-codex/bin/pip install -e .`
  - `./.venv-codex/bin/python -m unittest discover -v tests`

## Install Targets

- Remote install: `pip install "git+https://github.com/EdwardCoventry/sudachi.rs.git@main#subdirectory=python"`
- Local editable install: `pip install -e /path/to/sudachi.rs/python`
