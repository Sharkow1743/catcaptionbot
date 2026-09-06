use rand::distr::Distribution;
use rand::distr::weighted::WeightedIndex;
use sqlx::Row;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use std::str::FromStr;

// ============================================================================
// QUOTE BANK — random/weighted-by-match Russian sentence lookup
// backed by sentences.db (built by scripts/build_quotes_db.py from
// rus_sentences.tsv, the Tatoeba Russian sentence export).
//
// Schema expected in sentences.db:
//   CREATE TABLE quotes(id INTEGER PRIMARY KEY, text TEXT NOT NULL);
//   CREATE VIRTUAL TABLE quotes_fts USING fts5(text, content='quotes', content_rowid='id', tokenize='unicode61');
// ============================================================================

#[derive(Debug, Clone)]
pub struct Quote {
    pub id: i64,
    pub text: String,
    /// Relative match strength for this query (higher = better match).
    /// 0.0 for quotes returned with no query / as pure random fallback.
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

    /// Returns every quote in the database. Careful with memory on large corpora.
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

    /// Returns `n` completely random quotes, no query involved.
    pub async fn random_quotes(&self, n: usize) -> anyhow::Result<Vec<Quote>> {
        if n == 0 {
            return Ok(vec![]);
        }
        let rows: Vec<(i64, String)> =
            sqlx::query_as("SELECT id, text FROM quotes ORDER BY RANDOM() LIMIT ?")
                .bind(n as i64)
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

    /// Returns up to `n` quotes containing (fuzzily) `query`, weighted-sampled
    /// by match quality — i.e. better matches are more likely to be picked,
    /// but it's not a strict top-N, similar to how translator UIs surface
    /// varied example sentences rather than always the single best hit.
    ///
    /// Falls back progressively if nothing scores well:
    ///   1. FTS5 match (BM25-ranked) on the query terms
    ///   2. plain substring LIKE match
    ///   3. pure random quotes (so this never returns empty on a non-empty DB)
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

    /// Convenience: single weighted-random match, or `None` if the DB is empty.
    pub async fn random_quote_containing(&self, query: &str) -> anyhow::Result<Option<Quote>> {
        Ok(self
            .random_quotes_containing(query, 1)
            .await?
            .into_iter()
            .next())
    }

    // ------------------------------------------------------------------
    // internals
    // ------------------------------------------------------------------

    async fn fts_candidates(&self, q: &str, limit: usize) -> anyhow::Result<Vec<Quote>> {
        // Try as an exact phrase first (best signal), then fall back to an
        // OR-of-prefixes match so partial/typo'd input still finds things.
        let phrase_expr = format!("\"{}\"", sanitize_fts_term(q));
        if let Some(rows) = self.run_fts(&phrase_expr, limit).await? {
            if !rows.is_empty() {
                return Ok(rows);
            }
        }

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
            Err(_) => return Ok(None), // malformed FTS query syntax etc. — fall through
        };

        let mut quotes = Vec::with_capacity(rows.len());
        for row in rows {
            let id: i64 = row.try_get("id")?;
            let text: String = row.try_get("text")?;
            // bm25() in sqlite is negative & lower-is-better; flip and shift
            // so higher score = better match, and keep it positive for weighting.
            let rank: f64 = row.try_get("rank").unwrap_or(0.0);
            let score = (-rank).max(0.0) + 0.01;
            quotes.push(Quote { id, text, score });
        }
        Ok(Some(quotes))
    }

    async fn like_candidates(&self, q: &str, limit: usize) -> anyhow::Result<Vec<Quote>> {
        let pattern = format!("%{}%", q.replace('%', "").replace('_', ""));
        let rows: Vec<(i64, String)> = sqlx::query_as(
            "SELECT id, text FROM quotes WHERE text LIKE ? ORDER BY RANDOM() LIMIT ?",
        )
        .bind(&pattern)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|(id, text)| Quote {
                id,
                text,
                score: 1.0, // flat weight — LIKE gives no ranking signal
            })
            .collect())
    }
}

/// Strips characters that would break FTS5 query syntax (quotes, wildcards
/// used mid-token, etc.) from a single search term.
fn sanitize_fts_term(term: &str) -> String {
    term.chars()
        .filter(|c| !matches!(c, '"' | '*' | '(' | ')' | ':' | '^'))
        .collect()
}

/// Weighted-samples up to `n` quotes without replacement, biased toward
/// higher `score`. Falls back to uniform weight if all scores are zero.
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
