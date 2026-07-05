<script>
  let count = $state(0);
</script>

<main>
  <h1>Shipyard — SvelteKit Example</h1>
  <p>A minimal static site deployed via Shipyard.</p>
  <button onclick={() => count++}>Clicked {count} times</button>
</main>

<style>
  main { font-family: sans-serif; max-width: 600px; margin: 80px auto; text-align: center; }
  h1   { color: #6366f1; }
  button {
    margin-top: 16px;
    padding: 10px 24px;
    background: #6366f1;
    color: white;
    border: none;
    border-radius: 8px;
    font-size: 16px;
    cursor: pointer;
  }
  button:hover { opacity: 0.85; }
</style>
