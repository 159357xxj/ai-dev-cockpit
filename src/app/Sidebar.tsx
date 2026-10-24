import { NavLink } from 'react-router';

import { Badge } from '@/components/ui/badge';
import { cn } from '@/lib/utils';
import { useAppSettings } from './settings';
import { NAV_ITEMS } from './nav-items';

function linkClass({ isActive }: { isActive: boolean }): string {
  return cn(
    'flex items-center gap-2 rounded-md px-3 py-2 text-sm transition-colors',
    isActive
      ? 'bg-accent font-medium text-accent-foreground'
      : 'text-muted-foreground hover:bg-accent/50 hover:text-foreground',
  );
}

function NavLinks({ className }: { className?: string }) {
  return (
    <nav aria-label="主导航" className={className}>
      {NAV_ITEMS.map((item) => (
        <NavLink key={item.to} to={item.to} end={item.end} className={linkClass}>
          <item.icon className="size-4 shrink-0" aria-hidden />
          <span className="whitespace-nowrap">{item.label}</span>
        </NavLink>
      ))}
    </nav>
  );
}

/** 桌面端左侧导航栏。 */
export function Sidebar() {
  const { settings } = useAppSettings();
  return (
    <aside className="hidden w-60 shrink-0 flex-col border-r border-border bg-card/40 md:flex">
      <div className="px-4 py-4">
        <p className="text-sm font-semibold tracking-tight">AI Dev Cockpit</p>
        <p className="mt-0.5 text-xs text-muted-foreground">开发者网络诊断台</p>
      </div>
      <NavLinks className="flex-1 space-y-1 px-2" />
      <div className="space-y-1 border-t border-border px-4 py-3 text-xs text-muted-foreground">
        <div className="flex items-center justify-between">
          <span>v0.1.0 · Phase 1</span>
          {settings.demoMode ? (
            <span className="flex items-center gap-1 text-amber-500">
              <span className="size-1.5 rounded-full bg-amber-500" aria-hidden />
              演示
            </span>
          ) : null}
        </div>
        <p>只读诊断 · 不修改系统配置</p>
      </div>
    </aside>
  );
}

/** 窄窗口顶部横向导航。 */
export function MobileNav() {
  const { settings } = useAppSettings();
  return (
    <header className="sticky top-0 z-10 border-b border-border bg-background md:hidden">
      <div className="flex items-center justify-between px-3 py-2">
        <p className="text-sm font-semibold tracking-tight">AI Dev Cockpit</p>
        {settings.demoMode ? <Badge variant="warning">演示模式</Badge> : null}
      </div>
      <NavLinks className="flex gap-1 overflow-x-auto px-2 pb-2" />
    </header>
  );
}
