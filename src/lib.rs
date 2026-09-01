/**
 * Tauri 应用主模块
 *
 * 职责: 托盘管理、窗口管理、后台轮询、IPC 通信
 */

mod config;
mod providers;

use std::sync::Mutex;
use std::time::Duration;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{Emitter, Manager, WindowEvent};

struct AppState {
    results: Mutex<Vec<providers::UsageResult>>,
}

// ── IPC 命令 ──────────────────────────────────────────────

#[tauri::command]
fn get_config() -> config::AppConfig {
    config::get()
}

#[tauri::command]
fn update_config(partial: serde_json::Value) -> config::AppConfig {
    let result = config::update(partial);
    result
}

#[tauri::command]
fn add_provider(provider_config: config::ProviderConfig) -> config::ProviderConfig {
    config::add_provider(provider_config)
}

#[tauri::command]
fn update_provider(id: String, updates: serde_json::Value) -> Option<config::ProviderConfig> {
    config::update_provider(&id, updates)
}

#[tauri::command]
fn remove_provider(id: String) {
    config::remove_provider(&id);
}

#[tauri::command]
fn list_providers() -> Vec<providers::ProviderMeta> {
    providers::list_providers()
}

#[tauri::command]
fn get_groups() -> Vec<providers::ProviderGroup> {
    providers::get_groups()
}

#[tauri::command]
fn get_latest(state: tauri::State<AppState>) -> Vec<providers::UsageResult> {
    state.results.lock().unwrap().clone()
}

#[tauri::command]
async fn refresh_usage(app: tauri::AppHandle, state: tauri::State<'_, AppState>) -> Result<Vec<providers::UsageResult>, String> {
    refresh_all_inner(&app, &state).await;
    Ok(state.results.lock().unwrap().clone())
}

#[tauri::command]
async fn query_one(provider_config: config::ProviderConfig) -> providers::UsageResult {
    providers::query_one(&provider_config).await
}

#[tauri::command]
fn hide_window(app: tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
    }
}

#[tauri::command]
fn quit_app(app: tauri::AppHandle) {
    app.exit(0);
}

// ── 内部刷新逻辑 ──────────────────────────────────────────

async fn refresh_all_inner(app: &tauri::AppHandle, state: &AppState) {
    let cfg = config::load();
    let enabled: Vec<_> = cfg.providers.iter()
        .filter(|p| p.enabled)
        .cloned()
        .collect();

    let results = if enabled.is_empty() {
        vec![]
    } else {
        providers::query_all(&enabled).await
    };

    // 更新缓存
    {
        let mut guard = state.results.lock().unwrap();
        *guard = results.clone();
    }

    // 更新托盘 tooltip
    let tooltip = build_tray_tooltip(&results, &cfg.providers);
    if let Some(tray) = app.tray_by_id("main-tray") {
        let _ = tray.set_tooltip(Some(&tooltip));
    }

    // 发送到前端
    let payload = serde_json::json!({
        "results": results,
        "config": cfg.providers,
    });
    let _ = app.emit("usage-update", payload);
}

/// 构建托盘 tooltip 文本 (鼠标悬停时显示)
fn build_tray_tooltip(results: &[providers::UsageResult], providers_cfg: &[config::ProviderConfig]) -> String {
    if results.is_empty() {
        return "AI 用量监控\n未配置任何厂商".into();
    }

    let mut lines = vec!["AI 用量监控".to_string()];

    for r in results {
        let prov_cfg = providers_cfg.iter().find(|p| p.id == r.config_id);
        let name = prov_cfg.map(|p| p.name.as_str()).unwrap_or(&r.provider_id);

        if !r.success {
            lines.push(format!("  {}：{}", name, r.error.as_deref().unwrap_or("查询失败")));
            continue;
        }

        for tier in &r.tiers {
            let reset_info = format_reset_time_rust(tier.resets_at.as_deref());
            lines.push(format!("  {} - {}：{:.0}%{}", name, tier.label, tier.used_percentage, reset_info));
        }
        if let Some(bal) = &r.balance {
            lines.push(format!("  {} - 余额：¥{:.2}", name, bal.available));
        }
        if r.tiers.is_empty() && r.balance.is_none() {
            lines.push(format!("  {}：无数据", name));
        }
    }

    lines.join("\n")
}

