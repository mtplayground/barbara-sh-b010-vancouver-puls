# barbara-sh-b010-vancouver-puls

Monorepo for a React/Tailwind frontend and Rust/Axum backend service.

## Repository Layout

- `apps/web` - React SPA built with Vite, TypeScript, and Tailwind CSS.
- `apps/api` - Rust backend service built with Axum and Tokio.

## Prerequisites

- Node.js 20+
- npm 9+
- Rust toolchain with Cargo

## Development

Install frontend dependencies:

```bash
npm install
```

Run the frontend dev server:

```bash
npm run dev:web
```

Run the backend service:

```bash
HOST=0.0.0.0 PORT=8080 cargo run -p api
```

The backend exposes:

- `GET /healthz`
- `GET /api/health`

Build the frontend:

```bash
npm run build:web
```

Build the backend:

```bash
cargo build
```
