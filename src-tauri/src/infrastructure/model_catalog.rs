//! Modelos que os CLIs do ChatGPT (Codex) e do Gemini (Antigravity) dizem aceitar.
//! A lista do Claude fica com a interface, que já a usa no seletor das conversas.

use super::{antigravity_cli, codex_cli, providers::cli_command};
use crate::application::{ports::ModelCatalog, AppError, AppResult};
use crate::domain::account::Provider;
use async_trait::async_trait;
use serde_json::Value;
use std::{path::Path, process::Stdio, time::Duration};

const LIST_TIMEOUT: Duration = Duration::from_secs(30);

pub struct CliModels;

#[async_trait]
impl ModelCatalog for CliModels {
    async fn models(&self, provider: Provider) -> AppResult<Vec<String>> {
        match provider {
            Provider::Claude => Ok(vec![]),
            // Comando de diagnóstico do Codex: imprime o catálogo de modelos em JSON.
            Provider::ChatGpt => {
                let cli = codex_cli::find_cli().ok_or_else(|| AppError("O Codex não está instalado neste computador.".into()))?;
                let out = run(&cli, &["debug", "models"]).await?;
                Ok(parse_codex(&out))
            }
            Provider::Gemini => {
                let cli = antigravity_cli::find_cli().ok_or_else(|| AppError("O Antigravity (agy) não está instalado neste computador.".into()))?;
                Ok(parse_lines(&run(&cli, &["models"]).await?))
            }
        }
    }
}

async fn run(cli: &Path, args: &[&str]) -> AppResult<String> {
    let child = cli_command(cli).args(args).stdin(Stdio::null()).output();
    let output = tokio::time::timeout(LIST_TIMEOUT, child)
        .await
        .map_err(|_| AppError("O CLI demorou demais para listar os modelos.".into()))?
        .map_err(|e| AppError(format!("Não foi possível executar o CLI: {e}")))?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr);
        return Err(AppError(format!("O CLI não listou os modelos: {}", detail.trim())));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Identificadores de modelo no JSON do Codex (campos `slug`, `id` ou `model` dos objetos).
/// Se a saída não for JSON, lê como lista de linhas.
fn parse_codex(text: &str) -> Vec<String> {
    let Ok(value) = serde_json::from_str::<Value>(text) else { return parse_lines(text) };
    let mut out = vec![];
    collect(&value, &mut out);
    out
}

fn collect(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::Array(items) => items.iter().for_each(|v| collect(v, out)),
        Value::Object(map) => {
            let id = ["slug", "id", "model"].iter().find_map(|k| map.get(*k).and_then(Value::as_str));
            match id {
                Some(id) if looks_like_model(id) => push(out, id),
                _ => map.values().for_each(|v| collect(v, out)),
            }
        }
        _ => {}
    }
}

/// Uma linha por modelo, o identificador no começo (marcadores e cabeçalhos são ignorados).
fn parse_lines(text: &str) -> Vec<String> {
    let mut out = vec![];
    for line in text.lines() {
        let line = line.trim().trim_start_matches(['-', '*', '•', ' ']);
        if let Some(token) = line.split_whitespace().next().map(|t| t.trim_end_matches([':', ',']))
            && looks_like_model(token)
        {
            push(&mut out, token);
        }
    }
    out
}

/// Identificador de modelo: letras, dígitos e `.-_:/`, com hífen ou dígito (palavras soltas
/// de cabeçalho, como "Available", ficam de fora).
fn looks_like_model(token: &str) -> bool {
    token.chars().next().is_some_and(|c| c.is_ascii_alphanumeric())
        && token.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | ':' | '/'))
        && token.chars().any(|c| c == '-' || c.is_ascii_digit())
}

fn push(out: &mut Vec<String>, id: &str) {
    if !out.iter().any(|m| m == id) {
        out.push(id.to_owned());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_codex_catalog_and_plain_lists() {
        let json = r#"{"models":[{"slug":"gpt-5.5-codex","display_name":"GPT-5.5 Codex"},{"slug":"gpt-5.5-mini"},{"slug":"gpt-5.5-codex"}]}"#;
        assert_eq!(parse_codex(json), ["gpt-5.5-codex", "gpt-5.5-mini"]);
        assert_eq!(parse_codex(r#"[{"id":"o9"},{"model":"gpt-6"}]"#), ["o9", "gpt-6"]);
        let text = "Available models:\n  - gemini-3.8-pro-high (default)\n  * gemini-3.8-flash-medium\n\nUse --model to pick one.";
        assert_eq!(parse_lines(text), ["gemini-3.8-pro-high", "gemini-3.8-flash-medium"]);
        assert_eq!(parse_codex(text), ["gemini-3.8-pro-high", "gemini-3.8-flash-medium"]);
    }
}
