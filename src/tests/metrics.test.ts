import {
  aggregateSamples,
  executedTotal,
  non2xxResponseRatio,
  transportFailureRate,
} from '@/lib/metrics';
import type { DiagnosticError, ErrorKind, ProbeSample } from '@/lib/types';
import { describe, expect, it } from 'vitest';

/** 与 Rust 侧 D-011/D-014 聚合测试互为镜像，守护前后端口径一致。 */

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

describe('aggregateSamples（D-011 + D-014 口径）', () => {
  it('拆分传输失败与收到响应', () => {
    const samples = [
      httpsSample(1, 200, true, 100),
      httpsSample(2, 200, true, 200),
      httpsSample(3, 401, false, null),
      httpsSample(4, null, false, null, transportError('tcp_failure')),
      httpsSample(5, null, false, null, transportError('timeout')),
    ];
    const aggregate = aggregateSamples('https', samples);

    expect(aggregate.attempts_total).toBe(5);
    expect(aggregate.cancelled_count).toBe(0);
    expect(aggregate.success_count).toBe(2);
    expect(aggregate.http_response_count).toBe(3);
    expect(aggregate.transport_failure_count).toBe(2);
    expect(aggregate.request_failure_rate).toBeCloseTo(0.6, 9);
    expect(transportFailureRate(aggregate)).toBeCloseTo(0.4, 9);
    expect(non2xxResponseRatio(aggregate)).toBeCloseTo(1 / 3, 9);

    // 耗时分布只含成功样本（401/失败/超时均为 None）。
    expect(aggregate.duration_min_ms).toBe(100);
    expect(aggregate.duration_median_ms).toBe(150);
    expect(aggregate.duration_max_ms).toBe(200);
  });

  it('取消样本单独计数，不进入失败率分母（D-014）', () => {
    const samples = [
      httpsSample(1, 200, true, 100),
      httpsSample(2, null, false, null, transportError('tcp_failure')),
      httpsSample(3, null, false, null, transportError('cancelled')),
      httpsSample(4, null, false, null, transportError('cancelled')),
    ];
    const aggregate = aggregateSamples('https', samples);

    expect(aggregate.attempts_total).toBe(4);
    expect(aggregate.cancelled_count).toBe(2);
    expect(executedTotal(aggregate)).toBe(2);
    expect(aggregate.success_count).toBe(1);
    expect(aggregate.transport_failure_count).toBe(1);
    expect(transportFailureRate(aggregate)).toBeCloseTo(0.5, 9);
    expect(aggregate.request_failure_rate).toBeCloseTo(0.5, 9);
  });

  it('偶数样本中位数取中间两值均值，空/全取消输入不崩溃', () => {
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
    expect(transportFailureRate(empty)).toBeNull();

    const allCancelled = aggregateSamples('https', [
      httpsSample(1, null, false, null, transportError('cancelled')),
    ]);
    expect(allCancelled.cancelled_count).toBe(1);
    expect(transportFailureRate(allCancelled)).toBeNull();
    expect(allCancelled.request_failure_rate).toBe(0);
  });
});
