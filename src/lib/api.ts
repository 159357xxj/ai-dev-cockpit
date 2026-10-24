/**
 * 唯一允许调用 Tauri IPC 的模块（docs/ARCHITECTURE.md §3）。
 * 页面与组件不得直接 import '@tauri-apps/api'。
 */
import { invoke } from '@tauri-apps/api/core';

import type {
  CancelOutcome,
  DiagnosticRequest,
  IpcError,
  IpcErrorCode,
  JobHandle,
  SystemSnapshot,
} from './types';

export function isTauriRuntime(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

/** 把任意异常规整为 IpcError 形状，保证 UI 只处理一种错误结构。 */
export function normalizeIpcError(raw: unknown): IpcError {
  if (raw && typeof raw === 'object' && 'code' in raw && 'message' in raw) {
    const candidate = raw as { code?: unknown; message?: unknown; detail?: unknown };
    if (typeof candidate.code === 'string' && typeof candidate.message === 'string') {
      const code = candidate.code as IpcErrorCode;
      return {
        code,
        message: candidate.message,
        detail: typeof candidate.detail === 'string' ? candidate.detail : null,
      };
    }
  }
  if (raw instanceof Error) {
    return { code: 'internal', message: raw.message, detail: null };
  }
  return { code: 'internal', message: String(raw), detail: null };
}

async function callIpc<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (!isTauriRuntime()) {
    throw normalizeIpcError({
      code: 'not_available',
      message: '未在 Tauri 桌面环境中运行（当前为浏览器预览，IPC 不可用）',
    });
  }
  try {
    // 参数键一律 snake_case（D-013：后端命令宏已声明 rename_all = "snake_case"）。
    return await invoke<T>(command, args);
  } catch (raw) {
    throw normalizeIpcError(raw);
  }
}

export const api = {
  /** 系统快照（Phase 2 起返回真实数据；当前返回 not_available）。 */
  getSystemSnapshot: (): Promise<SystemSnapshot> =>
    callIpc<SystemSnapshot>('get_system_snapshot'),

  /** 启动诊断（Phase 1 桩：校验真实执行，探测返回 not_available）。 */
  startDiagnostic: (request: DiagnosticRequest): Promise<JobHandle> =>
    callIpc<JobHandle>('start_diagnostic', { request }),

  /** 取消诊断任务（Phase 2 起生效）。 */
  cancelDiagnostic: (jobId: string): Promise<CancelOutcome> =>
    callIpc<CancelOutcome>('cancel_diagnostic', { job_id: jobId }),
};
