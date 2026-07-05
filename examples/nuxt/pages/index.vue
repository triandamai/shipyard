<script setup>
const count = ref(0);
</script>

<template>
  <main>
    <h1>Shipyard — Nuxt Example</h1>
    <p>A minimal static site deployed via Shipyard.</p>
    <button @click="count++">Clicked {{ count }} times</button>
  </main>
</template>

<style scoped>
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
