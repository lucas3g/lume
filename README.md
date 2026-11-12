# Lume

Cliente desktop do Claude com várias contas: quando o limite de uma conta acaba, a conversa continua na próxima.

Feito com Tauri 2 (núcleo em Rust) e Svelte 5. O motor das conversas é o Claude Code CLI oficial, que precisa estar instalado no computador.

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

## Arquitetura

O núcleo (`src-tauri/src`) segue Clean Architecture, com as dependências apontando para dentro:

- `domain`: regras e tipos do negócio, sem I/O.
- `application`: casos de uso e portas (interfaces).
- `infrastructure`: SQLite, Claude Code CLI, git e sistema de arquivos, implementando as portas.
- `presentation`: comandos Tauri, janelas e a composição das dependências.

`scripts/make_icon.py` redesenha o ícone do app.
