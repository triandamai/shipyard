let n = 0;
const btn = document.getElementById('btn');
btn.addEventListener('click', () => {
  btn.textContent = `Clicked ${++n} times`;
});
