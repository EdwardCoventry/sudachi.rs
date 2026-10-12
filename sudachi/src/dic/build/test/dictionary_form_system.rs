use super::*;

#[test]
fn explicit_system_form_survives_multiple_user_lexicons_and_local_collision() {
    let mut builder = DictBuilder::new_system();
    builder.read_conn(MATRIX_10_10).unwrap();
    builder.read_lexicon("システム,1,1,1000,システム,形容詞,一般,*,*,形容詞,終止形-一般,システム,システム,*,A,*,*,*,*".as_bytes()).unwrap();
    let mut system_data = Vec::new();
    builder.compile(&mut system_data).unwrap();
    let system = DictionaryLoader::read_system_dictionary(&system_data)
        .unwrap()
        .to_loaded()
        .unwrap();
    let mut builder = DictBuilder::new_user(&system);
    builder.read_lexicon("ローカル,1,1,1000,ローカル,形容詞,一般,*,*,形容詞,終止形-一般,ローカル,ローカル,*,A,*,*,*,*\nシステム参照,1,1,1000,システム参照,形容詞,一般,*,*,形容詞,連用形-一般,システムサンショウ,システム,S0,A,*,*,*,*\nローカル参照,1,1,1000,ローカル参照,形容詞,一般,*,*,形容詞,連用形-一般,ローカルサンショウ,ローカル,0,A,*,*,*,*".as_bytes()).unwrap();
    let mut user_data = Vec::new();
    builder.compile(&mut user_data).unwrap();
    let first = DictionaryLoader::read_user_dictionary(&user_data).unwrap();
    let second = DictionaryLoader::read_user_dictionary(&user_data).unwrap();
    let merged = system
        .merge_dictionary(first)
        .unwrap()
        .merge_dictionary(second)
        .unwrap();
    for lex in [1, 2] {
        let explicit = merged
            .lexicon_set
            .get_word_info(WordId::new(lex, 1))
            .unwrap();
        assert_eq!(explicit.dictionary_form(), "システム");
        assert_eq!(
            WordId::from_raw(explicit.dictionary_form_word_id() as u32)
                .system_dictionary_form_reference(),
            Some(WordId::new(0, 0))
        );
        let local = merged
            .lexicon_set
            .get_word_info(WordId::new(lex, 2))
            .unwrap();
        assert_eq!(local.dictionary_form(), "ローカル");
        assert_eq!(local.dictionary_form_word_id(), 0);
        let surface_only = merged
            .lexicon_set
            .get_word_info_subset(WordId::new(lex, 1), InfoSubset::SURFACE)
            .unwrap();
        assert_eq!(surface_only.surface(), "システム参照");
    }
}

#[test]
fn explicit_system_form_bounds_are_checked_without_external_reference_exception() {
    let mut builder = DictBuilder::new_system();
    builder.read_conn(MATRIX_10_10).unwrap();
    builder
        .read_lexicon(include_bytes!("data_1word.csv"))
        .unwrap();
    let mut data = Vec::new();
    builder.compile(&mut data).unwrap();
    let system = DictionaryLoader::read_system_dictionary(&data)
        .unwrap()
        .to_loaded()
        .unwrap();
    for reference in ["S1", "S100000000"] {
        let mut builder = DictBuilder::new_user(&system);
        let row =
            format!("参照,1,1,1000,参照,名詞,一般,*,*,*,*,サンショウ,参照,{reference},A,*,*,*,*");
        builder.read_lexicon(row.as_bytes()).unwrap();
        assert!(builder.compile(&mut sink()).is_err());
    }
}
