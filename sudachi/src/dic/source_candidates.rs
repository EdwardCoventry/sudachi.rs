//! Source-membership lookup, including native records absent from a stock trie.
use crate::{dic::{model_matrix::ModelMatrix, word_id::WordId}, error::SudachiResult};
use memmap2::Mmap;
use sha2::{Digest, Sha256};
use std::{fs::File, io::{Error, ErrorKind}, path::Path};

const HEADER: usize = 56;
const RECORD: usize = 20;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceCandidate {
    pub word: WordId,
    pub left: u16,
    pub right: u16,
    pub cost: i32,
    pub end: usize,
}

pub struct SourceCandidates {
    bytes: Mmap,
    count: usize,
    surfaces: usize,
    max_length: usize,
    // High bits identify the word; low bits locate its attested scoring state.
    words: Vec<u64>,
}

fn invalid(message: &str) -> Error { Error::new(ErrorKind::InvalidData, message) }
fn u32_at(data: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(data[offset..offset+4].try_into().unwrap())
}

impl SourceCandidates {
    pub fn load(path: &Path, expected_sha256: &str, matrix: &ModelMatrix, sizes: &[usize], model_fingerprint: &str) -> SudachiResult<Self> {
        let file = File::open(path)?;
        if file.metadata()?.len() < HEADER as u64 {
            return Err(invalid("truncated selected-source candidates").into());
        }
        let bytes = unsafe { Mmap::map(&file)? };
        if format!("{:x}", Sha256::digest(&bytes)) != expected_sha256 {
            return Err(invalid("selected-source candidate fingerprint mismatch").into());
        }
        if &bytes[..8] != b"KIKUCN01" || u32_at(&bytes, 8) != 1 {
            return Err(invalid("unsupported selected-source candidate format").into());
        }
        if &bytes[24..56] != Sha256::digest(model_fingerprint.as_bytes()).as_slice() {
            return Err(invalid("source candidates belong to a different selected model").into());
        }
        let count = u32_at(&bytes, 12) as usize;
        let length = u32_at(&bytes, 16) as usize;
        let max_length = u32_at(&bytes, 20) as usize;
        let surfaces = count.checked_mul(RECORD).and_then(|n| n.checked_add(HEADER))
            .ok_or_else(|| invalid("selected-source candidate dimensions overflow"))?;
        if surfaces.checked_add(length) != Some(bytes.len()) {
            return Err(invalid("invalid selected-source candidate length").into());
        }
        let mut bank = Self { bytes, count, surfaces, max_length, words:Vec::new() };
        let mut words=Vec::with_capacity(count);
        let mut previous: Option<(&[u8], u32, u16, u16, i32)> = None;
        let mut observed_max = 0;
        for ordinal in 0..count {
            let record = bank.record(ordinal);
            let word = u32_at(record, 0);
            let id = WordId::from_raw(word);
            words.push((word as u64) << 32 | ordinal as u64);
            if id.dic() >= 15 || id.word() >= 100_000_000 ||
                sizes.get(id.dic() as usize).map_or(true, |size| id.word() as usize >= *size) {
                return Err(invalid("source candidate identity outside fixed native inventory").into());
            }
            let left = u16::from_le_bytes(record[4..6].try_into().unwrap());
            let right = u16::from_le_bytes(record[6..8].try_into().unwrap());
            matrix.cost(right, left)?;
            let cost = i32::from_le_bytes(record[8..12].try_into().unwrap());
            let offset = u32_at(record, 12) as usize;
            let length = u32_at(record, 16) as usize;
            let start = bank.surfaces.checked_add(offset).ok_or_else(|| invalid("candidate surface overflow"))?;
            let end = start.checked_add(length).ok_or_else(|| invalid("candidate surface overflow"))?;
            let surface = bank.bytes.get(start..end).ok_or_else(|| invalid("candidate surface outside bank"))?;
            if surface.is_empty() || std::str::from_utf8(surface).is_err() {
                return Err(invalid("candidate surface is empty or invalid UTF-8").into());
            }
            let ordered = (surface, word, left, right, cost);
            if previous.is_some_and(|old| old >= ordered) {
                return Err(invalid("source candidates are unsorted or duplicate scoring states").into());
            }
            observed_max = observed_max.max(length);
            previous = Some(ordered);
        }
        if observed_max != max_length {
            return Err(invalid("selected-source maximum surface length disagrees").into());
        }
        words.sort_unstable();
        words.dedup();
        bank.words=words;
        Ok(bank)
    }

