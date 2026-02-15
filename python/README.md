# EdwardCoventry/sudachi.rs Fork - Python Notes

This fork keeps Python docs centralized in the repository root:

- [Fork README](../README.md)

For upstream baseline Python usage/docs, see:

- [Original Python README](https://github.com/WorksApplications/sudachi.rs/blob/develop/python/README.md)

Fork-specific Python additions are documented in the root README, including:

- Extended `WordInfo` ID fields (`word_id`, `word_id_packed`, `word_id_relative`, `lex_id`, dictionary-form lex fields)
- Reading candidate API: `Tokenizer.tokenize_reading_candidates(..., min_tokens=1)`
- In-memory user dictionary workflows (`build_user_dic_bytes`, `Dictionary(..., user_data=[...])`)
