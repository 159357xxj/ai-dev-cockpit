/**
 * 聚合计算（lib/metrics.ts）—— `ProbeAggregate::from_samples`（Rust）的前端镜像，
 * 供 Demo 数据与 Phase 2 真实结果的展示层复用。
 *
 * 口径遵循 D-011：逐样本互斥分类，传输失败与非 2xx 响应分母不同，禁止合并。
 */
import type { ProbeAggregate, ProbeSample, ProbeType } from './types';

export function aggregateSamples(probeType: ProbeType, samples: ProbeSample[]): ProbeAggregate {
  const attemptsTotal = samples.length;
  let successCount = 0;
  let transportFailureCount = 0;
  let httpResponseCount = 0;
  const durations: number[] = [];

  for (const sample of samples) {
    if (sample.duration_ms !== null && sample.duration_ms !== undefined) {
      durations.push(sample.duration_ms);
    }
    if (sample.error !== null && sample.error !== undefined) {
      transportFailureCount += 1;
    } else if (sample.http_status !== null && sample.http_status !== undefined) {
      httpResponseCount += 1;
      if (sample.success) successCount += 1;
    } else if (sample.success) {
      successCount += 1;
    } else {
      transportFailureCount += 1;
    }
  }

  durations.sort((a, b) => a - b);
  let min: number | null = null;
  let median: number | null = null;
  let max: number | null = null;
  if (durations.length > 0) {
    min = durations[0];
    max = durations[durations.length - 1];
    const mid = Math.floor(durations.length / 2);
    median =
      durations.length % 2 === 1
        ? durations[mid]
        : Math.floor((durations[mid - 1] + durations[mid]) / 2);
  }

  const requestFailureRate =
    attemptsTotal === 0 ? 0 : (attemptsTotal - successCount) / attemptsTotal;

  return {
    probe_type: probeType,
    attempts_total: attemptsTotal,
    success_count: successCount,
    transport_failure_count: transportFailureCount,
    http_response_count: httpResponseCount,
    request_failure_rate: requestFailureRate,
    duration_min_ms: min,
    duration_median_ms: median,
    duration_max_ms: max,
  };
}

/** 传输失败率 = transport_failure_count / attempts_total（分母：全部样本）。 */
export function transportFailureRate(aggregate: ProbeAggregate): number {
  if (aggregate.attempts_total === 0) return 0;
  return aggregate.transport_failure_count / aggregate.attempts_total;
}

/** 非 2xx 响应比例 = non_2xx / http_response_count（分母：收到响应的样本）。 */
export function non2xxResponseRatio(aggregate: ProbeAggregate): number | null {
  if (aggregate.http_response_count === 0) return null;
  return (aggregate.http_response_count - aggregate.success_count) / aggregate.http_response_count;
}
