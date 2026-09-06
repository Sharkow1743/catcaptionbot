use rand::RngExt;
use rand::distr::Distribution;
use rand::distr::weighted::WeightedIndex;
use sqlx::Row;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use std::str::FromStr;

#[derive(Debug, Clone)]
pub struct Quote {
    pub id: i64,
    pub text: String,
    pub score: f64,
}

#[derive(Clone)]
pub struct QuoteBank {
    pool: SqlitePool,
}

impl QuoteBank {
    pub async fn new(db_path: impl AsRef<str>) -> anyhow::Result<Self> {
        let opts = SqliteConnectOptions::from_str(&format!("sqlite:{}", db_path.as_ref()))?
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
            .synchronous(sqlx::sqlite::SqliteSynchronous::Normal)
            .pragma("mmap_size", "268435456")
            .pragma("cache_size", "-64000")
            .pragma("temp_store", "MEMORY");

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(opts)
            .await?;

        Ok(Self { pool })
    }

    pub async fn count(&self) -> anyhow::Result<i64> {
        let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM quotes")
            .fetch_one(&self.pool)
            .await?;
        Ok(row.0)
    }

    pub async fn all_quotes(&self) -> anyhow::Result<Vec<Quote>> {
        let rows: Vec<(i64, String)> = sqlx::query_as("SELECT id, text FROM quotes")
            .fetch_all(&self.pool)
            .await?;
        Ok(rows
            .into_iter()
            .map(|(id, text)| Quote {
                id,
                text,
                score: 0.0,
            })
            .collect())
    }

    /// Fast indexed random selection avoiding ORDER BY RANDOM() on full table
    pub async fn random_quotes(&self, n: usize) -> anyhow::Result<Vec<Quote>> {
        if n == 0 {
            return Ok(vec![]);
        }

        let max_id: Option<(i64,)> = sqlx::query_as("SELECT MAX(id) FROM quotes")
            .fetch_optional(&self.pool)
            .await?;

        let max_id = match max_id.and_then(|r| Some(r.0)) {
            Some(m) if m > 0 => m,
            _ => return Ok(vec![]),
        };

        // Generate all random IDs upfront so `ThreadRng` (!Send) is dropped before `.await`
        let random_ids: Vec<i64> = {
            let mut rng = rand::rng();
            (0..n).map(|_| rng.random_range(1..=max_id)).collect()
        };

        let mut results = Vec::with_capacity(n);

        // Fetch using random ID points (fast index lookup per item)
        for random_id in random_ids {
            let row: Option<(i64, String)> =
                sqlx::query_as("SELECT id, text FROM quotes WHERE id >= ? ORDER BY id ASC LIMIT 1")
                    .bind(random_id)
                    .fetch_optional(&self.pool)
                    .await?;

            if let Some((id, text)) = row {
                results.push(Quote {
                    id,
                    text,
                    score: 0.0,
                });
            }
        }

        Ok(results)
    }

    pub async fn random_quotes_containing(
        &self,
        query: &str,
        n: usize,
    ) -> anyhow::Result<Vec<Quote>> {
        if n == 0 {
            return Ok(vec![]);
        }
        let q = query.trim();
        if q.is_empty() {
            return self.random_quotes(n).await;
        }

        let mut candidates = self.fts_candidates(q, n.max(20) * 4).await?;

        if candidates.is_empty() {
            candidates = self.like_candidates(q, n.max(20) * 4).await?;
        }

        if candidates.is_empty() {
            return self.random_quotes(n).await;
        }

        Ok(weighted_sample(candidates, n))
    }

    pub async fn random_quote_containing(&self, query: &str) -> anyhow::Result<Option<Quote>> {
        Ok(self
            .random_quotes_containing(query, 1)
            .await?
            .into_iter()
            .next())
    }

    // ------------------------------------------------------------------
    // Internals
    // ------------------------------------------------------------------

