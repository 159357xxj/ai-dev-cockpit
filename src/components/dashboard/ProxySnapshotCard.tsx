import { useEffect, useState } from 'react';
import { ShieldCheck } from 'lucide-react';

import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { api, normalizeIpcError } from '@/lib/api';
import { fmtUtcToLocal } from '@/lib/format';
import type { IpcError, ProxySnapshot } from '@/lib/types';

/** Windows 代理配置快照卡片：真实只读本地数据（无网络请求），全量脱敏后展示。 */
export function ProxySnapshotCard() {
  const [snapshot, setSnapshot] = useState<ProxySnapshot | null>(null);
  const [error, setError] = useState<IpcError | null>(null);

  useEffect(() => {
    let cancelled = false;
    api
      .getProxySnapshot()
      .then((value) => {
        if (!cancelled) setSnapshot(value);
      })
      .catch((raw) => {
        if (!cancelled) setError(normalizeIpcError(raw));
      });
    return () => {
      cancelled = true;
    };
  }, []);

  return (
    <Card>
      <CardHeader>
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2">
            <ShieldCheck className="size-4 text-muted-foreground" aria-hidden />
            <CardTitle className="text-base">代理配置快照（真实·只读本地）</CardTitle>
          </div>
          <Badge variant="secondary">零联网</Badge>
        </div>
        <CardDescription>
          三个来源分别检测；配置存在 ≠ 真实请求使用该配置；PAC 不下载不执行
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-2 text-sm">
        {error ? (
          <p className="text-xs text-destructive">
            代理快照不可用：{error.message}
          </p>
        ) : null}
        {!error && !snapshot ? (
          <p className="text-xs text-muted-foreground">读取中…</p>
        ) : null}
        {snapshot ? (
          <>
            <ProxyLine
              label="用户代理（WinINet）"
              status={snapshot.user_settings.status}
              readError={snapshot.user_settings.read_error}
              value={describeUserProxy(snapshot)}
            />
            <ProxyLine
              label="WinHTTP（机器级）"
              status={snapshot.winhttp.status}
              readError={snapshot.winhttp.read_error}
              value={describeWinHttp(snapshot)}
            />
            <div className="flex items-start justify-between gap-2">
              <span className="text-muted-foreground">环境变量代理</span>
              <span className="text-right">
                {describeEnvVars(snapshot.env_vars) ?? '未设置'}
              </span>
            </div>
            <p className="text-xs text-muted-foreground">
              快照时间：{fmtUtcToLocal(snapshot.captured_at)}（本地）
            </p>
          </>
        ) : null}
      </CardContent>
    </Card>
  );
}

function ProxyLine({
  label,
  status,
  readError,
  value,
}: {
  label: string;
  status: string;
  readError: string | null;
  value: string;
}) {
  return (
    <div className="flex items-start justify-between gap-2">
      <span className="shrink-0 text-muted-foreground">{label}</span>
      <span className="text-right">
        {status === 'unknown' ? (
          <span className="text-amber-500">未知{readError ? `（${readError}）` : ''}</span>
        ) : (
          value
        )}
      </span>
    </div>
  );
}

function describeUserProxy(snapshot: ProxySnapshot): string {
  const info = snapshot.user_settings;
  const parts: string[] = [];
  parts.push(
    info.manual_proxy
      ? `手动代理 ${info.manual_proxy}`
      : '手动代理未配置',
  );
  parts.push(info.pac_url ? 'PAC 已配置' : 'PAC 未配置');
  if (info.auto_detect !== null) {
    parts.push(info.auto_detect ? '自动检测开启' : '自动检测关闭');
  }
  return parts.join(' · ');
}

function describeWinHttp(snapshot: ProxySnapshot): string {
  const info = snapshot.winhttp;
  if (info.access_type === null) return '未知';
  if (info.access_type.startsWith('named_proxy')) {
    return `${info.access_type}：${info.proxy ?? '未知'}`;
  }
  return info.access_type;
}

function describeEnvVars(vars: ProxySnapshot['env_vars']): string | null {
  const present = vars.filter((item) => item.present);
  if (present.length === 0) return null;
  return present
    .map((item) => `${item.name}=${item.value_redacted ?? '[空]'}`)
    .join(' · ');
}
