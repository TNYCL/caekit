#!/usr/bin/env node
// Fails when the documentation has drifted from the code it describes.
//
//   node scripts/check-docs.mjs
//
// AGENTS.md lists what a change has to update. This script checks the parts of that
// contract that a program can check: every physics kind has its example, page, tests and
// README row; every output is documented; every crate is described; versions quoted in
// prose match the manifests; links resolve; published READMEs use absolute links; and every
// page with Rust examples is compiled by docs/doctests.rs. Only Node.js built-ins are used.
import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../", import.meta.url));
const problems = [];
const problem = (message) => problems.push(message);
const exists = (file) => existsSync(path.join(root, file));
const missing = new Set();
/** The text of a file, or an empty string and one reported problem when it is missing. */
function read(file) {
  if (!exists(file)) {
    if (!missing.has(file)) problem(`${file} is missing`);
    missing.add(file);
    return "";
  }
  return readFileSync(path.join(root, file), "utf8").replaceAll("\r\n", "\n");
}

function requireFile(file, why) {
  if (!exists(file) && !missing.has(file)) {
    missing.add(file);
    problem(`${file} is missing (${why})`);
  }
  return exists(file);
}

const snake = (name) => name.replace(/([a-z0-9])([A-Z])/g, "$1_$2").toLowerCase();
const kebab = (name) => snake(name).replaceAll("_", "-");

/** Variant names of `pub enum <name>` in a Rust source file. */
function enumVariants(file, name) {
  const match = new RegExp(`pub enum ${name} \\{\\n([\\s\\S]*?)\\n\\}`).exec(read(file));
  if (!match) {
    problem(`${file}: pub enum ${name} not found; update scripts/check-docs.mjs`);
    return [];
  }
  return [...match[1].matchAll(/^ {4}([A-Z][A-Za-z0-9]*)\b/gm)].map((variant) => variant[1]);
}

// ---- Markdown files of the repository --------------------------------------------------
// Build output, and every hidden directory except .github.
const SKIP_DIRS = new Set(["target", "node_modules", "npm", "pkg", "dist", "runs"]);
const skipped = (entry) => SKIP_DIRS.has(entry) || (entry.startsWith(".") && entry !== ".github");
function markdownFiles(dir = "") {
  const found = [];
  for (const entry of readdirSync(path.join(root, dir))) {
    const file = path.posix.join(dir, entry);
    if (statSync(path.join(root, file)).isDirectory()) {
      if (!skipped(entry)) found.push(...markdownFiles(file));
    } else if (entry.endsWith(".md")) {
      found.push(file);
    }
  }
  return found.sort();
}
const pages = markdownFiles();
const prose = (file) => read(file).replace(/^```[\s\S]*?^```/gm, "");

// ---- Physics kinds ---------------------------------------------------------------------
const kinds = enumVariants("crates/caekit/src/model.rs", "Physics");
if (kinds.length === 0) problem("crates/caekit/src/model.rs: enum Physics has no variants");
const readme = read("README.md");
const references = read("scripts/verify-reference-cases.mjs");
const cliTests = read("crates/caekit-cli/tests/reference_cases.rs");
for (const variant of kinds) {
  const kind = snake(variant);
  const page = `docs/physics/${kebab(variant)}.md`;
  const example = `examples/${kind}.json`;
  if (requireFile(example, `reference model of physics kind ${kind}`)) {
    const model = JSON.parse(read(example));
    if (model.physics?.kind !== kind) problem(`${example}: physics.kind is ${model.physics?.kind}, expected ${kind}`);
  }
  if (requireFile(page, `physics page of kind ${kind}`)) {
    if (!read(page).includes(`\`${kind}\``) && !read(page).includes(`"${kind}"`)) problem(`${page} never names the kind \`${kind}\``);
    if (!read(page).includes(example)) problem(`${page} does not show how to run ${example}`);
  }
  requireFile(`crates/caekit/tests/${kind}.rs`, `library tests of physics kind ${kind}`);
  if (!read("crates/caekit/tests/model_identity.rs").includes(`"../../../examples/${kind}.json"`)) {
    problem(`crates/caekit/tests/model_identity.rs does not freeze the digest of ${example}`);
  }
  if (!readme.includes(`(${page})`) || !readme.includes(`\`${kind}\``)) {
    problem(`README.md: the physics table has no row for \`${kind}\` that links ${page}`);
  }
  if (!new RegExp(`^  ${kind}\\(report, label\\) \\{`, "m").test(references)) {
    problem(`scripts/verify-reference-cases.mjs has no analytical reference for ${kind}`);
  }
  if (!cliTests.includes(`check_and_run("${kind}.json")`)) {
    problem(`crates/caekit-cli/tests/reference_cases.rs does not run ${kind}.json`);
  }
  const crateReadme = read("crates/caekit/README.md");
  if (!crateReadme.includes(`\`${kind}\``)) problem(`crates/caekit/README.md: the physics table has no row for \`${kind}\``);
}
const known = new Set(kinds.map(snake));
for (const entry of readdirSync(path.join(root, "examples"))) {
  if (entry.endsWith(".json") && !known.has(path.basename(entry, ".json"))) problem(`examples/${entry} matches no physics kind`);
}
for (const entry of readdirSync(path.join(root, "docs", "physics"))) {
  if (entry.endsWith(".md") && !known.has(path.basename(entry, ".md").replaceAll("-", "_"))) {
    problem(`docs/physics/${entry} matches no physics kind`);
  }
}

