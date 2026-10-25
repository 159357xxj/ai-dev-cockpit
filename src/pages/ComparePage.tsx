import { useEffect, useRef, useState } from 'react';
import { GitCompareArrows, Play, Square } from 'lucide-react';

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
import { DiagnosisList } from '@/components/diagnosis/DiagnosisList';
import { useAppSettings } from '@/app/settings';
import { api, normalizeIpcError, runDiagnostic, type DiagnosticRunSession } from '@/lib/api';
import { fmtDurationMs, fmtPercent, fmtUtcToLocal } from '@/lib/format';
import { aggregateSamples, non2xxResponseRatio, transportFailureRate } from '@/lib/metrics';
import { validateProxyUrl } from '@/pages/DiagnosticsPage';
import {
  PRESET_TARGETS,
  REPEATS_ALLOWED,
  ROUTE_MODE_LABELS,
  TIMEOUT_DEFAULT_MS,
  TIMEOUT_MAX_MS,
  TIMEOUT_MIN_MS,
} from '@/lib/types';
import type {
  Diagnosis,
  DiagnosticRun,
  IpcError,
  ProbeSample,
  RouteMode,
} from '@/lib/types';
import { cn } from '@/lib/utils';
import { buildDemoCompare, MOCK_DIAGNOSES } from '@/features/demo/mockData';

/** 对比中每个模式的执行状态。 */
interface ModeRunState {
  status: 'pending' | 'running' | 'completed' | 'cancelled' | 'failed';
  samples: ProbeSample[];
  run: DiagnosticRun | null;
  error: IpcError | null;
  /** 本轮对比批次号：用于丢弃取消后残留的旧会话结果。 */
  batch: number;
}

const COMPARE_NOTES = [
  '仅比较相同目标的 HTTPS 样本；DNS/TCP 是独立本机基线，不参与代理路径对比。',
  '各模式在同一批次内按顺序执行（相近时间窗口），不同模式不共享任何样本。',
  'no_app_proxy 仅指不使用应用层显式代理；TUN/透明路由仍可能生效——本工具不声称能检测或绕过。',
  'library_default 的代理选择受环境变量与 Windows 系统配置影响（优先级链见 ARCHITECTURE §7.2，PAC 不参与）。',
  'manual_local_proxy 仅接受 loopback 地址；代理不可达归类 proxy_unavailable，绝不回落直连。',
  'HTTP 401/403 表示「收到响应，但业务能力未验证」，不标记为业务正常。',
  '中位数仅基于成功样本计算；无有效样本时显示 —。单次对比不构成某路径必然更快的结论。',
];

const ALL_MODES: RouteMode[] = ['no_app_proxy', 'library_default', 'manual_local_proxy'];

