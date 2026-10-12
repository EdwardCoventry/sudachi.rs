/*
 *  Copyright (c) 2021-2024 Works Applications Co., Ltd.
 *
 *  Licensed under the Apache License, Version 2.0 (the "License");
 *  you may not use this file except in compliance with the License.
 *  You may obtain a copy of the License at
 *
 *      http://www.apache.org/licenses/LICENSE-2.0
 *
 *   Unless required by applicable law or agreed to in writing, software
 *  distributed under the License is distributed on an "AS IS" BASIS,
 *  WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 *  See the License for the specific language governing permissions and
 *  limitations under the License.
 */

use std::borrow::Cow;
use std::str::FromStr;

use lazy_static::lazy_static;
use regex::Regex;

use crate::analysis::Mode;
use crate::dic::build::error::{BuildFailure, DicWriteResult};
use crate::dic::build::{MAX_ARRAY_LEN, MAX_DIC_STRING_LEN};
use crate::dic::word_id::WordId;

#[cfg(test)]
const NATIVE_LEX_SHIFT: u32 = 28;

#[inline(always)]
pub fn it_next<'a, I, T, F>(
    orig: &'a str,
    data: &mut I,
    field: &'static str,
    f: F,
) -> DicWriteResult<T>
where
    I: Iterator<Item = &'a str>,
    F: FnOnce(&'a str) -> DicWriteResult<T>,
    T: 'a,
{
    match data.next() {
        Some(s) => f(s),
        None => Err(BuildFailure::SplitFormatError {
            original: orig.to_owned(),
            field,
        }),
    }
}

pub(crate) fn none_if_equal(surface: &str, data: Cow<str>) -> Option<String> {
    if surface == data {
        None
    } else {
        match data {
            Cow::Borrowed(x) => Some(x.to_owned()),
            Cow::Owned(x) => Some(x),
        }
    }
}

#[inline]
pub(crate) fn parse_mode(data: &str) -> DicWriteResult<Mode> {
    match data.trim() {
        "a" | "A" => Ok(Mode::A),
        "b" | "B" => Ok(Mode::B),
        "c" | "C" | "*" => Ok(Mode::C),
        "BC" => Ok(Mode::B),
        _ => Err(BuildFailure::InvalidSplit(data.to_owned())),
    }
}

#[inline]
pub(crate) fn parse_i16(data: &str) -> DicWriteResult<i16> {
    match i16::from_str(data) {
        Ok(v) => Ok(v),
        Err(_) => Err(BuildFailure::InvalidI16Literal(data.to_owned())),
    }
}

#[inline]
pub(crate) fn parse_u32(data: &str) -> DicWriteResult<u32> {
    match u32::from_str(data) {
        Ok(v) => Ok(v),
        Err(_) => Err(BuildFailure::InvalidU32Literal(data.to_owned())),
    }
}

#[inline]
pub(crate) fn parse_dic_form(data: &str) -> DicWriteResult<WordId> {
    if let Some(system) = data.strip_prefix('S') {
        WordId::system_dictionary_form(parse_u32(system)?)
            .map_err(|_| BuildFailure::InvalidWordId(data.to_owned()))
    } else if data == "*" || data == "-1" {
        Ok(WordId::INVALID)
    } else {
        parse_wordid(data)
    }
}

#[inline]
pub(crate) fn parse_wordid(data: &str) -> DicWriteResult<WordId> {
    if let Some(stripped) = data.strip_prefix('R') {
        let (dic, row) = stripped.split_once(':')
            .ok_or_else(|| BuildFailure::InvalidWordId(data.to_owned()))?;
        let dic = u8::from_str(dic).map_err(|_| BuildFailure::InvalidWordId(data.to_owned()))?;
        WordId::native_reference(dic, parse_u32(row)?)
            .map_err(|_| BuildFailure::InvalidWordId(data.to_owned()))
    } else if let Some(stripped) = data.strip_prefix('U') {
        let wid = parse_wordid_raw(stripped);
        wid.map(|w| WordId::new(1, w.word()))
    } else {
        // Bare CSV numbers are local system rows. Cross-lex references use
        // R<slot>:<row>; packed native numbers can overlap Kiku stride IDs.
        parse_wordid_raw(data)
    }
}

#[inline]
fn parse_wordid_raw(data: &str) -> DicWriteResult<WordId> {
    match u32::from_str(data) {
        Ok(v) if v < WordId::ROW_LIMIT => match WordId::checked(0, v) {
            Ok(id) => Ok(id),
            Err(_) => Err(BuildFailure::InvalidWordId(data.to_owned())),
        },
        _ => Err(BuildFailure::InvalidWordId(data.to_owned())),
    }
}

#[inline]
pub(crate) fn parse_wordid_list(data: &str) -> DicWriteResult<Vec<WordId>> {
    if data.is_empty() || data == "*" {
        return Ok(Vec::new());
    }

    parse_slash_list(data, parse_wordid)
}

#[inline]
pub(crate) fn parse_u32_list(data: &str) -> DicWriteResult<Vec<u32>> {
    if data.is_empty() || data == "*" {
        return Ok(Vec::new());
    }

    parse_slash_list(data, parse_u32)
}

lazy_static! {
    pub(crate) static ref WORD_ID_LITERAL: Regex = Regex::new(r"^(?:U?\d+|R\d+:\d+)$").unwrap();
}

#[inline]
pub(crate) fn parse_slash_list<T, F>(data: &str, mut f: F) -> DicWriteResult<Vec<T>>
where
    F: FnMut(&str) -> DicWriteResult<T>,
{
    let mut result = Vec::with_capacity(4);

    for part in data.split('/') {
        result.push(f(part)?);
    }

    if result.len() > MAX_ARRAY_LEN {
        return Err(BuildFailure::InvalidSize {
            expected: MAX_ARRAY_LEN,
            actual: result.len(),
        });
    }

    Ok(result)
}

lazy_static! {
    static ref UNICODE_LITERAL: Regex =
        Regex::new(r"\\u(?:\{([0-9a-fA-F]{1,6})\}|([0-9a-fA-F]{4}))").unwrap();
}

fn check_str_len(data: &str) -> DicWriteResult<()> {
    if data.len() > MAX_DIC_STRING_LEN {
        Err(BuildFailure::InvalidSize {
            expected: MAX_DIC_STRING_LEN,
            actual: data.len(),
        })
    } else {
        Ok(())
    }
}

#[inline]
pub(crate) fn unescape_cow(data: &str) -> DicWriteResult<Cow<str>> {
    check_str_len(data)?;
    if !UNICODE_LITERAL.is_match(data) {
        Ok(Cow::Borrowed(data))
    } else {
        unescape_slow(data).map(Cow::Owned)
    }
}

#[inline]
pub(crate) fn unescape(data: &str) -> DicWriteResult<String> {
    check_str_len(data)?;
    if !UNICODE_LITERAL.is_match(data) {
        Ok(data.to_owned())
    } else {
        unescape_slow(data)
    }
}

#[inline(never)]
fn unescape_slow(original: &str) -> DicWriteResult<String> {
    let mut result = String::with_capacity(original.len());
    let mut start = 0;
    for c in UNICODE_LITERAL.captures_iter(original) {
        let whole = c.get(0).unwrap();
        let braces = c.get(1).or_else(|| c.get(2)).unwrap();
        result.push_str(&original[start..whole.start()]);
        match u32::from_str_radix(braces.as_str(), 16) {
            Ok(c) => match char::from_u32(c) {
                Some(cx) => result.push(cx),
                None => return Err(BuildFailure::InvalidCharLiteral(braces.as_str().to_owned())),
            },
            Err(_) => return Err(BuildFailure::InvalidCharLiteral(braces.as_str().to_owned())),
        }
        start = whole.end();
    }
    result.push_str(&original[start..]);
    Ok(result)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn decode_plain() {
        assert_eq!(unescape("").unwrap(), "");
        assert_eq!(unescape("a").unwrap(), "a");
        assert_eq!(unescape("あ").unwrap(), "あ");
    }

    #[test]
    fn decode_escape_1() {
        assert_eq!(unescape("\\u0020").unwrap(), "\u{20}");
        assert_eq!(unescape("\\u{20}").unwrap(), "\u{20}");
        assert_eq!(unescape("\\u{1f49e}").unwrap(), "💞");
    }

    #[test]
    fn decode_escape_2() {
        assert_eq!(unescape("\\u020f").unwrap(), "\u{20f}");
        assert_eq!(unescape("\\u{20}f").unwrap(), "\u{20}f");
    }

    #[test]
    fn decode_escape_3() {
        assert_eq!(unescape("f\\u0020").unwrap(), "f\u{20}");
        assert_eq!(unescape("f\\u{20}").unwrap(), "f\u{20}");
    }

    #[test]
    fn decode_escape_4() {
        assert_eq!(unescape("\\u100056").unwrap(), "\u{1000}56");
    }

    #[test]
    fn decode_escape_ported() {
        assert_eq!(unescape("a\\u002cc").unwrap(), "a,c");
        assert_eq!(unescape("a\\u{2c}c").unwrap(), "a,c");
    }

    #[test]
    fn decode_escape_fail() {
        assert_eq!(unescape("\\u{10FFFF}").unwrap(), "\u{10FFFF}");
        // max character
        claim::assert_matches!(unescape("\\u{110000}"), Err(_));
        claim::assert_matches!(unescape("\\u{FFFFFF}"), Err(_));
    }

    #[test]
    fn parse_dic_form_system_reference() {
        assert_eq!(parse_dic_form("S42").unwrap(), WordId::new(15, 42));
        assert_eq!(
            parse_dic_form("S0")
                .unwrap()
                .system_dictionary_form_reference(),
            Some(WordId::new(0, 0))
        );
        assert!(parse_dic_form("S268435453").is_err());
        assert!(parse_dic_form("S268435456").is_err());
        assert!(parse_dic_form("S-1").is_err());
        assert!(parse_wordid("S0").is_err());
    }

    #[test]
    fn parse_dic_form_minus_one() {
        assert_eq!(parse_dic_form("-1").unwrap(), WordId::INVALID);
    }

    #[test]
    fn parse_wordid_rejects_ambiguous_native_packed_csv_number() {
        let raw = (2 << NATIVE_LEX_SHIFT) + 123;
        assert!(parse_wordid(&raw.to_string()).is_err());
    }
}
