/**
 * AI 用量监控 - 前端应用逻辑 (Tauri 版)
 */

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

let providersList = [];
let groupsList = [];
let currentConfig = null;

// ── 初始化 ──────────────────────────────────────────

async function init() {
  currentConfig = await invoke('get_config');
  providersList = await invoke('list_providers');
  groupsList = await invoke('get_groups');

  renderUsagePlaceHolder();
  renderProviderList();
  renderProviderSelect();
  renderSettings();
  bindEvents();

  // 监听事件
  await listen('usage-update', (event) => {
    renderUsage(event.payload);
  });

  await listen('navigate', (event) => {
    switchTab(event.payload);
  });

  // 加载已有数据
  const latest = await invoke('get_latest');
  if (latest && latest.length > 0) {
    renderUsage({ results: latest, config: currentConfig.providers });
  }

  // 触发首次刷新
  invoke('refresh_usage');
}

// ── 标签切换 ──────────────────────────────────────────

function switchTab(tabName) {
  document.querySelectorAll('.tab').forEach((t) => t.classList.remove('active'));
  document.querySelectorAll('.tab-content').forEach((c) => c.classList.remove('active'));
  const tab = document.querySelector(`.tab[data-tab="${tabName}"]`);
  const content = document.getElementById(`tab-${tabName}`);
  if (tab) tab.classList.add('active');
  if (content) content.classList.add('active');
}

function bindEvents() {
  // 标签切换
  document.querySelectorAll('.tab').forEach((tab) => {
    tab.addEventListener('click', () => switchTab(tab.dataset.tab));
  });

  // 刷新按钮
  document.getElementById('btn-refresh').addEventListener('click', async () => {
    const btn = document.getElementById('btn-refresh');
    btn.style.animation = 'spin 0.6s linear';
    setTimeout(() => { btn.style.animation = ''; }, 600);
    await invoke('refresh_usage');
  });

  // 最小化按钮
  document.getElementById('btn-minimize').addEventListener('click', () => {
    invoke('hide_window');
  });

  // 添加厂商
  document.getElementById('provider-select').addEventListener('change', renderDynamicFields);
  document.getElementById('btn-add').addEventListener('click', handleAdd);
  document.getElementById('btn-test').addEventListener('click', handleTest);

  // 设置
  document.getElementById('auto-refresh-toggle').addEventListener('change', handleAutoRefreshChange);
  document.getElementById('refresh-interval').addEventListener('change', handleIntervalChange);
}

// ── 用量渲染 ──────────────────────────────────────────

function renderUsagePlaceHolder() {
  const el = document.getElementById('usage-content');
  el.innerHTML = `
    <div class="empty-state">
      <div class="icon">⏳</div>
      <div>正在查询用量...</div>
    </div>
  `;
}

function formatResetTime(resetsAt) {
  if (!resetsAt) return '';
  const date = new Date(resetsAt);
  const now = new Date();
  const diffMs = date - now;
  if (diffMs <= 0) return '即将重置';

  // 格式化具体重置时间: X月X日 HH:MM
  const mo = date.getMonth() + 1;
  const dd = date.getDate();
  const hh = String(date.getHours()).padStart(2, '0');
  const mm = String(date.getMinutes()).padStart(2, '0');
  const dateStr = `${mo}月${dd}日 ${hh}:${mm}`;

  // 计算倒计时
  const totalSec = Math.floor(diffMs / 1000);
  const days = Math.floor(totalSec / 86400);
  const hours = Math.floor((totalSec % 86400) / 3600);
  const mins = Math.floor((totalSec % 3600) / 60);
  const secs = totalSec % 60;

  let countdown;
  if (days > 0) {
    countdown = `${days}天${hours}小时${mins}分`;
  } else if (hours > 0) {
    countdown = `${hours}小时${mins}分${secs}秒`;
  } else if (mins > 0) {
    countdown = `${mins}分${secs}秒`;
  } else {
    countdown = `${secs}秒`;
  }

  return `${countdown}后重置 (${dateStr})`;
}

function getUsageLevel(pct) {
  if (pct >= 80) return 'danger';
  if (pct >= 50) return 'warning';
  return '';
}

