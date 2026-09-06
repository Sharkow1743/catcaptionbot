use super::generator::WordGenerator;
use super::morph::Morpher;
use super::quotes::{Quote, QuoteBank};
use super::types::{Pos, WordSpec};
use std::path::Path;

// ============================================================================
// STANDALONE ORCHESTRATION HELPERS
// ============================================================================

/// Generates words matching a list of `WordSpec` templates and harmonizes them into a single string phrase.
pub async fn generate_and_harmonize(
    generator: &WordGenerator,
    morpher: &Morpher,
    template: impl AsRef<[WordSpec]>,
) -> anyhow::Result<String> {
    let words = generator
        .get_random_words_by_template(template.as_ref())
        .await?;
    let harmonized = morpher.harmonize(&words).await;
    Ok(harmonized)
}

/// Like `generate_and_harmonize`, but joins the final words with a custom separator
/// instead of morpher's default single-space join.
#[allow(dead_code)]
pub async fn generate_and_harmonize_with_separator(
    generator: &WordGenerator,
    morpher: &Morpher,
    template: impl AsRef<[WordSpec]>,
    separator: impl AsRef<str>,
) -> anyhow::Result<String> {
    let words = generator
        .get_random_words_by_template(template.as_ref())
        .await?;
    let harmonized = morpher.harmonize(&words).await;
    Ok(harmonized.replace(' ', separator.as_ref()))
}

/// Runs a template and returns the raw sampled words without grammatical harmonization.
#[allow(dead_code)]
pub async fn generate_words(
    generator: &WordGenerator,
    template: impl AsRef<[WordSpec]>,
) -> anyhow::Result<Vec<String>> {
    generator
        .get_random_words_by_template(template.as_ref())
        .await
}

/// Harmonizes an arbitrary, already-known list of words (skips generation entirely).
#[allow(dead_code)]
pub async fn harmonize_words(
    morpher: &Morpher,
    words: impl IntoIterator<Item = impl AsRef<str>>,
) -> String {
    let owned: Vec<String> = words.into_iter().map(|w| w.as_ref().to_string()).collect();
    morpher.harmonize(&owned).await
}

/// Computes or fetches vector embedding for `target_text`, picks `count` similar words matching `poses`,
/// and grammatically harmonizes them.
#[allow(dead_code)]
pub async fn generate_harmonized_phrase_by_similarity(
    generator: &WordGenerator,
    morpher: &Morpher,
    poses: impl AsRef<[Pos]>,
    target_text: impl AsRef<str>,
    count: usize,
) -> anyhow::Result<String> {
    let target_vec = generator
        .get_or_create_embedding(target_text.as_ref())
        .await?;
    let words = generator
        .get_random_words_by_pos_and_similarity(poses.as_ref(), &target_vec, count, None)
        .await?;
    let harmonized = morpher.harmonize(&words).await;
    Ok(harmonized)
}

/// Same as `generate_harmonized_phrase_by_similarity` but caps candidate pool to `top_k` matches.
#[allow(dead_code)]
pub async fn generate_harmonized_phrase_by_similarity_top_k(
    generator: &WordGenerator,
    morpher: &Morpher,
    poses: impl AsRef<[Pos]>,
    target_text: impl AsRef<str>,
    count: usize,
    top_k: usize,
) -> anyhow::Result<String> {
    let target_vec = generator
        .get_or_create_embedding(target_text.as_ref())
        .await?;
    let words = generator
        .get_random_words_by_pos_and_similarity(poses.as_ref(), &target_vec, count, Some(top_k))
        .await?;
    let harmonized = morpher.harmonize(&words).await;
    Ok(harmonized)
}

/// Runs the same template `n` times, producing `n` independent harmonized phrases.
#[allow(dead_code)]
pub async fn generate_many_phrases(
    generator: &WordGenerator,
    morpher: &Morpher,
    template: impl AsRef<[WordSpec]>,
    n: usize,
) -> anyhow::Result<Vec<String>> {
    let template = template.as_ref();
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        let words = generator.get_random_words_by_template(template).await?;
        out.push(morpher.harmonize(&words).await);
    }
    Ok(out)
}

