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

use std::ffi::CString;
use std::ops::DerefMut;
use std::str::FromStr;
use std::sync::Arc;

use pyo3::exceptions::{PyRuntimeWarning, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

use sudachi::analysis::stateful_tokenizer::StatefulTokenizer;
use sudachi::analysis::stateless_tokenizer::DictionaryAccess;
use sudachi::analysis::reading_candidates::MatchedToken;
use sudachi::dic::subset::InfoSubset;
use sudachi::prelude::*;

use crate::dictionary::{extract_mode, unpack_word_id, PyDicData};
use crate::errors;
use crate::morpheme::PyMorphemeListWrapper;
use crate::projection::PyProjector;
use crate::word_info::{LEX_ID_OOV, WORD_ID_OOV};

/// A native target or a set of attested origins of one virtual target.
#[derive(FromPyObject)]
enum RequiredIdentities {
    One(u32),
    Origins(Vec<u32>),
}

const CROSS_LEX_ID_STRIDE: i32 = 100_000_000;

fn strip_forced_split_whitespace(text: &str) -> (String, Vec<usize>) {
    let mut segments = Vec::new();
    let mut current = String::new();
    for ch in text.chars() {
        if ch.is_whitespace() {
            if !current.is_empty() {
                segments.push(std::mem::take(&mut current));
            }
        } else {
            current.push(ch);
        }
    }
    if !current.is_empty() {
        segments.push(current);
    }

    if segments.is_empty() {
        return (String::new(), Vec::new());
    }

    let mut joined = String::new();
    let mut boundaries = Vec::new();
    let mut offset = 0usize;
    for (idx, segment) in segments.iter().enumerate() {
        if idx > 0 {
            boundaries.push(offset);
        }
        joined.push_str(segment);
        offset += segment.chars().count();
    }

    (joined, boundaries)
}

/// Unit to split text.
///
/// A == short mode
///
/// B == middle mode
///
/// C == long mode
///
/// :param mode: string representation of the split mode. One of [A,B,C] in captital or lower case.
///     If None, returns SplitMode.C.
///
/// :type mode: str | None
#[pyclass(module = "sudachipy.tokenizer", name = "SplitMode", eq, eq_int, frozen)]
#[derive(Clone, PartialEq, Eq, Copy, Debug)]
#[repr(u8)]
pub enum PySplitMode {
    A,
    B,
    C,
}

impl From<PySplitMode> for Mode {
    fn from(mode: PySplitMode) -> Self {
        match mode {
            PySplitMode::A => Mode::A,
            PySplitMode::B => Mode::B,
            PySplitMode::C => Mode::C,
        }
    }
}

impl From<Mode> for PySplitMode {
    fn from(value: Mode) -> Self {
        match value {
            Mode::A => PySplitMode::A,
            Mode::B => PySplitMode::B,
            Mode::C => PySplitMode::C,
        }
    }
}

impl PyTokenizer {
    fn unpack_constraints(&self,matched_tokens:Option<Vec<(usize,usize,RequiredIdentities)>>) -> PyResult<Vec<MatchedToken>> {
        matched_tokens.unwrap_or_default().into_iter().map(|(begin,end,identities)| {
            let ids=match identities {RequiredIdentities::One(raw)=>vec![raw],RequiredIdentities::Origins(ids)=>ids};
            if ids.is_empty() {return Err(PyValueError::new_err("required identity group is empty"));}
            let mut word_ids=ids.into_iter().map(|raw| unpack_word_id(raw,self.tokenizer.dict().lexicon()))
                .collect::<PyResult<Vec<_>>>()?;
            word_ids.sort_unstable();word_ids.dedup();
            Ok(MatchedToken {begin,end,word_ids})
        }).collect()
    }
}

#[pymethods]
impl PySplitMode {
    /// Creates a split mode from a string value.
    ///
    /// :param mode: string representation of the split mode. One of [A,B,C] in captital or lower case.
    ///     If None, returns SplitMode.C.
    ///
    /// :type mode: str | None
    #[new]
    #[pyo3(
        signature = (mode=None),
        text_signature = "(mode=None) -> SplitMode"
    )]
    fn new(mode: Option<&str>) -> PyResult<PySplitMode> {
        let mode = match mode {
            Some(m) => m,
            None => return Ok(PySplitMode::C),
        };
        errors::wrap(Mode::from_str(mode).map(|m| m.into()))
    }
}

