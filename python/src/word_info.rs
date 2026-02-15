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

use sudachi::dic::lexicon::word_infos::{WordInfo, WordInfoData};
use sudachi::dic::word_id::WordId;

const LEGACY_LEX_STRIDE: i32 = 100_000_000;
const NATIVE_LEX_SHIFT: u32 = 28;
const NATIVE_WORD_MASK: u32 = 0x0fff_ffff;
const NATIVE_OOV_DIC_ID: i32 = 0xf;

#[pyclass(module = "sudachipy.wordinfo", name = "WordInfo", get_all)]
pub struct PyWordInfo {
    word_id: i32,
    word_id_packed: u32,
    word_id_relative: i32,
    lex_id: i32,
    dictionary_id: i32,
    surface: String,
    head_word_length: u16,
    pos_id: u16,
    normalized_form: String,
    dictionary_form_word_id: i32,
    dictionary_form_word_id_packed: i32,
    dictionary_form_word_id_relative: i32,
    dictionary_form_lex_id: i32,
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
        (-1, word_id)
    } else {
        (lex_id, word_id)
    }
}

fn decode_dictionary_form_word_id(
    raw_dictionary_form_word_id: i32,
    default_lex_id: i32,
) -> (i32, i32, i32, i32) {
    if raw_dictionary_form_word_id == -1 {
        return (-1, -1, -1, -1);
    }

    let raw = raw_dictionary_form_word_id as u32;
    let (native_lex_id, native_word_id) = unpack_native_word_id(raw);

    if raw >= (1 << NATIVE_LEX_SHIFT) && native_lex_id > 0 {
        let relative = native_word_id;
        let legacy = pack_legacy_word_id(native_lex_id, relative);
        let packed = raw_dictionary_form_word_id;
        (native_lex_id, legacy, packed, relative)
    } else if raw_dictionary_form_word_id >= LEGACY_LEX_STRIDE {
        let legacy_lex_id = raw_dictionary_form_word_id / LEGACY_LEX_STRIDE;
        let relative = raw_dictionary_form_word_id % LEGACY_LEX_STRIDE;
        if legacy_lex_id > 0 {
            return (
                legacy_lex_id,
                raw_dictionary_form_word_id,
                raw_dictionary_form_word_id,
                relative,
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
        )
    }
}

impl From<WordInfo> for PyWordInfo {
    fn from(word_info: WordInfo) -> Self {
        Self::from_word_info(word_info, WordId::INVALID)
    }
}

impl PyWordInfo {
    pub(crate) fn from_word_info(word_info: WordInfo, word_id: WordId) -> Self {
        let word_info: WordInfoData = word_info.into();
        let (lex_id, legacy_word_id, packed_word_id, relative_word_id) = if word_id.is_oov() {
            (-1, -1, word_id.as_raw(), -1)
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
        ) = decode_dictionary_form_word_id(word_info.dictionary_form_word_id, lex_id);

        Self {
            word_id: legacy_word_id,
            word_id_packed: packed_word_id,
            word_id_relative: relative_word_id,
            lex_id,
            dictionary_id: lex_id,
            head_word_length: word_info.head_word_length,
            pos_id: word_info.pos_id,
            normalized_form: copy_if_empty(word_info.normalized_form, &word_info.surface),
            dictionary_form_word_id,
            dictionary_form_word_id_packed,
            dictionary_form_word_id_relative,
            dictionary_form_lex_id,
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
