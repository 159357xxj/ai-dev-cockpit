import { History as HistoryIcon } from 'lucide-react';

import { DemoBadge, DemoBanner } from '@/components/layout/demo';
import { EmptyState } from '@/components/layout/states';
import { PageHeader } from '@/components/layout/PageHeader';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Label } from '@/components/ui/label';
import { Select } from '@/components/ui/select';
import { useAppSettings } from '@/app/settings';
import { fmtDurationMs, fmtUtcToLocal } from '@/lib/format';
import { ROUTE_MODE_LABELS } from '@/lib/types';
import type { RouteMode } from '@/lib/types';
import { MOCK_HISTORY_RUNS } from '@/features/demo/mockData';

export function HistoryPage() {
  const { settings } = useAppSettings();

  return (
    <div>
      <PageHeader
        title="历史记录"
        description="过往诊断会话的持久化、筛选与导出"
        actions={settings.demoMode ? <DemoBadge /> : null}
      />

      {settings.demoMode ? <DemoBanner /> : null}

      {settings.demoMode ? (
        <>
          <Card>
            <CardHeader>
              <CardTitle className="text-base">历史会话（模拟）</CardTitle>
            </CardHeader>
            <CardContent>
              <div className="overflow-x-auto rounded-md border border-border">
                <table className="w-full text-sm">
                  <thead className="bg-muted/50 text-left text-xs text-muted-foreground">
                    <tr>
                      <th className="px-3 py-2 font-medium">开始时间（本地）</th>
                      <th className="px-3 py-2 font-medium">目标</th>
                      <th className="px-3 py-2 font-medium">模式</th>
                      <th className="px-3 py-2 font-medium">状态</th>
                      <th className="px-3 py-2 font-medium">样本</th>
                      <th className="px-3 py-2 font-medium">中位耗时</th>
                    </tr>
                  </thead>
                  <tbody>
                    {MOCK_HISTORY_RUNS.map((run) => (
                      <tr key={run.run_id} className="border-t border-border">
                        <td className="px-3 py-2">{fmtUtcToLocal(run.started_at)}</td>
                        <td className="px-3 py-2">{run.targets}</td>
                        <td className="px-3 py-2">{ROUTE_MODE_LABELS[run.route_mode as RouteMode]}</td>
                        <td className="px-3 py-2">
                          {run.status === 'cancelled' ? (
                            <span className="text-amber-500">已取消</span>
                          ) : (
                            '已完成'
                          )}
                        </td>
                        <td className="px-3 py-2">{run.samples}</td>
                        <td className="px-3 py-2">{fmtDurationMs(run.median_ms)}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </CardContent>
          </Card>
          <Alert className="mt-4" variant="info">
            <AlertDescription>
              SQLite 持久化、跨重启历史、趋势对比与脱敏导出将在 Phase 5 提供；当前列表仅为演示数据。
            </AlertDescription>
          </Alert>
        </>
      ) : (
        <EmptyState
          icon={HistoryIcon}
          title="历史记录将在 Phase 5 提供"
          description="届时诊断会话将持久化到本地 SQLite（30 天 / 500 次保留策略），支持筛选、对比与脱敏导出。"
        />
      )}

      <div className="mt-4 grid gap-3 sm:grid-cols-2 lg:max-w-md">
        <div className="space-y-1.5">
          <Label htmlFor="history-target-filter">按目标筛选</Label>
          <Select id="history-target-filter" disabled>
            <option>Phase 5 提供</option>
          </Select>
        </div>
        <div className="space-y-1.5">
          <Label htmlFor="history-mode-filter">按模式筛选</Label>
          <Select id="history-mode-filter" disabled>
            <option>Phase 5 提供</option>
          </Select>
        </div>
      </div>
    </div>
  );
}
