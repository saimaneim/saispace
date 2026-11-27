use serde_json::Value;

pub const CORRECTION_REMINDER: &str = "El usuario te está corrigiendo. Al resolverlo, si es una regla duradera, guárdala como `preference` (o actualiza la existente con su `topic_key`). Si fue un error tuyo con causa no obvia, guárdalo como `bugfix` con la causa.";

pub const STOP_REMINDER: &str = "Hubo correcciones o errores de herramientas sin guardar lección en saispace. Si queda una regla duradera guárdala como `preference` (o actualiza con `topic_key`), y si hubo causa no obvia como `bugfix`; si no aporta nada duradero, termina sin guardar.";

const MAX_PROMPT_CHARS: usize = 800;
/// Token emitted at the start of the text and after each sentence break.
const SENTENCE: &str = "\u{1}";
/// User-role transcript entries written by harnesses, not by the user.
const SYNTHETIC_PREFIXES: &[&str] = &[
    "Delegated task",
    "Used MCP tool",
    "[T3 Code",
    "[Request interrupted",
    "This session is being continued",
    "Caveat:",
    "Base directory for this skill",
];
const IGNORED_TAGS: &[&str] = &["pasted_content", "system-reminder"];

const CORRECTION_PHRASES: &[&[&str]] = &[
    &["no", "hagas"],
    &["no", "lo", "hagas"],
    &["no", "me", "hagas"],
    &["no", "me", "lo", "hagas"],
    &["no", "le", "hagas"],
    &["no", "vuelvas", "a"],
    &["no", "uses"],
    &["no", "lo", "uses"],
    &["no", "pongas"],
    &["no", "toques"],
    &["no", "borres"],
    &["no", "cambies"],
    &["no", "modifiques"],
    &["no", "edites"],
    &["no", "agregues"],
    &["no", "anadas"],
    &["no", "metas"],
    &["no", "ejecutes"],
    &["no", "corras"],
    &["te", "dije"],
    &["te", "lo", "dije"],
    &["te", "he", "dicho"],
    // "otra vez" alone is often a plain request ("corre otra vez los tests").
    &["otra", "vez", "lo", "mismo"],
    &["otra", "vez", "igual"],
    &["otra", "vez", "mal"],
    &["otra", "vez", "con", "eso"],
    &["de", "nuevo", "lo", "mismo"],
    &["again", "the", "same"],
    &["hiciste", "mal"],
    &[SENTENCE, "para", "de"],
    // Rule words count only when they open a sentence ("Nunca uses unwrap"),
    // not inside one ("explica por qué nunca pasa").
    &[SENTENCE, "nunca"],
    &[SENTENCE, "jamas"],
    &["siempre", "en"],
    &["nunca", "uses"],
    &["nunca", "hagas"],
    &["always", "use"],
    &["never", "use"],
    &["por", "que", "hiciste"],
    &["por", "que", "lo", "hiciste"],
    &["por", "que", "me", "hiciste"],
    &["por", "que", "cambiaste"],
    &["por", "que", "borraste"],
    &["por", "que", "tocaste"],
    &["por", "que", "usaste"],
    &["por", "que", "pusiste"],
    &["por", "que", "modificaste"],
    &["por", "que", "editaste"],
    &["por", "que", "quitaste"],
    &["para", "que", "hiciste"],
    &["porque", "hiciste"],
    &["no", "te", "pedi"],
    &["no", "te", "lo", "pedi"],
    &["no", "pedi"],
    &["no", "he", "pedido"],
    &["nadie", "te", "pidio"],
    &["te", "equivocaste"],
    &["estas", "equivocado"],
    &["estas", "equivocada"],
    &["cometiste", "un", "error"],
    &["no", "es", "lo", "que", "pedi"],
    &["no", "es", "lo", "que", "te", "pedi"],
    &["no", "es", "lo", "que", "queria"],
    &["no", "es", "lo", "que", "quise"],
    &["no", "es", "lo", "que", "esperaba"],
    &["no", "es", "lo", "que", "necesitaba"],
    &["no", "es", "lo", "que", "solicite"],
    &["de", "ahora", "en", "adelante"],
    &["i", "told", "you"],
    &["already", "told", "you"],
    &["you're", "wrong"],
    &["youre", "wrong"],
    &["got", "it", "wrong"],
    &["stop", "doing"],
    &["stop", "using"],
    &["stop", "editing"],
    &["stop", "changing"],
    &["stop", "touching"],
    &["stop", "adding"],
    &["stop", "deleting"],
    &["stop", "modifying"],
    &["stop", "making"],
    &[SENTENCE, "never"],
    &[SENTENCE, "always"],
    &["why", "did", "you"],
    &["why", "would", "you"],
    &["why'd", "you"],
    &["whyd", "you"],
    &["why", "didn't", "you"],
    &["why", "didnt", "you"],
    &["i've", "told", "you"],
    &["ive", "told", "you"],
    &["not", "what", "i", "asked"],
    &["wasn't", "what", "i", "asked"],
    &["wasnt", "what", "i", "asked"],
    &["not", "what", "i", "wanted"],
    &["i", "didn't", "ask"],
    &["i", "didnt", "ask"],
    &["i", "did", "not", "ask"],
    &["you", "broke"],
    &["you", "messed", "up"],
    &["from", "now", "on"],
    // Durable rules stated as corrections.
    &["going", "forward"],
    &["de", "aqui", "en", "adelante"],
    // Corrective preference: "mejor usa X" replaces the agent's choice.
    &["mejor", "usa"],
    &["mejor", "usar"],
    &["mejor", "haz"],
    &["mejor", "pon"],
    &["mejor", "cambia"],
    &["mejor", "deja"],
    &["mejor", "prefiere"],
    &["mejor", "quita"],
    &["mejor", "borra"],
    // Reminders of an unfulfilled request or a previous agreement.
    &["no", "hace", "falta"],
    &["no", "hacen", "falta"],
    &["te", "pedi"],
    &["ya", "acordamos"],
    &["ya", "lo", "acordamos"],
    &["como", "acordamos"],
    &["como", "lo", "acordamos"],
    &["como", "habiamos", "acordado"],
    &["habiamos", "acordado"],
    &["ya", "lo", "habiamos", "acordado"],
    &["segun", "lo", "acordado"],
    &["we", "agreed"],
    &["we", "had", "agreed"],
    &["had", "agreed"],
    &["like", "we", "agreed"],
    &["as", "we", "agreed"],
    &["like", "we", "decided"],
    &["as", "we", "decided"],
    &["we", "decided", "on"],
    // Present-tense blame on the agent's output.
    &["esto", "rompe"],
    &["eso", "rompe"],
    &["esto", "falla"],
    &["eso", "falla"],
    &["esto", "no", "funciona"],
    &["eso", "no", "funciona"],
    &["esto", "causa"],
    &["eso", "causa"],
    &["esto", "provoca"],
    &["this", "is", "slower"],
    &["that", "is", "slower"],
    // 2nd-person destructive past-tense blame in English.
    &["you", "deleted"],
    &["you", "overwrote"],
    &["you", "wiped"],
    &["you", "clobbered"],
    &["you", "reverted"],
    &["you've", "deleted"],
    &["you've", "overwrote"],
    &["you've", "wiped"],
    &["you've", "clobbered"],
    &["you've", "reverted"],
    // Correction of a wrong reference or direction.
    &["wasnt", "what", "i"],
    &["wasn't", "what", "i"],
    &["i", "meant"],
    &["queria", "decir"],
    &["quise", "decir"],
    &["that's", "backwards"],
    &["thats", "backwards"],
    &["you", "have", "it", "backwards"],
    &["es", "al", "reves"],
    &["esta", "al", "reves"],
    &["quedo", "al", "reves"],
    &["tienes", "al", "reves"],
    &["todo", "al", "reves"],
    &["esta", "invertido"],
    &["esta", "invertida"],
    // Direct rollback commands.
    &[SENTENCE, "wrong"],
    &[SENTENCE, "revert"],
    &[SENTENCE, "undo"],
    &[SENTENCE, "rollback"],
    &[SENTENCE, "revierte"],
    &[SENTENCE, "deshaz"],
    &["revert", "that"],
    &["undo", "that"],
    &["revert", "it"],
    &["undo", "it"],
    &["revert", "the", "change"],
    &["undo", "the", "change"],
    // Repetition and frustration markers.
    &["otra", "vez", "mas"],
    &["otra", "vez", "te"],
    &["otra", "vez", "me"],
    &["ya", "otra", "vez"],
    &["you", "keep", "doing"],
    &["you", "keep", "using"],
    &["you", "keep", "making"],
    &["sigues", "haciendo"],
    &["sigues", "usando"],
    &["estoy", "harto"],
    &["estoy", "harta"],
    &["same", "mistake"],
    &["same", "mistakes"],
    &["mismo", "error"],
    &["mismo", "problema"],
    &["not", "again"],
    &["ya", "basta"],
    &["basta", "de"],
    &["cambia", "eso"],
];

