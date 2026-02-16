/*
 *  Copyright (c) 2021 Works Applications Co., Ltd.
 *
 *  Licensed under the Apache License, Version 2.0 (the "License");
 *  you may not use this file except in compliance with the License.
 *  You may obtain a copy of the License at
 *
 *      http://www.apache.org/licenses/LICENSE-2.0
 *
 *   Unless required by applicable law or agreed to in writing, software
 *  distributed under the License is distributed on an "AS IS" BASIS,
 *  WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 *  See the License for the specific language governing permissions and
 *  limitations under the License.
 */

extern crate lazy_static;
extern crate sudachi;

use std::ops::Deref;
use sudachi::prelude::Mode;

mod common;
use crate::common::TestStatefulTokenizer as TestTokenizer;
use common::LEX_CSV;

#[test]
fn empty() {
    let mut tok = TestTokenizer::new_built(Mode::C);
    let ms = tok.tokenize("");
    assert_eq!(0, ms.len());
}

#[test]
fn tokenize_small_katakana_only() {
    let mut tok = TestTokenizer::new_built(Mode::C);
    let ms = tok.tokenize("ァ");
    assert_eq!(1, ms.len());
}

#[test]
fn get_word_id() {
    let mut tok = TestTokenizer::new_built(Mode::C);
    let ms = tok.tokenize("京都");
    assert_eq!(1, ms.len());
    let m0 = ms.get(0);
    let pos = m0.part_of_speech();
    assert_eq!(&["名詞", "固有名詞", "地名", "一般", "*", "*"], pos);

    // we do not have word_id field in Morpheme and skip testing.
    let ms = tok.tokenize("ぴらる");
    assert_eq!(1, ms.len());
    let m0 = ms.get(0);
    let pos = m0.part_of_speech();
    assert_eq!(&["名詞", "普通名詞", "一般", "*", "*", "*"], pos);
}

#[test]
fn get_dictionary_id() {
    let mut tok = TestTokenizer::new_built(Mode::C);
    let ms = tok.tokenize("京都");
    assert_eq!(1, ms.len());
    assert_eq!(0, ms.get(0).dictionary_id());

    let ms = tok.tokenize("ぴらる");
    assert_eq!(1, ms.len());
    assert_eq!(1, ms.get(0).dictionary_id());

    let ms = tok.tokenize("京");
    assert_eq!(1, ms.len());
    assert!(ms.get(0).dictionary_id() < 0);
}

#[test]
fn get_synonym_group_id() {
    let mut tok = TestTokenizer::new_built(Mode::C);
    let ms = tok.tokenize("京都");
    assert_eq!(1, ms.len());
    assert_eq!([1, 5], ms.get(0).synonym_group_ids());

    let ms = tok.tokenize("ぴらる");
    assert_eq!(1, ms.len());
    assert!(ms.get(0).synonym_group_ids().is_empty());

    let ms = tok.tokenize("東京府");
    assert_eq!(1, ms.len());
    assert_eq!([1, 3], ms.get(0).synonym_group_ids());
}

#[test]
fn tokenize_kanji_alphabet_word() {
    let mut tok = TestTokenizer::new_built(Mode::C);
    assert_eq!(1, tok.tokenize("特a").len());
    assert_eq!(1, tok.tokenize("ab").len());
    assert_eq!(2, tok.tokenize("特ab").len());
}

#[test]
fn tokenize_with_dots() {
    let mut tok = TestTokenizer::new_built(Mode::C);
    let ms = tok.tokenize("京都…");
    assert_eq!(4, ms.len());
    assert_eq!("…", ms.get(1).surface().deref());
    assert_eq!(".", ms.get(1).normalized_form());
    assert_eq!("", ms.get(2).surface().deref());
    assert_eq!(".", ms.get(2).normalized_form());
    assert_eq!("", ms.get(3).surface().deref());
    assert_eq!(".", ms.get(3).normalized_form());
}

#[test]
fn tokenizer_morpheme_split() {
    let mut tok = TestTokenizer::new_built(Mode::C);
    let ms = tok.tokenize("東京都");
    assert_eq!(1, ms.len());
    assert_eq!("東京都", ms.get(0).surface().deref());

    tok.set_mode(Mode::A);
    let ms = tok.tokenize("東京都");
    assert_eq!(2, ms.len());
    assert_eq!("東京", ms.get(0).surface().deref());
    assert_eq!("都", ms.get(1).surface().deref());
}

