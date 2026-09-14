/**
 * DeepSeek Harness 桌面端状态页：
 * 按 Rust 侧推送的 dsh-state 事件切换界面区块，按钮答复通过 invoke 回传。
 */
(function () {
  var tauri = window.__TAURI__;
  var VIEW_IDS = [
    'view-checking',
    'view-need-install',
    'view-need-node',
    'view-installing',
    'view-pick',
    'view-reuse-failed',
    'view-error'
  ];

  // 只显示指定区块，其余隐藏
  function show(viewId) {
    VIEW_IDS.forEach(function (id) {
      document.getElementById(id).classList.toggle('hidden', id !== viewId);
    });
  }

  // 安装日志追加一行并滚到底部
  function appendLog(line) {
    if (!line) {
      return;
    }
    var log = document.getElementById('install-log');
    log.textContent += line + '\n';
    log.scrollTop = log.scrollHeight;
  }

  // 渲染多实例列表，每项一个「接入」按钮
  function renderInstances(instances) {
    var list = document.getElementById('instance-list');
    list.textContent = '';
    instances.forEach(function (item) {
      var row = document.createElement('div');
      row.className = 'instance-item';

      var label = document.createElement('span');
      var addr = document.createElement('span');
      addr.className = 'addr';
      addr.textContent = '127.0.0.1:' + item.port;
      var pid = document.createElement('span');
      pid.className = 'pid';
      pid.textContent = '进程号 ' + item.pid;
      label.appendChild(addr);
      label.appendChild(pid);

      var button = document.createElement('button');
      button.className = 'primary';
      button.textContent = '接入';
      button.addEventListener('click', function () {
        tauri.core.invoke('pick_instance', { port: item.port });
        show('view-checking');
      });

      row.appendChild(label);
      row.appendChild(button);
      list.appendChild(row);
    });
  }

  // 关闭主窗口
  function closeWindow() {
    tauri.window.getCurrentWindow().close();
  }

  // 订阅 Rust 侧状态事件，驱动界面切换
  tauri.event.listen('dsh-state', function (event) {
    var payload = event.payload || {};

    if (payload.state === 'checking') {
      show('view-checking');
    } else if (payload.state === 'need_install') {
      show(payload.has_npm ? 'view-need-install' : 'view-need-node');
    } else if (payload.state === 'installing') {
      show('view-installing');
      appendLog(payload.line);
    } else if (payload.state === 'pick_instance') {
      renderInstances(payload.instances || []);
      show('view-pick');
    } else if (payload.state === 'reuse_failed') {
      document.getElementById('reuse-failed-port').textContent = String(payload.port || '');
      document.getElementById('reuse-failed-reason').textContent = payload.message ? '（' + payload.message + '）' : '';
      show('view-reuse-failed');
    } else if (payload.state === 'error') {
      document.getElementById('error-detail').textContent = payload.message || '未知错误';
      show('view-error');
    }
  });

  // 安装确认
  document.getElementById('btn-install').addEventListener('click', function () {
    tauri.core.invoke('install_decision', { confirmed: true });
    show('view-installing');
  });
  document.getElementById('btn-cancel-install').addEventListener('click', function () {
    tauri.core.invoke('install_decision', { confirmed: false });
    closeWindow();
  });

  // 无 Node 环境：退出按钮（官网链接是普通 a 标签，target=_blank 交给系统浏览器）
  document.getElementById('btn-quit-node').addEventListener('click', closeWindow);

  // 失败页退出
  document.getElementById('btn-quit-reuse').addEventListener('click', closeWindow);
  document.getElementById('btn-quit-error').addEventListener('click', closeWindow);

  show('view-checking');
})();
