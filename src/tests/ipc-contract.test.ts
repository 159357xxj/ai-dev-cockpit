/**
 * IPC 线上契约测试（JS 侧）：
 * - 事件订阅必须先于 start_diagnostic 调用建立（竞争防护）；
 * - invoke 命令名与参数键严格 snake_case（D-013）；
 * - 事件按 job_id 过滤，其它任务的事件不得串扰。
 *
 * 限制说明：本测试 mock 了 @tauri-apps/api，验证的是前端发出的线上形状；
 * Rust 命令宏的参数名匹配行为由 rename_all 源码核查 + `tauri dev` 人工
 * 联调覆盖（见 TEST_PLAN 限制记录）。
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const invokeMock = vi.fn();
const listenCalls: Array<{ event: string; handler: (event: { payload: unknown }) => void }> = [];

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: (event: string, handler: (event: { payload: unknown }) => void) => {
    listenCalls.push({ event, handler });
    return Promise.resolve(() => {
      const index = listenCalls.findIndex((call) => call.event === event);
      if (index >= 0) listenCalls.splice(index, 1);
    });
  },
}));

import { api, runDiagnostic } from '@/lib/api';
import type { DiagnosticRequest } from '@/lib/types';

function fakeTauriRuntime(): void {
  Object.defineProperty(window, '__TAURI_INTERNALS__', {
    value: {},
    configurable: true,
  });
}

function fire(event: string, payload: unknown): void {
  for (const call of listenCalls) {
    if (call.event === event) call.handler({ payload });
  }
}

beforeEach(() => {
  fakeTauriRuntime();
  invokeMock.mockReset();
  listenCalls.length = 0;
});

afterEach(() => {
  delete (window as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
});

describe('IPC 线上契约', () => {
  it('订阅先于启动，invoke 参数为 snake_case', async () => {
    invokeMock.mockImplementation(async (_command: string) => {
      expect(listenCalls.map((call) => call.event)).toEqual(
        expect.arrayContaining(['diagnostic://progress', 'diagnostic://completed', 'diagnostic://failed']),
      );
      return { job_id: 'job-1' };
    });

    const request: DiagnosticRequest = {
      target_ids: ['github', 'openai-api'],
      route_mode: 'no_app_proxy',
      manual_local_proxy_url: null,
      repeats: 3,
      timeout_ms: 10000,
    };
    const session = await runDiagnostic(request, {});
    expect(session.jobId).toBe('job-1');
    expect(invokeMock).toHaveBeenCalledWith('start_diagnostic', { request });
  });

  it('cancel_diagnostic 使用 job_id 键（snake_case）', async () => {
    invokeMock.mockResolvedValue({ cancelled: true });
    const outcome = await api.cancelDiagnostic('job-9');
    expect(outcome.cancelled).toBe(true);
    expect(invokeMock).toHaveBeenCalledWith('cancel_diagnostic', { job_id: 'job-9' });
  });

  it('事件按 job_id 过滤，其它任务的事件被丢弃', async () => {
    invokeMock.mockResolvedValue({ job_id: 'job-1' });
    const onProgress = vi.fn();
    const onCompleted = vi.fn();
    const session = await runDiagnostic(
      {
        target_ids: ['github'],
        route_mode: 'no_app_proxy',
        manual_local_proxy_url: null,
        repeats: 3,
        timeout_ms: 10000,
      },
      { onProgress, onCompleted },
    );

    // 其它任务：不应触达本会话的 handler。
    fire('diagnostic://progress', {
      job_id: 'job-other',
      percent: 10,
      status: 'sample_finished',
      message: null,
      sample: null,
    });
    // 本任务进度：触发。
    fire('diagnostic://progress', {
      job_id: 'job-1',
      percent: 33,
      status: 'sample_finished',
      message: null,
      sample: null,
    });
    // 本任务终态：触发一次。
    fire('diagnostic://completed', {
      job_id: 'job-1',
      run: {
        run_id: 'run-1',
        started_at: '2026-10-24T00:00:00Z',
        finished_at: '2026-10-24T00:00:01Z',
        status: 'completed',
        route_mode: 'no_app_proxy',
        repeats: 3,
        app_version: '0.1.0',
        targets: [],
      },
    });

    expect(onProgress).toHaveBeenCalledTimes(1);
    expect(onCompleted).toHaveBeenCalledTimes(1);
    session.unsubscribe();
    expect(listenCalls).toHaveLength(0);
  });

  it('start 与 job_id 赋值之间到达的事件经缓冲回放（竞争防护）', async () => {
    // invoke resolve 的同时后端已发出第一个事件（真实竞态窗口的模拟）。
    invokeMock.mockImplementation(async () => {
      // 此时订阅已建立但 jobId 尚未赋值：事件进入缓冲区。
      fire('diagnostic://progress', {
        job_id: 'job-fast',
        percent: 10,
        status: 'running',
        message: null,
        sample: null,
      });
      return { job_id: 'job-fast' };
    });
    const onProgress = vi.fn();
    const session = await runDiagnostic(
      {
        target_ids: ['github'],
        route_mode: 'no_app_proxy',
        manual_local_proxy_url: null,
        repeats: 3,
        timeout_ms: 10000,
      },
      { onProgress },
    );
    expect(session.jobId).toBe('job-fast');
    expect(onProgress).toHaveBeenCalledTimes(1);
    session.unsubscribe();
  });
});
