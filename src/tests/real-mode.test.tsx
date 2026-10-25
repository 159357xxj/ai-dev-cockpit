/**
 * 真实模式（非演示）诊断页事件流测试：
 * mock '@/lib/api' 的事件发射，验证 UI 从进度 → 终态的完整接线，
 * 以及取消按钮到 cancelDiagnostic 的调用。不依赖 Tauri 运行时。
 */
import { cleanup, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { RouterProvider } from 'react-router';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { DiagnosticRunHandlers } from '@/lib/api';
import type { DiagnosticRun, ProbeSample } from '@/lib/types';
import { SettingsProvider } from '@/app/settings';
import { createAppRouter } from '@/app/router';

const cancelMock = vi.fn();
let capturedHandlers: DiagnosticRunHandlers | null = null;

const diagnoseRunsMock = vi.fn();

vi.mock('@/lib/api', () => ({
  runDiagnostic: vi.fn((_request: unknown, handlers: DiagnosticRunHandlers) => {
    capturedHandlers = handlers;
    return Promise.resolve({ jobId: 'job-1', unsubscribe: vi.fn() });
  }),
  cancelDiagnostic: (...args: unknown[]) => cancelMock(...args),
  normalizeIpcError: (raw: unknown) => raw as { code: string; message: string },
  api: {
    cancelDiagnostic: (...args: unknown[]) => cancelMock(...args),
    diagnoseRuns: (...args: unknown[]) => diagnoseRunsMock(...args),
  },
}));

function renderDiagnostics(): void {
  render(
    <SettingsProvider>
      <RouterProvider router={createAppRouter('memory', ['/diagnostics'])} />
    </SettingsProvider>,
  );
}

function sample(partial: Partial<ProbeSample>): ProbeSample {
  return {
    target_id: 'github',
    probe_type: 'https',
    route_mode: 'no_app_proxy',
    attempt_no: 1,
    success: true,
    duration_ms: 830,
    http_status: 200,
    http_outcome: 'ok_2xx',
    error: null,
    tested_at: '2026-10-24T00:00:00Z',
    ...partial,
  };
}

function fakeRun(status: 'completed' | 'cancelled'): DiagnosticRun {
  return {
    run_id: 'run-1',
    started_at: '2026-10-24T00:00:00Z',
    finished_at: '2026-10-24T00:00:02Z',
    status,
    route_mode: 'no_app_proxy',
    repeats: 3,
    app_version: '0.1.0',
    targets: [
      {
        target_id: 'github',
        samples: [
          sample({ probe_type: 'dns', attempt_no: 1, success: true, duration_ms: 12, http_status: null, http_outcome: null }),
          sample({ probe_type: 'tcp', attempt_no: 1, success: true, duration_ms: 45, http_status: null, http_outcome: null }),
          sample({ attempt_no: 1 }),
          sample({ attempt_no: 2, success: false, duration_ms: null, http_status: 403, http_outcome: 'auth_required_401_403' }),
        ],
        aggregates: [],
      },
    ],
  };
}

beforeEach(() => {
  window.localStorage.clear();
  window.localStorage.setItem(
    'adc.settings.v1',
    JSON.stringify({ theme: 'dark', demoMode: false }),
  );
  capturedHandlers = null;
  cancelMock.mockReset();
  diagnoseRunsMock.mockReset();
  diagnoseRunsMock.mockResolvedValue([]);
});

afterEach(() => {
  cleanup();
  window.localStorage.clear();
});

describe('真实模式诊断页', () => {
  it('进度事件实时渲染样本，终态 run 覆盖为权威结果', async () => {
    renderDiagnostics();
    const user = userEvent.setup();

    await user.click(screen.getByRole('button', { name: /GitHub/ }));
    await user.click(screen.getByRole('button', { name: /开始诊断/ }));

    // 进度：单样本到达即渲染。
    capturedHandlers?.onProgress?.({
      job_id: 'job-1',
      percent: 25,
      status: 'sample_finished',
      message: null,
      sample: sample({ probe_type: 'dns', attempt_no: 1, success: true, duration_ms: 12, http_status: null, http_outcome: null }),
    });
    expect(await screen.findByText('DNS')).toBeInTheDocument();

    // 终态：run 为权威结果（含 403 样本）。
    capturedHandlers?.onCompleted?.({ job_id: 'job-1', run: fakeRun('completed') });
    expect(await screen.findByText('已完成')).toBeInTheDocument();
    expect(screen.getByText('收到响应（403）')).toBeInTheDocument();
    expect(screen.getByText(/非2xx 50%/)).toBeInTheDocument();
    // 终态后必须请求规则诊断（纯函数 IPC，输入为该 run）
    await vi.waitFor(() => expect(diagnoseRunsMock).toHaveBeenCalledTimes(1));
    const runsArg = diagnoseRunsMock.mock.calls[0][0] as Array<{ run_id: string }>;
    expect(runsArg[0].run_id).toBe('run-1');
    // 真实模式绝不出现演示标识。
    expect(screen.queryAllByText('演示数据')).toHaveLength(0);
  });

  it('取消按钮调用 cancelDiagnostic，取消终态如实展示', async () => {
    cancelMock.mockResolvedValue({ cancelled: true });
    renderDiagnostics();
    const user = userEvent.setup();

    await user.click(screen.getByRole('button', { name: /GitHub/ }));
    await user.click(screen.getByRole('button', { name: /开始诊断/ }));
    await user.click(screen.getByRole('button', { name: /取消/ }));

    expect(cancelMock).toHaveBeenCalledWith('job-1');
    capturedHandlers?.onCompleted?.({ job_id: 'job-1', run: fakeRun('cancelled') });
    expect(await screen.findByText(/已取消（已完成样本保留）/)).toBeInTheDocument();
  });

  it('Phase 3：三种连接模式在真实模式下全部可用', () => {
    renderDiagnostics();
    for (const label of [/库默认/, /手动本地代理/, /无应用代理/]) {
      const option = screen.getByRole('option', { name: label }) as HTMLOptionElement;
      expect(option.disabled).toBe(false);
    }
  });
});