export function ComparePage() {
  const { settings } = useAppSettings();
  const demoMode = settings.demoMode;

  const [targetId, setTargetId] = useState('github');
  const [selectedModes, setSelectedModes] = useState<RouteMode[]>(['no_app_proxy']);
  const [manualProxyUrl, setManualProxyUrl] = useState('');
  const [repeats, setRepeats] = useState(3);
  const [timeoutMs, setTimeoutMs] = useState(String(TIMEOUT_DEFAULT_MS));
  const [formError, setFormError] = useState<string | null>(null);
  const [runningMode, setRunningMode] = useState<RouteMode | null>(null);
  const [batch, setBatch] = useState(0);
  const [modeStates, setModeStates] = useState<Record<string, ModeRunState>>({});
  const [diagnoses, setDiagnoses] = useState<Diagnosis[] | null>(null);
  const sessionRef = useRef<DiagnosticRunSession | null>(null);
  const batchRef = useRef(0);

  useEffect(
    () => () => {
      batchRef.current += 1; // 使残留回调失效
      sessionRef.current?.unsubscribe();
    },
    [],
  );

  const toggleMode = (mode: RouteMode): void => {
    setSelectedModes((previous) =>
      previous.includes(mode)
        ? previous.filter((item) => item !== mode)
        : [...previous, mode],
    );
  };

  // modeStates 的同步镜像，供 waitForTerminal 读取最新值。
  const modeStatesRef = useRef(modeStates);
  useEffect(() => {
    modeStatesRef.current = modeStates;
  }, [modeStates]);

  const validate = (): string | null => {
    if (selectedModes.length === 0) return '至少选择一种要对比的请求模式';
    if (selectedModes.includes('manual_local_proxy')) {
      const problem = validateProxyUrl(manualProxyUrl);
      if (manualProxyUrl.trim() === '') return '手动代理模式必须提供本地代理地址';
      if (problem) return problem;
    }
    const timeout = Number(timeoutMs);
    if (!Number.isInteger(timeout) || timeout < TIMEOUT_MIN_MS || timeout > TIMEOUT_MAX_MS) {
      return `单请求超时须在 ${TIMEOUT_MIN_MS}..=${TIMEOUT_MAX_MS} 毫秒`;
    }
    return null;
  };

  const upsertSample = (samples: ProbeSample[], incoming: ProbeSample): ProbeSample[] => {
    const key = (sample: ProbeSample): string =>
      `${sample.probe_type}|${sample.attempt_no}`;
    return [...samples.filter((sample) => key(sample) !== key(incoming)), incoming].sort(
      (a, b) => a.attempt_no - b.attempt_no,
    );
  };

  const onCancel = async (): Promise<void> => {
    if (demoMode) return; // 演示模式无真实任务
    try {
      if (sessionRef.current) {
        await api.cancelDiagnostic(sessionRef.current.jobId);
      }
    } catch (raw) {
      setFormError(normalizeIpcError(raw).message);
    }
  };

  const startCompare = async (): Promise<void> => {
    const problem = validate();
    setFormError(problem);
    if (problem) return;
    if (demoMode) return; // 演示数据为静态展示，无需执行

    setDiagnoses(null);
    const currentBatch = batch + 1;
    setBatch(currentBatch);
    batchRef.current = currentBatch;
    const initial: Record<string, ModeRunState> = {};
    for (const mode of selectedModes) {
      initial[mode] = { status: 'pending', samples: [], run: null, error: null, batch: currentBatch };
    }
    setModeStates(initial);
    setRunningMode(selectedModes[0]);

    const timeout = Number(timeoutMs);
    let aborted = false;

    for (const mode of selectedModes) {
      if (batchRef.current !== currentBatch || aborted) break;
      setRunningMode(mode);
      setModeStates((previous) => ({
        ...previous,
        [mode]: { status: 'running', samples: [], run: null, error: null, batch: currentBatch },
      }));

      const request = {
        target_ids: [targetId],
        route_mode: mode,
        manual_local_proxy_url: mode === 'manual_local_proxy' ? manualProxyUrl.trim() : null,
        repeats,
        timeout_ms: timeout,
      };

      try {
        const session = await runDiagnostic(request, {
          onProgress: (payload) => {
            if (batchRef.current !== currentBatch) return;
            if (payload.sample) {
              setModeStates((previous) => {
                const state = previous[mode];
                if (!state || state.batch !== currentBatch) return previous;
                return {
                  ...previous,
                  [mode]: { ...state, samples: upsertSample(state.samples, payload.sample as ProbeSample) },
                };
              });
            }
          },
          onCompleted: ({ run }) => {
            if (batchRef.current !== currentBatch) return;
            sessionRef.current?.unsubscribe();
            sessionRef.current = null;
            setModeStates((previous) => ({
              ...previous,
              [mode]: {
                status: run.status === 'cancelled' ? 'cancelled' : 'completed',
                samples: run.targets.flatMap((target) => target.samples.filter((sample) => sample.probe_type === 'https')),
                run,
                error: null,
                batch: currentBatch,
              },
            }));
            aborted = run.status === 'cancelled';
          },
          onFailed: ({ error }) => {
            if (batchRef.current !== currentBatch) return;
            sessionRef.current?.unsubscribe();
            sessionRef.current = null;
            setModeStates((previous) => ({
              ...previous,
              [mode]: { status: 'failed', samples: [], run: null, error, batch: currentBatch },
            }));
            aborted = true;
          },
        });
        sessionRef.current = session;
        // 等待当前模式终态（completed/failed 回调置位）。
        await waitForTerminal(currentBatch, mode);
      } catch (raw) {
        const error = normalizeIpcError(raw);
        setModeStates((previous) => ({
          ...previous,
          [mode]: { status: 'failed', samples: [], run: null, error, batch: currentBatch },
        }));
        aborted = true;
      }
    }
    setRunningMode(null);

    // 批次结束：对已完成的运行请求规则诊断（纯函数 IPC；失败则不出卡片，不伪造）。
    const finished = Object.values(modeStatesRef.current)
      .filter((state) => state.batch === currentBatch && state.run !== null)
      .map((state) => state.run as DiagnosticRun);
    if (finished.length > 0) {
      api
        .diagnoseRuns(finished)
        .then((items) => setDiagnoses(items))
        .catch(() => setDiagnoses(null));
    } else {
      setDiagnoses(null);
    }
  };

  /** 等待指定模式的终态（轮询状态，终态由事件回调写入）。 */
  const waitForTerminal = async (currentBatch: number, mode: RouteMode): Promise<void> => {
    for (let waited = 0; waited < 200; waited += 1) {
      if (batchRef.current !== currentBatch) return;
      const state = modeStatesRef.current[mode];
      if (state && state.status !== 'pending' && state.status !== 'running') return;
      await new Promise((resolve) => setTimeout(resolve, 100));
    }
  };

  const running = runningMode !== null;
  const hasResults = Object.values(modeStates).some((state) => state.samples.length > 0);

  return (
    <div>
      <PageHeader
        title="路径对比"
        description="对同一目标用不同请求模式独立测量 HTTPS 并对比"
        actions={demoMode ? <DemoBadge /> : null}
      />

      {demoMode ? <DemoBanner /> : null}

      <div className="grid gap-4 lg:grid-cols-[380px_minmax(0,1fr)]">
        <Card>
          <CardHeader>
            <CardTitle className="text-base">对比配置</CardTitle>
            <CardDescription>各模式按顺序独立执行，样本互不共享</CardDescription>
          </CardHeader>
          <CardContent className="space-y-4">
            <div className="space-y-1.5">
              <Label htmlFor="compare-target">对比目标（单一）</Label>
              <Select
                id="compare-target"
                value={targetId}
                disabled={running}
                onChange={(event) => setTargetId(event.target.value)}
              >
                {PRESET_TARGETS.map((target) => (
                  <option key={target.id} value={target.id}>
                    {target.label}（{target.host}）
                  </option>
                ))}
              </Select>
            </div>

            <div className="space-y-1.5">
              <Label>对比模式（可多选）</Label>
              <div className="flex flex-wrap gap-2">
                {ALL_MODES.map((mode) => {
                  const active = selectedModes.includes(mode);
                  return (
                    <button
                      key={mode}
                      type="button"
                      aria-pressed={active}
                      disabled={running}
                      onClick={() => toggleMode(mode)}
                      className={cn(
                        'rounded-md border px-3 py-1.5 text-xs transition-colors',
                        active
                          ? 'border-primary bg-primary/10 font-medium text-foreground'
                          : 'border-border text-muted-foreground hover:bg-accent hover:text-foreground',
                      )}
                    >
                      {ROUTE_MODE_LABELS[mode]}
                    </button>
                  );
                })}
              </div>
            </div>

            {selectedModes.includes('manual_local_proxy') ? (
              <div className="space-y-1.5">
                <Label htmlFor="compare-proxy">本地代理地址</Label>
                <Input
                  id="compare-proxy"
                  placeholder="http://127.0.0.1:7890 或 socks5h://127.0.0.1:7890"
                  value={manualProxyUrl}
                  disabled={running}
                  onChange={(event) => setManualProxyUrl(event.target.value)}
                />
              </div>
            ) : null}

            <div className="grid grid-cols-2 gap-3">
              <div className="space-y-1.5">
                <Label htmlFor="compare-repeats">每模式采样次数</Label>
                <Select
                  id="compare-repeats"
                  value={String(repeats)}
                  disabled={running}
                  onChange={(event) => setRepeats(Number(event.target.value))}
                >
                  {REPEATS_ALLOWED.map((count) => (
                    <option key={count} value={count}>
                      {count} 次
                    </option>
                  ))}
                </Select>
              </div>
              <div className="space-y-1.5">
                <Label htmlFor="compare-timeout">单请求超时（毫秒）</Label>
                <Input
                  id="compare-timeout"
                  type="number"
                  min={TIMEOUT_MIN_MS}
                  max={TIMEOUT_MAX_MS}
                  value={timeoutMs}
                  disabled={running}
                  onChange={(event) => setTimeoutMs(event.target.value)}
                />
              </div>
            </div>

            {formError ? <p className="text-xs text-destructive">{formError}</p> : null}

            <div className="flex gap-2 pt-1">
              <Button onClick={() => void startCompare()} disabled={running} className="flex-1">
                <Play aria-hidden /> 开始对比
              </Button>
              <Button onClick={() => void onCancel()} disabled={!running} variant="outline">
                <Square aria-hidden /> 取消
              </Button>
            </div>
          </CardContent>
        </Card>

        <div className="space-y-4">
          {demoMode ? (
            <Card>
              <CardHeader>
                <div className="flex items-center justify-between">
                  <CardTitle className="text-base">github.com — 三种连接模式对比（模拟）</CardTitle>
                  <DemoBadge />
                </div>
              </CardHeader>
              <CardContent>
                <CompareTable
                  rows={buildDemoCompare().map(({ route_mode: mode, samples }) => ({
                    mode,
                    samples,
                    testedAt: null,
                  }))}
                />
              </CardContent>
            </Card>
          ) : null}

          {!demoMode && !hasResults && !running ? (
            <EmptyState
              icon={GitCompareArrows}
              title="尚无对比结果"
              description="选择目标与模式后点击「开始对比」。三种模式将按顺序各自独立执行，结果互不共享。"
            />
          ) : null}

          {!demoMode && formError === null && running ? (
            <Alert variant="info">
              <AlertDescription>
                正在执行：{runningMode ? ROUTE_MODE_LABELS[runningMode] : ''}（批次内按顺序执行，可取消）
              </AlertDescription>
            </Alert>
          ) : null}

          {!demoMode && Object.keys(modeStates).length > 0 ? (
            <Card>
              <CardHeader>
                <CardTitle className="text-base">
                  对比结果（{targetId}）
                </CardTitle>
                <CardDescription>仅统计 HTTPS 样本；状态列如实标注执行情况</CardDescription>
              </CardHeader>
              <CardContent className="space-y-3">
                {selectedModes.map((mode) => {
                  const state = modeStates[mode];
                  if (!state) return null;
                  return (
                    <div key={mode} className="space-y-1">
                      <div className="flex items-center gap-2 text-sm font-medium">
                        <span>{ROUTE_MODE_LABELS[mode]}</span>
                        <span
                          className={cn(
                            'rounded px-1.5 py-0.5 text-xs',
                            state.status === 'completed' && 'bg-secondary text-secondary-foreground',
                            state.status === 'running' && 'bg-blue-500/10 text-blue-500',
                            state.status === 'cancelled' && 'bg-amber-500/10 text-amber-500',
                            state.status === 'failed' && 'bg-destructive/10 text-destructive',
                            state.status === 'pending' && 'bg-muted text-muted-foreground',
                          )}
                        >
                          {state.status === 'completed'
                            ? '完成'
                            : state.status === 'running'
                              ? '执行中'
                              : state.status === 'cancelled'
                                ? '已取消'
                                : state.status === 'failed'
                                  ? '失败'
                                  : '排队中'}
                        </span>
                        {state.status === 'failed' && state.error ? (
                          <span className="text-xs text-destructive">{state.error.message}</span>
                        ) : null}
                      </div>
                      {state.error ? <ErrorState error={state.error} title={`${ROUTE_MODE_LABELS[mode]} 失败`} /> : null}
                      {state.samples.length > 0 ? (
                        <ModeStatRow mode={mode} samples={state.samples} testedAt={state.run?.finished_at ?? null} />
                      ) : null}
                    </div>
                  );
                })}
              </CardContent>
            </Card>
          ) : null}

          {demoMode ? <DiagnosisList diagnoses={MOCK_DIAGNOSES} isDemo /> : null}
          {!demoMode && diagnoses ? <DiagnosisList diagnoses={diagnoses} /> : null}

          <Card>
            <CardHeader>
              <CardTitle className="text-base">对比边界说明</CardTitle>
            </CardHeader>
            <CardContent>
              <ul className="list-disc space-y-1 pl-5 text-xs text-muted-foreground">
                {COMPARE_NOTES.map((note) => (
                  <li key={note}>{note}</li>
                ))}
              </ul>
            </CardContent>
          </Card>
        </div>
      </div>
    </div>
  );
}

