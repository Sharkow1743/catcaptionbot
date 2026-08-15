pub mod morph;
pub mod random;

pub use morph::SqlMorpher;
pub use random::RandomWordDb;

// ============================================================================
// SHARED GRAMMATICAL ENUMS & DATA STRUCTURES
// ============================================================================

/// Unified Part-of-Speech (POS) categories across Morph analysis and Random DB queries
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Pos {
    Noun,    // "NOUN"
    Adj,     // "ADJ" (Adjective)
    Verb,    // "VERB"
    Prt,     // "PRT" (Participle / Particle)
    Grnd,    // "GRND" (Gerund)
    Adv,     // "ADV" (Adverb)
    Comp,    // "COMP" (Comparative)
    Pred,    // "PRED" (Predicate)
    Npro,    // "NPRO" (Pronoun)
    Numr,    // "NUMR" (Numeral)
    Prep,    // "PREP" (Preposition)
    Misc,    // "MISC"
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gender {
    Masculine,
    Feminine,
    Neuter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Number {
    Singular,
    Plural,
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

#[derive(Debug, Clone)]
pub struct WordAnalysis {
    pub original: String,
    pub pos: Pos,
    pub gender: Gender,
    pub number: Number,
    pub case: Case,
}

/// Template specification for generating a single random word
#[derive(Debug, Clone, Default)]
pub struct WordSpec {
    /// Restrict to these POS categories (if empty or None, any POS allowed)
    pub poses: Option<Vec<Pos>>,
    /// Optional target vector (phrase or word embedding)
    pub target_vec: Option<Vec<f32>>,
    /// Limit candidate pool to top K closest words by vector similarity before weighted random choice
    pub top_k: Option<usize>,
    /// Minimum cosine similarity threshold (e.g. 0.3)
    pub min_similarity: Option<f32>,
}
