//! Tipos de conta: leva cada pedido ao CLI do serviço da conta (Claude Code, Codex ou
//! Antigravity) e reúne o que os CLIs que não são do Claude têm em comum.

use super::claude_cli::shell_path;
use crate::application::{
    ports::{AccountGateway, ChatEngine, TurnListener, UsageProbe},
    AppResult, TurnControl,
};
use crate::domain::{
    account::{Identity, Provider, UsageLimit},
    chat::{Attachment, AttachmentKind, RateInfo, TurnEnd, TurnRequest},
};
use async_trait::async_trait;
use base64::{engine::general_purpose::STANDARD, Engine};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::process::Command;

/// Uma implementação por tipo de conta.
pub struct ByProvider<T: ?Sized> {
    pub claude: Arc<T>,
    pub chatgpt: Arc<T>,
    pub gemini: Arc<T>,
}

impl<T: ?Sized> ByProvider<T> {
    fn of(&self, provider: Provider) -> &T {
        match provider {
            Provider::Claude => &self.claude,
            Provider::ChatGpt => &self.chatgpt,
            Provider::Gemini => &self.gemini,
        }
    }
}

#[async_trait]
impl AccountGateway for ByProvider<dyn AccountGateway> {
    fn installed(&self, provider: Provider) -> bool {
        self.of(provider).installed(provider)
    }
    fn prepare_profile(&self, provider: Provider, account_id: &str) -> AppResult<String> {
        self.of(provider).prepare_profile(provider, account_id)
    }
    fn discard_profile(&self, provider: Provider, config_dir: &str) {
        self.of(provider).discard_profile(provider, config_dir)
    }
    async fn identity(&self, provider: Provider, config_dir: Option<&str>) -> Identity {
        self.of(provider).identity(provider, config_dir).await
    }
    async fn login(&self, provider: Provider, config_dir: &str) -> AppResult<Identity> {
        self.of(provider).login(provider, config_dir).await
    }
}

#[async_trait]
impl UsageProbe for ByProvider<dyn UsageProbe> {
    async fn usage(&self, provider: Provider, config_dir: Option<&str>) -> AppResult<RateInfo> {
        self.of(provider).usage(provider, config_dir).await
    }
    async fn limits(&self, provider: Provider, config_dir: Option<&str>) -> AppResult<Vec<UsageLimit>> {
        self.of(provider).limits(provider, config_dir).await
    }
}

#[async_trait]
impl ChatEngine for ByProvider<dyn ChatEngine> {
    async fn run_turn(&self, request: &TurnRequest, control: &mut TurnControl, on_event: TurnListener<'_>) -> TurnEnd {
        self.of(request.provider).run_turn(request, control, on_event).await
    }
}

/// Procura um executável pelos nomes dados: nos locais usuais, no PATH do processo,
/// no PATH do shell de login (um app aberto pelo Finder herda um PATH mínimo) e em `extra`.
pub(crate) fn find_executable(names: &[&str], extra: &[PathBuf]) -> Option<PathBuf> {
    let mut dirs: Vec<PathBuf> = vec![];
    if let Some(home) = dirs::home_dir() {
        dirs.extend([home.join(".local/bin"), home.join(".npm-global/bin"), home.join("bin")]);
    }
    dirs.extend([PathBuf::from("/opt/homebrew/bin"), PathBuf::from("/usr/local/bin")]);
    if let Some(path) = std::env::var_os("PATH") {
        dirs.extend(std::env::split_paths(&path));
    }
    if let Some(path) = shell_path() {
        dirs.extend(std::env::split_paths(path));
    }
    dirs.iter()
        .flat_map(|dir| names.iter().map(move |name| dir.join(name)))
        .chain(extra.iter().cloned())
        .find(|p| p.is_file())
}

/// Comando de um CLI com o PATH do shell do usuário (o Codex instalado pelo npm precisa do `node`).
pub(crate) fn cli_command(cli: &Path) -> Command {
    let mut cmd = Command::new(cli);
    if let Some(path) = shell_path() {
        cmd.env("PATH", path);
    }
    cmd.kill_on_drop(true);
    super::hide_console_async(&mut cmd);
    cmd
}

