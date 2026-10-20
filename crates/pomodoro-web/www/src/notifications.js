// 本地通知：同一份代码同时服务三种运行环境。
//
//   1. Capacitor 原⽣壳（iOS / Android）→ @capacitor/local-notifications，
//      由系统在指定时刻弹出，应用在后台或锁屏也能响。
//   2. 浏览器 PWA（已授权）→ Web Notifications API + setTimeout。
//   3. 其他情况 → 静默 no-op，绝不让计时因为通知失败而出错。
//
// Rust 侧在 `crates/pomodoro-web/src/lib.rs` 里通过 `__pomodoroNotify` 调用，
// 且是在**阶段开始**时就预约，所以应用被冻结也不影响提醒。

const NOTIFICATION_ID = 1;

/** @type {number | null} */
let timeoutId = null;

function isNative() {
  return Boolean(globalThis.Capacitor?.isNativePlatform?.());
}

let pluginModule = null;

/// 加载 Capacitor 插件。
///
/// 不能直接 `return LocalNotifications`：Capacitor 的插件对象是一个识别
/// thenable 的 Proxy，`async` 函数返回它时引擎会把它当 Promise 等待，于是去
/// 读 `.then` 并报 “LocalNotifications.then() is not implemented on android”。
/// 包一层普通对象断开这条链。
async function nativePlugin() {
  if (!pluginModule) {
    const mod = await import('@capacitor/local-notifications');
    pluginModule = { plugin: mod.LocalNotifications };
  }
  return pluginModule;
}
async function ensureNativePermission() {
  const { plugin } = await nativePlugin();
  const status = await plugin.checkPermissions();
  if (status.display !== 'granted') {
    await plugin.requestPermissions();
  }
}

async function ensureWebPermission() {
  if (!('Notification' in globalThis)) return false;
  if (Notification.permission === 'granted') return true;
  // 只在用户手势触发的调用里请求；浏览器会拒绝其它时机的请求。
  if (Notification.permission === 'default') {
    return (await Notification.requestPermission()) === 'granted';
  }
  return false;
}

/**
 * 预约一条 `seconds` 秒后触发的通知，覆盖之前未触发的预约。
 * @param {number} seconds
 * @param {string} title
 * @param {string} body
 */
export async function schedule(seconds, title, body) {
  try {
    await cancel();
    if (seconds <= 0) return;

    if (isNative()) {
      await ensureNativePermission();
      const { plugin } = await nativePlugin();
      await plugin.schedule({
        notifications: [
          {
            id: NOTIFICATION_ID,
            title,
            body,
            schedule: { at: new Date(Date.now() + seconds * 1000) },
            smallIcon: 'ic_stat_icon_config_sample',
          },
        ],
      });
      return;
    }

    if (!(await ensureWebPermission())) return;
    timeoutId = setTimeout(() => {
      timeoutId = null;
      try {
        new Notification(title, { body, tag: 'pomodoro' });
      } catch (error) {
        console.warn('Notification failed:', error);
      }
    }, seconds * 1000);
  } catch (error) {
    // 通知是增强项：失败只记录，不能影响计时。
    console.warn('Scheduling notification failed:', error);
  }
}

/** 取消尚未触发的预约。 */
export async function cancel() {
  if (timeoutId !== null) {
    clearTimeout(timeoutId);
    timeoutId = null;
  }
  if (!isNative()) return;
  try {
    const { plugin } = await nativePlugin();
    await plugin.cancel({ notifications: [{ id: NOTIFICATION_ID }] });
  } catch (error) {
    console.warn('Cancelling notification failed:', error);
  }
}

/** 暴露给 Rust 的入口。 */
export function installRustBridge() {
  globalThis.__pomodoroNotify = { schedule, cancel };

  // 浏览器的通知权限必须由用户手势触发。Rust 侧调用 schedule() 时已经离开了
  // 手势上下文，直接 requestPermission() 会被拒。所以在第一次点击/按键时
  // 就预先请求，之后 schedule() 只需要检查权限。
  if (!isNative() && 'Notification' in globalThis && Notification.permission === 'default') {
    const request = () => {
      Notification.requestPermission().catch(() => {});
      globalThis.removeEventListener('pointerdown', request);
      globalThis.removeEventListener('keydown', request);
    };
    globalThis.addEventListener('pointerdown', request, { once: true });
    globalThis.addEventListener('keydown', request, { once: true });
  }
}
