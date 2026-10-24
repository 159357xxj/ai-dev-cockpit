import type { LucideIcon } from 'lucide-react';
import { AlertCircle, Loader2 } from 'lucide-react';
import type { ReactNode } from 'react';

import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert';
import type { IpcError } from '@/lib/types';

export function EmptyState({
  icon: Icon,
  title,
  description,
  children,
}: {
  icon: LucideIcon;
  title: string;
  description?: string;
  children?: ReactNode;
}) {
  return (
    <div className="flex flex-col items-center justify-center gap-2 rounded-lg border border-dashed border-border px-6 py-12 text-center">
      <Icon className="size-8 text-muted-foreground" aria-hidden />
      <p className="text-sm font-medium">{title}</p>
      {description ? (
        <p className="max-w-md text-xs text-muted-foreground">{description}</p>
      ) : null}
      {children}
    </div>
  );
}

export function ErrorState({
  title = '操作未完成',
  error,
  children,
}: {
  title?: string;
  error: IpcError | null;
  children?: ReactNode;
}) {
  return (
    <Alert variant="destructive">
      <AlertCircle aria-hidden />
      <AlertTitle>{title}</AlertTitle>
      <AlertDescription>
        <p>{error ? error.message : '发生未知错误。'}</p>
        {error?.detail ? <p className="mt-1 text-xs opacity-80">{error.detail}</p> : null}
        {children}
      </AlertDescription>
    </Alert>
  );
}

export function LoadingState({ label = '加载中…' }: { label?: string }) {
  return (
    <div className="flex items-center justify-center gap-2 rounded-lg border border-dashed border-border px-6 py-12 text-muted-foreground">
      <Loader2 className="size-4 animate-spin" aria-hidden />
      <span className="text-sm">{label}</span>
    </div>
  );
}
