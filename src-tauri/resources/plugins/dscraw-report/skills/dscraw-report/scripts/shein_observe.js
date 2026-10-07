(() => {
  const visible = e => e.getClientRects().length > 0 && getComputedStyle(e).visibility !== 'hidden';
  const selector = e => {
    const parts = [];
    while (e && e.nodeType === 1) {
      const tag = e.tagName.toLowerCase();
      if (!e.parentElement) { parts.unshift(tag); break; }
      const siblings = [...e.parentElement.children].filter(x => x.tagName === e.tagName);
      parts.unshift(tag + ':nth-of-type(' + (siblings.indexOf(e) + 1) + ')');
      e = e.parentElement;
    }
    return parts.join(' > ');
  };
  const text = document.body?.innerText || '';
  const lines = text.split('\n').map(x => x.trim()).filter(Boolean);
  const after = label => {const i=lines.indexOf(label); return i<0 ? null : lines[i+1];};
  const labels = ['交易概览','近7天','昨天','导出','新客GMV','新客销量','新客支付订单数'];
  const controls = [...document.querySelectorAll('button,.merchant-ui-tabs-panel-title span,.text-overflow-ellipsis-1')]
    .filter(e => visible(e) && !e.disabled && labels.includes(e.innerText.trim()))
    .map(e => ({text:e.innerText.trim(), selector:selector(e), selected:e.className.includes('checked')}));
  return JSON.stringify({url:location.origin+location.pathname+location.hash, text, controls,
    period:after('统计期间:'), updatedAt:after('数据更新时间:'), site:after('站点'),
    totals:{GMV:after('总GMV'), '销量':after('总销量'), '支付订单数':after('总支付订单数')},
    chart:lines.find(x => ['GMV趋势图','销量趋势图','支付订单数趋势图'].includes(x)) || null});
})()
