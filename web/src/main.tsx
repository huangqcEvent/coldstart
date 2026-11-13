/**
 * React 应用启动入口。
 *
 * 挂载根组件并加载全局样式；不在此处执行项目 I/O 或 Tauri 命令。
 */
import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';

import App from './App';
import './styles.css';
import './theme.css';
import './delivery.css';
import { initializeTheme } from './theme';

initializeTheme();

const rootElement = document.getElementById('root');

if (!rootElement) {
  throw new Error('ColdStart root element is missing');
}

createRoot(rootElement).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
