//! Conversas anteriores importadas de outros aplicativos neste computador.

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct HistSession {
    pub id: String,
    pub source: String,
    pub cwd: Option<String>,
    pub title: Option<String>,
    pub mtime: i64,
    pub msg_count: i64,
}

/// Trecho relevante de uma conversa anterior, usado como contexto.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Snippet {
    pub session_id: String,
    pub title: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct HistoryStatus {
    pub sessions: i64,
    pub projects: i64,
    pub desktop_sessions: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DialogueTurn {
    pub role: String,
    pub text: String,
}

/// Diálogo de uma sessão importada, sem ferramentas nem avisos do sistema.
#[derive(Debug, Clone, Default)]
pub struct ImportedSession {
    pub cwd: Option<String>,
    pub title: Option<String>,
    pub turns: Vec<DialogueTurn>,
}

/// Texto anexado ao prompt de sistema com os trechos encontrados.
pub fn context_prompt(snippets: &[Snippet]) -> Option<String> {
    if snippets.is_empty() {
        return None;
    }
    let mut out = String::from(
        "Trechos de conversas anteriores do usuário neste computador, que podem ser relevantes. \
         Use-os apenas se ajudarem a responder; não os mencione se não forem úteis.\n",
    );
    for s in snippets {
        out.push_str(&format!("\n[{}]\n{}\n", s.title, s.text));
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_prompt_lists_each_snippet() {
        assert_eq!(context_prompt(&[]), None);
        let p = context_prompt(&[Snippet { session_id: "1".into(), title: "Índices".into(), text: "use btree".into() }])
            .unwrap();
        assert!(p.contains("[Índices]\nuse btree"));
    }
}
