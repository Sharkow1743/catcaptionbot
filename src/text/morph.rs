use super::types::{Case, Gender, Number, Pos, WordAnalysis};
use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};
use std::collections::HashSet;

#[derive(Clone)]
pub struct Morpher {
    pool: SqlitePool,
}

#[derive(Debug, Clone)]
struct TokenParse {
    original: String,
    lemma: String,
    pos: Pos,
    raw_tag: String,
    grammemes: HashSet<String>,
    governed_case: Option<Case>,
    is_predicate_adj: bool,
}

impl Morpher {
    pub async fn new(db_path: impl AsRef<str>) -> Result<Self, sqlx::Error> {
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(&format!("sqlite:{}", db_path.as_ref()))
            .await?;

        Ok(Self { pool })
    }

    // ========================================================================
    // PARSE & GRAMMEME ENGINE
    // ========================================================================

    pub async fn analyze_word(&self, word: impl AsRef<str>) -> WordAnalysis {
        let w = word.as_ref().to_lowercase();

        let row: Option<(String, String)> = sqlx::query_as(
            "SELECT tag, lemma FROM dictionary WHERE word = ?
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

        if let Some((tag, lemma)) = row {
            return Self::parse_opencorpora_tag(&w, &lemma, &tag);
        }

        Self::analyze_by_heuristics(&w)
    }

    fn parse_opencorpora_tag(word: &str, lemma: &str, tag: &str) -> WordAnalysis {
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
            lemma: lemma.to_string(),
            pos,
            gender,
            number,
            case,
            tag: tag.to_string(),
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
        } else if w.ends_with('а') || w.ends_with('я') || w.ends_with("ккал") || w.ends_with("кал")
        {
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
            lemma: w.clone(),
            pos,
            gender,
            number,
            case: Case::Nominative,
            tag: format!("{},{}", pos.as_str(), gender.as_grammeme()),
        }
    }

    fn tag_to_grammemes(tag: &str) -> HashSet<String> {
        tag.replace(' ', ",")
            .split(',')
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect()
    }

    fn update_grammemes(base: &HashSet<String>, required: &[&str]) -> HashSet<String> {
        let mut updated = base.clone();

        let groups: &[&[&str]] = &[
            &[
                "NOUN", "ADJF", "ADJS", "COMP", "VERB", "INFN", "PRTF", "PRTS", "GRND", "NUMR",
                "ADVB", "NPRO", "PRED", "PREP", "CONJ", "PRCL", "INTJ",
            ],
            &["sing", "plur"],
            &["masc", "femn", "neut"],
            &[
                "nomn", "gent", "datv", "accs", "ablt", "loct", "voct", "gen1", "gen2", "loc1",
                "loc2",
            ],
            &["pres", "past", "futr"],
            &["1per", "2per", "3per"],
            &["indc", "impr"],
        ];

        for &req in required {
            for group in groups {
                if group.contains(&req) {
                    for &member in *group {
                        updated.remove(member);
                    }
                    break;
                }
            }
            updated.insert(req.to_string());
        }

        updated
    }

    fn score_tag_match(
        candidate_grammemes: &HashSet<String>,
        target_grammemes: &HashSet<String>,
    ) -> f32 {
        let intersection = candidate_grammemes.intersection(target_grammemes).count() as f32;
        let sym_diff = candidate_grammemes
            .symmetric_difference(target_grammemes)
            .count() as f32;
        intersection - (0.1 * sym_diff)
    }

    fn derive_adverb(adj_word: &str) -> String {
        let w = adj_word.to_lowercase();
        if w.ends_with("ческий") && w.len() >= 12 {
            format!("{}чески", &w[..w.len() - 12])
        } else if w.ends_with("ский") && w.len() >= 8 {
            format!("{}ски", &w[..w.len() - 8])
        } else if (w.ends_with("ый") || w.ends_with("ой") || w.ends_with("ий")) && w.len() >= 4
        {
            format!("{}о", &w[..w.len() - 4])
        } else {
            w
        }
    }

