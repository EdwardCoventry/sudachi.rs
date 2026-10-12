"""Required lexical identities constrain native search before its result cap."""
import gc
import tempfile
import unittest
from pathlib import Path

from sudachipy import Config, Dictionary, sudachipy
from sudachipy.errors import SudachiError


class MatchedTokenTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.system = self.root / 'system.dic'
        entries = [('甲', 'コウ', 100), ('甲', 'カン', 110),
                   ('乙', 'オツ', 100), ('甲乙', 'コウオツ', 50),
                   ('ab', 'エービー', 100), ('ガ', 'ガ', 100)]
        lex = '\n'.join(f'{s},0,0,{cost},{s},名詞,普通名詞,一般,*,*,*,{r},{s},*,A,*,*,*'
                        for s, r, cost in entries) + '\n'
        sudachipy.build_system_dic(b'1 1\n0 0 0\n', [lex.encode()], self.system)
        self.config = Config(system=str(self.system),
            inputTextPlugin=[{'class': 'com.worksap.nlp.sudachi.DefaultInputTextPlugin'}],
            pathRewritePlugin=[], connectionCostPlugin=[],
            oovProviderPlugin=[{'class': 'com.worksap.nlp.sudachi.SimpleOovPlugin',
                'oovPOS': ['名詞', '普通名詞', '一般', '*', '*', '*'],
                'leftId': 0, 'rightId': 0, 'cost': 10000}])
        self.dictionary = Dictionary(config=self.config)
        self.tokenizer = self.dictionary.create()
        self.extra = []

    def tearDown(self):
        self.tokenizer = None
        for d in self.extra:
            d.close()
        self.dictionary.close()
        gc.collect()
        self.temp.cleanup()

    def wid(self, surface, reading=None):
        return next(m.word_id() for m in self.dictionary.lookup(surface)
                    if reading is None or m.reading_form() == reading)

    def search(self, required=None, text='甲乙', reading='コウオツ', **kwargs):
        return self.tokenizer.tokenize_reading_candidates(text, reading,
            matched_tokens=required, **kwargs)

    def test_limit_applies_after_constraints(self):
        self.assertEqual(['甲乙'], [t['surface'] for t in self.search(max_results=1)[0]['tokens']])
        required = [(0, 1, self.wid('甲', 'コウ')), (1, 2, self.wid('乙'))]
        paths = self.search(required, max_results=1)
        self.assertEqual(['甲', '乙'], [t['surface'] for t in paths[0]['tokens']])
        self.assertEqual([r[2] for r in required], [t['word_id'] for t in paths[0]['tokens']])
        native=self.tokenizer.tokenize_native_candidates('甲乙',max_results=1,matched_tokens=required)
        self.assertEqual([r[2] for r in required],[t['word_id'] for t in native[0]['tokens']])
        self.assertEqual('unhinted',native[0]['reading_policy'])
        self.assertEqual(['コウ','オツ'],[t['reading_form'] for t in native[0]['tokens']])
        self.assertEqual(paths[0]['total_cost'],native[0]['total_cost'])

    def test_forced_boundaries_apply_to_every_candidate_path(self):
        options=dict(max_results=1,forced_boundaries=[1])
        paths=[self.tokenizer.tokenize_native_candidates('甲乙',**options),
               self.tokenizer.tokenize_reading_candidates('甲乙','コウオツ',**options),
               self.tokenizer.tokenize_reading_candidates('甲乙','コウフ',mismatch_policy='advisory',**options),
               self.tokenizer.tokenize_furigana_candidates('甲[こう]乙[おつ]',**options)['candidates']]
        for candidates in paths:
            self.assertEqual(1,len(candidates))
            self.assertEqual(['甲','乙'],[t['surface'] for t in candidates[0]['tokens']])
        joint=[(0,2,self.wid('甲乙'))]
        self.assertEqual([],self.tokenizer.tokenize_native_candidates('甲乙',matched_tokens=joint,**options))
        self.assertEqual([],self.tokenizer.tokenize_reading_candidates('甲乙','コウオツ',matched_tokens=joint,**options))
        normalized=self.tokenizer.tokenize_native_candidates('ＡＢｶﾞ',forced_boundaries=[2],max_results=1)
        self.assertEqual(['ＡＢ','ｶﾞ'],[t['surface'] for t in normalized[0]['tokens']])
        for search in [lambda:self.tokenizer.tokenize_native_candidates('ＡＢｶﾞ',forced_boundaries=[3]),
                       lambda:self.tokenizer.tokenize_reading_candidates('ＡＢｶﾞ','エービーガ',forced_boundaries=[3])]:
            with self.assertRaises(SudachiError):
                search()

    def test_origin_groups_keep_all_valid_alternatives_before_limit(self):
        origins=[self.wid('甲','コウ'),self.wid('甲','カン')]
        required=[(0,1,origins),(1,2,self.wid('乙'))]
        for reading,expected in [('コウオツ',origins[0]),('カンオツ',origins[1])]:
            paths=self.search(required,reading=reading,max_results=1)
            self.assertEqual(expected,paths[0]['tokens'][0]['word_id'])
            annotated='甲['+('こう' if expected==origins[0] else 'かん')+']乙[おつ]'
            furigana=self.tokenizer.tokenize_furigana_candidates(annotated,max_results=1,matched_tokens=required)
            self.assertEqual(expected,furigana['candidates'][0]['tokens'][0]['word_id'])
        advisory=self.search(required,reading='カンフ',max_results=1,mismatch_policy='advisory')
        # Advisory hints retain native path scoring rather than forcing a reading.
        self.assertIn(advisory[0]['tokens'][0]['word_id'],origins)
        self.assertEqual((0,1),(advisory[0]['tokens'][0]['begin'],advisory[0]['tokens'][0]['end']))
        self.assertFalse(advisory[0]['is_exact'])
        with self.assertRaises(ValueError):
            self.search([(0,1,[])],max_results=0)
        duplicate=self.search([(0,1,[origins[0],origins[0]])],max_results=1)
        self.assertEqual(origins[0],duplicate[0]['tokens'][0]['word_id'])

    def test_homograph_identity_reading_and_span_are_exact(self):
        self.assertEqual([], self.search([(0, 1, self.wid('甲', 'カン'))]))
        self.assertTrue(self.search([(0, 1, self.wid('甲', 'カン'))], reading='カンオツ'))
        self.assertEqual([], self.search([(0, 2, self.wid('甲', 'コウ'))]))
        self.assertEqual([], self.search([(0, 1, self.wid('乙'))]))

    def test_original_unicode_offsets_survive_normalization(self):
        required = [(0, 2, self.wid('ab')), (2, 4, self.wid('ガ'))]
        paths = self.search(required, text='ＡＢｶﾞ', reading='エービーガ', max_results=1)
        self.assertEqual(['ＡＢ', 'ｶﾞ'], [t['surface'] for t in paths[0]['tokens']])
        self.assertEqual([(0, 2), (2, 4)], [(t['begin'], t['end']) for t in paths[0]['tokens']])
        self.assertEqual([], self.search([(2, 3, self.wid('ガ'))], text='ＡＢｶﾞ', reading='エービーガ'))

    def test_original_boundaries_preserve_source_and_reject_destroyed_positions(self):
        result=self.tokenizer.tokenize('ＡＢｶﾞ',forced_boundaries=[2])
        self.assertEqual(['ＡＢ','ｶﾞ'],[item.surface() for item in result])
        self.assertEqual([(0,2),(2,4)],[(item.begin(),item.end()) for item in result])
        with self.assertRaises(Exception):
            self.tokenizer.tokenize('ＡＢｶﾞ',forced_boundaries=[3])
        with self.assertRaises(Exception):
            self.tokenizer.tokenize('',forced_boundaries=[1])

    def test_repeated_surface_uses_supplied_position(self):
        required = [(1, 2, self.wid('甲', 'カン'))]
        paths = self.search(required, text='甲甲', reading='コウカン')
        self.assertTrue(paths)
        self.assertEqual(required[0][2], paths[0]['tokens'][1]['word_id'])

    def test_empty_constraints_preserve_search(self):
        self.assertEqual(self.search(), self.search([]))
        self.assertEqual([], self.search([(0, 1, self.wid('甲'))], max_results=0))

    def test_invalid_constraints_raise(self):
        wid = self.wid('甲')
        for required in [[(0, 0, wid)], [(0, 3, wid)], [(0, 1, wid), (0, 2, wid)],
                         [(0, 1, sum(self.dictionary.dictionary_sizes()))]]:
            with self.subTest(required=required), self.assertRaises(Exception):
                self.search(required)

    def test_best_effort_never_relaxes_required_tokens(self):
        split = [(0, 1, self.wid('甲', 'コウ'))]
        for policy in ['silent', 'warn', 'error', 'oov']:
            self.assertEqual([], self.search(split, reading='コウテイ', mismatch_policy=policy))
        # A required correct token is retained when only an unconstrained neighbour disagrees.
        paths = self.search([(0, 2, self.wid('ab'))], text='ＡＢｶﾞ', reading='エービーゴ', mismatch_policy='silent')
        self.assertTrue(paths[0]['tokens'][0]['reading_matches'])
        self.assertFalse(paths[0]['tokens'][1]['reading_matches'])
        self.assertEqual([], self.search([(2, 4, self.wid('ガ'))], text='ＡＢｶﾞ', reading='エービーゴ', mismatch_policy='oov'))

    def test_user_dictionary_cross_lex_identity(self):
        user_path = self.root / 'user.dic'
        user_csv = '甲乙,0,0,5000,甲乙,名詞,普通名詞,一般,*,*,*,コウオツ,甲乙,*,A,*,*,*\n'
        sudachipy.build_user_dic(self.system, [user_csv.encode()], user_path)
        dictionary = Dictionary(config=self.config.update(user=[str(user_path)]))
        self.extra.append(dictionary)
        wid = next(m.word_id() for m in dictionary.lookup('甲乙') if m.lex_id() > 0)
        paths = dictionary.create().tokenize_reading_candidates('甲乙', 'コウオツ',
            max_results=1, matched_tokens=[(0, 2, wid)])
        self.assertEqual(wid, paths[0]['tokens'][0]['word_id'])


if __name__ == '__main__':
    unittest.main()
