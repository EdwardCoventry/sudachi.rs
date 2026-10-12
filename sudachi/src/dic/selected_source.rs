//! One source owns discovery, matrix parameters and unknown-word behaviour.
use crate::{dic::{grammar::Grammar, model_characters::ModelCharacters, model_matrix::ModelMatrix,
                 source_candidates::{SourceCandidate, SourceCandidates}, word_id::WordId},
            error::SudachiResult};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{collections::HashMap, fs::File, io::{Error, ErrorKind}, path::Path, sync::Arc};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Asset { path: String, sha256: String }

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    schema: u32,
    main_lex: String,
    model_fingerprint: String,
    native_sha256: Vec<String>,
    matrix: Asset,
    candidates: Asset,
    characters: Option<Asset>,
    unknown: Option<Asset>,
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::dic::{model_characters::tests::table, model_matrix::tests::matrix,
                     source_candidates::tests::bank};
    use crate::util::testing::{build_mock_grammar,GRAMMAR_BYTES};
    use std::io::Write;

    pub(crate) fn fixture() -> (Vec<tempfile::NamedTempFile>, tempfile::NamedTempFile) {
        let (matrix_file, matrix_sha) = matrix(&[1,2,3,4,5,6]);
        let (characters_file, _) = table(1,false,true);
        let characters_sha = format!("{:x}", Sha256::digest(std::fs::read(characters_file.path()).unwrap()));
        let (candidate_file, candidate_sha) = bank(&[("青い",5,1,1,0,30000),("青い",5,1,1,0,70000)]);
        let mut unknown_file = tempfile::NamedTempFile::new().unwrap();
        let inventory = serde_json::json!({"schema":1,"model_fingerprint":"model-a",
            "analyzer_settings":{"max_grouping_size":10,"unknown_feature":null},
            "records":[
                {"category":"DEFAULT","left":2,"right":1,"cost":9,"pos":["名詞","普通名詞","一般","*","*","*"]},
                {"category":"SPACE","left":1,"right":0,"cost":8,"pos":["補助記号","空白","*","*","*","*"]},
                {"category":"HIRAGANA","left":0,"right":1,"cost":7,"pos":["名詞","普通名詞","一般","*","*","*"]}
            ]});
        let bytes = serde_json::to_vec(&inventory).unwrap();
        unknown_file.write_all(&bytes).unwrap();
        let unknown_sha = format!("{:x}", Sha256::digest(bytes));
        let mut manifest_file = tempfile::NamedTempFile::new().unwrap();
        let native_hash = format!("{:x}",Sha256::digest([]));
        let manifest = serde_json::json!({"schema":1,"main_lex":"ninjal-chuko","model_fingerprint":"model-a",
            "native_sha256":vec![native_hash;15],
            "matrix":{"path":matrix_file.path(),"sha256":matrix_sha},
            "candidates":{"path":candidate_file.path(),"sha256":candidate_sha},
            "characters":{"path":characters_file.path(),"sha256":characters_sha},
            "unknown":{"path":unknown_file.path(),"sha256":unknown_sha}});
        manifest_file.write_all(&serde_json::to_vec(&manifest).unwrap()).unwrap();
        (vec![matrix_file,characters_file,candidate_file,unknown_file],manifest_file)
    }

    #[test]
    fn source_owns_known_variants_unknown_grammar_and_every_parameter() {
        let (_assets, manifest) = fixture();
        let mut grammar = build_mock_grammar(&GRAMMAR_BYTES);
        let source = SelectedSource::load(manifest.path(),&mut grammar,&[&[] as &[u8];15],&[2;15]).unwrap();
        let known = source.lookup("青い",0).unwrap();
        assert_eq!(known.len(),2);
        source.require_word(known[0].word).unwrap();
        let excluded=WordId::checked(6,1).unwrap();
        assert!(matches!(source.require_word(excluded),
            Err(crate::error::SudachiError::ProfileConstraintConflict {word_id,..}) if word_id==excluded.as_raw()));
        assert_eq!(known[1].cost,70000);
        assert_eq!(known[1].word.dic(),5);
        let unknown = source.lookup("あ",0).unwrap();
        assert_eq!(unknown.len(),1);
        assert!(unknown[0].word.is_oov());
        assert_eq!((unknown[0].left,unknown[0].right,unknown[0].cost),(0,1,7));
        assert_eq!(grammar.pos_components(unknown[0].word.word() as u16)[0],"名詞");
        assert_eq!(source.skip_space("  あ",0).unwrap(),2);
        assert_eq!(source.matrix.cost(unknown[0].right,known[0].left).unwrap(),4);
    }

    #[test]
    fn sudachi_keeps_native_unknown_behaviour_and_rejects_foreign_oov_assets() {
        let (_assets, manifest) = fixture();
        let mut value: serde_json::Value=serde_json::from_reader(File::open(manifest.path()).unwrap()).unwrap();
        value["main_lex"]=serde_json::json!("sudachi");
        let mut grammar=build_mock_grammar(&GRAMMAR_BYTES);
        std::fs::write(manifest.path(),serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(SelectedSource::load(manifest.path(),&mut grammar,&[&[] as &[u8];15],&[2;15]).is_err());
        value["characters"]=serde_json::Value::Null;
        value["unknown"]=serde_json::Value::Null;
        std::fs::write(manifest.path(),serde_json::to_vec(&value).unwrap()).unwrap();
        let source=SelectedSource::load(manifest.path(),&mut grammar,&[&[] as &[u8];15],&[2;15]).unwrap();
        assert!(source.uses_native_oov());
        assert_eq!(source.lookup("青い",0).unwrap().len(),2);
        assert!(source.lookup("あ",0).unwrap().is_empty());
        assert_eq!(source.skip_space("  あ",0).unwrap(),0);
    }

    #[test]
    fn fixed_inventory_fingerprints_prevent_slot_reordering_or_partial_loads() {
        let (_assets, manifest) = fixture();
        let mut grammar = build_mock_grammar(&GRAMMAR_BYTES);
        assert!(SelectedSource::load(manifest.path(),&mut grammar,&[&[] as &[u8];14],&[2;14]).is_err());
        let mut native: [&[u8];15] = [&[];15];
        native[5]=b"wrong native slot";
        assert!(SelectedSource::load(manifest.path(),&mut grammar,&native,&[2;15]).is_err());
    }

    #[test]
    fn fixed_split_scoring_uses_full_sequence_and_selected_boundaries() {
        let (file,hash)=matrix(&[1,10,3,4,5,6]);
        let matrix=ModelMatrix::load(file.path(),&hash).unwrap();
        let word=WordId::checked(5,1).unwrap();
        let states=vec![vec![
            SourceCandidate {word,left:1,right:1,cost:0,end:0},
            SourceCandidate {word,left:1,right:0,cost:2,end:0}],
            vec![SourceCandidate {word,left:0,right:0,cost:70000,end:0}]];
        let path=score_fixed_path(&matrix,&states).unwrap();
        // The more expensive first lexical state wins through its connection.
        assert_eq!((path[0].0.right,path[0].1,path[1].1),(0,5,70006));
        assert_eq!(path[1].1+matrix.cost(path[1].0.right,0).unwrap() as i64,70007);
        assert!(score_fixed_path(&matrix,&[vec![]]).is_err());
        assert!(add_cost(i64::MAX,1).is_err());
        assert!(score_fixed_path(&matrix,&[vec![SourceCandidate {word,left:3,right:0,cost:0,end:0}]]).is_err());
    }

    #[test]
    fn split_materialisation_replaces_storage_sentinels_and_keeps_spans() {
        use crate::analysis::{Node,node::{ResultNode,LatticeNode,PathCost,RightId}};
        use crate::dic::lexicon::word_infos::WordInfoData;
        let (_assets,manifest)=fixture();
        let mut grammar=build_mock_grammar(&GRAMMAR_BYTES);
        let source=SelectedSource::load(manifest.path(),&mut grammar,&[&[] as &[u8];15],&[2;15]).unwrap();
        let word=WordId::checked(5,1).unwrap();
        let mut nodes=vec![ResultNode::new(Node::new(2,4,u16::MAX,u16::MAX,i16::MAX,word),
            i64::MAX,6,12,WordInfoData::default().into())];
        source.materialize_path(&mut nodes).unwrap();
        assert_eq!((nodes[0].left_id(),nodes[0].right_id(),nodes[0].cost(),nodes[0].total_cost()),(1,0,30000,30003));
        assert_eq!((nodes[0].begin(),nodes[0].end(),nodes[0].begin_bytes(),nodes[0].end_bytes()),(2,4,6,12));
        let mut excluded=vec![ResultNode::new(Node::new(2,4,u16::MAX,u16::MAX,i16::MAX,WordId::new(6,1)),
            i64::MAX,6,12,WordInfoData::default().into())];
        assert!(matches!(source.materialize_path(&mut excluded),Err(crate::error::SudachiError::ProfileConstraintConflict {..})));
        assert_eq!(excluded[0].total_cost(),i64::MAX);
    }
}

