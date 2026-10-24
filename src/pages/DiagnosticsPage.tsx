import { useEffect, useRef, useState } from 'react';
import { Inbox, Play, Square } from 'lucide-react';

import { DemoBadge, DemoBanner } from '@/components/layout/demo';
import { EmptyState, ErrorState } from '@/components/layout/states';
import { PageHeader } from '@/components/layout/PageHeader';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { Button } from '@/components/ui/button';
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from '@/components/ui/card';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Select } from '@/components/ui/select';
import { useAppSettings } from '@/app/settings';
import { api, normalizeIpcError } from '@/lib/api';
import { fmtDurationMs, fmtPercent } from '@/lib/format';
import { aggregateSamples, non2xxResponseRatio, transportFailureRate } from '@/lib/metrics';
import {
  ERROR_KIND_LABELS,
  MAX_TARGETS,
  PRESET_TARGETS,
  REPEATS_ALLOWED,
  ROUTE_MODE_LABELS,
  TIMEOUT_DEFAULT_MS,
  TIMEOUT_MAX_MS,
  TIMEOUT_MIN_MS,
} from '@/lib/types';
import type { DiagnosticRequest, IpcError, ProbeSample, RouteMode, RunStatus } from '@/lib/types';
import { cn } from '@/lib/utils';
import { buildDemoTargets } from '@/features/demo/mockData';

/** 镜像 Rust security::validation 的手动代理校验规则（展示层预检，后端仍二次校验）。 */
export function validateProxyUrl(raw: string): string | null {
  let parsed: URL;
  try {
    parsed = new URL(raw.trim());
  } catch {
    return '代理地址无法解析';
  }
  const scheme = parsed.protocol.replace(/:$/, '');
  if (scheme !== 'http' && scheme !== 'socks5' && scheme !== 'socks5h') {
    return '代理协议仅支持 http / socks5 / socks5h';
  }
  if (parsed.username !== '' || parsed.password !== '') {
    return '代理地址不允许携带用户名或密码';
  }
  const host = parsed.hostname.toLowerCase();
  if (host !== '127.0.0.1' && host !== 'localhost' && host !== '[::1]') {
    return '代理主机仅允许 127.0.0.1 / localhost / [::1]';
  }
  if (parsed.port === '') {
    return '代理地址必须显式指定端口';
  }
  const port = Number(parsed.port);
  if (!Number.isInteger(port) || port < 1 || port > 65535) {
    return '代理端口须为 1..=65535';
  }
  if (parsed.pathname !== '/' && parsed.pathname !== '') {
    return '代理地址不允许携带路径';
  }
  if (parsed.search !== '' || parsed.hash !== '') {
    return '代理地址不允许携带查询参数或锚点';
  }
  return null;
}

interface FormState {
  targetIds: string[];
  routeMode: RouteMode;
  proxyUrl: string;
  repeats: number;
  timeoutMs: string;
}

type FormErrors = Partial<Record<'targets' | 'proxy' | 'timeout', string>>;

function validateForm(form: FormState): FormErrors {
  const errors: FormErrors = {};
  if (form.targetIds.length === 0) {
    errors.targets = '至少选择一个诊断目标';
  } else if (form.targetIds.length > MAX_TARGETS) {
    errors.targets = `诊断目标最多 ${MAX_TARGETS} 个`;
  }
  if (form.routeMode === 'manual_local_proxy') {
    const trimmed = form.proxyUrl.trim();
    if (trimmed === '') {
      errors.proxy = '手动代理模式必须提供本地代理地址';
    } else {
      const problem = validateProxyUrl(trimmed);
      if (problem) errors.proxy = problem;
    }
  }
  const timeout = Number(form.timeoutMs);
  if (!Number.isInteger(timeout) || timeout < TIMEOUT_MIN_MS || timeout > TIMEOUT_MAX_MS) {
    errors.timeout = `单请求超时须在 ${TIMEOUT_MIN_MS}..=${TIMEOUT_MAX_MS} 毫秒`;
  }
  return errors;
}

function sampleResultLabel(sample: ProbeSample): string {
  if (sample.error) return ERROR_KIND_LABELS[sample.error.kind];
  if (sample.http_status !== null) {
    return sample.success ? '收到响应（2xx）' : `收到响应（${sample.http_status}）`;
  }
  return sample.success ? '成功' : '失败';
}

