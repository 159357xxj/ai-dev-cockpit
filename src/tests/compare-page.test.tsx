/**
 * 路径对比页（真实模式）测试：多模式顺序执行、样本不共享、取消、校验与空态。
 * mock '@/lib/api' 的 runDiagnostic/cancelDiagnostic，按调用次序回放不同模式的事件。
 */
import { cleanup, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { RouterProvider } from 'react-router';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { DiagnosticRunHandlers } from '@/lib/api';
import type { ProbeSample } from '@/lib/types';
import { SettingsProvider } from '@/app/settings';
import { createAppRouter } from '@/app/router';

const cancelMock = vi.fn();
const diagnoseRunsMock = vi.fn();
const handlerStack: DiagnosticRunHandlers[] = [];
let requestLog: Array<{ mode: string; manual: string | null }> = [];

vi.mock('@/lib/api', () => ({
  runDiagnostic: vi.fn((request: { route_mode: string; manual_local_proxy_url: string | null }, handlers: DiagnosticRunHandlers) => {
    handlerStack.push(handlers);
    requestLog.push({ mode: request.route_mode, manual: request.manual_local_proxy_url });
    return Promise.resolve({ jobId: `job-${handlerStack.length}`, unsubscribe: vi.fn() });
  }),
  cancelDiagnostic: (...args: unknown[]) => cancelMock(...args),
  normalizeIpcError: (raw: unknown) => raw as { code: string; message: string },
  api: {
    cancelDiagnostic: (...args: unknown[]) => cancelMock(...args),
    diagnoseRuns: (...args: unknown[]) => diagnoseRunsMock(...args),
  },
}));

function renderCompare(): void {
  render(
    <SettingsProvider>
      <RouterProvider router={createAppRouter('memory', ['/compare'])} />
    </SettingsProvider>,
  );
}

function httpsSample(attemptNo: number, status: number, success: boolean): ProbeSample {
  return {
    target_id: 'github',
    probe_type: 'https',
    route_mode: 'no_app_proxy',
    attempt_no: attemptNo,
    success,
    duration_ms: success ? 500 + attemptNo * 10 : null,
    http_status: status,
    http_outcome: status === 200 ? 'ok_2xx' : 'auth_required_401_403',
    error: null,
    tested_at: '2026-10-25T00:00:00Z',
  };
}

function emitRun(handlers: DiagnosticRunHandlers, statuses: Array<[number, number]>): void {
  const samples = statuses.map(([attempt, status]) =>
    httpsSample(attempt, status, status === 200),
  );
  for (const sample of samples) {
    handlers.onProgress?.({
      job_id: 'job-x',
      percent: 50,
      status: 'sample_finished',
      message: null,
      sample,
    });
  }
  // 终态 run 是权威结果：必须携带 targets（页面据此整体替换样本）。
  handlers.onCompleted?.({
    job_id: 'job-x',
    run: {
      run_id: 'run-x',
      started_at: '2026-10-25T00:00:00Z',
      finished_at: '2026-10-25T00:00:02Z',
      status: 'completed',
      route_mode: 'no_app_proxy',
      repeats: 3,
      app_version: '0.1.0',
      targets: [{ target_id: 'github', samples, aggregates: [] }],
    },
  });
}

beforeEach(() => {
  window.localStorage.clear();
  window.localStorage.setItem(
    'adc.settings.v1',
    JSON.stringify({ theme: 'dark', demoMode: false }),
  );
  handlerStack.length = 0;
  requestLog = [];
  cancelMock.mockReset();
  diagnoseRunsMock.mockReset();
  diagnoseRunsMock.mockResolvedValue([]);
});

afterEach(() => {
  cleanup();
  window.localStorage.clear();
});

describe('路径对比页（真实模式）', () => {
  it('未选择模式时提示校验错误', async () => {
    renderCompare();
    const user = userEvent.setup();
    // 取消唯一默认选中的模式
    await user.click(screen.getByRole('button', { name: /无应用代理/ }));
    await user.click(screen.getByRole('button', { name: /开始对比/ }));
    expect(screen.getByText('至少选择一种要对比的请求模式')).toBeInTheDocument();
  });

  it('顺序执行多模式并渲染各模式统计（样本互不共享）', async () => {
    renderCompare();
    const user = userEvent.setup();

    // 选择 no_app_proxy（默认）+ 库默认
    await user.click(screen.getByRole('button', { name: /库默认/ }));
    await user.click(screen.getByRole('button', { name: /开始对比/ }));

    // 第一个模式的事件：2 成功
    expect(handlerStack.length).toBe(1);
    emitRun(handlerStack[0], [[1, 200], [2, 200]]);

    // 终态后第二个模式启动（页面以 100ms 轮询推进批次）：1 成功 + 1 个 403
    await vi.waitFor(() => expect(handlerStack.length).toBe(2));
    emitRun(handlerStack[1], [[1, 200], [2, 403]]);
    await vi.waitFor(() =>
      expect(screen.getByText('对比结果（github）')).toBeInTheDocument(),
    );
    const successes = screen.getAllByText(/成功/);
    expect(successes.length).toBeGreaterThan(0);
    // 403 样本计入"收到响应"，非 2xx 比例 50%（第二个模式）
    expect(await screen.findByText('50%')).toBeInTheDocument();
    // 请求顺序：先 no_app_proxy 后 library_default
    expect(requestLog.map((entry) => entry.mode)).toEqual(['no_app_proxy', 'library_default']);
    // 批次结束后用两个 run 请求规则诊断（R10 跨运行规则的输入）
    await vi.waitFor(() => expect(diagnoseRunsMock).toHaveBeenCalledTimes(1));
    const runsArg = diagnoseRunsMock.mock.calls[0][0] as Array<{ run_id: string }>;
    expect(runsArg).toHaveLength(2);
    // 无演示标识（真实模式）
    expect(screen.queryAllByText('演示数据')).toHaveLength(0);
  });

  it('取消当前模式后不再继续后续模式', async () => {
    cancelMock.mockResolvedValue({ cancelled: true });
    renderCompare();
    const user = userEvent.setup();

    await user.click(screen.getByRole('button', { name: /库默认/ }));
    await user.click(screen.getByRole('button', { name: /开始对比/ }));

    // 第一个模式被取消（onCompleted 带 cancelled 状态）
    handlerStack[0]?.onCompleted?.({
      job_id: 'job-1',
      run: {
        run_id: 'run-1',
        started_at: '2026-10-25T00:00:00Z',
        finished_at: '2026-10-25T00:00:01Z',
        status: 'cancelled',
        route_mode: 'no_app_proxy',
        repeats: 3,
        app_version: '0.1.0',
        targets: [],
      },
    });

    expect(cancelMock).not.toHaveBeenCalled(); // 本测试直接以终态模拟取消结果
    // 只应发出一次 runDiagnostic（第二个模式不再启动）
    expect(handlerStack.length).toBe(1);
  });

  it('手动代理模式需要合法地址', async () => {
    renderCompare();
    const user = userEvent.setup();

    await user.click(screen.getByRole('button', { name: /手动本地代理/ }));
    await user.type(screen.getByLabelText('本地代理地址'), 'http://proxy.example.com:8080');
    await user.click(screen.getByRole('button', { name: /开始对比/ }));

    expect(screen.getByText(/代理主机仅允许 127\.0\.0\.1/)).toBeInTheDocument();
  });
});
