export type ProviderId =
  | "gemini"
  | "openai"
  | "azure_openai"
  | "anthropic"
  | "openrouter"
  | "groq"
  | "together"
  | "fireworks"
  | "perplexity"
  | "mistral"
  | "cohere"
  | "deepseek"
  | "huggingface"
  | "custom";

export type ApiProviderOption = {
  id: ProviderId;
  label: string;
  placeholder?: string;
  hint?: string;
};

export const API_PROVIDERS: ApiProviderOption[] = [
  {
    id: "gemini",
    label: "Google Gemini (AI Studio / Vertex)",
    placeholder: "AIza... or OAuth",
    hint: "Primary tone engine",
  },
  { id: "openai", label: "OpenAI", placeholder: "sk-..." },
  { id: "azure_openai", label: "Azure OpenAI", placeholder: "Azure API key" },
  { id: "anthropic", label: "Anthropic Claude", placeholder: "sk-ant-..." },
  { id: "openrouter", label: "OpenRouter", placeholder: "sk-or-..." },
  { id: "groq", label: "Groq", placeholder: "gsk_..." },
  { id: "together", label: "Together", placeholder: "together_xxx" },
  { id: "fireworks", label: "Fireworks", placeholder: "fk-..." },
  { id: "perplexity", label: "Perplexity", placeholder: "pplx-..." },
  { id: "mistral", label: "Mistral", placeholder: "sk-..." },
  { id: "cohere", label: "Cohere", placeholder: "api token" },
  { id: "deepseek", label: "DeepSeek", placeholder: "sk-..." },
  { id: "huggingface", label: "Hugging Face", placeholder: "hf_..." },
  {
    id: "custom",
    label: "Custom / self-hosted",
    placeholder: "Bearer token",
    hint: "Use with a custom endpoint",
  },
];
