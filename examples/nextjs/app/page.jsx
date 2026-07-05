'use client';
import { useState } from 'react';

export default function Home() {
  const [count, setCount] = useState(0);
  return (
    <main style={{ fontFamily: 'sans-serif', maxWidth: 600, margin: '80px auto', textAlign: 'center' }}>
      <h1 style={{ color: '#6366f1' }}>Shipyard — Next.js Example</h1>
      <p>A minimal static site deployed via Shipyard.</p>
      <button
        onClick={() => setCount(c => c + 1)}
        style={{ marginTop: 16, padding: '10px 24px', background: '#6366f1', color: 'white', border: 'none', borderRadius: 8, fontSize: 16, cursor: 'pointer' }}
      >
        Clicked {count} times
      </button>
    </main>
  );
}
