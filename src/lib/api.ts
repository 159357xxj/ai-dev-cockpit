/**
 * 唯一允许调用 Tauri IPC 的模块（docs/ARCHITECTURE.md §3）。
 * 页面与组件不得直接 import '@tauri-apps/api'。
 */
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

import type {
  CancelOutcome,
  CompletedPayload,
  ProxySnapshot,
  DiagnosticRequest,
  FailedPayload,
  IpcError,
  IpcErrorCode,
  JobHandle,
  DiagnosticRun,
  Diagnosis,
  ExportResult,
  ProgressPayload,
  ReportFormat,
  ReportPreview,
  RunQuery,
  RunSummary,
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
  /** 系统快照（后续阶段返回真实数据；当前返回 not_available）。 */
  getSystemSnapshot: (): Promise<SystemSnapshot> =>
    callIpc<SystemSnapshot>('get_system_snapshot'),

  /** Windows 代理配置只读快照（Phase 3：三来源分别标注，全量脱敏，纯本地读取）。 */
  getProxySnapshot: (): Promise<ProxySnapshot> =>
    callIpc<ProxySnapshot>('get_proxy_snapshot'),

  /** 规则诊断（Phase 4：纯函数、离线、确定性；输入为已持有的运行结果）。 */
  diagnoseRuns: (runs: DiagnosticRun[]): Promise<Diagnosis[]> =>
    callIpc<Diagnosis[]>('diagnose_runs', { runs }),

  /** 历史（Phase 5）：列表 / 详情 / 删除 / 清空 / 设置（白名单键）。 */
  listRuns: (query: RunQuery): Promise<RunSummary[]> =>
    callIpc<RunSummary[]>('list_runs', { query }),
  getRun: (runId: string): Promise<DiagnosticRun> =>
    callIpc<DiagnosticRun>('get_run', { run_id: runId }),
  deleteRun: (runId: string): Promise<boolean> =>
    callIpc<boolean>('delete_run', { run_id: runId }),
  clearHistory: (): Promise<number> => callIpc<number>('clear_history', {}),
  getSetting: (key: string): Promise<string | null> =>
    callIpc<string | null>('get_setting', { key }),
  setSetting: (key: string, valueJson: string): Promise<void> =>
    callIpc<void>('set_setting', { key, value_json: valueJson }),
  /** 脱敏报告：先预览（不落盘），确认后导出到应用数据目录 exports/。 */
  previewSanitizedReport: (runId: string, format: ReportFormat): Promise<ReportPreview> =>
    callIpc<ReportPreview>('preview_sanitized_report', { run_id: runId, format }),
  exportSanitizedReport: (
    runId: string,
    format: ReportFormat,
    snapshotId: string,
  ): Promise<ExportResult> =>
    callIpc<ExportResult>('export_sanitized_report', {
      run_id: runId,
      format,
      snapshot_id: snapshotId,
    }),

  /** 启动诊断（低层句柄：一般应使用 runDiagnostic 以获得事件流）。 */
  startDiagnostic: (request: DiagnosticRequest): Promise<JobHandle> =>
    callIpc<JobHandle>('start_diagnostic', { request }),

  /** 取消诊断任务（幂等；已结束返回 cancelled=false）。 */
  cancelDiagnostic: (jobId: string): Promise<CancelOutcome> =>
    callIpc<CancelOutcome>('cancel_diagnostic', { job_id: jobId }),
};

export interface DiagnosticRunHandlers {
  onProgress?: (payload: ProgressPayload) => void;
  onCompleted?: (payload: CompletedPayload) => void;
  onFailed?: (payload: FailedPayload) => void;
}

export interface DiagnosticRunSession {
  jobId: string;
  unsubscribe: () => void;
}

type IncomingEvent =
  | { kind: 'progress'; jobId: string; payload: ProgressPayload }
  | { kind: 'completed'; jobId: string; payload: CompletedPayload }
  | { kind: 'failed'; jobId: string; payload: FailedPayload };

/**
 * 运行一次诊断：先建立事件订阅、再启动任务（消除"监听建立前任务已结束"的竞争）。
 *
 * 订阅先于 invoke 建立；invoke 返回前到达的事件进入缓冲区，拿到 job_id 后
 * 按其过滤回放，其它任务的事件直接丢弃。终态事件（completed/failed）到达后
 * 由调用方决定何时 unsubscribe（本函数不自动退订 completed——run 结果就在其中）。
 */
export async function runDiagnostic(
  request: DiagnosticRequest,
  handlers: DiagnosticRunHandlers,
): Promise<DiagnosticRunSession> {
  if (!isTauriRuntime()) {
    throw normalizeIpcError({
      code: 'not_available',
      message: '未在 Tauri 桌面环境中运行（当前为浏览器预览，IPC 不可用）',
    });
  }

  let jobId: string | null = null;
  const pending: IncomingEvent[] = [];

  const dispatch = (incoming: IncomingEvent): void => {
    if (jobId === null) {
      pending.push(incoming);
      return;
    }
    if (incoming.jobId !== jobId) return; // 其它任务的事件
    if (incoming.kind === 'progress') handlers.onProgress?.(incoming.payload);
    else if (incoming.kind === 'completed') handlers.onCompleted?.(incoming.payload);
    else handlers.onFailed?.(incoming.payload);
  };

  const unlistens = await Promise.all([
    listen<ProgressPayload>('diagnostic://progress', (event) =>
      dispatch({ kind: 'progress', jobId: event.payload.job_id, payload: event.payload }),
    ),
    listen<CompletedPayload>('diagnostic://completed', (event) =>
      dispatch({ kind: 'completed', jobId: event.payload.job_id, payload: event.payload }),
    ),
    listen<FailedPayload>('diagnostic://failed', (event) =>
      dispatch({ kind: 'failed', jobId: event.payload.job_id, payload: event.payload }),
    ),
  ]);

  try {
    const handle = await api.startDiagnostic(request);
    jobId = handle.job_id;
    // 回放 start 与 job_id 赋值之间到达的本任务事件。
    const buffered = pending.splice(0, pending.length).filter((item) => item.jobId === jobId);
    for (const incoming of buffered) dispatch({ ...incoming });
    return {
      jobId,
      unsubscribe: () => unlistens.forEach((unlisten) => unlisten()),
    };
  } catch (raw) {
    unlistens.forEach((unlisten) => unlisten());
    throw normalizeIpcError(raw);
  }
}
