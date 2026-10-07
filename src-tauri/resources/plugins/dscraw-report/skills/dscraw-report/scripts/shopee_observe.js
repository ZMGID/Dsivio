// Read visible page state only. Never read credentials, script text or session URLs.
(() => {
  const visible=e=>e.getClientRects().length>0 && getComputedStyle(e).visibility!=='hidden';
  const css=e=>{const p=[];while(e&&e.nodeType===1){const t=e.tagName.toLowerCase();const s=e.parentElement?[...e.parentElement.children].filter(x=>x.tagName===e.tagName):[e];p.unshift(t+':nth-of-type('+(s.indexOf(e)+1)+')');e=e.parentElement;}return p.join(' > ');};
  const txt=e=>e?.innerText?.trim()||'';
  const all=s=>[...document.querySelectorAll(s)].filter(visible);
  const controls=[];
  const add=(key,selector,label=null,prefix=false)=>{
    for(const e of all(selector).filter(e=>!e.disabled)) {
      if(label!==null && !(prefix?txt(e).startsWith(label):txt(e)===label)) continue;
      controls.push({key,selector:css(e),text:txt(e)});
    }
  };
  add('verify','button','进行验证');
  add('login','button,a','Login with Main/Sub Account');
  add('businessDate','.bi-date-input.track-click-open-time-selector');
  for(const e of all('.eds-date-shortcut-item')) {
    const label=txt(e).split('\n')[0].replace(/\s/g,'');
    const key=label.startsWith('过去7天')||label.startsWith('Últimos7Dias')?'过去7天':
      label.startsWith('昨天')||label.startsWith('Ontem')?'昨天':null;
    if(key) controls.push({key,selector:css(e),text:txt(e)});
  }
  add('businessExport','button.track-click-normal-export');
  add('adsDate','[data-testid="ads-performance-date-range-selector"] .eds-selector');
  add('adsExport','[data-testid="export-data-dropdown-trigger"]');
  add('adsGroup','[data-testid="export-data-dropdown-item"]','广告组数据',true);
  add('tasks','[data-testid="export-data-result-trigger"]');
  for(const modal of all('.eds-modal__box')) {
    if(txt(modal.querySelector('.eds-modal__title'))==='广告组数据') {
      for(const e of [...modal.querySelectorAll('button')].filter(e=>visible(e)&&!e.disabled&&txt(e)==='确认'))
        controls.push({key:'adsConfirm',selector:css(e),text:'确认'});
    }
  }
  const text=document.body?.innerText||'';
  const section=(a,b)=>{const i=text.indexOf(a),j=text.indexOf(b,i);return i<0?'':text.slice(i,j>i?j:i+2000);};
  const businessStart=text.includes('统计时间')?'统计时间':text.includes('数据时段')?'数据时段':'Período dos Dados';
  const businessEnd=text.includes('每个指标的趋势图表')?'每个指标的趋势图表':
    text.includes('每个指标的图表')?'每个指标的图表':'Gráfico de Tendências de Cada Métrica';
  const business=section(businessStart,businessEnd);
  const ads=section(text.includes('所有商品广告表现')?'所有商品广告表现':'所有商品广告效果','所有商品广告列表');
  const dateText=txt(document.querySelector('.bi-date-input'));
  const u=new URL(location.href);
  const tasks=[...document.querySelectorAll('[data-testid="export-data-result-item"]')].map(e=>{
    const b=e.querySelector('button');
    return {stamp:e.getAttribute('data-test-timestamp'),text:txt(e),visible:visible(e),
      downloadable:!!b&&visible(b)&&!b.disabled&&txt(b)==='下载',selector:b?css(b):null};
  });
  return JSON.stringify({url:location.origin+location.pathname,identityLines:text.split('\n').slice(0,10),
    proxyError:text.includes('ERR_SOCKS_CONNECTION_FAILED'),localSeller:text.includes('Current View\nLocal Seller'),controls,tasks,business,ads,dateText,
    adsDate:txt(document.querySelector('[data-testid="ads-performance-date-range-selector"]')),
    from:u.searchParams.get('from'),to:u.searchParams.get('to'),
    updating:/更新中|数据正在处理中|如果没有显示数据/.test(business),
    loading:all('.eds-loading__mask,.eds-loading-mask,[aria-busy="true"]').length>0});
})()
