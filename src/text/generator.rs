use super::types::{Pos, WordSpec};
use anyhow::anyhow;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use fastembed::{InitOptionsUserDefined, TextEmbedding, TokenizerFiles, UserDefinedEmbeddingModel};
use rand::distr::Distribution;
use rand::distr::weighted::WeightedIndex;
use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};
use sqlx::{QueryBuilder, Sqlite};
use std::fs;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct WordGenerator {
    pool: SqlitePool,
    embedder: Arc<Mutex<TextEmbedding>>,
}

struct WordCandidate {
    word: String,
    weight: f64,
}

impl WordGenerator {
    /// Initializes DB Pool and loads local FastEmbed model from `robert-tiny2` directory
    pub async fn new(db_path: &str) -> anyhow::Result<Self> {
        Self::new_with_local_model(db_path, "robert-tiny2").await
    }

    /// Initializes DB Pool and loads FastEmbed using local ONNX model files from `model_dir`
    pub async fn new_with_local_model(
        db_path: &str,
        model_dir: impl AsRef<Path>,
    ) -> anyhow::Result<Self> {
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(&format!("sqlite:{}", db_path))
            .await?;

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
        })
    }

    // ========================================================================
    // HELPER FUNCTIONS FOR VECTORIZATION
    // ========================================================================

    /// Searches for a precomputed vector in the `vec_words` SQLite table
    pub async fn find_vector_in_db(
        &self,
        word: impl AsRef<str>,
    ) -> anyhow::Result<Option<Vec<f32>>> {
        let word_ref = word.as_ref();
        let row: Option<(Vec<u8>,)> = sqlx::query_as(
            "SELECT v.embedding
             FROM vec_words v
             JOIN words w ON v.rowid = w.id
             WHERE w.word = ?",
        )
        .bind(word_ref.to_lowercase())
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|(bytes,)| bytes_to_f32_vec(&bytes)))
    }

    /// Accepts a word or a multi-word phrase.
    /// Tries DB lookup first; if missing, computes embedding using local FastEmbed model.
    pub async fn get_or_create_embedding(&self, text: impl AsRef<str>) -> anyhow::Result<Vec<f32>> {
        let text_ref = text.as_ref();
        let trimmed = text_ref.trim().to_lowercase();

        if let Ok(Some(vec)) = self.find_vector_in_db(&trimmed).await {
            return Ok(vec);
        }

        let mut embedder = self.embedder.lock().await;
        let embeddings = embedder.embed(vec![trimmed], None)?;

        embeddings
            .into_iter()
            .next()
            .ok_or_else(|| anyhow!("Failed to compute embedding"))
    }

    // ========================================================================
    // MAIN PUBLIC QUERY FUNCTIONS
    // ========================================================================

    /// Get N weighted random words (any POS)
    pub async fn get_random_words(&self, count: usize) -> anyhow::Result<Vec<String>> {
        self.get_random_words_by_pos(&[], count).await
    }

    /// Get N weighted random words filtered by POS
    pub async fn get_random_words_by_pos(
        &self,
        poses: impl AsRef<[Pos]>,
        count: usize,
    ) -> anyhow::Result<Vec<String>> {
        let poses_slice = poses.as_ref();
        if count == 0 {
            return Ok(vec![]);
        }

        let mut builder: QueryBuilder<Sqlite> = QueryBuilder::new("SELECT word FROM words ");

        if !poses_slice.is_empty() {
            builder.push("WHERE pos IN (");
            let mut separated = builder.separated(", ");
            for pos in poses_slice {
                separated.push_bind(pos.as_str());
            }
            separated.push_unseparated(") ");
        }

        builder.push("ORDER BY RANDOM() * MAX(weight, 0.0001) DESC LIMIT ");
        builder.push_bind(count as i64);

        let rows: Vec<(String,)> = builder.build_query_as().fetch_all(&self.pool).await?;
        Ok(rows.into_iter().map(|(w,)| w).collect())
    }

    /// Get N random words close in meaning to target vector
    pub async fn get_random_words_by_similarity(
        &self,
        target_vec: impl AsRef<[f32]>,
        count: usize,
        top_k: Option<usize>,
    ) -> anyhow::Result<Vec<String>> {
        self.get_random_words_by_pos_and_similarity(&[], target_vec, count, top_k)
            .await
    }

    /// Get N random words filtered by POS AND close in meaning to target vector
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

    /// Get random words matching a template specification (`&[WordSpec]`)
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

            // Resolve the target vector from either an already-materialized Vec<f32>
            // or a precomputed base64 string.
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

            let candidates = if let Some(ref target_vec) = resolved_target_vec {
                self.fetch_knn_candidates(
                    target_vec,
                    poses,
                    &excludes,
                    spec.top_k,
                    spec.min_similarity,
                )
                .await?
            } else {
                self.fetch_random_candidates(poses, &excludes, 100).await?
            };

            let sampled = weighted_sample_without_replacement(&candidates, 1);
            if let Some(word) = sampled.into_iter().next() {
                already_chosen.push(word.to_lowercase());
                result.push(word);
            }
        }
        Ok(result)
    }

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
    // PRIVATE SQLITE-VEC QUERY HELPERS
    // ========================================================================

    /// Performs vector KNN search directly using sqlite-vec virtual table `vec_words`
    async fn fetch_knn_candidates(
        &self,
        target_vec: &[f32],
        poses: &[Pos],
        excludes: &[String],
        top_k: Option<usize>,
        min_similarity: Option<f32>,
    ) -> anyhow::Result<Vec<WordCandidate>> {
        let k = top_k.unwrap_or(64).max(32);
        // Expand search window if we have POS / exclude filters to prevent starvation
        let search_k = if !poses.is_empty() || !excludes.is_empty() {
            (k * 6).clamp(128, 1024)
        } else {
            k
        };

        let target_bytes = f32_vec_to_bytes(target_vec);

        let mut builder: QueryBuilder<Sqlite> = QueryBuilder::new(
            "SELECT w.word, w.weight, v.distance \
             FROM vec_words v \
             JOIN words w ON v.rowid = w.id \
             WHERE v.embedding MATCH ",
        );
        builder.push_bind(target_bytes);
        builder.push(" AND k = ");
        builder.push_bind(search_k as i64);

        if !poses.is_empty() {
            builder.push(" AND w.pos IN (");
            let mut separated = builder.separated(", ");
            for pos in poses {
                separated.push_bind(pos.as_str());
            }
            separated.push_unseparated(")");
        }

        if !excludes.is_empty() {
            builder.push(" AND w.word NOT IN (");
            let mut separated = builder.separated(", ");
            for word in excludes {
                separated.push_bind(word.to_lowercase());
            }
            separated.push_unseparated(")");
        }

        let rows: Vec<(String, f64, f64)> = builder.build_query_as().fetch_all(&self.pool).await?;

        let min_sim = min_similarity.unwrap_or(-1.0);

        let mut candidates: Vec<WordCandidate> = rows
            .into_iter()
            .filter_map(|(word, weight, distance)| {
                // Cosine distance = 1.0 - cosine_similarity
                let sim = 1.0 - distance as f32;
                if sim >= min_sim {
                    Some(WordCandidate { word, weight })
                } else {
                    None
                }
            })
            .collect();

        if let Some(limit) = top_k {
            candidates.truncate(limit);
        }

        Ok(candidates)
    }

    /// Fast non-vector candidate fetch for templates without similarity requirements
    async fn fetch_random_candidates(
        &self,
        poses: &[Pos],
        excludes: &[String],
        limit: usize,
    ) -> anyhow::Result<Vec<WordCandidate>> {
        let mut builder: QueryBuilder<Sqlite> =
            QueryBuilder::new("SELECT w.word, w.weight FROM words w ");

        let mut has_where = false;

        if !poses.is_empty() {
            builder.push("WHERE w.pos IN (");
            let mut separated = builder.separated(", ");
            for pos in poses {
                separated.push_bind(pos.as_str());
            }
            separated.push_unseparated(") ");
            has_where = true;
        }

        if !excludes.is_empty() {
            builder.push(if has_where {
                "AND w.word NOT IN ("
            } else {
                "WHERE w.word NOT IN ("
            });
            let mut separated = builder.separated(", ");
            for word in excludes {
                separated.push_bind(word.to_lowercase());
            }
            separated.push_unseparated(") ");
        }

        builder.push("ORDER BY RANDOM() * MAX(w.weight, 0.0001) DESC LIMIT ");
        builder.push_bind(limit as i64);

        let rows: Vec<(String, f64)> = builder.build_query_as().fetch_all(&self.pool).await?;

        Ok(rows
            .into_iter()
            .map(|(word, weight)| WordCandidate { word, weight })
            .collect())
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

/// Encodes a vector as base64 over its raw little-endian f32 bytes.
pub fn encode_embedding(v: &[f32]) -> String {
    let mut bytes = Vec::with_capacity(v.len() * 4);
    for f in v {
        bytes.extend_from_slice(&f.to_le_bytes());
    }
    STANDARD.encode(bytes)
}

/// Decodes a base64 string back into a `Vec<f32>`.
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

/// Encodes a vector as base64 over a quantized int8 representation.
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

/// Decodes a base64 string produced by `encode_embedding_q8` back into a `Vec<f32>`.
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
