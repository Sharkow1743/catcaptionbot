use super::{Case, Gender, Number, Pos, WordAnalysis};
use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};

#[derive(Clone)]
pub struct SqlMorpher {
    pool: SqlitePool,
}

impl SqlMorpher {
    pub async fn new(db_path: impl AsRef<str>) -> Result<Self, sqlx::Error> {
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(&format!("sqlite:{}", db_path.as_ref()))
            .await?;

        Ok(Self { pool })
    }

    pub async fn analyze_word(&self, word: impl AsRef<str>) -> WordAnalysis {
        let word_ref = word.as_ref();
        let w = word_ref.to_lowercase();

        let tag_row: Option<String> = sqlx::query_scalar(
            "SELECT tag FROM dictionary WHERE word = ?
             ORDER BY
               CASE
                 WHEN tag LIKE '%PREP%' THEN 1
                 WHEN tag LIKE '%CONJ%' THEN 1
                 WHEN tag LIKE '%PRCL%' THEN 1
                 WHEN tag LIKE '%ADVB%' THEN 2
                 WHEN tag LIKE '%NOUN%' THEN 3
                 WHEN tag LIKE '%VERB%' OR tag LIKE '%INFN%' THEN 4
                 WHEN tag LIKE '%ADJF%' OR tag LIKE '%PRTF%' THEN 5
                 ELSE 6
               END
             LIMIT 1",
        )
        .bind(&w)
        .fetch_optional(&self.pool)
        .await
        .unwrap_or(None);

        if let Some(tag) = tag_row {
            return Self::parse_opencorpora_tag(&w, &tag);
        }

        Self::analyze_by_heuristics(&w)
    }

    fn parse_opencorpora_tag(word: &str, tag: &str) -> WordAnalysis {
        let pos = if tag.contains("PREP") {
            Pos::Prep
        } else if tag.contains("ADVB") {
            Pos::Adv
        } else if tag.contains("NOUN") {
            Pos::Noun
        } else if tag.contains("ADJF") || tag.contains("ADJS") {
            Pos::Adj
        } else if tag.contains("PRTF") || tag.contains("PRTS") {
            Pos::Prt
        } else if tag.contains("GRND") {
            Pos::Grnd
        } else if tag.contains("VERB") || tag.contains("INFN") {
            Pos::Verb
        } else if tag.contains("COMP") {
            Pos::Comp
        } else if tag.contains("PRED") {
            Pos::Pred
        } else if tag.contains("NPRO") {
            Pos::Npro
        } else if tag.contains("NUMR") {
            Pos::Numr
        } else {
            Pos::Unknown
        };

        let gender = if tag.contains("femn") {
            Gender::Feminine
        } else if tag.contains("neut") {
            Gender::Neuter
        } else {
            Gender::Masculine
        };

        let number = if tag.contains("plur") {
            Number::Plural
        } else {
            Number::Singular
        };

        let case = if tag.contains("gent") || tag.contains("gen1") || tag.contains("gen2") {
            Case::Genitive
        } else if tag.contains("datv") {
            Case::Dative
        } else if tag.contains("accs") || tag.contains("acc1") || tag.contains("acc2") {
            Case::Accusative
        } else if tag.contains("ablt") {
            Case::Instrumental
        } else if tag.contains("loct") || tag.contains("loc1") || tag.contains("loc2") {
            Case::Locative
        } else {
            Case::Nominative
        };

        WordAnalysis {
            original: word.to_string(),
            pos,
            gender,
            number,
            case,
        }
    }

