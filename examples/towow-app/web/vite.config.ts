import { defineConfig } from 'vite';

// 开发与预览时把 /events、/api 代理到宿主，前端“先试同源”在 dev 下也成立；宿主地址用 TOWOW_URL 改。
const backend = process.env.TOWOW_URL || 'http://localhost:8794';
const proxy = {
  '/api': { target: backend, changeOrigin: true },
  '/events': { target: backend.replace(/^http/, 'ws'), ws: true, changeOrigin: true },
};

export default defineConfig({
  base: './',
  server: { port: 5178, strictPort: false, proxy },
  preview: { proxy },
  build: { target: 'es2022', chunkSizeWarningLimit: 1200 },
});
