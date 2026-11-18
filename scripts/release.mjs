#!/usr/bin/env node
// Release helper for the CAEKit workspace. The process is described in docs/RELEASING.md.
//
//   node scripts/release.mjs version                 print the workspace version
//   node scripts/release.mjs check [--tag vX.Y.Z]    versions, package files, CHANGELOG section
//   node scripts/release.mjs notes X.Y.Z             print the CHANGELOG section of a version
//   node scripts/release.mjs bump X.Y.Z              set the version and open its CHANGELOG section
//   node scripts/release.mjs sync-licenses           copy the root license files into each package
//   node scripts/release.mjs publish-crates [--dry-run]
//
// publish-crates publishes every crate whose manifest allows it, in dependency order, and
// skips versions that are already on the crates.io index, so a failed release can simply be
// run again. Only Node.js built-ins are used.
import { spawnSync } from "node:child_process";
import { copyFileSync, existsSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../", import.meta.url));
const LICENSE_FILES = ["LICENSE-MIT", "LICENSE-APACHE"];
// Packaged from its own directory (maturin) but not published to crates.io.
const EXTRA_LICENSE_DIRS = ["crates/caekit-py"];
const VERSION = /^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$/;
const INDEX = "https://index.crates.io";
const USER_AGENT = "caekit-release (https://github.com/caekit/caekit)";
// README.md carries this marker on the note that says no release exists yet.
const UNRELEASED_MARKER = "<!-- unreleased-note -->";
const PIN = /^(caekit[\w-]*) = \{ path = "[^"]+", version = "([^"]+)"(?:,[^}\n]*)? \}/gm;

function fail(message) {
  console.error(`release.mjs: ${message}`);
  process.exit(1);
}

/** The text with LF line endings and whether the file used CRLF. */
function readText(file) {
  const raw = readFileSync(path.join(root, file), "utf8");
  return [raw.replaceAll("\r\n", "\n"), raw.includes("\r\n")];
}

function writeText(file, text, crlf) {
  writeFileSync(path.join(root, file), crlf ? text.replaceAll("\n", "\r\n") : text);
}

function cargo(args, options = {}) {
  const result = spawnSync("cargo", args, { cwd: root, encoding: "utf8", ...options });
  if (result.error) fail(`cargo: ${result.error.message}`);
  if (result.status !== 0) {
    if (result.stderr) process.stderr.write(result.stderr);
    process.exit(result.status ?? 1);
  }
  return result.stdout;
}

function metadata() {
  const out = cargo(["metadata", "--no-deps", "--format-version", "1", "--locked"], { maxBuffer: 1 << 28 });
  return JSON.parse(out);
}

// `publish = false` is reported as an empty registry list.
const isPublishable = (pkg) => pkg.publish === null || pkg.publish.includes("crates-io");

const packageDir = (pkg) => path.relative(root, path.dirname(pkg.manifest_path)).split(path.sep).join("/");

/** Publishable packages, dependencies first. Dev-dependencies count: crates.io requires
 *  every dependency in the published manifest to exist already. */
function publishOrder(packages) {
  const names = new Set(packages.map((pkg) => pkg.name));
  const deps = new Map(
    packages.map((pkg) => [pkg.name, new Set(pkg.dependencies.map((dep) => dep.name).filter((name) => names.has(name)))]),
  );
  const order = [];
  while (order.length < packages.length) {
    const ready = [...names].filter((name) => !order.includes(name) && [...deps.get(name)].every((dep) => order.includes(dep))).sort();
    if (ready.length === 0) fail(`dependency cycle among ${[...names].filter((name) => !order.includes(name)).sort()}`);
    order.push(...ready);
  }
  return order.map((name) => packages.find((pkg) => pkg.name === name));
}

function workspaceVersion(meta) {
  const versions = new Set(meta.packages.map((pkg) => pkg.version));
  if (versions.size !== 1) {
    fail(`workspace members disagree on the version: ${meta.packages.map((pkg) => `${pkg.name} ${pkg.version}`).join(", ")}`);
  }
  return [...versions][0];
}

const escapeRegExp = (text) => text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");

function changelogSection(version) {
  const [text] = readText("CHANGELOG.md");
  const match = new RegExp(`^## \\[${escapeRegExp(version)}\\][^\\n]*\\n([\\s\\S]*?)(?=^## \\[|(?![\\s\\S]))`, "m").exec(text);
  if (!match) return undefined;
  // Drop link reference definitions; they belong to the whole file.
  const body = match[1].split("\n").filter((line) => !/^\[[^\]]+\]: /.test(line)).join("\n").trim();
  return body || undefined;
}

