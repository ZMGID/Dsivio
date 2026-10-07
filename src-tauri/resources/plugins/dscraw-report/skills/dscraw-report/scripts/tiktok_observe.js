(() => {
  const visible = e => e.getClientRects().length > 0;
  const text = document.body.innerText;
  const lines = text.split('\n').map(s => s.trim()).filter(Boolean);
  const key = lines.indexOf('关键指标');
  const metric = label => {
    const i = lines.indexOf(label, key + 1);
    if (key < 0 || i < 0) return null;
    const valueIndex = lines[i + 1] === 'R$' ? i + 2 : i + 1;
    const fraction = lines[valueIndex + 1];
    return lines[valueIndex] + (/^,\d{2}$/.test(fraction || '') ? fraction : '');
  };
  const inputs = [...document.querySelectorAll('input')].filter(visible);
  return JSON.stringify({
    url: location.origin + location.pathname,
    start: inputs.find(e => e.placeholder === '开始日期')?.value || null,
    end: inputs.find(e => e.placeholder === '结束日期')?.value || null,
    exportVisible: [...document.querySelectorAll('[data-testid="export-button"]')].filter(visible).length === 1,
    sales: metric('GMV'), orders: metric('订单数'), currencyBRL: key >= 0 && lines.slice(key).includes('R$'),
    historyVisible: text.includes('以下是您可以在7天内下载的报告'),
    history: [...document.querySelectorAll('.pcm-ae-record-item')].filter(visible).map(e => e.innerText),
    text
  });
})()
