import { useEffect, useState } from 'react';
import { FileDown, History as HistoryIcon, Trash2 } from 'lucide-react';

import { DiagnosisList } from '@/components/diagnosis/DiagnosisList';
import { DemoBadge, DemoBanner } from '@/components/layout/demo';
import { EmptyState, ErrorState } from '@/components/layout/states';
import { PageHeader } from '@/components/layout/PageHeader';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Label } from '@/components/ui/label';
import { Select } from '@/components/ui/select';
import { useAppSettings } from '@/app/settings';
import { api, normalizeIpcError } from '@/lib/api';
import { fmtDurationMs, fmtPercent, fmtUtcToLocal } from '@/lib/format';
import { non2xxResponseRatio, transportFailureRate } from '@/lib/metrics';
import type {
  Diagnosis,
  DiagnosticRun,
  IpcError,
  ReportFormat,
  ReportPreview,
  RouteMode,
  RunSummary,
} from '@/lib/types';
import { PRESET_TARGETS, ROUTE_MODE_LABELS } from '@/lib/types';
import { MOCK_HISTORY_RUNS } from '@/features/demo/mockData';

const STATUS_LABELS: Record<string, string> = {
  completed: '已完成',
  cancelled: '已取消',
  running: '进行中',
  failed: '失败',
};