function licenseDirs(meta) {
  const dirs = meta.packages.filter(isPublishable).map(packageDir);
  return [...new Set([...dirs, ...EXTRA_LICENSE_DIRS])].sort();
}

function staleLicenseCopies(meta) {
  const stale = [];
  for (const name of LICENSE_FILES) {
    const want = readFileSync(path.join(root, name));
    for (const dir of licenseDirs(meta)) {
      const copy = path.join(root, dir, name);
      if (!existsSync(copy) || !readFileSync(copy).equals(want)) stale.push(`${dir}/${name}`);
    }
  }
  return stale;
}

function cmdVersion() {
  console.log(workspaceVersion(metadata()));
}

function cmdCheck(args) {
  const tagAt = args.indexOf("--tag");
  const tag = tagAt === -1 ? undefined : args[tagAt + 1];
  if (tagAt !== -1 && !tag) fail("--tag needs a value, for example --tag v0.1.0");
  const meta = metadata();
  const version = workspaceVersion(meta);
  const problems = [];
  const [manifest] = readText("Cargo.toml");
  for (const [, name, pinned] of manifest.matchAll(PIN)) {
    if (pinned !== version) problems.push(`[workspace.dependencies] ${name} pins ${pinned}, the workspace is ${version}`);
  }
  for (const pkg of meta.packages.filter(isPublishable)) {
    // crates.io shows the package README; a path outside the package breaks its links.
    if (pkg.readme !== "README.md" || !existsSync(path.join(root, packageDir(pkg), "README.md"))) {
      problems.push(`${pkg.name}: set readme = "README.md" and add ${packageDir(pkg)}/README.md`);
    }
    for (const field of ["description", "license", "repository"]) {
      if (!pkg[field]) problems.push(`${pkg.name}: the manifest has no ${field}`);
    }
  }
  if (tag !== undefined) {
    if (tag !== `v${version}`) problems.push(`tag ${tag} does not match the workspace version ${version}`);
    if (changelogSection(version) === undefined) problems.push(`CHANGELOG.md has no non-empty '## [${version}]' section`);
    if (readText("README.md")[0].includes(UNRELEASED_MARKER)) {
      problems.push(`README.md still carries the pre-release note (${UNRELEASED_MARKER}); delete it for the release`);
    }
  }
  for (const copy of staleLicenseCopies(meta)) {
    problems.push(`${copy} differs from the root copy (run: node scripts/release.mjs sync-licenses)`);
  }
  if (problems.length > 0) fail(`\n  ${problems.join("\n  ")}`);
  console.log(`release check passed for ${version}`);
}

function cmdNotes(args) {
  const [version] = args;
  if (!version) fail("usage: node scripts/release.mjs notes X.Y.Z");
  const body = changelogSection(version);
  if (body === undefined) fail(`CHANGELOG.md has no non-empty '## [${version}]' section`);
  console.log(body);
}

