export function proxyHostForUrl(host: string): string {
  if (host === "0.0.0.0") return "127.0.0.1";
  if (host === "::") return "[::1]";
  return host.includes(":") && !host.startsWith("[") ? `[${host}]` : host;
}

export function proxyOrigin(host: string, port: number): string {
  return `http://${proxyHostForUrl(host)}:${port}`;
}

export function proxyBaseUrl(host: string, port: number): string {
  return `${proxyOrigin(host, port)}/v1`;
}