/// State-blame phrases ("está mal", "is wrong"): they signal a correction only
/// when they are not part of a question or a conditional, which is checked by
/// looking at the words right before the match.
const WEAK_CORRECTION_PHRASES: &[&[&str]] = &[
    &["esta", "mal"],
    &["esta", "muy", "mal"],
    &["estan", "mal"],
    &["estan", "muy", "mal"],
    &["salio", "mal"],
    &["quedo", "mal"],
    &["no", "es", "asi"],
    &["no", "era", "asi"],
    &["asi", "no"],
    &["no", "debe"],
    &["no", "deben"],
    &["no", "deberia"],
    &["no", "deberias"],
    &["no", "debia"],
    &["no", "debias"],
    &["no", "debian"],
    &["this", "breaks"],
    &["that", "breaks"],
    &["this", "fails"],
    &["that", "fails"],
    &["this", "causes"],
    &["that", "causes"],
    &["this", "doesn't", "work"],
    &["that", "doesn't", "work"],
    &["is", "wrong"],
    &["it's", "wrong"],
    &["its", "wrong"],
    &["thats", "wrong"],
    &["that's", "wrong"],
    &["is", "backwards"],
    &["it's", "backwards"],
    &["its", "backwards"],
];

/// Question/conditional words that suppress a weak phrase when one of them sits
/// just before it inside the same sentence ("¿qué está mal?", "check if the
/// date is wrong", "si el archivo está mal formado, devuelve 400").
const WEAK_GUARD_TOKENS: &[&str] = &[
    "que",
    "si",
    "cuando",
    "cual",
    "donde",
    "quien",
    "cuanto",
    "what",
    "when",
    "if",
    "so",
    "whether",
    "check",
    "verify",
    "mira",
    "revisa",
    "comprueba",
    "depends",
];

/// Corrective Spanish verbs in the second-person negated subjunctive. An
/// explicit inventory avoids treating present indicatives and English nouns as
/// commands merely because they end in -as/-es.
const NEGATED_SUBJUNCTIVE_VERBS: &[&str] = &[
    "agregues",
    "anadas",
    "borres",
    "cambies",
    "corras",
    "dependas",
    "edites",
    "ejecutes",
    "elimines",
    "escribas",
    "hagas",
    "inventes",
    "metas",
    "mezcles",
    "modifiques",
    "muevas",
    "pongas",
    "quites",
    "renombres",
    "rompas",
    "sigas",
    "sobrescribas",
    "toques",
    "uses",
];

/// Destructive or corrective second-person past forms that can express blame.
const SECOND_PERSON_PAST_VERBS: &[&str] = &[
    "borraste",
    "cambiaste",
    "editaste",
    "eliminaste",
    "metiste",
    "modificaste",
    "moviste",
    "pusiste",
    "quitaste",
    "revertiste",
    "rompiste",
    "sobrescribiste",
    "tocaste",
];

/// Verbs that make a bare English negation a command about code or behaviour.
const ENGLISH_NEGATED_ACTIONS: &[&str] = &[
    "add", "change", "commit", "create", "delete", "do", "edit", "install", "mock", "move", "push",
    "remove", "rename", "rewrite", "run", "touch", "use",
];

