import { cleanup, render, screen } from '@testing-library/react';
import { RouterProvider } from 'react-router';

import { createAppRouter } from '@/app/router';
import { SettingsProvider } from '@/app/settings';

/** 统一的测试装配：内存路由 + 设置 Provider。 */
export function renderApp(initialEntries: string[] = ['/']): void {
  render(
    <SettingsProvider>
      <RouterProvider router={createAppRouter('memory', initialEntries)} />
    </SettingsProvider>,
  );
}

/** 预置应用设置（localStorage 键与 settings.tsx 保持一致）。 */
export function seedSettings(demoMode: boolean): void {
  window.localStorage.setItem(
    'adc.settings.v1',
    JSON.stringify({ theme: 'dark', demoMode }),
  );
}

export { cleanup, screen };