function renderUsage(data) {
  const contentEl = document.getElementById('usage-content');
  const footerEl = document.getElementById('footer');
  const { results, config } = data;

  if (!results || results.length === 0) {
    contentEl.innerHTML = `
      <div class="empty-state">
        <div class="icon">📋</div>
        <div>尚未配置任何厂商</div>
        <div class="hint">点击「添加厂商」标签配置 API Key</div>
      </div>
    `;
    footerEl.textContent = '';
    return;
  }

  let html = '';
  for (const result of results) {
    const provConfig = config.find((p) => p.id === result.configId);
    const name = provConfig ? provConfig.name : result.providerId;

    if (!result.success) {
      html += `
        <div class="provider-card error">
          <div class="provider-name">
            <span class="label">${escapeHtml(name)}</span>
            <span class="badge red">失败</span>
          </div>
          <div class="error-msg">${escapeHtml(result.error || '查询失败')}</div>
        </div>
      `;
      continue;
    }

    let tierHtml = '';
    for (const tier of (result.tiers || [])) {
      const pct = tier.usedPercentage;
      const level = getUsageLevel(pct);
      let color = 'var(--green)';
      if (level === 'danger') color = 'var(--red)';
      else if (level === 'warning') color = 'var(--orange)';

      tierHtml += `
        <div class="tier-row">
          <div class="tier-header">
            <span class="tier-label">${escapeHtml(tier.label)}</span>
            <span class="tier-value" style="color: ${color}">${pct.toFixed(1)}%</span>
          </div>
          <div class="progress-bar">
            <div class="fill${level ? ' ' + level : ''}" style="width: ${Math.min(100, pct)}%"></div>
          </div>
      `;
      if (tier.resetsAt) {
        tierHtml += `<div class="tier-reset">⏱ ${formatResetTime(tier.resetsAt)}</div>`;
      }
      if (tier.used != null && tier.limit != null) {
        const usedK = tier.used >= 1000 ? (tier.used / 1000).toFixed(1) + 'k' : tier.used.toFixed(0);
        const limitK = tier.limit >= 1000 ? (tier.limit / 1000).toFixed(1) + 'k' : tier.limit.toFixed(0);
        tierHtml += `<div class="tier-reset">${usedK} / ${limitK} ${tier.unit || ''}</div>`;
      }
      tierHtml += `</div>`;
    }

    let balanceHtml = '';
    if (result.balance) {
      const cur = result.balance.currency === 'CNY' ? '¥' : '$';
      let balColor = '';
      if (result.balance.available < 10) balColor = 'color: var(--red)';
      else if (result.balance.available < 50) balColor = 'color: var(--orange)';

      balanceHtml += `
        <div class="balance-row">
          <span class="tier-label">💰 账户余额</span>
          <span class="amount" style="${balColor}">${cur}${result.balance.available.toFixed(2)}</span>
        </div>
      `;
      if (result.balance.voucher > 0 || result.balance.cash > 0) {
        balanceHtml += `<div class="tier-reset">代金券: ${cur}${result.balance.voucher.toFixed(2)} | 现金: ${cur}${result.balance.cash.toFixed(2)}</div>`;
      }
    }

    if (!tierHtml && !balanceHtml) {
      tierHtml = '<div class="tier-reset">暂无用量数据</div>';
    }

    let label = name;
    if (result.level) label += ` · ${result.level}`;

    html += `
      <div class="provider-card">
        <div class="provider-name">
          <span class="label">${escapeHtml(label)}</span>
        </div>
        ${tierHtml}
        ${balanceHtml}
      </div>
    `;
  }

  contentEl.innerHTML = html;

  // 更新底部时间
  const latestTime = results.reduce((max, r) => Math.max(max, r.queriedAt || 0), 0);
  if (latestTime) {
    const time = new Date(latestTime);
    footerEl.textContent = `更新于 ${time.toLocaleTimeString('zh-CN')}`;
  }
}

// ── 厂商列表 ──────────────────────────────────────────

async function refreshProviderList() {
  currentConfig = await invoke('get_config');
  renderProviderList();
}

function renderProviderList() {
  const container = document.getElementById('provider-list');
  if (!currentConfig.providers || currentConfig.providers.length === 0) {
    container.innerHTML = `
      <div class="empty-config">
        尚未添加任何厂商<br>
        请切换到「添加厂商」标签配置
      </div>
    `;
    return;
  }

  let html = '';
  for (const prov of currentConfig.providers) {
    const meta = providersList.find((p) => p.id === prov.providerId);
    const typeName = meta ? meta.name : prov.providerId;
    const checked = prov.enabled !== false ? 'checked' : '';

    html += `
      <div class="provider-item">
        <div class="info">
          <div class="name">${escapeHtml(prov.name)}</div>
          <div class="type">${escapeHtml(typeName)}${prov.enabled === false ? ' · 已禁用' : ''}</div>
        </div>
        <div class="actions">
          <label class="switch">
            <input type="checkbox" ${checked} data-id="${prov.id}" class="toggle-enable">
            <span class="slider"></span>
          </label>
          <button class="btn btn-sm btn-danger btn-remove" data-id="${prov.id}">删除</button>
        </div>
      </div>
    `;
  }
  container.innerHTML = html;

  // 绑定事件
  container.querySelectorAll('.toggle-enable').forEach((el) => {
    el.addEventListener('change', async (e) => {
      const id = e.target.dataset.id;
      await invoke('update_provider', { id, updates: { enabled: e.target.checked } });
      currentConfig = await invoke('get_config');
    });
  });

  container.querySelectorAll('.btn-remove').forEach((el) => {
    el.addEventListener('click', async () => {
      const id = el.dataset.id;
      await invoke('remove_provider', { id });
      await refreshProviderList();
      invoke('refresh_usage');
    });
  });
}

// ── 添加厂商 ──────────────────────────────────────────

function renderProviderSelect() {
  const select = document.getElementById('provider-select');
  let html = '';
  for (const group of groupsList) {
    html += `<optgroup label="${escapeHtml(group.label)}">`;
    for (const prov of group.providers) {
      html += `<option value="${prov.id}">${escapeHtml(prov.name)}</option>`;
    }
    html += `</optgroup>`;
  }
  select.innerHTML = html;
  renderDynamicFields();
}

