import { Cable } from "lucide-react";
import brandAssets from "@/assets/providers/brands.json";

const assets = import.meta.glob<string>("../../assets/providers/*.{svg,png,jpg,jpeg,webp}", { eager: true, query: "?url&no-inline", import: "default" });
interface ProviderIdentity { name: string; base_url: string }
type BrandAsset = { id: string; label: string; keywords: string[]; file: string; aliases?: string[]; monochrome?: boolean };
export const BRAND_ICONS: BrandAsset[] = brandAssets;
export function modelBrand(name: string): string | undefined {
  const model = name.toLowerCase();
  if (/gpt|o[134][-.]|codex|openai/.test(model)) return "openai";
  if (/claude|anthropic/.test(model)) return "claude";
  if (/glm|zhipu|智谱/.test(model)) return "zhipu";
  return brands.find((brand) => brand.names.some((label) => model.includes(label)))?.id
    ?? BRAND_ICONS.find((brand) => model === brand.id || model.startsWith(`${brand.id}-`) || model.startsWith(`${brand.id} `))?.id;
}
const brands = [
  { id: "openai", hosts: ["openai.com"], names: ["openai", "codex"] },
  { id: "claude", hosts: ["anthropic.com"], names: ["anthropic", "claude"] },
  { id: "deepseek", hosts: ["deepseek.com"], names: ["deepseek", "深度求索"] },
  { id: "gemini", hosts: ["googleapis.com"], names: ["gemini", "google"] },
  { id: "qwen", hosts: ["aliyuncs.com"], names: ["qwen", "通义", "百炼"] },
  { id: "kimi", hosts: ["moonshot.cn", "moonshot.ai", "kimi.com"], names: ["kimi", "moonshot", "月之暗面"] },
  { id: "minimax", hosts: ["minimax.io", "minimaxi.com"], names: ["minimax"] },
  { id: "siliconflow", hosts: ["siliconflow.cn", "siliconflow.com"], names: ["siliconflow", "硅基流动"] },
  { id: "openrouter", hosts: ["openrouter.ai"], names: ["openrouter"] },
  { id: "ollama", hosts: [], names: ["ollama"] },
  { id: "zhipu", hosts: ["bigmodel.cn", "z.ai"], names: ["zhipu", "智谱", "z.ai"] },
  { id: "mistral", hosts: ["mistral.ai"], names: ["mistral"] },
  { id: "longcat", hosts: ["longcat.chat"], names: ["longcat"] },
];
const normalizeBrandName = (name: string) => name.trim().toLowerCase();
// Icon-search keywords include category tags and other companies' model names.
// Ambiguous keywords need an explicit alias in the manifest for auto-detection.
const genericKeywords = new Set([
  "aggregator", "relay", "gateway", "cloud", "code", "codes", "coding plan",
  "third-party", "api key", "hub", "mix", "router", "chat", "us", "token", "run", "model market",
]);
const keywordOwners = new Map<string, Set<string>>();
for (const { id, keywords } of BRAND_ICONS) {
  for (const keyword of keywords) {
    const name = normalizeBrandName(keyword);
    const owners = keywordOwners.get(name) ?? new Set<string>();
    owners.add(id);
    keywordOwners.set(name, owners);
  }
}
const brandNames = BRAND_ICONS.flatMap(({ id, label }) =>
  [...new Set([id, label].map(normalizeBrandName))].map((name) => ({ id, name })),
);
const brandAliases = BRAND_ICONS.flatMap(({ id, aliases }) =>
  (aliases ?? []).map((name) => ({ id, name: normalizeBrandName(name) })),
);
const knownAliases = brands.flatMap(({ id, names }) => names.map((name) => ({ id, name })));
const keywordAliases = [...keywordOwners].flatMap(([name, owners]) =>
  name.length >= 3 && owners.size === 1 && !genericKeywords.has(name)
    ? [{ id: [...owners][0], name }] : [],
);
const providerNameMatchers = [...brandNames, ...brandAliases, ...knownAliases, ...keywordAliases]
  .filter(({ name }) => name.length > 0)
  .sort((a, b) => b.name.length - a.name.length)
  .map(({ id, name }) => {
    const escaped = name.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
    const start = /^[a-z0-9]/.test(name) ? "(^|[^a-z0-9])" : "";
    const end = /[a-z0-9]$/.test(name) ? "($|[^a-z0-9])" : "";
    return { id, pattern: new RegExp(`${start}${escaped}${end}`) };
  });

function providerNameBrand(name: string): string | undefined {
  const value = normalizeBrandName(name);
  // A brand's own name wins over aliases referring to it from another brand.
  return brandNames.find((brand) => brand.name === value)?.id
    ?? providerNameMatchers.find(({ pattern }) => pattern.test(value))?.id;
}

export function providerBrand(provider: ProviderIdentity): string | undefined {
  let hostname = "";
  try { hostname = new URL(provider.base_url).hostname.toLowerCase(); } catch { /* Unsaved URL. */ }
  const byHost = brands.find((brand) => brand.hosts.some((host) => hostname === host || hostname.endsWith(`.${host}`)));
  if (byHost) return byHost.id;
  return providerNameBrand(provider.name);
}
export function ProviderIcon({ provider, brand, size = 20 }: { provider?: ProviderIdentity; brand?: string; size?: number }) {
  const id = brand ?? (provider ? providerBrand(provider) : undefined);
  const asset = BRAND_ICONS.find((item) => item.id === id);
  const file = asset?.file ?? `${id}.svg`;
  const src = id ? assets[`../../assets/providers/${file}`] : undefined;
  return <span className={`provider-brand-icon${asset?.monochrome ? " monochrome" : ""}`} data-brand={id} style={{ width: size, height: size }} aria-hidden="true">{src ? <img src={src} alt="" width={size} height={size} /> : <Cable size={size} />}</span>;
}