/// A sudachi tokenizer
///
/// Create using Dictionary.create method.
#[pyclass(module = "sudachipy.tokenizer", name = "Tokenizer")]
pub(crate) struct PyTokenizer {
    tokenizer: StatefulTokenizer<Arc<PyDicData>>,
    projection: PyProjector,
}

impl PyTokenizer {
    pub(crate) fn new(
        dict: Arc<PyDicData>,
        mode: Mode,
        fields: InfoSubset,
        projection: PyProjector,
    ) -> Self {
        let mut tok = Self {
            tokenizer: StatefulTokenizer::new(dict, mode),
            projection,
        };
        tok.tokenizer.set_subset(fields);
        tok
    }
}

#[pymethods]
impl PyTokenizer {
    #[classattr]
    #[allow(non_snake_case)]
    fn SplitMode() -> PySplitMode {
        PySplitMode::C
    }

    /// Break text into morphemes.
    ///
    /// :param text: text to analyze.
    /// :param mode: analysis mode.
    ///    This parameter is deprecated.
    ///    Pass the analysis mode at the Tokenizer creation time and create different tokenizers for different modes.
    ///    If you need multi-level splitting, prefer using :py:meth:`Morpheme.split` method instead.
    /// :param logger: Arg for v0.5.* compatibility. Ignored.
    /// :param out: tokenization results will be written into this MorphemeList, a new one will be created instead.
    ///    See https://worksapplications.github.io/sudachi.rs/python/topics/out_param.html for details.
    ///
    /// :type text: str
    /// :type mode: SplitMode | str | None
    /// :type out: MorphemeList
    #[pyo3(
        text_signature="(self, /, text: str, mode=None, logger=None, out=None, *, forced_boundaries=None) -> MorphemeList",
        signature=(text, mode=None, logger=None, out=None, *, forced_boundaries=None)
    )]
    #[allow(unused_variables)]
    fn tokenize<'py>(
        &'py mut self,
        py: Python<'py>,
        text: &'py str,
        mode: Option<&Bound<'py, PyAny>>,
        logger: Option<Py<PyAny>>,
        out: Option<Bound<'py, PyMorphemeListWrapper>>,
        forced_boundaries: Option<Vec<usize>>,
    ) -> PyResult<Bound<'py, PyMorphemeListWrapper>> {
        // restore default mode on scope exit
        let mode = match mode {
            None => None,
            Some(m) => Some(extract_mode(m)?),
        };
        let default_mode = mode.map(|m| self.tokenizer.set_mode(m));
        let mut tokenizer = scopeguard::guard(&mut self.tokenizer, |t| {
            default_mode.map(|m| t.set_mode(m));
        });

        // analysis can be done without GIL
        errors::wrap_analysis(
            py.detach(|| {
                tokenizer.reset().push_str(text);
                tokenizer.do_tokenize_with_forced_boundaries(forced_boundaries.as_deref().unwrap_or(&[]))
            }),
        )?;

        let out_list = match out {
            None => {
                let dict = tokenizer.dict_clone();
                let morphemes = MorphemeList::empty(dict);
                let wrapper =
                    PyMorphemeListWrapper::from_components(morphemes, self.projection.clone());
                Bound::new(py, wrapper)?
            }
            Some(list) => list,
        };

        let mut borrow = out_list.try_borrow_mut();
        let morphemes = match borrow {
            Ok(ref mut ms) => ms.internal_mut(py),
            Err(_) => return errors::wrap(Err("out was used twice at the same time")),
        };

        if !std::sync::Arc::ptr_eq(morphemes.dict(), tokenizer.dict()) {
            return Err(PyValueError::new_err("Tokenisation output belongs to a different dictionary/profile"));
        }
        errors::wrap_ctx(
            morphemes.collect_results(tokenizer.deref_mut()),
            "Error during tokenization",
        )?;

        Ok(out_list)
    }

    /// Break text into morphemes while forcing boundaries at whitespace positions.
    ///
    /// Whitespace is removed before analysis, but each whitespace-separated segment
    /// boundary is treated as a mandatory token boundary.
    #[pyo3(
        text_signature="(self, /, text: str, mode=None, logger=None, out=None) -> MorphemeList",
        signature=(text, mode=None, logger=None, out=None)
    )]
    #[allow(unused_variables)]
    fn tokenize_forced_splits<'py>(
        &'py mut self,
        py: Python<'py>,
        text: &'py str,
        mode: Option<&Bound<'py, PyAny>>,
        logger: Option<Py<PyAny>>,
        out: Option<Bound<'py, PyMorphemeListWrapper>>,
    ) -> PyResult<Bound<'py, PyMorphemeListWrapper>> {
        let (joined_text, forced_boundaries) = strip_forced_split_whitespace(text);

        // restore default mode on scope exit
        let mode = match mode {
            None => None,
            Some(m) => Some(extract_mode(m)?),
        };
        let default_mode = mode.map(|m| self.tokenizer.set_mode(m));
        let mut tokenizer = scopeguard::guard(&mut self.tokenizer, |t| {
            default_mode.map(|m| t.set_mode(m));
        });

        // analysis can be done without GIL
        errors::wrap_analysis(
            py.detach(|| {
                tokenizer.reset().push_str(&joined_text);
                tokenizer.do_tokenize_with_forced_boundaries(&forced_boundaries)
            }),
        )?;

        let out_list = match out {
            None => {
                let dict = tokenizer.dict_clone();
                let morphemes = MorphemeList::empty(dict);
                let wrapper =
                    PyMorphemeListWrapper::from_components(morphemes, self.projection.clone());
                Bound::new(py, wrapper)?
            }
            Some(list) => list,
        };

        let mut borrow = out_list.try_borrow_mut();
        let morphemes = match borrow {
            Ok(ref mut ms) => ms.internal_mut(py),
            Err(_) => return errors::wrap(Err("out was used twice at the same time")),
        };

        if !std::sync::Arc::ptr_eq(morphemes.dict(), tokenizer.dict()) {
            return Err(PyValueError::new_err("Tokenisation output belongs to a different dictionary/profile"));
        }
        errors::wrap_ctx(
            morphemes.collect_results(tokenizer.deref_mut()),
            "Error during tokenization",
        )?;

        Ok(out_list)
    }

    /// SplitMode of the tokenizer.
    #[getter]
    fn mode(&self) -> PySplitMode {
        self.tokenizer.mode().into()
    }

    /// Enable/disable global whitespace-bridge transitions in Viterbi.
    ///
    /// When enabled, transitions from whitespace to non-whitespace tokens can
    /// consider both normal and bridged connections and pick the lower cost.
    #[pyo3(
        signature = (enabled),
        text_signature = "(self, /, enabled: bool) -> bool",
    )]
    fn set_global_whitespace_bridge(&mut self, enabled: bool) -> bool {
        self.tokenizer.set_global_whitespace_bridge(enabled)
    }

    /// Enumerate tokenization candidates constrained by reading.
    ///
    /// Returns a list sorted by total path cost in ascending order.
    /// Each element is a dict with:
    /// - total_cost: int
    /// - tokens: list[dict] containing surface/reading and word-id fields.
    /// `mismatch_policy` controls behavior when no complete exact path exists:
    /// - reject: preserve exact-only behavior and return an empty list;
    /// - advisory: constrain native identity, using readings as visible hints;
    /// - silent: return the ordinary path with explicit mismatch metadata;
    /// - warn: return that path and emit RuntimeWarning;
    /// - error: raise ValueError instead of returning that path.
    /// - oov: return mismatched tokens as OOV using the supplied reading.
    #[pyo3(
        signature = (text, reading, max_results=64, min_tokens=1, mismatch_policy="reject", *, matched_tokens=None, forced_boundaries=None),
        text_signature = "(self, /, text: str, reading: str, max_results=64, min_tokens=1, mismatch_policy='reject', *, matched_tokens=None, forced_boundaries=None) -> list[dict]",
    )]
    fn tokenize_reading_candidates<'py>(
        &'py mut self,
        py: Python<'py>,
        text: &str,
        reading: &str,
        max_results: usize,
        min_tokens: usize,
        mismatch_policy: &str,
        matched_tokens: Option<Vec<(usize, usize, RequiredIdentities)>>,
        forced_boundaries: Option<Vec<usize>>,
    ) -> PyResult<Bound<'py, PyList>> {
        let matched_tokens=self.unpack_constraints(matched_tokens)?;
        errors::wrap_analysis(self.tokenizer.validate_profile_constraints(&matched_tokens))?;
        let min_tokens = min_tokens.max(1);
        let best_effort = match mismatch_policy {
            "reject" => false,
            "silent" | "warn" | "error" | "oov" | "advisory" => true,
            other => {
                return Err(PyValueError::new_err(format!(
                    "unknown mismatch_policy {other:?}; expected 'reject', 'silent', 'warn', 'error', 'oov', or 'advisory'"
                )))
            }
        };
        let candidates = errors::wrap_ctx(
            py.detach(|| {
                self.tokenizer.reset().push_str(text);
                self.tokenizer.do_tokenize_with_forced_boundaries(forced_boundaries.as_deref().unwrap_or(&[]))?;
                if mismatch_policy == "advisory" {
                    self.tokenizer.reading_candidates_advisory(reading, max_results, min_tokens, &matched_tokens)
                } else if best_effort {
                    self.tokenizer
                        .reading_candidates_best_effort_with_matched_tokens(reading, max_results, min_tokens, &matched_tokens)
                } else {
                    self.tokenizer.reading_candidates_with_matched_tokens(
                        reading,
                        max_results,
                        min_tokens,
                        &matched_tokens,
                    )
                }
            }),
            "Error during reading candidate tokenization",
        )?;

        if let Some(candidate) = candidates.first().filter(|candidate| !candidate.is_exact) {
            let mismatches = candidate
                .tokens
                .iter()
                .filter(|token| !token.reading_matches)
                .map(|token| {
                    format!(
                        "{}/{}/{}",
                        token.surface, token.reading_form, token.supplied_reading
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            let message = format!(
                "Sudachi used best-effort reading alignment with {} mismatch(es): {}",
                candidate.mismatch_count, mismatches
            );
            match mismatch_policy {
                "warn" => {
                    let warning = CString::new(message).map_err(|_| {
                        PyValueError::new_err("reading mismatch warning contained a null byte")
                    })?;
                    PyErr::warn(
                        py,
                        &py.get_type::<PyRuntimeWarning>(),
                        warning.as_c_str(),
                        1,
                    )?;
                }
                "error" => return Err(PyValueError::new_err(message)),
                _ => {}
            }
        }

        candidates_to_python(py,candidates,mismatch_policy)
    }

    /// Enumerate native paths without a reading hint; targets constrain search before caps.
    #[pyo3(signature=(text,max_results=64,min_tokens=1,*,matched_tokens=None,forced_boundaries=None))]
    fn tokenize_native_candidates<'py>(&'py mut self,py:Python<'py>,text:&str,max_results:usize,
        min_tokens:usize,matched_tokens:Option<Vec<(usize,usize,RequiredIdentities)>>,
        forced_boundaries:Option<Vec<usize>>) -> PyResult<Bound<'py,PyList>> {
        let constraints=self.unpack_constraints(matched_tokens)?;
        errors::wrap_analysis(self.tokenizer.validate_profile_constraints(&constraints))?;
        let candidates=errors::wrap_analysis(py.detach(|| {
            self.tokenizer.reset().push_str(text);
            self.tokenizer.do_tokenize_with_forced_boundaries(forced_boundaries.as_deref().unwrap_or(&[]))?;
            self.tokenizer.native_candidates(max_results,min_tokens,&constraints)
        }))?;
        candidates_to_python(py,candidates,"unhinted")
    }

    /// Parse annotated input, retain its surface/reading spans, and run the
    /// existing native reading search. Annotations never create dictionary entries.
    #[pyo3(signature = (annotated, max_results=64, min_tokens=1, mismatch_policy="advisory", *, matched_tokens=None, forced_boundaries=None))]
    fn tokenize_furigana_candidates<'py>(
        &'py mut self, py: Python<'py>, annotated: &str, max_results: usize,
        min_tokens: usize, mismatch_policy: &str,
        matched_tokens: Option<Vec<(usize, usize, RequiredIdentities)>>,
        forced_boundaries:Option<Vec<usize>>,
    ) -> PyResult<Bound<'py, PyDict>> {
        let input = sudachi::analysis::furigana::parse_furigana(annotated)
            .map_err(|error| PyValueError::new_err(error.to_string()))?;
        let out = crate::furigana::to_dict(py, &input)?;
        let candidates = self.tokenize_reading_candidates(py, &input.text, &input.reading,
            max_results, min_tokens, mismatch_policy, matched_tokens,forced_boundaries)?;
        out.set_item("candidates", candidates)?;
        Ok(out)
    }
}

