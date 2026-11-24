//! Busca nas mensagens das conversas: um resultado por conversa, com o trecho em volta do termo.

use serde::Serialize;

/// Caracteres de cada lado do termo no trecho mostrado.
const SNIPPET_RADIUS: usize = 60;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MessageHit {
    pub conversation_id: String,
    pub title: String,
    pub snippet: String,
    pub created_at: i64,
}

fn fold(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
}

/// Trecho do texto em volta da primeira ocorrência do termo (sem diferenciar maiúsculas), numa linha só.
pub fn snippet(text: &str, query: &str) -> String {
    let chars: Vec<char> = text.split_whitespace().collect::<Vec<_>>().join(" ").chars().collect();
    let needle: Vec<char> = query.trim().chars().map(fold).collect();
    let at = (!needle.is_empty())
        .then(|| chars.windows(needle.len()).position(|w| w.iter().map(|c| fold(*c)).eq(needle.iter().copied())))
        .flatten()
        .unwrap_or(0);
    let start = at.saturating_sub(SNIPPET_RADIUS);
    let end = (at + needle.len() + SNIPPET_RADIUS).min(chars.len());
    let mut out: String = chars[start..end].iter().collect();
    if start > 0 {
        out.insert(0, '…');
    }
    if end < chars.len() {
        out.push('…');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snippet_centers_on_the_term_ignoring_case() {
        let text = format!("{} Exportar   em\nCSV {}", "a".repeat(100), "b".repeat(100));
        let s = snippet(&text, "exportar em csv");
        assert!(s.starts_with('…') && s.ends_with('…'));
        assert!(s.contains("Exportar em CSV"));
        assert_eq!(snippet("curto", "zzz"), "curto");
    }
}
