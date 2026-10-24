import React from 'react';
import ReactDOM from 'react-dom/client';
import { RouterProvider } from 'react-router';

import '@/index.css';
import { AppErrorBoundary } from '@/app/ErrorBoundary';
import { createAppRouter } from '@/app/router';
import { SettingsProvider } from '@/app/settings';

ReactDOM.createRoot(document.getElementById('root') as HTMLElement).render(
  <React.StrictMode>
    <AppErrorBoundary>
      <SettingsProvider>
        <RouterProvider router={createAppRouter('browser')} />
      </SettingsProvider>
    </AppErrorBoundary>
  </React.StrictMode>,
);
