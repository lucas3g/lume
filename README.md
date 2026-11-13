# Lume

Cliente desktop do Claude com várias contas: quando o limite de uma conta acaba, a conversa continua na próxima.

Feito com Tauri 2 (núcleo em Rust) e Svelte 5. O motor das conversas é o Claude Code CLI oficial, que precisa estar instalado no computador.

## Tipos de conta

Cada conta tem um tipo, e o Lume conversa pelo CLI desse serviço instalado no computador:

| Tipo | CLI | Login | Uso da conta |
|---|---|---|---|
| Claude | Claude Code (`claude`) | perfil isolado por conta | sessão, semana e por modelo |
| ChatGPT | Codex (`codex`) | o login do computador ou perfil isolado (`CODEX_HOME`) | o último que o Codex gravou |
| Gemini | Antigravity (`agy`) | só o login do computador (um por máquina) | não informado fora do terminal |

Quando a conta seguinte é de outro serviço, ela abre uma sessão própria e recebe o diálogo já gravado como contexto. Contas do ChatGPT e do Gemini usam o modelo configurado no CLI delas e não pedem autorização a cada ação: o que passa do modo de permissão da conversa é recusado pelo próprio CLI. Skills, conectores e agentes do Lume valem só para contas do Claude.

## Desenvolvimento

Requisitos: Rust, Node 24, pnpm e o Claude Code CLI.

```sh
pnpm install
pnpm tauri dev
```

## Verificações

```sh
pnpm check                      # tipos do frontend
cd src-tauri && cargo test      # testes do núcleo
cd src-tauri && cargo clippy --all-targets
```

Os testes marcados como `ignored` usam o Claude Code real e consomem uso da conta: `cargo test -- --ignored`.

## Empacotar

```sh
pnpm tauri build --bundles app   # macOS: src-tauri/target/release/bundle/macos/Lume.app
```

O instalador para Windows é compilado pelo GitHub Actions (`.github/workflows/build-windows.yml`) e fica disponível como artefato da execução.

## Publicar uma versão

Uma tag `vX.Y.Z` dispara `.github/workflows/release.yml`, que publica no GitHub Releases o `.dmg` (macOS), o instalador NSIS (Windows) e o `latest.json` consultado pelo updater do app. Versões instaladas avisam e atualizam sozinhas.

O workflow precisa dos segredos `TAURI_SIGNING_PRIVATE_KEY` (conteúdo de `~/.tauri/lume.key`) e `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` (vazio) no repositório. A chave pública correspondente está em `src-tauri/tauri.conf.json`; sem a privada não dá para assinar atualizações.

```sh
gh secret set TAURI_SIGNING_PRIVATE_KEY < ~/.tauri/lume.key
gh secret set TAURI_SIGNING_PRIVATE_KEY_PASSWORD --body ""
git tag v0.2.0 && git push origin v0.2.0
```

## Arquitetura

O núcleo (`src-tauri/src`) segue Clean Architecture, com as dependências apontando para dentro:

- `domain`: regras e tipos do negócio, sem I/O.
- `application`: casos de uso e portas (interfaces).
- `infrastructure`: SQLite, Claude Code CLI, git e sistema de arquivos, implementando as portas.
- `presentation`: comandos Tauri, janelas e a composição das dependências.

`scripts/make_icon.py` redesenha o ícone do app.
