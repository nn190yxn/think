// 本地替身模型服务：按 OpenAI 兼容形状回话，用来在没有密钥时把会诊链路整条跑通。
//
// 用法：node tools/stub-model-server.js [端口]
// 作用：验证「编排 → 逐席作答 → 审计落库 → 结论」这条链路，不验证模型输出像不像真人。
// 说话人从 system 提示里的「名字」取，答案可预测，方便核对哪一席发了言。

import http from 'node:http';

const port = Number(process.argv[2] || 8799);

function speakerOf(system) {
  const match = /「([^」]+)」/.exec(system || '');
  return match ? match[1] : '编排方';
}

const server = http.createServer((request, response) => {
  if (request.method !== 'POST') {
    response.writeHead(404, { 'Content-Type': 'application/json' });
    response.end(JSON.stringify({ error: { message: 'only POST is served' } }));
    return;
  }
  let body = '';
  request.on('data', (chunk) => {
    body += chunk;
  });
  request.on('end', () => {
    let payload = {};
    try {
      payload = JSON.parse(body);
    } catch {
      payload = {};
    }
    const messages = Array.isArray(payload.messages) ? payload.messages : [];
    const system = messages
      .filter((message) => message.role === 'system')
      .map((message) => message.content)
      .join('\n');
    const name = speakerOf(system);
    const content =
      `${name}的判断：先看动机与边界，再看时机与代价。` +
      `执行上分三步：先定不可退让的部分，再算可承受的代价，最后留出复盘的时点。` +
      `这条判断只在资料覆盖到的情形成立，超出范围要重新验。`;
    const reply = {
      id: `chatcmpl-stub-${Date.now()}`,
      object: 'chat.completion',
      created: Math.floor(Date.now() / 1000),
      model: payload.model || 'stub-1',
      choices: [
        { index: 0, message: { role: 'assistant', content }, finish_reason: 'stop' },
      ],
      usage: { prompt_tokens: 120, completion_tokens: 80, total_tokens: 200 },
    };
    response.writeHead(200, { 'Content-Type': 'application/json' });
    response.end(JSON.stringify(reply));
  });
});

server.listen(port, '127.0.0.1', () => {
  console.log(`stub model server: http://127.0.0.1:${port}/v1/chat/completions`);
});
