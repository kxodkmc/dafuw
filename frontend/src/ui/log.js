//! 日志面板：带时间戳的事件播报与聊天，支持类型过滤与聊天输入。

import { h } from './dom.js';

const FILTERS = [
  { id: 'all', label: '全部' },
  { id: 'sys', label: '事件' },
  { id: 'chat', label: '聊天' },
];

const two = (n) => String(n).padStart(2, '0');
const clock = (ts) => {
  const d = new Date(ts);
  return `${two(d.getHours())}:${two(d.getMinutes())}`;
};

/**
 * 全量重绘日志（过滤态挂在 mountEl 上保持稳定）。
 * @param {Array<{kind:'sys'|'chat', text:string, ts:number}>} log
 * @param {HTMLElement} mountEl 容器
 * @param {{onSend?:(text:string)=>void}} [opts] 提供时展示聊天输入框
 */
export function renderLog(log, mountEl, opts = {}) {
  const filter = (mountEl._filter ??= 'all');
  const shown = log.filter((e) => filter === 'all' || e.kind === filter);

  const list = h('div', { class: 'log-list' },
    shown.map((e) => h('div', { class: `log-item ${e.kind}` }, [
      h('span', { class: 'log-ts', text: clock(e.ts) }),
      h('span', { text: e.text }),
    ])),
  );

  // 保留贴底滚动语义
  const prev = mountEl.querySelector('.log-list');
  const stick = !prev || prev.scrollHeight - prev.scrollTop - prev.clientHeight < 30;

  const body = h('div', { class: 'col', style: 'flex:1;min-height:0;gap:6px' }, [
    h('div', { class: 'row' }, [
      h('div', { class: 'card-title grow', text: '动态' }),
      ...FILTERS.map((f) => h('button', {
        class: `chip${filter === f.id ? ' selected' : ''}`,
        text: f.label,
        onclick: () => {
          mountEl._filter = f.id;
          renderLog(log, mountEl, opts);
        },
      })),
    ]),
    list,
  ]);

  if (opts.onSend) {
    const input = h('input', {
      class: 'input',
      placeholder: '发消息给房间…',
      maxlength: '200',
    });
    const send = () => {
      const text = input.value.trim();
      if (!text) return;
      opts.onSend(text);
      input.value = '';
    };
    input.addEventListener('keydown', (e) => {
      if (e.key === 'Enter') send();
    });
    body.append(h('div', { class: 'row' }, [
      input,
      h('button', { class: 'btn primary', text: '发送', onclick: send }),
    ]));
  }

  mountEl.replaceChildren(body);
  if (stick) list.scrollTop = list.scrollHeight;
}