/// Imperatives that turn a temporal discourse marker into a durable rule.
const DURABLE_RULE_COMPLEMENTS: &[&str] = &[
    "actualiza",
    "agrega",
    "anade",
    "borra",
    "cambia",
    "corre",
    "crea",
    "documenta",
    "ejecuta",
    "escribe",
    "evita",
    "genera",
    "guarda",
    "haz",
    "incluye",
    "manten",
    "pon",
    "prefiere",
    "quita",
    "responde",
    "revisa",
    "usa",
    "utiliza",
    "valida",
];

/// Imperative stems that, carrying a clitic ("cámbialo"), command the agent to
/// redo part of its own previous work.
const CORRECTIVE_ENCLITIC_STEMS: &[&str] = &[
    "cambia",
    "quita",
    "borra",
    "elimina",
    "arregla",
    "corrige",
    "reemplaza",
    "reverte",
    "revierte",
    "mueve",
    "saca",
    "renombra",
    "actualiza",
    "deshaz",
    "deja",
    "retira",
];

const CORRECTIVE_ENCLITIC_SUFFIXES: &[&str] =
    &["lo", "la", "los", "las", "selo", "sela", "selos", "selas"];

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TranscriptSignals {
    pub corrections: usize,
    pub failed_tools: usize,
    pub saved_after_last_correction: bool,
    pub edits: usize,
}

fn find_ascii_ci(haystack: &str, needle: &str) -> Option<usize> {
    let target = needle.as_bytes();
    if target.is_empty() || haystack.len() < target.len() {
        return None;
    }
    haystack
        .as_bytes()
        .windows(target.len())
        .position(|w| w.eq_ignore_ascii_case(target))
        .filter(|&idx| haystack.is_char_boundary(idx))
}

enum TagSkip {
    Matched(usize),
    UnclosedTag(usize),
    NoCloseBracket,
}

fn match_ignored_tag_end(rest_after_lt: &str, unclosed_mask: u8) -> TagSkip {
    for (idx, &tag) in IGNORED_TAGS.iter().enumerate() {
        if unclosed_mask & (1 << idx) != 0 {
            continue;
        }
        let len = tag.len();
        if rest_after_lt.len() <= len {
            continue;
        }
        let bytes = rest_after_lt.as_bytes();
        if !bytes[..len].eq_ignore_ascii_case(tag.as_bytes()) {
            continue;
        }
        let delim = bytes[len];
        if delim != b'>' && !delim.is_ascii_whitespace() {
            continue;
        }
        let Some(open_close) = rest_after_lt[len..].find('>') else {
            return TagSkip::NoCloseBracket;
        };
        let after_open = len + open_close + 1;
        let close_tag = format!("</{tag}>");
        let Some(close_pos) = find_ascii_ci(&rest_after_lt[after_open..], &close_tag) else {
            return TagSkip::UnclosedTag(idx);
        };
        return TagSkip::Matched(after_open + close_pos + close_tag.len());
    }
    TagSkip::Matched(0)
}

fn strip_ignored_tags(prompt: &str) -> String {
    let mut out = String::with_capacity(prompt.len());
    let mut pos = 0;
    let mut unclosed_mask = 0u8;
    while let Some(rel_lt) = prompt[pos..].find('<') {
        let lt = pos + rel_lt;
        out.push_str(&prompt[pos..lt]);
        match match_ignored_tag_end(&prompt[lt + 1..], unclosed_mask) {
            TagSkip::Matched(0) => {
                out.push('<');
                pos = lt + 1;
            }
            TagSkip::Matched(skip_len) => {
                pos = lt + 1 + skip_len;
            }
            TagSkip::UnclosedTag(idx) => {
                unclosed_mask |= 1 << idx;
                out.push('<');
                pos = lt + 1;
            }
            TagSkip::NoCloseBracket => {
                out.push_str(&prompt[lt..]);
                return out;
            }
        }
    }
    out.push_str(&prompt[pos..]);
    out
}

fn deaccent(c: char) -> char {
    match c {
        'á' | 'à' | 'â' | 'ä' | 'ã' | 'å' => 'a',
        'ç' => 'c',
        'é' | 'è' | 'ê' | 'ë' => 'e',
        'í' | 'ì' | 'î' | 'ï' => 'i',
        'ó' | 'ò' | 'ô' | 'ö' | 'õ' => 'o',
        'ú' | 'ù' | 'û' | 'ü' => 'u',
        'ñ' => 'n',
        'ý' | 'ÿ' => 'y',
        _ => c,
    }
}

