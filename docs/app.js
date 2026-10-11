'use strict';
const $ = selector => document.querySelector(selector);
const sections = [...document.querySelectorAll('main > section')];
const links = [...document.querySelectorAll('nav a')];
const sidebar = $('#sidebar');
const menu = $('#menu');
const search = $('#search');
let selectedTheme;
try { selectedTheme = localStorage.getItem('ncell-docs-theme'); } catch {}
const systemTheme = matchMedia('(prefers-color-scheme: dark)');
function setTheme(theme) {
  document.documentElement.dataset.theme = theme;
  $('#theme').setAttribute('aria-label', `Switch to ${theme === 'dark' ? 'light' : 'dark'} theme`);
}
setTheme(selectedTheme || (systemTheme.matches ? 'dark' : 'light'));
$('#theme').addEventListener('click', () => {
  selectedTheme = document.documentElement.dataset.theme === 'dark' ? 'light' : 'dark';
  setTheme(selectedTheme);
  try { localStorage.setItem('ncell-docs-theme', selectedTheme); } catch {}
});
systemTheme.addEventListener('change', event => {
  if (!selectedTheme) setTheme(event.matches ? 'dark' : 'light');
});
function toggleMenu(open) {
  sidebar.classList.toggle('open', open);
  menu.setAttribute('aria-expanded', String(open));
}
menu.addEventListener('click', () => toggleMenu(!sidebar.classList.contains('open')));
function filterDocs() {
  const query = search.value.trim().toLowerCase();
  let count = 0;
  sections.forEach(section => {
    const match = !query || section.textContent.toLowerCase().includes(query);
    section.hidden = !match;
    const link = links.find(link => link.hash === `#${section.id}`);
    if (link) link.hidden = !match;
    if (match) count++;
  });
  $('#search-info').hidden = !query;
  $('#search-info').textContent = count ? `${count} matching guide${count === 1 ? '' : 's'}. Clear search to show all documentation.` : 'No matching guides. Try “login”, “sort” or “refresh”.';
}
search.addEventListener('input', filterDocs);
function clearSearch() { search.value = ''; filterDocs(); }
links.forEach(link => link.addEventListener('click', () => {
  toggleMenu(false);
  if (search.value) clearSearch();
}));
window.addEventListener('hashchange', () => {
  if (search.value) clearSearch();
});
document.addEventListener('keydown', event => {
  const editing = /INPUT|TEXTAREA|SELECT/.test(document.activeElement.tagName) || document.activeElement.isContentEditable;
  if (event.key === '/' && !editing) {
    event.preventDefault();
    if (matchMedia('(max-width: 900px)').matches) toggleMenu(true);
    search.focus();
  }
  if (event.key === 'Escape') {
    clearSearch();
    toggleMenu(false);
    if (document.activeElement === search) menu.focus();
  }
});
document.addEventListener('click', event => {
  if (!sidebar.contains(event.target) && !menu.contains(event.target)) toggleMenu(false);
});
const observer = new IntersectionObserver(entries => {
  const visible = entries.filter(entry => entry.isIntersecting);
  if (!visible.length) return;
  const id = visible[0].target.id;
  links.forEach(link => {
    const active = link.hash === `#${id}`;
    link.classList.toggle('active', active);
    if (active) link.setAttribute('aria-current', 'location');
    else link.removeAttribute('aria-current');
  });
}, { rootMargin: '-90px 0px -60% 0px' });
sections.forEach(section => observer.observe(section));
for (const pre of document.querySelectorAll('pre')) {
  const button = document.createElement('button');
  button.className = 'copy';
  button.type = 'button';
  button.textContent = 'Copy';
  button.setAttribute('aria-label', 'Copy code to clipboard');
  button.addEventListener('click', async () => {
    const text = pre.querySelector('code').textContent;
    try {
      await navigator.clipboard.writeText(text);
      button.textContent = 'Copied ✓';
    } catch {
      button.textContent = 'Select code';
      const range = document.createRange();
      range.selectNodeContents(pre.querySelector('code'));
      const selection = window.getSelection();
      selection.removeAllRanges();
      selection.addRange(range);
    }
    setTimeout(() => { button.textContent = 'Copy'; }, 1800);
  });
  pre.append(button);
}
function buildCommand() {
  const keyword = $('#keyword').value.replace(/'/g, "''");
  $('#built-command').textContent = `ncell search '${keyword}' --tag ${$('#tag').value} --sort ${$('#sort').value}${$('#json-option').checked ? ' --json' : ''}`;
}
$('#builder').addEventListener('input', buildCommand);
$('#builder').addEventListener('submit', event => event.preventDefault());
buildCommand();
window.addEventListener('scroll', () => { $('#top').hidden = scrollY < 700; }, { passive: true });
$('#top').addEventListener('click', () => window.scrollTo({ top: 0, behavior: matchMedia('(prefers-reduced-motion: reduce)').matches ? 'instant' : 'smooth' }));
