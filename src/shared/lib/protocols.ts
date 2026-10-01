/** 上游 / 客户端协议标识。 */
export type ProviderType = "anthropic" | "openai_chat" | "responses";

interface ProtocolDefinition {
  id: ProviderType;
  name: string;
  shortName: string;
  description: string;
  brand: "openai" | "anthropic";
}

export const PROTOCOLS = [
  { id: "responses", name: "Responses API", shortName: "Responses", description: "Responses · reasoning 参数直通", brand: "openai" },
  { id: "openai_chat", name: "OpenAI Chat Completions", shortName: "Chat", description: "Chat Completions · reasoning_effort", brand: "openai" },
  { id: "anthropic", name: "Anthropic Messages", shortName: "Anthropic", description: "Messages API · thinking budget", brand: "anthropic" },
] as const satisfies readonly ProtocolDefinition[];

export const PROTOCOLS_BY_ID: Record<ProviderType, ProtocolDefinition> = {
  responses: PROTOCOLS[0],
  openai_chat: PROTOCOLS[1],
  anthropic: PROTOCOLS[2],
};
