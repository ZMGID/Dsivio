JSON.stringify((() => {
  const visible = e => e.getBoundingClientRect().width > 0 && e.getBoundingClientRect().height > 0;
  const text = document.body.innerText;
  const buttons = [...document.querySelectorAll('button')];
  const yesterday = buttons.filter(e => visible(e) && e.innerText.trim() === '昨天');
  return {url:location.origin+location.pathname, text,
    start:document.querySelector('input[placeholder="开始日期"]')?.value,
    end:document.querySelector('input[placeholder="结束日期"]')?.value,
    yesterdaySelector:yesterday.length === 1 && document.querySelectorAll('button.theme-arco-btn-size-mini:nth-of-type(2)').length === 1 && document.querySelector('button.theme-arco-btn-size-mini:nth-of-type(2)') === yesterday[0]
      ? 'button.theme-arco-btn-size-mini:nth-of-type(2)' : null,
    exportSelector:buttons.filter(e => visible(e) && (e.getAttribute('data-testid') || '').startsWith('export-button-index-')).length === 1
      ? '[data-testid="'+buttons.find(e => visible(e) && (e.getAttribute('data-testid') || '').startsWith('export-button-index-')).getAttribute('data-testid')+'"]' : null};
})())
