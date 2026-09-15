import { invoke } from "@tauri-apps/api/core";

export type SharedSecretKey =
  | "oauth_client_id"
  | "bearer_token"
  | "oauth_client_secret"
  | "oauth_password"
  | "oauth_token_secret";

export async function getSharedSecret(key: SharedSecretKey): Promise<string | null> {
  return invoke<string | null>("get_shared_secret", { key });
}

export async function setSharedSecret(key: SharedSecretKey, value: string): Promise<void> {
  return invoke("set_shared_secret", { key, value });
}

export async function setSharedSecrets(updates: Array<{ key: SharedSecretKey; value: string }>): Promise<void> {
  return invoke("set_shared_secrets", { updates });
}

export async function regenerateSharedSecret(key: SharedSecretKey): Promise<string> {
  return invoke<string>("regenerate_shared_secret", { key });
}
