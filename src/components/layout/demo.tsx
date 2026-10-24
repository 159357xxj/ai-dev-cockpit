import { FlaskConical } from 'lucide-react';

import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert';
import { Badge } from '@/components/ui/badge';
import { cn } from '@/lib/utils';

/** “演示数据”徽标：所有模拟数据必须可见地携带此标识（AGENTS.md / D-009）。 */
export function DemoBadge({ className }: { className?: string }) {
  return (
    <Badge variant="warning" className={cn('gap-1', className)}>
      <FlaskConical className="size-3" aria-hidden />
      演示数据
    </Badge>
  );
}

/** 演示模式横幅：进入演示页面时顶部常驻提示。 */
export function DemoBanner() {
  return (
    <Alert variant="warning" className="mb-4">
      <FlaskConical aria-hidden />
      <AlertTitle>演示模式已开启</AlertTitle>
      <AlertDescription>
        以下内容为明确标注的模拟数据，仅用于界面演示，不代表任何真实网络测量结果。可在「设置」中关闭。
      </AlertDescription>
    </Alert>
  );
}
