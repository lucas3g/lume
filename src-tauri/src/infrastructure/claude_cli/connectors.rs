//! Conectores que o próprio Claude Code carrega para uma conta: os da conta
//! claude.ai (web), os configurados no terminal e os de plugins.

use super::{blocking_command, run_cli, run_cli_login};
use crate::application::{ports::AccountConnectors, AppError, AppResult};
use crate::domain::extension::{parse_mcp_list, AccountConnector, ConnectorStatus};
use serde_json::{json, Map, Value};
use std::{
    collections::{HashMap, HashSet},
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::Stdio,
    sync::mpsc,
    time::Duration,
};

const LIST_TIMEOUT: Duration = Duration::from_secs(90);
const LOGIN_TIMEOUT: Duration = Duration::from_secs(300);

/// Onde os conectores da conta claude.ai são vinculados e desvinculados.
const CLAUDE_AI_CONNECTORS_URL: &str = "https://claude.ai/customize/connectors";
/// Arquivo em que o CLI guarda, por 15 minutos, os conectores que pediram login; as
/// conversas pulam esses conectores mesmo depois que o usuário os vincula.
const NEEDS_AUTH_CACHE: &str = "mcp-needs-auth-cache.json";
/// Quanto esperar a situação dos conectores vista por uma conversa. Costuma levar 2 s; a lista
/// espera por ela, então um conector lento não pode segurar a tela.
const SESSION_STATUS_TIMEOUT: Duration = Duration::from_secs(20);
/// Quanto esperar o CLI sair sozinho depois de responder.
const EXIT_GRACE: Duration = Duration::from_secs(5);

pub struct ClaudeCliConnectors {
    cli: PathBuf,
}

impl ClaudeCliConnectors {
    pub fn new(cli: PathBuf) -> Self {
        ClaudeCliConnectors { cli }
    }
}

impl AccountConnectors for ClaudeCliConnectors {
    fn list(&self, config_dir: Option<&str>) -> AppResult<Vec<AccountConnector>> {
        let cache = config_root(config_dir).map(|dir| dir.join(NEEDS_AUTH_CACHE));
        // Lido antes da limpeza abaixo: quem está no cache aparece como "precisa de login" na conversa.
        let cached = cache.as_deref().map(cached_needs_auth).unwrap_or_default();
        // `mcp list` testa cada conector; a conversa diz quais chegam ao assistente. Em paralelo.
        let (text, session) = std::thread::scope(|scope| {
            let session = scope.spawn(|| session_status(&self.cli, config_dir));
            let text = run_cli(&self.cli, config_dir, &["mcp", "list"], LIST_TIMEOUT, "A verificação dos conectores demorou demais.");
            let session = session.join().unwrap_or_else(|_| Err("a verificação parou com erro".into()));
            (text, session)
        });
        // Sem a visão da conversa, a lista fica como o `mcp list` disse.
        let session = session.inspect_err(|e| eprintln!("[conectores] situação vista pelas conversas indisponível: {e}")).ok();
        let mut list = parse_mcp_list(&text?);
        if let Some(session) = &session {
            apply_session_view(&mut list, session, &cached);
        }
        // As conversas confiam no cache: um conector conectado que ficou nele não chegaria
        // ao assistente até o cache vencer.
        let connected: Vec<&str> = list.iter().filter(|c| c.status == ConnectorStatus::Connected).map(|c| c.name.as_str()).collect();
        if let Some(path) = &cache {
            forget_needs_auth(path, &connected);
        }
        Ok(list)
    }

    /// Abre o login do conector no navegador e espera a conclusão.
    fn login(&self, config_dir: Option<&str>, name: &str) -> AppResult<()> {
        run_cli_login(&self.cli, config_dir, &["mcp", "login", name], LOGIN_TIMEOUT, "O login não foi concluído em 5 minutos.").map(|_| ())
    }

    /// Conectores da conta claude.ai só se desvinculam no próprio claude.ai (o CLI não
    /// mexe neles): abre a página de conectores no navegador.
    fn logout(&self, config_dir: Option<&str>, name: &str) -> AppResult<()> {
        if is_claude_ai(name) {
            return tauri_plugin_opener::open_url(CLAUDE_AI_CONNECTORS_URL, None::<&str>)
                .map_err(|e| AppError(format!("Não foi possível abrir {CLAUDE_AI_CONNECTORS_URL}: {e}")));
        }
        run_cli(&self.cli, config_dir, &["mcp", "logout", name], LIST_TIMEOUT, "A operação demorou demais.").map(|_| ())
    }
}

