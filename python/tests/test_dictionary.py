# Copyright (c) 2019 Works Applications Co., Ltd.
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

import sudachipy
from sudachipy import Dictionary, Tokenizer
from sudachipy.errors import SudachiError


class TestDictionary(unittest.TestCase):

    def setUp(self):
        resource_dir = os.path.join(os.path.dirname(
            os.path.abspath(__file__)), 'resources')
        self.dict_ = Dictionary(os.path.join(
            resource_dir, 'sudachi.json'), resource_dir=resource_dir)

    def tearDown(self) -> None:
        self.dict_.close()

    def test_create(self):
        self.assertEqual(Tokenizer, type(self.dict_.create()))

    def test_pos_of(self):
        self.assertIsNotNone(self.dict_.pos_of(0))

    def test_repr(self):
        repr_str = repr(self.dict_)
        self.assertTrue(repr_str.startswith("<SudachiDictionary(system="))
        self.assertTrue(repr_str.endswith("user.dic.test])>"))

    def test_lookup(self):
        ms = self.dict_.lookup("東京都")
        self.assertEqual(1, len(ms))
        self.assertEqual("トウキョウト", ms[0].reading_form())
        self.assertEqual(0, ms[0].begin())
        self.assertEqual(3, ms[0].end())
        splits = ms[0].split(sudachipy.SplitMode.A)
        self.assertEqual(2, len(splits))
        ms = self.dict_.lookup("京都", out=ms)
        self.assertEqual(1, len(ms))
        self.assertEqual("キョウト", ms[0].reading_form())
        self.assertEqual(0, ms[0].begin())
        self.assertEqual(2, ms[0].end())

    def test_word_info_by_id(self):
        ms = self.dict_.lookup("東京府")
        self.assertEqual(1, len(ms))
        word_id = ms[0].word_id()

        wi = self.dict_.word_info(word_id)
        self.assertEqual(100000002, wi.word_id)
        self.assertEqual(2**28 + 2, wi.word_id_packed)
        self.assertEqual(2, wi.word_id_relative)
        self.assertEqual(1, wi.lex_id)
        self.assertEqual(ms[0].dictionary_id(), wi.dictionary_id)
        self.assertEqual("東京府", wi.surface)
        self.assertEqual([5, 2**28 + 1], wi.a_unit_split)

        split_info = self.dict_.word_info(wi.a_unit_split[1])
        self.assertEqual(100000001, split_info.word_id)
        self.assertEqual(2**28 + 1, split_info.word_id_packed)
        self.assertEqual(1, split_info.word_id_relative)
        self.assertEqual(1, split_info.lex_id)
        self.assertEqual(1, split_info.dictionary_id)
        self.assertEqual("府", split_info.surface)

    def test_word_info_by_id_invalid(self):
        with self.assertRaises(SudachiError):
            self.dict_.word_info(15 << 28)

        with self.assertRaises(SudachiError):
            self.dict_.word_info(2 << 28)

    def test_word_info_by_id_out_of_range(self):
        sizes = self.dict_.dictionary_sizes()
        with self.assertRaises(SudachiError):
            self.dict_.word_info(sizes[0])

    def test_dictionary_sizes(self):
        sizes = self.dict_.dictionary_sizes()
        self.assertEqual(2, len(sizes))
        self.assertGreater(sizes[0], 0)
        self.assertGreater(sizes[1], 0)


if __name__ == '__main__':
    unittest.main()
