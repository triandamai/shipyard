import { Elysia, t } from 'elysia';
import { swagger } from '@elysiajs/swagger';
import { cors } from '@elysiajs/cors';

// Simple in-memory database for demonstration
interface Todo {
  id: string;
  title: string;
  completed: boolean;
  createdAt: string;
}

const todos: Todo[] = [
  {
    id: '1',
    title: 'Explore Elysia.js performance',
    completed: true,
    createdAt: new Date().toISOString(),
  },
  {
    id: '2',
    title: 'Containerize with Docker',
    completed: false,
    createdAt: new Date().toISOString(),
  },
  {
    id: '3',
    title: 'Push to Registry using GitHub Actions',
    completed: false,
    createdAt: new Date().toISOString(),
  },
];

// Track request statistics
const stats = {
  requestCount: 0,
  startTime: Date.now(),
};

const app = new Elysia()
  // Global CORS setup
  .use(cors())
  // Swagger documentation
  .use(
    swagger({
      documentation: {
        info: {
          title: 'Elysia Shipyard API',
          version: '1.0.0',
          description: 'A blazing fast API example using Elysia.js and Bun',
        },
        tags: [
          { name: 'System', description: 'System health and diagnostics' },
          { name: 'Todos', description: 'Task management endpoints' },
        ],
      },
      path: '/swagger',
    })
  )
  // Request logger middleware
  .onRequest(({ request }) => {
    stats.requestCount++;
    console.log(`[${new Date().toISOString()}] ${request.method} ${request.url}`);
  })
  // Visual Dashboard
  .get('/', () => {
    const uptimeSeconds = Math.floor((Date.now() - stats.startTime) / 1000);
    const html = `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>Elysia.js Shipyard Example</title>
  <link rel="preconnect" href="https://fonts.googleapis.com">
  <link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
  <link href="https://fonts.googleapis.com/css2?family=Outfit:wght@300;400;500;600;700&family=Plus+Jakarta+Sans:wght@300;400;500;600;700&display=swap" rel="stylesheet">
  <style>
    :root {
      --bg-dark: #090a0f;
      --card-bg: rgba(17, 19, 31, 0.75);
      --border-color: rgba(255, 255, 255, 0.08);
      --accent-primary: #8b5cf6;
      --accent-secondary: #06b6d4;
      --accent-glow: rgba(139, 92, 246, 0.15);
      --text-main: #f3f4f6;
      --text-muted: #9ca3af;
      --success: #10b981;
    }

    * {
      box-sizing: border-box;
      margin: 0;
      padding: 0;
    }

    body {
      font-family: 'Plus Jakarta Sans', 'Outfit', sans-serif;
      background-color: var(--bg-dark);
      color: var(--text-main);
      min-height: 100vh;
      display: flex;
      flex-direction: column;
      align-items: center;
      justify-content: center;
      padding: 2rem 1rem;
      background-image: 
        radial-gradient(circle at 10% 20%, rgba(139, 92, 246, 0.15) 0%, transparent 40%),
        radial-gradient(circle at 90% 80%, rgba(6, 118, 212, 0.15) 0%, transparent 40%);
      background-attachment: fixed;
    }

    .container {
      width: 100%;
      max-width: 800px;
      z-index: 10;
    }

    .header {
      text-align: center;
      margin-bottom: 2.5rem;
      animation: fadeInDown 0.8s ease-out;
    }

    .logo-container {
      display: inline-flex;
      align-items: center;
      gap: 0.75rem;
      margin-bottom: 1rem;
    }

    .logo-badge {
      background: linear-gradient(135deg, var(--accent-primary), var(--accent-secondary));
      padding: 0.5rem 1rem;
      border-radius: 9999px;
      font-size: 0.85rem;
      font-weight: 700;
      text-transform: uppercase;
      letter-spacing: 0.05em;
      box-shadow: 0 0 20px rgba(139, 92, 246, 0.4);
    }

    h1 {
      font-size: 2.5rem;
      font-weight: 700;
      font-family: 'Outfit', sans-serif;
      background: linear-gradient(to right, #ffffff, #d1d5db);
      -webkit-background-clip: text;
      -webkit-text-fill-color: transparent;
      margin-bottom: 0.5rem;
    }

    .subtitle {
      color: var(--text-muted);
      font-size: 1.1rem;
      font-weight: 400;
    }

    .card {
      background: var(--card-bg);
      backdrop-filter: blur(16px);
      -webkit-backdrop-filter: blur(16px);
      border: 1px solid var(--border-color);
      border-radius: 24px;
      padding: 2.5rem;
      box-shadow: 0 20px 40px rgba(0, 0, 0, 0.3);
      margin-bottom: 2rem;
      animation: fadeInUp 0.8s ease-out;
    }

    .grid {
      display: grid;
      grid-template-columns: repeat(2, 1fr);
      gap: 1.5rem;
      margin-bottom: 2.5rem;
    }

    @media (max-width: 600px) {
      .grid {
        grid-template-columns: 1fr;
      }
    }

    .stat-box {
      background: rgba(255, 255, 255, 0.02);
      border: 1px solid rgba(255, 255, 255, 0.04);
      border-radius: 16px;
      padding: 1.25rem;
      transition: all 0.3s ease;
    }

    .stat-box:hover {
      transform: translateY(-2px);
      border-color: rgba(139, 92, 246, 0.3);
      background: rgba(139, 92, 246, 0.02);
    }

    .stat-label {
      font-size: 0.85rem;
      color: var(--text-muted);
      text-transform: uppercase;
      letter-spacing: 0.05em;
      margin-bottom: 0.5rem;
    }

    .stat-value {
      font-size: 1.5rem;
      font-weight: 600;
      color: #ffffff;
      font-family: 'Outfit', sans-serif;
    }

    .interactive-section {
      border-top: 1px solid var(--border-color);
      padding-top: 2rem;
    }

    h2 {
      font-size: 1.4rem;
      margin-bottom: 1.25rem;
      font-family: 'Outfit', sans-serif;
      font-weight: 600;
      display: flex;
      align-items: center;
      gap: 0.5rem;
    }

    .btn-group {
      display: flex;
      flex-wrap: wrap;
      gap: 1rem;
      margin-bottom: 1.5rem;
    }

    .btn {
      display: inline-flex;
      align-items: center;
      justify-content: center;
      padding: 0.75rem 1.5rem;
      border-radius: 12px;
      font-weight: 600;
      font-size: 0.95rem;
      text-decoration: none;
      cursor: pointer;
      transition: all 0.2s ease;
      border: none;
    }

    .btn-primary {
      background: linear-gradient(135deg, var(--accent-primary), #7c3aed);
      color: white;
      box-shadow: 0 4px 15px rgba(139, 92, 246, 0.3);
    }

    .btn-primary:hover {
      transform: translateY(-1px);
      box-shadow: 0 6px 20px rgba(139, 92, 246, 0.4);
    }

    .btn-secondary {
      background: rgba(255, 255, 255, 0.06);
      color: var(--text-main);
      border: 1px solid rgba(255, 255, 255, 0.1);
    }

    .btn-secondary:hover {
      background: rgba(255, 255, 255, 0.1);
      transform: translateY(-1px);
    }

    .console-output {
      background: rgba(5, 5, 10, 0.8);
      border: 1px solid rgba(255, 255, 255, 0.05);
      border-radius: 12px;
      padding: 1.25rem;
      font-family: 'Courier New', Courier, monospace;
      font-size: 0.9rem;
      color: #38bdf8;
      overflow-x: auto;
      max-height: 200px;
      white-space: pre-wrap;
    }

    .footer {
      text-align: center;
      color: var(--text-muted);
      font-size: 0.85rem;
      animation: fadeIn 1.2s ease-out;
    }

    .footer a {
      color: var(--accent-secondary);
      text-decoration: none;
    }

    .footer a:hover {
      text-decoration: underline;
    }

    .badge-live {
      display: inline-flex;
      align-items: center;
      gap: 0.35rem;
      color: var(--success);
      font-size: 0.85rem;
      font-weight: 600;
    }

    .pulse {
      width: 8px;
      height: 8px;
      background-color: var(--success);
      border-radius: 50%;
      box-shadow: 0 0 0 0 rgba(16, 185, 129, 0.7);
      animation: pulse-animation 1.5s infinite;
    }

    @keyframes pulse-animation {
      0% {
        transform: scale(0.95);
        box-shadow: 0 0 0 0 rgba(16, 185, 129, 0.7);
      }
      70% {
        transform: scale(1);
        box-shadow: 0 0 0 8px rgba(16, 185, 129, 0);
      }
      100% {
        transform: scale(0.95);
        box-shadow: 0 0 0 0 rgba(16, 185, 129, 0);
      }
    }

    @keyframes fadeInDown {
      from {
        opacity: 0;
        transform: translateY(-20px);
      }
      to {
        opacity: 1;
        transform: translateY(0);
      }
    }

    @keyframes fadeInUp {
      from {
        opacity: 0;
        transform: translateY(20px);
      }
      to {
        opacity: 1;
        transform: translateY(0);
      }
    }

    @keyframes fadeIn {
      from { opacity: 0; }
      to { opacity: 1; }
    }
  </style>
</head>
<body>
  <div class="container">
    <header class="header">
      <div class="logo-container">
        <span class="logo-badge">Elysia.js</span>
        <div class="badge-live">
          <span class="pulse"></span>
          <span>Online</span>
        </div>
      </div>
      <h1>Shipyard Example Server</h1>
      <p class="subtitle">High-performance Bun API server containerized and ready</p>
    </header>

    <main class="card">
      <div class="grid">
        <div class="stat-box">
          <div class="stat-label">Runtime Engine</div>
          <div class="stat-value" style="color: #ff8a65;">Bun ${Bun.version}</div>
        </div>
        <div class="stat-box">
          <div class="stat-label">Framework</div>
          <div class="stat-value" style="color: #a78bfa;">Elysia v1.1</div>
        </div>
        <div class="stat-box">
          <div class="stat-label">Server Uptime</div>
          <div class="stat-value" id="uptime">${uptimeSeconds}s</div>
        </div>
        <div class="stat-box">
          <div class="stat-label">Total Requests</div>
          <div class="stat-value" id="requests">${stats.requestCount}</div>
        </div>
      </div>

      <div class="interactive-section">
        <h2>Interactive API Testing</h2>
        <p style="color: var(--text-muted); margin-bottom: 1.25rem; font-size: 0.95rem;">
          Use the quick-action triggers below to query endpoints on this server and view the live response payload:
        </p>
        
        <div class="btn-group">
          <button class="btn btn-primary" onclick="testEndpoint('/api/todos')">List Todos (GET)</button>
          <button class="btn btn-primary" onclick="createTodo()">Add Todo (POST)</button>
          <button class="btn btn-secondary" onclick="testEndpoint('/api/health')">Health Check (GET)</button>
          <a class="btn btn-secondary" href="/swagger" target="_blank">Swagger Documentation ↗</a>
        </div>

        <div class="stat-label" style="margin-bottom: 0.5rem;">Response Payload</div>
        <div class="console-output" id="output">Click a button above to run a request...</div>
      </div>
    </main>

    <footer class="footer">
      <p>Created as part of the Shipyard ecosystem. Deploy instantly with Docker.</p>
    </footer>
  </div>

  <script>
    let uptime = ${uptimeSeconds};
    setInterval(() => {
      uptime++;
      document.getElementById('uptime').innerText = uptime + 's';
    }, 1000);

    async function testEndpoint(path, options = {}) {
      const output = document.getElementById('output');
      output.innerText = 'Connecting...';
      try {
        const response = await fetch(path, options);
        const data = await response.json();
        
        // Refresh total request count
        const reqCount = document.getElementById('requests');
        if (reqCount) {
          reqCount.innerText = parseInt(reqCount.innerText) + 1;
        }

        output.innerText = '// ' + response.status + ' ' + response.statusText + '\\n' + JSON.stringify(data, null, 2);
        output.style.color = '#38bdf8';
      } catch (err) {
        output.innerText = 'Error: ' + err.message;
        output.style.color = '#ef4444';
      }
    }

    async function createTodo() {
      const randomId = Math.floor(Math.random() * 1000);
      const titles = [
        'Add integration tests',
        'Configure build caching',
        'Deploy backend replica',
        'Audit security policies',
        'Optimize container sizes'
      ];
      const title = titles[Math.floor(Math.random() * titles.length)] + ' #' + randomId;

      await testEndpoint('/api/todos', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ title })
      });
    }
  </script>
</body>
</html>`;

    return new Response(html, {
      headers: { 'Content-Type': 'text/html; charset=utf-8' },
    });
  })
  // Grouped API Endpoints
  .group('/api', (api) =>
    api
      // Health Check Endpoint
      .get(
        '/health',
        () => ({
          status: 'ok',
          uptime: Math.floor((Date.now() - stats.startTime) / 1000),
          timestamp: new Date().toISOString(),
          bunVersion: Bun.version,
          elysiaVersion: '1.1.3',
        }),
        {
          detail: {
            tags: ['System'],
            summary: 'Get service health diagnostics',
          },
        }
      )
      // GET: Retrieve list of todos
      .get('/todos', () => todos, {
        detail: {
          tags: ['Todos'],
          summary: 'Retrieve all todo tasks',
        },
      })
      // POST: Create a new todo
      .post(
        '/todos',
        ({ body }) => {
          const newTodo: Todo = {
            id: String(todos.length + 1),
            title: body.title,
            completed: false,
            createdAt: new Date().toISOString(),
          };
          todos.push(newTodo);
          return newTodo;
        },
        {
          body: t.Object({
            title: t.String({
              minLength: 3,
              maxLength: 100,
              description: 'The title of the task to complete',
            }),
          }),
          detail: {
            tags: ['Todos'],
            summary: 'Create a new todo task',
          },
        }
      )
  )
  // Listen on port 3000
  .listen({
    port: process.env.PORT ? parseInt(process.env.PORT) : 3000,
    hostname: '0.0.0.0',
  });

console.log(
  `🚀 Elysia server is running at http://${app.server?.hostname}:${app.server?.port}`
);
console.log(`📖 Swagger docs available at http://${app.server?.hostname}:${app.server?.port}/swagger`);
