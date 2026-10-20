import { defineConfig } from 'vite';
import wasm from 'vite-plugin-wasm';
import path from 'path';
import { fileURLToPath } from 'url';
import fs from 'fs';
import crypto from 'crypto';

const __dirname = path.dirname(fileURLToPath(import.meta.url));

// 图标由 GPUI Kit 的 assets crate 提供，Web 端按需下载。
// 开发时从这个本地检出目录读取；生产环境应换成 CDN 或复制到站点目录。
const ICONS_DIR = path.resolve(__dirname, '../../../../gpui-kit/crates/assets/assets/icons');

// 部署到子路径时用 `VITE_BASE=/pomodoro/ bun run build`，默认部署在站点根。
const base = process.env.VITE_BASE ?? '/';

/**
 * 把构建版本号注入 `sw.js` 的缓存名。
 *
 * 版本号取自 wasm 产物（`src/wasm/*.wasm`）的内容哈希：内容变了哈希就变，
 * 缓存名跟着变，Service Worker 的 activate 阶段会清掉所有旧缓存。
 * 否则发新版后旧缓存还在，新 HTML 引用的带哈希资源既不在缓存里、
 * 服务器上也已经改名，会 404 并卡在启动页。
 */
function swVersion() {
  return {
    name: 'sw-version',
    apply: 'build',
    generateBundle(_options, bundle) {
      const hashes = [];
      for (const fileName of Object.keys(bundle)) {
        if (fileName.endsWith('.wasm')) {
          const chunk = bundle[fileName];
          if (chunk.type === 'asset' && chunk.source) {
            hashes.push(crypto.createHash('sha256').update(chunk.source).digest('hex'));
          }
        }
      }
      // 没有 wasm 时退回用产物目录的文件名列表算一个版本。
      if (hashes.length === 0) {
        const distDir = path.resolve(__dirname, 'dist');
        if (fs.existsSync(distDir)) {
          for (const name of fs.readdirSync(distDir).sort()) {
            hashes.push(name);
          }
        }
      }
      const version = crypto
        .createHash('sha256')
        .update(hashes.join('|'))
        .digest('hex')
        .slice(0, 12);

      // `public/` 下的文件不进 bundle，单独处理。
      const swPath = path.resolve(__dirname, 'dist/sw.js');
      if (fs.existsSync(swPath)) {
        const sw = fs.readFileSync(swPath, 'utf8');
        fs.writeFileSync(swPath, sw.replace(/__BUILD_VERSION__/g, version));
      }
    },
  };
}

export default defineConfig({
  base,
  plugins: [
    wasm(),
    swVersion(),
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
