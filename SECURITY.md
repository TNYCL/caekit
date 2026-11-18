# Security policy

## Stance on untrusted input

A model is a JSON document that may come from a third party, so CAEKit treats it as
untrusted:

- Decoding is strict. Unknown schema versions, fields, physics kinds and condition kinds
  are errors, and validation checks geometry, meshes, conditions and outputs before any
  system is assembled.
- No model input is meant to cause a panic. A panic on any input is a bug; please report
  it.
- The solver crates contain no `unsafe` code (the workspace forbids it) and do no I/O.
  `unsafe` appears only inside the binding macros of the Python and WebAssembly crates.
- Memory and time grow with the size of the mesh and with the iteration limits in the
  model, and CAEKit does not cap them. When you solve models that you do not trust, run
  the solve in a sandboxed process with a memory and a time limit. Solving a model is not
  a security boundary by itself.

Out of scope: wrong engineering conclusions drawn from a valid run, and denial of service
by a model that is large but well formed.

## Reporting a vulnerability

Please report privately and do not open a public issue. Use GitHub private vulnerability
reporting on the repository: **Security**, then **Report a vulnerability**
(<https://github.com/caekit/caekit/security/advisories/new>).

Include the affected version or commit, a minimal model document and what you observed,
such as a panic message, memory growth or a hang. You can expect an acknowledgement
within a few days and a fix or mitigation plan after triage. Reporters are credited in
the changelog unless they prefer to stay anonymous.

## Supported versions

While the project is before 1.0, only the latest release and the `master` branch receive
fixes.
