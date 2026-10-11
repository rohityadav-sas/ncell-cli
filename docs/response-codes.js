(() => {
  const search = document.getElementById('code-search');
  const clear = document.getElementById('clear-code-search');
  const trigger = document.getElementById('code-prefix');
  const list = document.getElementById('prefix-options');
  const rows = [...document.querySelectorAll('#code-rows tr')];
  const counts = new Map();
  for (const row of rows) {
    counts.set(row.dataset.prefix, (counts.get(row.dataset.prefix) || 0) + 1);
    if (/^Unconfirmed:/.test(row.cells[1].textContent)) row.classList.add('unconfirmed');
  }
  const values = ['', ...[...counts.keys()].sort()];
  let selected = 0, active = 0, typed = '', lastTyped = 0;
  function filter() {
    const terms = search.value.toLowerCase().trim().split(/\s+/);
    let shown = 0;
    for (const row of rows) {
      row.hidden = (selected !== 0 && row.dataset.prefix !== values[selected]) || !terms.every(term => row.textContent.toLowerCase().includes(term));
      if (!row.hidden) shown++;
    }
    document.getElementById('code-count').textContent = shown === rows.length ? `${rows.length} codes` : `${shown} of ${rows.length} codes`;
    document.getElementById('code-empty').hidden = shown !== 0;
    clear.hidden = !search.value;
  }
  function highlight(index) {
    active = Math.max(0, Math.min(values.length - 1, index));
    [...list.children].forEach((option, i) => option.classList.toggle('active', i === active));
    trigger.setAttribute('aria-activedescendant', `prefix-option-${active}`);
    list.children[active].scrollIntoView({ block: 'nearest' });
  }
  function close() {
    list.hidden = true;
    trigger.setAttribute('aria-expanded', 'false');
    trigger.removeAttribute('aria-activedescendant');
  }
  function open() {
    list.hidden = false;
    trigger.setAttribute('aria-expanded', 'true');
    highlight(selected);
  }
  function choose(index) {
    selected = index;
    document.getElementById('prefix-value').textContent = values[index] || 'All prefixes';
    [...list.children].forEach((option, i) => option.setAttribute('aria-selected', String(i === index)));
    close();
    filter();
    trigger.focus({ preventScroll: true });
  }
  values.forEach((value, i) => {
    const option = document.createElement('div');
    option.id = `prefix-option-${i}`;
    option.className = 'prefix-option';
    option.setAttribute('role', 'option');
    option.setAttribute('aria-selected', String(i === selected));
    const label = document.createElement('span');
    label.textContent = value || 'All prefixes';
    const count = document.createElement('span');
    count.className = 'prefix-count';
    count.textContent = value ? counts.get(value) : rows.length;
    option.append(label, count);
    option.addEventListener('click', () => choose(i));
    option.addEventListener('pointermove', () => highlight(i));
    list.append(option);
  });
  trigger.addEventListener('click', () => list.hidden ? open() : close());
  list.addEventListener('mousedown', event => event.preventDefault());
  trigger.addEventListener('keydown', event => {
    const key = event.key;
    if (key === 'Tab') { close(); return; }
    if (key === 'Escape') { if (!list.hidden) { event.preventDefault(); event.stopPropagation(); close(); } return; }
    if (['ArrowDown', 'ArrowUp', 'Home', 'End', 'Enter', ' '].includes(key)) {
      event.preventDefault();
      if (list.hidden) { open(); return; }
      if (key === 'Enter' || key === ' ') choose(active);
      else highlight(key === 'Home' ? 0 : key === 'End' ? values.length - 1 : active + (key === 'ArrowDown' ? 1 : -1));
    } else if (key.length === 1 && !event.ctrlKey && !event.metaKey && !event.altKey) {
      event.preventDefault();
      typed = Date.now() - lastTyped > 700 ? key : typed + key;
      lastTyped = Date.now();
      if (list.hidden) open();
      const index = values.findIndex(value => value.startsWith(typed.toUpperCase()));
      if (index >= 0) highlight(index);
    }
  });
  document.addEventListener('pointerdown', event => { if (!event.target.closest('.prefix-field')) close(); });
  trigger.addEventListener('blur', close);
  search.addEventListener('input', filter);
  clear.addEventListener('click', () => { search.value = ''; filter(); search.focus(); });
  addEventListener('response-code-selected', event => {
    const row = document.getElementById(event.detail);
    if (row?.hidden) { search.value = ''; choose(0); }
  });
  filter();
})();
