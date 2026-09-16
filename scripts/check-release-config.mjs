// 发布前检查 tauri.conf.json 的自动更新配置是否还是占位值。
//
// 背景：占位 pubkey 不会让构建失败，装出来的包却无法验证更新签名；这个脚本
// 把该问题前移到发布流程的第一步，避免带着占位配置产出安装包。
//
// 用法：node scripts/check-release-config.mjs [配置文件路径]
// 退出码：0 通过，1 存在占位或非法配置，2 文件读不到或不是合法 JSON。

import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';

const here = dirname(fileURLToPath(import.meta.url));
const defaultConfig = resolve(here, '..', 'src-tauri', 'tauri.conf.json');
const configPath = process.argv[2] ? resolve(process.argv[2]) : defaultConfig;

// 公钥里出现这些标记说明还没换成本次发布真正要用的值。
const PUBKEY_MARKERS = [
  'REPLACE',
  'PLACEHOLDER',
  'CHANGEME',
  'CHANGE_ME',
  'TODO',
  'XXX',
  'YOUR_',
  'EXAMPLE',
];

// 地址里出现这些标记说明还没换成真实域名。这里不包含 EXAMPLE：主机名合法地
// 含有 example 并不等于占位（例如公司就叫 example-inc），只按下面的主机名清单判定。
const URL_MARKERS = ['REPLACE', 'PLACEHOLDER', 'CHANGEME', 'CHANGE_ME', 'YOUR_', 'TODO'];

// 保留给文档与测试的主机名，不可能是真实更新地址。
const PLACEHOLDER_HOSTS = [
  'example.com',
  'example.org',
  'example.net',
  'localhost',
  '127.0.0.1',
  '0.0.0.0',
];

const problems = [];

function hasMarker(value, markers) {
  const upper = value.toUpperCase();
  return markers.filter((marker) => upper.includes(marker));
}

function checkPubkey(pubkey) {
  if (typeof pubkey !== 'string' || pubkey.trim() === '') {
    problems.push('plugins.updater.pubkey 为空：发布包将无法验证更新签名');
    return;
  }
  const markers = hasMarker(pubkey, PUBKEY_MARKERS);
  if (markers.length > 0) {
    problems.push(
      `plugins.updater.pubkey 仍是占位值（含 ${markers.join('、')}）：请换成 tauri signer generate 产出的公钥`,
    );
    return;
  }
  // Tauri 更新公钥是 minisign 公钥的 base64 文本，长度明显长于普通标识符。
  if (!/^[A-Za-z0-9+/=]+$/.test(pubkey) || pubkey.length < 56) {
    problems.push(
      `plugins.updater.pubkey 不像 base64 公钥（长度 ${pubkey.length}）：请用 tauri signer generate 重新生成`,
    );
  }
}

function checkEndpoints(endpoints) {
  if (!Array.isArray(endpoints) || endpoints.length === 0) {
    problems.push('plugins.updater.endpoints 为空：已发布版本不会收到更新检查');
    return;
  }
  endpoints.forEach((endpoint, index) => {
    const at = `plugins.updater.endpoints[${index}]`;
    if (typeof endpoint !== 'string' || endpoint.trim() === '') {
      problems.push(`${at} 为空`);
      return;
    }
    let url;
    try {
      url = new URL(endpoint);
    } catch {
      problems.push(`${at} 不是合法 URL：${endpoint}`);
      return;
    }
    if (url.protocol !== 'https:') {
      problems.push(`${at} 必须用 https：${endpoint}`);
    }
    const markers = hasMarker(endpoint, URL_MARKERS);
    if (markers.length > 0) {
      problems.push(`${at} 仍是占位地址（含 ${markers.join('、')}）：${endpoint}`);
      return;
    }
    if (PLACEHOLDER_HOSTS.includes(url.hostname.toLowerCase())) {
      problems.push(`${at} 指向保留主机名，不可能是真实更新地址：${url.hostname}`);
    }
  });
}

function checkIdentity(config) {
  if (typeof config.productName !== 'string' || config.productName.trim() === '') {
    problems.push('productName 为空');
  }
  if (typeof config.version !== 'string' || !/^\d+\.\d+\.\d+/.test(config.version)) {
    problems.push(`version 不是可比较的语义版本：${config.version}`);
  }
  if (typeof config.identifier !== 'string' || !config.identifier.includes('.')) {
    problems.push(`identifier 不是反写域名：${config.identifier}`);
  }
}

let config;
try {
  config = JSON.parse(readFileSync(configPath, 'utf8'));
} catch (error) {
  console.error(`无法读取或解析配置文件：${configPath}`);
  console.error(error.message);
  process.exit(2);
}

checkIdentity(config);

const updater = config?.plugins?.updater;
if (!updater) {
  problems.push('缺少 plugins.updater：安装包将不带自动更新能力');
} else {
  checkPubkey(updater.pubkey);
  checkEndpoints(updater.endpoints);
}

console.log(`发布配置检查：${configPath}`);
if (problems.length > 0) {
  for (const problem of problems) {
    console.error(`未过：${problem}`);
  }
  console.error(
    '\n这些值不换掉，产出的安装包无法验证或接收更新（对应真机清单 V10）。',
  );
  process.exit(1);
}

console.log('通过：自动更新配置已就绪');
console.log(
  '提醒：endpoints 指向的域名能否真正返回更新清单，仍要在 V10 真机步骤里确认。',
);
