import { GitCompareArrows } from 'lucide-react';

import { DemoBadge, DemoBanner } from '@/components/layout/demo';
import { EmptyState } from '@/components/layout/states';
import { PageHeader } from '@/components/layout/PageHeader';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { useAppSettings } from '@/app/settings';
import { fmtDurationMs, fmtPercent } from '@/lib/format';
import { aggregateSamples, non2xxResponseRatio, transportFailureRate } from '@/lib/metrics';
import { ROUTE_MODE_LABELS } from '@/lib/types';
import type { RouteMode } from '@/lib/types';
import { buildDemoCompare } from '@/features/demo/mockData';

const COMPARE_NOTES = [
  '仅比较 HTTPS 样本；DNS/TCP 是独立的本机基线，不参与代理路径对比。',
  'no_app_proxy 仅指不使用应用层显式代理；系统 TUN/透明路由仍可能生效。',
  'manual_local_proxy 指定代理不可用时明确失败，不会产生直连回落结果。',
  'library_default 的代理源语义将在 Phase 3 用本地 mock 实测后固化说明。',
];

export function ComparePage() {
  const { settings } = useAppSettings();

  return (
    <div>
      <PageHeader
        title="路径对比"
        description="对同一 HTTPS 目标用不同连接模式重复测量并对比"
        actions={settings.demoMode ? <DemoBadge /> : null}
      />

      {settings.demoMode ? <DemoBanner /> : null}

      {settings.demoMode ? (
        <Card>
          <CardHeader>
            <CardTitle className="text-base">github.com — 三种连接模式对比（模拟）</CardTitle>
          </CardHeader>
          <CardContent>
            <div className="overflow-x-auto rounded-md border border-border">
              <table className="w-full text-sm">
                <thead className="bg-muted/50 text-left text-xs text-muted-foreground">
                  <tr>
                    <th className="px-3 py-2 font-medium">连接模式</th>
                    <th className="px-3 py-2 font-medium">样本</th>
                    <th className="px-3 py-2 font-medium">成功</th>
                    <th className="px-3 py-2 font-medium">传输失败</th>
                    <th className="px-3 py-2 font-medium">收到响应</th>
                    <th className="px-3 py-2 font-medium">非 2xx 比例</th>
                    <th className="px-3 py-2 font-medium">中位耗时</th>
                  </tr>
                </thead>
                <tbody>
                  {buildDemoCompare().map(({ route_mode: mode, samples }) => {
                    const aggregate = aggregateSamples('https', samples);
                    return (
                      <tr key={mode} className="border-t border-border">
                        <td className="px-3 py-2">{ROUTE_MODE_LABELS[mode as RouteMode]}</td>
                        <td className="px-3 py-2">{aggregate.attempts_total}</td>
                        <td className="px-3 py-2">{aggregate.success_count}</td>
                        <td className="px-3 py-2">
                          {aggregate.transport_failure_count}
                          {`（${fmtPercent(transportFailureRate(aggregate))}）`}
                        </td>
                        <td className="px-3 py-2">{aggregate.http_response_count}</td>
                        <td className="px-3 py-2">{fmtPercent(non2xxResponseRatio(aggregate))}</td>
                        <td className="px-3 py-2">{fmtDurationMs(aggregate.duration_median_ms)}</td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            </div>
            <ul className="mt-3 list-disc space-y-1 pl-5 text-xs text-muted-foreground">
              {COMPARE_NOTES.map((note) => (
                <li key={note}>{note}</li>
              ))}
            </ul>
          </CardContent>
        </Card>
      ) : (
        <EmptyState
          icon={GitCompareArrows}
          title="路径对比将在 Phase 3 提供"
          description="该页面将复用 Phase 2 的真实 HTTPS 探测，在三种连接模式下对比同一目标；届时也会实测固化 library_default 的代理源语义。"
        />
      )}
    </div>
  );
}