    fn analyze_by_heuristics(word: &str) -> WordAnalysis {
        let w = word.to_lowercase();
        let (pos, gender, number) = if w.ends_with("ый") || w.ends_with("ий") || w.ends_with("ой")
        {
            (Pos::Adj, Gender::Masculine, Number::Singular)
        } else if w.ends_with("ая") || w.ends_with("яя") {
            (Pos::Adj, Gender::Feminine, Number::Singular)
        } else if w.ends_with("ое") || w.ends_with("ее") {
            (Pos::Adj, Gender::Neuter, Number::Singular)
        } else if w.ends_with("ые") || w.ends_with("ие") {
            (Pos::Adj, Gender::Masculine, Number::Plural)
        } else if w.ends_with("ть") || w.ends_with("ти") || w.ends_with("ться") {
            (Pos::Verb, Gender::Masculine, Number::Singular)
        } else if w.ends_with('а') || w.ends_with('я') {
            (Pos::Noun, Gender::Feminine, Number::Singular)
        } else if w.ends_with('о')
            || w.ends_with('е')
            || w.ends_with('и')
            || w.ends_with('у')
            || w.ends_with('ю')
        {
            (Pos::Noun, Gender::Neuter, Number::Singular)
        } else if w.ends_with('ы') {
            (Pos::Noun, Gender::Masculine, Number::Plural)
        } else {
            (Pos::Noun, Gender::Masculine, Number::Singular)
        };

        WordAnalysis {
            original: word.to_string(),
            pos,
            gender,
            number,
            case: Case::Nominative,
        }
    }

    pub async fn harmonize<S: AsRef<str>>(&self, words: impl IntoIterator<Item = S>) -> String {
        let mut analyses = Vec::new();
        for w in words {
            analyses.push(self.analyze_word(w).await);
        }

        if analyses.is_empty() {
            return String::new();
        }

        let head_noun = analyses.iter().find(|a| a.pos == Pos::Noun).cloned();

        let mut result = Vec::new();

        for word in &analyses {
            match word.pos {
                Pos::Adj | Pos::Prt => {
                    if let Some(ref head) = head_noun {
                        let stem = Self::extract_stem(&word.original);
                        result.push(Self::inflect_adjective(&stem, head.gender, head.number));
                    } else {
                        result.push(word.original.clone());
                    }
                }
                Pos::Verb | Pos::Grnd => {
                    if let Some(ref head) = head_noun {
                        result.push(Self::inflect_verb(&word.original, head.number));
                    } else {
                        result.push(word.original.clone());
                    }
                }
                Pos::Noun => {
                    if word.case != Case::Nominative {
                        result.push(Self::normalize_noun(&word.original, word.case, word.gender));
                    } else {
                        result.push(word.original.clone());
                    }
                }
                _ => {
                    result.push(word.original.clone());
                }
            }
        }

        result.join(" ")
    }

    fn inflect_adjective(stem: &str, gender: Gender, number: Number) -> String {
        let last_char = stem.chars().last().unwrap_or('\0');
        let is_velar_or_sibilant = "гкхжчшщ".contains(last_char);

        if number == Number::Plural {
            if is_velar_or_sibilant {
                format!("{}ие", stem)
            } else {
                format!("{}ые", stem)
            }
        } else {
            match gender {
                Gender::Feminine => format!("{}ая", stem),
                Gender::Neuter => format!("{}ое", stem),
                Gender::Masculine => {
                    if is_velar_or_sibilant {
                        format!("{}ий", stem)
                    } else {
                        format!("{}ый", stem)
                    }
                }
            }
        }
    }

