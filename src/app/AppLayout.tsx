import { Outlet } from 'react-router';

import { MobileNav, Sidebar } from './Sidebar';

/** 全局布局：桌面端左侧栏，窄窗口顶部导航。 */
export function AppLayout() {
  return (
    <div className="flex min-h-screen flex-col md:flex-row">
      <MobileNav />
      <Sidebar />
      <main className="min-w-0 flex-1 p-4 md:p-6">
        <Outlet />
      </main>
    </div>
  );
}
