//! 通用模态框：单实例层叠，ESC/遮罩关闭。

import { h } from './dom.js';

const root = () => document.getElementById('modal-root');

/**
 * 打开模态框。
 * @param {{title:string, closable?:boolean}} opts
 * @param {Node} bodyEl 内容节点
 * @returns {{close:()=>void}} 关闭句柄
 */
export function openModal({ title, closable = true }, bodyEl) {
  const overlay = h('div', { class: 'modal-overlay' });
  const closeBtn = h('button', {
    class: 'modal-close', title: '关闭', text: '✕',
    onclick: () => closable && close(),
  });
  const modal = h('div', { class: 'modal' }, [
    h('div', { class: 'modal-head' }, [h('span', { text: title }), closeBtn]),
    h('div', { class: 'modal-body' }, [bodyEl]),
  ]);
  overlay.append(modal);

  const close = () => {
    overlay.remove();
    document.removeEventListener('keydown', onKey);
  };
  const onKey = (e) => {
    if (e.key === 'Escape' && closable) close();
  };

  if (closable) {
    overlay.addEventListener('click', (e) => {
      if (e.target === overlay) close();
    });
    document.addEventListener('keydown', onKey);
  }

  root().append(overlay);
  return { close };
}
