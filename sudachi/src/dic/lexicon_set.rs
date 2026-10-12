/*
 * Copyright (c) 2021-2024 Works Applications Co., Ltd.
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

use thiserror::Error;

use crate::dic::lexicon::word_infos::{WordInfo, WordInfoData};
use crate::dic::lexicon::{Lexicon, LexiconEntry, MAX_DICTIONARIES};
use crate::dic::subset::InfoSubset;
use crate::dic::word_id::WordId;
use crate::prelude::*;

/// Sudachi error
#[derive(Error, Debug, Eq, PartialEq)]
pub enum LexiconSetError {
    #[error("too large word_id {0} in dict {1}")]
    TooLargeWordId(u32, usize),

    #[error("too large dictionary_id {0}")]
    TooLargeDictionaryId(usize),

    #[error("too many user dictionaries")]
    TooManyDictionaries,
}

/// Set of Lexicons
///
/// Handles multiple lexicons as one lexicon
/// The first lexicon in the list must be from system dictionary
pub struct LexiconSet<'a> {
    lexicons: Vec<Lexicon<'a>>,
    pos_offsets: Vec<usize>,
    num_system_pos: usize,
}

impl<'a> LexiconSet<'a> {
    /// Creates a LexiconSet given a lexicon
    ///
    /// It is assumed that the passed lexicon is the system dictionary
    pub fn new(mut system_lexicon: Lexicon, num_system_pos: usize) -> LexiconSet {
        system_lexicon.set_dic_id(0);
        LexiconSet {
            lexicons: vec![system_lexicon],
            pos_offsets: vec![0],
            num_system_pos,
        }
    }

    /// Add a lexicon to the lexicon list
    ///
    /// pos_offset: number of pos in the grammar
    pub fn append(
        &mut self,
        mut lexicon: Lexicon<'a>,
        pos_offset: usize,
    ) -> Result<(), LexiconSetError> {
        if self.is_full() {
            return Err(LexiconSetError::TooManyDictionaries);
        }
        lexicon.set_dic_id(self.lexicons.len() as u8);
        self.lexicons.push(lexicon);
        self.pos_offsets.push(pos_offset);
        Ok(())
    }

    /// Returns if dictionary capacity is full
    pub fn is_full(&self) -> bool {
        self.lexicons.len() >= MAX_DICTIONARIES
    }
}

impl LexiconSet<'_> {
    pub fn num_lexicons(&self) -> usize {
        self.lexicons.len()
    }

    pub fn lexicon_size(&self, dict_id: usize) -> Option<u32> {
        self.lexicons.get(dict_id).map(|l| l.size())
    }

    /// Returns iterator which yields all words in the dictionary, starting from the `offset` bytes
    ///
    /// Searches dictionaries in the reverse order: user dictionaries first and then system dictionary
    #[inline]
    pub fn lookup<'b>(
        &'b self,
        input: &'b [u8],
        offset: usize,
    ) -> impl Iterator<Item = LexiconEntry> + 'b {
        // word_id fixup was moved to lexicon itself
        self.lexicons
            .iter()
            .rev()
            .flat_map(move |l| l.lookup(input, offset))
    }

    /// Returns WordInfo for given WordId
    pub fn get_word_info(&self, id: WordId) -> SudachiResult<WordInfo> {
        self.get_word_info_subset(id, InfoSubset::all())
    }

    /// Returns WordInfo for given WordId.
    /// Only fills a requested subset of fields.
    /// Rest will be of default values (0 or empty).
    pub fn get_word_info_subset(&self, id: WordId, subset: InfoSubset) -> SudachiResult<WordInfo> {
        let dict_id = id.dic();
        let owner = self.lexicons.get(dict_id as usize)
            .ok_or(LexiconSetError::TooLargeDictionaryId(dict_id as usize))?;
        let mut word_info: WordInfoData = owner
            .get_word_info(id.word(), subset)?
            .into();

        if subset.contains(InfoSubset::DIC_FORM_WORD_ID) {
            let reference = WordId::from_raw(word_info.dictionary_form_word_id as u32);
            if reference != WordId::INVALID {
                let target = if let Some(system) = reference.system_dictionary_form_reference() {
                    system
                } else if let Some(fixed) = reference.native_reference_target() {
                    fixed
                } else if reference.dic() <= 1 {
                    // Bare dictionary-form numbers and U<n> are local to the owner.
                    WordId::checked(dict_id, reference.word())?
                } else {
                    reference
                };
                self.check_reference(target)?;
                // Surface-only access prevents dictionary-form cycles from recursing.
                let surface = self.lexicons[target.dic() as usize]
                    .get_word_info(target.word(), InfoSubset::SURFACE)?;
                word_info.dictionary_form = surface.surface().to_owned();
                if reference.dic() == 1 && reference.native_reference_target().is_none() {
                    word_info.dictionary_form_word_id = target.as_raw() as i32;
                }
            }
        }

        if subset.contains(InfoSubset::POS_ID) {
            let pos_id = word_info.pos_id as usize;
            if dict_id > 0 && pos_id >= self.num_system_pos {
                // user defined part-of-speech
                word_info.pos_id =
                    (pos_id - self.num_system_pos + self.pos_offsets[dict_id as usize]) as u16;
            }
        }

        if subset.contains(InfoSubset::SPLIT_A) {
            Self::update_dict_id(&mut word_info.a_unit_split, dict_id)?;
            self.check_references(&word_info.a_unit_split)?;
        }

        if subset.contains(InfoSubset::SPLIT_B) {
            Self::update_dict_id(&mut word_info.b_unit_split, dict_id)?;
            self.check_references(&word_info.b_unit_split)?;
        }

        if subset.contains(InfoSubset::WORD_STRUCTURE) {
            Self::update_dict_id(&mut word_info.word_structure, dict_id)?;
            self.check_references(&word_info.word_structure)?;
        }

        Ok(word_info.into())
    }

    /// Returns word_param for given word_id
    pub fn get_word_param(&self, id: WordId) -> (i16, i16, i16) {
        let dic_id = id.dic() as usize;
        self.lexicons[dic_id].get_word_param(id.word())
    }

    fn update_dict_id(split: &mut Vec<WordId>, dict_id: u8) -> SudachiResult<()> {
        for id in split.iter_mut() {
            if let Some(target) = id.native_reference_target() {
                *id = target;
                continue;
            }
            let cur_dict_id = id.dic();
            if cur_dict_id == 1 {
                // User dictionaries store local references with dic_id=1.
                // Keep explicit cross-lex ids (dic_id >= 2) untouched.
                *id = WordId::checked(dict_id, id.word())?;
            }
        }
        Ok(())
    }

    fn check_reference(&self, id: WordId) -> SudachiResult<()> {
        let size = self.lexicon_size(id.dic() as usize)
            .ok_or(LexiconSetError::TooLargeDictionaryId(id.dic() as usize))?;
        if id.word() >= size {
            return Err(LexiconSetError::TooLargeWordId(id.word(), id.dic() as usize).into());
        }
        Ok(())
    }

    fn check_references(&self, references: &[WordId]) -> SudachiResult<()> {
        for id in references {
            self.check_reference(*id)?;
        }
        Ok(())
    }

    pub fn size(&self) -> u32 {
        self.lexicons.iter().fold(0, |acc, lex| acc + lex.size())
    }
}
