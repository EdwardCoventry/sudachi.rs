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
import unittest

from sudachipy import Dictionary


class TestReadingCandidates(unittest.TestCase):
    def setUp(self):
        resource_dir = os.path.join(os.path.dirname(os.path.abspath(__file__)), "resources")
        self.dict_ = Dictionary(os.path.join(resource_dir, "sudachi.json"), resource_dir=resource_dir)
        self.tokenizer_obj = self.dict_.create()
        self.default_dict_ = Dictionary()
        self.default_tokenizer_obj = self.default_dict_.create()

    def tearDown(self):
        self.dict_.close()
        self.default_dict_.close()

    def _assert_candidate_covers_text(self, text: str, cand: dict):
        tokens = cand["tokens"]
        self.assertGreaterEqual(len(tokens), 1)
        reconstructed = "".join(t["surface"] for t in tokens)
        self.assertEqual(text, reconstructed)

        prev_end = 0
        for token in tokens:
            self.assertEqual(prev_end, token["begin"])
            self.assertGreater(token["end"], token["begin"])
            prev_end = token["end"]
        self.assertEqual(len(text), prev_end)

    def test_sorted_candidates_and_alternative_segmentation(self):
        cands = self.tokenizer_obj.tokenize_reading_candidates(
            "東京都", "トウキョウト", max_results=16
        )
        self.assertGreaterEqual(len(cands), 1)
        self.assertEqual(["東京都"], [t["surface"] for t in cands[0]["tokens"]])

        has_split = any([t["surface"] for t in c["tokens"]] == ["東京", "都"] for c in cands)
        self.assertTrue(has_split)

        costs = [c["total_cost"] for c in cands]
        self.assertEqual(costs, sorted(costs))

    def test_candidate_token_spans_cover_input(self):
        text = "東京都。"
        cands = self.tokenizer_obj.tokenize_reading_candidates(
            text, "トウキョウト。", max_results=16
        )
        self.assertGreaterEqual(len(cands), 1)
        for c in cands:
            self._assert_candidate_covers_text(text, c)

    def test_no_match_and_limit(self):
        cands = self.tokenizer_obj.tokenize_reading_candidates(
            "東京都", "トウキョウフ", max_results=16
        )
        self.assertEqual([], cands)

        limited = self.tokenizer_obj.tokenize_reading_candidates(
            "東京都", "トウキョウト", max_results=1
        )
        self.assertEqual(1, len(limited))
        self.assertEqual(["東京都"], [t["surface"] for t in limited[0]["tokens"]])

    def test_best_effort_localizes_gifu_contextual_reading(self):
        candidates = self.default_tokenizer_obj.tokenize_reading_candidates(
            "僕はお義父さんのために",
            "ボクハオトウサンノタメニ",
            mismatch_policy="silent",
        )

        self.assertEqual(1, len(candidates))
        candidate = candidates[0]
        self.assertFalse(candidate["is_exact"])
        self.assertEqual(1, candidate["mismatch_count"])
        mismatches = [token for token in candidate["tokens"] if not token["reading_matches"]]
        self.assertEqual(
            [("義父", "ギフ", "トウ")],
            [
                (token["surface"], token["reading_form"], token["supplied_reading"])
                for token in mismatches
            ],
        )

    def test_best_effort_localizes_naota_kanji_fragment(self):
        candidates = self.default_tokenizer_obj.tokenize_reading_candidates(
            "太",
            "タ",
            mismatch_policy="silent",
        )

        self.assertEqual(1, len(candidates))
        token = candidates[0]["tokens"][0]
        self.assertEqual(("太", "フトシ", "タ", False), (
            token["surface"],
            token["reading_form"],
            token["supplied_reading"],
            token["reading_matches"],
        ))

    def test_best_effort_mismatch_policy_warns_or_errors(self):
        with self.assertWarns(RuntimeWarning):
            warned = self.default_tokenizer_obj.tokenize_reading_candidates(
                "太", "タ", mismatch_policy="warn"
            )
        self.assertEqual(1, len(warned))

        with self.assertRaisesRegex(ValueError, "太/フトシ/タ"):
            self.default_tokenizer_obj.tokenize_reading_candidates(
                "太", "タ", mismatch_policy="error"
            )

    def test_best_effort_oov_policy_converts_only_mismatched_tokens(self):
        candidate = self.default_tokenizer_obj.tokenize_reading_candidates(
            "僕はお義父さんのために",
            "ボクハオトウサンノタメニ",
            mismatch_policy="oov",
        )[0]

        mismatch = next(token for token in candidate["tokens"] if not token["reading_matches"])
        self.assertEqual("義父", mismatch["surface"])
        self.assertEqual("トウ", mismatch["reading_form"])
        self.assertEqual("ギフ", mismatch["dictionary_reading_form"])
        self.assertEqual("トウ", mismatch["supplied_reading"])
        self.assertTrue(mismatch["coerced_to_oov"])
        self.assertTrue(mismatch["is_oov"])
        self.assertEqual(-2, mismatch["lex_id"])
        self.assertEqual(-2, mismatch["word_id"])
        self.assertIsNone(mismatch["word_id_packed"])
        self.assertGreaterEqual(mismatch["source_lex_id"], 0)
        self.assertGreaterEqual(mismatch["source_word_id"], 0)

        exact_tokens = [token for token in candidate["tokens"] if token["reading_matches"]]
        self.assertTrue(exact_tokens)
        self.assertTrue(all(not token["coerced_to_oov"] for token in exact_tokens))

    def test_case_width_and_symbol_variants(self):
        for reading in ("A/B", "a/b", "aキゴウb", "ａ／ｂ"):
            with self.subTest(reading=reading):
                cands = self.default_tokenizer_obj.tokenize_reading_candidates(
                    "A/B", reading, max_results=16
                )
                self.assertGreaterEqual(len(cands), 1)

    def test_hiragana_and_number_surface_variants(self):
        hira = self.default_tokenizer_obj.tokenize_reading_candidates(
            "東京都", "とうきょうと", max_results=16
        )
        self.assertGreaterEqual(len(hira), 1)

        number_surface = self.default_tokenizer_obj.tokenize_reading_candidates(
            "第3話", "ダイ3ワ", max_results=16
        )
        self.assertGreaterEqual(len(number_surface), 1)

        number_reading = self.default_tokenizer_obj.tokenize_reading_candidates(
            "第3話", "ダイサンワ", max_results=16
        )
        self.assertGreaterEqual(len(number_reading), 1)

    def test_min_tokens_filters_single_token_candidates(self):
        with_single = self.default_tokenizer_obj.tokenize_reading_candidates(
            "東京都", "トウキョウト", max_results=16, min_tokens=1
        )
        self.assertGreaterEqual(len(with_single), 1)
        self.assertEqual(["東京都"], [t["surface"] for t in with_single[0]["tokens"]])

        no_single = self.default_tokenizer_obj.tokenize_reading_candidates(
            "東京都", "トウキョウト", max_results=16, min_tokens=2
        )
        self.assertGreaterEqual(len(no_single), 1)
        self.assertTrue(all(len(c["tokens"]) >= 2 for c in no_single))
        self.assertTrue(all([t["surface"] for t in c["tokens"]] != ["東京都"] for c in no_single))

    def test_min_tokens_too_large_returns_empty(self):
        no_path = self.default_tokenizer_obj.tokenize_reading_candidates(
            "東京都", "トウキョウト", max_results=16, min_tokens=10
        )
        self.assertEqual([], no_path)

    def test_min_tokens_zero_is_treated_as_one(self):
        as_one = self.default_tokenizer_obj.tokenize_reading_candidates(
            "東京都", "トウキョウト", max_results=16, min_tokens=1
        )
        as_zero = self.default_tokenizer_obj.tokenize_reading_candidates(
            "東京都", "トウキョウト", max_results=16, min_tokens=0
        )
        self.assertEqual(as_one, as_zero)


if __name__ == "__main__":
    unittest.main()
