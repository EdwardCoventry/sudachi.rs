//! Character classes and unknown-word span rules belonging to the main model.
//! Layout/behaviour follow MeCab char_property and tokenizer, not stock Sudachi.
use crate::error::SudachiResult;
use memmap2::Mmap;
use sha2::{Digest, Sha256};
use std::{fs::File, io::{Error, ErrorKind}, path::Path};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CharacterInfo {
    pub types: u32,
    pub default_type: usize,
    pub length: usize,
    pub group: bool,
    pub invoke: bool,
}

pub struct ModelCharacters {
    bytes: Mmap,
    offset: usize,
    pub names: Vec<String>,
}

fn invalid(message: &str) -> Error { Error::new(ErrorKind::InvalidData, message) }

impl ModelCharacters {
    pub fn load(path: &Path, expected_sha256: &str) -> SudachiResult<Self> {
        let file = File::open(path)?;
        if file.metadata()?.len() < 4 {
            return Err(invalid("truncated selected-model character table").into());
        }
        let bytes = unsafe { Mmap::map(&file)? };
        if format!("{:x}", Sha256::digest(&bytes)) != expected_sha256 {
            return Err(invalid("selected-model character fingerprint mismatch").into());
        }
        let count = u32::from_le_bytes(bytes[..4].try_into().unwrap()) as usize;
        if count == 0 || count >= 18 || bytes.len() != 4+32*count+4*0xffff {
            return Err(invalid("invalid selected-model character dimensions").into());
        }
        let mut names = Vec::with_capacity(count);
        for index in 0..count {
            let encoded = &bytes[4+32*index..4+32*(index+1)];
            let name = std::str::from_utf8(encoded.split(|b| *b == 0).next().unwrap())
                .map_err(|_| invalid("invalid selected-model category name"))?;
            if name.is_empty() || !name.is_ascii() || names.iter().any(|old| old == name) {
                return Err(invalid("invalid or duplicate selected-model category name").into());
            }
            names.push(name.to_owned());
        }
        if !names.iter().any(|name| name == "DEFAULT") || !names.iter().any(|name| name == "SPACE") {
            return Err(invalid("selected-model categories omit DEFAULT or SPACE").into());
        }
        let table = Self { bytes, offset: 4+32*count, names };
        for codepoint in 0..0xffff {
            let info = table.at(codepoint);
            if info.default_type >= count || info.types >> count != 0 ||
                info.types & (1 << info.default_type) == 0 {
                return Err(invalid("character refers to an unavailable selected-model category").into());
            }
        }
        Ok(table)
    }

    fn at(&self, codepoint: usize) -> CharacterInfo {
        let start = self.offset + 4*codepoint;
        let value = u32::from_le_bytes(self.bytes[start..start+4].try_into().unwrap());
        CharacterInfo { types: value & 0x3ffff, default_type: ((value >> 18) & 0xff) as usize,
            length: ((value >> 26) & 15) as usize, group: value & (1<<30) != 0, invoke: value & (1<<31) != 0 }
    }

    pub fn info(&self, ch: char) -> SudachiResult<CharacterInfo> {
        let codepoint = ch as usize;
        if codepoint == 0xffff {
            return Err(invalid("Unicode noncharacter has no selected-model table entry").into());
        }
        // MeCab UTF-8 -> UCS-2 maps supplementary scalars to entry zero.
        Ok(self.at(if codepoint < 0xffff { codepoint } else { 0 }))
    }

    pub fn skip_space(&self, text: &str, offset: usize) -> SudachiResult<usize> {
        let rest = text.get(offset..).ok_or_else(|| invalid("space lookup is not a UTF-8 boundary"))?;
        let mut category = self.info(' ')?;
        let mut end = offset;
        for (index, ch) in rest.char_indices() {
            let next = self.info(ch)?;
            if category.types & next.types == 0 { break; }
            end = offset + index + ch.len_utf8();
            category = next;
        }
        Ok(end)
    }

    pub fn unknown_ends(&self, text: &str, offset: usize, has_known: bool,
                        max_grouping_size: usize) -> SudachiResult<Vec<usize>> {
        let rest = text.get(offset..).ok_or_else(|| invalid("unknown lookup is not a UTF-8 boundary"))?;
        let mut chars = rest.char_indices();
        let Some((_, first)) = chars.next() else { return Ok(Vec::new()); };
        let category = self.info(first)?;
        if has_known && !category.invoke { return Ok(Vec::new()); }
        let first_end = offset + first.len_utf8();
        let mut ends = Vec::new();
        if category.group {
            let mut prior = category;
            let mut group_end = first_end;
            let mut following_count = 0;
            for (index, ch) in chars.clone() {
                let next = self.info(ch)?;
                if prior.types & next.types == 0 { break; }
                group_end = offset + index + ch.len_utf8();
                following_count += 1;
                prior = next;
            }
            if following_count <= max_grouping_size { ends.push(group_end); }
        }
        let mut end = first_end;
        for _ in 1..=category.length {
            if !ends.contains(&end) { ends.push(end); }
            let Some((index, ch)) = chars.next() else { break; };
            if category.types & self.info(ch)?.types == 0 { break; }
            end = offset + index + ch.len_utf8();
        }
        if !has_known && ends.is_empty() { ends.push(end); }
        ends.sort_unstable();
        Ok(ends)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::io::Write;

    pub(crate) fn table(length: u32, group: bool, invoke: bool) -> (tempfile::NamedTempFile, ModelCharacters) {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        let mut bytes = Vec::from(3u32.to_le_bytes());
        for name in ["DEFAULT", "SPACE", "HIRAGANA"] {
            let mut encoded = [0u8;32]; encoded[..name.len()].copy_from_slice(name.as_bytes()); bytes.extend(encoded);
        }
        for cp in 0..0xffff {
            let value = if cp == 32 { 2 | (1<<18) | (1<<30) }
                else if (0x3041..=0x3096).contains(&cp) {
                    4 | (2<<18) | (length<<26) | ((group as u32)<<30) | ((invoke as u32)<<31)
                } else { 1 | (1<<31) };
            bytes.extend(value.to_le_bytes());
        }
        file.write_all(&bytes).unwrap();
        let hash = format!("{:x}", Sha256::digest(bytes));
        let model = ModelCharacters::load(file.path(), &hash).unwrap();
        (file, model)
    }

    #[test]
    fn grouping_length_and_invocation_follow_selected_categories() {
        let (_file, model) = table(2, true, false);
        assert!(model.unknown_ends("あいう漢",0,true,10).unwrap().is_empty());
        assert_eq!(model.unknown_ends("あいう漢",0,false,10).unwrap(), vec![3,6,9]);
        assert_eq!(model.unknown_ends("あいう漢",0,false,1).unwrap(), vec![3,6]);
        let (_file, model) = table(0, false, true);
        assert_eq!(model.unknown_ends("あいう",0,false,10).unwrap(), vec![3]);
        assert!(model.unknown_ends("あいう",0,true,10).unwrap().is_empty());
    }

    #[test]
    fn supplementary_unicode_and_space_spans_preserve_source_behaviour() {
        let (_file, model) = table(1, false, true);
        assert_eq!(model.info('𠀀').unwrap(), model.info('\0').unwrap());
        assert_eq!(model.skip_space("x  あ",1).unwrap(),3);
        assert!(model.info('\u{ffff}').is_err());
        assert!(model.unknown_ends("青",1,false,10).is_err());
    }
}