/// Tries a template and falls back to `fallback` if generation yields no words.
#[allow(dead_code)]
pub async fn generate_and_harmonize_or(
    generator: &WordGenerator,
    morpher: &Morpher,
    template: impl AsRef<[WordSpec]>,
    fallback: impl AsRef<str>,
) -> String {
    let words = generator
        .get_random_words_by_template(template.as_ref())
        .await;
    if let Ok(words) = words {
        return morpher.harmonize(words).await;
    }
    fallback.as_ref().to_string()
}

/// Generates a harmonized phrase and finds a matching quote from `QuoteBank`.
#[allow(dead_code)]
pub async fn generate_and_find_quote(
    generator: &WordGenerator,
    morpher: &Morpher,
    quotes: &QuoteBank,
    template: impl AsRef<[WordSpec]>,
) -> anyhow::Result<(String, Option<Quote>)> {
    let phrase = generate_and_harmonize(generator, morpher, template).await?;
    let quote = quotes.random_quote_containing(&phrase).await?;
    Ok((phrase, quote))
}

// ============================================================================
// UNIFIED COMPOSER STRUCT
// ============================================================================

/// High-level wrapper combining `Morpher`, `WordGenerator`, and `QuoteBank`
/// for executing unified generation, inflection, and quote lookup workflows.
#[derive(Clone)]
pub struct TextComposer {
    pub morpher: Morpher,
    pub generator: WordGenerator,
    pub quotes: QuoteBank,
}

impl TextComposer {
    /// Creates a composer from pre-existing instances of Morpher, WordGenerator, and QuoteBank.
    pub fn new(morpher: Morpher, generator: WordGenerator, quotes: QuoteBank) -> Self {
        Self {
            morpher,
            generator,
            quotes,
        }
    }

    /// Initializes all three components from individual database paths (using default model path "robert-tiny2").
    pub async fn from_dbs(
        dict_db_path: &str,
        words_db_path: &str,
        quotes_db_path: &str,
    ) -> anyhow::Result<Self> {
        let morpher = Morpher::new(dict_db_path).await?;
        let generator = WordGenerator::new(words_db_path).await?;
        let quotes = QuoteBank::new(quotes_db_path).await?;
        Ok(Self::new(morpher, generator, quotes))
    }

    /// Initializes all components with separate DB paths and a custom embedding model directory.
    pub async fn from_dbs_with_model(
        dict_db_path: &str,
        words_db_path: &str,
        quotes_db_path: &str,
        model_dir: impl AsRef<Path>,
    ) -> anyhow::Result<Self> {
        let morpher = Morpher::new(dict_db_path).await?;
        let generator = WordGenerator::new_with_local_model(words_db_path, model_dir).await?;
        let quotes = QuoteBank::new(quotes_db_path).await?;
        Ok(Self::new(morpher, generator, quotes))
    }

    /// Initializes all three engines from a single consolidated SQLite database.
    pub async fn from_single_db(db_path: &str) -> anyhow::Result<Self> {
        Self::from_dbs(db_path, db_path, db_path).await
    }

    /// Initializes all three engines from a single SQLite database with a custom embedding model.
    pub async fn from_single_db_with_model(
        db_path: &str,
        model_dir: impl AsRef<Path>,
    ) -> anyhow::Result<Self> {
        Self::from_dbs_with_model(db_path, db_path, db_path, model_dir).await
    }

    // ========================================================================
    // GENERATION & MORPHOLOGY
    // ========================================================================

    /// Generates words matching template specifications and harmonizes them grammatically.
    pub async fn generate_and_harmonize(
        &self,
        template: impl AsRef<[WordSpec]>,
    ) -> anyhow::Result<String> {
        generate_and_harmonize(&self.generator, &self.morpher, template).await
    }

    /// Same, with a custom separator between words.
    #[allow(dead_code)]
    pub async fn generate_and_harmonize_with_separator(
        &self,
        template: impl AsRef<[WordSpec]>,
        separator: impl AsRef<str>,
    ) -> anyhow::Result<String> {
        generate_and_harmonize_with_separator(&self.generator, &self.morpher, template, separator)
            .await
    }

