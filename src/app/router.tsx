import { Compass } from 'lucide-react';
import { Link, createBrowserRouter, createMemoryRouter, type RouteObject } from 'react-router';

import { AppLayout } from './AppLayout';
import { EmptyState } from '@/components/layout/states';
import { Button } from '@/components/ui/button';
import { ComparePage } from '@/pages/ComparePage';
import { DashboardPage } from '@/pages/DashboardPage';
import { DiagnosticsPage } from '@/pages/DiagnosticsPage';
import { HistoryPage } from '@/pages/HistoryPage';
import { SettingsPage } from '@/pages/SettingsPage';

const NotFound = () => (
  <EmptyState icon={Compass} title="页面不存在" description="请从导航进入有效页面。">
    <Button asChild variant="outline" size="sm">
      <Link to="/">返回总览</Link>
    </Button>
  </EmptyState>
);

export const routes: RouteObject[] = [
  {
    path: '/',
    element: <AppLayout />,
    children: [
      { index: true, element: <DashboardPage /> },
      { path: 'diagnostics', element: <DiagnosticsPage /> },
      { path: 'compare', element: <ComparePage /> },
      { path: 'history', element: <HistoryPage /> },
      { path: 'settings', element: <SettingsPage /> },
      { path: '*', element: <NotFound /> },
    ],
  },
];

export function createAppRouter(
  kind: 'browser' | 'memory',
  initialEntries: string[] = ['/'],
): ReturnType<typeof createBrowserRouter> {
  if (kind === 'browser') {
    return createBrowserRouter(routes);
  }
  return createMemoryRouter(routes, { initialEntries });
}
