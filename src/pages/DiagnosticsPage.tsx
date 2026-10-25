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
import { normalizeIpcError, runDiagnostic, api, type DiagnosticRunSession } from '@/lib/api';
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
import type {
  DiagnosticRequest,
  IpcError,
  ProbeSample,
  ProbeType,
  RouteMode,
} from '@/lib/types';
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

const PROBE_TYPE_LABELS: Record<ProbeType, string> = {
  dns: 'DNS',
  tcp: 'TCP',
  https: 'HTTPS',
};

function sampleResultLabel(sample: ProbeSample): string {
  if (sample.error) return ERROR_KIND_LABELS[sample.error.kind];
  if (sample.http_status !== null) {
    return sample.success ? '收到响应（2xx）' : `收到响应（${sample.http_status}）`;
  }
  return sample.success ? '成功' : '失败';
}

type RunState = 'idle' | 'running' | 'completed' | 'cancelled' | 'failed';

/** 按 (target_id, probe_type, attempt_no) 去重合并样本（事件乱序/重复安全）。 */
function upsertSample(samples: ProbeSample[], incoming: ProbeSample): ProbeSample[] {
  const key = (sample: ProbeSample): string =>
    `${sample.target_id}|${sample.probe_type}|${sample.attempt_no}`;
  const filtered = samples.filter((sample) => key(sample) !== key(incoming));
  return [...filtered, incoming].sort(
    (a, b) =>
      a.target_id.localeCompare(b.target_id) ||
      a.probe_type.localeCompare(b.probe_type) ||
      a.attempt_no - b.attempt_no,
  );
}

