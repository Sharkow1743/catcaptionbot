use super::{Pos, WordSpec};
use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
use rand::distr::Distribution;
use rand::distr::weighted::WeightedIndex;
use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};
use sqlx::{QueryBuilder, Result as SqlxResult, Sqlite};
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct RandomWordDb {
    pool: SqlitePool,
    embedder: Arc<Mutex<TextEmbedding>>,
}

struct WordCandidate {
    word: String,
    weight: f64,
    embedding_bytes: Option<Vec<u8>>,
}

impl RandomWordDb {
    /// Initializes DB Pool and FastEmbed embedder
    pub async fn new(db_path: &str) -> SqlxResult<Self> {
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(&format!("sqlite:{}", db_path))
            .await?;

        let init_options = InitOptions::new(EmbeddingModel::ParaphraseMLMiniLML12V2)
            .with_show_download_progress(true);

        let embedder = TextEmbedding::try_new(init_options)
            .map_err(|e| sqlx::Error::Protocol(format!("FastEmbed init error: {}", e)))?;

        Ok(Self {
            pool,
            embedder: Arc::new(Mutex::new(embedder)),
        })
    }

    // ========================================================================
    // HELPER FUNCTIONS FOR VECTORIZATION
    // ========================================================================

    /// Tries to find the vector for a word in the database table
    pub async fn find_vector_in_db(&self, word: impl AsRef<str>) -> SqlxResult<Option<Vec<f32>>> {
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
    /// Tries DB search first; if not found, computes embedding using FastEmbed.
    pub async fn get_or_create_embedding(&self, text: impl AsRef<str>) -> SqlxResult<Vec<f32>> {
        let text_ref = text.as_ref();
        let trimmed = text_ref.trim().to_lowercase();

        if let Ok(Some(vec)) = self.find_vector_in_db(&trimmed).await {
            return Ok(vec);
        }

        let mut embedder = self.embedder.lock().await;
        let embeddings = embedder
            .embed(vec![trimmed], None)
            .map_err(|e| sqlx::Error::Protocol(format!("Embedding error: {}", e)))?;

        embeddings
            .into_iter()
            .next()
            .ok_or_else(|| sqlx::Error::Protocol("Failed to compute embedding".into()))
    }

    // ========================================================================
    // MAIN PUBLIC QUERY FUNCTIONS
    // ========================================================================

    /// Get N weighted random words
    pub async fn get_random_words(&self, count: usize) -> SqlxResult<Vec<String>> {
        self.get_random_words_by_pos(&[], count).await
    }

    /// Get N weighted random words from specified POSes
    pub async fn get_random_words_by_pos(
        &self,
        poses: impl AsRef<[Pos]>,
        count: usize,
    ) -> SqlxResult<Vec<String>> {
        let poses_slice = poses.as_ref();
        if count == 0 {
            return Ok(vec![]);
        }

        let candidates = self.fetch_candidates(poses_slice, false).await?;
        Ok(weighted_sample_without_replacement(&candidates, count))
    }

    /// Get N random words close in meaning to target vector
    pub async fn get_random_words_by_similarity(
        &self,
        target_vec: impl AsRef<[f32]>,
        count: usize,
        top_k: Option<usize>,
    ) -> SqlxResult<Vec<String>> {
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
    ) -> SqlxResult<Vec<String>> {
        let poses_slice = poses.as_ref();
        let target_slice = target_vec.as_ref();

        if count == 0 {
            return Ok(vec![]);
        }

        let candidates = self.fetch_candidates(poses_slice, true).await?;
        let filtered = filter_and_rank_by_similarity(candidates, target_slice, top_k, None);

        Ok(weighted_sample_without_replacement(&filtered, count))
    }

    /// Get random words matching a template (e.g., `Vec<WordSpec>`)
    pub async fn get_random_words_by_template(
        &self,
        template: &[WordSpec],
    ) -> SqlxResult<Vec<String>> {
        let mut result = Vec::with_capacity(template.len());

        for spec in template {
            let poses = spec.poses.as_deref().unwrap_or(&[]);
            let requires_vectors = spec.target_vec.is_some();
            let candidates = self.fetch_candidates(poses, requires_vectors).await?;

            let final_candidates = if let Some(ref target_vec) = spec.target_vec {
                filter_and_rank_by_similarity(
                    candidates,
                    target_vec,
                    spec.top_k,
                    spec.min_similarity,
                )
            } else {
                candidates
            };

            let sampled = weighted_sample_without_replacement(&final_candidates, 1);
            if let Some(word) = sampled.into_iter().next() {
                result.push(word);
            }
        }

        Ok(result)
    }

    // ========================================================================
    // PRIVATE SQL HELPERS
    // ========================================================================

    async fn fetch_candidates(
        &self,
        poses: &[Pos],
        include_embeddings: bool,
    ) -> SqlxResult<Vec<WordCandidate>> {
        let mut builder: QueryBuilder<Sqlite> = QueryBuilder::new("SELECT w.word, w.weight, ");

        if include_embeddings {
            builder.push("v.embedding FROM words w JOIN vec_words v ON w.id = v.rowid ");
        } else {
            builder.push("NULL as embedding FROM words w ");
        }

        if !poses.is_empty() {
            builder.push("WHERE w.pos IN (");
            let mut separated = builder.separated(", ");
            for pos in poses {
                separated.push_bind(pos.as_str());
            }
            separated.push_unseparated(") ");
        }

        let query = builder.build_query_as::<(String, f64, Option<Vec<u8>>)>();
        let rows = query.fetch_all(&self.pool).await?;

        Ok(rows
            .into_iter()
            .map(|(word, weight, embedding_bytes)| WordCandidate {
                word,
                weight,
                embedding_bytes,
            })
            .collect())
    }
}

// ============================================================================
// VECTOR MATH & SAMPLING UTILITIES
// ============================================================================

fn bytes_to_f32_vec(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|chunk| f32::from_le_bytes(chunk.try_into().unwrap()))
        .collect()
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let mut dot = 0.0;
    let mut norm_a = 0.0;
    let mut norm_b = 0.0;

    for (x, y) in a.iter().zip(b) {
        dot += x * y;
        norm_a += x * x;
        norm_b += y * y;
    }

    if norm_a == 0.0 || norm_b == 0.0 {
        0.0
    } else {
        dot / (norm_a.sqrt() * norm_b.sqrt())
    }
}

fn filter_and_rank_by_similarity(
    candidates: Vec<WordCandidate>,
    target_vec: &[f32],
    top_k: Option<usize>,
    min_similarity: Option<f32>,
) -> Vec<WordCandidate> {
    let min_sim = min_similarity.unwrap_or(-1.0);

    let mut scored: Vec<(f32, WordCandidate)> = candidates
        .into_iter()
        .filter_map(|cand| {
            if let Some(ref bytes) = cand.embedding_bytes {
                let vec = bytes_to_f32_vec(bytes);
                let sim = cosine_similarity(target_vec, &vec);
                if sim >= min_sim {
                    Some((sim, cand))
                } else {
                    None
                }
            } else {
                None
            }
        })
        .collect();

    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

    if let Some(k) = top_k {
        scored.truncate(k);
    }

    scored.into_iter().map(|(_, cand)| cand).collect()
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