function ModeStatRow({
  mode,
  samples,
  testedAt,
}: {
  mode: RouteMode;
  samples: ProbeSample[];
  testedAt: string | null;
}) {
  const aggregate = aggregateSamples('https', samples);
  const rate = transportFailureRate(aggregate);
  return (
    <div className="overflow-x-auto rounded-md border border-border">
      <div className="bg-muted/40 px-3 py-1.5 text-xs font-medium text-muted-foreground">
        {ROUTE_MODE_LABELS[mode]}
      </div>
      <table className="w-full text-sm">
        <tbody>
          <tr className="border-b border-border">
            <td className="px-3 py-2 text-xs text-muted-foreground">样本</td>
            <td className="px-3 py-2">{aggregate.attempts_total}</td>
            <td className="px-3 py-2 text-xs text-muted-foreground">成功</td>
            <td className="px-3 py-2">{aggregate.success_count}</td>
            <td className="px-3 py-2 text-xs text-muted-foreground">传输失败</td>
            <td className="px-3 py-2">
              {aggregate.transport_failure_count}（{fmtPercent(rate)}）
            </td>
          </tr>
          <tr className="border-b border-border">
            <td className="px-3 py-2 text-xs text-muted-foreground">收到响应</td>
            <td className="px-3 py-2">{aggregate.http_response_count}</td>
            <td className="px-3 py-2 text-xs text-muted-foreground">非 2xx 比例</td>
            <td className="px-3 py-2">{fmtPercent(non2xxResponseRatio(aggregate))}</td>
            <td className="px-3 py-2 text-xs text-muted-foreground">成功中位耗时</td>
            <td className="px-3 py-2">{fmtDurationMs(aggregate.duration_median_ms)}</td>
          </tr>
          <tr>
            <td className="px-3 py-2 text-xs text-muted-foreground">耗时范围</td>
            <td className="px-3 py-2">
              {fmtDurationMs(aggregate.duration_min_ms)} ~ {fmtDurationMs(aggregate.duration_max_ms)}
            </td>
            <td className="px-3 py-2 text-xs text-muted-foreground">完成时间（本地）</td>
            <td className="px-3 py-2" colSpan={3}>
              {testedAt ? fmtUtcToLocal(testedAt) : '—'}
            </td>
          </tr>
        </tbody>
      </table>
    </div>
  );
}

function CompareTable({
  rows,
}: {
  rows: Array<{ mode: RouteMode; samples: ProbeSample[]; testedAt: string | null }>;
}) {
  return (
    <div className="space-y-2">
      {rows.map((row) => (
        <ModeStatRow key={row.mode} mode={row.mode} samples={row.samples} testedAt={row.testedAt} />
      ))}
    </div>
  );
}
