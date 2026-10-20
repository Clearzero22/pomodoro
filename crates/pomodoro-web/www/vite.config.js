import { defineConfig } from 'vite';
import wasm from 'vite-plugin-wasm';
import path from 'path';
import { fileURLToPath } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));

// 图标由 GPUI Kit 的 assets crate 提供，Web 端按需下载。
// 开发时从这个本地检出目录读取；生产环境应换成 CDN 或复制到站点目录。
const ICONS_DIR = path.resolve(__dirname, '../../../../gpui-kit/crates/assets/assets/icons');

// 部署到子路径时用 `VITE_BASE=/pomodoro/ bun run build`，默认部署在站点根。
const base = process.env.VITE_BASE ?? '/';

export default defineConfig({
  base,
  plugins: [
    wasm(),
    {
      name: 'serve-icons',
      configureServer(server) {
        server.middlewares.use('/assets/icons', (req, res, next) => {
          const name = decodeURIComponent(req.url.split('?')[0]);
          const filePath = path.join(ICONS_DIR, name);
          if (!filePath.startsWith(ICONS_DIR)) {
            next();
            return;
          }
          import('fs').then(({ default: fs }) => {
            if (fs.existsSync(filePath) && fs.statSync(filePath).isFile()) {
              res.setHeader('Content-Type', 'image/svg+xml');
              res.setHeader('Access-Control-Allow-Origin', '*');
              fs.createReadStream(filePath).pipe(res);
            } else {
              next();
            }
          });
        });
      },
    },
  ],
  server: {
    port: 3001,
    open: true,
    fs: {
      strict: false,
      allow: ['..'],
    },
    // GPUI 的 Web 平台用 SharedArrayBuffer 时（多线程 WASM）需要这组头。
    // 当前是单线程构建，保留它们不会有害，也方便以后切换。
    headers: {
      'Cross-Origin-Embedder-Policy': 'require-corp',
      'Cross-Origin-Opener-Policy': 'same-origin',
    },
  },
  build: {
    target: 'esnext',
    minify: true,
    sourcemap: false,
    rollupOptions: {
      onwarn(warning, warn) {
        // wasm-bindgen 生成的绑定代码会用到 eval。
        if (warning.code === 'EVAL' && warning.id?.endsWith('/src/wasm/pomodoro_web.js')) {
          return;
        }
        warn(warning);
      },
    },
  },
  optimizeDeps: {
    exclude: ['./src/wasm'],
  },
});
