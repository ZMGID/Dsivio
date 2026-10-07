import { api, isTauriRuntime } from '../../../api/tauri'
import { graphProblems } from './workflowGraph'
import type { GenerationWorkflow } from './workflowModel'

/** Read-only validation; never submits AI/media tasks. Desktop rules live in the backend. */
export async function checkWorkflow(flow: GenerationWorkflow): Promise<string[]> {
  if (!isTauriRuntime()) return graphProblems(flow)
  try { return await api.checkWorkflow(flow) }
  catch (error) { return [`配置检查失败：${String(error)}`] }
}
