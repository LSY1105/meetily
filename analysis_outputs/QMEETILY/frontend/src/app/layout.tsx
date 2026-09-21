import type { Metadata } from "next";
import "./globals.css";

export const metadata: Metadata = {
  title: "QMeetily",
  description: "Privacy-first local meeting assistant powered by Qwen3",
};

export default function RootLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return (
    <html lang="en">
      <body className="antialiased">{children}</body>
    </html>
  );
}
