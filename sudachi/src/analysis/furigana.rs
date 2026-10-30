//! Strict bracketed-kanji readings, retaining original Unicode character spans.
use crate::error::{SudachiError, SudachiResult};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FuriganaSpan {
    pub surface: String,
    pub reading: String,
    pub start: usize,
    pub end: usize,
    pub reading_start: usize,
    pub reading_end: usize,
    pub annotated: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FuriganaInput {
    pub text: String,
    pub reading: String,
    pub spans: Vec<FuriganaSpan>,
}

fn han(ch: char) -> bool {
    matches!(ch as u32, 0x3400..=0x4dbf | 0x4e00..=0x9fff | 0xf900..=0xfaff | 0x20000..=0x323af)
        || matches!(ch, '々' | '〆' | '〇')
}

/// ヶ/ヵ between kanji are counter signs inside the kanji run (三ヶ月, 一ヵ所);
/// elsewhere they may be ordinary small katakana.
fn counter_sign(ch: char) -> bool {
    matches!(ch, 'ヶ' | 'ヵ')
}

fn in_kanji_run(chars: &[char], index: usize) -> bool {
    han(chars[index]) || (counter_sign(chars[index]) && index > 0 && han(chars[index - 1])
        && chars.get(index + 1).is_some_and(|next| han(*next)))
}

/// Letters, digits and standalone ヶ/ヵ may carry a kana reading (ＮＯ[の],
/// 3[さん], ヶ[が]) but read as themselves when left unannotated.
fn optional_reading(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || counter_sign(ch)
        || matches!(ch as u32, 0xff10..=0xff19 | 0xff21..=0xff3a | 0xff41..=0xff5a)
}

fn kana(ch: char) -> bool {
    matches!(ch as u32, 0x3041..=0x3096 | 0x30a1..=0x30fa | 0x30fc)
}

fn invalid(message: &str) -> SudachiError {
    SudachiError::InvalidFurigana(message.to_owned())
}

/// Kana outside brackets remains in both projections. Annotations apply only to
/// the immediately preceding complete kanji run (required) or letter, digit or
/// standalone ヶ/ヵ run (optional). Spans are input evidence, not
/// token boundaries or certification that a dictionary supports the reading.
pub fn parse_furigana(input: &str) -> SudachiResult<FuriganaInput> {
    if input.is_empty() {
        return Err(invalid("annotated sentence is empty"));
    }
    let chars: Vec<char> = input.chars().collect();
    let mut result = FuriganaInput { text: String::new(), reading: String::new(), spans: Vec::new() };
    let (mut index, mut text_offset, mut reading_offset) = (0, 0, 0);
    while index < chars.len() {
        let begin = index;
        let required = han(chars[index]);
        let optional = !required && optional_reading(chars[index]);
        if required {
            while index < chars.len() && in_kanji_run(&chars, index) { index += 1; }
        } else if optional {
            while index < chars.len() && optional_reading(chars[index]) { index += 1; }
        }
        let annotated = required || (optional && chars.get(index) == Some(&'['));
        let (surface, reading): (String, String);
        if annotated {
            surface = chars[begin..index].iter().collect();
            if chars.get(index) != Some(&'[') {
                return Err(invalid("Every kanji run must have an immediate kana annotation in square brackets"));
            }
            index += 1;
            let reading_begin = index;
            while index < chars.len() && kana(chars[index]) { index += 1; }
            if index == reading_begin || chars.get(index) != Some(&']') {
                return Err(invalid("annotation must contain nonempty kana and a closing square bracket"));
            }
            reading = chars[reading_begin..index].iter().collect();
            index += 1;
        } else {
            if !optional {
                while index < chars.len() && !han(chars[index]) && !optional_reading(chars[index]) {
                    if matches!(chars[index], '[' | ']' | '\r' | '\n') {
                        return Err(invalid("unattached brackets and line breaks are invalid"));
                    }
                    index += 1;
                }
            }
            surface = chars[begin..index].iter().collect();
            reading = surface.clone();
        }
        let (start, reading_start) = (text_offset, reading_offset);
        let end = text_offset + surface.chars().count();
        let reading_end = reading_offset + reading.chars().count();
        result.text.push_str(&surface);
        result.reading.push_str(&reading);
        text_offset = end;
        reading_offset = reading_end;
        // Unannotated letters/digits and their neighbours stay one plain span.
        if let Some(last) = result.spans.last_mut().filter(|last| !last.annotated && !annotated) {
            last.surface.push_str(&surface);
            last.reading.push_str(&reading);
            last.end = end;
            last.reading_end = reading_end;
            continue;
        }
        result.spans.push(FuriganaSpan { surface, reading, start, end, reading_start, reading_end, annotated });
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mixed_text_preserves_local_allocations() {
        let parsed = parse_furigana("その場[ば]に座[すわ]る。").unwrap();
        assert_eq!(parsed.text, "その場に座る。");
        assert_eq!(parsed.reading, "そのばにすわる。");
        assert_eq!(parsed.spans[1], FuriganaSpan { surface: "場".into(), reading: "ば".into(),
            start: 2, end: 3, reading_start: 2, reading_end: 3, annotated: true });
        assert_eq!(parsed.spans[3].reading_start, 4);
        assert_eq!(parsed.spans[3].reading_end, 6);
    }

    #[test]
    fn rejects_incomplete_or_unattached_annotations() {
        for input in ["", "場", "場[]", "場[ba]", "場[ば", "場[ば]]", "ば[ば]", "場[ば]\n", "場[ば]\r", "東[ひがし]京"] {
            assert!(parse_furigana(input).is_err(), "{input:?}");
        }
    }

    #[test]
    fn supplementary_unicode_and_plain_symbols_keep_character_offsets() {
        let parsed = parse_furigana("🙂𠮷野々[よしのの]Ａｶﾞ。").unwrap();
        assert_eq!(parsed.text, "🙂𠮷野々Ａｶﾞ。");
        assert_eq!(parsed.spans[1].start, 1);
        assert_eq!(parsed.spans[1].end, 4);
        assert_eq!(parsed.reading, "🙂よしののＡｶﾞ。");
    }

    #[test]
    fn whole_kanji_run_has_one_annotation() {
        let parsed = parse_furigana("山々[やまやま]。〇[まる]").unwrap();
        assert_eq!(parsed.spans[0].surface, "山々");
        assert_eq!(parsed.spans[0].end, 2);
        assert_eq!(parsed.spans[0].reading_end, 4);
    }

    #[test]
    fn letters_digits_and_counter_signs_take_readings() {
        let parsed = parse_furigana("ＮＯ[の]だ。").unwrap();
        assert_eq!((parsed.text.as_str(), parsed.reading.as_str()), ("ＮＯだ。", "のだ。"));
        assert!(parsed.spans[0].annotated);
        let parsed = parse_furigana("三ヶ月[さんかげつ]、関ヶ原[せきがはら]、ヶ[が]、3[さん]人[にん]").unwrap();
        assert_eq!(parsed.text, "三ヶ月、関ヶ原、ヶ、3人");
        assert_eq!(parsed.reading, "さんかげつ、せきがはら、が、さんにん");
        // Outside a kanji run, ヶ/ヵ may simply be small katakana.
        assert_eq!(parse_furigana("ヵ行[ぎょう]のヶ").unwrap().reading, "ヵぎょうのヶ");
        assert_eq!(parse_furigana("ヶ[ヶ]").unwrap().reading, "ヶ");
        for input in ["三ヶ月", "三[さん]ヶ月", "ＮＯ[no]", "ＮＯ[]"] {
            assert!(parse_furigana(input).is_err(), "{input:?}");
        }
    }

    #[test]
    fn unannotated_letters_merge_into_plain_spans() {
        let parsed = parse_furigana("はＡＢＣだ").unwrap();
        assert_eq!(parsed.spans.len(), 1);
        assert_eq!(parsed.reading, "はＡＢＣだ");
    }

    #[test]
    fn parser_does_not_silently_correct_readings() {
        let parsed = parse_furigana("その場[そのば]").unwrap();
        assert_eq!(parsed.reading, "そのそのば");
        assert_eq!(parse_furigana("策[はかりごと]").unwrap().reading, "はかりごと");
    }
}
