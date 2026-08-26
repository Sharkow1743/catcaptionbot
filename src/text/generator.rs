use super::types::{Pos, WordSpec};
use anyhow::anyhow;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use fastembed::{InitOptionsUserDefined, TextEmbedding, TokenizerFiles, UserDefinedEmbeddingModel};
use rand::distr::Distribution;
use rand::distr::weighted::WeightedIndex;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::str::FromStr;
use std::sync::{Arc, RwLock};
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct WordCandidate {
    pub word: String,
    pub weight: f64,
}

#[derive(Clone)]
struct WordStore {
    all_words: Vec<WordCandidate>,
    by_pos: HashMap<Pos, Vec<WordCandidate>>,
    by_id: HashMap<i64, (String, Pos, f64)>,
    word_to_id: HashMap<String, i64>,
}

#[derive(Clone)]
pub struct WordGenerator {
    pool: SqlitePool,
    embedder: Arc<Mutex<TextEmbedding>>,
    embedding_cache: Arc<RwLock<HashMap<String, Vec<f32>>>>,
    word_store: Arc<WordStore>,
}

impl WordGenerator {
    pub async fn new(db_path: &str) -> anyhow::Result<Self> {
        Self::new_with_local_model(db_path, "robert-tiny2").await
    }

    pub async fn new_with_local_model(
        db_path: &str,
        model_dir: impl AsRef<Path>,
    ) -> anyhow::Result<Self> {
        let connection_options = SqliteConnectOptions::from_str(&format!("sqlite:{}", db_path))?
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
            .synchronous(sqlx::sqlite::SqliteSynchronous::Normal)
            .pragma("mmap_size", "268435456")
            .pragma("cache_size", "-64000")
            .pragma("temp_store", "MEMORY");

        let pool = SqlitePoolOptions::new()
            .max_connections(10)
            .connect_with(connection_options)
            .await?;

        // Preload words into in-memory store for instant zero-SQL candidate selection
        let rows: Vec<(i64, String, String, f64)> =
            sqlx::query_as("SELECT id, word, pos, weight FROM words")
                .fetch_all(&pool)
                .await
                .unwrap_or_default();

        let mut all_words = Vec::with_capacity(rows.len());
        let mut by_pos: HashMap<Pos, Vec<WordCandidate>> = HashMap::new();
        let mut by_id: HashMap<i64, (String, Pos, f64)> = HashMap::with_capacity(rows.len());
        let mut word_to_id: HashMap<String, i64> = HashMap::with_capacity(rows.len());

        for (id, word, pos_str, weight) in rows {
            let pos = match pos_str.as_str() {
                "NOUN" => Pos::Noun,
                "ADJ" | "ADJF" | "ADJS" => Pos::Adj,
                "VERB" | "INFN" => Pos::Verb,
                "PRT" | "PRTF" | "PRTS" => Pos::Prt,
                "GRND" => Pos::Grnd,
                "ADV" | "ADVB" => Pos::Adv,
                "COMP" => Pos::Comp,
                "PRED" => Pos::Pred,
                "NPRO" => Pos::Npro,
                "NUMR" => Pos::Numr,
                "PREP" => Pos::Prep,
                _ => Pos::Misc,
            };

            let candidate = WordCandidate {
                word: word.clone(),
                weight: weight.max(0.0001),
            };

            by_pos.entry(pos).or_default().push(candidate.clone());
            by_id.insert(id, (word.clone(), pos, weight.max(0.0001)));
            word_to_id.insert(word.to_lowercase(), id);
            all_words.push(candidate);
        }

        let word_store = Arc::new(WordStore {
            all_words,
            by_pos,
            by_id,
            word_to_id,
        });

        let dir = model_dir.as_ref();
        let onnx_bytes = fs::read(dir.join("model.onnx"))?;
        let tokenizer_bytes = fs::read(dir.join("tokenizer.json"))?;

        let tokenizer_files = TokenizerFiles {
            tokenizer_file: tokenizer_bytes,
            config_file: fs::read(dir.join("config.json")).unwrap_or_default(),
            special_tokens_map_file: fs::read(dir.join("special_tokens_map.json"))
                .unwrap_or_default(),
            tokenizer_config_file: fs::read(dir.join("tokenizer_config.json")).unwrap_or_default(),
        };

        let user_model = UserDefinedEmbeddingModel::new(onnx_bytes, tokenizer_files);
        let embedder = TextEmbedding::try_new_from_user_defined(
            user_model,
            InitOptionsUserDefined::default(),
        )?;

        Ok(Self {
            pool,
            embedder: Arc::new(Mutex::new(embedder)),
            embedding_cache: Arc::new(RwLock::new(HashMap::with_capacity(1024))),
            word_store,
        })
    }

