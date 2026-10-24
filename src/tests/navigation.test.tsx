import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';

import { cleanup, renderApp, screen, seedSettings } from './helpers';

afterEach(() => {
  cleanup();
  window.localStorage.clear();
});

describe('五个页面导航', () => {
  beforeEach(() => {
    // 演示模式开启，页面渲染确定性演示数据，不触发 IPC。
    seedSettings(true);
  });

  it('默认渲染总览页', () => {
    renderApp();
    expect(screen.getByRole('heading', { name: '网络总览' })).toBeInTheDocument();
  });

  it('可通过导航在五个页面间切换', async () => {
    const user = userEvent.setup();
    renderApp();

    const pages: Array<[RegExp, RegExp]> = [
      [/^服务诊断$/, /^服务诊断$/],
      [/^路径对比$/, /^路径对比$/],
      [/^历史记录$/, /^历史记录$/],
      [/^设置$/, /^设置$/],
    ];
    for (const [navLabel, heading] of pages) {
      // 桌面侧栏与移动导航渲染同一组链接（jsdom 不应用媒体查询），取第一个即可。
      const links = screen.getAllByRole('link', { name: navLabel });
      await user.click(links[0]);
      expect(await screen.findByRole('heading', { name: heading })).toBeInTheDocument();
    }
    // 返回总览
    await user.click(screen.getAllByRole('link', { name: /^总览$/ })[0]);
    expect(screen.getByRole('heading', { name: '网络总览' })).toBeInTheDocument();
  });

  it('未知路径显示页面不存在空态', () => {
    renderApp(['/no-such-page']);
    expect(screen.getByText('页面不存在')).toBeInTheDocument();
  });
});
