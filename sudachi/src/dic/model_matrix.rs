//! Checked, shared MeCab connection matrices. Numeric IDs are model-local.
use crate::error::SudachiResult;
use memmap2::Mmap;
use sha2::{Digest, Sha256};
use std::{collections::HashMap, fs::File, io::{Error, ErrorKind}, path::Path,
          sync::{Arc, Mutex, OnceLock, Weak}};

pub struct ModelMatrix {
    bytes: Mmap,
    preceding: usize,
    following: usize,
}

static MATRICES: OnceLock<Mutex<HashMap<String, Weak<ModelMatrix>>>> = OnceLock::new();

fn invalid(message: &str) -> Error {
    Error::new(ErrorKind::InvalidData, message)
}

impl ModelMatrix {
    pub fn load(path: &Path, expected_sha256: &str) -> SudachiResult<Arc<Self>> {
        // Always verify the supplied artifact, including cache hits. A replaced
        // path must not acquire a cached matrix under a stale fingerprint.
        let file = File::open(path)?;
        if file.metadata()?.len() < 4 {
            return Err(invalid("truncated selected-model matrix").into());
        }
        let bytes = unsafe { Mmap::map(&file)? };
        let hash = format!("{:x}", Sha256::digest(&bytes));
        if hash != expected_sha256 {
            return Err(invalid("selected-model matrix fingerprint mismatch").into());
        }
        let preceding = u16::from_le_bytes(bytes[0..2].try_into().unwrap()) as usize;
        let following = u16::from_le_bytes(bytes[2..4].try_into().unwrap()) as usize;
        let expected = preceding.checked_mul(following).and_then(|n| n.checked_mul(2))
            .and_then(|n| n.checked_add(4));
        if preceding == 0 || following == 0 || expected != Some(bytes.len()) {
            return Err(invalid("invalid selected-model matrix dimensions or length").into());
        }
        let mut cache = MATRICES.get_or_init(|| Mutex::new(HashMap::new())).lock()
            .map_err(|_| invalid("selected-model matrix cache lock poisoned"))?;
        if let Some(matrix) = cache.get(&hash).and_then(Weak::upgrade) {
            return Ok(matrix);
        }
        cache.retain(|_, value| value.strong_count() > 0);
        let matrix = Arc::new(Self { bytes, preceding, following });
        cache.insert(hash, Arc::downgrade(&matrix));
        Ok(matrix)
    }

    pub fn preceding(&self) -> usize { self.preceding }
    pub fn following(&self) -> usize { self.following }

    pub fn cost(&self, prior_right: u16, next_left: u16) -> SudachiResult<i16> {
        if prior_right as usize >= self.preceding || next_left as usize >= self.following {
            return Err(invalid("connection ID outside selected-model matrix").into());
        }
        let offset = 4 + 2 * (prior_right as usize + self.preceding * next_left as usize);
        Ok(i16::from_le_bytes(self.bytes[offset..offset+2].try_into().unwrap()))
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::io::Write;

    pub(crate) fn matrix(values: &[i16]) -> (tempfile::NamedTempFile, String) {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        let mut bytes = Vec::new();
        bytes.extend(2u16.to_le_bytes());
        bytes.extend(3u16.to_le_bytes());
        for cost in values { bytes.extend(cost.to_le_bytes()); }
        file.write_all(&bytes).unwrap();
        (file, format!("{:x}", Sha256::digest(bytes)))
    }

    #[test]
    fn same_numeric_ids_use_different_model_matrices() {
        let (a, ah) = matrix(&[11,12,21,22,31,32]);
        let (b, bh) = matrix(&[-11,-12,-21,-22,-31,-32]);
        let a = ModelMatrix::load(a.path(), &ah).unwrap();
        let b = ModelMatrix::load(b.path(), &bh).unwrap();
        assert_eq!(a.cost(1,2).unwrap(), 32);
        assert_eq!(b.cost(1,2).unwrap(), -32);
        assert_eq!(a.cost(0,1).unwrap(), 21);
        assert!(a.cost(2,0).is_err());
        assert!(a.cost(0,3).is_err());
    }

    #[test]
    fn live_matrix_mapping_is_shared_and_cache_hits_verify_input() {
        let (file, hash) = matrix(&[1,2,3,4,5,6]);
        let first = ModelMatrix::load(file.path(), &hash).unwrap();
        let second = ModelMatrix::load(file.path(), &hash).unwrap();
        assert!(Arc::ptr_eq(&first, &second));
        let (other, _) = matrix(&[6,5,4,3,2,1]);
        assert!(ModelMatrix::load(other.path(), &hash).is_err());
    }

    #[test]
    fn corrupt_dimensions_and_short_matrices_are_rejected() {
        let (file, hash) = matrix(&[1,2,3]);
        assert!(ModelMatrix::load(file.path(), &hash).is_err());
        let short = tempfile::NamedTempFile::new().unwrap();
        assert!(ModelMatrix::load(short.path(), "").is_err());
    }
}
