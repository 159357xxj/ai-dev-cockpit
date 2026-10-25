import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from 'react';

/** 应用设置：主题 + 演示模式。持久化于 localStorage；正式构建默认关闭演示模式（D-009）。 */

export type ThemeSetting = 'dark' | 'light' | 'system';

export interface AppSettings {
  theme: ThemeSetting;
  demoMode: boolean;
}

const STORAGE_KEY = 'adc.settings.v1';
const DEFAULT_SETTINGS: AppSettings = { theme: 'dark', demoMode: false };

/** Demo 仅在开发构建可用；正式构建强制关闭且不可开启（D-009 / Phase 6 加固）。 */
export const DEMO_AVAILABLE: boolean = import.meta.env.DEV;

function loadSettings(): AppSettings {
  try {
    const raw = window.localStorage.getItem(STORAGE_KEY);
    if (!raw) return DEFAULT_SETTINGS;
    const parsed: unknown = JSON.parse(raw);
    if (!parsed || typeof parsed !== 'object') return DEFAULT_SETTINGS;
    const record = parsed as Record<string, unknown>;
    const theme =
      record.theme === 'dark' || record.theme === 'light' || record.theme === 'system'
        ? record.theme
        : DEFAULT_SETTINGS.theme;
    // 正式构建忽略持久化的 demoMode（即使开发期残留也会被硬关）。
    const demoMode = DEMO_AVAILABLE && record.demoMode === true;
    return { theme, demoMode };
  } catch {
    return DEFAULT_SETTINGS;
  }
}

function resolveTheme(theme: ThemeSetting): 'dark' | 'light' {
  if (theme !== 'system') return theme;
  if (typeof window.matchMedia !== 'function') return 'dark';
  return window.matchMedia('(prefers-color-scheme: light)').matches ? 'light' : 'dark';
}

interface SettingsContextValue {
  settings: AppSettings;
  resolvedTheme: 'dark' | 'light';
  setTheme: (theme: ThemeSetting) => void;
  setDemoMode: (demoMode: boolean) => void;
}

const SettingsContext = createContext<SettingsContextValue | null>(null);

export function SettingsProvider({ children }: { children: ReactNode }) {
  const [settings, setSettings] = useState<AppSettings>(loadSettings);

  useEffect(() => {
    const resolved = resolveTheme(settings.theme);
    document.documentElement.classList.toggle('dark', resolved === 'dark');
    try {
      window.localStorage.setItem(STORAGE_KEY, JSON.stringify(settings));
    } catch {
      // localStorage 不可用时仅影响持久化，不阻断运行。
    }
  }, [settings]);

  useEffect(() => {
    if (settings.theme !== 'system' || typeof window.matchMedia !== 'function') return;
    const media = window.matchMedia('(prefers-color-scheme: light)');
    const onChange = (): void => {
      document.documentElement.classList.toggle('dark', !media.matches);
    };
    media.addEventListener('change', onChange);
    return () => media.removeEventListener('change', onChange);
  }, [settings.theme]);

  const setTheme = useCallback((theme: ThemeSetting) => {
    setSettings((previous) => ({ ...previous, theme }));
  }, []);
  const setDemoMode = useCallback((demoMode: boolean) => {
    if (!DEMO_AVAILABLE) return;
    setSettings((previous) => ({ ...previous, demoMode }));
  }, []);

  const resolvedTheme = resolveTheme(settings.theme);
  const value = useMemo<SettingsContextValue>(
    () => ({ settings, resolvedTheme, setTheme, setDemoMode }),
    [settings, resolvedTheme, setTheme, setDemoMode],
  );

  return <SettingsContext.Provider value={value}>{children}</SettingsContext.Provider>;
}

export function useAppSettings(): SettingsContextValue {
  const context = useContext(SettingsContext);
  if (!context) {
    throw new Error('useAppSettings 必须在 SettingsProvider 内使用');
  }
  return context;
}
