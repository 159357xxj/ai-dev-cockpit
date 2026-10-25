/**
 * 演示趋势图（Recharts）。独立组件 + 默认导出，供 React.lazy 动态加载，
 * 把 recharts（约 400KB）从主包拆到按需 chunk（Phase 6 体积优化）。
 */
import {
  CartesianGrid,
  Line,
  LineChart,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from 'recharts';

import { MOCK_TREND } from '@/features/demo/mockData';

export default function TrendChart() {
  return (
    <div className="h-28" aria-hidden>
      <ResponsiveContainer width="100%" height="100%">
        <LineChart data={MOCK_TREND} margin={{ top: 4, right: 8, bottom: 0, left: -24 }}>
          <CartesianGrid strokeDasharray="3 3" stroke="var(--color-border)" />
          <XAxis dataKey="date" tick={{ fontSize: 10 }} stroke="var(--color-muted-foreground)" />
          <YAxis tick={{ fontSize: 10 }} stroke="var(--color-muted-foreground)" />
          <Tooltip />
          <Line
            type="monotone"
            dataKey="median_ms"
            stroke="var(--color-primary)"
            strokeWidth={2}
            dot
          />
        </LineChart>
      </ResponsiveContainer>
    </div>
  );
}