    // ========================================================================
    // HELPER FUNCTIONS FOR VECTORIZATION
    // ========================================================================

    /// Direct point-lookup by rowid avoiding virtual-table JOIN stalls
    #[allow(dead_code)]
    pub async fn find_vector_in_db(
        &self,
        word: impl AsRef<str>,
    ) -> anyhow::Result<Option<Vec<f32>>> {
        let word_ref = word.as_ref().trim().to_lowercase();

        let id = if let Some(&id) = self.word_store.word_to_id.get(&word_ref) {
            Some(id)
        } else {
            let row: Option<(i64,)> = sqlx::query_as("SELECT id FROM words WHERE word = ? LIMIT 1")
                .bind(&word_ref)
                .fetch_optional(&self.pool)
                .await?;
            row.map(|(i,)| i)
        };

        if let Some(rowid) = id {
            let row: Option<(Vec<u8>,)> =
                sqlx::query_as("SELECT embedding FROM vec_words WHERE rowid = ? LIMIT 1")
                    .bind(rowid)
                    .fetch_optional(&self.pool)
                    .await?;

            return Ok(row.map(|(bytes,)| bytes_to_f32_vec(&bytes)));
        }

        Ok(None)
    }

    #[allow(dead_code)]
    pub async fn get_or_create_embedding(&self, text: impl AsRef<str>) -> anyhow::Result<Vec<f32>> {
        let text_ref = text.as_ref();
        let trimmed = text_ref.trim().to_lowercase();

        if let Ok(cache) = self.embedding_cache.read() {
            if let Some(vec) = cache.get(&trimmed) {
                return Ok(vec.clone());
            }
        }

        if let Ok(Some(vec)) = self.find_vector_in_db(&trimmed).await {
            if let Ok(mut cache) = self.embedding_cache.write() {
                cache.insert(trimmed, vec.clone());
            }
            return Ok(vec);
        }

        let mut embedder = self.embedder.lock().await;
        let embeddings = embedder.embed(vec![trimmed.clone()], None)?;

        let vec = embeddings
            .into_iter()
            .next()
            .ok_or_else(|| anyhow!("Failed to compute embedding"))?;

        if let Ok(mut cache) = self.embedding_cache.write() {
            cache.insert(trimmed, vec.clone());
        }

        Ok(vec)
    }

    // ========================================================================
    // MAIN PUBLIC QUERY FUNCTIONS
    // ========================================================================

    #[allow(dead_code)]
    pub async fn get_random_words(&self, count: usize) -> anyhow::Result<Vec<String>> {
        self.get_random_words_by_pos(&[], count).await
    }

    #[allow(dead_code)]
    pub async fn get_random_words_by_pos(
        &self,
        poses: impl AsRef<[Pos]>,
        count: usize,
    ) -> anyhow::Result<Vec<String>> {
        let candidates = self.fetch_in_memory_candidates(poses.as_ref(), &[]);
        Ok(weighted_sample_without_replacement(&candidates, count))
    }

    #[allow(dead_code)]
    pub async fn get_random_words_by_similarity(
        &self,
        target_vec: impl AsRef<[f32]>,
        count: usize,
        top_k: Option<usize>,
    ) -> anyhow::Result<Vec<String>> {
        self.get_random_words_by_pos_and_similarity(&[], target_vec, count, top_k)
            .await
    }

    #[allow(dead_code)]
    pub async fn get_random_words_by_pos_and_similarity(
        &self,
        poses: impl AsRef<[Pos]>,
        target_vec: impl AsRef<[f32]>,
        count: usize,
        top_k: Option<usize>,
    ) -> anyhow::Result<Vec<String>> {
        let poses_slice = poses.as_ref();
        let target_slice = target_vec.as_ref();

        if count == 0 {
            return Ok(vec![]);
        }

        let candidates = self
            .fetch_knn_candidates(target_slice, poses_slice, &[], top_k, None)
            .await?;

        Ok(weighted_sample_without_replacement(&candidates, count))
    }

