export const MIN_PORT = 1;
export const MAX_PORT = 65535;

const PROXY_URL_PATTERN = /^(https?|socks5h?):\/\//i;

export function isValidPort(value: unknown): boolean {
  const port = Number(value);
  return Number.isInteger(port) && port >= MIN_PORT && port <= MAX_PORT;
}

export function isValidProxyUrl(value: string): boolean {
  return PROXY_URL_PATTERN.test(value.trim());
}
