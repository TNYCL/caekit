## Summary

- 

## Checklist

- [ ] `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings` and the workspace tests pass
- [ ] New or changed physics has an analytical reference derived independently of the solver, and a refinement study where the discretization has an error
- [ ] Earlier model documents keep their meaning, report fields and `model_sha256`
- [ ] The pages listed in section 3 of `AGENTS.md` describe the change, and `node scripts/check-docs.mjs` passes
- [ ] Every number added to the documentation was measured or derived, and the page says how
- [ ] `CHANGELOG.md` has an entry under `Unreleased` for anything a user can observe
- [ ] No code was copied or ported from a source whose licence is incompatible with `MIT OR Apache-2.0`; new sources are cited in `docs/provenance.md`