    async fn fts_candidates(&self, q: &str, limit: usize) -> anyhow::Result<Vec<Quote>> {
        // Try as exact phrase first
        let phrase_expr = format!("\"{}\"", sanitize_fts_term(q));
        if let Some(rows) = self.run_fts(&phrase_expr, limit).await? {
            if !rows.is_empty() {
                return Ok(rows);
            }
        }

        // Fall back to prefix matching (e.g. "прив*") which handles partial matches
        let or_expr = q
            .split_whitespace()
            .map(sanitize_fts_term)
            .filter(|t| !t.is_empty())
            .map(|t| format!("{}*", t))
            .collect::<Vec<_>>()
            .join(" OR ");

        if or_expr.is_empty() {
            return Ok(vec![]);
        }

        Ok(self.run_fts(&or_expr, limit).await?.unwrap_or_default())
    }

    async fn run_fts(&self, match_expr: &str, limit: usize) -> anyhow::Result<Option<Vec<Quote>>> {
        let rows = sqlx::query(
            "SELECT q.id AS id, q.text AS text, bm25(quotes_fts) AS rank
             FROM quotes_fts
             JOIN quotes q ON q.id = quotes_fts.rowid
             WHERE quotes_fts MATCH ?
             ORDER BY rank
             LIMIT ?",
        )
        .bind(match_expr)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await;

        let rows = match rows {
            Ok(r) => r,
            Err(_) => return Ok(None),
        };

        let mut quotes = Vec::with_capacity(rows.len());
        for row in rows {
            let id: i64 = row.try_get("id")?;
            let text: String = row.try_get("text")?;
            let rank: f64 = row.try_get("rank").unwrap_or(0.0);
            let score = (-rank).max(0.0) + 0.01;
            quotes.push(Quote { id, text, score });
        }
        Ok(Some(quotes))
    }

    async fn like_candidates(&self, q: &str, limit: usize) -> anyhow::Result<Vec<Quote>> {
        // Step 1: Get count of matches without ordering
        let pattern = format!("%{}%", q.replace('%', "").replace('_', ""));
        let count_row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM quotes WHERE text LIKE ?")
            .bind(&pattern)
            .fetch_one(&self.pool)
            .await?;

        let match_count = count_row.0;
        if match_count == 0 {
            return Ok(vec![]);
        }

        // Step 2: Sample offsets upfront so `ThreadRng` (!Send) is dropped before `.await`
        let fetch_count = (limit as i64).min(match_count);
        let offsets: Vec<i64> = {
            let mut rng = rand::rng();
            (0..fetch_count)
                .map(|_| rng.random_range(0..match_count))
                .collect()
        };

        let mut quotes = Vec::with_capacity(fetch_count as usize);

        for offset in offsets {
            let row: Option<(i64, String)> =
                sqlx::query_as("SELECT id, text FROM quotes WHERE text LIKE ? LIMIT 1 OFFSET ?")
                    .bind(&pattern)
                    .bind(offset)
                    .fetch_optional(&self.pool)
                    .await?;

            if let Some((id, text)) = row {
                quotes.push(Quote {
                    id,
                    text,
                    score: 1.0,
                });
            }
        }

        Ok(quotes)
    }
}

fn sanitize_fts_term(term: &str) -> String {
    term.chars()
        .filter(|c| !matches!(c, '"' | '*' | '(' | ')' | ':' | '^'))
        .collect()
}

fn weighted_sample(mut candidates: Vec<Quote>, n: usize) -> Vec<Quote> {
    if candidates.is_empty() || n == 0 {
        return vec![];
    }
    if candidates.len() <= n {
        return candidates;
    }

    let mut rng = rand::rng();
    let mut result = Vec::with_capacity(n);

    for _ in 0..n {
        if candidates.is_empty() {
            break;
        }
        let weights: Vec<f64> = candidates
            .iter()
            .map(|q| if q.score > 0.0 { q.score } else { 0.001 })
            .collect();

        match WeightedIndex::new(&weights) {
            Ok(dist) => {
                let idx = dist.sample(&mut rng);
                result.push(candidates.remove(idx));
            }
            Err(_) => break,
        }
    }

    result
}
