/**
 * Demo 模式专用模拟数据（D-009）。
 *
 * 所有数据均为确定性构造的演示样本，必须配合 DemoBadge/DemoBanner 展示；
 * 严禁在任何非演示路径渲染这些数据，更不得让它们看起来像真实测量结果。
 */
import type {
  DiagnosticError,
  ProbeSample,
  RouteMode,
  SystemSnapshot,
  TargetResult,
} from '@/lib/types';
import { aggregateSamples } from '@/lib/metrics';

const DEMO_TIME = '2026-10-24T08:30:00+00:00';

interface SampleOverrides {
  status?: number;
  duration?: number | null;
  error?: DiagnosticError;
  success?: boolean;
}

function httpsSample(
  targetId: string,
  attemptNo: number,
  routeMode: RouteMode,
  overrides: SampleOverrides = {},
): ProbeSample {
  const status = overrides.status ?? 200;
  const success = overrides.success ?? (status >= 200 && status < 300);
  return {
    target_id: targetId,
    probe_type: 'https',
    route_mode: routeMode,
    attempt_no: attemptNo,
    success,
    duration_ms: overrides.duration !== undefined ? overrides.duration : 800 + attemptNo * 37,
    http_status: overrides.error ? null : status,
    http_outcome: overrides.error
      ? null
      : status >= 200 && status < 300
        ? 'ok_2xx'
        : status === 401 || status === 403
          ? 'auth_required_401_403'
          : null,
    error: overrides.error ?? null,
    tested_at: DEMO_TIME,
  };
}

function timeoutError(): DiagnosticError {
  return { kind: 'timeout', summary: '在限定时间内未完成本次测量（演示）', detail: null };
}

function proxyUnavailableError(): DiagnosticError {
  return {
    kind: 'proxy_unavailable',
    summary: '指定的本地代理不可用，未产生直连结果（演示）',
    detail: null,
  };
}

/** 确定性演示样本：github 全成功；openai-api 含一次超时；claude 含一次 401。 */
function demoSamplesForTarget(targetId: string, repeats: number, routeMode: RouteMode): ProbeSample[] {
  const samples: ProbeSample[] = [];
  for (let attempt = 1; attempt <= repeats; attempt += 1) {
    if (targetId === 'openai-api' && attempt === 2) {
      samples.push(httpsSample(targetId, attempt, routeMode, { error: timeoutError(), duration: null, success: false }));
    } else if (targetId === 'claude' && attempt === 1) {
      samples.push(httpsSample(targetId, attempt, routeMode, { status: 401, success: false }));
    } else if (targetId === 'zcode') {
      samples.push(httpsSample(targetId, attempt, routeMode, { duration: 1500 + attempt * 80 }));
    } else {
      samples.push(httpsSample(targetId, attempt, routeMode));
    }
  }
  // 手动代理模式演示“指定代理不可用且绝不回落”。
  if (routeMode === 'manual_local_proxy' && targetId === 'github') {
    samples[0] = httpsSample(targetId, 1, routeMode, {
      error: proxyUnavailableError(),
      duration: null,
      success: false,
    });
  }
  return samples;
}

export function buildDemoTargets(targetIds: string[], repeats: number, routeMode: RouteMode): TargetResult[] {
  return targetIds.map((targetId) => {
    const samples = demoSamplesForTarget(targetId, repeats, routeMode);
    return {
      target_id: targetId,
      samples,
      aggregates: [aggregateSamples('https', samples)],
    };
  });
}

/** 比较页演示数据：同一目标在三种模式下的样本。 */
export function buildDemoCompare(): Array<{ route_mode: RouteMode; samples: ProbeSample[] }> {
  return [
    { route_mode: 'no_app_proxy', samples: demoSamplesForTarget('github', 3, 'no_app_proxy') },
    { route_mode: 'library_default', samples: demoSamplesForTarget('github', 3, 'library_default') },
    { route_mode: 'manual_local_proxy', samples: demoSamplesForTarget('github', 3, 'manual_local_proxy') },
  ];
}

export const MOCK_SYSTEM_SNAPSHOT: SystemSnapshot = {
  os_version: 'Windows 11 23H2（演示值）',
  arch: 'x86_64',
  app_version: '0.1.0',
  adapters: [
    { name: '以太网（演示）', adapter_type: 'ethernet', status_up: true, ipv4: '192.0.2.10' },
  ],
  dns_servers: ['192.0.2.53（演示）'],
  captured_at: DEMO_TIME,
};

export const MOCK_RECENT_RUN = {
  run_id: 'demo-run-1',
  started_at: DEMO_TIME,
  status: 'completed' as const,
  route_mode: 'no_app_proxy' as RouteMode,
  target_count: 3,
  median_ms: 918,
};

/** 总览迷你趋势图：近 5 次演示运行的 HTTPS 中位耗时（ms）。 */
export const MOCK_TREND = [
  { date: '10-20', median_ms: 1040 },
  { date: '10-21', median_ms: 980 },
  { date: '10-22', median_ms: 1120 },
  { date: '10-23', median_ms: 870 },
  { date: '10-24', median_ms: 918 },
];

export const MOCK_HISTORY_RUNS = [
  { run_id: 'demo-run-5', started_at: '2026-10-24T08:30:00+00:00', targets: 'github + 2', route_mode: 'no_app_proxy', status: 'completed', samples: 9, median_ms: 918 },
  { run_id: 'demo-run-4', started_at: '2026-10-23T21:12:00+00:00', targets: 'openai-api', route_mode: 'manual_local_proxy', status: 'completed', samples: 3, median_ms: 1180 },
  { run_id: 'demo-run-3', started_at: '2026-10-23T09:40:00+00:00', targets: 'claude', route_mode: 'library_default', status: 'completed', samples: 3, median_ms: 1020 },
  { run_id: 'demo-run-2', started_at: '2026-10-22T19:05:00+00:00', targets: 'chatgpt', route_mode: 'no_app_proxy', status: 'cancelled', samples: 2, median_ms: null },
  { run_id: 'demo-run-1', started_at: '2026-10-22T08:00:00+00:00', targets: 'zcode', route_mode: 'no_app_proxy', status: 'completed', samples: 3, median_ms: 1640 },
];
