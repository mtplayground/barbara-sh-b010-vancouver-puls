const readinessItems = [
  "React SPA workspace",
  "Tailwind design system entry point",
  "Rust/Axum service boundary",
];

export function App() {
  return (
    <main className="bg-paper text-ink min-h-screen">
      <section className="mx-auto flex min-h-screen w-full max-w-5xl flex-col justify-center px-6 py-12">
        <p className="text-pine mb-4 text-sm font-semibold uppercase tracking-wide">
          Monorepo initialized
        </p>
        <h1 className="max-w-3xl text-4xl font-semibold leading-tight sm:text-5xl">
          React client and Axum API are ready for feature work.
        </h1>
        <p className="mt-6 max-w-2xl text-lg leading-8 text-slate-700">
          This first slice establishes the application boundaries, build scripts, and a runnable
          local development setup.
        </p>
        <div className="mt-10 grid gap-4 sm:grid-cols-3">
          {readinessItems.map((item) => (
            <div key={item} className="border-coral border-l-4 bg-white px-5 py-4 shadow-sm">
              <p className="text-sm font-medium text-slate-900">{item}</p>
            </div>
          ))}
        </div>
      </section>
    </main>
  );
}