// ---- Outputs ---------------------------------------------------------------------------
const modelPage = read("docs/model.md");
for (const variant of enumVariants("crates/caekit-types/src/quantity.rs", "Output")) {
  if (!modelPage.includes(`\`${snake(variant)}\``)) problem(`docs/model.md does not describe the output \`${snake(variant)}\``);
}

// ---- Workspace crates ------------------------------------------------------------------
const manifest = read("Cargo.toml");
const members = [...(/^members = \[\n([\s\S]*?)\n\]/m.exec(manifest)?.[1] ?? "").matchAll(/"([^"]+)"/g)].map((member) => member[1]);
if (members.length === 0) problem("Cargo.toml: workspace members not found; update scripts/check-docs.mjs");
const architecture = read("docs/ARCHITECTURE.md");
const publishedReadmes = [];
for (const member of members.filter((member) => member.startsWith("crates/"))) {
  const name = path.posix.basename(member);
  const crateReadme = `${member}/README.md`;
  if (requireFile(crateReadme, `README of the package ${name}`)) publishedReadmes.push(crateReadme);
  if (!readme.includes(`\`${name}\``)) problem(`README.md: the workspace table does not list \`${name}\``);
  if (!architecture.includes(`\`${name}\``)) problem(`docs/ARCHITECTURE.md does not describe \`${name}\``);
}