    fn record(&self, ordinal: usize) -> &[u8] {
        let start = HEADER + ordinal * RECORD;
        &self.bytes[start..start+RECORD]
    }

    pub fn contains_word(&self, word: WordId) -> bool {
        let key = (word.as_raw() as u64) << 32;
        let start = self.words.partition_point(|value| *value < key);
        self.words.get(start).is_some_and(|value| value >> 32 == word.as_raw() as u64)
    }

    pub fn parameters(&self, word: WordId) -> Vec<SourceCandidate> {
        let key = (word.as_raw() as u64) << 32;
        let start = self.words.partition_point(|value| *value < key);
        let mut variants = Vec::new();
        for value in &self.words[start..] {
            if value >> 32 != word.as_raw() as u64 { break; }
            let record = self.record(*value as u32 as usize);
            variants.push(SourceCandidate { word,
                left: u16::from_le_bytes(record[4..6].try_into().unwrap()),
                right: u16::from_le_bytes(record[6..8].try_into().unwrap()),
                cost: i32::from_le_bytes(record[8..12].try_into().unwrap()), end:0 });
        }
        variants.sort_unstable_by_key(|state| (state.left,state.right,state.cost));
        variants.dedup();
        variants
    }

    fn surface(&self, ordinal: usize) -> &[u8] {
        let record = self.record(ordinal);
        let start = self.surfaces + u32_at(record, 12) as usize;
        &self.bytes[start..start+u32_at(record, 16) as usize]
    }