    /// Raw sampled words for a template without grammatical inflection.
    #[allow(dead_code)]
    pub async fn generate_words(
        &self,
        template: impl AsRef<[WordSpec]>,
    ) -> anyhow::Result<Vec<String>> {
        generate_words(&self.generator, template).await
    }

    /// Harmonize a caller-supplied list of words directly.
    #[allow(dead_code)]
    pub async fn harmonize_words(
        &self,
        words: impl IntoIterator<Item = impl AsRef<str>>,
    ) -> String {
        harmonize_words(&self.morpher, words).await
    }

    /// Generates N semantically similar words relative to `target_text` and harmonizes them.
    #[allow(dead_code)]
    pub async fn generate_harmonized_phrase_by_similarity(
        &self,
        poses: impl AsRef<[Pos]>,
        target_text: impl AsRef<str>,
        count: usize,
    ) -> anyhow::Result<String> {
        generate_harmonized_phrase_by_similarity(
            &self.generator,
            &self.morpher,
            poses,
            target_text,
            count,
        )
        .await
    }

    /// Same, capped to the top-K closest candidates before sampling.
    #[allow(dead_code)]
    pub async fn generate_harmonized_phrase_by_similarity_top_k(
        &self,
        poses: impl AsRef<[Pos]>,
        target_text: impl AsRef<str>,
        count: usize,
        top_k: usize,
    ) -> anyhow::Result<String> {
        generate_harmonized_phrase_by_similarity_top_k(
            &self.generator,
            &self.morpher,
            poses,
            target_text,
            count,
            top_k,
        )
        .await
    }

    /// Produce `n` independent phrases from the same template.
    #[allow(dead_code)]
    pub async fn generate_many_phrases(
        &self,
        template: impl AsRef<[WordSpec]>,
        n: usize,
    ) -> anyhow::Result<Vec<String>> {
        generate_many_phrases(&self.generator, &self.morpher, template, n).await
    }

    /// Template with fallback string if generation produces nothing.
    #[allow(dead_code)]
    pub async fn generate_and_harmonize_or(
        &self,
        template: impl AsRef<[WordSpec]>,
        fallback: impl AsRef<str>,
    ) -> String {
        generate_and_harmonize_or(&self.generator, &self.morpher, template, fallback).await
    }

    // ========================================================================
    // QUOTE BANK OPERATIONS
    // ========================================================================

    /// Total number of quotes in the quote bank.
    #[allow(dead_code)]
    pub async fn quote_count(&self) -> anyhow::Result<i64> {
        self.quotes.count().await
    }

    /// Returns every quote in the database.
    #[allow(dead_code)]
    pub async fn all_quotes(&self) -> anyhow::Result<Vec<Quote>> {
        self.quotes.all_quotes().await
    }

    /// Returns `n` random quotes from the quote bank.
    #[allow(dead_code)]
    pub async fn random_quotes(&self, n: usize) -> anyhow::Result<Vec<Quote>> {
        self.quotes.random_quotes(n).await
    }

    /// Returns up to `n` quotes matching the given search query.
    #[allow(dead_code)]
    pub async fn random_quotes_containing(
        &self,
        query: &str,
        n: usize,
    ) -> anyhow::Result<Vec<Quote>> {
        self.quotes.random_quotes_containing(query, n).await
    }

    /// Returns a single quote matching the search query, if any.
    #[allow(dead_code)]
    pub async fn random_quote_containing(&self, query: &str) -> anyhow::Result<Option<Quote>> {
        self.quotes.random_quote_containing(query).await
    }

    /// Generates a harmonized phrase from the template and finds a matching quote for it.
    #[allow(dead_code)]
    pub async fn generate_phrase_with_quote(
        &self,
        template: impl AsRef<[WordSpec]>,
    ) -> anyhow::Result<(String, Option<Quote>)> {
        generate_and_find_quote(&self.generator, &self.morpher, &self.quotes, template).await
    }
}