/// 格式化重置时间: 倒计时 + 具体日期
fn format_reset_time_rust(resets_at: Option<&str>) -> String {
    let Some(time_str) = resets_at else { return String::new(); };
    let Ok(date) = chrono::DateTime::parse_from_rfc3339(time_str) else { return String::new(); };
    let now = chrono::Utc::now();
    let diff = date.signed_duration_since(now);
    let secs = diff.num_seconds();
    if secs <= 0 {
        return " - 即将重置".into();
    }

    let days = secs / 86400;
    let hours = (secs % 86400) / 3600;
    let mins = (secs % 3600) / 60;
    let s = secs % 60;

    let countdown = if days > 0 {
        format!("{}天{}小时{}分", days, hours, mins)
    } else if hours > 0 {
        format!("{}小时{}分{}秒", hours, mins, s)
    } else if mins > 0 {
        format!("{}分{}秒", mins, s)
    } else {
        format!("{}秒", s)
    };

    let local = date.with_timezone(&chrono::Local);
    let date_str = local.format("%-m月%-d日 %H:%M").to_string();

    format!(" - {}后重置 ({})", countdown, date_str)
}

// ── 应用启动 ──────────────────────────────────────────────

/// 显示并置顶主窗口
fn show_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        // Windows 前台锁定策略下 set_focus 可能失效，先临时置顶强制窗口浮到最上层
        let _ = window.set_always_on_top(true);
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        let _ = window.set_always_on_top(false);
    }
}

pub fn run() {
    let mut builder = tauri::Builder::default();

    builder = builder
        .plugin(tauri_plugin_shell::init())
        .manage(AppState {
            results: Mutex::new(vec![]),
        })
        .setup(|app| {
            // 创建托盘菜单
            let menu = Menu::with_items(
                app,
                &[
                    &MenuItem::with_id(app, "show", "📊 显示主界面", true, None::<&str>)?,
                    &MenuItem::with_id(app, "refresh", "🔄 立即刷新", true, None::<&str>)?,
                    &MenuItem::with_id(app, "settings", "⚙ 设置", true, None::<&str>)?,
                    &PredefinedMenuItem::separator(app)?,
                    &MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?,
                ],
            )?;

            // 创建托盘图标
            let _tray = TrayIconBuilder::with_id("main-tray")
                .icon(app.default_window_icon().unwrap().clone())
                .tooltip("AI 用量监控")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        show_main_window(tray.app_handle());
                    }
                })
                .build(app)?;

            // 显示窗口 (除非 --hidden 参数)
            let args: Vec<String> = std::env::args().collect();
            let start_hidden = args.iter().any(|a| a == "--hidden");
            if let Some(window) = app.get_webview_window("main") {
                // 运行时强制设置中文标题，防止 JSON 编码问题
                let _ = window.set_title("AI \u{7528}\u{91CF}\u{76D1}\u{63A7}");
                if !start_hidden {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }

            // 启动后台轮询
            let app_handle: tauri::AppHandle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                // 首次延迟 1 秒执行
                tokio::time::sleep(Duration::from_secs(1)).await;

                loop {
                    let cfg = config::load();
                    if cfg.auto_refresh {
                        let state = app_handle.state::<AppState>();
                        refresh_all_inner(&app_handle, state.inner()).await;
                    }
                    let interval = cfg.refresh_interval.max(10);
                    tokio::time::sleep(Duration::from_secs(interval)).await;
                }
            });

            Ok(())
        })
        .on_window_event(|window, event| {
            // 关闭窗口 = 发送事件给前端，由前端决定隐藏或退出
            if let WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    api.prevent_close();
                    let _ = window.emit("close-requested", ());
                }
            }
        });

    // 菜单事件处理
    let builder = builder.on_menu_event(move |app, event| {
        match event.id().as_ref() {
            "show" => {
                show_main_window(app);
            }
            "refresh" => {
                let app_handle: tauri::AppHandle = app.clone();
                tauri::async_runtime::spawn(async move {
                    let state = app_handle.state::<AppState>();
                    refresh_all_inner(&app_handle, state.inner()).await;
                });
            }
            "settings" => {
                show_main_window(app);
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.emit("navigate", "settings");
                }
            }
            "quit" => {
                app.exit(0);
            }
            _ => {}
        }
    });

    builder
        .invoke_handler(tauri::generate_handler![
            get_config,
            update_config,
            add_provider,
            update_provider,
            remove_provider,
            list_providers,
            get_groups,
            get_latest,
            refresh_usage,
            query_one,
            hide_window,
            quit_app,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
