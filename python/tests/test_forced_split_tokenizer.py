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

import re
import unittest

from sudachipy import Dictionary


def _joined_text_and_boundaries(spec: str) -> tuple[str, list[int]]:
    segments = [part for part in re.split(r"\s+", spec.strip()) if part]
    joined = "".join(segments)
    boundaries: list[int] = []
    offset = 0
    for segment in segments[:-1]:
        offset += len(segment)
        boundaries.append(offset)
    return joined, boundaries


class TestForcedSplitTokenizer(unittest.TestCase):
    def setUp(self):
        self.dict_ = Dictionary()
        self.tokenizer_obj = self.dict_.create()

    def tearDown(self):
        self.dict_.close()

    def _assert_respects_forced_boundaries(self, text_spec: str):
        joined, boundaries = _joined_text_and_boundaries(text_spec)
        ms = self.tokenizer_obj.tokenize_forced_splits(text_spec)
        surfaces = [m.surface() for m in ms]
        reconstructed = "".join(surfaces)
        self.assertEqual(joined, reconstructed)

        if not ms:
            self.assertEqual("", joined)
            return

        spans = [(m.begin(), m.end()) for m in ms]
        self.assertEqual(0, spans[0][0])
        self.assertEqual(len(joined), spans[-1][1])

        ends = [end for _, end in spans]
        for boundary in boundaries:
            self.assertIn(boundary, ends)
            self.assertTrue(all(not (begin < boundary < end) for begin, end in spans))

    def test_repeated_interjection_forces_two_tokens(self):
        ms = self.tokenizer_obj.tokenize_forced_splits("いや いや")
        self.assertEqual(["いや", "いや"], [m.surface() for m in ms])

    def test_forced_split_prevents_cross_boundary_token(self):
        normal = self.tokenizer_obj.tokenize("東京都")
        self.assertEqual("東京都", normal[0].surface())

        forced = self.tokenizer_obj.tokenize_forced_splits("東京 都")
        self.assertNotIn("東京都", [m.surface() for m in forced])
        self.assertEqual(["東京", "都"], [m.surface() for m in forced])

    def test_forced_split_respects_many_japanese_patterns(self):
        cases = [
            "いや いや",
            "見 て いる",
            "食べ られる",
            "行っ て くる",
            "書い て しまっ た",
            "読ん で いる",
            "静か だ",
            "きれい な 花",
            "とても すばらしい",
            "一 つ ずつ",
            "おはよう ございます",
            "ありがとう ござい ます",
        ]
        for text_spec in cases:
            with self.subTest(text_spec=text_spec):
                self._assert_respects_forced_boundaries(text_spec)

    def test_forced_split_collapses_multiple_space_variants(self):
        a = [m.surface() for m in self.tokenizer_obj.tokenize_forced_splits("東京 都")]
        b = [m.surface() for m in self.tokenizer_obj.tokenize_forced_splits("東京   都")]
        c = [m.surface() for m in self.tokenizer_obj.tokenize_forced_splits("東京\t都")]
        d = [m.surface() for m in self.tokenizer_obj.tokenize_forced_splits("東京　都")]
        self.assertEqual(a, b)
        self.assertEqual(a, c)
        self.assertEqual(a, d)

    def test_forced_split_without_spaces_matches_regular_tokenize(self):
        text = "東京都へ行く"
        normal = [m.surface() for m in self.tokenizer_obj.tokenize(text)]
        forced = [m.surface() for m in self.tokenizer_obj.tokenize_forced_splits(text)]
        self.assertEqual(normal, forced)

    def test_forced_split_out_param(self):
        out = self.tokenizer_obj.tokenize("東京都")
        reused = self.tokenizer_obj.tokenize_forced_splits("東京 都", out=out)
        self.assertEqual(id(out), id(reused))
        self.assertEqual(["東京", "都"], [m.surface() for m in reused])

    def test_forced_split_whitespace_only_returns_empty(self):
        ms = self.tokenizer_obj.tokenize_forced_splits(" \t　 ")
        self.assertEqual(0, len(ms))


if __name__ == "__main__":
    unittest.main()
