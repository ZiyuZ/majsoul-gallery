function normalizeReportQuery(value) {
  return value.toLowerCase().replace(/ü/g, 'v').replace(/[^\p{L}\p{N}]/gu, '');
}

function reportEntryMatches(entry, query, filter = 'all') {
  return (filter === 'all' || entry.limited === (filter === 'limited')) &&
    entry.terms.some(term => term.includes(query));
}
