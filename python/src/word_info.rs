/*
 *  Copyright (c) 2021 Works Applications Co., Ltd.
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

use pyo3::prelude::*;

use sudachi::dic::grammar::Grammar;
use sudachi::dic::lexicon::word_infos::{WordInfo, WordInfoData};
use sudachi::dic::word_id::WordId;

const LEGACY_LEX_STRIDE: i32 = 100_000_000;
const NATIVE_LEX_SHIFT: u32 = 28;
const NATIVE_WORD_MASK: u32 = 0x0fff_ffff;
const NATIVE_OOV_DIC_ID: i32 = 0xf;
pub(crate) const LEX_ID_MISSING: i32 = -1;
pub(crate) const LEX_ID_OOV: i32 = -2;
pub(crate) const WORD_ID_MISSING: i32 = -1;
pub(crate) const WORD_ID_OOV: i32 = -2;

#[pyclass(module = "sudachipy.wordinfo", name = "WordInfo", get_all)]
pub struct PyWordInfo {
    word_id: i32,
    word_id_packed: u32,
    word_id_relative: i32,
    lex_id: i32,
    surface: String,
    head_word_length: u16,
    pos_id: u16,
    normalized_form: String,
    dictionary_form_word_id: i32,
    dictionary_form_word_id_packed: i32,
    dictionary_form_word_id_relative: i32,
    dictionary_form_lex_id: i32,
    is_dictionary_form: bool,
    is_inflected: bool,
    dictionary_form: String,
    reading_form: String,
    a_unit_split: Vec<u32>,
    b_unit_split: Vec<u32>,
    word_structure: Vec<u32>,
    synonym_group_ids: Vec<u32>,
}

fn copy_if_empty(v1: String, v2: &String) -> String {
    if v1.is_empty() {
        v2.clone()
    } else {
        v1
    }
}

fn pack_legacy_word_id(lex_id: i32, relative_word_id: i32) -> i32 {
    if lex_id <= 0 {
        relative_word_id
    } else {
        lex_id * LEGACY_LEX_STRIDE + relative_word_id
    }
}

fn unpack_native_word_id(raw_word_id: u32) -> (i32, i32) {
    let lex_id = ((raw_word_id >> NATIVE_LEX_SHIFT) & 0xf) as i32;
    let word_id = (raw_word_id & NATIVE_WORD_MASK) as i32;
    if lex_id == NATIVE_OOV_DIC_ID {
        (LEX_ID_OOV, word_id)
    } else {
        (lex_id, word_id)
    }
}

fn clamp_u32_to_i32(value: u32) -> i32 {
    if value > i32::MAX as u32 {
        i32::MAX
    } else {
        value as i32
    }
}

fn decode_dictionary_form_word_id(
    raw_dictionary_form_word_id: i32,
    default_lex_id: i32,
    default_word_id: i32,
    default_word_id_packed: u32,
    default_word_id_relative: i32,
    is_non_inflected: bool,
) -> (i32, i32, i32, i32, bool) {
    // OOV tokens use dedicated OOV sentinels (-2), distinct from
    // "missing/non-lexicon dictionary form" sentinels (-1).
    if default_lex_id == LEX_ID_OOV {
        return (LEX_ID_OOV, WORD_ID_OOV, WORD_ID_OOV, WORD_ID_OOV, false);
    }

    // Non-inflected POS entries (conjugation type/form: "*", "*") do not expose
    // dictionary-form ids via WordInfo and are represented as -1.
    if is_non_inflected {
        return (
            LEX_ID_MISSING,
            WORD_ID_MISSING,
            WORD_ID_MISSING,
            WORD_ID_MISSING,
            true,
        );
    }

    if raw_dictionary_form_word_id == WORD_ID_MISSING {
        // In Sudachi dictionaries, -1 means "same as this entry".
        // Normalize it to the current token ids for inflected entries.
        if default_lex_id != LEX_ID_MISSING
            && default_word_id != WORD_ID_MISSING
            && default_word_id_relative != WORD_ID_MISSING
        {
            return (
                default_lex_id,
                default_word_id,
                clamp_u32_to_i32(default_word_id_packed),
                default_word_id_relative,
                false,
            );
        }
        return (
            LEX_ID_MISSING,
            WORD_ID_MISSING,
            WORD_ID_MISSING,
            WORD_ID_MISSING,
            false,
        );
    }

    let raw = raw_dictionary_form_word_id as u32;
    let (native_lex_id, native_word_id) = unpack_native_word_id(raw);

    if raw >= (1 << NATIVE_LEX_SHIFT) && native_lex_id > 0 {
        let relative = native_word_id;
        let legacy = pack_legacy_word_id(native_lex_id, relative);
        let packed = raw_dictionary_form_word_id;
        (native_lex_id, legacy, packed, relative, false)
    } else if raw_dictionary_form_word_id >= LEGACY_LEX_STRIDE {
        let legacy_lex_id = raw_dictionary_form_word_id / LEGACY_LEX_STRIDE;
        let relative = raw_dictionary_form_word_id % LEGACY_LEX_STRIDE;
        if legacy_lex_id > 0 {
            return (
                legacy_lex_id,
                raw_dictionary_form_word_id,
                raw_dictionary_form_word_id,
                relative,
                false,
            );
        }
        // fall through to default-local behavior for malformed values
        let relative = raw_dictionary_form_word_id;
        let legacy = pack_legacy_word_id(default_lex_id, relative);
        (
            default_lex_id,
            legacy,
            raw_dictionary_form_word_id,
            relative,
            false,
        )
    } else {
        // Non-packed dictionary-form ids are relative to the current lexicon.
        let relative = raw_dictionary_form_word_id;
        let legacy = pack_legacy_word_id(default_lex_id, relative);
        (
            default_lex_id,
            legacy,
            raw_dictionary_form_word_id,
            relative,
            false,
        )
    }
}

pub(crate) fn is_non_inflected_pos(grammar: &Grammar<'_>, pos_id: u16) -> bool {
    match grammar.pos_list.get(pos_id as usize) {
        Some(pos) => {
            pos.get(4).map(|s| s.as_str()) == Some("*")
                && pos.get(5).map(|s| s.as_str()) == Some("*")
        }
        None => false,
    }
}

impl PyWordInfo {
    pub(crate) fn from_word_info(
        word_info: WordInfo,
        word_id: WordId,
        is_non_inflected: bool,
    ) -> Self {
        let word_info: WordInfoData = word_info.into();
        let (lex_id, legacy_word_id, packed_word_id, relative_word_id) = if word_id.is_oov() {
            (LEX_ID_OOV, WORD_ID_OOV, word_id.as_raw(), WORD_ID_OOV)
        } else {
            let lex_id = word_id.dic() as i32;
            let relative_word_id = word_id.word() as i32;
            let legacy_word_id = pack_legacy_word_id(lex_id, relative_word_id);
            (lex_id, legacy_word_id, word_id.as_raw(), relative_word_id)
        };
        let (
            dictionary_form_lex_id,
            dictionary_form_word_id,
            dictionary_form_word_id_packed,
            dictionary_form_word_id_relative,
            dictionary_form_missing,
        ) = decode_dictionary_form_word_id(
            word_info.dictionary_form_word_id,
            lex_id,
            legacy_word_id,
            packed_word_id,
            relative_word_id,
            is_non_inflected,
        );
        let is_inflected = !is_non_inflected;
        let is_dictionary_form = dictionary_form_missing
            || (dictionary_form_word_id == legacy_word_id && dictionary_form_lex_id == lex_id);

        Self {
            word_id: legacy_word_id,
            word_id_packed: packed_word_id,
            word_id_relative: relative_word_id,
            lex_id,
            head_word_length: word_info.head_word_length,
            pos_id: word_info.pos_id,
            normalized_form: copy_if_empty(word_info.normalized_form, &word_info.surface),
            dictionary_form_word_id,
            dictionary_form_word_id_packed,
            dictionary_form_word_id_relative,
            dictionary_form_lex_id,
            is_dictionary_form,
            is_inflected,
            dictionary_form: copy_if_empty(word_info.dictionary_form, &word_info.surface),
            reading_form: copy_if_empty(word_info.reading_form, &word_info.surface),
            surface: word_info.surface,
            // WordId is repr(transparent) with a single u32 field so transmute is safe
            a_unit_split: unsafe { std::mem::transmute(word_info.a_unit_split) },
            b_unit_split: unsafe { std::mem::transmute(word_info.b_unit_split) },
            word_structure: unsafe { std::mem::transmute(word_info.word_structure) },
            synonym_group_ids: word_info.synonym_group_ids,
        }
    }
}

#[pymethods]
impl PyWordInfo {
    fn length(&self) -> u16 {
        self.head_word_length
    }
}
