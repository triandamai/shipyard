export const metadata = { title: 'Shipyard — Next.js Example' };

export default function RootLayout({ children }) {
  return (
    <html lang="en">
      <body>{children}</body>
    </html>
  );
}
