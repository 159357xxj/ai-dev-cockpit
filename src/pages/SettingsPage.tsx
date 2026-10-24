import { ShieldCheck } from 'lucide-react';

import { PageHeader } from '@/components/layout/PageHeader';
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert';
import { Badge } from '@/components/ui/badge';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Label } from '@/components/ui/label';
import { Separator } from '@/components/ui/separator';
import { Switch } from '@/components/ui/switch';
import { useAppSettings, type ThemeSetting } from '@/app/settings';

const THEME_OPTIONS: Array<{ value: ThemeSetting; label: string }> = [
  { value: 'dark', label: '深色（默认）' },
  { value: 'light', label: '浅色' },
  { value: 'system', label: '跟随系统' },
];

const PRIVACY_POINTS = [
  '本地优先：无账号、无云端依赖、无遥测；启动与使用均不后台上传数据。',
  '只读诊断：不修改系统代理、DNS、路由、防火墙、hosts，不请求管理员权限。',
  '不采集浏览器 Cookie、API Key、OAuth token、代理订阅与 SSH 私钥。',
  '手动代理仅接受 loopback 地址且不携带认证信息；失败绝不静默回落。',
  '导出报告先在本地脱敏并预览确认；认证材料与个人路径不会出现在报告中。',
];

export function SettingsPage() {
  const { settings, setTheme, setDemoMode } = useAppSettings();

  return (
    <div className="max-w-3xl">
      <PageHeader title="设置" description="外观、演示模式与隐私说明" />

      <div className="space-y-4">
        <Card>
          <CardHeader>
            <CardTitle className="text-base">外观</CardTitle>
            <CardDescription>深色主题优先；选择跟随系统时随 Windows 设置变化</CardDescription>
          </CardHeader>
          <CardContent>
            <div className="flex flex-wrap gap-4">
              {THEME_OPTIONS.map((option) => (
                <div key={option.value} className="flex items-center gap-2">
                  <input
                    type="radio"
                    id={`theme-${option.value}`}
                    name="theme"
                    className="size-4 accent-neutral-700 dark:accent-neutral-300"
                    checked={settings.theme === option.value}
                    onChange={() => setTheme(option.value)}
                  />
                  <Label htmlFor={`theme-${option.value}`}>{option.label}</Label>
                </div>
              ))}
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle className="text-base">演示模式</CardTitle>
            <CardDescription>用明确标注的模拟数据展示界面形态（AGENTS.md / D-009）</CardDescription>
          </CardHeader>
          <CardContent className="space-y-3">
            <div className="flex items-center justify-between gap-4">
              <div className="space-y-1">
                <Label htmlFor="demo-mode-switch">开启演示模式</Label>
                <p className="text-xs text-muted-foreground">
                  当前状态：
                  {settings.demoMode ? (
                    <span className="text-amber-500">已开启（所有演示页可见模拟数据）</span>
                  ) : (
                    '已关闭（正式默认状态）'
                  )}
                </p>
              </div>
              <Switch
                id="demo-mode-switch"
                checked={settings.demoMode}
                onCheckedChange={setDemoMode}
              />
            </div>
            {settings.demoMode ? (
              <Alert variant="warning">
                <AlertTitle>演示模式说明</AlertTitle>
                <AlertDescription>
                  诊断按钮会运行模拟流程，所有数据均为模拟生成并带「演示数据」标识；这不是真实网络测量，也不会发起任何网络请求。
                </AlertDescription>
              </Alert>
            ) : null}
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle className="text-base">后续阶段设置项</CardTitle>
            <CardDescription>随对应阶段开放</CardDescription>
          </CardHeader>
          <CardContent className="space-y-3 text-sm">
            <div className="flex items-center justify-between gap-4">
              <div>
                <p>历史数据保留策略</p>
                <p className="text-xs text-muted-foreground">默认 30 天或 500 次会话，可配置</p>
              </div>
              <Badge variant="secondary">Phase 5 开放</Badge>
            </div>
            <Separator />
            <div className="flex items-center justify-between gap-4">
              <div>
                <p>默认本地代理端口</p>
                <p className="text-xs text-muted-foreground">手动代理模式的快捷端口记忆</p>
              </div>
              <Badge variant="secondary">Phase 3 开放</Badge>
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <div className="flex items-center gap-2">
              <ShieldCheck className="size-4 text-muted-foreground" aria-hidden />
              <CardTitle className="text-base">隐私与边界</CardTitle>
            </div>
          </CardHeader>
          <CardContent>
            <ul className="list-disc space-y-1.5 pl-5 text-sm text-muted-foreground">
              {PRIVACY_POINTS.map((point) => (
                <li key={point}>{point}</li>
              ))}
            </ul>
          </CardContent>
        </Card>
      </div>
    </div>
  );
}