function renderDynamicFields() {
  const select = document.getElementById('provider-select');
  const providerId = select.value;
  const provider = providersList.find((p) => p.id === providerId);
  if (!provider) return;

  const container = document.getElementById('dynamic-fields');
  let html = '';
  for (const field of provider.fields) {
    const reqMark = field.required ? ' *' : '';
    html += `<div class="form-group"><label>${escapeHtml(field.label)}${reqMark}</label>`;

    if (field.type === 'select') {
      html += `<select id="field-${field.key}" data-field-key="${field.key}">`;
      for (const opt of (field.options || [])) {
        const sel = field.default === opt.value ? 'selected' : '';
        html += `<option value="${opt.value}" ${sel}>${escapeHtml(opt.label)}</option>`;
      }
      html += `</select>`;
    } else {
      const val = field.default ? `value="${escapeHtml(field.default)}"` : '';
      const ph = field.placeholder ? `placeholder="${escapeHtml(field.placeholder)}"` : '';
      html += `<input type="${field.type || 'text'}" id="field-${field.key}" data-field-key="${field.key}" ${val} ${ph}>`;
    }

    if (field.description) {
      html += `<div class="hint">${escapeHtml(field.description)}</div>`;
    }
    html += `</div>`;
  }
  container.innerHTML = html;
}

function collectCreds() {
  const providerId = document.getElementById('provider-select').value;
  const provider = providersList.find((p) => p.id === providerId);
  if (!provider) return null;

  const creds = {};
  for (const field of provider.fields) {
    const el = document.getElementById(`field-${field.key}`);
    if (el) {
      creds[field.key] = el.value.trim();
    }
  }
  return { providerId, creds };
}

async function handleAdd() {
  const collected = collectCreds();
  if (!collected) return;

  const provider = providersList.find((p) => p.id === collected.providerId);
  for (const field of provider.fields) {
    if (field.required && !collected.creds[field.key]) {
      showTestResult('error', `请填写 ${field.label}`);
      return;
    }
  }

  const displayName = document.getElementById('display-name').value.trim();
  const name = displayName || provider.name;

  await invoke('add_provider', {
    providerConfig: {
      id: '',
      name,
      providerId: collected.providerId,
      creds: collected.creds,
      enabled: true,
    }
  });

  await refreshProviderList();
  switchTab('providers');
  showTestResult('success', `${name} 已添加`);
  document.getElementById('display-name').value = '';
  invoke('refresh_usage');
}

async function handleTest() {
  const collected = collectCreds();
  if (!collected) return;

  const provider = providersList.find((p) => p.id === collected.providerId);
  for (const field of provider.fields) {
    if (field.required && !collected.creds[field.key]) {
      showTestResult('error', `请填写 ${field.label}`);
      return;
    }
  }

  const btn = document.getElementById('btn-test');
  btn.textContent = '查询中...';
  btn.disabled = true;

  try {
    const result = await invoke('query_one', {
      providerConfig: {
        id: 'test',
        name: 'test',
        providerId: collected.providerId,
        creds: collected.creds,
        enabled: true,
      }
    });

    if (result.success) {
      const parts = [];
      for (const tier of (result.tiers || [])) {
        parts.push(`${tier.label}: ${tier.usedPercentage.toFixed(1)}%`);
      }
      if (result.balance) {
        const cur = result.balance.currency === 'CNY' ? '¥' : '$';
        parts.push(`余额: ${cur}${result.balance.available.toFixed(2)}`);
      }
      if (result.level) parts.push(`套餐: ${result.level}`);
      showTestResult('success', `查询成功！${parts.join(' | ')}`);
    } else {
      showTestResult('error', `查询失败: ${result.error || '未知错误'}`);
    }
  } catch (err) {
    showTestResult('error', `异常: ${err}`);
  } finally {
    btn.textContent = '测试查询';
    btn.disabled = false;
  }
}

function showTestResult(type, message) {
  const el = document.getElementById('test-result');
  el.className = `test-result ${type}`;
  el.textContent = message;
  if (type === 'success') {
    setTimeout(() => { el.className = 'test-result'; }, 5000);
  }
}

// ── 通用设置 ──────────────────────────────────────────

function renderSettings() {
  document.getElementById('auto-refresh-toggle').checked = currentConfig.autoRefresh !== false;
  document.getElementById('refresh-interval').value = currentConfig.refreshInterval || 60;
}

async function handleAutoRefreshChange(e) {
  await invoke('update_config', { partial: { autoRefresh: e.target.checked } });
  currentConfig = await invoke('get_config');
}

async function handleIntervalChange(e) {
  const val = Math.max(10, parseInt(e.target.value) || 60);
  await invoke('update_config', { partial: { refreshInterval: val } });
  currentConfig = await invoke('get_config');
}

// ── 工具函数 ──────────────────────────────────────────

function escapeHtml(str) {
  const div = document.createElement('div');
  div.textContent = String(str || '');
  return div.innerHTML;
}

// 启动
init();