    pub fn lookup(&self, input: &str, offset: usize) -> SudachiResult<Vec<SourceCandidate>> {
        let rest = input.get(offset..).ok_or_else(|| invalid("candidate lookup offset is not a UTF-8 boundary"))?;
        let mut found = Vec::new();
        for (start, ch) in rest.char_indices() {
            let end = start + ch.len_utf8();
            if end > self.max_length { break; }
            let spelling = &rest.as_bytes()[..end];
            let mut low = 0;
            let mut high = self.count;
            while low < high {
                let middle = low + (high-low)/2;
                if self.surface(middle) < spelling { low=middle+1; } else { high=middle; }
            }
            while low < self.count && self.surface(low) == spelling {
                let record = self.record(low);
                found.push(SourceCandidate {
                    word: WordId::from_raw(u32_at(record, 0)),
                    left: u16::from_le_bytes(record[4..6].try_into().unwrap()),
                    right: u16::from_le_bytes(record[6..8].try_into().unwrap()),
                    cost: i32::from_le_bytes(record[8..12].try_into().unwrap()),
                    end: offset+end,
                });
                low += 1;
            }
        }
        Ok(found)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::io::Write;

    fn matrix() -> (tempfile::NamedTempFile, std::sync::Arc<ModelMatrix>) {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        let mut bytes = Vec::new();
        bytes.extend(2u16.to_le_bytes()); bytes.extend(3u16.to_le_bytes());
        for value in [1i16,2,3,4,5,6] { bytes.extend(value.to_le_bytes()); }
        file.write_all(&bytes).unwrap();
        let hash = format!("{:x}", Sha256::digest(bytes));
        let model = ModelMatrix::load(file.path(), &hash).unwrap();
        (file, model)
    }

    // Synthetic identities occupy fixed slots; no stock Sudachi IDs are embedded.
    pub(crate) fn bank(rows: &[(&str, u8, u32, u16, u16, i32)]) -> (tempfile::NamedTempFile, String) {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        let mut records = Vec::new();
        let mut surfaces: Vec<u8> = Vec::new();
        let mut max = 0usize;
        for (surface, slot, row, left, right, cost) in rows {
            let word = WordId::checked(*slot, *row).unwrap();
            records.extend(word.as_raw().to_le_bytes());
            records.extend(left.to_le_bytes()); records.extend(right.to_le_bytes());
            records.extend(cost.to_le_bytes());
            records.extend((surfaces.len() as u32).to_le_bytes());
            records.extend((surface.len() as u32).to_le_bytes());
            surfaces.extend(surface.as_bytes());
            max = max.max(surface.len());
        }
        let mut bytes = Vec::from(*b"KIKUCN01");
        for value in [1, rows.len() as u32, surfaces.len() as u32, max as u32] { bytes.extend(value.to_le_bytes()); }
        bytes.extend(Sha256::digest(b"model-a"));
        bytes.extend(records); bytes.extend(surfaces);
        file.write_all(&bytes).unwrap();
        (file, format!("{:x}", Sha256::digest(bytes)))
    }

    #[test]
    fn prefix_lookup_recovers_unindexed_owners_and_preserves_scoring_variants() {
        let (_mf, model) = matrix();
        let (file, hash) = bank(&[("青",0,1,2,1,10), ("青い",5,1,1,0,30000), ("青い",5,1,1,0,70000)]);
        let sizes = [2; 15];
        let candidates = SourceCandidates::load(file.path(), &hash, &model, &sizes, "model-a").unwrap();
        let found = candidates.lookup("x青い空", 1).unwrap();
        assert_eq!(found.len(), 3);
        assert_eq!(found[0].end, 4);
        assert_eq!(found[1].end, 7);
        assert_eq!(found[1].word.dic(), 5);
        assert_eq!(found[2].cost, 70000);
        assert!(candidates.lookup("添へ",0).unwrap().is_empty());
        assert!(candidates.contains_word(WordId::new(5,1)));
        assert!(!candidates.contains_word(WordId::new(6,1)));
        let variants=candidates.parameters(WordId::new(5,1));
        assert_eq!(variants.len(),2);
        assert_eq!(variants[1].cost,70000);
        assert!(candidates.parameters(WordId::new(6,1)).is_empty());
        assert!(candidates.lookup("青い",1).is_err());
    }

    #[test]
    fn identity_parameters_deduplicate_spellings_without_losing_variants() {
        let (_mf,model)=matrix();
        let (file,hash)=bank(&[("あおい",5,1,1,0,10),("青い",5,1,1,0,10),("青い",5,1,2,1,20)]);
        let candidates=SourceCandidates::load(file.path(),&hash,&model,&[2;15],"model-a").unwrap();
        let variants=candidates.parameters(WordId::new(5,1));
        assert_eq!(variants.len(),2);
        assert_eq!((variants[1].left,variants[1].right,variants[1].cost),(2,1,20));
    }

    #[test]
    fn foreign_contexts_slots_and_rows_are_rejected_before_lookup() {
        let (_mf, model) = matrix();
        for rows in [vec![("青",0,1,3,0,10)], vec![("青",5,2,1,0,10)]] {
            let (file, hash) = bank(&rows);
            assert!(SourceCandidates::load(file.path(), &hash, &model, &[2;15], "model-a").is_err());
        }
    }

    #[test]
    fn duplicate_or_unsorted_states_and_wrong_fingerprints_are_rejected() {
        let (_mf, model) = matrix();
        for rows in [vec![("青",0,1,1,0,10),("青",0,1,1,0,10)],
                     vec![("青い",0,1,1,0,10),("青",0,1,1,0,10)]] {
            let (file, hash) = bank(&rows);
            assert!(SourceCandidates::load(file.path(), &hash, &model, &[2;15], "model-a").is_err());
        }
        let (file, _) = bank(&[("青",0,1,1,0,10)]);
        assert!(SourceCandidates::load(file.path(), "wrong", &model, &[2;15], "model-a").is_err());
        let (file, hash) = bank(&[("青",0,1,1,0,10)]);
        assert!(SourceCandidates::load(file.path(), &hash, &model, &[2;15], "model-b").is_err());
    }
}
