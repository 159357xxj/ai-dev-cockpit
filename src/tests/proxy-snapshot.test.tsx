/**
 * 代理配置快照卡片测试：三来源展示、未知状态不伪造"未启用"、脱敏值原样透出。
 * mock '@/lib/api' 的 getProxySnapshot，不依赖 Tauri 运行时。
 */
import { cleanup, render, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { ProxySnapshotCard } from '@/components/dashboard/ProxySnapshotCard';
import type { ProxySnapshot } from '@/lib/types';

const getProxySnapshotMock = vi.fn();

vi.mock('@/lib/api', () => ({
  api: {
    getProxySnapshot: (...args: unknown[]) => getProxySnapshotMock(...args),
  },
  normalizeIpcError: (raw: unknown) => raw as { code: string; message: string },
}));

afterEach(() => {
  cleanup();
  getProxySnapshotMock.mockReset();
});

function fixture(overrides: Partial<ProxySnapshot> = {}): ProxySnapshot {
  return {
    user_settings: {
      source: 'wininet_api:WinHttpGetIEProxyConfigForCurrentUser',
      captured_at: '2026-10-25T00:00:00Z',
      status: 'ok',
      read_error: null,
      auto_detect: false,
      pac_url: null,
      manual_proxy: '[凭证已隐藏]@127.0.0.1:7890',
      proxy_bypass: null,
    },
    winhttp: {
      source: 'winhttp_api:WinHttpGetDefaultProxyConfiguration',
      captured_at: '2026-10-25T00:00:00Z',
      status: 'ok',
      read_error: null,
      access_type: 'direct(直连，WinHTTP 不使用代理)',
      proxy: null,
      bypass: null,
    },
    env_vars: [
      { name: 'HTTP_PROXY', present: false, value_redacted: null, note: '' },
      {
        name: 'HTTPS_PROXY',
        present: true,
        value_redacted: 'http://[凭证已隐藏]@127.0.0.1:7897',
        note: '',
      },
    ],
    manual_proxy_enabled: true,
    pac_configured: false,
    auto_detect_enabled: false,
    captured_at: '2026-10-25T00:00:00Z',
    note: '配置存在 ≠ 真实请求使用该配置',
    ...overrides,
  };
}

describe('代理配置快照卡片', () => {
  it('展示三来源与脱敏值，并保留免责说明', async () => {
    getProxySnapshotMock.mockResolvedValue(fixture());
    render(<ProxySnapshotCard />);

    expect(
      await screen.findByText(/手动代理 \[凭证已隐藏\]@127\.0\.0\.1:7890/),
    ).toBeInTheDocument();
    expect(screen.getByText(/PAC 未配置/)).toBeInTheDocument();
    expect(screen.getByText(/direct/)).toBeInTheDocument();
    expect(
      screen.getByText(/HTTPS_PROXY=http:\/\/\[凭证已隐藏\]@127\.0\.0\.1:7897/),
    ).toBeInTheDocument();
    expect(screen.getByText(/HTTPS_PROXY=http:\/\/\[凭证已隐藏\]@127.0.0.1:7897/)).toBeInTheDocument();
    expect(screen.getByText(/配置存在 ≠ 真实请求使用该配置/)).toBeInTheDocument();
  });

  it('读取失败显示未知，绝不伪造"未启用"', async () => {
    getProxySnapshotMock.mockResolvedValue(
      fixture({
        user_settings: fixture().user_settings && {
          ...fixture().user_settings,
          status: 'unknown',
          read_error: 'API 调用失败',
          auto_detect: null,
          pac_url: null,
          manual_proxy: null,
        },
        manual_proxy_enabled: null,
        pac_configured: null,
      }),
    );
    render(<ProxySnapshotCard />);

    expect(await screen.findByText(/未知（API 调用失败）/)).toBeInTheDocument();
    expect(screen.queryByText(/手动代理未配置/)).not.toBeInTheDocument();
  });

  it('IPC 失败时显示错误而非假数据', async () => {
    getProxySnapshotMock.mockRejectedValue({ code: 'internal', message: 'IPC 不可用' });
    render(<ProxySnapshotCard />);
    await waitFor(() => expect(screen.getByText(/IPC 不可用/)).toBeInTheDocument());
  });
});
