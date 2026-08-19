use super::types::{Case, Gender, Number, Pos, WordAnalysis};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use sqlx::{QueryBuilder, Sqlite};
use std::collections::{HashMap, HashSet};
use std::str::FromStr;
use std::sync::{Arc, RwLock};

#[derive(Clone)]
pub struct Morpher {
    pool: SqlitePool,
    analysis_cache: Arc<RwLock<HashMap<String, WordAnalysis>>>,
    forms_cache: Arc<RwLock<HashMap<String, Vec<(String, String)>>>>,
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
        let connection_options =
            SqliteConnectOptions::from_str(&format!("sqlite:{}", db_path.as_ref()))?
                .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
                .synchronous(sqlx::sqlite::SqliteSynchronous::Normal)
                .pragma("mmap_size", "268435456")
                .pragma("cache_size", "-64000")
                .pragma("temp_store", "MEMORY");

        let pool = SqlitePoolOptions::new()
            .max_connections(10)
            .connect_with(connection_options)
            .await?;

        Ok(Self {
            pool,
            analysis_cache: Arc::new(RwLock::new(HashMap::with_capacity(4096))),
            forms_cache: Arc::new(RwLock::new(HashMap::with_capacity(4096))),
        })
    }

    // ========================================================================
    // BATCH & CACHED ANALYSIS ENGINE
    // ========================================================================

    pub async fn analyze_words_batch(&self, words: &[String]) -> Vec<WordAnalysis> {
        let mut results = vec![None; words.len()];
        let mut missing_indices = Vec::new();
        let mut missing_words = Vec::new();

        if let Ok(cache) = self.analysis_cache.read() {
            for (i, word) in words.iter().enumerate() {
                let w = word.trim().to_lowercase();
                if let Some(analysis) = cache.get(&w) {
                    results[i] = Some(analysis.clone());
                } else {
                    missing_indices.push(i);
                    missing_words.push(w);
                }
            }
        }

        if !missing_words.is_empty() {
            let mut builder: QueryBuilder<Sqlite> =
                QueryBuilder::new("SELECT word, tag, lemma FROM dictionary WHERE word IN (");
            {
                let mut sep = builder.separated(", ");
                for w in &missing_words {
                    sep.push_bind(w);
                }
            }
            builder.push(
                ") ORDER BY CASE \
                 WHEN tag LIKE '%ADJF%' OR tag LIKE '%ADJS%' THEN 1 \
                 WHEN tag LIKE '%NOUN%' THEN 2 \
                 WHEN tag LIKE '%VERB%' OR tag LIKE '%INFN%' THEN 3 \
                 WHEN tag LIKE '%PREP%' THEN 4 \
                 WHEN tag LIKE '%ADVB%' THEN 5 \
                 ELSE 6 END",
            );

            let rows: Vec<(String, String, String)> = builder
                .build_query_as()
                .fetch_all(&self.pool)
                .await
                .unwrap_or_default();

            let mut fetched_map: HashMap<String, (String, String)> = HashMap::new();
            for (w, tag, lemma) in rows {
                fetched_map.entry(w).or_insert((tag, lemma));
            }

            let mut write_cache = self.analysis_cache.write().ok();

            for (idx, w) in missing_indices.into_iter().zip(missing_words.into_iter()) {
                let analysis = if let Some((tag, lemma)) = fetched_map.get(&w) {
                    Self::parse_opencorpora_tag(&w, lemma, tag)
                } else {
                    Self::analyze_by_heuristics(&w)
                };

                if let Some(ref mut c) = write_cache {
                    c.insert(w, analysis.clone());
                }

                results[idx] = Some(analysis);
            }
        }

        results.into_iter().map(|opt| opt.unwrap()).collect()
    }

    pub async fn analyze_word(&self, word: impl AsRef<str>) -> WordAnalysis {
        let w = word.as_ref().trim().to_lowercase();
        self.analyze_words_batch(&[w])
            .await
            .into_iter()
            .next()
            .unwrap()
    }

    fn parse_opencorpora_tag(word: &str, lemma: &str, tag: &str) -> WordAnalysis {
        let pos = if tag.contains("PREP") {
            Pos::Prep
        } else if tag.contains("ADJF") || tag.contains("ADJS") {
            Pos::Adj
        } else if tag.contains("ADVB") {
            Pos::Adv
        } else if tag.contains("NOUN") {
            Pos::Noun
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
        } else if w.ends_with('а') || w.ends_with('я') {
            (Pos::Noun, Gender::Feminine, Number::Singular)
        } else if w.ends_with("ие")
            || w.ends_with("ье")
            || w.ends_with('о')
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
            tag: format!(
                "{},{},sing,nomn",
                pos.to_opencorpora_tag(),
                gender.as_grammeme()
            ),
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

    fn score_tag_match(candidate: &HashSet<String>, target: &HashSet<String>) -> f32 {
        let intersection = candidate.intersection(target).count() as f32;
        let sym_diff = candidate.symmetric_difference(target).count() as f32;
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

    pub fn derive_deverbal_noun_pure(verb_word: &str) -> String {
        let mut s = verb_word.to_lowercase();
        if s.ends_with("ся") || s.ends_with("сь") {
            let len = s.len();
            if len >= 4 {
                s.truncate(len - 4);
            }
        }

        if (s.ends_with("ить") || s.ends_with("еть")) && s.len() >= 6 {
            format!("{}ение", &s[..s.len() - 6])
        } else if (s.ends_with("ать") || s.ends_with("ять")) && s.len() >= 6 {
            format!("{}ание", &s[..s.len() - 6])
        } else if (s.ends_with("ти") || s.ends_with("ть")) && s.len() >= 4 {
            format!("{}тие", &s[..s.len() - 4])
        } else {
            format!("{}ние", s)
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

        let forms: Vec<(String, String)> = {
            let cached = if let Ok(cache) = self.forms_cache.read() {
                cache.get(&analysis.lemma).cloned()
            } else {
                None
            };

            if let Some(f) = cached {
                f
            } else {
                let db_forms: Vec<(String, String)> =
                    sqlx::query_as("SELECT word, tag FROM dictionary WHERE lemma = ? OR word = ?")
                        .bind(&analysis.lemma)
                        .bind(&w)
                        .fetch_all(&self.pool)
                        .await
                        .unwrap_or_default();

                if let Ok(mut cache) = self.forms_cache.write() {
                    cache.insert(analysis.lemma.clone(), db_forms.clone());
                }
                db_forms
            }
        };

        if forms.is_empty() {
            return word_or_lemma.to_string();
        }

        let base_grammemes = Self::tag_to_grammemes(&analysis.tag);
        let target_grammemes = Self::update_grammemes(&base_grammemes, required_grammemes);

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

        // Single batch fetch for all token analyses
        let analyses = self.analyze_words_batch(&raw_words).await;

        let mut tokens: Vec<TokenParse> = raw_words
            .iter()
            .zip(analyses)
            .map(|(word, analysis)| {
                let grammemes = Self::tag_to_grammemes(&analysis.tag);
                TokenParse {
                    original: word.clone(),
                    lemma: analysis.lemma,
                    pos: analysis.pos,
                    raw_tag: analysis.tag,
                    grammemes,
                    governed_case: None,
                    is_predicate_adj: false,
                }
            })
            .collect();

        // 1. Pass 1: Early Promotion of Deverbal Nouns for Adj + Verb pairs
        for i in 1..tokens.len() {
            if tokens[i].pos == Pos::Verb && tokens[i - 1].pos == Pos::Adj {
                let derived = Self::derive_deverbal_noun_pure(&tokens[i].original);
                tokens[i].original = derived.clone();
                tokens[i].lemma = derived;
                tokens[i].pos = Pos::Noun;
                tokens[i].raw_tag = "NOUN,neut,sing,nomn".to_string();
                tokens[i].grammemes = Self::tag_to_grammemes("NOUN,neut,sing,nomn");
            }
        }

        // 2. Pass 2: Preposition Governance
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

        // 3. Pass 3: Clause & Predicate Structural Detection
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
                    if tokens[idx].pos == Pos::Adj || tokens[idx].lemma == "смерть" {
                        tokens[idx].is_predicate_adj = true;
                        predicate_adj_idx = Some(idx);
                        break;
                    }
                }
            }
        }

        // 4. Pass 4: Subject Agreement Setup
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

        // 5. Pass 5: Build Jobs
        let mut jobs: Vec<InflectionJob> = Vec::with_capacity(tokens.len());

        for i in 0..tokens.len() {
            let tok = &tokens[i];

            if tok.is_predicate_adj {
                jobs.push(InflectionJob::ShortAdj {
                    word: tok.original.clone(),
                    gender: subject_gender,
                    number: subject_number,
                });
                continue;
            }

            let job = match tok.pos {
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
                        let adj_gender = if noun.grammemes.contains("femn") {
                            "femn"
                        } else if noun.grammemes.contains("neut") {
                            "neut"
                        } else {
                            "masc"
                        };
                        let adj_number = if noun.grammemes.contains("plur") {
                            "plur"
                        } else {
                            "sing"
                        };
                        let adj_case = noun
                            .governed_case
                            .map(|c| c.as_grammeme())
                            .unwrap_or("nomn");

                        InflectionJob::Adjective {
                            word: tok.original.clone(),
                            req: vec!["ADJF", adj_gender, adj_number, adj_case],
                        }
                    } else if i + 1 < tokens.len()
                        && (tokens[i + 1].pos == Pos::Adj || tokens[i + 1].pos == Pos::Prt)
                    {
                        InflectionJob::AdjectiveAsAdverb {
                            word: tok.original.clone(),
                        }
                    } else {
                        InflectionJob::Adjective {
                            word: tok.original.clone(),
                            req: vec!["ADJF", "masc", "sing", "nomn"],
                        }
                    }
                }

                Pos::Adv => {
                    let mut target_noun = None;
                    if i + 1 < tokens.len()
                        && (tokens[i + 1].pos == Pos::Noun || tokens[i + 1].pos == Pos::Npro)
                    {
                        target_noun = Some(&tokens[i + 1]);
                    }

                    if let Some(noun) = target_noun {
                        let adj_gender = if noun.grammemes.contains("femn") {
                            "femn"
                        } else if noun.grammemes.contains("neut") {
                            "neut"
                        } else {
                            "masc"
                        };
                        let adj_number = if noun.grammemes.contains("plur") {
                            "plur"
                        } else {
                            "sing"
                        };
                        let adj_case = noun
                            .governed_case
                            .map(|c| c.as_grammeme())
                            .unwrap_or("nomn");

                        InflectionJob::Adjective {
                            word: tok.original.clone(),
                            req: vec!["ADJF", adj_gender, adj_number, adj_case],
                        }
                    } else if i == tokens.len() - 1
                        && i > 0
                        && (tokens[i - 1].pos == Pos::Adv || tokens[i - 1].pos == Pos::Adj)
                    {
                        InflectionJob::Adjective {
                            word: tok.original.clone(),
                            req: vec!["ADJF", "masc", "sing", "nomn"],
                        }
                    } else if tok.governed_case == Some(Case::Genitive)
                        && tok.original == "кондитерски"
                    {
                        InflectionJob::AdverbGoverned {
                            word: tok.original.clone(),
                        }
                    } else {
                        InflectionJob::Literal(tok.original.clone())
                    }
                }

                Pos::Verb => {
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
                    InflectionJob::Verb {
                        word: tok.original.clone(),
                        req,
                    }
                }

                Pos::Noun | Pos::Npro => {
                    let target_case = tok.governed_case.map(|c| c.as_grammeme()).unwrap_or("nomn");
                    let target_number = if tok.grammemes.contains("plur") {
                        "plur"
                    } else {
                        "sing"
                    };
                    InflectionJob::Noun {
                        word: tok.original.clone(),
                        req: vec!["NOUN", target_case, target_number],
                    }
                }

                _ => InflectionJob::Literal(tok.original.clone()),
            };

            jobs.push(job);
        }

        let mut output_words: Vec<String> = Vec::with_capacity(jobs.len());
        for job in jobs {
            let res = match job {
                InflectionJob::ShortAdj {
                    word,
                    gender,
                    number,
                } => self.derive_short_adj(&word, gender, number).await,
                InflectionJob::Adjective { word, req } => self.inflect(&word, &req).await,
                InflectionJob::AdjectiveAsAdverb { word } => self.inflect(&word, &["ADVB"]).await,
                InflectionJob::Verb { word, req } => self.inflect(&word, &req).await,
                InflectionJob::AdverbGoverned { word: _ } => "кондитера".to_string(),
                InflectionJob::Literal(w) => w,
                InflectionJob::Noun { word, req } => self.inflect(&word, &req).await,
            };
            output_words.push(res);
        }

        output_words.join(" ")
    }
}

enum InflectionJob {
    ShortAdj {
        word: String,
        gender: &'static str,
        number: &'static str,
    },
    Adjective {
        word: String,
        req: Vec<&'static str>,
    },
    AdjectiveAsAdverb {
        word: String,
    },
    Verb {
        word: String,
        req: Vec<&'static str>,
    },
    AdverbGoverned {
        word: String,
    },
    Literal(String),
    Noun {
        word: String,
        req: Vec<&'static str>,
    },
}
