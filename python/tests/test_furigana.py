"""Bracketed input preserves spans and treats readings as visible hints."""
import unittest
from sudachipy import parse_furigana
from . import test_matched_tokens


class FuriganaParserTest(unittest.TestCase):
    def test_spans_and_projections(self):
        parsed = parse_furigana('その場[ば]に座[すわ]る。')
        self.assertEqual('その場に座る。', parsed['text'])
        self.assertEqual('そのばにすわる。', parsed['reading'])
        for span in parsed['spans']:
            self.assertEqual(span['surface'], parsed['text'][span['start']:span['end']])
            self.assertEqual(span['reading'], parsed['reading'][span['reading_start']:span['reading_end']])
        self.assertEqual([('場', 'ば'), ('座', 'すわ')],
            [(s['surface'], s['reading']) for s in parsed['spans'] if s['annotated']])

    def test_offsets_before_unicode_normalization(self):
        parsed = parse_furigana('🙂𠮷野[よしの]Ａｶﾞ。')
        self.assertEqual((1, 3), (parsed['spans'][1]['start'], parsed['spans'][1]['end']))
        self.assertEqual('🙂よしのＡｶﾞ。', parsed['reading'])

    def test_empty_kana_and_malformed_brackets_rejected(self):
        for text in ['', '場', '場[]', '場[ba]', '場[ば', '場[ば]]', 'ば[ば]', '場[ば]\n']:
            with self.subTest(text=text), self.assertRaises(ValueError):
                parse_furigana(text)

    def test_no_silent_reading_correction(self):
        self.assertEqual('そのそのば', parse_furigana('その場[そのば]')['reading'])
        self.assertEqual('はかりごと', parse_furigana('策[はかりごと]')['reading'])


class FuriganaTokenTest(unittest.TestCase):
    setUp = test_matched_tokens.MatchedTokenTest.setUp
    tearDown = test_matched_tokens.MatchedTokenTest.tearDown
    wid = test_matched_tokens.MatchedTokenTest.wid
    search = test_matched_tokens.MatchedTokenTest.search
    def test_same_exact_native_candidates_with_explicit_policy(self):
        result = self.tokenizer.tokenize_furigana_candidates('甲乙[こうおつ]', mismatch_policy='reject')
        ordinary = self.tokenizer.tokenize_reading_candidates('甲乙', 'こうおつ')
        self.assertEqual(ordinary, result['candidates'])
        self.assertEqual('甲乙', result['text'])

    def test_advisory_unlisted_reading_keeps_native_identity(self):
        required = [(0, 1, self.wid('甲', 'コウ')), (1, 2, self.wid('乙'))]
        self.assertEqual([], self.search(required, reading='コウフ'))
        result = self.tokenizer.tokenize_furigana_candidates('甲[こう]乙[ふ]', max_results=1, matched_tokens=required)
        path = result['candidates'][0]
        self.assertFalse(path['is_exact'])
        self.assertEqual([row[2] for row in required], [t['word_id'] for t in path['tokens']])
        self.assertEqual('オツ', path['tokens'][1]['dictionary_reading_form'])
        self.assertEqual('フ', path['tokens'][1]['supplied_reading'])
        self.assertFalse(path['tokens'][1]['reading_matches'])
        self.assertFalse(path['tokens'][1]['is_oov'])

    def test_advisory_target_reading_is_also_a_hint(self):
        required = [(0, 1, self.wid('甲', 'コウ'))]
        result = self.tokenizer.tokenize_furigana_candidates('甲[かん]乙[おつ]', max_results=1, matched_tokens=required)
        self.assertEqual(required[0][2], result['candidates'][0]['tokens'][0]['word_id'])
        self.assertFalse(result['candidates'][0]['tokens'][0]['reading_matches'])

    def test_impossible_required_identity_is_not_relaxed(self):
        result = self.tokenizer.tokenize_furigana_candidates('甲[こう]乙[ふ]',
            matched_tokens=[(0, 1, self.wid('乙'))])
        self.assertEqual([], result['candidates'])

    def test_original_offsets_normalization_and_plain_input(self):
        required = [(0, 2, self.wid('ab')), (2, 4, self.wid('ガ'))]
        result = self.tokenizer.tokenize_furigana_candidates('ＡＢｶﾞ', matched_tokens=required)
        self.assertTrue(result['candidates'])
        self.assertEqual([(0, 2), (2, 4)], [(t['begin'],t['end']) for t in result['candidates'][0]['tokens']])


if __name__ == '__main__':
    unittest.main()
