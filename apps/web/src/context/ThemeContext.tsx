'use client';

import React, { createContext, useContext, useEffect, useState } from 'react';

type ThemeMode = 'dark' | 'light';

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
  const [theme, setThemeState] = useState<ThemeMode>('dark');
  const [mounted, setMounted] = useState(false);

  useEffect(() => {
    try {
      const isHtmlLight = document.documentElement.classList.contains('light');
      const saved = localStorage.getItem('synevid_theme') as ThemeMode;
      const initial: ThemeMode = saved === 'light' || isHtmlLight ? 'light' : 'dark';
      setThemeState(initial);
      applyTheme(initial);
    } catch (e) {
      applyTheme('dark');
    }
    setMounted(true);
  }, []);

  const applyTheme = (t: ThemeMode) => {
    const root = document.documentElement;
    root.classList.remove('dark', 'light');
    root.classList.add(t);
    root.style.colorScheme = t;
    root.setAttribute('data-theme', t);
  };

  const setTheme = (t: ThemeMode) => {
    setThemeState(t);
    try {
      localStorage.setItem('synevid_theme', t);
    } catch (e) {}
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