    fn inflect_verb(original: &str, number: Number) -> String {
        let w = original.to_lowercase();

        if number == Number::Plural {
            if w.ends_with("ются")
                || w.ends_with("ятся")
                || w.ends_with("ют")
                || w.ends_with("ят")
                || w.ends_with("ут")
                || w.ends_with("ат")
            {
                return w;
            }
            if w.ends_with("оваться") {
                format!("{}уются", w.trim_end_matches("оваться"))
            } else if w.ends_with("еваться") {
                format!("{}уются", w.trim_end_matches("еваться"))
            } else if w.ends_with("овать") {
                format!("{}уют", w.trim_end_matches("овать"))
            } else if w.ends_with("евать") {
                format!("{}уют", w.trim_end_matches("евать"))
            } else if w.ends_with("иться") {
                format!("{}ятся", w.trim_end_matches("иться"))
            } else if w.ends_with("ться") {
                format!("{}ются", w.trim_end_matches("ться"))
            } else if w.ends_with("ить") {
                format!("{}ят", w.trim_end_matches("ить"))
            } else if w.ends_with("ть") {
                format!("{}ют", w.trim_end_matches("ть"))
            } else if w.ends_with("ти") {
                format!("{}ут", w.trim_end_matches("ти"))
            } else {
                w
            }
        } else {
            if w.ends_with("ется")
                || w.ends_with("ится")
                || w.ends_with("уется")
                || w.ends_with("ет")
                || w.ends_with("ит")
                || w.ends_with("ует")
                || w.ends_with("ает")
                || w.ends_with("яет")
            {
                return w;
            }
            if w.ends_with("оваться") {
                format!("{}уется", w.trim_end_matches("оваться"))
            } else if w.ends_with("еваться") {
                format!("{}уется", w.trim_end_matches("еваться"))
            } else if w.ends_with("овать") {
                format!("{}ует", w.trim_end_matches("овать"))
            } else if w.ends_with("евать") {
                format!("{}ует", w.trim_end_matches("евать"))
            } else if w.ends_with("иться") {
                format!("{}ится", w.trim_end_matches("иться"))
            } else if w.ends_with("яться") {
                format!("{}ется", w.trim_end_matches("яться"))
            } else if w.ends_with("ться") {
                format!("{}ется", w.trim_end_matches("ться"))
            } else if w.ends_with("ить") {
                format!("{}ит", w.trim_end_matches("ить"))
            } else if w.ends_with("ть") {
                format!("{}ет", w.trim_end_matches("ть"))
            } else if w.ends_with("ти") {
                format!("{}ет", w.trim_end_matches("ти"))
            } else {
                w
            }
        }
    }

    fn normalize_noun(word: &str, case: Case, gender: Gender) -> String {
        if case == Case::Nominative {
            return word.to_string();
        }
        let w = word.to_lowercase();
        match gender {
            Gender::Masculine => {
                if w.ends_with("ом") || w.ends_with("ем") {
                    if w.len() >= 4 {
                        w[..w.len() - 4].to_string()
                    } else {
                        w
                    }
                } else if w.ends_with("е") || w.ends_with("а") || w.ends_with("у") {
                    if w.len() >= 2 {
                        w[..w.len() - 2].to_string()
                    } else {
                        w
                    }
                } else {
                    w
                }
            }
            Gender::Feminine => {
                if w.ends_with("е") || w.ends_with("у") || w.ends_with("ой") || w.ends_with("ей")
                {
                    let stem = Self::extract_stem(&w);
                    format!("{}а", stem)
                } else {
                    w
                }
            }
            _ => w,
        }
    }

    fn extract_stem(word: &str) -> String {
        let mut w = word.to_lowercase();

        if w.chars().count() <= 3 {
            return w;
        }

        if w.ends_with("ёр") {
            if let Some(pos) = w.rfind("ёр") {
                return format!("{}р", &w[..pos]);
            }
        } else if w.ends_with("ок") {
            if let Some(pos) = w.rfind("ок") {
                return format!("{}к", &w[..pos]);
            }
        }

        let endings = [
            "оваться",
            "еваться",
            "аться",
            "яться",
            "еться",
            "иться",
            "овать",
            "евать",
            "ивать",
            "ывать",
            "ать",
            "ять",
            "еть",
            "ить",
            "уть",
            "ти",
            "ть",
            "ая",
            "яя",
            "ое",
            "ее",
            "ый",
            "ий",
            "ой",
            "ые",
            "ие",
            "ами",
            "ями",
            "ам",
            "ям",
            "ах",
            "ях",
            "ов",
            "ев",
            "ей",
            "ом",
            "ем",
            "им",
            "ум",
            "а",
            "я",
            "о",
            "е",
            "у",
            "ю",
            "ы",
            "и",
        ];

        for ending in endings {
            if w.ends_with(ending) {
                let cut_len = w.len() - ending.len();
                if w[..cut_len].chars().count() >= 2 {
                    w.truncate(cut_len);
                    break;
                }
            }
        }
        w
    }
}