#[derive(Deserialize)]
struct UnknownInventory {
    schema: u32,
    model_fingerprint: String,
    analyzer_settings: AnalyzerSettings,
    records: Vec<UnknownRecord>,
}

#[derive(Deserialize)]
struct AnalyzerSettings { max_grouping_size: usize, unknown_feature: Option<String> }

#[derive(Deserialize)]
struct UnknownRecord { category: String, left: u16, right: u16, cost: i32, pos: [String;6] }

struct UnknownVariant { word: WordId, left: u16, right: u16, cost: i32 }

enum UnknownBehaviour {
    Sudachi,
    Mecab { characters: ModelCharacters, variants: HashMap<usize,Vec<UnknownVariant>>, max_grouping_size: usize },
}

pub struct SelectedSource {
    pub main_lex: String,
    pub fingerprint: String,
    pub matrix: Arc<ModelMatrix>,
    candidates: SourceCandidates,
    unknown: UnknownBehaviour,
}

fn invalid(message: &str) -> Error { Error::new(ErrorKind::InvalidData, message) }

impl SelectedSource {
    pub fn load(path: &Path, grammar: &mut Grammar, native: &[&[u8]], sizes: &[usize]) -> SudachiResult<Self> {
        let manifest: Manifest = serde_json::from_reader(File::open(path)?)?;
        const SOURCES: [&str;8] = ["sudachi","ninjal-spoken","ninjal-written","ninjal-chuko","ninjal-chusei-bungo",
                                  "ninjal-kinsei-bungo","ninjal-qkana","ninjal-waka"];
        if manifest.schema != 1 || !SOURCES.contains(&manifest.main_lex.as_str()) {
            return Err(invalid("unsupported single-source manifest").into());
        }
        if native.len() != 15 || sizes.len() != 15 || manifest.native_sha256.len() != 15 {
            return Err(invalid("selected source requires the complete fixed native layout").into());
        }
        for (bytes, expected) in native.iter().zip(&manifest.native_sha256) {
            if format!("{:x}", Sha256::digest(bytes)) != *expected {
                return Err(invalid("selected source native inventory fingerprint mismatch").into());
            }
        }
        let base = path.parent().unwrap_or(Path::new("."));
        let matrix = ModelMatrix::load(&base.join(&manifest.matrix.path), &manifest.matrix.sha256)?;
        let candidates = SourceCandidates::load(&base.join(&manifest.candidates.path),
            &manifest.candidates.sha256, &matrix, sizes, &manifest.model_fingerprint)?;
        if manifest.main_lex=="sudachi" {
            if manifest.characters.is_some() || manifest.unknown.is_some() {
                return Err(invalid("Sudachi main source must use its native unknown-word behaviour").into());
            }
            return Ok(Self { main_lex:manifest.main_lex,fingerprint:manifest.model_fingerprint,matrix,candidates,
                             unknown:UnknownBehaviour::Sudachi });
        }
        let char_asset=manifest.characters.ok_or_else(||invalid("NINJAL main source omits its character table"))?;
        let unk_asset=manifest.unknown.ok_or_else(||invalid("NINJAL main source omits its unknown inventory"))?;
        let characters = ModelCharacters::load(&base.join(&char_asset.path), &char_asset.sha256)?;
        let unknown_bytes = std::fs::read(base.join(&unk_asset.path))?;
        if format!("{:x}", Sha256::digest(&unknown_bytes)) != unk_asset.sha256 {
            return Err(invalid("selected-model unknown-word fingerprint mismatch").into());
        }
        let inventory: UnknownInventory = serde_json::from_slice(&unknown_bytes)?;
        if inventory.schema != 1 || inventory.model_fingerprint != manifest.model_fingerprint {
            return Err(invalid("unknown words belong to a different selected model").into());
        }
        let override_pos = match inventory.analyzer_settings.unknown_feature {
            Some(value) => {
                let values: Vec<_> = value.split(',').map(str::to_owned).collect();
                if values.len() != 6 {
                    return Err(invalid("selected-model unknown-feature override has unsupported grammar schema").into());
                }
                Some(grammar.register_pos(&values)?)
            }
            None => None,
        };
        let mut unknown: HashMap<usize,Vec<UnknownVariant>> = HashMap::new();
        for row in inventory.records {
            let category = characters.names.iter().position(|name| name == &row.category)
                .ok_or_else(|| invalid("unknown word refers to an unavailable selected-model category"))?;
            matrix.cost(row.right,row.left)?;
            let pos = match override_pos { Some(pos) => pos, None => grammar.register_pos(&row.pos)? };
            unknown.entry(category).or_default().push(UnknownVariant {
                word: WordId::oov(pos as u32), left: row.left, right: row.right, cost: row.cost });
        }
        if (0..characters.names.len()).any(|category| unknown.get(&category).map_or(true,Vec::is_empty)) {
            return Err(invalid("selected-model unknown categories are incomplete").into());
        }
        Ok(Self { main_lex: manifest.main_lex, fingerprint: manifest.model_fingerprint, matrix,
            candidates, unknown:UnknownBehaviour::Mecab {characters,variants:unknown,
                max_grouping_size:inventory.analyzer_settings.max_grouping_size} })
    }

