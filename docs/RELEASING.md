# Releasing CAEKit

One tag publishes everything. `.github/workflows/release.yml` builds and tests every
artifact, waits for approval on the `release` environment, then publishes:

| Channel | Package | Contents |
| --- | --- | --- |
| crates.io | `caekit-types`, `caekit-numerics`, `caekit-mesh`, `caekit-fluids`, `caekit-mechanics`, `caekit-thermal`, `caekit`, `caekit-cli` | Rust sources; docs.rs builds the API documentation |
| PyPI | `caekit` | abi3 wheels for Linux x86_64 and aarch64 (manylinux), macOS arm64 and x86_64, Windows x64, and a source distribution |
| npm | `caekit-wasm` | One package for browsers, bundlers and Node.js (ES module and CommonJS) |
| GitHub Releases | `caekit-<target>` | The command-line tool for Linux x86_64 and aarch64 (static musl), macOS arm64 and x86_64, Windows x64, and `caekit-wasm32-wasip1` |

Every crate and package carries the workspace version. `caekit-py` and `caekit-wasm` have
`publish = false`: they are distributed through PyPI and npm, not crates.io. The GitHub
release also carries the wheels, the source distribution, the npm tarball, `SHA256SUMS`
and build-provenance attestations for every file
(`gh attestation verify <file> -R caekit/caekit`). PyPI and npm attach their own
provenance through trusted publishing.

Archive names carry no version, so the newest build always has a stable link, for example
`https://github.com/caekit/caekit/releases/latest/download/caekit-x86_64-unknown-linux-musl.tar.gz`.
`cargo binstall caekit-cli` downloads these archives.

## Dry run

Run the workflow by hand from the Actions tab, or with
`gh workflow run release.yml --ref <branch>`. It runs every build, test and package step,
including `cargo publish --dry-run` for the whole workspace, and publishes nothing. Do
this before tagging whenever the packaging changed. The **Packaging** job of the
verification workflow runs the crates.io part on every pull request.

## One-time setup

Registry accounts belong to the maintainers; enable two-factor authentication on each.

1. **GitHub environment.** Settings → Environments → `release`: a maintainer is a required
   reviewer and deployments are limited to tags that match `v*`. Secrets for the first
   release (below) go into this environment, not into the repository.
2. **Rulesets.** "Release tags" lets only organization administrators create, move or
   delete tags that match `v*`. "Protect master" forbids deleting or force-pushing
   `master` and requires the **Required CI** check before a change reaches it;
   organization administrators can bypass both.
3. **PyPI.** Add a *pending* trusted publisher at
   <https://pypi.org/manage/account/publishing/>: project `caekit`, owner `caekit`,
   repository `caekit`, workflow `release.yml`, environment `release`. No token is ever
   needed. A pending publisher does not reserve the name; the first upload does.
4. **crates.io.** Trusted publishing works only for crates that already exist. For the
   first release, create an API token at <https://crates.io/settings/tokens> with the
   scopes `publish-new` and `publish-update`, the crate pattern `caekit*` and a short
   expiry, and save it as the secret `CARGO_REGISTRY_TOKEN` of the `release` environment.
   After the first release, add a trusted publisher on each crate's settings page
   (repository `caekit/caekit`, workflow `release.yml`, environment `release`) and the
   GitHub team of the maintainers as an owner, then delete the secret and revoke the
   token. Without the secret the workflow uses trusted publishing.
5. **npm.** The package must exist before it can have a trusted publisher. For the first
   release, create a granular access token at npmjs.com with read and write access, limited
   to publishing, with a short expiry, and save it as the secret `NPM_TOKEN` of the
   `release` environment. After the first release, add a trusted publisher in the
   `caekit-wasm` package settings (GitHub Actions, `caekit/caekit`, workflow `release.yml`,
   environment `release`), delete the secret and revoke the token.

## Each release

1. `node scripts/release.mjs bump X.Y.Z`. It sets the workspace version and the internal
   dependency versions, refreshes `Cargo.lock` and turns `## [Unreleased]` in
   `CHANGELOG.md` into `## [X.Y.Z] - <date>` below a new empty `Unreleased` section. Edit
   the changelog section; it becomes the GitHub release notes. For the first release, run
   `bump 0.1.0` although the version does not change, and delete the pre-release note
   that is marked `<!-- unreleased-note -->` at the top of `README.md`.
2. `node scripts/release.mjs check --tag vX.Y.Z` must pass. Commit, let the verification
   workflow pass on `master` and, when the packaging changed, run the dry run.
3. Tag the commit on `master` and push the tag:
   `git tag -a vX.Y.Z -m "CAEKit X.Y.Z" && git push origin vX.Y.Z`.
4. When the builds pass, approve the `release` deployment on the workflow run. The three
   publish jobs start together; the GitHub release is created after all of them succeed.
5. Check docs.rs, the PyPI and npm pages and the release assets, then install each
   package once: `cargo binstall caekit-cli`, `pip install caekit`,
   `npm install caekit-wasm`.

A version with a suffix (`0.2.0-rc.1`) becomes a GitHub pre-release and the npm dist-tag
`next`.

## When something fails

- Before approval nothing is published: fix on `master`, delete the tag
  (`git push --delete origin vX.Y.Z && git tag -d vX.Y.Z`) and tag again.
- After approval, use *Re-run failed jobs*. Every publish step can run twice: crates that
  are already on the index are skipped, PyPI uses `skip-existing` and npm skips an
  existing version.
- A published version cannot be replaced on any registry. If a release is broken, fix it
  in the next patch version; `cargo yank`, PyPI's yank and `npm deprecate` mark the bad
  one.

## Package files

Cargo and maturin package only files inside the package directory, so each published
crate directory and `crates/caekit-py` hold copies of `LICENSE-MIT` and
`LICENSE-APACHE`. After editing a root file, run
`node scripts/release.mjs sync-licenses`; the release check fails while a copy differs.
The check also requires every published crate to carry its own `README.md`, because a
registry shows that file and cannot follow a link into the repository. Those READMEs use
absolute URLs for the same reason. The npm package takes its licence files and its README
from `crates/caekit-wasm/build.mjs`.
