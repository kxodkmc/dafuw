//! DOM 构建小工具：h() 创建元素，fmtMoney 金额格式化。

/**
 * 创建元素。
 * @param {string} tag 标签名
 * @param {Record<string, any>} [attrs] 属性；`class`/`text`/`html` 为便捷键，`on*` 绑定事件
 * @param {Node|string|Array<Node|string>} [children] 子节点（数组会递归展平）
 */
export function h(tag, attrs = {}, children = []) {
  const el = document.createElement(tag);
  for (const [key, value] of Object.entries(attrs ?? {})) {
    if (value == null || value === false) continue;
    if (key === 'class') el.className = value;
    else if (key === 'text') el.textContent = value;
    else if (key.startsWith('on') && typeof value === 'function') {
      el.addEventListener(key.slice(2).toLowerCase(), value);
    } else if (value === true) el.setAttribute(key, '');
    else el.setAttribute(key, String(value));
  }
  const kids = Array.isArray(children) ? children : [children];
  for (const child of kids.flat(Infinity)) {
    if (child == null) continue;
    el.append(child.nodeType ? child : document.createTextNode(String(child)));
  }
  return el;
}

/** 金额格式化：官方世界版量纲，≥1M 显示 M、≥1K 显示 K（如 $2.6M / $600K）。 */
export function fmtMoney(n) {
  const sign = n < 0 ? '-' : '';
  const v = Math.abs(n);
  const trim = (x) => String(Number(x.toFixed(2)));
  let text;
  if (v >= 1e6) text = `${trim(v / 1e6)}M`;
  else if (v >= 1e3) text = `${trim(v / 1e3)}K`;
  else text = String(v);
  return `${sign}$${text}`;
}

/** 8 位房号补零。 */
export function fmtCode(code) {
  return String(code).padStart(8, '0');
}

/** 复制文本到剪贴板（Electron file:// 环境的兜底路径）。 */
export async function copyText(text) {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    const ta = document.createElement('textarea');
    ta.value = text;
    ta.style.position = 'fixed';
    ta.style.opacity = '0';
    document.body.append(ta);
    ta.select();
    const ok = document.execCommand('copy');
    ta.remove();
    return ok;
  }
}