    pub fn uses_native_oov(&self) -> bool { matches!(self.unknown,UnknownBehaviour::Sudachi) }

    pub fn contains_word(&self, word: WordId) -> bool { self.candidates.contains_word(word) }

    pub fn require_word(&self, word: WordId) -> SudachiResult<()> {
        if !word.is_oov() && !self.contains_word(word) {
            return Err(crate::error::SudachiError::ProfileConstraintConflict {
                profile:self.main_lex.clone(),word_id:word.as_raw() });
        }
        Ok(())
    }

    /// Score a fixed decomposition as a standalone path in this model. Native
    /// split storage contains no runtime contexts, so every child obtains an
    /// attested state before its connection parameters can be consumed.
    pub(crate) fn materialize_path(&self, path: &mut [crate::analysis::node::ResultNode]) -> SudachiResult<()> {
        use crate::analysis::node::{LatticeNode,RightId};
        let mut states = Vec::with_capacity(path.len());
        for node in path.iter() {
            self.require_word(node.word_id())?;
            let variants = if node.is_oov() {
                self.matrix.cost(node.right_id(),node.left_id())?;
                vec![SourceCandidate {word:node.word_id(),left:node.left_id(),right:node.right_id(),cost:node.cost(),end:0}]
            } else { self.candidates.parameters(node.word_id()) };
            states.push(variants);
        }
        let choices = score_fixed_path(&self.matrix,&states)?;
        for (node,(state,total)) in path.iter_mut().zip(choices) {
            node.set_source_state(&state,total);
        }
        Ok(())
    }