/// Para um CLI que não lê fora da pasta de trabalho: o conteúdo das pastas extras
/// (o conhecimento do projeto) vai junto das instruções, como texto.
pub(crate) fn inline_read_dirs(request: &TurnRequest) -> TurnRequest {
    let mut request = request.clone();
    let mut files: Vec<(String, String)> = std::mem::take(&mut request.read_dirs)
        .iter()
        .filter_map(|dir| std::fs::read_dir(dir).ok())
        .flat_map(|entries| entries.flatten())
        .filter(|e| e.path().is_file() && !e.file_name().to_string_lossy().starts_with('.'))
        .filter_map(|e| Some((e.file_name().to_string_lossy().into_owned(), std::fs::read_to_string(e.path()).ok()?)))
        .collect();
    if files.is_empty() {
        return request;
    }
    files.sort();
    let contents: String = files.iter().map(|(name, text)| format!("Conteúdo de {name}:\n{text}\n")).collect();
    let system = request.extra_system.take().unwrap_or_default();
    request.extra_system = Some(format!("{system}\n\nOs arquivos de conhecimento já seguem aqui, não é preciso abri-los:\n{contents}"));
    request
}

/// O texto que um CLI sem canal próprio de instruções e anexos recebe como mensagem:
/// as instruções do aplicativo (só ao abrir a sessão), os anexos e a mensagem do usuário.
/// Imagens e PDFs viram arquivos temporários, apagados quando este valor sai de cena.
pub(crate) struct Prompt {
    pub text: String,
    /// Imagens gravadas em disco, para os CLIs que as recebem por caminho.
    pub images: Vec<PathBuf>,
    dir: Option<PathBuf>,
    /// Quantos anexos já foram gravados; numera os arquivos para nomes iguais não colidirem.
    saved: usize,
}

impl Prompt {
    pub fn compose(request: &TurnRequest) -> Prompt {
        let mut text = String::new();
        // A sessão retomada já recebeu as instruções no turno que a abriu.
        if let Some(system) = request.extra_system.as_deref().filter(|s| !s.trim().is_empty() && request.resume.is_none()) {
            text.push_str(&format!("Instruções e contexto do aplicativo (não são a mensagem do usuário):\n{system}\n\n---\n\n"));
        }
        let mut prompt = Prompt { text, images: vec![], dir: None, saved: 0 };
        for attachment in &request.attachments {
            prompt.attach(attachment);
        }
        prompt.text.push_str(&request.prompt);
        prompt
    }

    fn attach(&mut self, attachment: &Attachment) {
        if attachment.kind == AttachmentKind::Text {
            self.text.push_str(&format!("Arquivo anexado — {}:\n{}\n\n", attachment.name, attachment.data));
            return;
        }
        match self.save(attachment) {
            Some(path) => {
                self.text.push_str(&format!("Arquivo anexado — {}: salvo em {}\n\n", attachment.name, path.display()));
                if attachment.kind == AttachmentKind::Image {
                    self.images.push(path);
                }
            }
            None => self.text.push_str(&format!("(O anexo {} não pôde ser entregue.)\n\n", attachment.name)),
        }
    }

    fn save(&mut self, attachment: &Attachment) -> Option<PathBuf> {
        let bytes = STANDARD.decode(&attachment.data).ok()?;
        let dir = match &self.dir {
            Some(dir) => dir.clone(),
            None => {
                let dir = std::env::temp_dir().join(format!("lume-anexos-{}", uuid::Uuid::new_v4()));
                std::fs::create_dir_all(&dir).ok()?;
                self.dir = Some(dir.clone());
                dir
            }
        };
        // Só o nome do arquivo: um anexo não escolhe onde é gravado.
        let name = Path::new(&attachment.name).file_name()?;
        let path = dir.join(format!("{}-{}", self.saved, name.to_string_lossy()));
        std::fs::write(&path, bytes).ok()?;
        self.saved += 1;
        Some(path)
    }
}

impl Drop for Prompt {
    fn drop(&mut self) {
        if let Some(dir) = &self.dir {
            let _ = std::fs::remove_dir_all(dir);
        }
    }
}

/// Como um turno que falhou deve ser tratado: limite da conta, login vencido ou erro comum.
pub(crate) fn failure(message: &str, now: i64) -> TurnEnd {
    let text = message.to_lowercase();
    let has = |patterns: &[&str]| patterns.iter().any(|p| text.contains(p));
    if has(&["usage limit", "hit your limit", "rate limit", "limit reached", "quota", "resource_exhausted", "resource exhausted"]) {
        return TurnEnd::Limited { resets_at: retry_after(&text, now) };
    }
    if has(&["authentication", "not logged in", "unauthorized", "401", "log in again", "sign in again", "refresh token"]) {
        return TurnEnd::AuthError(message.trim().to_owned());
    }
    let message = message.trim();
    TurnEnd::Failed(if message.is_empty() { "O processo terminou sem resposta.".into() } else { message.to_owned() })
}