    pub async fn get_random_words_by_template(
        &self,
        template: impl AsRef<[WordSpec]>,
    ) -> anyhow::Result<Vec<String>> {
        let template = template.as_ref();
        let mut result = Vec::with_capacity(template.len());
        let mut already_chosen: Vec<String> = Vec::new();

        for spec in template {
            if let Some(ref fixed) = spec.fixed_word {
                result.push(fixed.clone());
                already_chosen.push(fixed.to_lowercase());
                continue;
            }
            let poses = spec.poses.as_deref().unwrap_or(&[]);

            let resolved_target_vec: Option<Vec<f32>> = match (
                &spec.target_vec,
                &spec.target_vec_q8_str,
                &spec.target_vec_str,
            ) {
                (Some(v), _, _) => Some(v.clone()),
                (None, Some(s), _) => Some(decode_embedding_q8_str(s)?),
                (None, None, Some(s)) => Some(decode_embedding_str(s)?),
                (None, None, None) => None,
            };

            let mut excludes = already_chosen.clone();
            if let Some(ref extra) = spec.exclude {
                excludes.extend(extra.iter().map(|w| w.to_lowercase()));
            }

            let sampled = if let Some(ref target_vec) = resolved_target_vec {
                let candidates = self
                    .fetch_knn_candidates(
                        target_vec,
                        poses,
                        &excludes,
                        spec.top_k,
                        spec.min_similarity,
                    )
                    .await?;
                weighted_sample_without_replacement(&candidates, 1)
            } else {
                let pool = self.fetch_in_memory_candidates(poses, &excludes);
                weighted_sample_without_replacement(&pool, 1)
            };

            if let Some(word) = sampled.into_iter().next() {
                already_chosen.push(word.to_lowercase());
                result.push(word);
            }
        }
        Ok(result)
    }

    #[allow(dead_code)]
    pub async fn get_random_words_by_pos_and_similarity_str(
        &self,
        poses: impl AsRef<[Pos]>,
        encoded_target_vec: impl AsRef<str>,
        count: usize,
        top_k: Option<usize>,
    ) -> anyhow::Result<Vec<String>> {
        let target_vec = decode_embedding_str(encoded_target_vec)?;
        self.get_random_words_by_pos_and_similarity(poses, &target_vec, count, top_k)
            .await
    }

    // ========================================================================
    // FAST IN-MEMORY & KNN SAMPLING
    // ========================================================================

    fn fetch_in_memory_candidates(&self, poses: &[Pos], excludes: &[String]) -> Vec<WordCandidate> {
        let candidates: Vec<&WordCandidate> = if poses.is_empty() {
            self.word_store.all_words.iter().collect()
        } else {
            poses
                .iter()
                .filter_map(|p| self.word_store.by_pos.get(p))
                .flatten()
                .collect()
        };

        if excludes.is_empty() {
            candidates.into_iter().cloned().collect()
        } else {
            let exclude_set: HashSet<&str> = excludes.iter().map(|s| s.as_str()).collect();
            candidates
                .into_iter()
                .filter(|c| !exclude_set.contains(c.word.to_lowercase().as_str()))
                .cloned()
                .collect()
        }
    }