// ---- Versions quoted in prose ----------------------------------------------------------
const msrv = /^rust-version = "([^"]+)"/m.exec(manifest)?.[1];
if (!msrv) problem("Cargo.toml: rust-version not found");
const workflow = read(".github/workflows/verify.yml");
const nodeVersions = new Set([...workflow.matchAll(/node-version: "(\d+)"/g)].map((match) => match[1]));
if (nodeVersions.size !== 1) problem(`.github/workflows/verify.yml uses several Node.js versions: ${[...nodeVersions].join(", ")}`);
const [node] = nodeVersions;
for (const page of pages) {
  const text = read(page);
  for (const [, quoted] of text.matchAll(/\bRust (\d+\.\d+)(?:\.\d+)?\b(?! \()/g)) {
    // A page may name the toolchain a measurement was taken with; that is not the MSRV.
    if (quoted !== msrv && !/\bwith Ubuntu\b|\bmeasured\b/i.test(text.slice(Math.max(0, text.indexOf(`Rust ${quoted}`) - 200), text.indexOf(`Rust ${quoted}`) + 200))) {
      problem(`${page} names Rust ${quoted}; Cargo.toml declares rust-version ${msrv}`);
    }
  }
  for (const [, quoted] of text.matchAll(/cargo \+(\d+\.\d+)\.\d+ /g)) {
    if (quoted !== msrv) problem(`${page} runs cargo +${quoted}; Cargo.toml declares rust-version ${msrv}`);
  }
  for (const [, quoted] of text.matchAll(/Node\.js (\d+)/g)) {
    if (quoted !== node && !/\bNode\.js \d+ or newer\b/.test(text)) problem(`${page} names Node.js ${quoted}; CI uses ${node}`);
  }
}

// ---- Links -----------------------------------------------------------------------------
// GitHub heading anchors: lower case, punctuation dropped, spaces to hyphens.
function anchors(file) {
  const seen = new Map();
  const result = new Set();
  for (const [, heading] of prose(file).matchAll(/^#{1,6} +(.+?) *#*$/gm)) {
    const base = heading
      .replace(/\[([^\]]*)\]\([^)]*\)/g, "$1")
      .replace(/[`*_]/g, "")
      .toLowerCase()
      .replace(/[^\p{L}\p{N} _-]/gu, "")
      .replaceAll(" ", "-");
    const count = seen.get(base) ?? 0;
    seen.set(base, count + 1);
    result.add(count === 0 ? base : `${base}-${count}`);
  }
  return result;
}
for (const page of pages) {
  for (const [, target] of prose(page).matchAll(/\[[^\]]*\]\(([^)\s]+)(?: "[^"]*")?\)/g)) {
    if (/^(https?:|mailto:)/.test(target)) continue;
    if (publishedReadmes.includes(page)) {
      problem(`${page} links ${target}: a README published to a registry must use absolute URLs`);
      continue;
    }
    const [file, anchor] = target.split("#");
    const resolved = file === "" ? page : path.posix.normalize(path.posix.join(path.posix.dirname(page), file));
    if (resolved.startsWith("..") || !exists(resolved)) {
      problem(`${page} links ${target}, which does not exist`);
    } else if (anchor && resolved.endsWith(".md") && !anchors(resolved).has(anchor)) {
      problem(`${page} links ${target}: ${resolved} has no heading with the anchor #${anchor}`);
    }
  }
}

// ---- Rust examples are compiled --------------------------------------------------------
const doctests = read("docs/doctests.rs");
for (const page of pages) {
  if (!/^```rust/m.test(read(page))) continue;
  // The README of the caekit crate is its crate documentation and is tested there.
  if (page === "crates/caekit/README.md") {
    if (!read("crates/caekit/src/lib.rs").includes('include_str!("../README.md")')) {
      problem("crates/caekit/src/lib.rs must include ../README.md as the crate documentation");
    }
    continue;
  }
  const include = path.posix.relative("docs", page);
  if (!doctests.includes(`include_str!("${include}")`)) {
    problem(`${page} has Rust examples that nothing compiles: add include_str!("${include}") to docs/doctests.rs`);
  }
}
for (const [, include] of doctests.matchAll(/include_str!\("([^"]+)"\)/g)) {
  if (!exists(path.posix.normalize(path.posix.join("docs", include)))) problem(`docs/doctests.rs includes ${include}, which does not exist`);
}

// ---- Files that every change relies on -------------------------------------------------
if (requireFile("CHANGELOG.md", "release notes come from it") && !/^## \[Unreleased\]$/m.test(read("CHANGELOG.md"))) {
  problem("CHANGELOG.md has no '## [Unreleased]' section");
}
if (requireFile("CLAUDE.md", "it points Claude Code at AGENTS.md") && read("CLAUDE.md").trim() !== "@AGENTS.md") {
  problem("CLAUDE.md must contain only '@AGENTS.md': the rules live in one file");
}
for (const file of ["AGENTS.md", "CONTRIBUTING.md", "SECURITY.md", "docs/ARCHITECTURE.md", "docs/RELEASING.md", "docs/model.md", "docs/provenance.md", "docs/python.md", "docs/wasm.md"]) {
  requireFile(file, "AGENTS.md refers to it");
}

if (problems.length > 0) {
  console.error(`check-docs: ${problems.length} problem${problems.length === 1 ? "" : "s"}\n`);
  for (const message of problems) console.error(`  - ${message}`);
  process.exit(1);
}
console.log(`check-docs: ${pages.length} pages, ${kinds.length} physics kinds and ${members.length} workspace members are consistent`);
