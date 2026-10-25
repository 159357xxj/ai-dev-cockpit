/**
 * 诊断结论卡组件测试：排序、去重、置信徽标、空结果不渲染、可展开细节。
 */
import { cleanup, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it } from 'vitest';

import { DiagnosisList, sortAndDedup } from '@/components/diagnosis/DiagnosisList';
import type { Diagnosis } from '@/lib/types';

function diagnosis(partial: Partial<Diagnosis> & { rule_id: string }): Diagnosis {
  return {
    title: `规则 ${partial.rule_id} 的核心发现`,
    severity: 'warning',
    confidence: 'possible',
    observed_facts: ['已观测事实 A', '已观测事实 B'],
    possible_causes: ['可能原因 1', '可能原因 2'],
    verification_steps: ['步骤 1', '步骤 2'],
    limitations: ['不能推断 X'],
    related_target_ids: ['github'],
    related_probe_types: ['https'],
    ...partial,
  };
}

afterEach(() => {
  cleanup();
});

describe('DiagnosisList', () => {
  it('空结果不渲染任何诊断（不伪造）', () => {
    render(<DiagnosisList diagnoses={[]} />);
    expect(screen.queryByText(/诊断结论/)).not.toBeInTheDocument();
  });

  it('按严重程度排序：阻断在前，中性事实在后', () => {
    const items = [
      diagnosis({ rule_id: 'R05', severity: 'info' }),
      diagnosis({ rule_id: 'R09', severity: 'critical' }),
      diagnosis({ rule_id: 'R03', severity: 'warning' }),
    ];
    render(<DiagnosisList diagnoses={items} />);
    const badges = screen.getAllByText(/^(阻断|需关注|中性事实)$/);
    expect(badges.map((badge) => badge.textContent)).toEqual([
      '阻断',
      '需关注',
      '中性事实',
    ]);
  });

  it('同规则同目标去重', () => {
    const items = [
      diagnosis({ rule_id: 'R03' }),
      diagnosis({ rule_id: 'R03' }),
      diagnosis({ rule_id: 'R04', related_target_ids: ['github'] }),
    ];
    const deduped = sortAndDedup(items);
    expect(deduped).toHaveLength(2);
  });

  it('展示证据充分程度徽标（中文）与规则编号', () => {
    render(
      <DiagnosisList
        diagnoses={[diagnosis({ rule_id: 'R03', confidence: 'evidence_sufficient' })]}
      />,
    );
    expect(screen.getByText('证据充分')).toBeInTheDocument();
    expect(screen.getAllByText(/规则 R03/).length).toBeGreaterThan(0);
  });

  it('事实证据直接可见，详细分析可展开（可能原因/步骤/局限）', async () => {
    const user = userEvent.setup();
    render(<DiagnosisList diagnoses={[diagnosis({ rule_id: 'R03' })]} />);
    expect(screen.getByText('已观测事实 A')).toBeInTheDocument();
    // 展开前：原因不可见
    expect(screen.getByText('可能原因 1').closest('details')).not.toBeVisible();
    await user.click(screen.getByText(/详细分析/));
    expect(screen.getByText('可能原因 1')).toBeVisible();
    expect(screen.getByText('步骤 1')).toBeVisible();
    expect(screen.getByText('不能推断 X')).toBeVisible();
  });

  it('Demo 模式带演示数据标识', () => {
    render(<DiagnosisList diagnoses={[diagnosis({ rule_id: 'R03' })]} isDemo />);
    expect(screen.getAllByText(/演示数据/).length).toBeGreaterThan(0);
  });
});