/// Situação de cada conector como uma conversa a vê, pelo protocolo de controle do CLI (sem
/// chamar o modelo nem gravar sessão). O erro diz por que não deu para saber.
fn session_status(cli: &Path, config_dir: Option<&str>) -> Result<HashMap<String, String>, String> {
    let mut cmd = blocking_command(cli, config_dir);
    cmd.env("MCP_CONNECTION_NONBLOCKING", "false")
        .args(["-p", "--input-format", "stream-json", "--output-format", "stream-json", "--verbose", "--tools", "", "--no-session-persistence"])
        .current_dir(std::env::temp_dir())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    // Grupo próprio: se for preciso matar, os servidores locais que o CLI abriu vão junto.
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut cmd, 0);
    let mut child = cmd.spawn().map_err(|e| format!("não abriu o Claude Code: {e}"))?;
    let (Some(mut stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
        kill_tree(&mut child);
        return Err("sem entrada ou saída do Claude Code".into());
    };
    let request = |id: &str, subtype: &str| json!({ "type": "control_request", "request_id": id, "request": { "subtype": subtype } }).to_string();
    // A entrada fica aberta até a resposta: fechada, o CLI poderia sair antes de responder.
    let sent = writeln!(stdin, "{}\n{}", request("lume-init", "initialize"), request("lume-status", "mcp_status")).and_then(|_| stdin.flush());
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let found = BufReader::new(stdout).lines().map_while(Result::ok).find_map(|line| parse_session_status(&line));
        let _ = tx.send(found);
    });
    let status = match sent {
        Err(e) => Err(format!("não deu para escrever no Claude Code: {e}")),
        Ok(()) => match rx.recv_timeout(SESSION_STATUS_TIMEOUT) {
            Ok(Some(status)) => Ok(status),
            Ok(None) => Err("o Claude Code saiu sem responder ao mcp_status".into()),
            Err(_) => Err(format!("sem resposta ao mcp_status em {} s", SESSION_STATUS_TIMEOUT.as_secs())),
        },
    };
    // Com a entrada fechada o CLI encerra sozinho, levando os servidores locais que abriu;
    // só é morto (com o grupo todo) se demorar.
    drop(stdin);
    let started = std::time::Instant::now();
    while !matches!(child.try_wait(), Ok(Some(_))) {
        if started.elapsed() > EXIT_GRACE {
            kill_tree(&mut child);
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    status
}

/// Mata o CLI e os processos que ele abriu. Assim a saída fecha e a leitura em segundo plano termina.
fn kill_tree(child: &mut std::process::Child) {
    let pid = child.id().to_string();
    #[cfg(unix)]
    let _ = std::process::Command::new("kill").args(["-KILL", &format!("-{pid}")]).status();
    #[cfg(windows)]
    let _ = crate::infrastructure::hide_console(std::process::Command::new("taskkill").args(["/T", "/F", "/PID", &pid])).status();
    let _ = child.kill();
    let _ = child.wait();
}

/// Resposta ao pedido `mcp_status`: nome do conector → situação ("connected", "needs-auth"…).
fn parse_session_status(line: &str) -> Option<HashMap<String, String>> {
    let value: Value = serde_json::from_str(line).ok()?;
    let response = value.get("response").filter(|r| r["request_id"] == "lume-status")?;
    let servers = response["response"]["mcpServers"].as_array()?;
    Some(
        servers
            .iter()
            .filter_map(|s| Some((s["name"].as_str()?.to_owned(), s["status"].as_str()?.to_owned())))
            .collect(),
    )
}

/// Conector da conta que o `mcp list` dá como conectado, mas que a conversa trata como "precisa
/// de login": falta vinculá-lo. O `mcp list` só testa se o servidor responde, e alguns (os do
/// Google) respondem sem login; a conversa segue o claude.ai, que diz se ele foi vinculado. A
/// exceção é quem está no cache do CLI: vinculado há pouco, a conversa ainda não o tentou.
fn apply_session_view(list: &mut [AccountConnector], session: &HashMap<String, String>, cached: &HashSet<String>) {
    for c in list.iter_mut() {
        let needs_auth = session.get(&c.name).is_some_and(|s| s == "needs-auth");
        if c.status == ConnectorStatus::Connected && is_claude_ai(&c.name) && needs_auth && !cached.contains(&c.name) {
            c.status = ConnectorStatus::NeedsAuth;
        }
    }
}

/// Conectores no cache de "precisa de login" do CLI.
fn cached_needs_auth(path: &Path) -> HashSet<String> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<Map<String, Value>>(&text).ok())
        .map(|cache| cache.into_iter().map(|(name, _)| name).collect())
        .unwrap_or_default()
}

fn is_claude_ai(name: &str) -> bool {
    name.starts_with("claude.ai ")
}

/// Pasta de configuração do CLI para a conta (`None` = a do computador, `~/.claude`).
fn config_root(config_dir: Option<&str>) -> Option<PathBuf> {
    match config_dir {
        Some(dir) => Some(PathBuf::from(dir)),
        None => dirs::home_dir().map(|home| home.join(".claude")),
    }
}

