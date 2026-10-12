/*
 *  Copyright (c) 2026 Works Applications Co., Ltd.
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

use crate::analysis::inner::Node;
use crate::analysis::lattice::Lattice;
use crate::analysis::node::{LatticeNode, PathCost, ResultNode, RightId};
use crate::dic::connect::ConnectionMatrix;
use crate::dic::lexicon::word_infos::{WordInfo, WordInfoData};
use crate::dic::lexicon_set::LexiconSet;
use crate::dic::subset::InfoSubset;
use crate::dic::word_id::WordId;
use crate::error::{SudachiError, SudachiResult};
use crate::input_text::InputBuffer;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::collections::HashSet;
use unicode_normalization::UnicodeNormalization;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReadingCandidateToken {
    pub word_id: WordId,
    pub surface: String,
    pub reading_form: String,
    pub supplied_reading: String,
    pub reading_matches: bool,
    pub begin: usize,
    pub end: usize,
}

#[derive(Clone, Debug)]
pub struct ReadingCandidatePath {
    pub total_cost: i64,
    pub is_exact: bool,
    pub mismatch_count: usize,
    pub tokens: Vec<ReadingCandidateToken>,
}

/// One of the attested native identities required at an original-input span.
/// Groups represent genuine native origins of one derived Kiku target.
#[derive(Clone, Debug)]
pub struct MatchedToken {
    pub begin: usize,
    pub end: usize,
    pub word_ids: Vec<WordId>,
}

impl MatchedToken {
    fn permits(&self, begin: usize, end: usize, word_id: WordId) -> bool {
        if begin == end {
            return begin < self.begin || begin >= self.end;
        }
        end <= self.begin || begin >= self.end
            || (begin == self.begin && end == self.end && self.word_ids.contains(&word_id))
    }
}

#[derive(Copy, Clone, Debug)]
struct NodeRef {
    end: usize,
    index: usize,
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
struct SearchState {
    boundary: usize,
    prev_right_id: u16,
    reading_offset: usize,
}

#[derive(Copy, Clone, Debug)]
struct PathStep {
    node_ref: NodeRef,
    reading_begin: usize,
    reading_end: usize,
}

#[derive(Clone)]
struct NodeMeta {
    node: Node,
    word_info: WordInfo,
    match_variants: Vec<String>,
}

impl NodeMeta {
    fn reading_for_output(&self) -> &str {
        let reading = self.word_info.reading_form();
        if reading.is_empty() {
            self.word_info.surface()
        } else {
            reading
        }
    }

    fn as_candidate_token(&self, supplied_reading: &str) -> ReadingCandidateToken {
        let reading_form = self.reading_for_output().to_owned();
        ReadingCandidateToken {
            word_id: self.node.word_id(),
            surface: self.word_info.surface().to_owned(),
            supplied_reading: supplied_reading.to_owned(),
            reading_form,
            reading_matches: true,
            begin: self.node.begin(),
            end: self.node.end(),
        }
    }
}

fn hira_to_kata(ch: char) -> char {
    let code = ch as u32;
    if (0x3041..=0x3096).contains(&code) || (0x309d..=0x309f).contains(&code) {
        char::from_u32(code + 0x60).unwrap_or(ch)
    } else {
        ch
    }
}

fn normalize_for_matching(text: &str) -> String {
    let nfkc = text.nfkc().collect::<String>();
    let lower = nfkc
        .chars()
        .flat_map(|c| c.to_lowercase())
        .collect::<String>();
    lower.chars().map(hira_to_kata).collect::<String>()
}

fn build_match_variants(word_info: &WordInfo) -> Vec<String> {
    let mut variants = Vec::new();
    let mut seen = HashSet::new();

    let mut raws = Vec::new();
    let reading = word_info.reading_form();
    if reading.is_empty() {
        raws.push(word_info.surface());
    } else {
        raws.push(reading);
        raws.push(word_info.surface());
    }

    for raw in raws {
        let normalized = normalize_for_matching(raw);
        if normalized.is_empty() {
            continue;
        }
        if seen.insert(normalized.clone()) {
            variants.push(normalized);
        }
    }

    variants
}

fn make_word_info(
    node: &Node,
    input: &InputBuffer,
    lexicon: &LexiconSet,
    subset: InfoSubset,
) -> SudachiResult<WordInfo> {
    if node.word_id().is_oov() {
        let surface = input.curr_slice_c(node.char_range()).to_owned();
        Ok(WordInfoData {
            pos_id: node.word_id().word() as u16,
            surface,
            ..Default::default()
        }
        .into())
    } else {
        lexicon.get_word_info_subset(node.word_id(), subset)
    }
}

struct Searcher<'a> {
    require_reading: bool,
    conn: &'a ConnectionMatrix<'a>,
    reading: &'a [u8],
    reading_text: &'a str,
    end_boundary: usize,
    max_results: usize,
    min_tokens: usize,
    nodes_by_begin: &'a [Vec<NodeRef>],
    space_edges: &'a [Option<(usize, String)>],
    metas_by_end: &'a [Vec<NodeMeta>],
    path: Vec<PathStep>,
    results: Vec<ReadingCandidatePath>,
    min_cost_cache: HashMap<SearchState, Option<i64>>,
}

impl<'a> Searcher<'a> {
    fn skip_space(&self, mut state: SearchState) -> Option<SearchState> {
        let (end, text) = self.space_edges.get(state.boundary)?.as_ref()?;
        // Readings may retain actual source spaces or omit them.
        if self.require_reading && self.reading[state.reading_offset..].starts_with(text.as_bytes()) {
            state.reading_offset += text.len();
        }
        state.boundary = *end;
        Some(state)
    }

    fn new(
        conn: &'a ConnectionMatrix<'a>,
        reading: &'a str,
        end_boundary: usize,
        max_results: usize,
        min_tokens: usize,
        nodes_by_begin: &'a [Vec<NodeRef>],
        space_edges: &'a [Option<(usize, String)>],
        metas_by_end: &'a [Vec<NodeMeta>],
        require_reading: bool,
    ) -> Self {
        Self {
            require_reading,
            conn,
            reading: reading.as_bytes(),
            reading_text: reading,
            end_boundary,
            max_results,
            min_tokens,
            nodes_by_begin,
            space_edges,
            metas_by_end,
            path: Vec::new(),
            results: Vec::new(),
            min_cost_cache: HashMap::new(),
        }
    }

    fn run(mut self) -> Vec<ReadingCandidatePath> {
        let start = SearchState {
            boundary: 0,
            prev_right_id: 0,
            reading_offset: 0,
        };
        self.dfs(start, 0);
        self.results.sort_by(compare_paths);
        if self.results.len() > self.max_results {
            self.results.truncate(self.max_results);
        }
        self.results
    }

    fn worst_kept_cost(&self) -> Option<i64> {
        if self.results.len() < self.max_results {
            None
        } else {
            self.results.iter().map(|x| x.total_cost).max()
        }
    }

    fn record_result(&mut self, total_cost: i64) {
        let mut tokens = Vec::with_capacity(self.path.len());
        for step in &self.path {
            let meta = &self.metas_by_end[step.node_ref.end][step.node_ref.index];
            tokens.push(
                meta.as_candidate_token(&self.reading_text[step.reading_begin..step.reading_end]),
            );
        }
        let candidate = ReadingCandidatePath {
            total_cost,
            is_exact: true,
            mismatch_count: 0,
            tokens,
        };
        // Latent pronunciation states must not consume distinct public result slots.
        if let Some(existing) = self.results.iter_mut().find(|p| p.tokens == candidate.tokens) {
            if candidate.total_cost < existing.total_cost { *existing = candidate; }
            return;
        }
        if self.results.len() < self.max_results {
            self.results.push(candidate);
            return;
        }

        if let Some((idx, current_worst)) = self
            .results
            .iter()
            .enumerate()
            .max_by(|a, b| compare_paths(a.1, b.1))
        {
            if compare_paths(&candidate, current_worst).is_lt() {
                self.results[idx] = candidate;
            }
        }
    }

    fn min_additional_cost_from_state(&mut self, state: SearchState) -> Option<i64> {
        if let Some(cached) = self.min_cost_cache.get(&state) {
            return *cached;
        }

        let result = if let Some(next) = self.skip_space(state) {
            self.min_additional_cost_from_state(next)
        } else if state.boundary == self.end_boundary {
            if state.reading_offset == self.reading.len() {
                Some(
                    self.conn
                        .transition(state.prev_right_id, 0),
                )
            } else {
                None
            }
        } else {
            let node_refs = self.nodes_by_begin[state.boundary].clone();
            let mut best: Option<i64> = None;

            for node_ref in node_refs {
                let meta = &self.metas_by_end[node_ref.end][node_ref.index];
                let step_cost = self.conn.transition(state.prev_right_id, meta.node.left_id()) + self.conn.node_cost(&meta.node);
                for token_reading in &meta.match_variants {
                    let token_reading = token_reading.as_bytes();
                    if token_reading.is_empty() && self.require_reading {
                        continue;
                    }
                    if state.reading_offset + token_reading.len() > self.reading.len() {
                        continue;
                    }
                    if !self.reading[state.reading_offset..].starts_with(token_reading) {
                        continue;
                    }

                    let next_state = SearchState {
                        boundary: meta.node.end(),
                        prev_right_id: meta.node.right_id(),
                        reading_offset: state.reading_offset + token_reading.len(),
                    };
                    if let Some(rem) = self.min_additional_cost_from_state(next_state) {
                        let candidate = step_cost + rem;
                        best = match best {
                            None => Some(candidate),
                            Some(cur) => Some(cur.min(candidate)),
                        };
                    }
                }
            }
            best
        };

        self.min_cost_cache.insert(state, result);
        result
    }

    fn dfs(&mut self, state: SearchState, base_cost: i64) {
        if let Some(next) = self.skip_space(state) {
            self.dfs(next, base_cost);
            return;
        }
        let Some(min_additional) = self.min_additional_cost_from_state(state) else {
            return;
        };

        if let Some(worst_kept) = self.worst_kept_cost() {
            if base_cost + min_additional > worst_kept {
                return;
            }
        }

        if state.boundary == self.end_boundary {
            if state.reading_offset != self.reading.len() {
                return;
            }
            if self.path.len() < self.min_tokens {
                return;
            }
            let total_cost = base_cost
                + self
                    .conn
                    .transition(state.prev_right_id, 0);
            self.record_result(total_cost);
            return;
        }

        let node_refs = self.nodes_by_begin[state.boundary].clone();
        let mut transitions: Vec<(i64, i64, NodeRef, SearchState)> = Vec::new();

        for node_ref in node_refs {
            let meta = &self.metas_by_end[node_ref.end][node_ref.index];
            let step_cost = self.conn.transition(state.prev_right_id, meta.node.left_id()) + self.conn.node_cost(&meta.node);
            for token_reading in &meta.match_variants {
                let token_reading = token_reading.as_bytes();
                if token_reading.is_empty() && self.require_reading {
                    continue;
                }
                if state.reading_offset + token_reading.len() > self.reading.len() {
                    continue;
                }
                if !self.reading[state.reading_offset..].starts_with(token_reading) {
                    continue;
                }
                let next_state = SearchState {
                    boundary: meta.node.end(),
                    prev_right_id: meta.node.right_id(),
                    reading_offset: state.reading_offset + token_reading.len(),
                };
                if let Some(rem) = self.min_additional_cost_from_state(next_state) {
                    let est_total = base_cost + step_cost + rem;
                    transitions.push((est_total, step_cost, node_ref, next_state));
                }
            }
        }

        transitions.sort_by(|a, b| {
            if a.0 != b.0 {
                a.0.cmp(&b.0)
            } else {
                Ordering::Equal
            }
        });

        for (est_total, step_cost, node_ref, next_state) in transitions {
            if let Some(worst_kept) = self.worst_kept_cost() {
                if est_total > worst_kept {
                    continue;
                }
            }
            self.path.push(PathStep {
                node_ref,
                reading_begin: state.reading_offset,
                reading_end: next_state.reading_offset,
            });
            self.dfs(next_state, base_cost + step_cost);
            self.path.pop();
        }
    }
}

fn compare_paths(a: &ReadingCandidatePath, b: &ReadingCandidatePath) -> Ordering {
    a.total_cost.cmp(&b.total_cost)
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct AlignmentScore {
    mismatched_reading_chars: usize,
    mismatch_count: usize,
    mismatched_surface_chars: usize,
    reading_length_delta: usize,
}

impl AlignmentScore {
    fn key(self) -> (usize, usize, usize, usize) {
        (
            self.mismatched_reading_chars,
            self.mismatch_count,
            self.mismatched_surface_chars,
            self.reading_length_delta,
        )
    }

    fn with_mismatch(self, surface: &str, dictionary_reading: &str, supplied: &str) -> Self {
        let dictionary_len = dictionary_reading.chars().count();
        let supplied_len = supplied.chars().count();
        Self {
            mismatched_reading_chars: self.mismatched_reading_chars + supplied_len,
            mismatch_count: self.mismatch_count + 1,
            mismatched_surface_chars: self.mismatched_surface_chars + surface.chars().count(),
            reading_length_delta: self.reading_length_delta + dictionary_len.abs_diff(supplied_len),
        }
    }
}

#[derive(Clone, Debug)]
struct AlignmentStep {
    token_index: usize,
    reading_begin: usize,
    reading_end: usize,
    reading_matches: bool,
}

#[derive(Clone, Debug)]
struct TopPathAlignment {
    score: AlignmentScore,
    steps: Vec<AlignmentStep>,
}

fn reading_boundaries(reading: &str) -> Vec<usize> {
    let mut result = reading
        .char_indices()
        .map(|(idx, _)| idx)
        .collect::<Vec<_>>();
    result.push(reading.len());
    result
}

fn align_top_path_from(
    tokens: &[ResultNode],
    reading: &str,
    boundaries: &[usize],
    token_index: usize,
    reading_offset: usize,
    memo: &mut HashMap<(usize, usize), Option<TopPathAlignment>>,
) -> Option<TopPathAlignment> {
    if let Some(cached) = memo.get(&(token_index, reading_offset)) {
        return cached.clone();
    }

    let result = if token_index == tokens.len() {
        (reading_offset == reading.len()).then(|| TopPathAlignment {
            score: AlignmentScore::default(),
            steps: Vec::new(),
        })
    } else {
        let token = &tokens[token_index];
        let word_info = token.word_info();
        let surface = word_info.surface();
        let dictionary_reading = if word_info.reading_form().is_empty() {
            surface
        } else {
            word_info.reading_form()
        };
        let variants = build_match_variants(word_info);
        let mut best: Option<TopPathAlignment> = None;

        for variant in &variants {
            let end = reading_offset + variant.len();
            if end > reading.len()
                || !reading.is_char_boundary(end)
                || !reading[reading_offset..].starts_with(variant)
            {
                continue;
            }
            if let Some(mut suffix) =
                align_top_path_from(tokens, reading, boundaries, token_index + 1, end, memo)
            {
                suffix.steps.insert(
                    0,
                    AlignmentStep {
                        token_index,
                        reading_begin: reading_offset,
                        reading_end: end,
                        reading_matches: true,
                    },
                );
                if best
                    .as_ref()
                    .is_none_or(|current| suffix.score.key() < current.score.key())
                {
                    best = Some(suffix);
                }
            }
        }

        for &end in boundaries
            .iter()
            .filter(|&&offset| offset >= reading_offset)
        {
            if let Some(mut suffix) =
                align_top_path_from(tokens, reading, boundaries, token_index + 1, end, memo)
            {
                let supplied = &reading[reading_offset..end];
                suffix.score = suffix
                    .score
                    .with_mismatch(surface, dictionary_reading, supplied);
                suffix.steps.insert(
                    0,
                    AlignmentStep {
                        token_index,
                        reading_begin: reading_offset,
                        reading_end: end,
                        reading_matches: false,
                    },
                );
                if best
                    .as_ref()
                    .is_none_or(|current| suffix.score.key() < current.score.key())
                {
                    best = Some(suffix);
                }
            }
        }

        best
    };

    memo.insert((token_index, reading_offset), result.clone());
    result
}

/// Align a supplied reading to Sudachi's ordinary winning path.
///
/// Exact reading candidates should be attempted first. This fallback keeps the
/// ordinary token identities and makes every dictionary/supplied-reading
/// disagreement explicit on the returned token.
pub fn align_top_path_with_reading(
    top_path: &[ResultNode],
    reading: &str,
    min_tokens: usize,
) -> Option<ReadingCandidatePath> {
    if top_path.len() < min_tokens.max(1) {
        return None;
    }
    let normalized_reading = normalize_for_matching(reading);
    if normalized_reading.is_empty() {
        return None;
    }

    let boundaries = reading_boundaries(&normalized_reading);
    let mut memo = HashMap::new();
    let alignment =
        align_top_path_from(top_path, &normalized_reading, &boundaries, 0, 0, &mut memo)?;

    let tokens = alignment
        .steps
        .into_iter()
        .map(|step| {
            let token = &top_path[step.token_index];
            let word_info = token.word_info();
            let reading_form = if word_info.reading_form().is_empty() {
                word_info.surface().to_owned()
            } else {
                word_info.reading_form().to_owned()
            };
            ReadingCandidateToken {
                word_id: token.word_id(),
                surface: word_info.surface().to_owned(),
                reading_form,
                supplied_reading: normalized_reading[step.reading_begin..step.reading_end]
                    .to_owned(),
                reading_matches: step.reading_matches,
                begin: token.begin(),
                end: token.end(),
            }
        })
        .collect();

    Some(ReadingCandidatePath {
        total_cost: top_path.last().map_or(0, PathCost::total_cost),
        is_exact: alignment.score.mismatch_count == 0,
        mismatch_count: alignment.score.mismatch_count,
        tokens,
    })
}

pub fn enumerate_reading_candidates(
    lattice: &Lattice,
    input: &InputBuffer,
    lexicon: &LexiconSet,
    conn: &ConnectionMatrix,
    subset: InfoSubset,
    reading: &str,
    max_results: usize,
    min_tokens: usize,
    matched_tokens: &[MatchedToken],
) -> SudachiResult<Vec<ReadingCandidatePath>> {
    enumerate_candidates(lattice, input, lexicon, conn, subset, reading, max_results, min_tokens, matched_tokens, true, true)
}

/// Native-cost paths with hard identity constraints and explicit advisory readings.
/// Used when a complete exact reading search cannot satisfy the supplied hints.
pub fn enumerate_advisory_candidates(
    lattice: &Lattice, input: &InputBuffer, lexicon: &LexiconSet,
    conn: &ConnectionMatrix, subset: InfoSubset, reading: &str,
    max_results: usize, min_tokens: usize, matched_tokens: &[MatchedToken],
) -> SudachiResult<Vec<ReadingCandidatePath>> {
    enumerate_candidates(lattice, input, lexicon, conn, subset, reading, max_results, min_tokens, matched_tokens, false, true)
}

/// Unhinted native paths with exact identity/span constraints before ranking.
pub fn enumerate_native_candidates(
    lattice: &Lattice, input: &InputBuffer, lexicon: &LexiconSet,
    conn: &ConnectionMatrix, subset: InfoSubset,
    max_results: usize, min_tokens: usize, matched_tokens: &[MatchedToken],
) -> SudachiResult<Vec<ReadingCandidatePath>> {
    enumerate_candidates(lattice,input,lexicon,conn,subset,"",max_results,min_tokens,matched_tokens,false,false)
}

fn enumerate_candidates(
    lattice: &Lattice, input: &InputBuffer, lexicon: &LexiconSet,
    conn: &ConnectionMatrix, subset: InfoSubset, reading: &str,
    max_results: usize, min_tokens: usize, matched_tokens: &[MatchedToken],
    require_reading: bool, align_reading: bool,
) -> SudachiResult<Vec<ReadingCandidatePath>> {
    let text_len = input.original().chars().count();
    for (index, token) in matched_tokens.iter().enumerate() {
        if token.begin >= token.end || token.end > text_len {
            return Err(SudachiError::InvalidMatchedToken(format!(
                "span {}..{} is outside text of length {}", token.begin, token.end, text_len
            )));
        }
        if matched_tokens[..index].iter().any(|other|
            token.begin < other.end && token.end > other.begin) {
            return Err(SudachiError::InvalidMatchedToken("spans overlap".into()));
        }
        if token.word_ids.is_empty() {
            return Err(SudachiError::InvalidMatchedToken("required identity group is empty".into()));
        }
        for word in &token.word_ids {
            if word.is_oov() || lexicon.lexicon_size(word.dic() as usize)
                .is_none_or(|size| word.word() >= size) {
                return Err(SudachiError::InvalidMatchedToken(format!(
                    "{} is not an existing dictionary word", word
                )));
            }
        }
    }
    if max_results == 0 {
        return Ok(Vec::new());
    }
    let min_tokens = min_tokens.max(1);

    let normalized_reading = normalize_for_matching(reading);
    if (require_reading || align_reading) && normalized_reading.is_empty() {
        return Ok(Vec::new());
    }

    let boundary_count = lattice.boundary_count();
    if boundary_count == 0 {
        return Ok(Vec::new());
    }

    let end_boundary = boundary_count - 1;
    let mut nodes_by_begin = vec![Vec::new(); boundary_count];
    let space_edges: Vec<_> = (0..boundary_count).map(|begin| {
        lattice.skipped_space_end(begin).map(|end| {
            (end, normalize_for_matching(input.curr_slice_c(begin..end)))
        })
    }).collect();
    let mut metas_by_end: Vec<Vec<NodeMeta>> = Vec::with_capacity(boundary_count);
    let mut found = vec![false; matched_tokens.len()];

    let read_subset = (subset | InfoSubset::READING_FORM | InfoSubset::SURFACE).normalize();

    for end in 0..boundary_count {
        let nodes = lattice.nodes_ending_at(end);
        let mut metas = Vec::with_capacity(nodes.len());
        for node in nodes {
            let orig_begin = input.to_orig_char_idx(node.begin());
            let orig_end = input.to_orig_char_idx(node.end());
            if !matched_tokens.iter().all(|token| token.permits(orig_begin, orig_end, node.word_id())) {
                continue;
            }
            for (index, token) in matched_tokens.iter().enumerate() {
                if (orig_begin, orig_end) == (token.begin, token.end) && token.word_ids.contains(&node.word_id()) {
                    found[index] = true;
                }
            }
            let word_info = make_word_info(node, input, lexicon, read_subset)?;
            let meta = NodeMeta {
                node: node.clone(),
                match_variants: build_match_variants(&word_info),
                word_info,
            };
            nodes_by_begin[node.begin()].push(NodeRef {
                end,
                index: metas.len(),
            });
            metas.push(meta);
        }
        metas_by_end.push(metas);
    }

    if found.iter().any(|present| !present) {
        return Ok(Vec::new());
    }

    let search_reading = if require_reading { &normalized_reading } else { "" };
    if !require_reading {
        for metas in &mut metas_by_end {
            for meta in metas { meta.match_variants = vec![String::new()]; }
        }
    }
    let results = Searcher::new(
        conn,
        search_reading,
        end_boundary,
        max_results,
        min_tokens,
        &nodes_by_begin,
        &space_edges,
        &metas_by_end,
        require_reading,
    )
    .run();
    if require_reading || !align_reading { return Ok(results); }
    Ok(results.into_iter().filter_map(|candidate| {
        let nodes: Vec<_> = candidate.tokens.iter().map(|token| {
            let meta = metas_by_end[token.end].iter().find(|meta|
                meta.node.begin() == token.begin && meta.node.word_id() == token.word_id)
                .expect("candidate node exists in the filtered native lattice");
            ResultNode::new(meta.node.clone(), candidate.total_cost,
                input.to_curr_byte_idx(token.begin) as u16, input.to_curr_byte_idx(token.end) as u16,
                meta.word_info.clone())
        }).collect();
        align_top_path_with_reading(&nodes, reading, min_tokens)
    }).collect())
}

#[cfg(test)]
mod selected_space_tests {
    use super::*;

    #[test]
    fn reading_search_keeps_original_spans_across_selected_model_space_edges() {
        let raw=vec![0u8;8];
        let conn=ConnectionMatrix::from_offset_size(&raw,0,2,2).unwrap();
        let word_info: WordInfo=WordInfoData { surface:"青い".into(), reading_form:"アオイ".into(),
            ..Default::default() }.into();
        let node=Node::new(2,4,1,1,17,WordId::checked(0,1).unwrap());
        let mut metas=vec![Vec::new();6];
        metas[4].push(NodeMeta {node,word_info,match_variants:vec!["アオイ".into()]});
        let mut refs=vec![Vec::new();6];
        refs[2].push(NodeRef {end:4,index:0});
        let spaces=vec![Some((2,"  ".into())),None,None,None,Some((5," ".into())),None];
        for reading in ["アオイ","  アオイ "] {
            let results=Searcher::new(&conn,reading,5,1,1,&refs,&spaces,&metas,true).run();
            assert_eq!(results.len(),1);
            assert_eq!(results[0].total_cost,17);
            assert_eq!(results[0].tokens[0].begin,2);
            assert_eq!(results[0].tokens[0].end,4);
            assert_eq!(results[0].tokens[0].supplied_reading,"アオイ");
        }
        let advisory=Searcher::new(&conn,"",5,1,1,&refs,&spaces,&metas,false);
        let state=SearchState {boundary:0,prev_right_id:1,reading_offset:0};
        assert_eq!(advisory.skip_space(state).unwrap(),SearchState {boundary:2,..state});
    }
}
