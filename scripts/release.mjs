#!/usr/bin/env node
// Sobe a versão do Lume e publica: `pnpm release 0.2.1`.
// Atualiza a versão em package.json, tauri.conf.json, Cargo.toml e Cargo.lock,
// faz o commit na main e o push. O push dispara .github/workflows/release.yml,
// que compila o instalador do Windows e publica a atualização para quem tem o app.
import { execFileSync } from "node:child_process";
import { readFileSync, writeFileSync } from "node:fs";

const fail = (message) => {
  console.error(message);
  process.exit(1);
};

const version = process.argv[2]?.replace(/^v/, "");
if (!version || !/^\d+\.\d+\.\d+$/.test(version)) fail("Uso: pnpm release X.Y.Z");

const git = (...args) => execFileSync("git", args, { encoding: "utf8" }).trim();

if (git("branch", "--show-current") !== "main") fail("Publique a partir da main.");
if (git("status", "--porcelain")) fail("Há mudanças não commitadas; faça o commit antes de publicar.");
git("fetch", "origin", "main", "--tags", "--quiet");
if (git("rev-parse", "HEAD") !== git("rev-parse", "origin/main")) {
  fail("A main local não está igual à do GitHub; faça o pull (ou o push) antes de publicar.");
}
if (git("tag", "--list", `v${version}`)) fail(`A v${version} já foi publicada.`);

const current = JSON.parse(readFileSync("src-tauri/tauri.conf.json", "utf8")).version;
const parts = (v) => v.split(".").map(Number);
const newer = (a, b) => {
  const [x, y] = [parts(a), parts(b)];
  const i = x.findIndex((n, k) => n !== y[k]);
  return i >= 0 && x[i] > y[i];
};
if (!newer(version, current)) fail(`A versão nova precisa ser maior que a atual (${current}); os apps instalados não aceitam voltar versão.`);

// Todas as trocas são conferidas antes de gravar: ou os quatro arquivos mudam, ou nenhum.
const edits = [
  ["package.json", /("version": ")[^"]+"/],
  ["src-tauri/tauri.conf.json", /("version": ")[^"]+"/],
  ["src-tauri/Cargo.toml", /(^\[package\][\s\S]*?\r?\nversion = ")[^"]+"/],
  ["src-tauri/Cargo.lock", /(\r?\nname = "lume"\r?\nversion = ")[^"]+"/],
].map(([path, pattern]) => {
  const text = readFileSync(path, "utf8");
  const next = text.replace(pattern, (_, before) => `${before}${version}"`);
  if (next === text) fail(`Não achei a versão em ${path}.`);
  return [path, next];
});
for (const [path, next] of edits) writeFileSync(path, next);

const files = edits.map(([path]) => path);
git("add", ...files);
git("commit", "-m", `Versão ${version}`);
try {
  git("push", "origin", "main");
} catch (e) {
  // Sem o push, o commit local só atrapalharia a próxima tentativa: ele é desfeito.
  git("reset", "--hard", "HEAD~1");
  fail(`O push falhou e o commit da versão foi desfeito; nada foi publicado.\n${e.message}`);
}
console.log(`Versão ${version} enviada; o GitHub Actions vai compilar e publicar a atualização.`);
