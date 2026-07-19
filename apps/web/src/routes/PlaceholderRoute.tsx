interface PlaceholderRouteProps {
  title: string;
  description: string;
  items: string[];
}

export function PlaceholderRoute({ title, description, items }: PlaceholderRouteProps) {
  return (
    <div className="space-y-6">
      <section className="bg-white px-5 py-5 shadow-sm">
        <p className="text-pine text-sm font-semibold uppercase tracking-wide">Workspace area</p>
        <h2 className="mt-2 text-3xl font-semibold tracking-normal">{title}</h2>
        <p className="mt-3 max-w-3xl text-base leading-7 text-slate-700">{description}</p>
      </section>

      <section className="grid gap-3 md:grid-cols-3">
        {items.map((item) => (
          <article key={item} className="border-coral border-l-4 bg-white px-4 py-4 shadow-sm">
            <p className="text-sm font-medium leading-6 text-slate-700">{item}</p>
          </article>
        ))}
      </section>
    </div>
  );
}
