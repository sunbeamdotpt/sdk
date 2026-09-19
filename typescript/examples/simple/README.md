# sunbeam-g2v · simple example

Real Connect-RPC end-to-end demo. The FE talks to the Rust server in
`server/examples/simple/` which implements the same Eliza
service using `connectrpc-build`-generated stubs from
`proto/connectrpc/eliza/v1/eliza.proto`.

## Run

```sh
# Terminal 1 — start the Rust server
cargo run -p sunbeam-g2v --example simple
# listens on http://localhost:8080

# Terminal 2 — start the FE dev server
cd client/examples/simple
npm install
npm run dev
# open http://localhost:5173
```

The FE defaults to `http://localhost:8080`. To point at a different server:

```sh
VITE_CONNECT_BASE_URL=https://demo.connectrpc.com npm run dev
```

The page renders three sections:

1. **`useRpcQuery(ElizaService, "say", …)`** — fires immediately, refetchable.
2. **`useRpcMutation(ElizaService, "say")`** — type a sentence, get a reply.
3. **`useAuth`** — login/logout flow that flips the `withAuth` interceptor's
   bearer header on every subsequent RPC.

## What this exercises

| Surface              | Where                                                     |
| -------------------- | --------------------------------------------------------- |
| `FrameworkProvider`  | `src/main.tsx`                                            |
| `createTransport`    | `src/main.tsx` (full interceptor chain)                   |
| `withRequestId`      | injects `x-request-id` on every RPC                       |
| `withAuth`           | injects `Authorization: Bearer <authStore.token>`         |
| `withTracing`        | OTel client span + W3C traceparent (when OTLP enabled)    |
| `withLogging`        | `console.info` / `console.error` per RPC                  |
| `withRetryInterceptor` | retries `Unavailable` / `DeadlineExceeded` on idempotent methods |
| `useRpcQuery`        | typed unary query (`ElizaService.Say`)                    |
| `useRpcMutation`     | typed unary mutation (`ElizaService.Say`)                 |
| `useAuth` / `useTheme` | `src/App.tsx`                                           |
| `notificationActions`| login validation toast                                    |
| `ServiceError`       | `Pretty` error rendering                                  |

## Codegen

Bindings live under `src/gen/` and are committed. Regenerate after editing
the proto:

```sh
npm run generate   # = buf generate
```

This runs `@bufbuild/buf` with `@bufbuild/protoc-gen-es` (target=ts).

## Env vars

| Var                      | Effect                                              |
| ------------------------ | --------------------------------------------------- |
| `VITE_CONNECT_BASE_URL`  | Override RPC server (default `https://demo.connectrpc.com`) |
| `VITE_SUNBEAM_OTLP_URL`  | Enable OTel tracing → OTLP HTTP collector           |

## Pointing at your own server

Replace `demo.connectrpc.com` with any Connect server that exposes the same
proto, or swap `ElizaService` for your own generated service. The framework
hooks are agnostic — they consume `DescService` shapes from
`@bufbuild/protobuf` directly.