/// Instante em que a conta volta, quando a mensagem diz "try again in 2 days 3 hours 5 minutes".
fn retry_after(text: &str, now: i64) -> Option<i64> {
    let rest = &text[text.find("again in ")? + "again in ".len()..];
    let mut seconds = 0;
    let mut words = rest.split_whitespace();
    while let Some(amount) = words.next().and_then(|w| w.parse::<i64>().ok()) {
        let unit = words.next()?.trim_matches(|c: char| !c.is_alphabetic());
        seconds += amount
            * match unit {
                u if u.starts_with("day") => 86_400,
                u if u.starts_with("hour") => 3600,
                u if u.starts_with("minute") => 60,
                u if u.starts_with("second") => 1,
                _ => return None,
            };
    }
    (seconds > 0).then(|| now + seconds)
}

pub(crate) fn now_seconds() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

#[cfg(test)]
pub(crate) fn sample_request(provider: Provider) -> TurnRequest {
    TurnRequest {
        provider,
        config_dir: None,
        cwd: std::env::temp_dir().to_string_lossy().into_owned(),
        // Só o Claude recebe o modelo da conversa; os outros, o escolhido para eles (ou nenhum).
        model: if provider == Provider::Claude { "opus".into() } else { String::new() },
        resume: None,
        prompt: "olá".into(),
        attachments: vec![],
        extra_system: None,
        tools: "WebSearch,WebFetch,Skill".into(),
        effort: "medium".into(),
        permission_mode: "auto".into(),
        interactive: true,
        denied_skills: vec![],
        denied_tools: vec![],
        mcp_config: None,
        allowed_tools: vec![],
        read_dirs: vec![],
        mcp_servers: vec![],
        artifacts: false,
        design: false,
        account_connectors: true,
        background_tasks: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_limit_login_and_plain_failures() {
        let limited = failure("You've hit your usage limit. Upgrade to Pro or try again in 2 days 3 hours 5 minutes.", 1000);
        assert_eq!(limited, TurnEnd::Limited { resets_at: Some(1000 + 2 * 86_400 + 3 * 3600 + 300) });
        assert_eq!(failure("RESOURCE_EXHAUSTED: quota exceeded", 0), TurnEnd::Limited { resets_at: None });
        assert_eq!(failure("try again in 45 minutes: rate limit", 10), TurnEnd::Limited { resets_at: Some(2710) });
        assert!(matches!(failure("authentication required", 0), TurnEnd::AuthError(_)));
        assert_eq!(failure("  modelo desconhecido ", 0), TurnEnd::Failed("modelo desconhecido".into()));
        assert_eq!(failure("", 0), TurnEnd::Failed("O processo terminou sem resposta.".into()));
    }

    #[test]
    fn prompt_carries_instructions_only_when_the_session_opens() {
        let mut request = sample_request(Provider::ChatGpt);
        request.extra_system = Some("Seja breve.".into());
        request.attachments = vec![Attachment::text("nota.md", "conteúdo")];
        let opened = Prompt::compose(&request);
        let dir = std::env::temp_dir().join(format!("lume-inline-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("regras.md"), "Use REST.").unwrap();
        std::fs::write(dir.join(".tmp"), "meio gravado").unwrap();
        let mut with_dir = request.clone();
        with_dir.read_dirs = vec![dir.to_string_lossy().into_owned()];
        let inlined = inline_read_dirs(&with_dir);
        let system = inlined.extra_system.unwrap();
        assert!(system.starts_with("Seja breve.") && system.contains("Conteúdo de regras.md:\nUse REST."));
        assert!(!system.contains("meio gravado") && inlined.read_dirs.is_empty());
        let _ = std::fs::remove_dir_all(dir);
        assert!(opened.text.starts_with("Instruções e contexto do aplicativo"));
        assert!(opened.text.contains("Seja breve.") && opened.text.contains("Arquivo anexado — nota.md:\nconteúdo"));
        assert!(opened.text.ends_with("olá"));

        request.resume = Some("sessao".into());
        assert!(!Prompt::compose(&request).text.contains("Seja breve."));
    }

    #[test]
    fn images_become_temporary_files_removed_afterwards() {
        let mut request = sample_request(Provider::ChatGpt);
        request.attachments =
            vec![Attachment { name: "../foto.png".into(), kind: AttachmentKind::Image, media_type: "image/png".into(), data: STANDARD.encode(b"png") }];
        let prompt = Prompt::compose(&request);
        let path = prompt.images[0].clone();
        assert_eq!(std::fs::read(&path).unwrap(), b"png");
        assert!(path.file_name().unwrap().to_string_lossy().ends_with("foto.png"));
        assert!(path.parent().unwrap().file_name().unwrap().to_string_lossy().starts_with("lume-anexos-"), "o nome do anexo não escapa da pasta");
        assert!(prompt.text.contains(&path.display().to_string()));
        drop(prompt);
        assert!(!path.exists());
    }
}