type RunState = 'idle' | RunStatus;

export function DiagnosticsPage() {
  const { settings } = useAppSettings();
  const [form, setForm] = useState<FormState>({
    targetIds: [],
    routeMode: 'no_app_proxy',
    proxyUrl: '',
    repeats: 3,
    timeoutMs: String(TIMEOUT_DEFAULT_MS),
  });
  const [errors, setErrors] = useState<FormErrors>({});
  const [runState, setRunState] = useState<RunState>('idle');
  const [revealedSamples, setRevealedSamples] = useState<ProbeSample[]>([]);
  const [plannedTargets, setPlannedTargets] = useState<string[]>([]);
  const [ipcError, setIpcError] = useState<IpcError | null>(null);
  const timerRef = useRef<number | null>(null);

  useEffect(
    () => () => {
      if (timerRef.current !== null) window.clearTimeout(timerRef.current);
    },
    [],
  );

  const stopTimer = (): void => {
    if (timerRef.current !== null) {
      window.clearTimeout(timerRef.current);
      timerRef.current = null;
    }
  };

  const toggleTarget = (targetId: string): void => {
    setForm((previous) => ({
      ...previous,
      targetIds: previous.targetIds.includes(targetId)
        ? previous.targetIds.filter((id) => id !== targetId)
        : [...previous.targetIds, targetId],
    }));
  };

  const startDemoRun = (): void => {
    const plan = buildDemoTargets(form.targetIds, form.repeats, form.routeMode);
    const flat = plan.flatMap((target) => target.samples);
    setPlannedTargets(plan.map((target) => target.target_id));
    setRevealedSamples([]);
    setIpcError(null);
    setRunState('running');
    let index = 0;
    const tick = (): void => {
      index += 1;
      setRevealedSamples(flat.slice(0, index));
      if (index >= flat.length) {
        setRunState('completed');
        timerRef.current = null;
      } else {
        timerRef.current = window.setTimeout(tick, 450);
      }
    };
    timerRef.current = window.setTimeout(tick, 450);
  };

  const startRealRun = async (): Promise<void> => {
    setIpcError(null);
    setRunState('running');
    const request: DiagnosticRequest = {
      target_ids: form.targetIds,
      route_mode: form.routeMode,
      manual_local_proxy_url:
        form.routeMode === 'manual_local_proxy' ? form.proxyUrl.trim() : null,
      repeats: form.repeats,
      timeout_ms: Number(form.timeoutMs),
    };
    try {
      const handle = await api.startDiagnostic(request);
      // Phase 2：订阅 diagnostic://progress 事件推进状态；当前不会到达此处。
      setRunState('completed');
      console.info('诊断任务已创建', handle.job_id);
    } catch (raw) {
      const error = normalizeIpcError(raw);
      setIpcError(error);
      setRunState('failed');
    }
  };

  const onStart = (): void => {
    const found = validateForm(form);
    setErrors(found);
    if (Object.keys(found).length > 0) return;
    if (settings.demoMode) {
      startDemoRun();
    } else {
      void startRealRun();
    }
  };

  const onCancel = (): void => {
    stopTimer();
    setRunState('cancelled');
  };

  const running = runState === 'running';

  return (
    <div>
      <PageHeader
        title="服务诊断"
        description="选择目标与连接模式，对预设服务执行独立探测"
        actions={settings.demoMode ? <DemoBadge /> : null}
      />

      {settings.demoMode ? <DemoBanner /> : null}

      <div className="grid gap-4 lg:grid-cols-[380px_minmax(0,1fr)]">
        <Card>
          <CardHeader>
            <CardTitle className="text-base">诊断配置</CardTitle>
            <CardDescription>仅访问预设 HTTPS 目标，不发送任何凭证</CardDescription>
          </CardHeader>
          <CardContent className="space-y-4">
            <div className="space-y-1.5">
              <Label>诊断目标（可多选）</Label>
              <div className="flex flex-wrap gap-2">
                {PRESET_TARGETS.map((target) => {
                  const active = form.targetIds.includes(target.id);
                  return (
                    <button
                      key={target.id}
                      type="button"
                      aria-pressed={active}
                      onClick={() => toggleTarget(target.id)}
                      disabled={running}
                      className={cn(
                        'rounded-md border px-3 py-1.5 text-sm transition-colors',
                        active
                          ? 'border-primary bg-primary/10 font-medium text-foreground'
                          : 'border-border text-muted-foreground hover:bg-accent hover:text-foreground',
                      )}
                    >
                      {target.label}
                    </button>
                  );
                })}
              </div>
              {errors.targets ? (
                <p className="text-xs text-destructive">{errors.targets}</p>
              ) : null}
            </div>

            <div className="space-y-1.5">
              <Label htmlFor="route-mode">连接模式</Label>
              <Select
                id="route-mode"
                value={form.routeMode}
                disabled={running}
                onChange={(event) =>
                  setForm((previous) => ({
                    ...previous,
                    routeMode: event.target.value as RouteMode,
                  }))
                }
              >
                {(Object.keys(ROUTE_MODE_LABELS) as RouteMode[]).map((mode) => (
                  <option key={mode} value={mode}>
                    {ROUTE_MODE_LABELS[mode]}
                  </option>
                ))}
              </Select>
            </div>

            {form.routeMode === 'manual_local_proxy' ? (
              <div className="space-y-1.5">
                <Label htmlFor="proxy-url">本地代理地址</Label>
                <Input
                  id="proxy-url"
                  placeholder="http://127.0.0.1:7890"
                  value={form.proxyUrl}
                  disabled={running}
                  onChange={(event) =>
                    setForm((previous) => ({ ...previous, proxyUrl: event.target.value }))
                  }
                />
                {errors.proxy ? (
                  <p className="text-xs text-destructive">{errors.proxy}</p>
                ) : (
                  <p className="text-xs text-muted-foreground">
                    仅允许 loopback（127.0.0.1 / localhost / [::1]）的 http / socks5 / socks5h，不带用户名密码。
                  </p>
                )}
              </div>
            ) : null}

            <div className="grid grid-cols-2 gap-3">
              <div className="space-y-1.5">
                <Label htmlFor="repeats">采样次数</Label>
                <Select
                  id="repeats"
                  value={String(form.repeats)}
                  disabled={running}
                  onChange={(event) =>
                    setForm((previous) => ({ ...previous, repeats: Number(event.target.value) }))
                  }
                >
                  {REPEATS_ALLOWED.map((count) => (
                    <option key={count} value={count}>
                      {count} 次
                    </option>
                  ))}
                </Select>
              </div>
              <div className="space-y-1.5">
                <Label htmlFor="timeout-ms">单请求超时（毫秒）</Label>
                <Input
                  id="timeout-ms"
                  type="number"
                  min={TIMEOUT_MIN_MS}
                  max={TIMEOUT_MAX_MS}
                  value={form.timeoutMs}
                  disabled={running}
                  onChange={(event) =>
                    setForm((previous) => ({ ...previous, timeoutMs: event.target.value }))
                  }
                />
              </div>
            </div>
            {errors.timeout ? <p className="text-xs text-destructive">{errors.timeout}</p> : null}

            <div className="flex gap-2 pt-1">
              <Button onClick={onStart} disabled={running} className="flex-1">
                <Play aria-hidden /> 开始诊断
              </Button>
              <Button onClick={onCancel} disabled={!running} variant="outline">
                <Square aria-hidden /> 取消
              </Button>
            </div>

            {form.routeMode === 'no_app_proxy' ? (
              <p className="text-xs text-muted-foreground">
                说明：无应用代理仅指不配置应用层 HTTP 代理；系统 TUN/透明路由仍可能生效。
              </p>
            ) : null}
            {form.routeMode === 'manual_local_proxy' ? (
              <p className="text-xs text-muted-foreground">
                说明：socks5 由本机解析域名，socks5h 由代理解析域名；代理不可用时明确失败，绝不回落。
              </p>
            ) : null}
          </CardContent>
        </Card>

        <div className="space-y-4">
          {ipcError ? <ErrorState error={ipcError} title="诊断未启动" /> : null}

          {runState === 'idle' && !ipcError ? (
            <EmptyState
              icon={Inbox}
              title="尚未开始诊断"
              description={
                settings.demoMode
                  ? '演示模式：点击「开始诊断」查看模拟流程与数据形态。'
                  : '真实 DNS/TCP/HTTPS 探测将在 Phase 2 提供；本阶段仅界面骨架。'
              }
            />
          ) : null}

          {revealedSamples.length > 0 ? (
            <Card>
              <CardHeader>
                <div className="flex items-center justify-between">
                  <CardTitle className="text-base">
                    诊断结果
                    {settings.demoMode ? '（模拟）' : ''}
                  </CardTitle>
                  <div className="flex items-center gap-2">
                    {runState === 'running' ? (
                      <span className="text-xs text-muted-foreground">进行中…</span>
                    ) : null}
                    {runState === 'completed' ? (
                      <span className="text-xs text-muted-foreground">已完成</span>
                    ) : null}
                    {runState === 'cancelled' ? (
                      <span className="text-xs text-amber-500">已取消</span>
                    ) : null}
                    {settings.demoMode ? <DemoBadge /> : null}
                  </div>
                </div>
              </CardHeader>
              <CardContent className="space-y-4">
                {plannedTargets.map((targetId) => {
                  const samples = revealedSamples.filter((sample) => sample.target_id === targetId);
                  if (samples.length === 0) return null;
                  const aggregate = aggregateSamples('https', samples);
                  return (
                    <div key={targetId} className="space-y-2">
                      <div className="flex flex-wrap items-center gap-2 text-sm font-medium">
                        <span>{targetId}</span>
                        <span className="rounded bg-secondary px-1.5 py-0.5 text-xs text-secondary-foreground">
                          样本 {aggregate.attempts_total}
                        </span>
                        <span className="rounded bg-secondary px-1.5 py-0.5 text-xs text-secondary-foreground">
                          成功 {aggregate.success_count}
                        </span>
                        <span className="rounded bg-secondary px-1.5 py-0.5 text-xs text-secondary-foreground">
                          传输失败 {aggregate.transport_failure_count}
                          {`（${fmtPercent(transportFailureRate(aggregate))}）`}
                        </span>
                        <span className="rounded bg-secondary px-1.5 py-0.5 text-xs text-secondary-foreground">
                          非2xx {fmtPercent(non2xxResponseRatio(aggregate))}
                        </span>
                        <span className="rounded bg-secondary px-1.5 py-0.5 text-xs text-secondary-foreground">
                          min {fmtDurationMs(aggregate.duration_min_ms)} / med{' '}
                          {fmtDurationMs(aggregate.duration_median_ms)} / max{' '}
                          {fmtDurationMs(aggregate.duration_max_ms)}
                        </span>
                      </div>
                      <div className="overflow-x-auto rounded-md border border-border">
                        <table className="w-full text-sm">
                          <thead className="bg-muted/50 text-left text-xs text-muted-foreground">
                            <tr>
                              <th className="px-3 py-2 font-medium">#</th>
                              <th className="px-3 py-2 font-medium">探测</th>
                              <th className="px-3 py-2 font-medium">结果</th>
                              <th className="px-3 py-2 font-medium">HTTP 状态</th>
                              <th className="px-3 py-2 font-medium">耗时</th>
                            </tr>
                          </thead>
                          <tbody>
                            {samples.map((sample) => (
                              <tr key={`${sample.target_id}-${sample.attempt_no}`} className="border-t border-border">
                                <td className="px-3 py-1.5">{sample.attempt_no}</td>
                                <td className="px-3 py-1.5">HTTPS</td>
                                <td className="px-3 py-1.5">{sampleResultLabel(sample)}</td>
                                <td className="px-3 py-1.5">{sample.http_status ?? '—'}</td>
                                <td className="px-3 py-1.5">{fmtDurationMs(sample.duration_ms)}</td>
                              </tr>
                            ))}
                          </tbody>
                        </table>
                      </div>
                    </div>
                  );
                })}
                {settings.demoMode ? (
                  <Alert variant="warning">
                    <AlertDescription>
                      以上样本与统计全部为模拟数据（D-009），不代表真实网络测量。
                    </AlertDescription>
                  </Alert>
                ) : null}
              </CardContent>
            </Card>
          ) : null}
        </div>
      </div>
    </div>
  );
}
