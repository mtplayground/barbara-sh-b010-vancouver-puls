import { useCallback, useEffect, useMemo, type ReactNode } from "react";
import { useQuery } from "@tanstack/react-query";

import { apiClient } from "../api/client";
import type { AuthSessionResponse } from "../api/types";
import { useAuthStore } from "./authStore";
import { AuthContext, type AuthContextValue } from "./authContextValue";
import type { AuthSnapshot } from "./types";

interface AuthProviderProps {
  children: ReactNode;
}

export function AuthProvider({ children }: AuthProviderProps) {
  const status = useAuthStore((state) => state.status);
  const user = useAuthStore((state) => state.user);
  const roles = useAuthStore((state) => state.roles);
  const setAuthSnapshot = useAuthStore((state) => state.setAuthSnapshot);
  const resetAuth = useAuthStore((state) => state.resetAuth);
  const session = useQuery({
    queryKey: ["auth", "session"],
    queryFn: apiClient.getAuthSession,
    retry: false,
  });

  useEffect(() => {
    if (session.isLoading) {
      return;
    }

    if (session.data) {
      setAuthSnapshot(sessionToSnapshot(session.data));
      return;
    }

    if (session.isError) {
      resetAuth();
      setAuthSnapshot({
        status: "anonymous",
        user: null,
        roles: [],
      });
    }
  }, [resetAuth, session.data, session.isError, session.isLoading, setAuthSnapshot]);

  const signIn = useCallback(() => {
    window.location.assign(apiClient.getLoginUrl());
  }, []);

  const value = useMemo<AuthContextValue>(
    () => ({
      status,
      user,
      roles,
      isAuthenticated: status === "authenticated",
      signIn,
      setAuthSnapshot,
      resetAuth,
    }),
    [resetAuth, roles, setAuthSnapshot, signIn, status, user],
  );

  return <AuthContext.Provider value={value}>{children}</AuthContext.Provider>;
}

function sessionToSnapshot(session: AuthSessionResponse): AuthSnapshot {
  if (!session.authenticated || !session.user) {
    return {
      status: "anonymous",
      user: null,
      roles: [],
    };
  }

  return {
    status: "authenticated",
    user: {
      id: session.user.sub,
      email: session.user.email,
      name: session.user.name,
      pictureUrl: session.user.picture_url,
    },
    roles: [session.user.role],
  };
}
