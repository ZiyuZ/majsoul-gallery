function initGiftReport() {
  const controls = document.querySelector('#report-controls');
  const input = document.querySelector('#character-search');
  const buttons = [...controls.querySelectorAll('[data-filter]')];
  const entries = [...document.querySelectorAll('.character')].map(node => ({
    node,
    limited: node.dataset.limited === 'true',
    terms: JSON.parse(node.dataset.search),
  }));
  const groups = [...document.querySelectorAll('.row')].map(node => ({
    node, cards: [...node.querySelectorAll('.character')],
  }));
  const count = document.querySelector('#result-count');
  const empty = document.querySelector('#report-empty');
  const heading = document.querySelector('.head');
  let filter = 'all';

  function update() {
    const query = normalizeReportQuery(input.value);
    let visible = 0;
    for (const entry of entries) {
      entry.node.hidden = !reportEntryMatches(entry, query, filter);
      if (!entry.node.hidden) visible++;
    }
    for (const group of groups) {
      group.node.hidden = !group.cards.some(card => !card.hidden);
    }
    count.textContent = `显示 ${visible} / ${entries.length} 位角色`;
    empty.hidden = visible > 0;
    heading.hidden = visible === 0;
    for (const button of buttons) {
      button.setAttribute('aria-pressed', String(button.dataset.filter === filter));
    }
  }

  input.addEventListener('input', update);
  for (const button of buttons) {
    button.addEventListener('click', () => { filter = button.dataset.filter; update(); });
  }
  document.querySelector('#reset-filters').addEventListener('click', () => {
    input.value = '';
    filter = 'all';
    update();
    input.focus();
  });
  controls.hidden = false;
  update();
}

if (typeof document !== 'undefined') initGiftReport();
