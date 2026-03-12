use serde::Serialize;

use crate::llm::local;

/// Result of a single local-model enrichment step.
/// Tokens are always 0 — the local sidecar has no token counting.
#[derive(Debug, Clone)]
pub struct EnrichmentResult {
    pub text: String,
    pub input_tokens: u32,
    pub output_tokens: u32,
}

/// Rewrite the user query into a clearer, search-optimized form using the local model.
/// Returns Err if the local sidecar is not running (non-fatal — pipeline continues).
pub fn rewrite_query(query: &str) -> Result<EnrichmentResult, String> {
    let system = "Rewrite the query into concise search terms. Expand single words like 'summarize' into a full question. Output ONLY the rewritten query.";
    let text = local::chat_inference(system, query, 80)?;
    Ok(EnrichmentResult { text: text.trim().to_string(), input_tokens: 0, output_tokens: 0 })
}

/// Generate a hypothetical document passage that would answer the query (HyDE technique).
/// The generated passage is used to extract richer search terms that match document language.
/// Returns Err if the local sidecar is not running (non-fatal — pipeline continues).
pub fn generate_hyde(query: &str) -> Result<EnrichmentResult, String> {
    let system = "Write a short factual paragraph (2-3 sentences) a document would contain to answer this query. Use specific terminology. Output ONLY the paragraph.";
    let text = local::chat_inference(system, query, 150)?;
    Ok(EnrichmentResult { text: text.trim().to_string(), input_tokens: 0, output_tokens: 0 })
}

/// Generate a broader, more general version of the query (StepBack prompting technique).
/// The broader question helps retrieve contextual information the specific query might miss.
/// Returns Err if the local sidecar is not running (non-fatal — pipeline continues).
pub fn stepback_query(query: &str) -> Result<EnrichmentResult, String> {
    let system = "Generate a single broader question that provides background context for this query. Output ONLY the broader question.";
    let text = local::chat_inference(system, query, 80)?;
    Ok(EnrichmentResult { text: text.trim().to_string(), input_tokens: 0, output_tokens: 0 })
}

/// Extract search terms from arbitrary text. Public wrapper used by the enrichment pipeline
/// to pull search terms from LLM-generated text (rewritten queries, HyDE passages, etc.).
pub fn extract_terms_from_text(text: &str) -> Vec<String> {
    extract_search_terms(text)
}

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

#[cfg(test)]
mod tests {
    use super::*;

    // ── classify_intent tests (via preprocess_query) ──────────────────

    #[test]
    fn intent_summarize() {
        let pq = preprocess_query("summarize this document");
        assert_eq!(pq.intent, QueryIntent::Summarize);
    }

    #[test]
    fn intent_entity() {
        let pq = preprocess_query("who is the author");
        assert_eq!(pq.intent, QueryIntent::Entity);
    }

    #[test]
    fn intent_comparison() {
        let pq = preprocess_query("compare A vs B");
        assert_eq!(pq.intent, QueryIntent::Comparison);
    }

    #[test]
    fn intent_list_extract() {
        let pq = preprocess_query("list all features");
        assert_eq!(pq.intent, QueryIntent::ListExtract);
    }

    #[test]
    fn intent_factual() {
        let pq = preprocess_query("what is the main topic");
        assert_eq!(pq.intent, QueryIntent::Factual);
    }

    #[test]
    fn intent_specific() {
        let pq = preprocess_query("specific technical term query");
        assert_eq!(pq.intent, QueryIntent::Specific);
    }

    // ── extract_search_terms tests (via preprocess_query.search_terms) ─

    #[test]
    fn search_terms_removes_stop_words() {
        let pq = preprocess_query("what is the main topic of this document");
        // "what", "is", "the", "of", "this" are stop words
        assert!(!pq.search_terms.contains(&"what".to_string()));
        assert!(!pq.search_terms.contains(&"is".to_string()));
        assert!(!pq.search_terms.contains(&"the".to_string()));
        assert!(!pq.search_terms.contains(&"of".to_string()));
        assert!(!pq.search_terms.contains(&"this".to_string()));
    }

    #[test]
    fn search_terms_removes_short_words() {
        let pq = preprocess_query("an ox is by me");
        // "an", "ox", "is", "by", "me" — all <=2 chars or stop words
        for term in &pq.search_terms {
            assert!(term.len() > 2, "short word '{}' should have been removed", term);
        }
    }

    #[test]
    fn search_terms_lowercased() {
        let pq = preprocess_query("Rust Async Runtime Architecture");
        for term in &pq.search_terms {
            assert_eq!(
                term,
                &term.to_lowercase(),
                "term '{}' should be lowercased",
                term
            );
        }
    }

    // ── preprocess_query integration tests ────────────────────────────

    #[test]
    fn recommended_max_steps_in_range() {
        let queries = [
            "summarize this document",
            "who is the author",
            "compare A vs B",
            "list all features",
            "what is the main topic",
            "specific technical term query",
        ];
        for q in &queries {
            let pq = preprocess_query(q);
            assert!(
                pq.recommended_max_steps >= 6 && pq.recommended_max_steps <= 15,
                "query '{}' has recommended_max_steps={}, expected 6..=15",
                q,
                pq.recommended_max_steps
            );
        }
    }

    #[test]
    fn min_tool_calls_matches_intent() {
        // Summarize and ListExtract typically need more tool calls than Factual/Specific
        let summarize = preprocess_query("summarize this document");
        let factual = preprocess_query("what is the main topic");
        assert!(
            summarize.min_tool_calls >= factual.min_tool_calls,
            "Summarize min_tool_calls ({}) should be >= Factual ({})",
            summarize.min_tool_calls,
            factual.min_tool_calls
        );

        let comparison = preprocess_query("compare A vs B");
        assert!(
            comparison.min_tool_calls >= factual.min_tool_calls,
            "Comparison min_tool_calls ({}) should be >= Factual ({})",
            comparison.min_tool_calls,
            factual.min_tool_calls
        );
    }

    #[test]
    fn exploration_hint_contains_query_type() {
        let queries = [
            "summarize this document",
            "who is the author",
            "compare A vs B",
            "list all features",
            "what is the main topic",
            "specific technical term query",
        ];
        for q in &queries {
            let pq = preprocess_query(q);
            assert!(
                pq.exploration_hint.contains("QUERY TYPE:"),
                "exploration_hint for '{}' should contain 'QUERY TYPE:', got: {}",
                q,
                pq.exploration_hint
            );
        }
    }
}
