import type { LucideIcon } from 'lucide-react';
import {
  Activity,
  GitCompareArrows,
  History,
  LayoutDashboard,
  Settings,
} from 'lucide-react';

export interface NavItem {
  to: string;
  label: string;
  icon: LucideIcon;
  /** NavLink 的 end 属性（根路径需要）。 */
  end?: boolean;
}

export const NAV_ITEMS: NavItem[] = [
  { to: '/', label: '总览', icon: LayoutDashboard, end: true },
  { to: '/diagnostics', label: '服务诊断', icon: Activity },
  { to: '/compare', label: '路径对比', icon: GitCompareArrows },
  { to: '/history', label: '历史记录', icon: History },
  { to: '/settings', label: '设置', icon: Settings },
];