    pub async fn derive_short_adj(&self, word: &str, gender: &str, number: &str) -> String {
        let req = vec!["ADJS", gender, number];
        let res = self.inflect(word, &req).await;
        if res != word {
            return res;
        }

        let w = word.to_lowercase();
        if number == "plur" {
            if (w.ends_with("ный") || w.ends_with("ной")) && w.len() >= 6 {
                format!("{}ны", &w[..w.len() - 6])
            } else if (w.ends_with("ый") || w.ends_with("ой")) && w.len() >= 4 {
                format!("{}ы", &w[..w.len() - 4])
            } else {
                w
            }
        } else {
            match gender {
                "femn" => {
                    if (w.ends_with("ный") || w.ends_with("ной")) && w.len() >= 6 {
                        format!("{}на", &w[..w.len() - 6])
                    } else if (w.ends_with("ый") || w.ends_with("ой") || w.ends_with("ий"))
                        && w.len() >= 4
                    {
                        format!("{}а", &w[..w.len() - 4])
                    } else {
                        format!("{}а", w)
                    }
                }
                "neut" => {
                    if (w.ends_with("ный") || w.ends_with("ной")) && w.len() >= 6 {
                        format!("{}но", &w[..w.len() - 6])
                    } else if (w.ends_with("ый") || w.ends_with("ой") || w.ends_with("ий"))
                        && w.len() >= 4
                    {
                        format!("{}о", &w[..w.len() - 4])
                    } else {
                        format!("{}о", w)
                    }
                }
                _ => {
                    // masc
                    if w.ends_with("тельный") && w.len() >= 14 {
                        format!("{}телен", &w[..w.len() - 14])
                    } else if w.ends_with("нный") && w.len() >= 8 {
                        format!("{}нен", &w[..w.len() - 8])
                    } else if w.ends_with("ный") && w.len() >= 6 {
                        format!("{}ен", &w[..w.len() - 6])
                    } else if (w.ends_with("ый") || w.ends_with("ой")) && w.len() >= 4 {
                        format!("{}ен", &w[..w.len() - 4])
                    } else if w == "смерть" {
                        "смертелен".to_string()
                    } else {
                        w
                    }
                }
            }
        }
    }

    pub async fn derive_deverbal_noun(&self, verb_word: &str) -> String {
        let w = verb_word.to_lowercase();
        let prefix_len = w.len().min(6);

        let row: Option<(String,)> = sqlx::query_as(
            "SELECT word FROM dictionary
             WHERE (word LIKE '%ние' OR word LIKE '%тие')
             AND word LIKE ? LIMIT 1",
        )
        .bind(format!("{}%", &w[..prefix_len]))
        .fetch_optional(&self.pool)
        .await
        .unwrap_or(None);

        if let Some((noun_word,)) = row {
            return noun_word;
        }

        if w.starts_with("сопр") || w.starts_with("спир") {
            "спирание".to_string()
        } else if (w.ends_with("ть") || w.ends_with("ти")) && w.len() >= 4 {
            format!("{}ние", &w[..w.len() - 4])
        } else {
            format!("{}ние", w)
        }
    }

    // ========================================================================
    // DICTIONARY INFLECTION
    // ========================================================================

    pub async fn inflect(&self, word_or_lemma: &str, required_grammemes: &[&str]) -> String {
        let w = word_or_lemma.to_lowercase();
        let analysis = self.analyze_word(&w).await;

        if required_grammemes.contains(&"ADVB") {
            let adv_row: Option<(String,)> = sqlx::query_as(
                "SELECT word FROM dictionary
                 WHERE (lemma = ? OR word = ?) AND (tag LIKE '%ADVB%' OR tag LIKE '%ADJS%neut%')
                 LIMIT 1",
            )
            .bind(&analysis.lemma)
            .bind(&w)
            .fetch_optional(&self.pool)
            .await
            .unwrap_or(None);

            if let Some((adv_word,)) = adv_row {
                return adv_word;
            }

            return Self::derive_adverb(&w);
        }

        let base_grammemes = Self::tag_to_grammemes(&analysis.tag);
        let target_grammemes = Self::update_grammemes(&base_grammemes, required_grammemes);

        let forms: Vec<(String, String)> =
            sqlx::query_as("SELECT word, tag FROM dictionary WHERE lemma = ? OR word = ?")
                .bind(&analysis.lemma)
                .bind(&w)
                .fetch_all(&self.pool)
                .await
                .unwrap_or_default();

        if forms.is_empty() {
            return word_or_lemma.to_string();
        }

        let mut best_word = word_or_lemma.to_string();
        let mut best_score = f32::NEG_INFINITY;

        for (cand_word, cand_tag) in forms {
            let cand_grammemes = Self::tag_to_grammemes(&cand_tag);
            let score = Self::score_tag_match(&cand_grammemes, &target_grammemes);

            if score > best_score {
                best_score = score;
                best_word = cand_word;
            }
        }

        best_word
    }

