import { describe, expect, it } from 'vitest';

import { aggregateSamples, non2xxResponseRatio, transportFailureRate } from '@/lib/metrics';
import type { DiagnosticError, ErrorKind, ProbeSample } from '@/lib/types';

/** 与 Rust 侧 D-011 指标拆分测试互为镜像，守护前后端口径一致。 */

function httpsSample(
  attemptNo: number,
  status: number | null,
  success: boolean,
  durationMs: number | null,
  error: DiagnosticError | null = null,
): ProbeSample {
  return {
    target_id: 'github',
    probe_type: 'https',
    route_mode: 'no_app_proxy',
    attempt_no: attemptNo,
    success,
    duration_ms: durationMs,
    http_status: status,
    http_outcome: status === 200 ? 'ok_2xx' : status === 401 ? 'auth_required_401_403' : null,
    error,
    tested_at: '2026-10-24T00:00:00Z',
  };
}

function transportError(kind: ErrorKind): DiagnosticError {
  return { kind, summary: '模拟传输失败', detail: null };
}

describe('aggregateSamples（D-011 口径）', () => {
  it('拆分传输失败与收到响应', () => {
    const samples = [
      httpsSample(1, 200, true, 100),
      httpsSample(2, 200, true, 200),
      httpsSample(3, 401, false, 300),
      httpsSample(4, null, false, null, transportError('tcp_failure')),
      httpsSample(5, null, false, null, transportError('timeout')),
    ];
    const aggregate = aggregateSamples('https', samples);

    expect(aggregate.attempts_total).toBe(5);
    expect(aggregate.success_count).toBe(2);
    expect(aggregate.http_response_count).toBe(3);
    expect(aggregate.transport_failure_count).toBe(2);
    expect(aggregate.request_failure_rate).toBeCloseTo(0.6, 9);
    expect(transportFailureRate(aggregate)).toBeCloseTo(0.4, 9);
    expect(non2xxResponseRatio(aggregate)).toBeCloseTo(1 / 3, 9);

    // 超时样本无耗时，不进入分布
    expect(aggregate.duration_min_ms).toBe(100);
    expect(aggregate.duration_median_ms).toBe(200);
    expect(aggregate.duration_max_ms).toBe(300);
  });

  it('偶数样本中位数取中间两值均值，空输入返回 0/None', () => {
    const even = aggregateSamples('https', [
      httpsSample(1, 200, true, 10),
      httpsSample(2, 200, true, 20),
      httpsSample(3, 200, true, 30),
      httpsSample(4, 200, true, 40),
    ]);
    expect(even.duration_median_ms).toBe(25);

    const empty = aggregateSamples('https', []);
    expect(empty.attempts_total).toBe(0);
    expect(empty.request_failure_rate).toBe(0);
    expect(empty.duration_min_ms).toBeNull();
    expect(non2xxResponseRatio(empty)).toBeNull();
  });
});
