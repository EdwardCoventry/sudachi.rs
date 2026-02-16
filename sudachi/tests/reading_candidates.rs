/*
 * Copyright (c) 2026 Works Applications Co., Ltd.
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

mod common;

use common::TestStatefulTokenizer as TestTokenizer;
use sudachi::analysis::Mode;
use sudachi::analysis::reading_candidates::ReadingCandidatePath;

fn surfaces(path: &[sudachi::analysis::reading_candidates::ReadingCandidateToken]) -> Vec<String> {
    path.iter().map(|t| t.surface.clone()).collect()
}

fn assert_candidate_paths_cover_text(text: &str, candidates: &[ReadingCandidatePath]) {
    for cand in candidates {
        assert!(!cand.tokens.is_empty());
        let mut prev_end = 0usize;
        let mut reconstructed = String::new();
        for token in &cand.tokens {
            assert_eq!(prev_end, token.begin);
            assert!(token.end > token.begin);
            reconstructed.push_str(&token.surface);
            prev_end = token.end;
        }
        assert_eq!(text.chars().count(), prev_end);
        assert_eq!(text, reconstructed);
    }
}

#[test]
fn reading_candidates_sorted_and_include_alternative_path() {
    let mut tok = TestTokenizer::new_built(Mode::C);
    tok.tok.reset().push_str("東京都");
    tok.tok.do_tokenize().expect("tokenize");

    let candidates = tok
        .tok
        .reading_candidates("トウキョウト", 16)
        .expect("candidates");

    assert!(!candidates.is_empty());
    assert_eq!(vec!["東京都".to_owned()], surfaces(&candidates[0].tokens));

    let has_split = candidates
        .iter()
        .any(|c| surfaces(&c.tokens) == vec!["東京".to_owned(), "都".to_owned()]);
    assert!(has_split);

    for i in 1..candidates.len() {
        assert!(candidates[i - 1].total_cost <= candidates[i].total_cost);
    }
}

#[test]
fn reading_candidates_have_valid_spans_and_cover_input() {
    let mut tok = TestTokenizer::new_built(Mode::C);
    let text = "東京都。";
    tok.tok.reset().push_str(text);
    tok.tok.do_tokenize().expect("tokenize");

    let candidates = tok
        .tok
        .reading_candidates("トウキョウト。", 16)
        .expect("candidates");
    assert!(!candidates.is_empty());
    assert_candidate_paths_cover_text(text, &candidates);
}

#[test]
fn reading_candidates_no_match_and_limit() {
    let mut tok = TestTokenizer::new_built(Mode::C);
    tok.tok.reset().push_str("東京都");
    tok.tok.do_tokenize().expect("tokenize");

    let no_match = tok
        .tok
        .reading_candidates("トウキョウフ", 16)
        .expect("no match");
    assert!(no_match.is_empty());

    let limited = tok
        .tok
        .reading_candidates("トウキョウト", 1)
        .expect("limited");
    assert_eq!(1, limited.len());
}

#[test]
fn reading_candidates_handles_case_width_and_symbols() {
    let mut tok = TestTokenizer::new_built(Mode::C);
    tok.tok.reset().push_str("A/B");
    tok.tok.do_tokenize().expect("tokenize");

    let upper_surface = tok
        .tok
        .reading_candidates("A/B", 16)
        .expect("upper surface");
    assert!(!upper_surface.is_empty());

    let lower_surface = tok
        .tok
        .reading_candidates("a/b", 16)
        .expect("lower surface");
    assert!(!lower_surface.is_empty());

    let fullwidth = tok.tok.reading_candidates("ａ／ｂ", 16).expect("fullwidth");
    assert!(!fullwidth.is_empty());
}

#[test]
fn reading_candidates_handles_hiragana_and_numeric_surface_style() {
    let mut tok = TestTokenizer::new_built(Mode::C);
    tok.tok.reset().push_str("東京都");
    tok.tok.do_tokenize().expect("tokenize");

    let hira = tok
        .tok
        .reading_candidates("とうきょうと", 16)
        .expect("hiragana");
    assert!(!hira.is_empty());

    tok.tok.reset().push_str("123");
    tok.tok.do_tokenize().expect("tokenize");
    let surface_number = tok
        .tok
        .reading_candidates("123", 16)
        .expect("surface number");
    assert!(!surface_number.is_empty());

    let width_number = tok
        .tok
        .reading_candidates("１２３", 16)
        .expect("width number");
    assert!(!width_number.is_empty());
}

#[test]
fn reading_candidates_min_tokens_filters_single_token_paths() {
    let mut tok = TestTokenizer::new_built(Mode::C);
    tok.tok.reset().push_str("東京都");
    tok.tok.do_tokenize().expect("tokenize");

    let with_single = tok
        .tok
        .reading_candidates_with_min_tokens("トウキョウト", 16, 1)
        .expect("with single");
    assert!(!with_single.is_empty());
    assert_eq!(vec!["東京都".to_owned()], surfaces(&with_single[0].tokens));

    let no_single = tok
        .tok
        .reading_candidates_with_min_tokens("トウキョウト", 16, 2)
        .expect("without single");
    assert!(!no_single.is_empty());
    assert!(no_single.iter().all(|c| c.tokens.len() >= 2));
    assert!(no_single
        .iter()
        .all(|c| surfaces(&c.tokens) != vec!["東京都".to_owned()]));
}

#[test]
fn reading_candidates_min_tokens_too_large_returns_empty() {
    let mut tok = TestTokenizer::new_built(Mode::C);
    tok.tok.reset().push_str("東京都");
    tok.tok.do_tokenize().expect("tokenize");

    let no_path = tok
        .tok
        .reading_candidates_with_min_tokens("トウキョウト", 16, 10)
        .expect("no path");
    assert!(no_path.is_empty());
}
