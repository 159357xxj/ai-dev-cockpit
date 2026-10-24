import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it } from 'vitest';

import { cleanup, renderApp, screen, seedSettings } from './helpers';

afterEach(() => {
  cleanup();
  window.localStorage.clear();
});

describe('演示模式标识与开关', () => {
  it('关闭（默认）时无演示标识且显示待开放空态', () => {
    window.localStorage.clear(); // 无设置 → 默认 demoMode=false（正式构建默认态）
    renderApp();

    expect(screen.queryAllByText('演示数据')).toHaveLength(0);
    expect(screen.queryByText(/演示模式已开启/)).not.toBeInTheDocument();
    expect(screen.getByText(/系统快照将在后续阶段提供/)).toBeInTheDocument();
  });

  it('开启后显示演示横幅、徽标与模拟内容', () => {
    seedSettings(true);
    renderApp();

    expect(screen.getByText(/演示模式已开启/)).toBeInTheDocument();
    expect(screen.getAllByText('演示数据').length).toBeGreaterThan(0);
    expect(screen.getByText(/模拟快照，非真实系统信息/)).toBeInTheDocument();
  });

  it('设置页可切换演示模式并持久化到 localStorage', async () => {
    seedSettings(false);
    renderApp(['/settings']);
    const user = userEvent.setup();

    const toggle = screen.getByRole('switch', { name: /开启演示模式/ });
    expect(toggle).not.toBeChecked();

    await user.click(toggle);
    expect(toggle).toBeChecked();
    const stored: unknown = JSON.parse(
      window.localStorage.getItem('adc.settings.v1') ?? '{}',
    );
    expect((stored as { demoMode?: boolean }).demoMode).toBe(true);

    // 开启后设置页出现演示模式警示说明
    expect(screen.getByText('演示模式说明')).toBeInTheDocument();

    // 再次点击关闭
    await user.click(toggle);
    expect(toggle).not.toBeChecked();
  });

  it('诊断页演示开始产生带标识的模拟结果，取消后不再推进', async () => {
    seedSettings(true);
    renderApp(['/diagnostics']);
    const user = userEvent.setup();

    await user.click(screen.getByRole('button', { name: /GitHub/ }));
    await user.click(screen.getByRole('button', { name: /开始诊断/ }));
    expect(await screen.findByText(/诊断结果（模拟）/)).toBeInTheDocument();
    expect(screen.getAllByText('演示数据').length).toBeGreaterThan(0);

    // 未选超时默认合法；模拟推进由定时器驱动，取消后状态固定为已取消
    await user.click(screen.getByRole('button', { name: /取消/ }));
    expect(await screen.findByText(/已取消/)).toBeInTheDocument();
  });
});
