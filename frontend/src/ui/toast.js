//! 轻量 toast：错误/成功/信息提示，自动消失。

import { h } from './dom.js';

/** @param {string} message @param {'info'|'error'|'success'} [type] */
export function toast(message, type = 'info') {
  const root = document.getElementById('toasts');
  const node = h('div', { class: `toast ${type}`, text: message });
  root.append(node);
  setTimeout(() => node.remove(), 3500);
}
