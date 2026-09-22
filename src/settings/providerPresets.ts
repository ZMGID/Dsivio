import { VIDEO_PROVIDER_PRESETS } from '../data/videoModels'
import type { ProviderApiFormat } from '../api/tauri'

// Presets prefill connection metadata. Official video suggestions are supplied by
// the shared catalog; all models still require explicit user enablement.

export type ProviderPreset = {
  comfy?: boolean
  oauth?: 'codex' | 'kimi' | 'antigravity'
  name: string
  /** Native API base URL, including its version prefix when required. */
  baseUrl: string
  /** 申请 API Key 的页面（在 API 密钥区显示「获取 API Key」引导链接）。本地/无需 key 的可省略。 */
  apiKeyUrl?: string
  /** 接口协议，省略即 openai_chat。Grok 之类有专属协议的必须写明，否则一键添加出来是错的。 */
  apiFormat?: ProviderApiFormat
  /** 赞助位：预设网格置顶并打标。 */
  sponsored?: boolean
}

/** 媒体预设统一来自目录；随后保留聊天、套餐与本地供应商。 */
export const PROVIDER_PRESETS: ProviderPreset[] = [
  { name: 'ComfyUI 本地', baseUrl: 'http://127.0.0.1:8188', comfy: true },
  ...VIDEO_PROVIDER_PRESETS.map(p => ({
    name: p.name, baseUrl: p.baseUrl, apiKeyUrl: p.apiKeyUrl, apiFormat: p.apiFormat as ProviderApiFormat,
  })),
  { name: 'Codex OAuth', baseUrl: 'https://chatgpt.com/backend-api/codex', apiFormat: 'openai_responses', oauth: 'codex' },
  { name: 'Kimi OAuth', baseUrl: 'https://api.kimi.com/coding/v1', oauth: 'kimi' },
  { name: 'Antigravity OAuth', baseUrl: 'https://daily-cloudcode-pa.googleapis.com', apiFormat: 'gemini', oauth: 'antigravity' },
  { name: 'OpenCode Free', baseUrl: 'https://opencode.ai/zen/v1' },
  {
    name: 'Hezubus',
    baseUrl: 'https://hezubus.cc/',
    apiKeyUrl: 'https://hezubus.cc/console',
    sponsored: true,
  },
  {
    name: 'Kimi for Coding',
    baseUrl: 'https://api.kimi.com/coding/v1',
    apiKeyUrl: 'https://www.kimi.com/code/console',
  },
  {
    name: 'GLM Coding Plan',
    baseUrl: 'https://open.bigmodel.cn/api/coding/paas/v4',
    apiKeyUrl: 'https://www.bigmodel.cn/coding-plan/personal/overview',
  },
  {
    name: 'Xiaomi Token Plan',
    baseUrl: 'https://token-plan-cn.xiaomimimo.com/v1',
    apiKeyUrl: 'https://mimo.mi.com',
  },
  {
    name: 'MiniMax Token Plan',
    baseUrl: 'https://api.minimaxi.com/v1',
    apiKeyUrl: 'https://platform.minimaxi.com/user-center/payment/token-plan',
  },
  {
    name: 'OpenCode Go',
    baseUrl: 'https://opencode.ai/zen/go/v1',
    apiKeyUrl: 'https://opencode.ai/auth',
  },
  {
    name: 'DeepSeek',
    baseUrl: 'https://api.deepseek.com/v1',
    apiKeyUrl: 'https://platform.deepseek.com/api_keys',
  },
  {
    name: 'OpenAI',
    baseUrl: 'https://api.openai.com/v1',
    apiKeyUrl: 'https://platform.openai.com/api-keys',
  },
  {
    name: 'Anthropic',
    baseUrl: 'https://api.anthropic.com',
    apiKeyUrl: 'https://console.anthropic.com/settings/keys',
    apiFormat: 'anthropic_messages',
  },
  {
    name: 'Moonshot',
    baseUrl: 'https://api.moonshot.cn/v1',
    apiKeyUrl: 'https://platform.moonshot.cn/console/api-keys',
  },
  {
    name: 'GLM',
    baseUrl: 'https://open.bigmodel.cn/api/paas/v4',
    apiKeyUrl: 'https://open.bigmodel.cn/usercenter/apikeys',
  },
  {
    name: 'Qwen',
    baseUrl: 'https://dashscope.aliyuncs.com/compatible-mode/v1',
    apiKeyUrl: 'https://bailian.console.aliyun.com/?tab=model#/api-key',
  },
  {
    name: 'SiliconFlow',
    baseUrl: 'https://api.siliconflow.cn/v1',
    apiKeyUrl: 'https://cloud.siliconflow.cn/account/ak',
  },
  {
    name: 'OpenRouter',
    baseUrl: 'https://openrouter.ai/api/v1',
    apiKeyUrl: 'https://openrouter.ai/keys',
  },
  {
    name: 'Groq',
    baseUrl: 'https://api.groq.com/openai/v1',
    apiKeyUrl: 'https://console.groq.com/keys',
  },
  {
    name: 'StepFun',
    baseUrl: 'https://api.stepfun.com/v1',
    apiKeyUrl: 'https://platform.stepfun.com/interface-key',
  },
  {
    name: '302.AI',
    baseUrl: 'https://api.302.ai/v1',
    apiKeyUrl: 'https://302.ai',
  },
  {
    name: 'Mistral',
    baseUrl: 'https://api.mistral.ai/v1',
    apiKeyUrl: 'https://console.mistral.ai/api-keys',
  },
  {
    name: 'Together',
    baseUrl: 'https://api.together.xyz/v1',
    apiKeyUrl: 'https://api.together.ai/settings/api-keys',
  },
  {
    name: 'Fireworks',
    baseUrl: 'https://api.fireworks.ai/inference/v1',
    apiKeyUrl: 'https://fireworks.ai/account/api-keys',
  },
  {
    name: 'Perplexity',
    baseUrl: 'https://api.perplexity.ai',
    apiKeyUrl: 'https://www.perplexity.ai/settings/api',
  },
  {
    name: 'Ollama',
    baseUrl: 'https://ollama.com/v1',
    apiKeyUrl: 'https://ollama.com/settings/keys',
  },
  {
    name: 'Ollama Local',
    baseUrl: 'http://127.0.0.1:11434/v1',
  },
  {
    name: 'LM Studio',
    baseUrl: 'http://127.0.0.1:1234/v1',
  },
  {
    name: 'ModelScope',
    baseUrl: 'https://api-inference.modelscope.cn/v1',
    apiKeyUrl: 'https://modelscope.cn/my/myaccesstoken',
  },
  {
    name: 'GitHub Models',
    baseUrl: 'https://models.github.ai/inference',
    apiKeyUrl: 'https://github.com/settings/tokens',
  },
]
