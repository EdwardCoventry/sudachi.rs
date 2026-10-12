use super::*;

fn row(surface: &str, citation: &str, splits: &str) -> String {
    let mode = if splits == "*" { "A" } else { "B" };
    format!("{surface},1,1,1000,{surface},形容詞,一般,*,*,形容詞,連用形-一般,ア,{surface},{citation},{mode},{splits},*,*,*\n")
}

#[test]
fn fixed_native_owner_is_distinct_from_local_user_owner() {
    let mut builder = DictBuilder::new_system();
    builder.read_conn(MATRIX_10_10).unwrap();
    builder.read_lexicon(row("基底", "*", "*").as_bytes()).unwrap();
    let mut system_bytes = Vec::new();
    builder.compile(&mut system_bytes).unwrap();
    let system = DictionaryLoader::read_system_dictionary(&system_bytes).unwrap().to_loaded().unwrap();
    let mut user1 = DictBuilder::new_user(&system);
    user1.read_lexicon(row("共通", "*", "*").as_bytes()).unwrap();
    let mut bytes1 = Vec::new();
    user1.compile(&mut bytes1).unwrap();
    let mut user2 = DictBuilder::new_user(&system);
    let input = row("局所", "*", "*")
        + &row("他辞書", "R1:0", "R1:0/U0")
        + &row("自辞書", "U0", "*")
        + &row("システム", "R0:0", "*")
        + &row("欠落辞書", "R14:0", "*")
        + &row("欠落語", "R1:1", "*");
    user2.read_lexicon(input.as_bytes()).unwrap();
    let mut bytes2 = Vec::new();
    user2.compile(&mut bytes2).unwrap();
    let merged = system.merge_dictionary(DictionaryLoader::read_user_dictionary(&bytes1).unwrap()).unwrap()
        .merge_dictionary(DictionaryLoader::read_user_dictionary(&bytes2).unwrap()).unwrap();
    let foreign = merged.lexicon_set.get_word_info(WordId::new(2, 1)).unwrap();
    assert_eq!(foreign.dictionary_form(), "共通");
    assert_eq!(foreign.a_unit_split(), &[WordId::new(1, 0), WordId::new(2, 0)]);
    assert_eq!(WordId::from_raw(foreign.dictionary_form_word_id() as u32).native_reference_target(), Some(WordId::new(1, 0)));
    let local = merged.lexicon_set.get_word_info(WordId::new(2, 2)).unwrap();
    assert_eq!(local.dictionary_form(), "局所");
    assert_eq!(local.dictionary_form_word_id() as u32, WordId::new(2, 0).as_raw());
    assert_eq!(merged.lexicon_set.get_word_info(WordId::new(2, 3)).unwrap().dictionary_form(), "基底");
    assert!(merged.lexicon_set.get_word_info(WordId::new(2, 4)).is_err());
    assert!(merged.lexicon_set.get_word_info(WordId::new(2, 5)).is_err());
}

#[test]
fn reference_encoding_respects_native_and_kiku_limits() {
    for slot in 0..15 {
        let target = WordId::new(slot, 7);
        assert_eq!(WordId::native_reference(slot, 7).unwrap().native_reference_target(), Some(target));
    }
    assert!(WordId::native_reference(15, 0).is_err());
    assert!(WordId::native_reference(1, WordId::ROW_LIMIT).is_err());
    for invalid in ["R15:0", "R1:100000000", "R-1:0", "R1", "U100000000", "100000000"] {
        assert!(crate::dic::build::parse::parse_wordid(invalid).is_err(), "{invalid}");
    }
}