    pub fn skip_space(&self, input: &str, offset: usize) -> SudachiResult<usize> {
        match &self.unknown {
            UnknownBehaviour::Sudachi=>Ok(offset),
            UnknownBehaviour::Mecab {characters,..}=>characters.skip_space(input,offset),
        }
    }

    pub fn lookup(&self, input: &str, offset: usize) -> SudachiResult<Vec<SourceCandidate>> {
        let mut nodes = self.candidates.lookup(input,offset)?;
        let UnknownBehaviour::Mecab {characters,variants,max_grouping_size}=&self.unknown else { return Ok(nodes); };
        let unknown_ends = characters.unknown_ends(input,offset,!nodes.is_empty(),*max_grouping_size)?;
        if !unknown_ends.is_empty() {
            let ch = input.get(offset..).and_then(|rest| rest.chars().next())
                .ok_or_else(|| invalid("unknown-word lookup has no source character"))?;
            let category = characters.info(ch)?.default_type;
            let variants = variants.get(&category).ok_or_else(|| invalid("selected-model unknown category has no records"))?;
            for end in unknown_ends {
                for variant in variants {
                    nodes.push(SourceCandidate { word: variant.word, left: variant.left,
                        right: variant.right, cost: variant.cost, end });
                }
            }
        }
        Ok(nodes)
    }
}