function cmdBump(args) {
  const [next] = args;
  if (!next || !VERSION.test(next)) fail(`'${next ?? ""}' is not a semantic version`);
  const previous = workspaceVersion(metadata());

  // Prepare both files first, so that a problem in either leaves both untouched.
  let [manifest, manifestCrlf] = readText("Cargo.toml");
  let replaced = 0;
  manifest = manifest.replace(/(\[workspace\.package\][^[]*?\nversion = ")[^"]+(")/, (_, head, tail) => {
    replaced += 1;
    return `${head}${next}${tail}`;
  });
  if (replaced !== 1) fail("workspace version not found in Cargo.toml");
  manifest = manifest.replace(
    /^(caekit[\w-]* = \{ path = "[^"]+", version = ")[^"]+("(?:,[^}\n]*)? \})/gm,
    (_, head, tail) => `${head}${next}${tail}`,
  );

  let [log, logCrlf] = readText("CHANGELOG.md");
  if (new RegExp(`^## \\[${escapeRegExp(next)}\\]`, "m").test(log)) fail(`CHANGELOG.md already has a section for ${next}`);
  if (!/^## \[Unreleased\]\n/m.test(log)) fail("CHANGELOG.md has no '## [Unreleased]' heading");
  const today = new Date().toISOString().slice(0, 10);
  log = log.replace(/^## \[Unreleased\]\n/m, `## [Unreleased]\n\n## [${next}] - ${today}\n`);
  // Link references: the first release has no earlier tag to compare with.
  const compare = /^\[Unreleased\]: (https:\/\/\S+?)\/compare\/v\S+?\.\.\.HEAD$/m;
  const first = /^\[Unreleased\]: (https:\/\/\S+?)\/commits\/\S+$/m;
  if (compare.test(log)) {
    log = log.replace(compare, (_, repo) => `[Unreleased]: ${repo}/compare/v${next}...HEAD\n[${next}]: ${repo}/compare/v${previous}...v${next}`);
  } else if (first.test(log)) {
    log = log.replace(first, (_, repo) => `[Unreleased]: ${repo}/compare/v${next}...HEAD\n[${next}]: ${repo}/releases/tag/v${next}`);
  } else {
    fail("CHANGELOG.md has no '[Unreleased]: https://...' link reference");
  }

  writeText("Cargo.toml", manifest, manifestCrlf);
  writeText("CHANGELOG.md", log, logCrlf);
  cargo(["update", "--workspace"], { stdio: "inherit" });
  console.log(`${previous} -> ${next}. Review CHANGELOG.md, commit, then tag v${next} on master (docs/RELEASING.md).`);
}

function cmdSyncLicenses() {
  const meta = metadata();
  for (const name of LICENSE_FILES) {
    for (const dir of licenseDirs(meta)) copyFileSync(path.join(root, name), path.join(root, dir, name));
  }
  console.log(`license files copied into: ${licenseDirs(meta).join(", ")}`);
}

// https://doc.rust-lang.org/cargo/reference/registry-index.html#index-files
function indexPath(name) {
  if (name.length <= 2) return `${name.length}/${name}`;
  if (name.length === 3) return `3/${name[0]}/${name}`;
  return `${name.slice(0, 2)}/${name.slice(2, 4)}/${name}`;
}

async function onIndex(name, version) {
  const response = await fetch(`${INDEX}/${indexPath(name.toLowerCase())}`, { headers: { "User-Agent": USER_AGENT } });
  if (response.status === 404) return false;
  if (!response.ok) fail(`crates.io index answered ${response.status} for ${name}`);
  const lines = (await response.text()).split("\n").filter((line) => line.trim());
  return lines.some((line) => JSON.parse(line).vers === version);
}

async function cmdPublishCrates(args) {
  const dryRun = args.includes("--dry-run");
  const meta = metadata();
  const version = workspaceVersion(meta);
  const crates = publishOrder(meta.packages.filter(isPublishable));
  const excluded = meta.packages.filter((pkg) => !isPublishable(pkg)).map((pkg) => pkg.name);
  console.log(`publish order: ${crates.map((pkg) => pkg.name).join(" -> ")}`);
  if (!dryRun) {
    const done = [];
    for (const pkg of crates) if (await onIndex(pkg.name, version)) done.push(pkg.name);
    if (done.length > 0) console.log(`already on crates.io at ${version}, skipped: ${done.join(", ")}`);
    if (done.length === crates.length) {
      console.log("nothing left to publish");
      return;
    }
    excluded.push(...done);
  }
  // cargo >= 1.90 publishes a workspace in dependency order and waits for the index.
  const command = ["publish", "--workspace", "--locked", ...excluded.flatMap((name) => ["--exclude", name])];
  if (dryRun) command.push("--dry-run");
  console.log(`+ cargo ${command.join(" ")}`);
  cargo(command, { stdio: "inherit" });
}

const commands = {
  version: cmdVersion,
  check: cmdCheck,
  notes: cmdNotes,
  bump: cmdBump,
  "sync-licenses": cmdSyncLicenses,
  "publish-crates": cmdPublishCrates,
};
const [name, ...rest] = process.argv.slice(2);
if (!Object.hasOwn(commands, name ?? "")) {
  fail(`usage: node scripts/release.mjs <${Object.keys(commands).join("|")}> (see docs/RELEASING.md)`);
}
await commands[name](rest);
