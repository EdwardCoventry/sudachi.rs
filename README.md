# EdwardCoventry/sudachi.rs Fork

This repository is a **fork-focused implementation** with custom ID and dictionary behavior.

This README documents only the fork-specific behavior and APIs added/changed for this project.
For general Sudachi installation, CLI usage, and baseline Python API docs, see upstream:

- [Original sudachi.rs README](https://github.com/WorksApplications/sudachi.rs/blob/develop/README.md)
- [Original Python README](https://github.com/WorksApplications/sudachi.rs/blob/develop/python/README.md)
- [Original Japanese README](https://github.com/WorksApplications/sudachi.rs/blob/develop/README.ja.md)

## Fork Objectives

- Preserve cross-lex integer word ID semantics using `10^8` offsets across multiple lexicons.
- Expose lexicon identity directly in Python `WordInfo` (including dictionary-form references).
- Preserve/extend user dictionary build behavior for migration workflows.
- Add reading-constrained candidate tokenization with cost-sorted alternatives.

## Fork Changes

## 1) Extended WordInfo IDs in Python

`WordInfo` now exposes explicit ID variants for both token IDs and dictionary-form IDs:

- `word_id`: cross-lex `10^8` offset integer (`lex_id * 10^8 + relative_word_id`)
- `word_id_packed`: native internal packed u32
- `word_id_relative`: ID relative to the current lexicon
- `lex_id` and `dictionary_id`: dictionary index for the token

Dictionary-form metadata:

- `dictionary_form_word_id`
- `dictionary_form_word_id_packed`
- `dictionary_form_word_id_relative`
- `dictionary_form_lex_id`

This lets callers determine source lexicon and dictionary-form source lexicon without app-side inference.

## 2) Dictionary-form parsing and cross-lex behavior

User dictionary build parsing accepts dictionary-form values with parity to existing workflows:

- `*` and `-1` are treated as `WordId::INVALID` (self-fallback behavior)
- cross-lex references are preserved
- cross-lex `10^8` offset IDs (`lex_id * 10^8 + relative_id`) are accepted where relevant

## 3) User dictionary build/load additions for migration flows

Python bindings include byte-oriented paths in addition to file-based flows:

- `build_user_dic_bytes(system, lex, description=None)` returns compiled dictionary bytes
- `Dictionary(..., user_data=[...])` loads user dictionaries from in-memory bytes

File-based build workflows remain available.

## 4) Reading-constrained candidate tokenization

New API to enumerate all tokenization candidates whose concatenated reading matches a given reading string, sorted by total path cost.

Rust (`StatefulTokenizer`):

- `reading_candidates(reading, max_results)`
- `reading_candidates_with_min_tokens(reading, max_results, min_tokens)`

Python (`Tokenizer`):

- `tokenize_reading_candidates(text, reading, max_results=64, min_tokens=1)`

Notes:

- `min_tokens` default is `1`
- use `min_tokens=2` to suppress one-token exact matches
- matching includes normalization for width/case and kana normalization to improve practical matching

## 5) Tests added for fork behavior

Fork-specific tests include coverage for:

- reading-candidate enumeration and sorting
- `min_tokens` filtering behavior
- variant matching (case/width/kana/symbol-like cases)
- user dictionary build and ID-field compatibility cases

## Status

This fork is intentionally opinionated for cross-lex ID compatibility and migration needs.
If behavior differs from upstream, treat this README as the source of truth for fork semantics.

## Upstream Base

Base project: [WorksApplications/sudachi.rs](https://github.com/WorksApplications/sudachi.rs)

License remains Apache-2.0 per upstream.