fn candidates_to_python<'py>(py:Python<'py>,candidates:Vec<sudachi::analysis::reading_candidates::ReadingCandidatePath>,
    mismatch_policy:&str) -> PyResult<Bound<'py,PyList>> {
        let out = PyList::empty(py);
        for cand in candidates {
            let cand_obj = PyDict::new(py);
            cand_obj.set_item("total_cost", cand.total_cost)?;
            cand_obj.set_item("is_exact", cand.is_exact)?;
            cand_obj.set_item("mismatch_count", cand.mismatch_count)?;
            cand_obj.set_item("reading_policy", mismatch_policy)?;

            let tokens = PyList::empty(py);
            for token in cand.tokens {
                let tok = PyDict::new(py);
                let source_packed_word_id = token.word_id.as_raw();
                let source_lex_id = if token.word_id.is_oov() {
                    LEX_ID_OOV
                } else {
                    token.word_id.dic() as i32
                };
                let source_relative_word_id = if token.word_id.is_oov() {
                    WORD_ID_OOV
                } else {
                    token.word_id.word() as i32
                };
                let source_cross_lex_word_id = if source_lex_id <= 0 {
                    source_relative_word_id
                } else {
                    source_lex_id * CROSS_LEX_ID_STRIDE + source_relative_word_id
                };
                let coerced_to_oov = mismatch_policy == "oov" && !token.reading_matches;
                let is_oov = token.word_id.is_oov() || coerced_to_oov;
                let (lex_id, relative_word_id, cross_lex_word_id) = if is_oov {
                    (LEX_ID_OOV, WORD_ID_OOV, WORD_ID_OOV)
                } else {
                    (
                        source_lex_id,
                        source_relative_word_id,
                        source_cross_lex_word_id,
                    )
                };
                let packed_word_id = (!coerced_to_oov).then_some(source_packed_word_id);
                let output_reading = if coerced_to_oov {
                    token.supplied_reading.clone()
                } else {
                    token.reading_form.clone()
                };

                tok.set_item("surface", token.surface)?;
                tok.set_item("reading_form", output_reading)?;
                tok.set_item("dictionary_reading_form", token.reading_form)?;
                tok.set_item("supplied_reading", token.supplied_reading)?;
                tok.set_item("reading_matches", token.reading_matches)?;
                tok.set_item("coerced_to_oov", coerced_to_oov)?;
                tok.set_item("is_oov", is_oov)?;
                tok.set_item("begin", token.begin)?;
                tok.set_item("end", token.end)?;
                tok.set_item("word_id", cross_lex_word_id)?;
                tok.set_item("word_id_relative", relative_word_id)?;
                tok.set_item("word_id_packed", packed_word_id)?;
                tok.set_item("lex_id", lex_id)?;
                tok.set_item("source_word_id", source_cross_lex_word_id)?;
                tok.set_item("source_word_id_relative", source_relative_word_id)?;
                tok.set_item("source_word_id_packed", source_packed_word_id)?;
                tok.set_item("source_lex_id", source_lex_id)?;
                tokens.append(tok)?;
            }

            cand_obj.set_item("tokens", tokens)?;
            out.append(cand_obj)?;
        }

        Ok(out)
}
