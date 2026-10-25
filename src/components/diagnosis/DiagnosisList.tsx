import { useState } from 'react';
import { ChevronDown, FlaskConical, Lightbulb } from 'lucide-react';

import { Badge } from '@/components/ui/badge';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import type { Confidence, Diagnosis, Severity } from '@/lib/types';
import { cn } from '@/lib/utils';

/** 严重程度排序与展示（critical 最前）。 */
const SEVERITY_ORDER: Record<Severity, number> = { critical: 0, warning: 1, info: 2 };
const SEVERITY_LABELS: Record<Severity, string> = {
  critical: '阻断',
  warning: '需关注',
  info: '中性事实',
};
const CONFIDENCE_LABELS: Record<Confidence, string> = {
  evidence_sufficient: '证据充分',
  possible: '可能',
  insufficient: '证据不足',
};
const CONFIDENCE_STYLES: Record<Confidence, string> = {
  evidence_sufficient: 'border-emerald-500/40 bg-emerald-500/10 text-emerald-600 dark:text-emerald-400',
  possible: 'border-amber-500/40 bg-amber-500/10 text-amber-600 dark:text-amber-400',
  insufficient: 'border-border bg-muted text-muted-foreground',
};

/** 排序（严重度 → rule_id）+ 去重（同规则同目标只保留一条）。 */
export function sortAndDedup(diagnoses: Diagnosis[]): Diagnosis[] {
  const seen = new Set<string>();
  const unique: Diagnosis[] = [];
  for (const item of [...diagnoses].sort(
    (left, right) =>
      SEVERITY_ORDER[left.severity] - SEVERITY_ORDER[right.severity] ||
      left.rule_id.localeCompare(right.rule_id) ||
      left.related_target_ids.join().localeCompare(right.related_target_ids.join()),
  )) {
    const key = `${item.rule_id}|${item.related_target_ids.join(',')}`;
    if (seen.has(key)) continue;
    seen.add(key);
    unique.push(item);
  }
  return unique;
}

/** 诊断结论卡列表。空结果不渲染（不伪造诊断）。 */
export function DiagnosisList({
  diagnoses,
  isDemo = false,
  className,
}: {
  diagnoses: Diagnosis[];
  isDemo?: boolean;
  className?: string;
}) {
  const sorted = sortAndDedup(diagnoses);
  if (sorted.length === 0) return null;

  return (
    <div className={cn('space-y-3', className)}>
      <div className="flex items-center gap-2">
        <Lightbulb className="size-4 text-muted-foreground" aria-hidden />
        <p className="text-sm font-medium">
          诊断结论
          {isDemo ? '（演示数据）' : `（${sorted.length} 条，按严重程度排序）`}
        </p>
        {isDemo ? <DemoBadgeInline /> : null}
      </div>
      {sorted.map((diagnosis) => (
        <DiagnosisCard key={`${diagnosis.rule_id}|${diagnosis.related_target_ids.join(',')}`} diagnosis={diagnosis} />
      ))}
      <p className="text-xs text-muted-foreground">
        诊断基于规则引擎对测量结果的确定性分析，只描述已观测事实与可能解释；
        「可能」与「证据不足」的条目需要进一步测量佐证。
      </p>
    </div>
  );
}

function DemoBadgeInline() {
  return (
    <Badge variant="warning" className="gap-1">
      <FlaskConical className="size-3" aria-hidden />
      演示数据
    </Badge>
  );
}

function DiagnosisCard({ diagnosis }: { diagnosis: Diagnosis }) {
  const [expanded, setExpanded] = useState(false);
  const targets = diagnosis.related_target_ids.join('、');

  return (
    <Card className={cn('border-l-4', severityBorder(diagnosis.severity))}>
      <CardHeader className="pb-2">
        <div className="flex flex-wrap items-center gap-2">
          <Badge variant={diagnosis.severity === 'critical' ? 'destructive' : 'secondary'}>
            {SEVERITY_LABELS[diagnosis.severity]}
          </Badge>
          <CardTitle className="text-sm font-medium leading-snug">
            {diagnosis.title}
          </CardTitle>
          <Badge variant="outline" className={cn('ml-auto', CONFIDENCE_STYLES[diagnosis.confidence])}>
            {CONFIDENCE_LABELS[diagnosis.confidence]}
          </Badge>
        </div>
        <p className="text-xs text-muted-foreground">
          规则 {diagnosis.rule_id} · 涉及目标：{targets}
        </p>
      </CardHeader>
      <CardContent className="space-y-2 text-sm">
        <div>
          <p className="mb-1 text-xs font-medium text-muted-foreground">事实证据</p>
          <ul className="list-disc space-y-0.5 pl-5">
            {diagnosis.observed_facts.map((fact) => (
              <li key={fact}>{fact}</li>
            ))}
          </ul>
        </div>
        <details
          open={expanded}
          onToggle={(event) => setExpanded((event.target as HTMLDetailsElement).open)}
        >
          <summary className="flex cursor-pointer select-none items-center gap-1 text-xs text-muted-foreground hover:text-foreground">
            <ChevronDown
              className={cn('size-3.5 transition-transform', expanded && 'rotate-180')}
              aria-hidden
            />
            详细分析（可能原因 / 排查步骤 / 无法确定的事项）
          </summary>
          <div className="mt-2 space-y-2">
            <Section title="可能原因（基于事实的解释，非结论）" items={diagnosis.possible_causes} />
            <Section title="建议排查步骤" items={diagnosis.verification_steps} numbered />
            <Section title="无法确定的事项与局限" items={diagnosis.limitations} muted />
          </div>
        </details>
      </CardContent>
    </Card>
  );
}

function Section({
  title,
  items,
  numbered = false,
  muted = false,
}: {
  title: string;
  items: string[];
  numbered?: boolean;
  muted?: boolean;
}) {
  const ListTag = numbered ? 'ol' : 'ul';
  return (
    <div>
      <p className={cn('mb-1 text-xs font-medium', muted ? 'text-muted-foreground/70' : 'text-muted-foreground')}>
        {title}
      </p>
      <ListTag
        className={cn(
          'space-y-0.5 pl-5 text-xs',
          numbered ? 'list-decimal' : 'list-disc',
          muted && 'text-muted-foreground',
        )}
      >
        {items.map((item) => (
          <li key={item}>{item}</li>
        ))}
      </ListTag>
    </div>
  );
}

function severityBorder(severity: Severity): string {
  switch (severity) {
    case 'critical':
      return 'border-l-destructive';
    case 'warning':
      return 'border-l-amber-500';
    case 'info':
      return 'border-l-blue-400';
  }
}
