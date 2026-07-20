# barbara-sh-b010-vancouver-puls

Monorepo for a React/Tailwind frontend and Rust/Axum backend service.

## Repository Layout

- `apps/web` - React SPA built with Vite, TypeScript, and Tailwind CSS.
- `apps/api` - Rust backend service built with Axum and Tokio.

## Prerequisites

- Node.js 20+
- npm 9+
- Rust toolchain with Cargo
- PostgreSQL 16 or compatible PostgreSQL server

## Development

Install frontend dependencies:

```bash
npm install
```

Copy `.env.example` for local development and fill in real values outside of
version control. The API reads configuration from environment variables only.
Set `VITE_API_BASE_URL` for the web dev server when it should call an API origin
other than the same host.

Run the frontend dev server:

```bash
npm run dev:web
```

Run the backend service:

```bash
DATABASE_URL=postgres://... HOST=0.0.0.0 PORT=8080 cargo run -p api
```

The backend exposes:

- `GET /healthz`
- `GET /api/health`
- `GET /api/health/db`
- `GET /api/health/storage`
- `GET /api/auth/login` redirects to the managed Ideavibes auth service with
  a frontend `return_to` URL.
- `GET /api/auth/session` verifies the `mctai_session` cookie, upserts the user
  in PostgreSQL, and returns the current app user without issuing an app JWT.

API responses use JSON error bodies shaped as
`{"error":{"code":"...","message":"..."}}`. The server enables request logging
and CORS for configured deployment origins plus localhost development origins.

Run database migrations against the configured PostgreSQL database:

```bash
DATABASE_URL=postgres://... npm run db:migrate
```

Build the frontend:

```bash
npm run build:web
```

Build the backend:

```bash
cargo build
```

Run all checks:

```bash
npm run check
```

Useful individual checks:

```bash
npm run typecheck:web
npm run lint:web
npm run lint:api
npm run format:check
```

## Self-Hosted Deployment

The API is a Rust/Axum service that listens on `0.0.0.0:8080` by default. The
React app builds to static files in `apps/web/dist`; serve those files with a
static web server or reverse proxy, and proxy `/api/*` plus `/healthz` to the API
process. The API does not use SQLite, JSON-file persistence, or local volumes for
state; all persistent state is stored in PostgreSQL and object storage.

1. Install dependencies on the host:

```bash
npm ci
cargo --version
psql --version
```

2. Create the production environment file and fill every value that applies to
the deployment. Keep this file out of version control.

```bash
cp .env.example .env.production
```

Required for a full production run:

- `DATABASE_URL` and `DATABASE_MAX_CONNECTIONS`
- `SELF_URL`, `ALLOWED_CORS_ORIGIN`, `HOST`, `PORT`, and `RUST_LOG`
- `MCTAI_AUTH_URL`, `MCTAI_AUTH_APP_TOKEN`, and `MCTAI_AUTH_JWKS_URL`
- `MCTAI_EMAIL_URL` and `MCTAI_EMAIL_APP_TOKEN`
- `OBJECT_STORAGE_ENDPOINT`, `OBJECT_STORAGE_REGION`, `OBJECT_STORAGE_BUCKET`,
  `OBJECT_STORAGE_ACCESS_KEY_ID`, `OBJECT_STORAGE_SECRET_ACCESS_KEY`, and
  `OBJECT_STORAGE_PREFIX`
- `INSTAGRAM_APP_ID`, `INSTAGRAM_APP_SECRET`, `INSTAGRAM_REDIRECT_URI`,
  `INSTAGRAM_GRAPH_API_VERSION`, `INSTAGRAM_ACCESS_TOKEN`,
  `INSTAGRAM_BUSINESS_ACCOUNT_ID`, and `INSTAGRAM_USERNAME`
- `ANTHROPIC_API_KEY`
- `VITE_API_BASE_URL` only when the browser should call an API origin different
  from the site origin
- `OPERATOR_ALERT_EMAIL` when scheduled publish failures should notify an
  operator

3. Build the frontend with the final frontend environment values:

```bash
set -a
. ./.env.production
set +a
npm run build:web
```

4. Build the backend:

```bash
cargo build --release -p api
```

5. Run migrations against PostgreSQL:

```bash
set -a
. ./.env.production
set +a
./target/release/api migrate
```

6. Start the API service:

```bash
set -a
. ./.env.production
set +a
./target/release/api
```

The ingestion, scheduled publisher, and Instagram insights jobs start inside the
API process. Keep exactly one API process running unless the scheduler ownership
model is changed; multiple processes would run the same background jobs.

7. Configure the reverse proxy:

- Serve `apps/web/dist` at `https://your-domain.example/`.
- Proxy `https://your-domain.example/api/*` and
  `https://your-domain.example/healthz` to `http://127.0.0.1:8080`.
- Preserve `Host`, `X-Forwarded-Host`, and `X-Forwarded-Proto` headers so login
  redirects and CORS checks use the public origin.

8. Verify the deployment:

```bash
curl -fsS https://your-domain.example/healthz
curl -fsS https://your-domain.example/api/health/db
curl -fsS https://your-domain.example/api/health/storage
```

If `/api/health/storage` reports unavailable, confirm the object storage
credentials and prefix in `.env.production`. If login redirects to a non-page
URL, confirm `SELF_URL` is the public frontend origin, not an API endpoint.
