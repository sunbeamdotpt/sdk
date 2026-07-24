# Changelog

## Unreleased

Queued cli requests (agent-mail #39/#44/#33) plus a kanban client refresh.
Additive; no breaking changes. Note: `From<connectrpc::ConnectError>` now
yields the new `Connect` variant instead of `Network` — the Display output
is unchanged, but match arms on `Network` for ConnectRPC failures must move.

- fix(tools): `ensure_tool` downloads on a dedicated OS thread — the
  `reqwest::blocking` runtime panicked when dropped inside an async context
  with a cold tool cache (cli #39)
- feat(error): `SunbeamError::Connect { code, context }` (behind
  `auth`/`kanban`) preserves the structured ConnectRPC `ErrorCode`, so
  consumers can match structurally instead of string-matching (cli #44)
- feat(config): `sso_url` / `sso_client_id` fields on `config::Context`
  (cli #33)
- feat(testing): `SsoGateway` enables the Kratos recovery flow and courier;
  `with_kratos_courier_smtp` points the courier at a real SMTP container
  (cli #33)
- feat(kanban): `KanbanClient::labels()` and `KanbanClient::milestones()`
  accessors for the new upstream `LabelService` / `MilestoneService`

## v3.2.0

Regenerated the sso-gateway ConnectRPC stubs from the latest
`buf.build/sunbeamdotpt/sso-gateway` module. Additive; no breaking changes.

- feat(auth): `skip_consent` first-party flag on `Application`,
  `CreateApplicationRequest`, and `UpdateApplicationRequest` (the latter via
  a `google.protobuf.BoolValue` toggle; omitting it leaves the flag
  unchanged). `UpdateApplicationRequest` is now documented as a partial
  update — fields left at their zero value keep the stored value
- fix(testing): `Kanban` orchestrator provisions its service app with
  `skip_consent: false` (machine-to-machine client; no browser flow)
- test(auth): sso-gateway integration suite now covers `skip_consent`
  round-trips and partial-update semantics, and runs against gateway image
  `v2026.07.22` by default (`SSO_GATEWAY_IMAGE_TAG` still overrides)

## v3.1.1

Real-boot fixes for the v3.1.0 `testing::Kanban` orchestrator, reported by
cli's integration suite.

- fix(testing): NATS readiness waits on **stderr** — nats-server logs
  "Server is ready" there, not stdout (the stdout wait always timed out)
- fix(testing): OpenSearch gets a host-side `/_cluster/health` poll
  (green/yellow) before dependents start — the builder had no readiness
  wait, and the kanban server does not retry its system migrations, so it
  crashed with connection-refused on the backfill migration
- fix(testing): kanban server readiness now polls `/healthz/live` from the
  host instead of waiting for the "kanban listening" log line, which
  differs across published image versions
- feat(error): `From<lettre::address::AddressError>` for `SunbeamError`
  behind the `lettre` feature

## v3.1.0

Requests from the cli repo's v3 migration (agent-mail #18/#19/#20). All
additive; no breaking changes.

- feat(secrets): make the seeding helpers public — consts `ADMIN_USERNAME`,
  `PG_USERS`, `SMTP_URI`; fns `gen_fernet_key`, `gen_dkim_key_pair`,
  `rand_token`, `rand_token_n`, `rand_string_32` (new), `port_forward_svc`,
  `get_or_create`, `configure_db_engine`, `psql_exec`, `wait_pod_running`,
  `scw_config`, `delete_resource`; structs `KratosIdentity`,
  `KratosRecovery`. Secret generation now uses `rand::rngs::OsRng` throughout
  (and `rand_string_32` rejection-samples to avoid modulo bias)
- feat(error): `From<lettre::error::Error>` and
  `From<lettre::transport::smtp::Error>` for `SunbeamError` behind the new
  opt-in `lettre` cargo feature (lettre was dropped as dead in v3.0.0; it
  returns as an optional, conversion-only dependency, not part of `full`)
- feat(error): `From<connectrpc::ConnectError>` for `SunbeamError` (features
  `auth`/`kanban`), mirroring the g2v `ClientError` conversion
- feat(kanban): `KanbanClient::connect(url)` one-call constructor and
  `KanbanClient::with_default_header(name, value)` for default per-call
  headers (e.g. `x-sunbeam-object-id`)
- feat(kanban): `sdk::kanban::prelude` re-exporting `connectrpc`, `buffa`,
  `buffa-types`, `sunbeam_g2v`, `KanbanClient`, and the generated `v1`
  surface — name public-API types through the prelude to avoid version skew
- feat: re-export public-API dependency crates: `sdk::reqwest`,
  `sdk::kube_rs`, `sdk::k8s_openapi` (the kube crate is renamed to `kube_rs`
  to avoid colliding with the SDK's own `kube` module)
- feat(testing): `Kanban` full-stack orchestrator (Postgres + NATS JetStream
  + OpenSearch + MinIO + `SsoGateway` stack + kanban image on a shared
  network; provisions the `kanban-test` tenant and a `kanban-service` app
  via IAM; requires the `auth` feature)
- feat(testing): `SsoGateway::with_network()` +
  `SsoGatewayHandle::internal_url()` so other containers can reach the
  gateway by container name; `OpenSearch::with_network()` /
  `with_container_name()`
- fix(wfectl): `resolve_token`'s not-logged-in error regains the
  "run `sunbeam auth login` first" hint

## v3.0.0

Third major revision of the Sunbeam SDK: a standalone library crate (the
2.0.0-rc restructure and the calendar-versioned snapshots are superseded;
1.x refers to the old CLI+SDK workspace).

- feat: standalone library crate — CLI code removed; the crate is consumed as
  a pure SDK (`sdk` on the `sunbeam` registry)
- feat(auth): `AuthClient` for the sso-gateway — generated ConnectRPC IAM
  surface (tenant, identity, OAuth2, federation, permission, SCIM,
  applications, client credentials, agents) with tenant-id default headers
- feat: service clients on the `sunbeam-g2v` client stack
  (`ClientBuilder`/`RestClient`): OpenSearch (`search/`), LiveKit (`media/`),
  Matrix (`matrix/`), Prometheus/Loki/Grafana (`monitoring/`), BuildKit
  (`build/`)
- feat: `kanban/` ConnectRPC client (boards, cards, projects, templates,
  attachments, search, subscriptions) and `wfectl/` gRPC client for the WFE
  workflow engine
- feat: absorb the `sunbeam-test` crate as `src/testing/` behind the `testing`
  cargo feature; sso-gateway integration tests are gated on the feature
- feat(testing): container builders for Postgres, OpenBao, OpenSearch,
  Tuwunel, LiveKit, Prometheus, Loki, Grafana, Stalwart, SearXNG, Headscale,
  OTel collector, the ory suite, OpenFGA, and the `SsoGateway` orchestrator
- test: testcontainers-backed tests for the OpenSearch, Matrix, LiveKit,
  monitoring, and OpenBao clients (`--features testing`, requires Docker);
  sso-gateway e2e suite serialized via `.config/nextest.toml`
- feat!: per-module cargo features for tree-shaking — `auth`, `kanban`,
  `wfectl`, `search`, `matrix`, `media`, `monitoring`, `build`, `kube`,
  `openbao`, `secrets`, `vault-keystore`, `vpn`, `testing`;
  `default = ["full"]` preserves previous behaviour
- chore(deps): reqwest 0.13, thiserror 2, kube 4 + k8s-openapi 0.28,
  indicatif 0.18, mockall 0.15, dirs 6, and a full in-range lock refresh
- chore(deps): remove dead dependencies (gix, repo-rs-*, camino, indexmap,
  ulid, comfy-table, rcgen, lettre)
- chore(deps): pinned intentionally — connectrpc/buffa 0.7 (sunbeam-g2v 0.5.2
  pairs with connectrpc 0.7), bollard 0.20 (testcontainers 0.27.3), and the
  RustCrypto line (aes-gcm/hmac/sha2/rand move as one ecosystem)

## v1.1.2

- 30dc4f9 fix(opensearch): make ML model registration idempotent
- 3d2d16d feat(secrets): add xchacha20-poly1305 cipher key seeding for Kratos
- 80ab6d6 feat: enable Meet external API, fix SDK path
- b08a80d refactor: nest infra commands under `sunbeam platform`

## v1.1.1

- cd80a57 fix: DynamicBearer auth, retry on 500/429, upload resilience
- de5c807 fix: progress bar tracks files not bytes, retry on 502, dedup folders
- 2ab2fd5 fix: polish Drive upload progress UI
- 27536b4 feat: parallel Drive upload with indicatif progress UI

## v1.1.0

- 477006e chore: bump to v1.1.0, update package description
- ca0748b feat: encrypted vault keystore, JWT auth, Drive upload
- 13e3f5d fix opensearch pod resolution + sol-agent vault policy
- faf5255 feat: async SunbeamClient factory with unified auth resolution

## v1.0.1

- 34647e6 feat: seed Sol agent vault policy + gitea creds, bump v1.0.1

## v1.0.0

- 051e17d chore: bump to v1.0.0, drop native-tls for pure rustls
- 7ebf900 feat: wire 15 service subcommands into CLI, remove old user command
- f867805 feat: CLI modules for all 25+ service clients
- 3d7a2d5 feat: OutputFormat enum + render/render_list/read_json_input helpers
- 756fbc5 chore: update Cargo.lock
- 97976e0 fix: include build module (was gitignored)
- f06a167 feat: BuildKit client + integration test suite (651 tests)
- b60e22e feat: La Suite clients — 7 DRF services (75 endpoints)
- 915f0b2 feat: monitoring clients — Prometheus, Loki, Grafana (57 endpoints)
- 21f9e18 feat: LiveKitClient — real-time media API (15 endpoints + JWT)
- a33697c feat: S3Client — object storage API (21 endpoints)
- 329c18b feat: OpenSearchClient — search and analytics API (60 endpoints)
- 2888d59 feat: MatrixClient — chat and collaboration API (80 endpoints)
- 890d7b8 feat: GiteaClient — unified git forge API (50+ endpoints)
- c597234 feat: HydraClient — OAuth2/OIDC admin API (35 endpoints)
- f0bc363 feat: KratosClient — identity management (30 endpoints)
- 6823772 feat: ServiceClient trait, HttpTransport, and SunbeamClient factory
- 31fde1a fix: forge URL derivation for bare IP hosts, add Cargo registry config
- 46d2133 docs: update README for Rust workspace layout
- 3ef3fc0 feat: Python upstream — Sol bot registration TODO
- e0961cc refactor: binary crate — thin main.rs + cli.rs dispatch
- 8e5d295 refactor: SDK small command modules — services, cluster, manifests, gitea, update, auth
- 6c7e1cd refactor: SDK users, pm, and checks modules with submodule splits
- bc65b91 refactor: SDK images and secrets modules with submodule splits
- 8e51e0b refactor: SDK kube, openbao, and tools modules
- b92700d refactor: SDK core modules — error, config, output, constants
- 2ffedb9 refactor: workspace scaffolding — sunbeam-sdk + sunbeam binary crate
- b6daf60 chore: suppress dead_code warning on exit code constants
- b92c6ad feat: Python upstream — onboard/offboard, mailbox, Projects, --no-cache
- 8d6e815 feat: --no-cache build flag and Sol build target
- f75f61f feat: user provisioning — mailbox, Projects, welcome email
- c6aa1bd feat: complete pm subcommands with board discovery and user resolution
- ffc0fe9 feat: split auth into sso/git, Planka token exchange, board discovery
- ded0ab4 refactor: remove --env flag, use --context like kubectl
- 88b02ac feat: kubectl-style contexts with per-domain auth tokens
- 3a5e1c6 fix: use predictable client_id via pre-seeded K8s secret
- 1029ff0 fix: auth login UX — timeout, Ctrl+C, suppress K8s error, center HTML
- 43b5a4e fix: URL-encode scope parameter with %20 instead of +
- 7fab2a7 fix: auth login domain resolution with --domain flag
- 184ad85 fix: install rustls ring crypto provider at startup
- 5bdb789 feat: unified project management across Planka and Gitea
- d4421d3 feat: OAuth2 CLI authentication with PKCE and token caching
- aad469e fix: stdin password, port-forward retry, seed advisory lock
- dff4588 fix: employee ID pagination, add async tests
- 019c73e fix: S3 auth signature tested against AWS reference vector
- e95ee4f fix: rewrite users.rs to fully async (was blocking tokio runtime)
- 24e98b4 fix: CNPG readiness, DKIM SPKI format, kv_patch, container name
- 6ec0666 fix: SSH tunnel leak, cmd_bao injection, discovery cache, DNS async
- bcfb443 refactor: deduplicate constants, fix secret key mismatch, add VSS pruning
- 503e407 feat: implement OpenSearch ML setup and model_id injection
- bc5eeaa feat: implement secrets.rs with OpenBao HTTP API
- 7fd8874 refactor: migrate all modules from anyhow to SunbeamError
- cc0b6a8 refactor: add thiserror error tree and tracing logging
- ec23568 feat: Phase 2 feature modules + comprehensive test suite (142 tests)
- 42c2a74 feat: Phase 1 foundations — kube-rs client, OpenBao HTTP client, self-update
- 80c67d3 feat: Rust rewrite scaffolding with embedded kustomize+helm
- d5b9632 refactor: cross-platform tool downloads, configurable infra dir and ACME email
- c82f15b feat: add tuwunel/matrix support with OpenSearch ML post-apply hooks
- 928323e fix(cli): unify proxy build path, fix Gitea password sync
- 956a883 chore: added AGENTS.md file for various models.
- 507b4d3 feat(config): add production host and infrastructure directory configuration
- cbf5c12 docs: update repository URLs to use HTTPS remotes for src.sunbeam.pt
- 133fc98 docs: add comprehensive README with professional documentation
- 33d7774 chore: added license
- 1a97781 docs: add comprehensive documentation for sunbeam CLI
- 28c266e feat(cli): partial apply with namespace filter
- 2569978 feat(cli): meet build/seed support, production kube tunnel, gitea OIDC bootstrap
- c759f2c feat(users): add disable/enable lockout commands; fix table output
- cb5a290 feat: auto-restart deployments on ConfigMap change after sunbeam apply
- 1a3df1f feat: add sunbeam build integration target
- de12847 feat: add impress image mirroring and docs secret seeding
- 14dd685 feat: add kratos-admin-ui build target and user management commands
- b917aa3 fix: specify -c openbao container in cmd_bao kubectl exec
- 352f0b6 feat: add sunbeam k8s kubectl passthrough; fix kube_exec container arg
- fb3fd93 fix: sunbeam apply and bootstrap reliability
- 0acbf66 check: rewrite seaweedfs probe with S3 SigV4 auth
- 6bd59ab sunbeam check: parallel execution, 5s timeout, external S3 check
- 39a2f70 Fix sunbeam check: group by namespace, never crash on network errors
- 1573faa Add sunbeam check verb with service-level health probes
