// ============================================================================
// SHARED GRAMMATICAL ENUMS & DATA STRUCTURES
// ============================================================================

/// Unified Part-of-Speech (POS) categories across Morph analysis and Word Generator queries
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Pos {
    Noun,    // "NOUN"
    Adj,     // "ADJ" / "ADJF" / "ADJS"
    Verb,    // "VERB" / "INFN"
    Prt,     // "PRT" / "PRTF" / "PRTS"
    Grnd,    // "GRND"
    Adv,     // "ADV" / "ADVB"
    Comp,    // "COMP"
    Pred,    // "PRED"
    Npro,    // "NPRO"
    Numr,    // "NUMR"
    Prep,    // "PREP"
    Misc,    // "MISC" / "CONJ" / "PRCL" / "INTJ"
    Unknown, // "UNKNOWN"
}

impl Pos {
    #[allow(dead_code)]
    pub fn as_str(&self) -> &'static str {
        match self {
            Pos::Noun => "NOUN",
            Pos::Adj => "ADJ",
            Pos::Verb => "VERB",
            Pos::Prt => "PRT",
            Pos::Grnd => "GRND",
            Pos::Adv => "ADV",
            Pos::Comp => "COMP",
            Pos::Pred => "PRED",
            Pos::Npro => "NPRO",
            Pos::Numr => "NUMR",
            Pos::Prep => "PREP",
            Pos::Misc => "MISC",
            Pos::Unknown => "UNKNOWN",
        }
    }

    pub fn to_opencorpora_tag(&self) -> &'static str {
        match self {
            Pos::Noun => "NOUN",
            Pos::Adj => "ADJF",
            Pos::Verb => "VERB",
            Pos::Prt => "PRTF",
            Pos::Grnd => "GRND",
            Pos::Adv => "ADVB",
            Pos::Comp => "COMP",
            Pos::Pred => "PRED",
            Pos::Npro => "NPRO",
            Pos::Numr => "NUMR",
            Pos::Prep => "PREP",
            Pos::Misc | Pos::Unknown => "NOUN",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gender {
    Masculine,
    Feminine,
    Neuter,
}

impl Gender {
    pub fn as_grammeme(&self) -> &'static str {
        match self {
            Gender::Masculine => "masc",
            Gender::Feminine => "femn",
            Gender::Neuter => "neut",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Number {
    Singular,
    Plural,
}

impl Number {
    #[allow(dead_code)]
    pub fn as_grammeme(&self) -> &'static str {
        match self {
            Number::Singular => "sing",
            Number::Plural => "plur",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Case {
    Nominative,
    Genitive,
    Dative,
    Accusative,
    Instrumental,
    Locative,
}

impl Case {
    pub fn as_grammeme(&self) -> &'static str {
        match self {
            Case::Nominative => "nomn",
            Case::Genitive => "gent",
            Case::Dative => "datv",
            Case::Accusative => "accs",
            Case::Instrumental => "ablt",
            Case::Locative => "loct",
        }
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct WordAnalysis {
    pub original: String,
    pub lemma: String,
    pub pos: Pos,
    pub gender: Gender,
    pub number: Number,
    pub case: Case,
    pub tag: String,
}

/// Template specification for generating a single random word
#[derive(Debug, Clone, Default)]
pub struct WordSpec {
    /// Restrict to these POS categories (if empty or None, any POS allowed)
    pub poses: Option<Vec<Pos>>,
    /// Optional target vector (phrase or word embedding)
    pub target_vec: Option<Vec<f32>>,
    pub target_vec_str: Option<String>,
    pub target_vec_q8_str: Option<String>,
    /// Limit candidate pool to top K closest words by vector similarity before weighted random choice
    pub top_k: Option<usize>,
    /// Minimum cosine similarity threshold (e.g. 0.3)
    pub min_similarity: Option<f32>,
    /// If set, skips random selection entirely and uses this literal word for the slot
    /// (still gets grammatically harmonized downstream).
    pub fixed_word: Option<String>,
    /// Extra words to exclude from candidate selection for this slot,
    /// on top of any words already chosen earlier in the same template.
    pub exclude: Option<Vec<String>>,
}

impl WordSpec {
    pub fn new() -> Self {
        Self::default()
    }

    /// Restrict this slot to the given POS categories
    pub fn poses(mut self, poses: impl Into<Vec<Pos>>) -> Self {
        self.poses = Some(poses.into());
        self
    }

    /// Rank candidates by similarity to a precomputed vector
    #[allow(dead_code)]
    pub fn similar_to(mut self, target_vec: impl Into<Vec<f32>>) -> Self {
        self.target_vec = Some(target_vec.into());
        self
    }

    /// Cap the candidate pool before weighted sampling
    #[allow(dead_code)]
    pub fn top_k(mut self, k: usize) -> Self {
        self.top_k = Some(k);
        self
    }

    /// Require at least this cosine similarity
    #[allow(dead_code)]
    pub fn min_similarity(mut self, min_sim: f32) -> Self {
        self.min_similarity = Some(min_sim);
        self
    }

    /// Use a literal word instead of sampling one (still harmonized in context)
    #[allow(dead_code)]
    pub fn fixed(word: impl Into<String>) -> Self {
        Self {
            fixed_word: Some(word.into()),
            ..Self::default()
        }
    }

    /// Never pick these specific words for this slot
    #[allow(dead_code)]
    pub fn exclude(mut self, words: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.exclude = Some(words.into_iter().map(Into::into).collect());
        self
    }

    #[allow(dead_code)]
    pub fn similar_to_str(mut self, encoded: impl Into<String>) -> Self {
        self.target_vec_str = Some(encoded.into());
        self
    }

    pub fn similar_to_q8_str(mut self, encoded: impl Into<String>) -> Self {
        self.target_vec_q8_str = Some(encoded.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_grammemes_and_tags() {
        assert_eq!(Pos::Noun.to_opencorpora_tag(), "NOUN");
        assert_eq!(Pos::Adj.to_opencorpora_tag(), "ADJF");
        assert_eq!(Pos::Verb.to_opencorpora_tag(), "VERB");

        assert_eq!(Gender::Masculine.as_grammeme(), "masc");
        assert_eq!(Gender::Feminine.as_grammeme(), "femn");
        assert_eq!(Gender::Neuter.as_grammeme(), "neut");

        assert_eq!(Number::Singular.as_grammeme(), "sing");
        assert_eq!(Number::Plural.as_grammeme(), "plur");

        assert_eq!(Case::Nominative.as_grammeme(), "nomn");
        assert_eq!(Case::Genitive.as_grammeme(), "gent");
        assert_eq!(Case::Dative.as_grammeme(), "datv");
    }

    #[test]
    fn test_word_spec_builder() {
        let spec = WordSpec::new()
            .poses(&[Pos::Adj, Pos::Noun])
            .top_k(10)
            .min_similarity(0.5)
            .exclude(vec!["cat", "dog"]);

        assert_eq!(spec.poses.unwrap(), vec![Pos::Adj, Pos::Noun]);
        assert_eq!(spec.top_k, Some(10));
        assert_eq!(spec.min_similarity, Some(0.5));
        assert_eq!(
            spec.exclude.unwrap(),
            vec!["cat".to_string(), "dog".to_string()]
        );
    }
}