#[test]
fn split_middle() {
    let mut tok = TestTokenizer::new_built(Mode::C);
    let ms = tok.tokenize("京都東京都京都");
    assert_eq!(ms.len(), 3);
    let m = ms.get(1);
    assert_eq!(m.surface().deref(), "東京都");

    let mut ms_a = ms.empty_clone();
    assert!(m.split_into(Mode::A, &mut ms_a).expect("works"));
    assert_eq!(ms_a.len(), 2);
    assert_eq!(ms_a.get(0).surface().deref(), "東京");
    assert_eq!(ms_a.get(0).begin_c(), 2);
    assert_eq!(ms_a.get(0).end_c(), 4);
    assert_eq!(ms_a.get(0).begin(), 6);
    assert_eq!(ms_a.get(0).end(), 12);
    assert_eq!(ms_a.get(1).surface().deref(), "都");
    assert_eq!(ms_a.get(1).begin_c(), 4);
    assert_eq!(ms_a.get(1).end_c(), 5);
    assert_eq!(ms_a.get(1).begin(), 12);
    assert_eq!(ms_a.get(1).end(), 15);
}

const OOV_CFG: &[u8] = include_bytes!("resources/sudachi.oov.json");

#[test]
fn istanbul_is_not_splitted() {
    let mut tok = TestTokenizer::builder(LEX_CSV).config(OOV_CFG).build();
    let ms = tok.tokenize("İstanbul");
    assert_eq!(ms.len(), 1);
}

#[test]
fn emoji_are_not_splitted() {
    let mut tok = TestTokenizer::builder(LEX_CSV).config(OOV_CFG).build();
    assert_eq!(tok.tokenize("⏸").len(), 1);
    assert_eq!(tok.tokenize("🦹‍♂️").len(), 1);
    assert_eq!(tok.tokenize("🎅🏾").len(), 1);
    assert_eq!(tok.tokenize("👳🏽‍♂").len(), 1);
}

#[test]
fn zeros_are_accepted() {
    let mut tok = TestTokenizer::builder(LEX_CSV).config(OOV_CFG).build();
    let ms = tok.tokenize("京都\0いく");
    assert_eq!(ms.len(), 3);
    assert_eq!(ms.get(0).surface().deref(), "京都");
    assert_eq!(ms.get(1).surface().deref(), "\0");
    assert_eq!(ms.get(2).surface().deref(), "いく");

    let ms = tok.tokenize("\0京都いく");
    assert_eq!(ms.len(), 3);
    assert_eq!(ms.get(0).surface().deref(), "\0");
    assert_eq!(ms.get(1).surface().deref(), "京都");
    assert_eq!(ms.get(2).surface().deref(), "いく");
}

#[test]
fn morpheme_extraction() {
    let mut tok = TestTokenizer::builder(LEX_CSV).config(OOV_CFG).build();
    let entries = tok.entries("東京都");
    assert_eq!(1, entries.len());
    let e = entries.get(0);
    assert_eq!("東京都", e.surface().deref());
    assert_eq!(0, e.begin());
    assert_eq!(9, e.end());
    assert_eq!(0, e.begin_c());
    assert_eq!(3, e.end_c());
}

#[test]
fn global_whitespace_bridge_keeps_surface_sequence_and_non_increasing_cost() {
    let text = "すもも も もも も ももの うち";
    let mut tok = TestTokenizer::new_built(Mode::C);

    tok.tok.set_global_whitespace_bridge(false);
    let normal = tok.tokenize(text);
    let normal_cost = normal.get_internal_cost();
    let normal_surfaces: Vec<String> = normal.iter().map(|m| m.surface().to_string()).collect();

    tok.tok.set_global_whitespace_bridge(true);
    let bridged = tok.tokenize(text);
    let bridged_cost = bridged.get_internal_cost();
    let bridged_surfaces: Vec<String> = bridged.iter().map(|m| m.surface().to_string()).collect();

    assert_eq!(normal_surfaces, bridged_surfaces);
    assert!(bridged_cost <= normal_cost);
}
