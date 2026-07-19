import { useMemo, type ReactNode } from "react";

import { useAuthStore } from "./authStore";
import { AuthContext, type AuthContextValue } from "./authContextValue";

interface AuthProviderProps {
  children: ReactNode;
}

export function AuthProvider({ children }: AuthProviderProps) {
  const status = useAuthStore((state) => state.status);
  const user = useAuthStore((state) => state.user);
  const roles = useAuthStore((state) => state.roles);
  const setAuthSnapshot = useAuthStore((state) => state.setAuthSnapshot);
  const resetAuth = useAuthStore((state) => state.resetAuth);

  const value = useMemo<AuthContextValue>(
    () => ({
      status,
      user,
      roles,
      isAuthenticated: status === "authenticated",
      setAuthSnapshot,
      resetAuth,
    }),
    [resetAuth, roles, setAuthSnapshot, status, user],
  );

  return <AuthContext.Provider value={value}>{children}</AuthContext.Provider>;
}
