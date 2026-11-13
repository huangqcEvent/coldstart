import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';

import App from './App';
import './styles.css';
import './theme.css';
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
