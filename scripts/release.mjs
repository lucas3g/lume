#!/usr/bin/env node
// Sobe a versão do Lume e publica: `pnpm release 0.2.1`.
// Atualiza a versão em package.json, tauri.conf.json, Cargo.toml e Cargo.lock,
// faz o commit na main e o push. O push dispara .github/workflows/release.yml,
// que compila o instalador do Windows e publica a atualização para quem tem o app.
import { execFileSync } from "node:child_process";
import { readFileSync, writeFileSync } from "node:fs";

const version = process.argv[2]?.replace(/^v/, "");
if (!version || !/^\d+\.\d+\.\d+$/.test(version)) {
  console.error("Uso: pnpm release X.Y.Z");
  process.exit(1);
}

const git = (...args) => execFileSync("git", args, { encoding: "utf8" }).trim();

if (git("branch", "--show-current") !== "main") {
  console.error("Publique a partir da main.");
  process.exit(1);
}
if (git("status", "--porcelain")) {
  console.error("Há mudanças não commitadas; faça o commit antes de publicar.");
  process.exit(1);
}
git("fetch", "--tags", "--quiet");
if (git("tag", "--list", `v${version}`)) {
  console.error(`A v${version} já foi publicada.`);
  process.exit(1);
}

function replace(path, pattern, label) {
  const text = readFileSync(path, "utf8");
  const next = text.replace(pattern, (_, before) => `${before}${version}"`);
  if (next === text) {
    console.error(`Não achei a versão em ${label}.`);
    process.exit(1);
  }
  writeFileSync(path, next);
}

replace("package.json", /("version": ")[^"]+"/, "package.json");
replace("src-tauri/tauri.conf.json", /("version": ")[^"]+"/, "tauri.conf.json");
replace("src-tauri/Cargo.toml", /(^\[package\][\s\S]*?\nversion = ")[^"]+"/, "Cargo.toml");
replace("src-tauri/Cargo.lock", /(\nname = "lume"\nversion = ")[^"]+"/, "Cargo.lock");

const files = ["package.json", "src-tauri/tauri.conf.json", "src-tauri/Cargo.toml", "src-tauri/Cargo.lock"];
git("add", ...files);
git("commit", "-m", `Versão ${version}`);
git("push", "origin", "main");
console.log(`Versão ${version} enviada; o GitHub Actions vai compilar e publicar a atualização.`);
