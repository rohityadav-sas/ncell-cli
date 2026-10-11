(() => {
  const root = document.documentElement;
  const themeKey = 'ncell-docs-theme';
  let theme;
  try { theme = localStorage.getItem(themeKey); } catch {}
  root.dataset.theme = theme === 'light' || theme === 'dark' ? theme : matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
  document.getElementById('reference-theme').addEventListener('click', () => {
    root.dataset.theme = root.dataset.theme === 'dark' ? 'light' : 'dark';
    try { localStorage.setItem(themeKey, root.dataset.theme); } catch {}
  });
  const search = document.getElementById('code-search');
  const prefix = document.getElementById('code-prefix');
  const rows = [...document.querySelectorAll('#code-rows tr')];
  function filter() {
    const terms = search.value.toLowerCase().trim().split(/\s+/);
    let count = 0;
    for (const row of rows) {
      row.hidden = (prefix.value && row.dataset.prefix !== prefix.value) || !terms.every(term => row.textContent.toLowerCase().includes(term));
      if (!row.hidden) count++;
    }
    document.getElementById('code-count').textContent = `${count} of ${rows.length} codes`;
    document.getElementById('code-empty').hidden = count !== 0;
  }
  search.addEventListener('input', filter);
  prefix.addEventListener('change', filter);
  filter();
})();
