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
use sudachi::analysis::reading_candidates::{MatchedToken, ReadingCandidatePath};
use sudachi::analysis::Mode;

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
fn reading_candidates_best_effort_marks_ordinary_path_mismatches() {
    let mut tok = TestTokenizer::new_built(Mode::C);
    tok.tok.reset().push_str("東京都");
    tok.tok.do_tokenize().expect("tokenize");

    let candidates = tok
        .tok
        .reading_candidates_best_effort("トウキョウフ", 16, 1)
        .expect("best effort");

    assert_eq!(1, candidates.len());
    assert!(!candidates[0].is_exact);
    assert_eq!(1, candidates[0].mismatch_count);
    assert_eq!(vec!["東京都".to_owned()], surfaces(&candidates[0].tokens));
    assert_eq!("トウキョウト", candidates[0].tokens[0].reading_form);
    assert_eq!("トウキョウフ", candidates[0].tokens[0].supplied_reading);
    assert!(!candidates[0].tokens[0].reading_matches);
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

#[test]
fn matched_tokens_constrain_before_result_limit() {
    let mut tok = TestTokenizer::new_built(Mode::C);
    tok.tok.reset().push_str("東京都");
    tok.tok.do_tokenize().unwrap();
    let all = tok.tok.reading_candidates("トウキョウト", 16).unwrap();
    let split = all.iter().find(|p| p.tokens.len() == 2).unwrap();
    let required: Vec<_> = split.tokens.iter().map(|t| MatchedToken {
        begin: t.begin, end: t.end, word_ids: vec![t.word_id],
    }).collect();
    let paths = tok.tok.reading_candidates_with_matched_tokens("トウキョウト", 1, 1, &required).unwrap();
    assert_eq!(paths.len(), 1);
    assert_eq!(surfaces(&paths[0].tokens), vec!["東京", "都"]);
    assert!(tok.tok.reading_candidates_with_matched_tokens("トウキョウフ", 1, 1, &required).unwrap().is_empty());
    let wrong = [MatchedToken { word_ids: vec![all[0].tokens[0].word_id], ..required[0].clone() }];
    assert!(tok.tok.reading_candidates_with_matched_tokens("トウキョウト", 1, 1, &wrong).unwrap().is_empty());
    assert!(tok.tok.reading_candidates_with_matched_tokens("トウキョウト", 1, 1, &[required[0].clone(), required[0].clone()]).is_err());
    assert!(tok.tok.reading_candidates_with_matched_tokens("トウキョウト", 1, 1, &[MatchedToken { end: 4, ..required[0].clone() }]).is_err());
    // Ordinary best-effort alignment cannot silently replace the required split.
    assert!(tok.tok.reading_candidates_best_effort_with_matched_tokens("トウキョウフ", 1, 1, &required).unwrap().is_empty());
}

#[test]
fn advisory_readings_preserve_required_identity_before_cap() {
    let mut tok = TestTokenizer::new_built(Mode::C);
    tok.tok.reset().push_str("東京都");
    tok.tok.do_tokenize().unwrap();
    let exact = tok.tok.reading_candidates("トウキョウト", 16).unwrap();
    let split = exact.iter().find(|path| path.tokens.len() == 2).unwrap();
    let required: Vec<_> = split.tokens.iter().map(|t| MatchedToken {
        begin: t.begin, end: t.end, word_ids: vec![t.word_id],
    }).collect();
    let paths = tok.tok.reading_candidates_advisory("トウキョウフ", 1, 1, &required).unwrap();
    assert_eq!(paths.len(), 1);
    assert_eq!(surfaces(&paths[0].tokens), vec!["東京", "都"]);
    assert!(!paths[0].is_exact);
    assert_eq!(paths[0].tokens[1].reading_form, "ト");
    assert_eq!(paths[0].tokens[1].supplied_reading, "フ");
    assert!(!paths[0].tokens[1].reading_matches);
    assert_eq!(paths[0].tokens[1].word_id, required[1].word_ids[0]);
    let wrong = [MatchedToken { word_ids: vec![exact[0].tokens[0].word_id], ..required[0].clone() }];
    assert!(tok.tok.reading_candidates_advisory("トウキョウフ", 1, 1, &wrong).unwrap().is_empty());
}

#[test]
fn origin_groups_are_alternatives_inside_one_span_before_caps() {
    let mut tok=TestTokenizer::new_built(Mode::C);
    tok.tok.reset().push_str("東京都");
    tok.tok.do_tokenize().unwrap();
    let all=tok.tok.reading_candidates("トウキョウト",16).unwrap();
    let split=all.iter().find(|path| path.tokens.len()==2).unwrap();
    let whole=all.iter().find(|path| path.tokens.len()==1).unwrap().tokens[0].word_id;
    let first=&split.tokens[0];
    let second=&split.tokens[1];
    let required=vec![MatchedToken {begin:first.begin,end:first.end,word_ids:vec![whole,first.word_id]},
                      MatchedToken {begin:second.begin,end:second.end,word_ids:vec![second.word_id]}];
    let paths=tok.tok.reading_candidates_with_matched_tokens("トウキョウト",1,1,&required).unwrap();
    assert_eq!(paths.len(),1);
    assert_eq!(surfaces(&paths[0].tokens),vec!["東京","都"]);
    let advisory=tok.tok.reading_candidates_advisory("トウキョウフ",1,1,&required).unwrap();
    assert_eq!(advisory.len(),1);
    assert_eq!(surfaces(&advisory[0].tokens),vec!["東京","都"]);
    assert!(!advisory[0].is_exact);
    let empty=[MatchedToken {word_ids:vec![],..required[0].clone()}];
    assert!(tok.tok.reading_candidates_with_matched_tokens("トウキョウト",0,1,&empty).is_err());
    let native=tok.tok.native_candidates(1,1,&required).unwrap();
    assert_eq!(native.len(),1);
    assert_eq!(surfaces(&native[0].tokens),vec!["東京","都"]);
    assert_eq!(native[0].tokens[0].reading_form,"トウキョウ");
    assert_eq!(native[0].tokens[0].supplied_reading,"");
    assert_eq!(native[0].total_cost,paths[0].total_cost);
    assert!(tok.tok.native_candidates(0,1,&empty).is_err());
}
