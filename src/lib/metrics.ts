/**
 * 聚合计算（lib/metrics.ts）—— `ProbeAggregate::from_samples`（Rust）的前端镜像，
 * 供 Demo 数据与 Phase 2 真实结果的展示层复用。
 *
 * 口径遵循 D-011 + D-014：逐样本互斥分类；传输失败与非 2xx 响应分母不同，
 * 禁止合并；取消样本单独计数，不进入任何失败率的分母。
 */
import type { ProbeAggregate, ProbeSample, ProbeType } from './types';

export function aggregateSamples(probeType: ProbeType, samples: ProbeSample[]): ProbeAggregate {
  const attemptsTotal = samples.length;
  let cancelledCount = 0;
  let successCount = 0;
  let transportFailureCount = 0;
  let httpResponseCount = 0;
  const durations: number[] = [];

  for (const sample of samples) {
    if (sample.duration_ms !== null && sample.duration_ms !== undefined) {
      durations.push(sample.duration_ms);
    }
    if (sample.error !== null && sample.error !== undefined && sample.error.kind === 'cancelled') {
      cancelledCount += 1;
    } else if (sample.error !== null && sample.error !== undefined) {
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

  const executedTotal = attemptsTotal - cancelledCount;
  const requestFailureRate =
    executedTotal === 0 ? 0 : (executedTotal - successCount) / executedTotal;

  return {
    probe_type: probeType,
    attempts_total: attemptsTotal,
    cancelled_count: cancelledCount,
    success_count: successCount,
    transport_failure_count: transportFailureCount,
    http_response_count: httpResponseCount,
    request_failure_rate: requestFailureRate,
    duration_min_ms: min,
    duration_median_ms: median,
    duration_max_ms: max,
  };
}

/** 实际执行（未取消）的样本数——失败率口径的分母。 */
export function executedTotal(aggregate: ProbeAggregate): number {
  return aggregate.attempts_total - aggregate.cancelled_count;
}

/** 传输失败率 = transport_failure_count / executed_total（D-014：取消不入分母）。 */
export function transportFailureRate(aggregate: ProbeAggregate): number | null {
  const executed = executedTotal(aggregate);
  if (executed === 0) return null;
  return aggregate.transport_failure_count / executed;
}

/** 非 2xx 响应比例 = non_2xx / http_response_count（分母：收到响应的样本）。 */
export function non2xxResponseRatio(aggregate: ProbeAggregate): number | null {
  if (aggregate.http_response_count === 0) return null;
  return (aggregate.http_response_count - aggregate.success_count) / aggregate.http_response_count;
}