export function DiagnosticsPage() {
  const { settings } = useAppSettings();
  const demoMode = settings.demoMode;
  const [form, setForm] = useState<FormState>({
    targetIds: [],
    routeMode: 'no_app_proxy',
    proxyUrl: '',
    repeats: 3,
    timeoutMs: String(TIMEOUT_DEFAULT_MS),
  });
  const [errors, setErrors] = useState<FormErrors>({});
  const [runState, setRunState] = useState<RunState>('idle');
  const [percent, setPercent] = useState(0);
  const [resultSamples, setResultSamples] = useState<ProbeSample[]>([]);
  const [ipcError, setIpcError] = useState<IpcError | null>(null);
  const demoTimerRef = useRef<number | null>(null);
  const sessionRef = useRef<DiagnosticRunSession | null>(null);

  useEffect(
    () => () => {
      if (demoTimerRef.current !== null) window.clearTimeout(demoTimerRef.current);
      sessionRef.current?.unsubscribe();
    },
    [],
  );

  const stopDemoTimer = (): void => {
    if (demoTimerRef.current !== null) {
      window.clearTimeout(demoTimerRef.current);
      demoTimerRef.current = null;
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

  // —— 演示模式：纯前端模拟，与真实路径完全隔离 ——

  const startDemoRun = (): void => {
    const plan = buildDemoTargets(form.targetIds, form.repeats, form.routeMode);
    const flat = plan.flatMap((target) => target.samples);
    setResultSamples([]);
    setIpcError(null);
    setPercent(0);
    setRunState('running');
    let index = 0;
    const tick = (): void => {
      index += 1;
      setResultSamples(flat.slice(0, index));
      setPercent(Math.round((index / flat.length) * 100));
      if (index >= flat.length) {
        setRunState('completed');
        demoTimerRef.current = null;
      } else {
        demoTimerRef.current = window.setTimeout(tick, 450);
      }
    };
    demoTimerRef.current = window.setTimeout(tick, 450);
  };

  // —— 真实模式：事件订阅先于启动（api.ts 内保证），样本只来自事件与终态 run ——

  const startRealRun = async (): Promise<void> => {
    setIpcError(null);
    setResultSamples([]);
    setPercent(0);
    setRunState('running');
    const request: DiagnosticRequest = {
      target_ids: form.targetIds,
      route_mode: form.routeMode,
      manual_local_proxy_url: null,
      repeats: form.repeats,
      timeout_ms: Number(form.timeoutMs),
    };
    try {
      const session = await runDiagnostic(request, {
        onProgress: (payload) => {
          setPercent(payload.percent);
          if (payload.sample) {
            setResultSamples((previous) => upsertSample(previous, payload.sample as ProbeSample));
          }
        },
        onCompleted: ({ run }) => {
          sessionRef.current?.unsubscribe();
          sessionRef.current = null;
          // 终态 run 是权威结果：整体替换，避免事件乱序残留。
          setResultSamples(run.targets.flatMap((target) => target.samples));
          setRunState(
            run.status === 'cancelled' ? 'cancelled' : 'completed',
          );
        },
        onFailed: ({ error }) => {
          sessionRef.current?.unsubscribe();
          sessionRef.current = null;
          setIpcError(error);
          setRunState('failed');
        },
      });
      sessionRef.current = session;
    } catch (raw) {
      setIpcError(normalizeIpcError(raw));
      setRunState('failed');
    }
  };

  const onStart = (): void => {
    const found = validateForm(form);
    setErrors(found);
    if (Object.keys(found).length > 0) return;
    if (demoMode) {
      startDemoRun();
    } else {
      void startRealRun();
    }
  };

  const onCancel = async (): Promise<void> => {
    if (demoMode) {
      stopDemoTimer();
      setRunState('cancelled');
      return;
    }
    // 真实模式：交给后端取消令牌；终态由 completed 事件（run.status=cancelled）驱动。
    try {
      await api.cancelDiagnostic(sessionRef.current?.jobId ?? '');
    } catch (raw) {
      setIpcError(normalizeIpcError(raw));
    }
  };

  const running = runState === 'running';

  return (
    <div>
      <PageHeader
        title="服务诊断"
        description="选择目标与连接模式，对预设服务执行独立探测（DNS / TCP / HTTPS 相互独立）"
        actions={demoMode ? <DemoBadge /> : null}
      />

      {demoMode ? <DemoBanner /> : null}

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
              {!demoMode && form.routeMode === 'library_default' ? (
                <p className="text-xs text-muted-foreground">
                  库默认：代理选择受环境变量与 Windows 系统配置影响（PAC 不参与）；不保证与任何特定桌面应用一致。
                </p>
              ) : null}
              {!demoMode && form.routeMode === 'no_app_proxy' ? (
                <p className="text-xs text-muted-foreground">
                  无应用代理仅指不配置应用层 HTTP 代理；系统 TUN/透明路由仍可能生效。
                </p>
              ) : null}
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
                <Label htmlFor="repeats">采样次数（HTTPS）</Label>
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
              <Button onClick={() => void onCancel()} disabled={!running} variant="outline">
                <Square aria-hidden /> 取消
              </Button>
            </div>
          </CardContent>
        </Card>

        <div className="space-y-4">
          {ipcError ? <ErrorState error={ipcError} title="诊断未启动" /> : null}

          {runState === 'idle' && !ipcError ? (
            <EmptyState
              icon={Inbox}
              title="尚未开始诊断"
              description={
                demoMode
                  ? '演示模式：点击「开始诊断」查看模拟流程与数据形态。'
                  : '开始后将依次执行 DNS、TCP、HTTPS 三类独立探测，可随时取消。'
              }
            />
          ) : null}

          {running ? (
            <Alert variant="info">
              <AlertDescription>
                探测进行中…（{percent}%）。DNS/TCP 为独立基线，HTTPS 按模式采样；可点击「取消」中止。
              </AlertDescription>
            </Alert>
          ) : null}

          {resultSamples.length > 0 ? (
            <Card>
              <CardHeader>
                <div className="flex items-center justify-between">
                  <CardTitle className="text-base">
                    诊断结果
                    {demoMode ? '（模拟）' : ''}
                  </CardTitle>
                  <div className="flex items-center gap-2">
                    {runState === 'completed' ? (
                      <span className="text-xs text-muted-foreground">已完成</span>
                    ) : null}
                    {runState === 'cancelled' ? (
                      <span className="text-xs text-amber-500">已取消（已完成样本保留）</span>
                    ) : null}
                    {demoMode ? <DemoBadge /> : null}
                  </div>
                </div>
              </CardHeader>
              <CardContent className="space-y-4">
                {Array.from(new Set(resultSamples.map((sample) => sample.target_id))).map(
                  (targetId) => {
                    const targetSamples = resultSamples.filter(
                      (sample) => sample.target_id === targetId,
                    );
                    const probeTypes = Array.from(
                      new Set(targetSamples.map((sample) => sample.probe_type)),
                    );
                    return (
                      <div key={targetId} className="space-y-2">
                        <div className="flex flex-wrap items-center gap-2 text-sm font-medium">
                          <span>{targetId}</span>
                          {probeTypes.map((probeType) => {
                            const aggregate = aggregateSamples(
                              probeType,
                              targetSamples.filter((sample) => sample.probe_type === probeType),
                            );
                            const rate = transportFailureRate(aggregate);
                            return (
                              <span
                                key={probeType}
                                className="rounded bg-secondary px-1.5 py-0.5 text-xs text-secondary-foreground"
                              >
                                {PROBE_TYPE_LABELS[probeType]}：{aggregate.success_count}/
                                {aggregate.attempts_total} 成功 · 传输失败 {rate === null ? '—' : fmtPercent(rate)} ·
                                非2xx {fmtPercent(non2xxResponseRatio(aggregate))} · med{' '}
                                {fmtDurationMs(aggregate.duration_median_ms)}
                              </span>
                            );
                          })}
                        </div>
                        <div className="overflow-x-auto rounded-md border border-border">
                          <table className="w-full text-sm">
                            <thead className="bg-muted/50 text-left text-xs text-muted-foreground">
                              <tr>
                                <th className="px-3 py-2 font-medium">探测</th>
                                <th className="px-3 py-2 font-medium">#</th>
                                <th className="px-3 py-2 font-medium">结果</th>
                                <th className="px-3 py-2 font-medium">HTTP 状态</th>
                                <th className="px-3 py-2 font-medium">耗时</th>
                              </tr>
                            </thead>
                            <tbody>
                              {targetSamples.map((sample) => (
                                <tr
                                  key={`${sample.probe_type}-${sample.attempt_no}`}
                                  className="border-t border-border"
                                >
                                  <td className="px-3 py-1.5">{PROBE_TYPE_LABELS[sample.probe_type]}</td>
                                  <td className="px-3 py-1.5">{sample.attempt_no}</td>
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
                  },
                )}
                {demoMode ? (
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
