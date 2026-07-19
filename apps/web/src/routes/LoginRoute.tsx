import { Navigate } from "react-router-dom";
import type { FormEvent } from "react";

import { useAuth } from "../auth/useAuth";

export function LoginRoute() {
  const auth = useAuth();

  if (auth.isAuthenticated) {
    return <Navigate to="/" replace />;
  }

  return (
    <main className="bg-paper min-h-screen text-slate-950">
      <div className="mx-auto grid min-h-screen w-full max-w-7xl items-center gap-10 px-4 py-10 sm:px-6 lg:grid-cols-[minmax(0,1fr)_420px] lg:px-8">
        <section className="max-w-2xl">
          <p className="text-pine text-sm font-semibold uppercase tracking-wide">
            Publishing operations
          </p>
          <h1 className="mt-4 text-4xl font-semibold tracking-normal text-slate-950 sm:text-5xl">
            Manage the daily content workflow from one secure workspace.
          </h1>
          <p className="mt-5 text-lg leading-8 text-slate-700">
            Review inbound sources, shape bilingual drafts, schedule posts, and keep publishing
            activity moving with admin and editor access.
          </p>
        </section>

        <section className="border border-slate-200 bg-white p-6 shadow-sm">
          <div className="border-coral border-l-4 pl-4">
            <h2 className="text-2xl font-semibold tracking-normal">Sign in</h2>
            <p className="mt-2 text-sm leading-6 text-slate-600">
              Continue with the account your administrator approved for this workspace.
            </p>
          </div>

          <form className="mt-8 space-y-5" onSubmit={(event) => handleSubmit(event, auth.signIn)}>
            <button
              type="submit"
              className="bg-pine hover:bg-pine/90 focus-visible:ring-coral w-full px-4 py-3 text-sm font-semibold text-white shadow-sm focus:outline-none focus-visible:ring-2 focus-visible:ring-offset-2"
            >
              Continue to sign in
            </button>
          </form>

          {auth.status === "unknown" ? (
            <p className="mt-5 text-sm text-slate-500">Checking existing session...</p>
          ) : null}
        </section>
      </div>
    </main>
  );
}

function handleSubmit(event: FormEvent<HTMLFormElement>, signIn: () => void) {
  event.preventDefault();
  signIn();
}
