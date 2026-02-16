# Copyright (c) 2026 Works Applications Co., Ltd.
#
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
#     http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.

import os
import random
import unittest

from sudachipy import Dictionary


def _is_bridge_separator_surface(surface: str) -> bool:
    if not surface:
        return True
    if surface.isspace():
        return True

    for ch in surface:
        if ch in ("…", "⋯", ".", "．", "・"):
            pass
        else:
            return False
    return True


def _non_ws_surfaces(ms):
    return [m.surface() for m in ms if not _is_bridge_separator_surface(m.surface())]


class TestWhitespaceBridgeCost(unittest.TestCase):
    def setUp(self):
        resource_dir = os.path.join(os.path.dirname(os.path.abspath(__file__)), "resources")
        self.dict_ = Dictionary(os.path.join(resource_dir, "sudachi.json"), resource_dir)
        self.tokenizer_obj = self.dict_.create()

    def _internal(self, text: str) -> int:
        return self.tokenizer_obj.tokenize(text).get_internal_cost()

    def _bridged(self, text: str) -> int:
        return self.tokenizer_obj.tokenize(text).get_internal_cost_whitespace_bridged()

    def test_global_bridge_toggle_api(self):
        prev = self.tokenizer_obj.set_global_whitespace_bridge(True)
        self.assertFalse(prev)
        prev = self.tokenizer_obj.set_global_whitespace_bridge(False)
        self.assertTrue(prev)

    def test_global_bridge_non_increasing_internal_cost(self):
        texts = [
            "東京都 大学",
            "東京 都大学",
            "高輪 ゲートウェイ 駅",
            "東 京 都 大 学",
            "東京\t大学\tです",
            "東京\n大学",
            "私は 東京 大学 へ 行く",
            "すもも も もも も ももの うち",
        ]
        for text in texts:
            with self.subTest(text=text):
                self.tokenizer_obj.set_global_whitespace_bridge(False)
                normal = self.tokenizer_obj.tokenize(text).get_internal_cost()
                self.tokenizer_obj.set_global_whitespace_bridge(True)
                bridged = self.tokenizer_obj.tokenize(text).get_internal_cost()
                self.assertLessEqual(bridged, normal)

    def test_global_bridge_does_not_change_surface_sequence(self):
        texts = [
            "東京都 大学",
            "高輪 ゲートウェイ 駅",
            "私は 東京 大学 へ 行く",
            "すもも も もも も ももの うち",
            "東京 ・ 大学",
        ]
        for text in texts:
            with self.subTest(text=text):
                self.tokenizer_obj.set_global_whitespace_bridge(False)
                normal = [m.surface() for m in self.tokenizer_obj.tokenize(text)]
                self.tokenizer_obj.set_global_whitespace_bridge(True)
                bridged = [m.surface() for m in self.tokenizer_obj.tokenize(text)]
                self.assertEqual(normal, bridged)

    def test_no_whitespace_matches_internal_cost(self):
        for text in [
            "",
            "東京都大学",
            "高輪ゲートウェイ駅",
            "東京大学です",
            "ＡＢＣ123",
            "！？",
        ]:
            with self.subTest(text=text):
                ms = self.tokenizer_obj.tokenize(text)
                self.assertEqual(
                    ms.get_internal_cost(),
                    ms.get_internal_cost_whitespace_bridged(),
                )

    def test_whitespace_only_is_zero(self):
        for text in [" ", "  ", "\t", "\n", " \t\n　 "]:
            with self.subTest(text=repr(text)):
                ms = self.tokenizer_obj.tokenize(text)
                self.assertEqual(0, ms.get_internal_cost_whitespace_bridged())

    def test_whitespace_tokens_are_kept_in_output(self):
        ms = self.tokenizer_obj.tokenize("東京 大学")
        self.assertIn(" ", [m.surface() for m in ms])
        self.assertLessEqual(
            ms.get_internal_cost_whitespace_bridged(),
            ms.get_internal_cost(),
        )

    def test_spaced_variants_have_same_bridged_cost(self):
        variants = [
            "東京都 大学",
            "東京都  大学",
            "東京都\t大学",
            "東京都　大学",
            "東京都 \t 大学",
            "　東京都 大学　",
            "東京都\n大学",
        ]
        scores = [self._bridged(v) for v in variants]
        self.assertTrue(all(s == scores[0] for s in scores), msg=str(list(zip(variants, scores))))

        non_ws = [_non_ws_surfaces(self.tokenizer_obj.tokenize(v)) for v in variants]
        self.assertTrue(all(s == non_ws[0] for s in non_ws), msg=str(list(zip(variants, non_ws))))

    def test_bridged_cost_matches_compact_internal_when_non_ws_path_matches(self):
        pairs = [
            ("東京都大学", "東京都 大学"),
            ("東京大学", "東京 大学"),
        ]
        for compact, spaced in pairs:
            with self.subTest(compact=compact, spaced=spaced):
                compact_ms = self.tokenizer_obj.tokenize(compact)
                spaced_ms = self.tokenizer_obj.tokenize(spaced)
                self.assertEqual(
                    [m.surface() for m in compact_ms],
                    _non_ws_surfaces(spaced_ms),
                )
                self.assertEqual(
                    compact_ms.get_internal_cost(),
                    spaced_ms.get_internal_cost_whitespace_bridged(),
                )

    def test_falls_back_for_rewritten_nodes_without_connection_ids(self):
        # Joined/re-written nodes can have sentinel connection params, so bridged
        # scoring falls back to regular internal cost.
        text = "高輪 ゲートウェイ 駅"
        ms = self.tokenizer_obj.tokenize(text)
        self.assertEqual(
            ms.get_internal_cost(),
            ms.get_internal_cost_whitespace_bridged(),
        )

    def test_leading_trailing_whitespace_does_not_change_bridged_score(self):
        base = "東京都 大学"
        base_score = self._bridged(base)
        for text in [" " + base, base + " ", "\n" + base + "\n", "　" + base + "　"]:
            with self.subTest(text=repr(text)):
                self.assertEqual(base_score, self._bridged(text))

    def test_bridged_cost_not_greater_than_internal_for_spaced_inputs(self):
        spaced_inputs = [
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
            "東京\n大学",
            "A/B テスト",
            "１２３ ４５６",
        ]
        for text in spaced_inputs:
            with self.subTest(text=text):
                ms = self.tokenizer_obj.tokenize(text)
                self.assertLessEqual(
                    ms.get_internal_cost_whitespace_bridged(),
                    ms.get_internal_cost(),
                )

    def test_ellipsis_variants_have_same_bridged_score(self):
        variants = [
            "東京都 大学",
            "東京都…大学",
            "東京都...大学",
            "東京都・・・大学",
            "東京都⋯大学",
            "東京都．．．大学",
        ]
        scores = [self._bridged(v) for v in variants]
        self.assertTrue(all(s == scores[0] for s in scores), msg=str(list(zip(variants, scores))))

        content = [_non_ws_surfaces(self.tokenizer_obj.tokenize(v)) for v in variants]
        self.assertTrue(all(s == content[0] for s in content), msg=str(list(zip(variants, content))))

    def test_stress_random_whitespace_separators(self):
        random.seed(0)
        parts = ["東京", "大学", "です"]
        seps = [" ", "  ", "\t", "\n", "　", " \t "]

        baseline_text = " ".join(parts)
        baseline_tokens = _non_ws_surfaces(self.tokenizer_obj.tokenize(baseline_text))
        baseline_score = self._bridged(baseline_text)

        for _ in range(100):
            text = parts[0] + random.choice(seps) + parts[1] + random.choice(seps) + parts[2]
            ms = self.tokenizer_obj.tokenize(text)
            self.assertEqual(baseline_tokens, _non_ws_surfaces(ms), msg=text)
            self.assertEqual(baseline_score, ms.get_internal_cost_whitespace_bridged(), msg=text)

    def test_japanese_phrase_readability_cases(self):
        # Sanity checks for common Japanese phrases with inserted spaces.
        cases = [
            ("私は東京大学へ行く", "私は 東京 大学 へ 行く"),
            ("すもももももももものうち", "すもも も もも も ももの うち"),
            ("東京・大学", "東京 ・ 大学"),
        ]
        for compact, spaced in cases:
            with self.subTest(compact=compact, spaced=spaced):
                compact_non_ws = _non_ws_surfaces(self.tokenizer_obj.tokenize(compact))
                spaced_non_ws = _non_ws_surfaces(self.tokenizer_obj.tokenize(spaced))
                # We expect same visible token sequence except for explicit spaces.
                self.assertEqual(compact_non_ws, spaced_non_ws)



if __name__ == "__main__":
    unittest.main()
