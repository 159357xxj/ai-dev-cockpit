import { Link } from 'react-router';
import { Activity, Info, Radar } from 'lucide-react';
import {
  CartesianGrid,
  Line,
  LineChart,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from 'recharts';

import { DemoBadge, DemoBanner } from '@/components/layout/demo';
import { ProxySnapshotCard } from '@/components/dashboard/ProxySnapshotCard';
import { EmptyState } from '@/components/layout/states';
import { PageHeader } from '@/components/layout/PageHeader';
import { Button } from '@/components/ui/button';
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from '@/components/ui/card';
import { useAppSettings } from '@/app/settings';
import { fmtDurationMs, fmtUtcToLocal } from '@/lib/format';
import { MOCK_RECENT_RUN, MOCK_SYSTEM_SNAPSHOT, MOCK_TREND } from '@/features/demo/mockData';

const LIMITATIONS = [
  '本工具是只读诊断工具：不修改代理、DNS、路由、防火墙或注册表，不请求管理员权限。',
  '「无应用代理」只是不配置应用层 HTTP 代理；系统 TUN/透明路由仍可能生效，工具不会声称绕过。',
  'HTTP 401/403 表示「收到响应，但业务能力未验证」，不代表服务可正常登录或使用。',
  '手动本地代理仅在 127.0.0.1/localhost/[::1] 范围内有效，失败时绝不静默回落为直连。',
];

export function DashboardPage() {
  const { settings } = useAppSettings();

  return (
    <div>
      <PageHeader
        title="网络总览"
        description="本机网络状态摘要与快捷入口（只读，不主动联网）"
      />

      {settings.demoMode ? <DemoBanner /> : null}

      {settings.demoMode ? (
        <div className="grid gap-4 lg:grid-cols-3">
          <Card>
            <CardHeader>
              <div className="flex items-center justify-between">
                <CardTitle className="text-base">系统状态摘要</CardTitle>
                <DemoBadge />
              </div>
              <CardDescription>来自模拟快照，非真实系统信息</CardDescription>
            </CardHeader>
            <CardContent className="space-y-1.5 text-sm">
              <p>
                <span className="text-muted-foreground">系统：</span>
                {MOCK_SYSTEM_SNAPSHOT.os_version}（{MOCK_SYSTEM_SNAPSHOT.arch}）
              </p>
              <p>
                <span className="text-muted-foreground">适配器：</span>
                {MOCK_SYSTEM_SNAPSHOT.adapters[0].name} · 已连接
              </p>
              <p>
                <span className="text-muted-foreground">DNS：</span>
                {MOCK_SYSTEM_SNAPSHOT.dns_servers.join('、')}
              </p>
              <p className="text-xs text-muted-foreground">
                快照时间：{fmtUtcToLocal(MOCK_SYSTEM_SNAPSHOT.captured_at)}（本地时间）
              </p>
            </CardContent>
          </Card>

          <Card>
            <CardHeader>
              <div className="flex items-center justify-between">
                <CardTitle className="text-base">最近诊断</CardTitle>
                <DemoBadge />
              </div>
              <CardDescription>模拟运行：{MOCK_RECENT_RUN.run_id}</CardDescription>
            </CardHeader>
            <CardContent className="space-y-2 text-sm">
              <p>
                <span className="text-muted-foreground">状态：</span>已完成 ·{' '}
                {MOCK_RECENT_RUN.target_count} 个目标 · 中位耗时{' '}
                {fmtDurationMs(MOCK_RECENT_RUN.median_ms)}
              </p>
              <div className="h-28" aria-hidden>
                <ResponsiveContainer width="100%" height="100%">
                  <LineChart data={MOCK_TREND} margin={{ top: 4, right: 8, bottom: 0, left: -24 }}>
                    <CartesianGrid strokeDasharray="3 3" stroke="var(--color-border)" />
                    <XAxis dataKey="date" tick={{ fontSize: 10 }} stroke="var(--color-muted-foreground)" />
                    <YAxis tick={{ fontSize: 10 }} stroke="var(--color-muted-foreground)" />
                    <Tooltip />
                    <Line
                      type="monotone"
                      dataKey="median_ms"
                      stroke="var(--color-primary)"
                      strokeWidth={2}
                      dot
                    />
                  </LineChart>
                </ResponsiveContainer>
              </div>
              <p className="text-xs text-muted-foreground">近 5 次（模拟）HTTPS 中位耗时</p>
            </CardContent>
          </Card>

          <Card>
            <CardHeader>
              <CardTitle className="text-base">快捷入口</CardTitle>
              <CardDescription>开始一次诊断或查看已有数据</CardDescription>
            </CardHeader>
            <CardContent className="flex flex-col gap-2">
              <Button asChild>
                <Link to="/diagnostics">
                  <Activity aria-hidden /> 发起服务诊断
                </Link>
              </Button>
              <Button asChild variant="outline">
                <Link to="/compare">路径对比</Link>
              </Button>
              <Button asChild variant="outline">
                <Link to="/history">历史记录</Link>
              </Button>
            </CardContent>
          </Card>
        </div>
      ) : (
        <EmptyState
          icon={Radar}
          title="系统快照将在后续阶段提供"
          description="本页面当前不读取系统信息、不发起任何网络请求。真实只读快照由后续阶段接入；网络探测已在「服务诊断」页可用，可在「设置」中开启演示模式查看界面形态。"
        />
      )}

      <div className="mt-4">
        <ProxySnapshotCard />
      </div>

      <Card className="mt-4">
        <CardHeader>
          <div className="flex items-center gap-2">
            <Info className="size-4 text-muted-foreground" aria-hidden />
            <CardTitle className="text-base">重要限制说明</CardTitle>
          </div>
        </CardHeader>
        <CardContent>
          <ul className="list-disc space-y-1.5 pl-5 text-sm text-muted-foreground">
            {LIMITATIONS.map((line) => (
              <li key={line}>{line}</li>
            ))}
          </ul>
        </CardContent>
      </Card>
    </div>
  );
}