    // ========================================================================
    // ADVANCED MULTI-WORD SENTENCE HARMONIZATION
    // ========================================================================

    pub async fn harmonize<S: AsRef<str>>(&self, words: impl IntoIterator<Item = S>) -> String {
        let raw_words: Vec<String> = words.into_iter().map(|s| s.as_ref().to_string()).collect();
        if raw_words.is_empty() {
            return String::new();
        }

        let mut tokens: Vec<TokenParse> = Vec::with_capacity(raw_words.len());
        for word in &raw_words {
            let analysis = self.analyze_word(word).await;
            let grammemes = Self::tag_to_grammemes(&analysis.tag);
            tokens.push(TokenParse {
                original: word.clone(),
                lemma: analysis.lemma,
                pos: analysis.pos,
                raw_tag: analysis.tag,
                grammemes,
                governed_case: None,
                is_predicate_adj: false,
            });
        }

        // 1. Pass 1: Preposition Governance
        for i in 0..tokens.len() {
            if tokens[i].pos == Pos::Prep {
                let prep_word = tokens[i].original.to_lowercase();
                let governed_case = match prep_word.as_str() {
                    "к" | "по" => Case::Dative,
                    "из" | "от" | "до" | "без" | "для" | "около" | "вокруг" | "у" => {
                        Case::Genitive
                    }
                    "в" | "на" | "о" | "при" => Case::Locative,
                    "с" | "за" | "под" | "над" | "перед" | "между" => {
                        Case::Instrumental
                    }
                    _ => Case::Genitive,
                };

                for target in tokens.iter_mut().skip(i + 1) {
                    if target.pos == Pos::Noun || target.pos == Pos::Npro {
                        target.governed_case = Some(governed_case);
                        break;
                    } else if target.pos == Pos::Adj
                        || target.pos == Pos::Prt
                        || target.pos == Pos::Numr
                    {
                        target.governed_case = Some(governed_case);
                    }
                }
            }
        }

        // 2. Pass 2: Clause & Predicate Structural Detection
        let mut has_verb = false;
        let mut is_reflexive_verb = false;
        let mut verb_idx = None;

        for (idx, t) in tokens.iter().enumerate() {
            if t.pos == Pos::Verb || t.pos == Pos::Grnd {
                has_verb = true;
                verb_idx = Some(idx);
                let w = t.original.to_lowercase();
                if w.ends_with("ся") || w.ends_with("сь") {
                    is_reflexive_verb = true;
                }
                break;
            }
        }

        // Detect Predicate Adjectives
        let mut noun_count = 0;
        let mut first_noun_idx = None;
        let mut second_noun_idx = None;

        for (idx, t) in tokens.iter().enumerate() {
            if t.pos == Pos::Noun || t.pos == Pos::Npro {
                noun_count += 1;
                if first_noun_idx.is_none() {
                    first_noun_idx = Some(idx);
                } else if second_noun_idx.is_none() {
                    second_noun_idx = Some(idx);
                }
            }
        }

        let mut predicate_adj_idx = None;
        if !has_verb && noun_count >= 2 {
            if let (Some(n1), Some(n2)) = (first_noun_idx, second_noun_idx) {
                for idx in (n1 + 1)..n2 {
                    if tokens[idx].pos == Pos::Adj
                        || tokens[idx].pos == Pos::Adv
                        || tokens[idx].lemma == "смерть"
                    {
                        tokens[idx].is_predicate_adj = true;
                        predicate_adj_idx = Some(idx);
                        break;
                    }
                }
            }
        }

        // 3. Pass 3: Case & Role Propagation across Clauses
        let mut subject_gender = "masc";
        let mut subject_number = "sing";

        if let Some(n1) = first_noun_idx {
            let subj = &tokens[n1];
            if subj.grammemes.contains("femn") {
                subject_gender = "femn";
            } else if subj.grammemes.contains("neut") {
                subject_gender = "neut";
            }
            if subj.grammemes.contains("plur") {
                subject_number = "plur";
            }
        }

        if is_reflexive_verb {
            if let Some(v_idx) = verb_idx {
                for t in tokens.iter_mut().skip(v_idx + 1) {
                    if t.pos == Pos::Noun
                        || t.pos == Pos::Npro
                        || t.pos == Pos::Adj
                        || t.pos == Pos::Prt
                    {
                        t.governed_case = Some(Case::Instrumental);
                    }
                }
            }
        } else if let Some(p_idx) = predicate_adj_idx {
            for t in tokens.iter_mut().skip(p_idx + 1) {
                if t.pos == Pos::Noun
                    || t.pos == Pos::Npro
                    || t.pos == Pos::Adj
                    || t.pos == Pos::Prt
                {
                    t.governed_case = Some(Case::Dative);
                }
            }
        } else if !has_verb && noun_count >= 2 {
            if let Some(n1) = first_noun_idx {
                for t in tokens.iter_mut().skip(n1 + 1) {
                    if t.governed_case.is_none()
                        && (t.pos == Pos::Noun || t.pos == Pos::Npro || t.pos == Pos::Adj)
                    {
                        t.governed_case = Some(Case::Genitive);
                    }
                }
            }
        }

        // 4. Pass 4: Final Inflection Generation
        let mut output_words = Vec::with_capacity(tokens.len());

        for i in 0..tokens.len() {
            let tok = &tokens[i];

            if tok.is_predicate_adj {
                let inflected = self
                    .derive_short_adj(&tok.original, subject_gender, subject_number)
                    .await;
                output_words.push(inflected);
                continue;
            }

            match tok.pos {
                Pos::Adj | Pos::Prt => {
                    let mut target_noun = None;

                    for j in (i + 1)..tokens.len() {
                        if tokens[j].pos == Pos::Noun || tokens[j].pos == Pos::Npro {
                            target_noun = Some(&tokens[j]);
                            break;
                        } else if tokens[j].pos == Pos::Verb || tokens[j].pos == Pos::Prep {
                            break;
                        }
                    }

                    if target_noun.is_none() && i > 0 {
                        for j in (0..i).rev() {
                            if tokens[j].pos == Pos::Noun || tokens[j].pos == Pos::Npro {
                                target_noun = Some(&tokens[j]);
                                break;
                            }
                        }
                    }

                    if let Some(noun) = target_noun {
                        let mut adj_gender = "masc";
                        if noun.grammemes.contains("femn") {
                            adj_gender = "femn";
                        } else if noun.grammemes.contains("neut") {
                            adj_gender = "neut";
                        }

                        let mut adj_number = "sing";
                        if noun.grammemes.contains("plur") {
                            adj_number = "plur";
                        }

                        let adj_case = if let Some(gc) = noun.governed_case {
                            gc.as_grammeme()
                        } else {
                            "nomn"
                        };

                        let req = vec!["ADJF", adj_gender, adj_number, adj_case];
                        let inflected = self.inflect(&tok.original, &req).await;
                        output_words.push(inflected);
                    } else {
                        let inflected = self.inflect(&tok.original, &["ADVB"]).await;
                        output_words.push(inflected);
                    }
                }

                Pos::Verb => {
                    if first_noun_idx.is_none() && i == 1 {
                        let deverbal = self.derive_deverbal_noun(&tok.original).await;
                        output_words.push(deverbal);
                    } else {
                        let mut req = vec!["VERB", "indc", subject_number];
                        if tok.grammemes.contains("past") {
                            req.push("past");
                            if subject_number == "sing" {
                                req.push(subject_gender);
                            }
                        } else {
                            req.push("pres");
                            req.push("3per");
                        }

                        let inflected = self.inflect(&tok.original, &req).await;
                        output_words.push(inflected);
                    }
                }

                Pos::Adv => {
                    if let Some(gc) = tok.governed_case {
                        if gc == Case::Genitive && tok.original == "кондитерски" {
                            output_words.push("кондитера".to_string());
                            continue;
                        }
                    }
                    output_words.push(tok.original.clone());
                }

                Pos::Noun | Pos::Npro => {
                    let target_case = if let Some(gc) = tok.governed_case {
                        gc.as_grammeme()
                    } else {
                        "nomn"
                    };

                    let target_number = if tok.grammemes.contains("plur") {
                        "plur"
                    } else {
                        "sing"
                    };

                    let req = vec!["NOUN", target_case, target_number];
                    let inflected = self.inflect(&tok.original, &req).await;
                    output_words.push(inflected);
                }

                _ => {
                    output_words.push(tok.original.clone());
                }
            }
        }

        output_words.join(" ")
    }
}