fn add_cost(a: i64, b: i64) -> SudachiResult<i64> {
    a.checked_add(b).ok_or_else(|| invalid("selected-model path cost overflow").into())
}

/// Select the cheapest complete sequence of legitimate lexical variants,
/// including this model's BOS and EOS transitions. Ties follow sorted states.
fn score_fixed_path(matrix: &ModelMatrix, states: &[Vec<SourceCandidate>])
    -> SudachiResult<Vec<(SourceCandidate,i64)>> {
    if states.is_empty() { return Ok(Vec::new()); }
    let mut costs: Vec<Vec<(i64,usize)>> = Vec::with_capacity(states.len());
    for (index,variants) in states.iter().enumerate() {
        if variants.is_empty() { return Err(invalid("selected-model split has no attested scoring states").into()); }
        let mut row = Vec::with_capacity(variants.len());
        for candidate in variants {
            let best = if index==0 {
                (add_cost(matrix.cost(0,candidate.left)? as i64,candidate.cost as i64)?,0)
            } else {
                let mut best = None;
                for (previous,prior) in states[index-1].iter().enumerate() {
                    let score = add_cost(add_cost(costs[index-1][previous].0,
                        matrix.cost(prior.right,candidate.left)? as i64)?,candidate.cost as i64)?;
                    if best.is_none_or(|old:(i64,usize)| score<old.0) { best=Some((score,previous)); }
                }
                best.ok_or_else(|| invalid("selected-model split is disconnected"))?
            };
            row.push(best);
        }
        costs.push(row);
    }
    let last = states.len()-1;
    let mut final_choice = None;
    for (index,state) in states[last].iter().enumerate() {
        let score = add_cost(costs[last][index].0,matrix.cost(state.right,0)? as i64)?;
        if final_choice.is_none_or(|old:(i64,usize)| score<old.0) { final_choice=Some((score,index)); }
    }
    let mut choice = final_choice.unwrap().1;
    let mut path = Vec::with_capacity(states.len());
    for index in (0..states.len()).rev() {
        path.push((states[index][choice],costs[index][choice].0));
        choice = costs[index][choice].1;
    }
    path.reverse();
    Ok(path)
}
