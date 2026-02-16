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

use common::TestStatefulTokenizer;
use common::TestTokenizer;
use sudachi::prelude::Mode;

fn is_bridge_separator_surface(surface: &str) -> bool {
    if surface.is_empty() {
        return true;
    }
    if surface.chars().all(char::is_whitespace) {
        return true;
    }
    for ch in surface.chars() {
        match ch {
            '…' | '⋯' | '.' | '．' | '・' => {}
            _ => return false,
        }
    }
    true
}

fn content_surfaces(data: &str) -> Vec<String> {
    let tok = TestTokenizer::new();
    tok.tokenize(data, Mode::C)
        .iter()
        .filter_map(|m| {
            let s = m.surface().to_string();
            if is_bridge_separator_surface(&s) {
                None
            } else {
                Some(s)
            }
        })
        .collect()
}

#[test]
fn whitespace_bridge_matches_internal_without_whitespace() {
    let tok = TestTokenizer::new();
    for text in ["", "東京都大学", "高輪ゲートウェイ駅", "東京大学です"] {
        let ms = tok.tokenize(text, Mode::C);
        assert_eq!(
            ms.get_internal_cost(),
            ms.get_internal_cost_whitespace_bridged(),
            "text={text:?}"
        );
    }
}

#[test]
fn whitespace_bridge_ignores_spacing_variants() {
    let tok = TestTokenizer::new();
    let variants = [
        "東京都 大学",
        "東京都  大学",
        "東京都\t大学",
        "東京都　大学",
    ];
    let base = tok
        .tokenize(variants[0], Mode::C)
        .get_internal_cost_whitespace_bridged();
    for v in variants.iter().skip(1) {
        let score = tok
            .tokenize(v, Mode::C)
            .get_internal_cost_whitespace_bridged();
        assert_eq!(base, score, "variant={v:?}");
    }
}

#[test]
fn whitespace_bridge_matches_compact_when_non_ws_path_matches() {
    let tok = TestTokenizer::new();
    let compact = "東京都大学";
    let spaced = "東京都 大学";
    let compact_ms = tok.tokenize(compact, Mode::C);
    let compact_internal = compact_ms.get_internal_cost();
    let spaced_bridge = tok
        .tokenize(spaced, Mode::C)
        .get_internal_cost_whitespace_bridged();
    assert_eq!(content_surfaces(spaced), content_surfaces(compact));
    assert_eq!(compact_internal, spaced_bridge);
}

#[test]
fn whitespace_bridge_cost_is_not_greater_than_internal() {
    let tok = TestTokenizer::new();
    for text in [
        "東京都 大学",
        "東京都…大学",
        "東京都...大学",
        "東京都・・・大学",
        "東京都⋯大学",
        "東京都．．．大学",
        "東京 都大学",
        "高輪 ゲートウェイ 駅",
        "東 京 都 大 学",
        "東京\t大学\tです",
    ] {
        let ms = tok.tokenize(text, Mode::C);
        assert!(
            ms.get_internal_cost_whitespace_bridged() <= ms.get_internal_cost(),
            "text={text:?}"
        );
    }
}

#[test]
fn whitespace_bridge_ignores_ellipsis_variants() {
    let tok = TestTokenizer::new();
    let variants = [
        "東京都 大学",
        "東京都…大学",
        "東京都...大学",
        "東京都・・・大学",
        "東京都⋯大学",
        "東京都．．．大学",
    ];
    let base_cost = tok
        .tokenize(variants[0], Mode::C)
        .get_internal_cost_whitespace_bridged();
    let base_surfaces = content_surfaces(variants[0]);
    for v in variants.iter().skip(1) {
        let score = tok
            .tokenize(v, Mode::C)
            .get_internal_cost_whitespace_bridged();
        assert_eq!(base_cost, score, "variant={v:?}");
        assert_eq!(base_surfaces, content_surfaces(v), "variant={v:?}");
    }
}

#[test]
fn global_bridge_non_increasing_internal_cost_in_stateful_tokenizer() {
    let text = "私は 東京 大学 へ 行く";
    let mut tok = TestStatefulTokenizer::new_built(Mode::C);

    tok.tok.set_global_whitespace_bridge(false);
    let normal = tok.tokenize(text).get_internal_cost();

    tok.tok.set_global_whitespace_bridge(true);
    let bridged = tok.tokenize(text).get_internal_cost();

    assert!(bridged <= normal, "normal={normal}, bridged={bridged}");
}
