import { signal } from "@preact/signals-react";

export interface UserInfo {
  id?: string;
  email?: string;
  username?: string;
  created_at?: string;
  avatar_url?: string;
  [key: string]: unknown;
}

export const authentication = {
  userInfo: signal<UserInfo | null>({
    id: "local_user",
    email: "local@floword.studio",
    username: "Floword Creator",
  }),
  isLoggedIn: signal<boolean>(true),
  token: signal<string | null>("local-session-token"),
  authSessionChecked: signal<boolean>(true),
  isLoading: signal<boolean>(false),
  submittingEmail: signal<boolean>(false),
  submittingOtp: signal<boolean>(false),
};

export function logout() {
  console.log("[Auth] Logout");
}

export function setLogoutStates() {
  console.log("[Auth] setLogoutStates");
}

export function persistLogin() {
  return Promise.resolve(true);
}

export function forceGetUserInfoAndSubcriptions() {
  return Promise.resolve();
}

export function fetchUserInfoAndSubcriptions() {
  return Promise.resolve();
}
