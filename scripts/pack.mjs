//! 一键打包：构建 Rust 服务器 → 同步产物到 dist/大富翁世界版/。
//!
//! 用法：
//!   node scripts/pack.mjs            # 构建服务器 + 同步
//!   node scripts/pack.mjs --fast     # 跳过 cargo 构建，仅同步
//!
//! 注意：打包前请关闭正在运行的游戏，否则 monopoly-cli.exe 被占用无法覆盖。

import { spawnSync } from 'node:child_process';
import { cpSync, mkdirSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const dst = join(root, 'dist', '大富翁世界版', 'resources', 'app');
const fast = process.argv.includes('--fast');

/* ---- 1. 构建服务器 ---- */

if (!fast) {
  console.log('[1/2] cargo build --release -p monopoly-cli …');
  const r = spawnSync('cargo', ['build', '--release', '-p', 'monopoly-cli'], {
    cwd: root,
    stdio: 'inherit',
    shell: true,
  });
  if (r.status !== 0) {
    console.error('服务器构建失败，打包中止。');
    process.exit(1);
  }
} else {
  console.log('[1/2] 跳过构建（--fast）');
}

/* ---- 2. 同步产物 ---- */

console.log('[2/2] 同步到 dist/大富翁世界版 …');
mkdirSync(join(dst, 'bin'), { recursive: true });

// Electron 壳
cpSync(join(root, 'electron', 'main.js'), join(dst, 'electron', 'main.js'));
cpSync(join(root, 'electron', 'preload.js'), join(dst, 'electron', 'preload.js'));

// 打包版入口 package.json（main 指向 electron/，勿用开发版覆盖）
writeFileSync(
  join(dst, 'package.json'),
  JSON.stringify(
    {
      name: 'dafuw-desktop',
      version: '0.1.0',
      private: true,
      description: '大富翁桌面客户端（打包版，入口在 electron/）',
      main: 'electron/main.js',
    },
    null,
    2,
  ),
);

// 前端（无构建步骤，原生 ES Module 直接拷贝）
for (const [src, rel] of [
  ['src', 'frontend/src'],
  ['styles', 'frontend/styles'],
]) {
  cpSync(join(root, 'frontend', src), join(dst, rel), { recursive: true });
}
cpSync(join(root, 'frontend', 'index.html'), join(dst, 'frontend', 'index.html'));

// 服务器二进制
cpSync(
  join(root, 'target', 'release', 'monopoly-cli.exe'),
  join(dst, 'bin', 'monopoly-cli.exe'),
);

console.log('打包完成：dist/大富翁世界版/大富翁.exe');