/// Tira do cache de "precisa de login" os conectores que já estão conectados. O arquivo
/// é só um cache do CLI: qualquer falha ao lê-lo ou gravá-lo é ignorada.
fn forget_needs_auth(path: &Path, connected: &[&str]) {
    let Ok(text) = std::fs::read_to_string(path) else { return };
    let Ok(Value::Object(mut cache)) = serde_json::from_str::<Value>(&text) else { return };
    if !remove_keys(&mut cache, connected) {
        return;
    }
    let Ok(json) = serde_json::to_string(&Value::Object(cache)) else { return };
    // Grava ao lado e troca de uma vez: o CLI nunca lê um arquivo pela metade.
    let tmp = path.with_extension("json.lume-tmp");
    if std::fs::write(&tmp, json).is_ok() && std::fs::rename(&tmp, path).is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
}

/// Remove as chaves; diz se alguma saiu.
fn remove_keys(cache: &mut Map<String, Value>, keys: &[&str]) -> bool {
    keys.iter().fold(false, |changed, key| cache.remove(*key).is_some() || changed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connected_connectors_leave_the_needs_auth_cache() {
        let dir = std::env::temp_dir().join(format!("lume-needs-auth-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(NEEDS_AUTH_CACHE);
        std::fs::write(&path, r#"{"claude.ai Atlassian":{"timestamp":1,"id":"a"},"claude.ai Notion":{"timestamp":2,"id":"n"}}"#).unwrap();
        forget_needs_auth(&path, &["claude.ai Atlassian", "claude.ai Gmail"]);
        let cache: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert!(cache.get("claude.ai Atlassian").is_none());
        assert_eq!(cache["claude.ai Notion"]["id"], "n", "quem ainda pede login continua no cache");
        // Sem nada a tirar, o arquivo não é regravado; sem arquivo, nada acontece.
        forget_needs_auth(&path, &["claude.ai Gmail"]);
        forget_needs_auth(&dir.join("nao-existe.json"), &["x"]);
        assert!(!dir.join("nao-existe.json").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn reads_connector_status_seen_by_a_conversation() {
        let line = r#"{"type":"control_response","response":{"subtype":"success","request_id":"lume-status","response":{"mcpServers":[{"name":"claude.ai Gmail","status":"needs-auth"},{"name":"claude.ai Atlassian","status":"connected"}]}}}"#;
        let status = parse_session_status(line).unwrap();
        assert_eq!(status["claude.ai Gmail"], "needs-auth");
        assert_eq!(status["claude.ai Atlassian"], "connected");
        assert!(parse_session_status(r#"{"type":"control_response","response":{"request_id":"lume-init","response":{}}}"#).is_none());
        assert!(parse_session_status("não é json").is_none());
    }

    #[test]
    fn connected_only_for_mcp_list_needs_linking() {
        let make = |name: &str, status| AccountConnector {
            name: name.into(),
            display_name: name.into(),
            key: String::new(),
            endpoint: String::new(),
            status,
            source: crate::domain::extension::ConnectorSource::Account,
            enabled: true,
        };
        let mut list = vec![
            make("claude.ai Gmail", ConnectorStatus::Connected),
            make("claude.ai Atlassian", ConnectorStatus::Connected),
            make("claude.ai Notion", ConnectorStatus::NeedsAuth),
            make("claude.ai Runway", ConnectorStatus::Connected),
        ];
        let session: HashMap<String, String> = [("claude.ai Gmail", "needs-auth"), ("claude.ai Atlassian", "needs-auth"), ("claude.ai Notion", "needs-auth"), ("claude.ai Runway", "connected")]
            .into_iter()
            .map(|(n, s)| (n.to_owned(), s.to_owned()))
            .collect();
        // O Atlassian só aparece bloqueado por estar no cache (vinculado há pouco): continua conectado.
        let cached: HashSet<String> = ["claude.ai Atlassian".to_owned()].into();
        apply_session_view(&mut list, &session, &cached);
        let status: Vec<ConnectorStatus> = list.iter().map(|c| c.status).collect();
        assert_eq!(status, [ConnectorStatus::NeedsAuth, ConnectorStatus::Connected, ConnectorStatus::NeedsAuth, ConnectorStatus::Connected]);
    }

    /// Com o Claude Code instalado e logado: a conversa informa a situação dos conectores.
    #[test]
    #[ignore = "usa o Claude Code real"]
    fn real_cli_reports_connector_status_seen_by_conversations() {
        let cli = super::super::find_cli().expect("Claude Code instalado");
        let config_dir = std::env::var("CLAUDE_CONFIG_DIR").ok();
        let started = std::time::Instant::now();
        let status = session_status(&cli, config_dir.as_deref()).unwrap();
        assert!(!status.is_empty(), "nenhum conector informado");
        assert!(started.elapsed() < SESSION_STATUS_TIMEOUT + EXIT_GRACE);
    }

    #[test]
    fn only_claude_ai_connectors_unlink_on_the_site() {
        assert!(is_claude_ai("claude.ai Atlassian"));
        assert!(!is_claude_ai("plugin:context7:context7"));
        assert!(!is_claude_ai("rea"));
    }
}
