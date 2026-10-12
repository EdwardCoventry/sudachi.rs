"""Fixed native references remain distinct from local rows across all slots."""
from sudachipy import Config, Dictionary, sudachipy


def entry(surface, citation="*", mode="A", split="*"):
    return f"{surface},0,0,1000,{surface},形容詞,一般,*,*,形容詞,終止形-一般,ア,{surface},{citation},{mode},{split},*,*\n"


def test_slot_fourteen_citation_and_splits_are_not_clipped_or_remapped(tmp_path):
    system = tmp_path / "system.dic"
    lex = entry("基底") + "名詞,0,0,1000,名詞,名詞,普通名詞,一般,*,*,*,メイシ,名詞,*,A,*,*,*\n"
    sudachipy.build_system_dic(b"1 1\n0 0 0\n", [lex.encode()], system)
    users = []
    for slot in range(1, 15):
        path = tmp_path / f"user-{slot}.dic"
        text = entry(f"所有{slot}")
        if slot == 14:
            text += entry("参照", "R8:0", "B", "R8:0/R1:0/U0/R0:0")
        sudachipy.build_user_dic(system=system, lex=[text.encode()], output=path)
        users.append(str(path))
    config = Config(system=str(system),user=users,pathRewritePlugin=[],connectionCostPlugin=[],
        inputTextPlugin=[],oovProviderPlugin=[{"class":"com.worksap.nlp.sudachi.SimpleOovPlugin",
            "oovPOS":["形容詞","一般","*","*","形容詞","終止形-一般"],
            "leftId":0,"rightId":0,"cost":10000}])
    dictionary = Dictionary(config=config)
    try:
        info = dictionary.word_info(14 * 100_000_000 + 1)
        assert info.dictionary_form == "所有8"
        assert info.dictionary_form_word_id == 8 * 100_000_000
        assert info.native_dictionary_form_word_id == 8 * 100_000_000
        assert info.dictionary_form_word_id_packed == 8 << 28
        assert info.a_unit_split == [8 * 100_000_000, 100_000_000, 14 * 100_000_000, 0]
        assert dictionary.word_info(14 * 100_000_000).dictionary_form_word_id_packed == 14 << 28
    finally:
        dictionary.close()


def test_physical_citation_is_available_for_a_non_inflected_spelling_variant(tmp_path):
    system = tmp_path / 'system.dic'
    text = ('基,0,0,1000,基,名詞,普通名詞,一般,*,*,*,キ,基,*,A,*,*,*\n'
            '別,0,0,1000,別,名詞,普通名詞,一般,*,*,*,ベツ,基,S0,A,*,*,*\n')
    sudachipy.build_system_dic(b'1 1\n0 0 0\n',[text.encode()],system)
    dictionary = Dictionary(config=Config(system=str(system),pathRewritePlugin=[],connectionCostPlugin=[],
        inputTextPlugin=[],oovProviderPlugin=[{'class':'com.worksap.nlp.sudachi.SimpleOovPlugin',
            'oovPOS':['名詞','普通名詞','一般','*','*','*'],'leftId':0,'rightId':0,'cost':10000}]))
    try:
        variant = dictionary.word_info(1)
        assert variant.dictionary_form_word_id == -1
        assert variant.native_dictionary_form_word_id == 0
        assert variant.dictionary_form == '基'
    finally:
        dictionary.close()
