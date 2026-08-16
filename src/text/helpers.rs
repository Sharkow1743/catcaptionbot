use super::generator::WordGenerator;
use super::morph::Morpher;
use super::types::{Pos, WordSpec};

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

/// Runs a template and returns the raw sampled words without grammatical harmonization —
/// useful for debugging a template or feeding words into custom logic.
pub async fn generate_words(
    generator: &WordGenerator,
    template: impl AsRef<[WordSpec]>,
) -> anyhow::Result<Vec<String>> {
    generator
        .get_random_words_by_template(template.as_ref())
        .await
}

/// Harmonizes an arbitrary, already-known list of words (skips generation entirely).
/// Handy for mixing user-supplied words with generated ones downstream.
pub async fn harmonize_words(
    morpher: &Morpher,
    words: impl IntoIterator<Item = impl AsRef<str>>,
) -> String {
    let owned: Vec<String> = words.into_iter().map(|w| w.as_ref().to_string()).collect();
    morpher.harmonize(&owned).await
}

/// Computes or fetches vector embedding for `target_text`, picks `count` similar words matching `poses`,
/// and grammatically harmonizes them.
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

/// Same as `generate_harmonized_phrase_by_similarity` but caps the candidate pool to `top_k`
/// closest matches before weighted sampling — gives tighter thematic control.
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

/// Tries a template, and falls back to `fallback` if generation yields no words at all
/// (e.g. an over-constrained template with no DB matches).
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

// ============================================================================
// UNIFIED COMPOSER STRUCT
// ============================================================================

/// High-level wrapper combining `Morpher` and `WordGenerator`
/// for executing combined text workflows conveniently.
#[derive(Clone)]
pub struct TextComposer {
    pub morpher: Morpher,
    pub generator: WordGenerator,
}

impl TextComposer {
    pub fn new(morpher: Morpher, generator: WordGenerator) -> Self {
        Self { morpher, generator }
    }

    /// Generates words matching the given template specifications and harmonizes them grammatically.
    pub async fn generate_and_harmonize(
        &self,
        template: impl AsRef<[WordSpec]>,
    ) -> anyhow::Result<String> {
        generate_and_harmonize(&self.generator, &self.morpher, template).await
    }

    /// Same, with a custom word separator in the output.
    pub async fn generate_and_harmonize_with_separator(
        &self,
        template: impl AsRef<[WordSpec]>,
        separator: impl AsRef<str>,
    ) -> anyhow::Result<String> {
        generate_and_harmonize_with_separator(&self.generator, &self.morpher, template, separator)
            .await
    }

    /// Raw sampled words for a template, no harmonization.
    pub async fn generate_words(
        &self,
        template: impl AsRef<[WordSpec]>,
    ) -> anyhow::Result<Vec<String>> {
        generate_words(&self.generator, template).await
    }

    /// Harmonize a caller-supplied list of words directly.
    pub async fn harmonize_words(
        &self,
        words: impl IntoIterator<Item = impl AsRef<str>>,
    ) -> String {
        harmonize_words(&self.morpher, words).await
    }

    /// Generates N semantically similar words relative to `target_text` and harmonizes them.
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
    pub async fn generate_many_phrases(
        &self,
        template: impl AsRef<[WordSpec]>,
        n: usize,
    ) -> anyhow::Result<Vec<String>> {
        generate_many_phrases(&self.generator, &self.morpher, template, n).await
    }

    /// Template with a fallback string if nothing matched.
    pub async fn generate_and_harmonize_or(
        &self,
        template: impl AsRef<[WordSpec]>,
        fallback: impl AsRef<str>,
    ) -> String {
        generate_and_harmonize_or(&self.generator, &self.morpher, template, fallback).await
    }
}
