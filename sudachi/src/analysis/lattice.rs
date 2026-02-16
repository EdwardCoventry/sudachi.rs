/*
 *  Copyright (c) 2021-2024 Works Applications Co., Ltd.
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

use crate::analysis::inner::{Node, NodeIdx};
use crate::analysis::node::{LatticeNode, PathCost, RightId};
use crate::dic::connect::ConnectionMatrix;
use crate::dic::grammar::Grammar;
use crate::dic::lexicon_set::LexiconSet;
use crate::dic::subset::InfoSubset;
use crate::dic::word_id::WordId;
use crate::error::SudachiResult;
use crate::input_text::InputBuffer;
use crate::prelude::SudachiError;
use std::fmt::{Display, Formatter};
use std::io::Write;

/// Lattice Node for Viterbi Search.
/// Extremely small for better cache locality.
/// Current implementation has 25% efficiency loss because of padding :(
/// Maybe we should use array-of-structs layout instead, but I want to try to measure the
/// efficiency of that without the effects of the current rewrite.
struct VNode {
    total_cost: i32,
    right_id: u16,
    prev_non_ws_right_id: u16,
}

impl RightId for VNode {
    #[inline]
    fn right_id(&self) -> u16 {
        self.right_id
    }
}

impl PathCost for VNode {
    #[inline]
    fn total_cost(&self) -> i32 {
        self.total_cost
    }
}

impl VNode {
    const NONE_RIGHT_ID: u16 = u16::MAX;

    #[inline]
    fn new(right_id: u16, total_cost: i32, prev_non_ws_right_id: u16) -> VNode {
        VNode {
            right_id,
            total_cost,
            prev_non_ws_right_id,
        }
    }
}

/// Lattice which is constructed for performing the Viterbi search.
/// Contain several parallel arrays.
/// First level of parallel arrays is indexed by end word boundary.
/// Word boundaries are always aligned to codepoint boundaries, not to byte boundaries.
///
/// During the successive analysis, we do not drop inner vectors, so
/// the size of vectors never shrink.
/// You must use the size parameter to check the current size and never
/// access vectors after the end.
#[derive(Default)]
pub struct Lattice {
    ends: Vec<Vec<VNode>>,
    ends_full: Vec<Vec<Node>>,
    indices: Vec<Vec<NodeIdx>>,
    eos: Option<(NodeIdx, i32)>,
    size: usize,
    global_whitespace_bridge: bool,
}

impl Lattice {
    pub fn set_global_whitespace_bridge(&mut self, enabled: bool) -> bool {
        std::mem::replace(&mut self.global_whitespace_bridge, enabled)
    }

    /// Number of boundaries in the current lattice.
    /// For non-empty input this equals `char_len + 1`.
    pub fn boundary_count(&self) -> usize {
        self.size
    }

    /// Nodes whose end boundary is `boundary`.
    pub fn nodes_ending_at(&self, boundary: usize) -> &[Node] {
        self.ends_full
            .get(boundary)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    fn reset_vec<T>(data: &mut Vec<Vec<T>>, target: usize) {
        for v in data.iter_mut() {
            v.clear();
        }
        let cur_len = data.len();
        if cur_len <= target {
            data.reserve(target - cur_len);
            for _ in cur_len..target {
                data.push(Vec::with_capacity(16))
            }
        }
    }

    /// Prepare lattice for the next analysis of a sentence with the
    /// specified length (in codepoints)
    pub fn reset(&mut self, length: usize) {
        Self::reset_vec(&mut self.ends, length + 1);
        Self::reset_vec(&mut self.ends_full, length + 1);
        Self::reset_vec(&mut self.indices, length + 1);
        self.eos = None;
        self.size = length + 1;
        self.connect_bos();
    }

    fn connect_bos(&mut self) {
        self.ends[0].push(VNode::new(0, 0, VNode::NONE_RIGHT_ID));
    }

    /// Find EOS node -- finish the lattice construction
    pub fn connect_eos(&mut self, conn: &ConnectionMatrix) -> SudachiResult<()> {
        let len = self.size;
        let eos_start = (len - 1) as u16;
        let eos_end = (len - 1) as u16;
        let node = Node::new(eos_start, eos_end, 0, 0, 0, WordId::EOS);
        let (idx, cost, _) = self.connect_node(&node, conn);
        if cost == i32::MAX {
            Err(SudachiError::EosBosDisconnect)
        } else {
            self.eos = Some((idx, cost));
            Ok(())
        }
    }

    /// Insert a single node in the lattice, founding the path to the previous node
    /// Assumption: lattice for all previous boundaries is already constructed
    pub fn insert(&mut self, node: Node, conn: &ConnectionMatrix) -> i32 {
        let (idx, cost, prev_non_ws_right_id) = self.connect_node(&node, conn);
        let end_idx = node.end();
        self.ends[end_idx].push(VNode::new(node.right_id(), cost, prev_non_ws_right_id));
        self.indices[end_idx].push(idx);
        self.ends_full[end_idx].push(node);
        cost
    }

    /// Find the path with the minimal cost through the lattice to the attached node
    /// Assumption: lattice for all previous boundaries is already constructed
    #[inline]
    pub fn connect_node(&self, r_node: &Node, conn: &ConnectionMatrix) -> (NodeIdx, i32, u16) {
        let begin = r_node.begin();

        let node_cost = r_node.cost() as i32;
        let mut min_cost = i32::MAX;
        let mut prev_idx = NodeIdx::empty();
        let mut prev_non_ws_right_id = VNode::NONE_RIGHT_ID;

        for (i, l_vnode) in self.ends[begin].iter().enumerate() {
            if !l_vnode.is_connected_to_bos() {
                continue;
            }

            let l_node_is_whitespace = if begin == 0 {
                false
            } else {
                self.ends_full[begin][i].is_whitespace()
            };
            let normal_connect_cost = conn.cost(l_vnode.right_id(), r_node.left_id()) as i32;
            let normal_cost = l_vnode.total_cost() + normal_connect_cost + node_cost;

            let mut best_cost_for_pred = normal_cost;
            if self.global_whitespace_bridge
                && l_node_is_whitespace
                && !r_node.is_whitespace()
                && l_vnode.prev_non_ws_right_id != VNode::NONE_RIGHT_ID
            {
                let bridged_connect_cost =
                    conn.cost(l_vnode.prev_non_ws_right_id, r_node.left_id()) as i32;
                let bridged_cost = l_vnode.total_cost() + bridged_connect_cost + node_cost;
                if bridged_cost < best_cost_for_pred {
                    best_cost_for_pred = bridged_cost;
                }
            }

            if best_cost_for_pred < min_cost {
                min_cost = best_cost_for_pred;
                prev_idx = NodeIdx::new(begin as u16, i as u16);
                prev_non_ws_right_id = if r_node.is_whitespace() {
                    l_vnode.prev_non_ws_right_id
                } else {
                    r_node.right_id()
                };
            }
        }

        (prev_idx, min_cost, prev_non_ws_right_id)
    }

    /// Checks if there exist at least one at the word end boundary
    pub fn has_previous_node(&self, i: usize) -> bool {
        self.ends.get(i).map(|d| !d.is_empty()).unwrap_or(false)
    }

    /// Lookup a node for the index
    pub fn node(&self, id: NodeIdx) -> (&Node, i32) {
        let node = &self.ends_full[id.end() as usize][id.index() as usize];
        let cost = self.ends[id.end() as usize][id.index() as usize].total_cost;
        (node, cost)
    }

    /// Fill the path with the minimum cost (indices only).
    /// **Attention**: the path will be reversed (end to beginning) and will need to be traversed
    /// in the reverse order.
    pub fn fill_top_path(&self, result: &mut Vec<NodeIdx>) {
        if self.eos.is_none() {
            return;
        }
        // start with EOS
        let (mut idx, _) = self.eos.unwrap();
        result.push(idx);
        loop {
            let prev_idx = self.indices[idx.end() as usize][idx.index() as usize];
            if prev_idx.end() != 0 {
                // add if not BOS
                result.push(prev_idx);
                idx = prev_idx;
            } else {
                // finish if BOS
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dic::word_id::WordId;

    fn make_node(
        begin: u16,
        end: u16,
        left_id: u16,
        right_id: u16,
        cost: i16,
        word_id: u32,
        is_whitespace: bool,
    ) -> Node {
        let mut node = Node::new(
            begin,
            end,
            left_id,
            right_id,
            cost,
            WordId::from_raw(word_id),
        );
        node.set_whitespace(is_whitespace);
        node
    }

    fn path_word_ids(lattice: &Lattice) -> Vec<u32> {
        let mut ids = Vec::new();
        let mut idx = Vec::new();
        lattice.fill_top_path(&mut idx);
        idx.reverse();
        for i in idx {
            ids.push(lattice.node(i).0.word_id().as_raw());
        }
        ids
    }

    #[test]
    fn whitespace_bridge_can_change_best_path() {
        let n = 16usize;
        let raw = vec![0u8; n * n * 2];
        let mut conn = ConnectionMatrix::from_offset_size(&raw, 0, n, n).unwrap();

        // left chunk preference
        conn.update(1, 1, 0); // L1 -> W1
        conn.update(2, 1, 100);
        conn.update(1, 2, 100);
        conn.update(2, 2, 0); // L2 -> W2

        // normal whitespace transition is expensive
        conn.update(9, 3, 50);
        // bridged costs prefer L2 context
        conn.update(1, 3, 100);
        conn.update(2, 3, 0);

        let mut plain = Lattice::default();
        plain.reset(3);
        plain.insert(make_node(0, 1, 0, 1, 0, 1, false), &conn);
        plain.insert(make_node(0, 1, 0, 2, 1, 2, false), &conn);
        plain.insert(make_node(1, 2, 1, 9, 0, 11, true), &conn);
        plain.insert(make_node(1, 2, 2, 9, 0, 12, true), &conn);
        plain.insert(make_node(2, 3, 3, 4, 0, 21, false), &conn);
        plain.connect_eos(&conn).unwrap();
        assert_eq!(vec![1, 11, 21], path_word_ids(&plain));

        let mut bridged = Lattice::default();
        bridged.set_global_whitespace_bridge(true);
        bridged.reset(3);
        bridged.insert(make_node(0, 1, 0, 1, 0, 1, false), &conn);
        bridged.insert(make_node(0, 1, 0, 2, 1, 2, false), &conn);
        bridged.insert(make_node(1, 2, 1, 9, 0, 11, true), &conn);
        bridged.insert(make_node(1, 2, 2, 9, 0, 12, true), &conn);
        bridged.insert(make_node(2, 3, 3, 4, 0, 21, false), &conn);
        bridged.connect_eos(&conn).unwrap();
        assert_eq!(vec![2, 12, 21], path_word_ids(&bridged));
    }

    #[test]
    fn whitespace_bridge_keeps_normal_transition_when_cheaper() {
        let n = 16usize;
        let raw = vec![0u8; n * n * 2];
        let mut conn = ConnectionMatrix::from_offset_size(&raw, 0, n, n).unwrap();

        conn.update(1, 1, 0);
        conn.update(2, 1, 100);
        conn.update(1, 2, 100);
        conn.update(2, 2, 0);

        // normal transition is already best.
        conn.update(9, 3, 0);
        conn.update(1, 3, 100);
        conn.update(2, 3, 100);

        let mut plain = Lattice::default();
        plain.reset(3);
        plain.insert(make_node(0, 1, 0, 1, 0, 1, false), &conn);
        plain.insert(make_node(0, 1, 0, 2, 1, 2, false), &conn);
        plain.insert(make_node(1, 2, 1, 9, 0, 11, true), &conn);
        plain.insert(make_node(1, 2, 2, 9, 0, 12, true), &conn);
        plain.insert(make_node(2, 3, 3, 4, 0, 21, false), &conn);
        plain.connect_eos(&conn).unwrap();

        let mut bridged = Lattice::default();
        bridged.set_global_whitespace_bridge(true);
        bridged.reset(3);
        bridged.insert(make_node(0, 1, 0, 1, 0, 1, false), &conn);
        bridged.insert(make_node(0, 1, 0, 2, 1, 2, false), &conn);
        bridged.insert(make_node(1, 2, 1, 9, 0, 11, true), &conn);
        bridged.insert(make_node(1, 2, 2, 9, 0, 12, true), &conn);
        bridged.insert(make_node(2, 3, 3, 4, 0, 21, false), &conn);
        bridged.connect_eos(&conn).unwrap();

        assert_eq!(path_word_ids(&plain), path_word_ids(&bridged));
    }
}

impl Lattice {
    pub fn dump<W: Write>(
        &self,
        input: &InputBuffer,
        grammar: &Grammar,
        lexicon: &LexiconSet,
        out: &mut W,
    ) -> SudachiResult<()> {
        enum PosData<'a> {
            Bos,
            Borrow(&'a [String]),
        }

        impl Display for PosData<'_> {
            fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
                match self {
                    PosData::Bos => write!(f, "BOS/EOS"),
                    PosData::Borrow(data) => {
                        for (i, s) in data.iter().enumerate() {
                            write!(f, "{}", s)?;
                            if i + 1 != data.len() {
                                write!(f, ", ")?;
                            }
                        }
                        Ok(())
                    }
                }
            }
        }

        let mut dump_idx = 0;

        for boundary in (0..self.indices.len()).rev() {
            for r_node in &self.ends_full[boundary] {
                let (surface, pos) = if r_node.is_special_node() {
                    ("(null)", PosData::Bos)
                } else if r_node.is_oov() {
                    let pos_id = r_node.word_id().word() as usize;
                    (
                        input.curr_slice_c(r_node.begin()..r_node.end()),
                        PosData::Borrow(&grammar.pos_list[pos_id]),
                    )
                } else {
                    let winfo =
                        lexicon.get_word_info_subset(r_node.word_id(), InfoSubset::POS_ID)?;
                    (
                        input.orig_slice_c(r_node.begin()..r_node.end()),
                        PosData::Borrow(&grammar.pos_list[winfo.pos_id() as usize]),
                    )
                };

                write!(
                    out,
                    "{}: {} {} {}{} {} {} {} {}:",
                    dump_idx,
                    r_node.begin(),
                    r_node.end(),
                    surface,
                    r_node.word_id(),
                    pos,
                    r_node.left_id(),
                    r_node.right_id(),
                    r_node.cost()
                )?;

                let conn = grammar.conn_matrix();

                for l_node in &self.ends[r_node.begin()] {
                    let connect_cost = conn.cost(l_node.right_id(), r_node.left_id());
                    write!(out, " {}", connect_cost)?;
                }

                writeln!(out)?;

                dump_idx += 1;
            }
        }
        Ok(())
    }
}
