'use client';

import React, { createContext, useContext, useSyncExternalStore } from 'react';

type ThemeMode = 'dark' | 'light';

function applyTheme(t: ThemeMode) {
  const root = document.documentElement;
  const desktopBridge = (window as any).synevidDesktop;
  if (desktopBridge?.markNextPresentedFrame) {
    desktopBridge.markNextPresentedFrame();
  }
  const mutationStartedAt = performance.now();
  root.setAttribute('data-theme-switching', 'true');
  root.classList.remove('dark', 'light');
  root.classList.add(t);
  root.style.colorScheme = t;
  root.setAttribute('data-theme', t);
  const mutationMs = performance.now() - mutationStartedAt;
  root.setAttribute('data-theme-mutation-ms', mutationMs.toFixed(3));
  window.dispatchEvent(new CustomEvent('synevid-theme-change', { detail: { mutationMs } }));
  window.requestAnimationFrame(() => root.removeAttribute('data-theme-switching'));
}

function getThemeSnapshot(): ThemeMode {
  if (typeof document === 'undefined') return 'dark';
  return document.documentElement.getAttribute('data-theme') === 'light' ? 'light' : 'dark';
}

function subscribeToTheme(onChange: () => void) {
  const onStorage = (event: StorageEvent) => {
    if (event.key !== 'synevid_theme' && event.key !== null) return;
    try {
      const stored = localStorage.getItem('synevid_theme');
      if (stored === 'light' || stored === 'dark') applyTheme(stored);
    } catch {}
    onChange();
  };
  window.addEventListener('synevid-theme-change', onChange);
  window.addEventListener('storage', onStorage);
  return () => {
    window.removeEventListener('synevid-theme-change', onChange);
    window.removeEventListener('storage', onStorage);
  };
}

function getServerThemeSnapshot(): ThemeMode {
  return 'dark';
}

interface ThemeContextType {
  theme: ThemeMode;
  toggleTheme: () => void;
  setTheme: (theme: ThemeMode) => void;
}

const ThemeContext = createContext<ThemeContextType>({
  theme: 'dark',
  toggleTheme: () => {},
  setTheme: () => {},
});

export function ThemeProvider({ children }: { children: React.ReactNode }) {
  const theme = useSyncExternalStore(subscribeToTheme, getThemeSnapshot, getServerThemeSnapshot);

  const setTheme = (t: ThemeMode) => {
    try {
      localStorage.setItem('synevid_theme', t);
    } catch {}
    applyTheme(t);
  };

  const toggleTheme = () => {
    const next = theme === 'dark' ? 'light' : 'dark';
    setTheme(next);
  };

  return (
    <ThemeContext.Provider value={{ theme, toggleTheme, setTheme }}>
      {children}
    </ThemeContext.Provider>
  );
}

export function useTheme() {
  return useContext(ThemeContext);
}