export function HistoryPage() {
  const { settings } = useAppSettings();
  const demoMode = settings.demoMode;

  const [runs, setRuns] = useState<RunSummary[]>([]);
  const [loading, setLoading] = useState(!demoMode);
  const [error, setError] = useState<IpcError | null>(null);
  const [targetFilter, setTargetFilter] = useState('');
  const [modeFilter, setModeFilter] = useState('');
  const [detail, setDetail] = useState<{ run: DiagnosticRun; diagnoses: Diagnosis[] } | null>(null);
  const [preview, setPreview] = useState<ReportPreview | null>(null);
  const [exportPath, setExportPath] = useState<string | null>(null);
  /** 保存被拒绝的明确提示（F-10：不得并入笼统的"保存失败"）。 */
  const [saveNotice, setSaveNotice] = useState<string | null>(null);
  const [confirmClear, setConfirmClear] = useState(false);

  const refresh = (target: string, mode: string): void => {
    void (async () => {
      setError(null);
      try {
        const items = await api.listRuns({
          target_id: target || null,
          route_mode: (mode || null) as RouteMode | null,
          limit: 200,
        });
        setRuns(items);
      } catch (raw) {
        setError(normalizeIpcError(raw));
      } finally {
        setLoading(false);
      }
    })();
  };

  useEffect(() => {
    if (!demoMode) {
      void refresh(targetFilter, modeFilter);
    }
  }, [demoMode, targetFilter, modeFilter]);

  /** 最近两次可比运行（相同目标集合 + 模式 + 采样数）的中位数趋势。 */
  const trend = (() => {
    for (const run of runs) {
      const key = `${run.target_ids.join(',')}|${run.route_mode}|${run.repeats}`;
      const comparable = runs.filter(
        (item) =>
          `${item.target_ids.join(',')}|${item.route_mode}|${item.repeats}` === key &&
          item.https_median_ms !== null &&
          run.https_median_ms !== null,
      );
      if (comparable.length >= 2) {
        const [latest, previous] = comparable;
        return { latest, previous };
      }
    }
    return null;
  })();

  const openDetail = (runId: string): void => {
    setExportPath(null);
    setSaveNotice(null);
    Promise.all([api.getRun(runId), api.diagnoseRuns([]).then(() => null)])
      .then(([,]) => api.getRun(runId))
      .then((run) =>
        api.diagnoseRuns([run]).then((diagnoses) => {
          setDetail({ run, diagnoses });
        }),
      )
      .catch((raw) => setError(normalizeIpcError(raw)));
  };

  const onDelete = (runId: string): void => {
    api
      .deleteRun(runId)
      .then(() => {
        if (detail?.run.run_id === runId) setDetail(null);
        refresh(targetFilter, modeFilter);
      })
      .catch((raw) => setError(normalizeIpcError(raw)));
  };

  const onClear = (): void => {
    api
      .clearHistory()
      .then(() => {
        setConfirmClear(false);
        setDetail(null);
        refresh(targetFilter, modeFilter);
      })
      .catch((raw) => setError(normalizeIpcError(raw)));
  };

  const onPreview = (runId: string, format: ReportFormat): void => {
    setExportPath(null);
    setSaveNotice(null);
    api
      .previewSanitizedReport(runId, format)
      .then(setPreview)
      .catch((raw) => setError(normalizeIpcError(raw)));
  };

  const onExport = (): void => {
    if (!preview) return;
    setSaveNotice(null);
    api
      .exportSanitizedReport(preview.run_id, preview.format, preview.snapshot_id)
      .then((result) => {
        setExportPath(result.path);
        setPreview(null);
      })
      .catch((raw) => {
        const error = normalizeIpcError(raw);
        if (error.code === 'cancelled') {
          return; // 用户在另存为对话框取消：快照保留，可再次导出。
        }
        if (error.code === 'target_exists') {
          // F-10：目标已存在是明确、可操作的结果——不用笼统错误呈现，
          // 保留预览与快照，用户换个文件名即可重试。
          setSaveNotice('文件已存在，请选择其他文件名');
          return;
        }
        setError(error);
      });
  };

  return (
    <div>
      <PageHeader
        title="历史记录"
        description="本地 SQLite 持久化（30 天 / 500 次保留策略，可在设置中调整）"
        actions={
          <>
            {demoMode ? <DemoBadge /> : null}
            {!demoMode && runs.length > 0 ? (
              <Button
                variant="destructive"
                size="sm"
                onClick={() => setConfirmClear(true)}
              >
                <Trash2 aria-hidden /> 清空历史
              </Button>
            ) : null}
          </>
        }
      />

      {demoMode ? <DemoBanner /> : null}
      {error ? <ErrorState error={error} title="历史操作失败" /> : null}

      {confirmClear ? (
        <Alert variant="destructive" className="mb-4">
          <AlertDescription>
            <p>确认清空全部历史记录？该操作不可恢复。</p>
            <div className="mt-2 flex gap-2">
              <Button size="sm" variant="destructive" onClick={onClear}>
                确认清空
              </Button>
              <Button size="sm" variant="outline" onClick={() => setConfirmClear(false)}>
                取消
              </Button>
            </div>
          </AlertDescription>
        </Alert>
      ) : null}

      <div className="mb-4 grid gap-3 sm:grid-cols-2 lg:max-w-md">
        <div className="space-y-1.5">
          <Label htmlFor="history-target">按目标筛选</Label>
          <Select
            id="history-target"
            value={targetFilter}
            disabled={demoMode}
            onChange={(event) => setTargetFilter(event.target.value)}
          >
            <option value="">全部目标</option>
            {PRESET_TARGETS.map((target) => (
              <option key={target.id} value={target.id}>
                {target.label}
              </option>
            ))}
          </Select>
        </div>
        <div className="space-y-1.5">
          <Label htmlFor="history-mode">按模式筛选</Label>
          <Select
            id="history-mode"
            value={modeFilter}
            disabled={demoMode}
            onChange={(event) => setModeFilter(event.target.value)}
          >
            <option value="">全部模式</option>
            {(Object.keys(ROUTE_MODE_LABELS) as RouteMode[]).map((mode) => (
              <option key={mode} value={mode}>
                {ROUTE_MODE_LABELS[mode]}
              </option>
            ))}
          </Select>
        </div>
      </div>

      {!demoMode && trend ? (
        <Card className="mb-4">
          <CardHeader>
            <CardTitle className="text-base">最近两次可比运行</CardTitle>
          </CardHeader>
          <CardContent className="text-sm">
            <p>
              {fmtUtcToLocal(trend.latest.started_at)}：中位 {fmtDurationMs(trend.latest.https_median_ms)}｜
              {fmtUtcToLocal(trend.previous.started_at)}：中位 {fmtDurationMs(trend.previous.https_median_ms)}
            </p>
            <p className="mt-1 text-xs text-muted-foreground">
              仅相同目标 / 模式 / 采样次数的运行可比；样本量 {trend.latest.sample_count}，差异不构成统计显著结论。
            </p>
          </CardContent>
        </Card>
      ) : null}

      {demoMode ? (
        <Card>
          <CardHeader>
            <div className="flex items-center justify-between">
              <CardTitle className="text-base">历史会话（模拟）</CardTitle>
              <DemoBadge />
            </div>
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
                  {MOCK_HISTORY_RUNS.map((item) => (
                    <tr key={item.run_id} className="border-t border-border">
                      <td className="px-3 py-2">{fmtUtcToLocal(item.started_at)}</td>
                      <td className="px-3 py-2">{item.targets}</td>
                      <td className="px-3 py-2">{ROUTE_MODE_LABELS[item.route_mode as RouteMode]}</td>
                      <td className="px-3 py-2">
                        {item.status === 'cancelled' ? (
                          <span className="text-amber-500">已取消</span>
                        ) : (
                          '已完成'
                        )}
                      </td>
                      <td className="px-3 py-2">{item.samples}</td>
                      <td className="px-3 py-2">{fmtDurationMs(item.median_ms)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </CardContent>
        </Card>
      ) : loading ? (
        <EmptyState icon={HistoryIcon} title="加载中…" />
      ) : runs.length === 0 ? (
        <EmptyState
          icon={HistoryIcon}
          title="暂无历史记录"
          description="完成一次诊断后，运行结果会自动保存到本地（重启应用后仍可查询）。"
        />
      ) : (
        <Card>
          <CardHeader>
            <CardTitle className="text-base">历史运行（{runs.length}）</CardTitle>
          </CardHeader>
          <CardContent className="overflow-x-auto">
            <table className="w-full text-sm">
              <thead className="bg-muted/50 text-left text-xs text-muted-foreground">
                <tr>
                  <th className="px-3 py-2 font-medium">开始时间（本地）</th>
                  <th className="px-3 py-2 font-medium">目标</th>
                  <th className="px-3 py-2 font-medium">模式</th>
                  <th className="px-3 py-2 font-medium">状态</th>
                  <th className="px-3 py-2 font-medium">样本</th>
                  <th className="px-3 py-2 font-medium">成功</th>
                  <th className="px-3 py-2 font-medium">传输失败</th>
                  <th className="px-3 py-2 font-medium">中位耗时</th>
                  <th className="px-3 py-2 font-medium">操作</th>
                </tr>
              </thead>
              <tbody>
                {runs.map((item) => (
                  <tr
                    key={item.run_id}
                    className="cursor-pointer border-t border-border hover:bg-accent/40"
                    onClick={() => openDetail(item.run_id)}
                  >
                    <td className="px-3 py-2">{fmtUtcToLocal(item.started_at)}</td>
                    <td className="px-3 py-2">{item.target_ids.join('、')}</td>
                    <td className="px-3 py-2">{ROUTE_MODE_LABELS[item.route_mode as RouteMode] ?? item.route_mode}</td>
                    <td className="px-3 py-2">
                      {item.status === 'cancelled' ? (
                        <span className="text-amber-500">已取消</span>
                      ) : (
                        STATUS_LABELS[item.status] ?? item.status
                      )}
                    </td>
                    <td className="px-3 py-2">{item.sample_count}</td>
                    <td className="px-3 py-2">{item.success_count}</td>
                    <td className="px-3 py-2">{item.transport_failure_count}</td>
                    <td className="px-3 py-2">{fmtDurationMs(item.https_median_ms)}</td>
                    <td className="px-3 py-2" onClick={(event) => event.stopPropagation()}>
                      <div className="flex gap-1">
                        <Button size="sm" variant="outline" onClick={() => onDelete(item.run_id)}>
                          删除
                        </Button>
                        <Button
                          size="sm"
                          variant="outline"
                          onClick={() => onPreview(item.run_id, 'markdown')}
                        >
                          <FileDown aria-hidden /> 报告
                        </Button>
                      </div>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </CardContent>
        </Card>
      )}

      {saveNotice ? (
        <Alert variant="warning" className="mt-4">
          <AlertDescription>{saveNotice}</AlertDescription>
        </Alert>
      ) : null}

      {exportPath ? (
        <Alert variant="info" className="mt-4">
          <AlertDescription>
            报告已导出（本地文件，未上传）：{exportPath}
          </AlertDescription>
        </Alert>
      ) : null}

      {preview ? (
        <Card className="mt-4">
          <CardHeader>
            <div className="flex items-center justify-between">
              <CardTitle className="text-base">脱敏报告预览（{preview.filename}）</CardTitle>
              <div className="flex gap-2">
                <Button size="sm" onClick={onExport}>
                  确认导出
                </Button>
                <Button size="sm" variant="outline" onClick={() => setPreview(null)}>
                  取消
                </Button>
              </div>
            </div>
          </CardHeader>
          <CardContent>
            <p className="mb-2 text-xs text-muted-foreground">{preview.note}</p>
            <pre className="max-h-80 overflow-auto rounded-md border border-border bg-muted/30 p-3 text-xs">
              {preview.content.slice(0, 4000)}
              {preview.content.length > 4000 ? '\n…（预览截断，导出为完整内容）' : ''}
            </pre>
          </CardContent>
        </Card>
      ) : null}

      {detail ? (
        <Card className="mt-4">
          <CardHeader>
            <div className="flex items-center justify-between">
              <CardTitle className="text-base">运行详情（{detail.run.run_id}）</CardTitle>
              <Button size="sm" variant="outline" onClick={() => setDetail(null)}>
                关闭
              </Button>
            </div>
          </CardHeader>
          <CardContent className="space-y-3">
            {detail.run.targets.map((target) => {
              const https = target.aggregates.find(
                (aggregate) => aggregate.probe_type === 'https',
              );
              const rate = https ? transportFailureRate(https) : null;
              return (
                <div key={target.target_id} className="text-sm">
                  <p className="font-medium">{target.target_id}</p>
                  {https ? (
                    <p className="text-xs text-muted-foreground">
                      HTTPS：成功 {https.success_count}/{https.attempts_total} · 传输失败{' '}
                      {https.transport_failure_count}（{rate === null ? '—' : fmtPercent(rate)}）· 非2xx{' '}
                      {fmtPercent(non2xxResponseRatio(https))} · 中位 {fmtDurationMs(https.duration_median_ms)}
                    </p>
                  ) : null}
                </div>
              );
            })}
            <DiagnosisList diagnoses={detail.diagnoses} />
          </CardContent>
        </Card>
      ) : null}
    </div>
  );
}
