(() => {
  const controls = document.querySelector('#owned-controls');
  const input = document.querySelector('#owned-search');
  const buttons = [...controls.querySelectorAll('[data-kind]')];
  const entries = [...document.querySelectorAll('.entry')].map(node => ({node,terms:JSON.parse(node.dataset.search),kind:node.closest('.owned-section').dataset.kind}));
  const groups = [...document.querySelectorAll('.owned-section')].map(node => ({node,cards:[...node.querySelectorAll('.entry')]}));
  let kind = 'all';
  function update() {
    const query = normalizeReportQuery(input.value);
    let visible = 0;
    for (const entry of entries) {
      entry.node.hidden = !(kind === 'all' || kind === entry.kind) || !reportEntryMatches(entry, query);
      if (!entry.node.hidden) visible++;
    }
    for (const group of groups) group.node.hidden = !group.cards.some(card => !card.hidden);
    const heading = document.querySelector('#decorations-heading');
    if (heading) heading.hidden = !groups.some(g => g.node.dataset.kind === 'decorations' && !g.node.hidden);
    document.querySelector('#owned-count').textContent = `显示 ${visible} / ${entries.length} 项收藏`;
    document.querySelector('#owned-empty').hidden = visible > 0;
    for (const button of buttons) button.setAttribute('aria-pressed', String(button.dataset.kind === kind));
  }
  input.addEventListener('input', update);
  for (const button of buttons) button.addEventListener('click', () => {kind=button.dataset.kind;update()});
  document.querySelector('#owned-reset').addEventListener('click', () => {input.value='';kind='all';update();input.focus()});
  controls.hidden=false;
  update();
})();
