use serde::Serialize;

/// The classified intent of a user query.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub enum QueryIntent {
    /// "summarize", "what is this about", "overview", "explain"
    Summarize,
    /// "who", "person", "author", "company", "name"
    Entity,
    /// "what is", "how does", "define", "when", "where"
    Factual,
    /// "compare", "difference", "vs", "better"
    Comparison,
    /// "list all", "what are the", "features", "enumerate"
    ListExtract,
    /// Anything with specific domain terms
    Specific,
}

/// Result of query preprocessing — search terms, intent, hints for the agent.
#[derive(Debug, Clone, Serialize)]
pub struct ProcessedQuery {
    pub original: String,
    pub intent: QueryIntent,
    pub search_terms: Vec<String>,
    pub exploration_hint: String,
    pub min_tool_calls: u32,
    /// Adaptive budget: recommended max_steps for this query
    pub recommended_max_steps: u32,
}

/// Estimate query complexity from word count and search term diversity.
fn estimate_complexity(query: &str, search_terms: &[String], intent: &QueryIntent) -> u32 {
    let word_count = query.split_whitespace().count();
    let term_count = search_terms.len();

    // Base complexity from intent
    let base = match intent {
        QueryIntent::Summarize => 3,
        QueryIntent::Comparison => 4,
        QueryIntent::ListExtract => 3,
        QueryIntent::Entity => 2,
        QueryIntent::Factual => 2,
        QueryIntent::Specific => 2,
    };

    // Boost for longer queries (more complex questions)
    let length_boost = if word_count > 20 { 2 } else if word_count > 10 { 1 } else { 0 };

    // Boost for many distinct search terms
    let term_boost = if term_count > 4 { 2 } else if term_count > 2 { 1 } else { 0 };

    base + length_boost + term_boost
}

/// Preprocess a user query: classify intent, extract search terms, generate hints.
pub fn preprocess_query(query: &str) -> ProcessedQuery {
    let intent = classify_intent(query);
    let search_terms = extract_search_terms(query);
    let exploration_hint = build_exploration_hint(&intent, &search_terms);
    let min_tool_calls = match intent {
        QueryIntent::Summarize => 3,
        QueryIntent::Entity => 2,
        QueryIntent::Comparison => 4,
        QueryIntent::ListExtract => 3,
        QueryIntent::Factual => 2,
        QueryIntent::Specific => 2,
    };

    let complexity = estimate_complexity(query, &search_terms, &intent);
    // Map complexity to max_steps: clamp between 6 and 15
    let recommended_max_steps = (complexity * 2 + 4).clamp(6, 15);

    ProcessedQuery {
        original: query.to_string(),
        intent,
        search_terms,
        exploration_hint,
        min_tool_calls,
        recommended_max_steps,
    }
}

fn classify_intent(query: &str) -> QueryIntent {
    let lower = query.to_lowercase();

    // Summarization
    if lower.contains("summarize")
        || lower.contains("summary")
        || lower.contains("what is this about")
        || lower.contains("what does this")
        || lower.contains("what is this file")
        || lower.contains("overview")
        || lower.contains("explain this")
        || lower.contains("tell me about")
        || lower.contains("describe this")
    {
        return QueryIntent::Summarize;
    }

    // Entity
    if lower.contains("who is")
        || lower.contains("who are")
        || lower.contains("person")
        || lower.contains("author")
        || lower.contains("name of")
        || lower.contains("mentioned")
        || lower.contains("company")
        || lower.contains("organization")
    {
        return QueryIntent::Entity;
    }

    // Comparison
    if lower.contains("compare")
        || lower.contains("difference")
        || lower.contains("differ")
        || lower.contains(" vs ")
        || lower.contains("versus")
        || lower.contains("better than")
    {
        return QueryIntent::Comparison;
    }

    // List extraction
    if lower.contains("list all")
        || lower.contains("list the")
        || lower.contains("what are the")
        || lower.contains("enumerate")
        || lower.contains("features")
        || lower.contains("all the")
    {
        return QueryIntent::ListExtract;
    }

    // Factual
    if lower.starts_with("what")
        || lower.starts_with("how")
        || lower.starts_with("when")
        || lower.starts_with("where")
        || lower.starts_with("why")
        || lower.starts_with("define")
        || lower.starts_with("is there")
    {
        return QueryIntent::Factual;
    }

    QueryIntent::Specific
}

/// Extract meaningful search terms from the query (stop words removed).
fn extract_search_terms(query: &str) -> Vec<String> {
    let stop_words: &[&str] = &[
        "a", "an", "the", "is", "are", "was", "were", "be", "been", "being",
        "have", "has", "had", "do", "does", "did", "will", "would", "could",
        "should", "may", "might", "can", "shall", "to", "of", "in", "for",
        "on", "with", "at", "by", "from", "as", "into", "about", "between",
        "through", "during", "before", "after", "above", "below", "and", "or",
        "but", "not", "no", "nor", "so", "if", "than", "that", "this", "it",
        "its", "what", "which", "who", "whom", "how", "when", "where", "why",
        "all", "each", "every", "both", "few", "more", "most", "some", "any",
        "me", "my", "we", "our", "you", "your", "he", "him", "his", "she",
        "her", "they", "them", "their", "i", "tell", "explain", "describe",
        "summarize", "summary", "please", "file", "document", "mentioned",
        "list", "give", "show", "find", "get",
    ];

    query
        .split(|c: char| !c.is_alphanumeric() && c != '-' && c != '_')
        .map(|w| w.trim().to_lowercase())
        .filter(|w| w.len() > 2 && !stop_words.contains(&w.as_str()))
        .collect()
}

/// Generate an exploration hint based on query intent and search terms.
fn build_exploration_hint(intent: &QueryIntent, search_terms: &[String]) -> String {
    let search_hint = if search_terms.is_empty() {
        String::new()
    } else {
        format!(
            "\nRecommended search terms: {}",
            search_terms.iter().map(|t| format!("\"{}\"", t)).collect::<Vec<_>>().join(", ")
        )
    };

    match intent {
        QueryIntent::Summarize => format!(
            "QUERY TYPE: Summarization — expand the top 3-5 most important sections to read their full content before answering.{}",
            search_hint
        ),
        QueryIntent::Entity => format!(
            "QUERY TYPE: Entity lookup — use search_content to find names, people, companies, or organizations. Then expand matching nodes to get full context.{}",
            search_hint
        ),
        QueryIntent::Factual => format!(
            "QUERY TYPE: Factual question — search for the specific topic first, then expand relevant sections to extract the answer.{}",
            search_hint
        ),
        QueryIntent::Comparison => format!(
            "QUERY TYPE: Comparison — identify the items being compared, expand their respective sections, and present differences side by side.{}",
            search_hint
        ),
        QueryIntent::ListExtract => format!(
            "QUERY TYPE: List extraction — find and expand the section(s) that contain the items to list. Read their full content.{}",
            search_hint
        ),
        QueryIntent::Specific => format!(
            "QUERY TYPE: Specific question — search for relevant keywords, then expand matching sections.{}",
            search_hint
        ),
    }
}
