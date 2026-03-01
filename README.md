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
- `lex_id`: source lexicon id for the token

Public boundary rule:

- `Dictionary.word_info(...)` expects `word_id` / cross-lex ids only
- packed native Sudachi ids are internal and not part of the public API contract
- any `>= 10^8` value is treated as a cross-lex id only; invalid lex ids or rows raise errors
- split arrays in `WordInfo` strip packed user-lex ids to cross-lex ids when they are unambiguous
- small relative split ids remain relative because they are lex-context-dependent, not packed

This fork keeps two parallel views of the same real lexicon entry:

- public/cross-lex: `lex_id * 10^8 + relative_word_id`
- internal/native packed: `(lex_id << 28) | relative_word_id`

Example for a real token with `lex_id = 3` and `relative_word_id = 500`:

- `word_id = 300000500`
- `word_id_packed = 805306868`
- `word_id_relative = 500`
- `lex_id = 3`

`Dictionary.word_info(300000500)` resolves that token and `WordInfo` exposes all four values above.

Dictionary-form metadata:

- `dictionary_form_word_id`
- `dictionary_form_word_id_packed`
- `dictionary_form_word_id_relative`
- `dictionary_form_lex_id`
- `is_dictionary_form`
- `is_inflected`

This lets callers determine source lexicon and dictionary-form source lexicon without app-side inference.
For real lexicon entries:

- `dictionary_form_word_id` is always cross-lex
- `dictionary_form_word_id_packed` is always the canonical native packed ID
- `dictionary_form_word_id_relative` is always lex-relative
- `dictionary_form_lex_id` is always the source lex id

Current runtime semantics for lex ids:

- `0` = system dictionary
- `>0` = user/custom dictionaries supplied to Sudachi
- `-1` = missing/non-lexicon placeholder
- `-2` = OOV/special token
- `<-2` values are reserved for custom post-processing lex ids

Current runtime semantics for dictionary-form IDs:

- fields are integer-only (no `"*"` output)
- non-inflected tokens (POS conjugation type/form are `*`, `*`) expose:
  - `dictionary_form_word_id = -1`
  - `dictionary_form_word_id_packed = -1`
  - `dictionary_form_word_id_relative = -1`
  - `dictionary_form_lex_id = -1`
  - `is_dictionary_form = true`
  - `is_inflected = false`
- inflected tokens with raw dictionary-form id `-1` resolve to self IDs (same `word_id` and `lex_id`)
- inflected tokens with explicit dictionary-form references resolve to the referenced lemma IDs

## 2) Dictionary-form parsing and cross-lex behavior

For user-dictionary CSV inputs, dictionary-form id values keep compatibility with existing workflows:

- `*` and `-1` are accepted in the dictionary-form id column and treated as `WordId::INVALID`
- cross-lex references are preserved
- cross-lex `10^8` offset IDs (`lex_id * 10^8 + relative_id`) are accepted where relevant

Notes:

- the Python bridge does not special-case CSV parsing itself; it forwards lexicon data to Sudachi's builder
- this fork has regression coverage for both `*` and `-1` dictionary-form id inputs
- For inflected entries, `WordId::INVALID` resolves to self IDs at runtime.
- For non-inflected entries, runtime `WordInfo` dictionary-form ID fields are exposed as `-1` (not self IDs), and `is_dictionary_form` should be used for dictionary-form checks.

## 3) User dictionary build/load additions for migration flows

Python bindings include byte-oriented paths in addition to file-based flows:

- `build_user_dic_bytes(system, lex, description=None)` returns compiled dictionary bytes
- `Dictionary(..., user_data=[...])` loads user dictionaries from in-memory bytes

File-based build workflows remain available.

## 4) Tests added for fork behavior

Fork-specific tests include coverage for:

- reading-candidate enumeration and sorting
- `min_tokens` filtering behavior
- variant matching (case/width/kana/symbol-like cases)
- user dictionary build and ID-field compatibility cases

## 5) New tokenization methods

### 5.1) Whitespace bridge and ellipsis separators

Whitespace bridging is a scoring feature, not a different token output format.

Python (`Tokenizer` / `MorphemeList`):

- `set_global_whitespace_bridge(enabled)`
- `get_internal_cost_whitespace_bridged()`

Bridge scoring and global whitespace-bridge tokenization treat all of the following as bridge separators:

- Unicode whitespace
- `…` and `⋯`
- dot variants and Japanese middle dot: `.`, `．`, `・`
- repeated forms such as `...`, `．．．`, `・・・`, including tokenizer-split fragments

This keeps costs connected across spacing/ellipsis separators without changing token output.

### 5.2) Forced-split tokenization by whitespace

New API to tokenize text while **enforcing token boundaries at whitespace positions**:

- whitespace is removed before analysis
- each boundary between whitespace-separated segments is mandatory
- best path is chosen under those boundary constraints

Python (`Tokenizer`):

- `tokenize_forced_splits(text, mode=None)`

Typical usage:

- force boundaries from compound specs where spaces are semantic split markers
- e.g. `"いや いや"` enforces a split in `"いやいや"` and returns best-cost tokens under that constraint

Example:

- input: `"いや いや"`
- analyzed text: `"いやいや"`
- enforced boundary: between the two `いや`
- output tokens: `["いや", "いや"]`

### 5.3) Reading-constrained candidate tokenization

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

## Install this fork

Install from GitHub (works on another machine):

- `pip install "git+https://github.com/EdwardCoventry/sudachi.rs.git@main#subdirectory=python"`

Install locally in editable mode (for local development):

- `pip install -e /path/to/sudachi.rs/python`

## Status

This fork is intentionally opinionated for cross-lex ID compatibility and migration needs.
If behavior differs from upstream, treat this README as the source of truth for fork semantics.

## Upstream Base

Base project: [WorksApplications/sudachi.rs](https://github.com/WorksApplications/sudachi.rs)

License remains Apache-2.0 per upstream.