    async fn fetch_knn_candidates(
        &self,
        target_vec: &[f32],
        poses: &[Pos],
        excludes: &[String],
        top_k: Option<usize>,
        min_similarity: Option<f32>,
    ) -> anyhow::Result<Vec<WordCandidate>> {
        let k = top_k.unwrap_or(64).max(32);
        let search_k = if !poses.is_empty() || !excludes.is_empty() {
            (k * 6).clamp(128, 1024)
        } else {
            k
        };

        let target_bytes = f32_vec_to_bytes(target_vec);

        let rows: Vec<(i64, f64)> = sqlx::query_as(
            "SELECT rowid, distance FROM vec_words WHERE embedding MATCH ? AND k = ?",
        )
        .bind(target_bytes)
        .bind(search_k as i64)
        .fetch_all(&self.pool)
        .await?;

        let min_sim = min_similarity.unwrap_or(-1.0);
        let exclude_set: HashSet<String> = excludes.iter().map(|w| w.to_lowercase()).collect();
        let pos_set: HashSet<Pos> = poses.iter().copied().collect();

        let mut candidates: Vec<WordCandidate> = Vec::with_capacity(rows.len());

        for (rowid, distance) in rows {
            let sim = 1.0 - distance as f32;
            if sim < min_sim {
                continue;
            }

            if let Some((word, pos, weight)) = self.word_store.by_id.get(&rowid) {
                if !pos_set.is_empty() && !pos_set.contains(pos) {
                    continue;
                }
                if !exclude_set.is_empty() && exclude_set.contains(&word.to_lowercase()) {
                    continue;
                }
                candidates.push(WordCandidate {
                    word: word.clone(),
                    weight: *weight,
                });
            }
        }

        if let Some(limit) = top_k {
            candidates.truncate(limit);
        }

        Ok(candidates)
    }
}

// ============================================================================
// VECTOR CONVERSION & SAMPLING UTILITIES
// ============================================================================

fn bytes_to_f32_vec(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|chunk| f32::from_le_bytes(chunk.try_into().unwrap()))
        .collect()
}

fn f32_vec_to_bytes(vec: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(vec.len() * 4);
    for &f in vec {
        bytes.extend_from_slice(&f.to_le_bytes());
    }
    bytes
}

fn weighted_sample_without_replacement(candidates: &[WordCandidate], count: usize) -> Vec<String> {
    if candidates.is_empty() || count == 0 {
        return vec![];
    }

    let mut rng = rand::rng();
    let mut pool: Vec<&WordCandidate> = candidates.iter().collect();
    let mut result = Vec::with_capacity(count);

    for _ in 0..count {
        if pool.is_empty() {
            break;
        }

        let weights: Vec<f64> = pool.iter().map(|c| c.weight.max(0.0001)).collect();
        if let Ok(dist) = WeightedIndex::new(&weights) {
            let idx = dist.sample(&mut rng);
            let selected = pool.remove(idx);
            result.push(selected.word.clone());
        } else {
            break;
        }
    }

    result
}

#[allow(dead_code)]
pub fn encode_embedding(v: &[f32]) -> String {
    let mut bytes = Vec::with_capacity(v.len() * 4);
    for f in v {
        bytes.extend_from_slice(&f.to_le_bytes());
    }
    STANDARD.encode(bytes)
}

pub fn decode_embedding_str(s: impl AsRef<str>) -> anyhow::Result<Vec<f32>> {
    let bytes = STANDARD.decode(s.as_ref().trim())?;
    if bytes.len() % 4 != 0 {
        return Err(anyhow!(
            "invalid embedding string: {} bytes is not a multiple of 4",
            bytes.len()
        ));
    }
    Ok(bytes_to_f32_vec(&bytes))
}

#[allow(dead_code)]
pub fn encode_embedding_q8(v: &[f32]) -> String {
    let max_abs = v.iter().fold(0.0f32, |acc, x| acc.max(x.abs()));
    let scale = if max_abs > 0.0 { max_abs / 127.0 } else { 1.0 };

    let mut bytes = Vec::with_capacity(4 + v.len());
    bytes.extend_from_slice(&scale.to_le_bytes());
    for &f in v {
        let q = (f / scale).round().clamp(-127.0, 127.0) as i8;
        bytes.push(q as u8);
    }
    STANDARD.encode(bytes)
}

pub fn decode_embedding_q8_str(s: impl AsRef<str>) -> anyhow::Result<Vec<f32>> {
    let bytes = STANDARD.decode(s.as_ref().trim())?;
    if bytes.len() < 5 {
        return Err(anyhow!(
            "invalid quantized embedding string: {} bytes, need at least 5",
            bytes.len()
        ));
    }
    let scale = f32::from_le_bytes(bytes[0..4].try_into().unwrap());
    let vec = bytes[4..]
        .iter()
        .map(|&b| (b as i8) as f32 * scale)
        .collect();
    Ok(vec)
}
