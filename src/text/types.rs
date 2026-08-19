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
    pub fn similar_to(mut self, target_vec: impl Into<Vec<f32>>) -> Self {
        self.target_vec = Some(target_vec.into());
        self
    }

    /// Cap the candidate pool before weighted sampling
    pub fn top_k(mut self, k: usize) -> Self {
        self.top_k = Some(k);
        self
    }

    /// Require at least this cosine similarity
    pub fn min_similarity(mut self, min_sim: f32) -> Self {
        self.min_similarity = Some(min_sim);
        self
    }

    /// Use a literal word instead of sampling one (still harmonized in context)
    pub fn fixed(word: impl Into<String>) -> Self {
        Self {
            fixed_word: Some(word.into()),
            ..Self::default()
        }
    }

    /// Never pick these specific words for this slot
    pub fn exclude(mut self, words: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.exclude = Some(words.into_iter().map(Into::into).collect());
        self
    }

    pub fn similar_to_str(mut self, encoded: impl Into<String>) -> Self {
        self.target_vec_str = Some(encoded.into());
        self
    }

    pub fn similar_to_q8_str(mut self, encoded: impl Into<String>) -> Self {
        self.target_vec_q8_str = Some(encoded.into());
        self
    }
}
