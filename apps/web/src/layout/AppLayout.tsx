import { NavLink, Outlet } from "react-router-dom";

import { useAuth } from "../auth/useAuth";

const navigationItems = [
  { label: "Dashboard", to: "/" },
  { label: "Sources", to: "/sources" },
  { label: "Inbox", to: "/inbox" },
  { label: "Drafts", to: "/drafts" },
  { label: "Calendar", to: "/calendar" },
  { label: "Backup", to: "/backup-library" },
  { label: "Users", to: "/users", adminOnly: true },
  { label: "Settings", to: "/settings" },
];

export function AppLayout() {
  const auth = useAuth();
  const displayName = auth.user?.name ?? auth.user?.email;
  const visibleNavigationItems = navigationItems.filter(
    (item) => !item.adminOnly || auth.roles.includes("admin"),
  );

  if (auth.status === "anonymous") {
    return <Outlet />;
  }

  return (
    <div className="bg-paper min-h-screen text-slate-950">
      <header className="border-b border-slate-200 bg-white">
        <div className="mx-auto flex w-full max-w-7xl flex-col gap-4 px-4 py-4 sm:px-6 lg:flex-row lg:items-center lg:justify-between">
          <div>
            <p className="text-pine text-sm font-semibold uppercase tracking-wide">
              Content workspace
            </p>
            <h1 className="mt-1 text-2xl font-semibold tracking-normal">Publishing operations</h1>
          </div>
          <div className="flex items-center gap-3 text-sm">
            {auth.isAuthenticated ? (
              <div className="flex items-center gap-3">
                {auth.user?.pictureUrl ? (
                  <img
                    src={auth.user.pictureUrl}
                    alt=""
                    className="h-9 w-9 rounded-full border border-slate-200"
                  />
                ) : null}
                <div className="text-right">
                  <p className="font-semibold text-slate-950">{displayName}</p>
                  <p className="text-xs uppercase text-slate-500">{auth.roles.join(", ")}</p>
                </div>
              </div>
            ) : (
              <button
                type="button"
                onClick={auth.signIn}
                className="bg-pine hover:bg-pine/90 focus-visible:ring-coral px-4 py-2 font-semibold text-white shadow-sm focus:outline-none focus-visible:ring-2 focus-visible:ring-offset-2"
              >
                Sign in
              </button>
            )}
          </div>
        </div>
      </header>

      <div className="mx-auto grid w-full max-w-7xl gap-6 px-4 py-6 sm:px-6 lg:grid-cols-[220px_minmax(0,1fr)]">
        <nav aria-label="Primary" className="lg:sticky lg:top-6 lg:self-start">
          <div className="flex gap-2 overflow-x-auto border-b border-slate-200 pb-3 lg:flex-col lg:border-b-0 lg:pb-0">
            {visibleNavigationItems.map((item) => (
              <NavLink
                key={item.to}
                to={item.to}
                className={({ isActive }) =>
                  [
                    "whitespace-nowrap border-l-4 px-3 py-2 text-sm font-medium",
                    isActive
                      ? "border-coral bg-white text-slate-950 shadow-sm"
                      : "border-transparent text-slate-600 hover:bg-white hover:text-slate-950",
                  ].join(" ")
                }
                end={item.to === "/"}
              >
                {item.label}
              </NavLink>
            ))}
          </div>
        </nav>

        <section className="min-w-0">
          <Outlet />
        </section>
      </div>
    </div>
  );
}
