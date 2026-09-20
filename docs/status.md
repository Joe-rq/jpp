# Current scope

This alpha contains a fixed snapshot of the project's runtime and composition sources. The public distribution adds packaging and an offline CLI. The main research workspace can continue evolving independently.

- Included: question values, components, algorithm constructors, runtime source (`src/foundation/jv`, kernel v0.1), synthetic mechanism tests (`tests/`, `tests/foundation_jv/`), and a curated copy of the research workspace documents in `research/` (design trail, ledger, experiment pre-registrations and results).
- Not included: private research conversations, credentials, raw model call records and run outputs, model weights, hosted service or an independent compiler. `research/` is synced by `tools/sync-from-workspace.sh`, which enforces these exclusions.
- Demonstrated: offline execution and finite-domain checking. Verification results are recorded in `verification.md` after running the distribution.
- Still being developed: independent syntax, richer static analysis, calibrated real-model examples and portability across providers.

## Backends

The default `jpp demo` uses `FixtureClient`, which returns synthetic observations at zero API cost. Its calibration records are synthetic fixtures only.

The bundled historical JEV adapter targets a specific API/model version and reads a credential from `~/.typesafe-key`. It is not used by the demo. Its current service compatibility and model accuracy have not been validated as part of this release. Do not reuse fixture calibration records for real decisions. A documented, tested live-backend quickstart is a roadmap item.

PyYAML is a dependency of the bundled foundation utilities; pytest is a development dependency. They are installed separately and retain their own licenses. Model services are not distributed or licensed by this repository.
