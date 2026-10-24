import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it } from 'vitest';

import { cleanup, renderApp, screen, seedSettings } from './helpers';

afterEach(() => {
  cleanup();
  window.localStorage.clear();
});

describe('诊断表单校验与错误提示', () => {
  it('未选择目标时提示至少选择一个诊断目标', async () => {
    seedSettings(true); // 演示模式，避免触发 IPC
    renderApp(['/diagnostics']);
    const user = userEvent.setup();

    await user.click(screen.getByRole('button', { name: /开始诊断/ }));
    expect(screen.getByText('至少选择一个诊断目标')).toBeInTheDocument();
  });

  it('手动代理模式下非法地址显示具体错误', async () => {
    seedSettings(true);
    renderApp(['/diagnostics']);
    const user = userEvent.setup();

    await user.click(screen.getByRole('button', { name: /GitHub/ }));
    await user.selectOptions(screen.getByLabelText('连接模式'), 'manual_local_proxy');

    const proxyInput = screen.getByLabelText('本地代理地址');
    await user.type(proxyInput, 'http://proxy.example.com:8080');
    await user.click(screen.getByRole('button', { name: /开始诊断/ }));

    expect(screen.getByText(/代理主机仅允许 127\.0\.0\.1/)).toBeInTheDocument();
  });

  it('手动代理禁止用户名密码', async () => {
    seedSettings(true);
    renderApp(['/diagnostics']);
    const user = userEvent.setup();

    await user.click(screen.getByRole('button', { name: /GitHub/ }));
    await user.selectOptions(screen.getByLabelText('连接模式'), 'manual_local_proxy');
    await user.type(screen.getByLabelText('本地代理地址'), 'http://user:pass@127.0.0.1:7890');
    await user.click(screen.getByRole('button', { name: /开始诊断/ }));

    expect(screen.getByText('代理地址不允许携带用户名或密码')).toBeInTheDocument();
  });

  it('超时范围外的输入被拒绝', async () => {
    seedSettings(true);
    renderApp(['/diagnostics']);
    const user = userEvent.setup();

    await user.click(screen.getByRole('button', { name: /GitHub/ }));
    const timeoutInput = screen.getByLabelText(/单请求超时/);
    await user.clear(timeoutInput);
    await user.type(timeoutInput, '20000');
    await user.click(screen.getByRole('button', { name: /开始诊断/ }));

    expect(screen.getByText(/单请求超时须在 1000\.\.=10000/)).toBeInTheDocument();
  });

  it('非演示模式下开始诊断给出诚实的未实现提示（不伪造测量）', async () => {
    seedSettings(false);
    renderApp(['/diagnostics']);
    const user = userEvent.setup();

    await user.click(screen.getByRole('button', { name: /GitHub/ }));
    await user.click(screen.getByRole('button', { name: /开始诊断/ }));

    // jsdom 无 Tauri 运行时，api 层返回 not_available；页面必须如实展示。
    expect(await screen.findByText(/未在 Tauri 桌面环境中运行/)).toBeInTheDocument();
    expect(screen.queryByText('已完成')).not.toBeInTheDocument();
  });
});
