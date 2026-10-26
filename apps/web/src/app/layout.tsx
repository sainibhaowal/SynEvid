import type { Metadata } from 'next';
import { Michroma, Orbitron, Syncopate, Space_Grotesk, Geist_Mono } from 'next/font/google';
import './globals.css';

const michroma = Michroma({
  subsets: ['latin'],
  variable: '--font-michroma',
  weight: ['400'],
  display: 'swap',
});

const orbitron = Orbitron({
  subsets: ['latin'],
  variable: '--font-orbitron',
  weight: ['400', '600', '700', '800', '900'],
  display: 'swap',
});

const syncopate = Syncopate({
  subsets: ['latin'],
  variable: '--font-syncopate',
  weight: ['400', '700'],
  display: 'swap',
});

const spaceGrotesk = Space_Grotesk({
  subsets: ['latin'],
  variable: '--font-space',
  weight: ['400', '500', '600', '700'],
  display: 'swap',
});

const geistMono = Geist_Mono({
  subsets: ['latin'],
  variable: '--font-mono',
  weight: ['400', '500', '600', '700'],
  display: 'swap',
});

export const metadata: Metadata = {
  title: 'SynEvid — Evidence Is All You Need | Deterministic Verification Engine',
  description:
    'SynEvid (Code Autopsy) is an offline, 100% deterministic software verification and change-impact engine. Offline ground-truth verification for autonomous AI coding agents and safety-critical systems.',
  icons: {
    icon: '/assets/logo_icon.png',
    shortcut: '/assets/logo_icon.png',
    apple: '/assets/logo_icon.png',
  },
};

export default function RootLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return (
    <html
      lang="en"
      className={`dark scroll-smooth ${michroma.variable} ${orbitron.variable} ${syncopate.variable} ${spaceGrotesk.variable} ${geistMono.variable}`}
    >
      <body className="min-h-screen bg-[#05080e] text-[#f0f4fc] font-sans antialiased overflow-x-hidden selection:bg-cyan-400 selection:text-black">
        {children}
      </body>
    </html>
  );
}