fn tokenize_prefix(text: &str) -> Vec<String> {
    let mut norm = Vec::with_capacity(text.len().min(MAX_PROMPT_CHARS));
    for c in text.trim_start().chars().take(MAX_PROMPT_CHARS) {
        if ('\u{0300}'..='\u{036f}').contains(&c) {
            continue;
        }
        let normalized = match c {
            '’' | '‘' | 'ʼ' => '\'',
            other => other,
        };
        for lc in normalized.to_lowercase() {
            norm.push(deaccent(lc));
        }
    }

    let mut tokens = vec![SENTENCE.to_string()];
    let mut current = String::new();
    for (i, &c) in norm.iter().enumerate() {
        let joiner_between_alnum = matches!(c, '\'' | '-')
            && i > 0
            && i + 1 < norm.len()
            && norm[i - 1].is_alphanumeric()
            && norm[i + 1].is_alphanumeric();
        if c.is_alphanumeric() || c == '_' || joiner_between_alnum {
            current.push(c);
        } else {
            if !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
            }
            if matches!(c, '.' | '!' | '?' | '\n' | ';' | ':' | '¡' | '¿')
                && tokens.last().is_some_and(|t| t != SENTENCE)
            {
                tokens.push(SENTENCE.to_string());
            }
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

/// True when `phrase` occurs in `tokens` starting exactly at `start`.
fn phrase_at(tokens: &[String], phrase: &[&str], start: usize) -> bool {
    start + phrase.len() <= tokens.len()
        && tokens[start..start + phrase.len()]
            .iter()
            .zip(phrase)
            .all(|(a, b)| a.as_str() == *b)
}

/// A weak phrase only counts when no question/conditional word sits just
/// before it inside the same sentence.
fn weak_phrase_is_guarded(tokens: &[String], start: usize) -> bool {
    let lo = start.saturating_sub(6);
    tokens[lo..start]
        .iter()
        .rev()
        .take_while(|t| t.as_str() != SENTENCE)
        .any(|t| WEAK_GUARD_TOKENS.contains(&t.as_str()))
}

/// Second-person subjunctive after "no" ("no borres", "no inventes").
fn negated_subjunctive_es(word: &str) -> bool {
    NEGATED_SUBJUNCTIVE_VERBS.contains(&word)
}

fn second_person_past_es(word: &str) -> bool {
    SECOND_PERSON_PAST_VERBS.contains(&word)
}

fn sentence_start(tokens: &[String], i: usize) -> usize {
    tokens[..i]
        .iter()
        .rposition(|t| t.as_str() == SENTENCE)
        .map_or(0, |p| p + 1)
}

/// Past-tense blame needs both a corrective verb and a blaming context. Neutral
/// questions opened by qué/cómo/dónde/cuándo never qualify.
fn second_person_past_blame_at(tokens: &[String], i: usize) -> bool {
    if !second_person_past_es(tokens[i].as_str()) {
        return false;
    }
    let start = sentence_start(tokens, i);
    let neutral_question = tokens[start..i].iter().enumerate().any(|(offset, t)| {
        let absolute = start + offset;
        match t.as_str() {
            "como" | "donde" | "cuando" => true,
            "que" => absolute == 0 || !matches!(tokens[absolute - 1].as_str(), "por" | "para"),
            _ => false,
        }
    });
    if neutral_question {
        return false;
    }
    if start == i
        || tokens
            .get(i.wrapping_sub(1))
            .is_some_and(|t| matches!(t.as_str(), "lo" | "la" | "los" | "las"))
    {
        return true;
    }
    tokens[start..i]
        .windows(2)
        .any(|w| matches!(w[0].as_str(), "por" | "para") && w[1].as_str() == "que")
}

/// "No era X, era Y" and "no era X sino Y" correct a reference; a bare "no
/// era" is just a statement about a previous state.
fn contrastive_no_era_at(tokens: &[String], i: usize) -> bool {
    if !phrase_at(tokens, &["no", "era"], i) {
        return false;
    }
    let mut seen_contrasted_term = false;
    for token in &tokens[i + 2..] {
        if token.as_str() == SENTENCE {
            break;
        }
        if seen_contrasted_term && matches!(token.as_str(), "era" | "sino") {
            return true;
        }
        seen_contrasted_term = true;
    }
    false
}

fn english_negated_action_at(tokens: &[String], i: usize) -> bool {
    let action_index = match tokens[i].as_str() {
        "don't" | "dont" => i + 1,
        "do" if tokens.get(i + 1).is_some_and(|t| t == "not") => i + 2,
        _ => return false,
    };
    tokens
        .get(action_index)
        .is_some_and(|t| ENGLISH_NEGATED_ACTIONS.contains(&t.as_str()))
}

fn durable_rule_at(tokens: &[String], i: usize) -> bool {
    let action_index = if phrase_at(tokens, &["desde", "ahora"], i)
        || phrase_at(tokens, &["desde", "hoy"], i)
        || phrase_at(tokens, &["desde", "ya"], i)
    {
        i + 2
    } else if phrase_at(tokens, &["a", "partir", "de", "ahora"], i)
        || phrase_at(tokens, &["a", "partir", "de", "hoy"], i)
    {
        i + 4
    } else {
        return false;
    };
    tokens
        .get(action_index)
        .is_some_and(|t| DURABLE_RULE_COMPLEMENTS.contains(&t.as_str()))
}

fn deja_de_infinitive_at(tokens: &[String], i: usize) -> bool {
    phrase_at(tokens, &["deja", "de"], i)
        && tokens
            .get(i + 2)
            .is_some_and(|t| ["ar", "er", "ir"].iter().any(|suffix| t.ends_with(suffix)))
}

/// Imperative of a corrective verb plus clitic ("cámbialo", "reviértela").
fn corrective_enclitic(word: &str) -> bool {
    CORRECTIVE_ENCLITIC_SUFFIXES.iter().any(|suffix| {
        word.strip_suffix(suffix)
            .is_some_and(|stem| stem.len() >= 4 && CORRECTIVE_ENCLITIC_STEMS.contains(&stem))
    })
}

fn neutral_deja_enclitic_at(tokens: &[String], i: usize) -> bool {
    let Some(suffix) = tokens[i].as_str().strip_prefix("deja") else {
        return false;
    };
    if !CORRECTIVE_ENCLITIC_SUFFIXES.contains(&suffix) {
        return false;
    }
    let next = tokens.get(i + 1).map(String::as_str);
    matches!(next, Some("asi" | "igual"))
        || (next == Some("como")
            && tokens
                .get(i + 2)
                .is_some_and(|t| matches!(t.as_str(), "esta" | "estaba")))
}

fn demonstrative_blame_is_guarded(tokens: &[String], start: usize) -> bool {
    matches!(tokens[start].as_str(), "this" | "that")
        && start
            .checked_sub(1)
            .is_some_and(|prev| tokens[prev].as_str() != SENTENCE)
}

/// Structural rules that generalize beyond the phrase lists.
fn lexical_signal_at(tokens: &[String], i: usize) -> bool {
    let word = tokens[i].as_str();
    if second_person_past_blame_at(tokens, i)
        || (corrective_enclitic(word) && !neutral_deja_enclitic_at(tokens, i))
        || english_negated_action_at(tokens, i)
        || durable_rule_at(tokens, i)
        || deja_de_infinitive_at(tokens, i)
        || contrastive_no_era_at(tokens, i)
    {
        return true;
    }
    let sentence_initial_siempre = word == "siempre"
        && i.checked_sub(1)
            .is_some_and(|prev| tokens[prev].as_str() == SENTENCE)
        && tokens.get(i + 1).is_none_or(|t| t != "que");
    sentence_initial_siempre
        || (word == "no"
            && tokens
                .get(i + 1)
                .is_some_and(|n| negated_subjunctive_es(n.as_str())))
}

fn correction_signal_at(tokens: &[String], i: usize) -> bool {
    if CORRECTION_PHRASES.iter().any(|p| phrase_at(tokens, p, i)) {
        return true;
    }
    if WEAK_CORRECTION_PHRASES
        .iter()
        .any(|p| phrase_at(tokens, p, i))
        && !weak_phrase_is_guarded(tokens, i)
        && !demonstrative_blame_is_guarded(tokens, i)
    {
        return true;
    }
    lexical_signal_at(tokens, i)
}

fn is_correction_stripped(stripped: &str) -> bool {
    let tokens = tokenize_prefix(stripped);
    if tokens.is_empty() {
        return false;
    }
    (0..tokens.len()).any(|i| correction_signal_at(&tokens, i))
}

#[must_use]
pub fn is_correction(prompt: &str) -> bool {
    let stripped = strip_ignored_tags(prompt);
    let trimmed = stripped.trim_start();
    !SYNTHETIC_PREFIXES.iter().any(|p| trimmed.starts_with(p)) && is_correction_stripped(&stripped)
}

fn is_memory_save_tool(name: &str, input: Option<&Value>) -> bool {
    if name.ends_with("save_memory") || name.contains("consolidate_memories") {
        return true;
    }
    if name.eq_ignore_ascii_case("bash")
        && let Some(cmd) = input.and_then(|v| v.get("command")).and_then(Value::as_str)
    {
        return cmd.contains("sai memory save") || cmd.contains("sai memory merge");
    }
    false
}

fn is_synthetic_text(text: &str) -> bool {
    let trimmed = text.trim_start();
    trimmed.starts_with('<') || SYNTHETIC_PREFIXES.iter().any(|p| trimmed.starts_with(p))
}

/// `is_error` when present; otherwise results whose text starts like an
/// error (some harnesses omit the flag).
fn is_failed_result(block: &Value, entry: &Value) -> bool {
    if let Some(flag) = block.get("is_error").and_then(Value::as_bool) {
        return flag;
    }
    let looks_failed = |text: &str| {
        let t = text.trim_start();
        t.starts_with("Error:") || t.starts_with("<tool_use_error>")
    };
    let content = block.get("content");
    content.and_then(Value::as_str).is_some_and(looks_failed)
        || content
            .and_then(Value::as_array)
            .and_then(|parts| parts.first())
            .and_then(|part| part.get("text"))
            .and_then(Value::as_str)
            .is_some_and(looks_failed)
        || entry
            .get("toolUseResult")
            .and_then(Value::as_str)
            .is_some_and(looks_failed)
}

fn record_user_prompt(cleaned: &str, signals: &mut TranscriptSignals) {
    if !is_synthetic_text(cleaned) && is_correction_stripped(cleaned) {
        signals.corrections += 1;
        signals.saved_after_last_correction = false;
    }
}

fn process_user_entry(entry: &Value, signals: &mut TranscriptSignals) {
    let Some(content) = entry.get("message").and_then(|m| m.get("content")) else {
        return;
    };
    let is_synthetic = entry
        .get("isMeta")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        || entry
            .get("isCompactSummary")
            .and_then(Value::as_bool)
            .unwrap_or(false);
    match content {
        Value::String(text) => {
            if !is_synthetic {
                let cleaned = strip_ignored_tags(text);
                record_user_prompt(&cleaned, signals);
            }
        }
        Value::Array(blocks) => {
            let mut texts = Vec::new();
            for block in blocks {
                let block_type = block.get("type").and_then(Value::as_str);
                if block_type == Some("tool_result") && is_failed_result(block, entry) {
                    signals.failed_tools += 1;
                } else if !is_synthetic
                    && block_type == Some("text")
                    && let Some(text) = block.get("text").and_then(Value::as_str)
                {
                    let cleaned = strip_ignored_tags(text);
                    let trimmed = cleaned.trim_start();
                    if !trimmed.is_empty() && !is_synthetic_text(trimmed) {
                        texts.push(cleaned);
                    }
                }
            }
            if !texts.is_empty() {
                record_user_prompt(&texts.join("\n"), signals);
            }
        }
        _ => {}
    }
}

fn process_assistant_entry(entry: &Value, signals: &mut TranscriptSignals) {
    let Some(blocks) = entry
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(Value::as_array)
    else {
        return;
    };
    for block in blocks {
        if block.get("type").and_then(Value::as_str) != Some("tool_use") {
            continue;
        }
        let Some(name) = block.get("name").and_then(Value::as_str) else {
            continue;
        };
        if is_memory_save_tool(name, block.get("input")) {
            signals.saved_after_last_correction = true;
        }
        if matches!(name, "Edit" | "Write" | "MultiEdit" | "NotebookEdit") {
            signals.edits += 1;
        }
    }
}

#[must_use]
pub fn analyze_transcript(jsonl: &str) -> TranscriptSignals {
    let mut signals = TranscriptSignals::default();
    for line in jsonl.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(entry) = serde_json::from_str::<Value>(trimmed) else {
            continue;
        };
        match entry.get("type").and_then(Value::as_str) {
            Some("user") => process_user_entry(&entry, &mut signals),
            Some("assistant") => process_assistant_entry(&entry, &mut signals),
            _ => {}
        }
    }
    signals
}

#[cfg(test)]
mod tests {
    use super::{
        CORRECTION_REMINDER, STOP_REMINDER, TranscriptSignals, analyze_transcript, is_correction,
    };

    #[test]
    fn reminder_constants_fit_budget() {
        assert!(CORRECTION_REMINDER.chars().count() <= 300);
        assert!(STOP_REMINDER.chars().count() <= 300);
        assert!(STOP_REMINDER.contains("si no aporta nada duradero, termina sin guardar"));
    }

    #[test]
    fn detects_spanish_and_english_corrections_case_and_accent_insensitively() {
        for sample in [
            "No hagas eso",
            "TE DIJE que uses bun",
            "ya te dije ayer",
            "Otra vez lo mismo",
            "Eso está mal",
            "eso esta mal!",
            "No es así",
            "no es asi",
            "Deja de tocar ese fichero",
            "No vuelvas a hacer commit",
            "NUNCA uses unwrap",
            "Responde SIEMPRE en español",
            "¿Por qué hiciste eso?",
            "por que hiciste eso",
            "No te pedí que cambiaras eso",
            "no te pedi eso",
            "Te equivocaste de ruta",
            "Eso no es lo que quería",
            "Don't edit that file",
            "don’t do that",
            "I told you before",
            "That is WRONG",
            "Stop doing that please",
            "Never mock the database",
            "Always run clippy first",
            "Why did you delete it?",
            "That's not what I asked",
        ] {
            assert!(is_correction(sample), "expected match for: {sample}");
        }
    }

    #[test]
    fn avoids_noise_and_respects_word_boundaries() {
        for sample in [
            "run it again",
            "Handle wrong passwords correctly",
            "Explain why this never happens",
            "explica por qué nunca pasa",
            "Delegated task node:x completed: no hagas nada",
            "corre otra vez los tests",
            "Please try again later",
            "Implement the login endpoint",
            "Ejecuta cargo test --workspace",
            "Revisa la variable is_wrong y always_inline",
            "Use an always-on toggle",
            "Whenever you have time, check it",
            "Esto es normal y minimal",
            "",
            "   ",
        ] {
            assert!(!is_correction(sample), "unexpected match for: {sample}");
        }
    }

    #[test]
    fn ignores_pasted_content_system_reminders_and_tail_after_800_chars() {
        assert!(!is_correction(
            "<system-reminder>Always check rules and never fail</system-reminder>Hola, mira main.rs"
        ));
        assert!(!is_correction(
            "<pasted_content path=\"log.txt\">Esto está mal, don't touch</pasted_content>Resume esto"
        ));
        assert!(is_correction(
            "<system-reminder>ok</system-reminder>\nOtra vez lo mismo"
        ));
        assert!(is_correction(
            "<pasted_content>log</pasted_content> Por qué hiciste esto"
        ));
        assert!(is_correction(
            "<system-reminder>unclosed tag without end, no hagas caso"
        ));

        let padding = "palabra ".repeat(105);
        assert!(padding.chars().count() > 800);
        let with_tail = format!("{padding}never do that");
        assert!(!is_correction(&with_tail));
    }

    #[test]
    fn analyzes_transcript_signals_across_turns_and_malformed_lines() {
        let lines = [
            "not valid json {{{",
            r#"{"type":"system","message":{"content":"ignore"}}"#,
            r#"{"type":"user","isMeta":true,"message":{"content":"Never do this meta"}}"#,
            r#"{"type":"user","message":{"content":"<command-name>wrong</command-name>"}}"#,
            r#"{"type":"user","message":{"content":"Te dije que uses bun"}}"#,
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Edit","input":{}},{"type":"tool_use","name":"Write","input":{}}]}}"#,
            r#"{"type":"user","message":{"content":[{"type":"tool_result","is_error":true},{"type":"tool_result","is_error":false}]}}"#,
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"mcp__saispace__save_memory","input":{}}]}}"#,
            r#"{"type":"user","message":{"content":[{"type":"text","text":"<ide_opened_file>x</ide_opened_file>"},{"type":"text","text":"otra vez lo mismo"}]}}"#,
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"MultiEdit","input":{}},{"type":"tool_use","name":"NotebookEdit","input":{}}]}}"#,
            r#"{"type":"user","message":{"content":[{"type":"tool_result","is_error":true}]}}"#,
            r#"{"type":"user","message":{"content":[{"type":"tool_result","is_error":true}]}}"#,
        ]
        .join("\n");

        assert_eq!(
            analyze_transcript(&lines),
            TranscriptSignals {
                corrections: 2,
                failed_tools: 3,
                saved_after_last_correction: false,
                edits: 4,
            }
        );
    }

    #[test]
    fn recognizes_all_memory_save_variants_and_zero_correction_saves() {
        let with_bash_save = [
            r#"{"type":"user","message":{"content":"don't do that"}}"#,
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Bash","input":{"command":"sai memory save --title t --content c"}}]}}"#,
        ]
        .join("\n");
        assert!(analyze_transcript(&with_bash_save).saved_after_last_correction);

        let with_bash_merge = [
            r#"{"type":"user","message":{"content":"don't do that"}}"#,
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Bash","input":{"command":"sai memory merge a b --title t --content -"}}]}}"#,
        ]
        .join("\n");
        assert!(analyze_transcript(&with_bash_merge).saved_after_last_correction);

        let with_consolidate = [
            r#"{"type":"user","message":{"content":"wrong"}}"#,
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"mcp__saispace__consolidate_memories","input":{}}]}}"#,
        ]
        .join("\n");
        assert!(analyze_transcript(&with_consolidate).saved_after_last_correction);

        let no_corrections_with_save = [
            r#"{"type":"user","message":{"content":"revisa esto"}}"#,
            r#"{"type":"user","message":{"content":[{"type":"tool_result","is_error":true},{"type":"tool_result","is_error":true},{"type":"tool_result","is_error":true}]}}"#,
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"save_memory","input":{}}]}}"#,
        ]
        .join("\n");
        let signals = analyze_transcript(&no_corrections_with_save);
        assert_eq!(signals.corrections, 0);
        assert_eq!(signals.failed_tools, 3);
        assert!(signals.saved_after_last_correction);
    }

    #[test]
    fn transcript_strips_ignored_tags_before_angle_filter_and_skips_compact_summaries() {
        let lines = [
            r#"{"type":"user","isCompactSummary":true,"message":{"content":"Always check tests and never ignore errors"}}"#,
            r#"{"type":"user","message":{"content":"<system-reminder>ok</system-reminder>\nOtra vez lo mismo"}}"#,
            r#"{"type":"user","message":{"content":[{"type":"text","text":"<pasted_content>log</pasted_content> No hagas eso"}]}}"#,
        ]
        .join("\n");
        let signals = analyze_transcript(&lines);
        assert_eq!(signals.corrections, 2);

        let repeated_unclosed = format!("{} no hagas", "<system-reminder>".repeat(5000));
        assert!(!is_correction(&repeated_unclosed));
        let prefixed_unclosed = format!("no hagas {}", "<system-reminder>".repeat(5000));
        assert!(is_correction(&prefixed_unclosed));
        assert!(is_correction("why didnt you run tests"));
        assert!(is_correction("ive told you twice"));
    }

    #[test]
    fn harness_messages_and_unflagged_errors_are_classified() {
        let jsonl = [
            r#"{"type":"user","message":{"content":"Delegated task node:x [completed]: no hagas eso"}}"#,
            r#"{"type":"user","message":{"content":[{"type":"text","text":"Used MCP tool: te dije"}]}}"#,
            r#"{"type":"user","message":{"content":[{"type":"tool_result","content":"Error: exit 1"}]}}"#,
            r#"{"type":"user","message":{"content":[{"type":"tool_result","content":[{"type":"text","text":"<tool_use_error>boom</tool_use_error>"}]}]}}"#,
            r#"{"type":"user","toolUseResult":"Error: no such file","message":{"content":[{"type":"tool_result","content":"x"}]}}"#,
            r#"{"type":"user","message":{"content":[{"type":"tool_result","is_error":false,"content":"Error: but flagged ok"}]}}"#,
        ]
        .join("\n");
        let signals = analyze_transcript(&jsonl);
        assert_eq!(signals.corrections, 0);
        assert_eq!(signals.failed_tools, 3);
    }

    #[test]
    fn detects_negated_subjunctive_imperatives() {
        for sample in [
            "no inventes rutas que no comprobaste",
            "No mezcles estilos de indentación",
            "no sigas con ese enfoque",
            "no escribas la clave en el fichero de config",
            "no dependas de la red para los tests",
            "no elimines el índice sin una migración",
            "no sobrescribas el snapshot manualmente",
        ] {
            assert!(is_correction(sample), "expected match for: {sample}");
        }
        for sample in [
            "no gracias, deja el resto igual",
            "no issues found en el último análisis",
            "no changes detected después del parche",
            "usa las credenciales del staging, no las de producción",
            "no sabías que el módulo existía",
            "comprueba que no trabajas con datos reales",
            "explica por qué no puedes abrir ese puerto",
            "verifica que no tienes que regenerar el lockfile",
            "no indexes appear in the query plan",
            "no instances are running in staging",
            "no resources were created by the dry run",
            "no releases are tagged for this crate",
        ] {
            assert!(!is_correction(sample), "unexpected match for: {sample}");
        }
    }

    #[test]
    fn detects_second_person_past_blame() {
        for sample in [
            "¿por qué moviste la constante de sitio?",
            "¿para qué quitaste esa validación?",
            "editaste el archivo equivocado",
            "borraste la migración sin avisar",
            "lo moviste y ahora fallan los imports",
        ] {
            assert!(is_correction(sample), "expected match for: {sample}");
        }
        for sample in [
            "el fichero existe en dos rutas",
            "en contraste con el dataset grande, este es pequeño",
            "la librería persiste los datos entre sesiones",
            "el chiste del comentario explica el fixture",
            "la librería resiste picos de tráfico",
            "el consumidor insiste en reintentar",
            "¿qué comiste?",
            "¿cómo instalaste la dependencia?",
            "¿cómo lo borraste en tu entorno?",
            "explica qué cambiaste en el refactor",
            "explica cómo lo borraste sin afectar al índice",
            "resume qué borraste del fixture",
        ] {
            assert!(!is_correction(sample), "unexpected match for: {sample}");
        }
    }

    #[test]
    fn detects_corrective_enclitics() {
        for sample in [
            "Cámbialo a minúsculas en todos los textos",
            "reviértelo antes del deploy",
            "Quítala del grupo de servicios",
            "arreglalo y sube la rama",
            "déjalo en español",
        ] {
            assert!(is_correction(sample), "expected match for: {sample}");
        }
        for sample in [
            "hazlo con cuidado mañana",
            "déjame revisar el plan",
            "ponla en la lista de pendientes",
            "quita los logs de debug del build",
            "déjalo así para mantener compatibilidad",
            "déjalo igual y añade un comentario",
            "déjalo como está hasta el próximo release",
            "déjala igual por ahora",
        ] {
            assert!(!is_correction(sample), "unexpected match for: {sample}");
        }
    }

    #[test]
    fn detects_only_contrastive_no_era_reference_corrections() {
        for sample in [
            "no era el entero con signo, era el entero sin signo",
            "no era el puerto 80, era el 8080",
            "no era una copia sino una referencia",
        ] {
            assert!(is_correction(sample), "expected match for: {sample}");
        }
        for sample in [
            "no, era el entero sin signo",
            "no era necesario reimprimir el ticket",
            "no era mi intención sonar brusco",
            "no era fácil adivinarlo",
            "el test no era estable antes del parche",
            "switch to the streaming reader",
            "add a kill switch to stop the indexer",
        ] {
            assert!(!is_correction(sample), "unexpected match for: {sample}");
        }
    }

    #[test]
    fn detects_agreements_preferences_and_unfulfilled_requests() {
        for sample in [
            "we agreed on retries at the worker level",
            "como acordamos, la cabecera va primero",
            "mejor usa un pool de conexiones",
            "ese campo no debe exponerse en la respuesta",
            "no debería abrir el puerto en local",
            "te pedí el resumen en una sola línea",
            "no hace falta regenerar las migraciones",
            "ya lo habíamos acordado en el hilo anterior",
        ] {
            assert!(is_correction(sample), "expected match for: {sample}");
        }
        for sample in [
            "we disagree about the naming convention",
            "usa el mejor algoritmo disponible",
        ] {
            assert!(!is_correction(sample), "unexpected match for: {sample}");
        }
    }

    #[test]
    fn detects_wrong_references_and_rollback_commands() {
        for sample in [
            "I meant the staging bucket",
            "quería decir el modo oscuro",
            "that wasn't what I meant",
            "That's backwards, the mask should go last",
            "Wrong file: those constants live in config.rs",
            "revert that migration, it dropped rows",
            "undo the rename of the column",
            "tienes todo al revés con los signos",
            "es al revés: primero valida y después escribe",
        ] {
            assert!(is_correction(sample), "expected match for: {sample}");
        }
        for sample in [
            "describe how to revert a commit step by step",
            "explain what undo does in the editor",
        ] {
            assert!(!is_correction(sample), "unexpected match for: {sample}");
        }
    }

    #[test]
    fn detects_durable_rules_from_now_on() {
        for sample in [
            "going forward, lint before every push",
            "desde ahora incluye el número de build",
            "desde ya, usa bun en el repo",
            "a partir de ahora ejecuta los tests antes del push",
            "a partir de hoy responde en español",
        ] {
            assert!(is_correction(sample), "expected match for: {sample}");
        }
        for sample in [
            "run it one more time",
            "la opción está disponible desde ya en nightly",
            "el módulo existe desde hoy en el registry",
            "el precio cambia a partir de hoy",
            "el cambio aplica a partir de este commit",
            "a partir de este archivo se genera el schema",
        ] {
            assert!(!is_correction(sample), "unexpected match for: {sample}");
        }
    }

    #[test]
    fn detects_present_tense_blame_and_frustration() {
        for sample in [
            "eso rompe el modo offline",
            "you deleted the coverage script",
            "you've deleted the helper again",
            "estoy harto de pedir lo mismo",
            "you keep making the same mistake",
            "es el mismo error de la semana pasada",
            "otra vez más el formateo descuadrado",
        ] {
            assert!(is_correction(sample), "expected match for: {sample}");
        }
        for sample in [
            "el usuario borra su token al salir",
            "the build keeps failing on CI",
            "you've moved the helper again",
        ] {
            assert!(!is_correction(sample), "unexpected match for: {sample}");
        }
    }

    #[test]
    fn requires_code_actions_after_bare_english_negations() {
        for sample in [
            "don't edit the generated file",
            "do not use the production bucket",
            "dont touch that migration",
            "don't delete the fixture",
            "don't change the public API",
            "don't add another dependency",
            "don't run the deploy yet",
            "don't commit the credentials",
            "don't push this branch",
            "don't mock the database in integration tests",
            "don't create a new endpoint",
            "don't rename the column",
            "don't move the helper yet",
            "don't remove the validation",
            "don't rewrite the parser without tests",
            "don't install packages globally",
            "don't do that again",
        ] {
            assert!(is_correction(sample), "expected match for: {sample}");
        }
        for sample in [
            "don't worry about the coverage drop",
            "don't forget to document the environment variable",
            "don't hesitate to ask about the API contract",
            "don't mind the old parser warning",
            "do not disturb the on-call engineer",
            "we don't think this cache is necessary",
            "don't know why the snapshot differs",
        ] {
            assert!(!is_correction(sample), "unexpected match for: {sample}");
        }
    }

    #[test]
    fn requires_first_person_requests_and_destructive_second_person_blame() {
        for sample in [
            "I didn't ask for a new abstraction",
            "I did not ask you to change the schema",
            "you deleted the coverage script",
            "you overwrote my local changes",
            "you wiped the test database",
            "you clobbered the patch",
            "you reverted the fix",
            "you broke the Windows build",
        ] {
            assert!(is_correction(sample), "expected match for: {sample}");
        }
        for sample in [
            "the API didn't ask for authentication",
            "the client did not ask for compression",
            "we didn't ask whether the cache was enabled",
            "show the diff where you changed the parser",
            "explain why you removed the deprecated option",
            "summarize what you edited in the last commit",
            "list the files you renamed for the release",
            "tell me which helper you moved into utils",
            "confirm that you touched only generated files",
            "check whether you overrode the default timeout",
            "show me where you swapped the arguments",
        ] {
            assert!(!is_correction(sample), "unexpected match for: {sample}");
        }
    }

    #[test]
    fn requires_infinitives_after_deja_de_and_ignores_siempre_que() {
        for sample in [
            "deja de tocar ese fichero",
            "deja de modificar el test",
            "deja de borrar los logs",
            "siempre ejecuta clippy antes de terminar",
        ] {
            assert!(is_correction(sample), "expected match for: {sample}");
        }
        for sample in [
            "deja de lado ese enfoque por ahora",
            "deja de lado los warnings y céntrate en el error",
            "siempre que pulses el botón, se abre el menú",
            "explica qué ocurre siempre que expira el token",
        ] {
            assert!(!is_correction(sample), "unexpected match for: {sample}");
        }
    }

    #[test]
    fn guards_demonstrative_failure_blame() {
        for sample in [
            "this fails on every run",
            "that breaks the retry logic",
            "this causes a race in the scheduler",
            "that doesn't work with PostgreSQL",
        ] {
            assert!(is_correction(sample), "expected match for: {sample}");
        }
        for sample in [
            "the test that fails on CI is the Unicode one",
            "the path that breaks Windows contains a colon",
            "a flag that causes contention is enabled",
            "the integration test that doesn't work on ARM is skipped",
            "check if this fails on Windows",
            "if that breaks the parser, add a regression test",
            "when this causes a timeout, increase the limit",
            "retry so this fails fast instead of hanging",
            "tell me whether that doesn't work with SQLite",
            "explain what this causes in the allocator",
            "investigate why this fails only under load",
        ] {
            assert!(!is_correction(sample), "unexpected match for: {sample}");
        }
    }

    #[test]
    fn requires_complements_after_reference_discourse_markers() {
        for sample in [
            "no es lo que quería",
            "no es lo que te pedí",
            "eso no es lo que esperaba",
        ] {
            assert!(is_correction(sample), "expected match for: {sample}");
        }
        for sample in [
            "no es lo que",
            "verifica que no es lo que hace el parser con UTF-8",
            "no es lo que hace la función, sino cómo la probamos",
            "al contrario de la versión anterior, este parser es incremental",
            "explica si es al contrario en little-endian",
            "el orden es al contrario que en Python",
        ] {
            assert!(!is_correction(sample), "unexpected match for: {sample}");
        }
    }

    #[test]
    fn suppresses_weak_phrases_inside_questions_and_conditionals() {
        for sample in [
            "¿qué está mal en este bloque?",
            "check if the date is wrong before exporting",
            "si el render sale mal, reintenta una vez",
            "mira si el checksum está mal en el manifiesto",
            "if the config is backwards, flip the flag",
            "si el cron no debe correr en staging, documéntalo",
        ] {
            assert!(!is_correction(sample), "unexpected match for: {sample}");
        }
        for sample in [
            "el despliegue salió mal otra vez",
            "esa validación está mal",
            "el campo secreto no debe exponerse en la API",
        ] {
            assert!(is_correction(sample), "expected match for: {sample}");
        }
    }
}
